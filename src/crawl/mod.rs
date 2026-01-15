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
use std::fs;
use std::path::Path;

use crate::crawl_db;
use crate::login::resolve_data_root;
use crate::window::WindowScope;

mod types;
pub use types::RoomCrawlStats;
use types::RoomJoinState;

mod decision;
use decision::{record_skipped_virgin_rooms, select_rooms_to_crawl};

mod discovery;
use discovery::{fetch_room_list_via_sliding_sync, setup_account};

mod pagination;
use pagination::crawl_room_events;

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
/// # Arguments
///
/// * `window` - Time window specification (e.g., "2025", "2025-03", "life")
/// * `user_id_flag` - Optional Matrix user ID to restrict crawling to one account
pub async fn run(window: String, user_id_flag: Option<String>) -> Result<()> {
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

    // Discover accounts
    let data_root = resolve_data_root()?;
    let accounts_root = data_root.join("accounts");

    if !accounts_root.exists() {
        eprintln!("⚠️  No accounts found. Please run 'my login' first.");
        return Ok(());
    }

    let mut target_accounts = Vec::new();

    if let Some(uid) = user_id_flag {
        // Single account specified
        target_accounts.push(uid);
    } else {
        // Discover all accounts
        for entry in fs::read_dir(&accounts_root).context("Failed to read accounts directory")? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let dir_name = entry.file_name();
                if let Some(account_id) = dir_name.to_str() {
                    // Convert dir name back to Matrix user ID (reverse of account_id_to_dirname)
                    let user_id = account_id.replace('_', ":");
                    target_accounts.push(user_id);
                }
            }
        }
    }

    if target_accounts.is_empty() {
        eprintln!("⚠️  No accounts found.");
        return Ok(());
    }

    eprintln!("🔍 Found {} account(s)", target_accounts.len());

    // Crawl each account
    for account_id in target_accounts {
        crawl_account(&account_id, &accounts_root, &window_scope)
            .await
            .unwrap_or_else(|e| {
                eprintln!("❌ Error crawling {}: {}", account_id, e);
            });
    }

    eprintln!("✅ Crawl complete");
    Ok(())
}

/// Crawls a single account for the given time window.
///
/// Coordinates the full crawl workflow:
/// 1. Sets up the account (client + database)
/// 2. Discovers joined rooms via sliding sync
/// 3. Decides which rooms need pagination
/// 4. Records virgin rooms that were skipped
/// 5. Crawls rooms in parallel with progress reporting
async fn crawl_account(
    account_id: &str,
    accounts_root: &Path,
    window_scope: &WindowScope,
) -> Result<()> {
    eprintln!("📱 Crawling account: {}", account_id);

    // 1) Account setup
    let (_account_dir, client, db) = setup_account(account_id, accounts_root)
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
        return Ok(());
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
    let (success_count, error_count) =
        crawl_rooms_parallel(rooms_to_crawl, window_scope, &db, account_id, total_rooms).await;

    eprintln!(
        "✅ Crawled {} rooms ({} errors)",
        success_count, error_count
    );

    Ok(())
}

/// Crawls a set of rooms in parallel, respecting concurrency limits.
///
/// Uses async streams to manage concurrent pagination operations.
/// Updates the database after each room completes.
///
/// Returns tuple of (success_count, error_count).
async fn crawl_rooms_parallel(
    rooms: Vec<matrix_sdk::Room>,
    window_scope: &WindowScope,
    db: &crawl_db::CrawlDb,
    account_id: &str,
    total_rooms: usize,
) -> (usize, usize) {
    let mut success_count = 0usize;
    let mut error_count = 0usize;

    let (window_start_ts, _) = window_scope.to_timestamp_range();
    let user_id = account_id.to_string();

    let progress = CrawlProgress::new(total_rooms);
    let progress_for_stream = progress.clone();

    let mut stream = futures_util::stream::iter(rooms)
        .map(move |room| {
            let uid = user_id.clone();
            let progress_for_room = progress_for_stream.clone();
            crawl_single_room(room, window_start_ts, uid, progress_for_room, db)
        })
        .buffer_unordered(MAX_CONCURRENCY);

    while let Some((room, stats_res, spinner)) = stream.next().await {
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
                    progress.println(&format!("  ✗ {} ({})", room_name, e));
                } else {
                    success_count += 1;
                    // Mark as success and update event counts
                    let _ = db.set_crawl_status(&room_id, crawl_db::CrawlStatus::Success);
                    let _ =
                        db.increment_event_counts(&room_id, stats.total_events, stats.user_events);

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
                progress.println(&format!("  ✗ {} ({})", room_name, e));
            }
        }

        progress.inc();
    }

    progress.finish();

    (success_count, error_count)
}

/// Crawls events from a single room.
///
/// Sets up pagination and delegates to the pagination module.
/// Returns the room, result, and optional spinner handle.
async fn crawl_single_room(
    room: matrix_sdk::Room,
    window_start_ts: Option<i64>,
    user_id: String,
    progress: CrawlProgress,
    db: &crawl_db::CrawlDb,
) -> (
    matrix_sdk::Room,
    Result<RoomCrawlStats>,
    Option<indicatif::ProgressBar>,
) {
    // Fetch the room's display name before creating the progress callback
    let room_name = room
        .display_name()
        .await
        .ok()
        .map(|n| n.to_string())
        .unwrap_or_else(|| room.room_id().to_string());

    let (progress_callback, spinner) = progress.make_callback(room_name);

    // Mark room as in-progress
    let room_id = room.room_id().to_string();
    let _ = db.set_crawl_status(&room_id, crawl_db::CrawlStatus::InProgress);

    let stats_res = crawl_room_events(&room, window_start_ts, &user_id, &*progress_callback).await;

    (room, stats_res, spinner)
}
