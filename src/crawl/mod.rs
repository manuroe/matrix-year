/// Matrix event crawling module.
///
/// Crawls user-sent messages from Matrix homeserver and stores them in the SDK database.
///
/// # Architecture
///
/// The module is organized into focused submodules:
/// - **types**: Data structures for room metadata and statistics
/// - **decision**: Core logic for determining which rooms to crawl
/// - **discovery**: Room list sync via sliding sync
/// - **pagination**: Event backward pagination and aggregation
/// - **progress**: Progress reporting and UI
use anyhow::{Context, Result};
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::Path;

use crate::account_selector::AccountSelector;
use crate::crawl_db;
use crate::stats;
use crate::window::WindowScope;

pub mod types;
pub use types::RoomCrawlStats;
use types::RoomJoinState;

mod decision;
use decision::{record_skipped_virgin_rooms, select_rooms_to_crawl};

mod discovery;
use discovery::{fetch_room_list_via_sliding_sync, setup_account};

mod pagination;

pub mod progress;
use progress::CrawlProgress;

/// Maximum number of rooms to crawl concurrently.
/// Balances throughput against server load.
const MAX_CONCURRENCY: usize = 8;

/// Main entry point for the crawl command.
///
/// Discovers all logged-in accounts and crawls them for the requested time window.
/// Optionally filters to a specific account if `user_id_flag` is provided.
///
/// Returns a vector of (account_id, Stats) tuples for each crawled account.
///
/// # Arguments
///
/// * `window` - Time window specification (e.g., "2025", "2025-03", "life")
/// * `user_id_flag` - Optional Matrix user ID to restrict crawling to one account
pub async fn run(
    window: String,
    user_id_flag: Option<String>,
) -> Result<Vec<(String, stats::Stats)>> {
    // Parse the window
    let window_scope = WindowScope::parse(&window).context("Failed to parse window")?;

    eprintln!(
        "📥 Crawling {} for window: {}",
        if user_id_flag.is_some() {
            "account"
        } else {
            "all accounts"
        },
        window,
    );

    // Select accounts (with multi-select enabled)
    let mut selector = AccountSelector::new()?;
    let accounts = selector.select_accounts(user_id_flag, true)?;

    eprintln!("🔍 Crawling {} account(s)", accounts.len());

    // Crawl each account and collect stats
    let mut account_stats = Vec::new();
    for (account_id, account_dir) in &accounts {
        match crawl_account(account_id, account_dir, &window_scope).await {
            Ok(stats) => {
                account_stats.push((account_id.clone(), stats));
            }
            Err(e) => {
                eprintln!("❌ Error crawling {}: {}", account_id, e);
                // Continue with other accounts on error
            }
        }
    }

    eprintln!("✅ Crawl complete");
    Ok(account_stats)
}

/// Crawls a single account for the given time window.
///
/// Coordinates the full crawl workflow:
/// 1. Sets up the account (client + database)
/// 2. Discovers joined rooms via sliding sync
/// 3. Decides which rooms need pagination
/// 4. Records virgin rooms that were skipped
/// 5. Crawls rooms in parallel with progress reporting
/// 6. Aggregates room statistics into account-level Stats
///
/// Returns the computed Stats for the account.
async fn crawl_account(
    account_id: &str,
    account_dir: &Path,
    window_scope: &WindowScope,
) -> Result<stats::Stats> {
    eprintln!("📱 Crawling account: {}", account_id);

    // 1) Account setup
    let (_account_dir_path, client, db) = setup_account(account_id, account_dir)
        .await
        .context("Account setup failed")?;

    // 2) Discover rooms via sliding sync
    let room_list = fetch_room_list_via_sliding_sync(&client).await?;

    // 3) Check which rooms need crawl
    let joined_room_ids: Vec<_> = room_list
        .iter()
        .filter(|r| matches!(r.join_state, RoomJoinState::Joined))
        .map(|r| r.room_id.clone())
        .collect();

    if joined_room_ids.is_empty() {
        eprintln!("ℹ️  No rooms to crawl");
    }

    let (window_start_ts, window_end_ts) = window_scope.to_timestamp_range();

    // Latest known events from room list for freshness checks
    let latest_events: HashMap<_, _> = room_list
        .iter()
        .filter_map(|r| {
            Some((
                r.room_id.clone(),
                (r.last_event_id.as_ref()?.clone(), r.last_event_ts?),
            ))
        })
        .collect();

    let joined_rooms = client.joined_rooms();

    let rooms_to_crawl = select_rooms_to_crawl(
        &joined_rooms,
        &db,
        window_start_ts,
        Some(window_end_ts),
        &latest_events,
    );

    // Record virgin rooms that are outside the window so we don't re-check them
    record_skipped_virgin_rooms(&db, &joined_rooms, &rooms_to_crawl, &latest_events)
        .context("Failed to record skipped virgin rooms")?;

    eprintln!(
        "📚 Found {} joined room(s), {} to crawl...",
        joined_rooms.len(),
        rooms_to_crawl.len()
    );

    // 4) Crawl rooms (parallel pagination, sequential DB updates)
    let total_rooms = rooms_to_crawl.len();
    let (success_count, error_count, room_stats_inputs) =
        crawl_rooms_parallel(rooms_to_crawl, window_scope, &db, account_id, total_rooms).await;

    eprintln!(
        "✅ Crawled {} rooms ({} errors)",
        success_count, error_count
    );

    // 5) Build account-level stats from room statistics
    // Note: Account profile fetch is not available in current SDK; passing None for now
    let stats = crate::stats_builder::build_stats(
        room_stats_inputs,
        account_id,
        None,
        None,
        window_scope,
        joined_rooms.len(),
    )
    .context("Failed to build account stats")?;

    Ok(stats)
}

/// Crawls a set of rooms in parallel, respecting concurrency limits.
///
/// Uses async streams to manage concurrent pagination operations.
/// Updates the database after each room completes.
///
/// Returns tuple of (success_count, error_count, room_stats_inputs).
async fn crawl_rooms_parallel(
    rooms: Vec<matrix_sdk::Room>,
    window_scope: &WindowScope,
    db: &crawl_db::CrawlDb,
    account_id: &str,
    total_rooms: usize,
) -> (usize, usize, Vec<crate::stats_builder::RoomStatsInput>) {
    let mut success_count = 0usize;
    let mut error_count = 0usize;
    let mut room_stats_inputs = Vec::new();

    let (window_start_ts, window_end_ts) = window_scope.to_timestamp_range();
    let user_id = account_id.to_string();

    let progress = CrawlProgress::new(total_rooms);
    let progress_for_stream = progress.clone();

    let mut stream = futures_util::stream::iter(rooms)
        .map(move |room| {
            let uid = user_id.clone();
            let progress_for_room = progress_for_stream.clone();
            crawl_single_room(
                room,
                window_start_ts,
                window_end_ts,
                uid,
                progress_for_room,
                db,
            )
        })
        .buffer_unordered(MAX_CONCURRENCY);

    while let Some((room, stats_res, room_type, detailed_stats, spinner)) = stream.next().await {
        // Finish spinner before printing results
        if let Some(ref sp) = spinner {
            sp.finish_and_clear();
        }

        let room_id = room.room_id().to_string();

        match stats_res {
            Ok(stats) => {
                let room_name = stats.room_name.clone();

                if let Err(e) = db.update_room_metadata(
                    &stats.room_id,
                    stats.oldest_event_id,
                    stats.oldest_ts,
                    stats.newest_event_id,
                    stats.newest_ts,
                    stats.fully_crawled,
                ) {
                    error_count += 1;
                    // Mark as error
                    let _ =
                        db.set_crawl_status(&room_id, crawl_db::CrawlStatus::Error(e.to_string()));
                    progress.println(&format!("  \x1b[31m✗\x1b[0m {} ({})", room_name, e));
                } else {
                    success_count += 1;
                    // Mark as success and update event counts
                    let _ = db.set_crawl_status(&room_id, crawl_db::CrawlStatus::Success);
                    let _ =
                        db.update_max_event_counts(&room_id, stats.total_events, stats.user_events);

                    use crate::crawl::progress::format_completed_room;
                    let formatted = format_completed_room(
                        &room_name,
                        stats.total_events,
                        stats.user_events,
                        stats.oldest_ts,
                        stats.newest_ts,
                        stats.fully_crawled,
                    );
                    progress.println(&format!("  ✓ {}", formatted));

                    // Collect room stats input for aggregation
                    if let (Some(room_type), Some(detailed)) = (room_type, detailed_stats) {
                        room_stats_inputs.push(crate::stats_builder::RoomStatsInput {
                            room_id: stats.room_id,
                            room_name: Some(stats.room_name),
                            room_type,
                            stats: detailed,
                        });
                    }
                }
            }
            Err(e) => {
                error_count += 1;

                // Mark as error
                let _ = db.set_crawl_status(&room_id, crawl_db::CrawlStatus::Error(e.to_string()));

                // Fetch room name for error reporting
                let room_name = room
                    .display_name()
                    .await
                    .ok()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| room.room_id().to_string());
                progress.println(&format!("  \x1b[31m✗\x1b[0m {} ({})", room_name, e));
            }
        }

        progress.inc();
    }

    progress.finish();

    (success_count, error_count, room_stats_inputs)
}

/// Crawls events from a single room.
///
/// Sets up pagination and delegates to the pagination module.
/// Collects detailed statistics for stats aggregation.
/// Returns the room, result, room type, detailed stats, and optional spinner handle.
async fn crawl_single_room(
    room: matrix_sdk::Room,
    window_start_ts: Option<i64>,
    window_end_ts: i64,
    user_id: String,
    progress: CrawlProgress,
    db: &crawl_db::CrawlDb,
) -> (
    matrix_sdk::Room,
    Result<RoomCrawlStats>,
    Option<RoomType>,
    Option<types::DetailedPaginationStats>,
    Option<indicatif::ProgressBar>,
) {
    // Fetch the room's display name before creating the progress callback
    let room_name = room
        .display_name()
        .await
        .ok()
        .map(|n| n.to_string())
        .unwrap_or_else(|| room.room_id().to_string());

    let (progress_callback, spinner) = progress.make_callback(room_name.clone());

    // Mark room as in-progress
    let room_id = room.room_id().to_string();
    if let Err(e) = db.set_crawl_status(&room_id, crawl_db::CrawlStatus::InProgress) {
        eprintln!(
            "Warning: Failed to mark room {} as InProgress: {}",
            room_id, e
        );
    }

    // Setup event cache and collect detailed stats (single pagination)
    // Note: Keep drop_handles alive throughout pagination to maintain cache subscription
    let room_event_cache_res = pagination::setup_event_cache(&room).await;

    let (stats_res, detailed_stats, room_type) =
        if let Ok((room_event_cache, _drop_handles)) = room_event_cache_res {
            // Call the unified pagination function that collects both basic and detailed stats
            match pagination::paginate_and_collect_detailed_stats(
                &room,
                &room_event_cache,
                window_start_ts,
                window_end_ts,
                &user_id,
                &room_name,
                None, // No initial newest event - start from current
                None, // No initial newest ts
                &*progress_callback,
            )
            .await
            {
                Ok((crawl_stats, detailed)) => {
                    let room_type = classify_room_type(&room).await.ok();
                    (Ok(crawl_stats), Some(detailed), room_type)
                }
                Err(e) => (Err(e), None, None),
            }
        } else {
            (Err(room_event_cache_res.unwrap_err()), None, None)
        };

    (room, stats_res, room_type, detailed_stats, spinner)
}

/// Room classification (DM, public, private).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomType {
    Dm,
    Public,
    Private,
}

/// Classifies a room as DM, public, or private.
///
/// Uses the Matrix SDK's direct-message flag and join rules to determine room type:
/// - DM: room is marked as a direct message (`is_direct() == true`)
/// - Public: join_rules = public
/// - Private: everything else (non-public rooms that are not marked as DMs)
async fn classify_room_type(room: &matrix_sdk::Room) -> Result<RoomType> {
    use matrix_sdk::ruma::events::room::join_rules::JoinRule;

    // Check if room is explicitly marked as a direct message
    if room.is_direct().await? {
        return Ok(RoomType::Dm);
    }

    // Get join rules
    let join_rule = room.join_rule();
    match join_rule {
        Some(JoinRule::Public) => Ok(RoomType::Public),
        _ => Ok(RoomType::Private),
    }
}
