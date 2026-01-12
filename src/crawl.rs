/// Matrix event crawling module
///
/// Crawls user-sent messages from Matrix homeserver and stores them in the SDK database.
use anyhow::{Context, Result};
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressStyle};
use matrix_sdk::ruma::events::StateEventType;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::crawl_db;
use crate::login::{account_id_to_dirname, resolve_data_root};
use crate::window::WindowScope;

// Inspired from https://github.com/matrix-org/matrix-rust-sdk/blob/matrix-sdk-ui-0.16.0/crates/matrix-sdk-ui/src/room_list_service/mod.rs#L81
const REQUIRED_STATE: &[(StateEventType, &str)] = &[
    (StateEventType::RoomName, ""),
    (StateEventType::RoomEncryption, ""),
    (StateEventType::RoomMember, "$LAZY"),
    (StateEventType::RoomMember, "$ME"),
    (StateEventType::RoomCanonicalAlias, ""),
    (StateEventType::CallMember, "*"),
    (StateEventType::RoomJoinRules, ""),
    (StateEventType::RoomTombstone, ""),
    (StateEventType::RoomCreate, ""),
    (StateEventType::RoomHistoryVisibility, ""),
    (StateEventType::MemberHints, ""),
    (StateEventType::SpaceParent, "*"),
    (StateEventType::SpaceChild, "*"),
];

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

    // Progress tracking
    let multi_progress = MultiProgress::new();
    let overall_style = ProgressStyle::default_bar()
        .template("{msg}")
        .context("Failed to create progress style")?;

    // Crawl each account
    for account_id in target_accounts {
        crawl_account(
            &account_id,
            &accounts_root,
            &window_scope,
            &multi_progress,
            &overall_style,
        )
        .await
        .unwrap_or_else(|e| {
            eprintln!("❌ Error crawling {}: {}", account_id, e);
        });
    }

    eprintln!("✅ Crawl complete");
    Ok(())
}

async fn crawl_account(
    account_id: &str,
    accounts_root: &Path,
    window_scope: &WindowScope,
    _multi_progress: &indicatif::MultiProgress,
    _overall_style: &ProgressStyle,
) -> Result<()> {
    eprintln!("📱 Crawling account: {}", account_id);

    // 1) Account setup
    let (_account_dir, client, db) = setup_account(account_id, accounts_root)
        .await
        .context("Account setup failed")?;

    // 2) Update room list (deterministic discovery)
    let room_list = update_room_list(&client).await?;

    // 3) Check rooms needing crawl
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
    for room in &joined_rooms {
        let room_id_str = room.room_id().to_string();
        if let Ok(None) = db.get_room_metadata(&room_id_str) {
            // Virgin room not selected for crawl
            if !rooms_to_crawl.iter().any(|r| r.room_id() == room.room_id()) {
                // This room was skipped, record that we've seen it
                if let Some((event_id, event_ts)) = latest_events.get(&room_id_str) {
                    let _ = db.update_room_metadata(
                        &room_id_str,
                        Some(event_id.clone()),
                        Some(*event_ts),
                        Some(event_id.clone()),
                        Some(*event_ts),
                        false,
                    );
                }
            }
        }
    }

    eprintln!(
        "📚 Found {} joined room(s), {} to crawl...",
        joined_rooms.len(),
        rooms_to_crawl.len()
    );

    // 4) Crawl rooms (parallel pagination, sequential DB updates)
    let total_rooms = rooms_to_crawl.len();
    let (success_count, error_count, skipped_count) =
        crawl_rooms_parallel(rooms_to_crawl, window_scope, &db, account_id, total_rooms).await;

    if skipped_count > 0 {
        eprintln!("⏭️  Skipped {} rooms (already crawled)", skipped_count);
    }
    eprintln!(
        "✅ Crawled {} rooms ({} errors, {} skipped)",
        success_count, error_count, skipped_count
    );

    Ok(())
}
// --- Helpers ---

async fn setup_account(
    account_id: &str,
    accounts_root: &Path,
) -> Result<(std::path::PathBuf, matrix_sdk::Client, crawl_db::CrawlDb)> {
    let dir_name = account_id_to_dirname(account_id);
    let account_dir = accounts_root.join(&dir_name);
    if !account_dir.exists() {
        anyhow::bail!("Account directory not found: {}", account_dir.display());
    }

    let db = crawl_db::CrawlDb::init(&account_dir)
        .context("Failed to initialize crawl metadata database")?;

    let client = crate::sdk::restore_client_for_account(&account_dir, account_id)
        .await
        .context("Failed to restore client")?;

    Ok((account_dir, client, db))
}

#[derive(Clone, Debug)]
enum RoomJoinState {
    Joined,
    #[allow(dead_code)]
    Left,
    #[allow(dead_code)]
    Invited,
    #[allow(dead_code)]
    JoinedSpace,
}

#[derive(Clone, Debug)]
struct RoomInfo {
    room_id: String,
    last_event_id: Option<String>,
    last_event_ts: Option<i64>,
    join_state: RoomJoinState,
}

async fn update_room_list(client: &matrix_sdk::Client) -> Result<Vec<RoomInfo>> {
    use matrix_sdk::sliding_sync::{SlidingSyncList, SlidingSyncListLoadingState, SlidingSyncMode};

    // Prepare a list builder in growing mode with a reasonable batch size.
    let list_builder = SlidingSyncList::builder("all_rooms")
        .sync_mode(SlidingSyncMode::new_growing(50))
        .timeline_limit(1)
        .required_state(
            REQUIRED_STATE
                .iter()
                .map(|(state_event, value)| (state_event.clone(), (*value).to_owned()))
                .collect(),
        );

    let sliding = client
        .sliding_sync("my-all")?
        .add_cached_list(list_builder)
        .await?
        .share_pos()
        .poll_timeout(std::time::Duration::from_secs(0))
        .build()
        .await
        .context("Failed to build sliding sync")?;

    // Ensure the global event cache is subscribed so room event caches can be queried.
    client
        .event_cache()
        .subscribe()
        .context("Failed to subscribe event cache")?;

    let sync_stream = sliding.sync();
    futures_util::pin_mut!(sync_stream);

    let list_handle = sliding
        .on_list("all_rooms", |list| {
            futures_util::future::ready(list.clone())
        })
        .await
        .expect("list should exist");
    let (current_state, mut state_stream) = list_handle.state_stream();

    let mut sync_count = 0;
    let mut fully_loaded = matches!(current_state, SlidingSyncListLoadingState::FullyLoaded);
    while !fully_loaded {
        tokio::select! {
            state = state_stream.next() => {
                if let Some(state) = state {
                    if matches!(state, SlidingSyncListLoadingState::FullyLoaded) {
                        fully_loaded = true;
                    }
                }
            }
            _tick = tokio::time::sleep(std::time::Duration::from_millis(200)) => {
                if let Some(state) = sliding
                    .on_list("all_rooms", |list| futures_util::future::ready(list.state()))
                    .await
                {
                    if matches!(state, SlidingSyncListLoadingState::FullyLoaded) {
                        fully_loaded = true;
                    }
                }
            }
            sync_result = sync_stream.next() => {
                if let Some(result) = sync_result {
                    if let Err(e) = result {
                        eprintln!("\n❌ Sync error details: {:#}", e);
                        return Err(e).context("Sync failed");
                    }
                    sync_count += 1;
                    eprintln!("  🔄 Sync #{} completed", sync_count);
                }
            }
        }
    }

    // Extract room list with latest events
    let mut room_list = Vec::new();

    eprintln!("🔍 Extracting room list...");
    for room in client.joined_rooms() {
        let room_id = room.room_id().to_string();
        let last_event = match room.event_cache().await {
            Ok((cache, _)) => cache
                .rfind_map_event_in_memory_by(|event, _prev| {
                    let event_id = event.event_id()?;
                    let ts: i64 = event.timestamp()?.get().into();
                    Some((event_id.to_string(), ts))
                })
                .await
                .ok()
                .flatten(),
            Err(_) => None,
        };

        room_list.push(RoomInfo {
            room_id,
            last_event_id: last_event.as_ref().map(|(id, _)| id.clone()),
            last_event_ts: last_event.map(|(_, ts)| ts),
            join_state: RoomJoinState::Joined,
        });
    }

    eprintln!("  ✓ Extracted {} rooms", room_list.len());
    Ok(room_list)
}

fn select_rooms_to_crawl(
    joined_rooms: &[matrix_sdk::Room],
    db: &crawl_db::CrawlDb,
    window_start_ts: Option<i64>,
    window_end_ts: Option<i64>,
    latest_events: &HashMap<String, (String, i64)>,
) -> Vec<matrix_sdk::Room> {
    let mut rooms = Vec::new();
    for room in joined_rooms.iter() {
        let room_id_str = room.room_id().to_string();
        let needs_crawl = should_crawl_room(
            db,
            &room_id_str,
            window_start_ts,
            window_end_ts.expect("window_end_ts required"),
            latest_events.get(&room_id_str),
        )
        .unwrap_or(true);
        if needs_crawl {
            rooms.push(room.clone());
        }
    }
    rooms
}

fn should_crawl_room(
    db: &crawl_db::CrawlDb,
    room_id: &str,
    window_start_ts: Option<i64>, // None means "beginning of time"
    window_end_ts: i64,           // End of requested window
    latest_event: Option<&(String, i64)>,
) -> Result<bool> {
    let metadata = db.get_room_metadata(room_id)?;

    let Some(meta) = metadata else {
        // Virgin room: check if it has events in the requested window
        if let Some((_latest_id, latest_ts)) = latest_event {
            // If latest event is before window start, skip this room
            if let Some(start) = window_start_ts {
                if *latest_ts < start {
                    return Ok(false);
                }
            }
            // Latest event is in or after window start, crawl it
            return Ok(true);
        }
        // No latest event at all, need to crawl to discover content
        return Ok(true);
    };

    // Determine if we still need to extend the old end of coverage
    let old_end_needs_crawl = match window_start_ts {
        None => !meta.fully_crawled,
        Some(start) => !meta.fully_crawled && meta.oldest_event_ts.is_none_or(|ts| ts > start),
    };

    // Determine if we need newer events to reach the window end
    let mut new_end_needs_crawl = meta.newest_event_ts.is_none_or(|ts| ts < window_end_ts);

    // If the latest event reported by discovery matches exactly what we have (id and ts),
    // there's no need to crawl the new end. We still might need the old end.
    if let Some((latest_id, latest_ts)) = latest_event {
        if meta.newest_event_id.as_deref() == Some(latest_id)
            && meta.newest_event_ts == Some(*latest_ts)
        {
            new_end_needs_crawl = false;
        }
    }

    Ok(old_end_needs_crawl || new_end_needs_crawl)
}

struct RoomCrawlStats {
    room_id: String,
    oldest_event_id: Option<String>,
    oldest_ts: Option<i64>,
    newest_event_id: Option<String>,
    newest_ts: Option<i64>,
    fully_crawled: bool,
    room_name: String,
    total_events: usize,
    user_events: usize,
}

fn fmt_ts(ts: Option<i64>) -> String {
    match ts {
        Some(ms) => {
            use std::time::UNIX_EPOCH;
            let duration = std::time::Duration::from_millis(ms as u64);
            let system_time = UNIX_EPOCH + duration;
            match system_time.duration_since(UNIX_EPOCH) {
                Ok(_) => {
                    let datetime: chrono::DateTime<chrono::Utc> = system_time.into();
                    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
                }
                Err(_) => "invalid timestamp".to_string(),
            }
        }
        None => "-".to_string(),
    }
}

async fn paginate_room_until<F>(
    room: &matrix_sdk::Room,
    window_start_ts: Option<i64>,
    user_id: &str,
    progress_callback: F,
) -> Result<RoomCrawlStats>
where
    F: Fn(&str, Option<i64>, Option<i64>, usize),
{
    let room_name = room
        .display_name()
        .await
        .map(|name| name.to_string())
        .unwrap_or_else(|_| room.room_id().to_string());

    // Use event cache API for direct access to events
    let (room_event_cache, _drop_handles) = room
        .event_cache()
        .await
        .context("Failed to get event cache")?;

    // Capture the latest event BEFORE pagination (which goes backward in time)
    let (newest_event_id_initial, newest_ts_initial) = room_event_cache
        .rfind_map_event_in_memory_by(|event, _prev| {
            let event_id = event.event_id()?;
            let ts: i64 = event.timestamp()?.get().into();
            Some((event_id.to_string(), ts))
        })
        .await
        .ok()
        .flatten()
        .unzip();

    let pagination = room_event_cache.pagination();

    // Aggregate stats while paginating to avoid holding all events in memory at once
    let mut fully_crawled = false;
    let mut stop_at_window = false;
    let mut oldest_event_id: Option<String> = None;
    let mut oldest_ts: Option<i64> = None;
    let mut newest_event_id = newest_event_id_initial;
    let mut newest_ts = newest_ts_initial;
    let mut total_events = 0usize;
    let mut user_events = 0usize;

    loop {
        let outcome = pagination
            .run_backwards_once(100)
            .await
            .context("Pagination failed")?;

        // Check reached_start BEFORE checking if events is empty
        // because SDK may return empty batch with reached_start=true
        if outcome.reached_start {
            fully_crawled = true;
            break;
        }

        if outcome.events.is_empty() {
            break;
        }

        for event in outcome.events.iter() {
            let event_id_str = event.event_id().map(|id| id.to_string());

            let Some(ts) = event.timestamp() else {
                continue; // Skip events without timestamp
            };
            let ts_millis: i64 = ts.get().into();

            total_events += 1;

            if let Ok(deserialized) = event.raw().deserialize() {
                if deserialized.sender() == user_id {
                    user_events += 1;
                }
            }

            if oldest_ts.is_none_or(|old_ts| ts_millis < old_ts) {
                oldest_ts = Some(ts_millis);
                oldest_event_id = event_id_str.clone();
            }

            // During backward pagination, we only update newest if we find something newer
            // (which shouldn't happen, but keep it for safety)
            if newest_ts.is_none_or(|new_ts| ts_millis > new_ts) {
                newest_ts = Some(ts_millis);
                newest_event_id = event_id_str;
            }

            if let Some(window_start) = window_start_ts {
                if ts_millis <= window_start {
                    stop_at_window = true;
                }
            }
        }

        // Call progress callback after each batch
        progress_callback(&room_name, oldest_ts, newest_ts, total_events);

        if stop_at_window {
            break;
        }
    }

    Ok(RoomCrawlStats {
        room_id: room.room_id().to_string(),
        oldest_event_id,
        oldest_ts,
        newest_event_id,
        newest_ts,
        fully_crawled,
        room_name,
        total_events,
        user_events,
    })
}

async fn crawl_rooms_parallel(
    rooms: Vec<matrix_sdk::Room>,
    window_scope: &WindowScope,
    db: &crawl_db::CrawlDb,
    account_id: &str,
    total_rooms: usize,
) -> (usize, usize, usize) {
    use futures_util::StreamExt as _;
    use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

    const MAX_CONCURRENCY: usize = 8;
    let mut success_count = 0usize;
    let mut error_count = 0usize;
    let skipped_count = 0usize; // already filtered out

    let (window_start_ts, _) = window_scope.to_timestamp_range();
    let user_id = account_id.to_string();

    // Check if stderr is a TTY to decide whether to show progress
    let is_tty = atty::is(atty::Stream::Stderr);

    let (multi_progress, overall_pb) = if is_tty {
        let mp = MultiProgress::new();
        // Overall progress bar at the bottom
        let overall_style = ProgressStyle::default_bar()
            .template("[{bar:40.cyan/blue}] {pos}/{len} rooms ({percent}%)")
            .unwrap()
            .progress_chars("█▓░");
        let overall = mp.add(ProgressBar::new(total_rooms as u64));
        overall.set_style(overall_style);
        (Some(mp), Some(overall))
    } else {
        (None, None)
    };

    // Clone overall_pb for use in the stream
    let overall_for_stream = overall_pb.clone();

    let mut stream = futures_util::stream::iter(rooms)
        .map(move |room| {
            let uid = user_id.clone();
            let mp = multi_progress.clone();
            let overall = overall_for_stream.clone();
            async move {
                let room_name = room
                    .display_name()
                    .await
                    .ok()
                    .map(|n| n.to_string())
                    .unwrap_or_else(|| room.room_id().to_string());

                // Create progress bar for this room
                let pb = if let Some(ref mp) = mp {
                    let style = ProgressStyle::default_spinner()
                        .template("{spinner:.green} {msg}")
                        .unwrap()
                        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");
                    let pb = if let Some(ref overall_bar) = overall {
                        mp.insert_before(overall_bar, ProgressBar::new_spinner())
                    } else {
                        mp.add(ProgressBar::new_spinner())
                    };
                    pb.set_style(style);
                    pb.set_message(format!("{}: 0 events", room_name));
                    pb.enable_steady_tick(std::time::Duration::from_millis(100));
                    Some(pb)
                } else {
                    None
                };

                // Progress callback to update progress bar
                let progress_callback = {
                    let pb = pb.clone();
                    let room_name_for_progress = room_name.clone();
                    move |_name: &str, oldest: Option<i64>, _newest: Option<i64>, events: usize| {
                        if let Some(ref pb) = pb {
                            let msg = if let Some(ts) = oldest {
                                let timestamp = fmt_ts(Some(ts));
                                format!(
                                    "{}: {} events from {}",
                                    room_name_for_progress, events, timestamp
                                )
                            } else {
                                format!("{}: {} events", room_name_for_progress, events)
                            };
                            pb.set_message(msg);
                        }
                    }
                };

                let stats_res =
                    paginate_room_until(&room, window_start_ts, &uid, progress_callback).await;

                // Print to scrollback and clear progress bar
                if let Some(ref pb) = pb {
                    if let Ok(stats) = &stats_res {
                        let creation_indicator = if stats.fully_crawled { "🌱" } else { " " };
                        let time_range = match (stats.oldest_ts, stats.newest_ts) {
                            (Some(oldest), Some(newest)) => {
                                format!(
                                    "{} {} → {}",
                                    fmt_ts(Some(oldest)),
                                    creation_indicator,
                                    fmt_ts(Some(newest))
                                )
                            }
                            _ => "unknown".to_string(),
                        };
                        // Print to scrollback
                        pb.println(format!("  ✓ {}", stats.room_name));
                        pb.println(format!("\t📅 {}", time_range));
                        pb.println(format!(
                            "\t📊 {} events ({} from you)",
                            stats.total_events, stats.user_events
                        ));
                        pb.finish_and_clear();
                    } else {
                        pb.println(format!("  ✗ {} | ❌ failed", room_name));
                        pb.finish_and_clear();
                    }
                } else if let Ok(stats) = &stats_res {
                    // Non-TTY output
                    eprintln!("  ✓ {}", stats.room_name);
                    let creation_indicator = if stats.fully_crawled { "🌱" } else { " " };
                    let time_range = match (stats.oldest_ts, stats.newest_ts) {
                        (Some(oldest), Some(newest)) => {
                            format!(
                                "{} {} →  {}",
                                fmt_ts(Some(oldest)),
                                creation_indicator,
                                fmt_ts(Some(newest))
                            )
                        }
                        _ => "unknown".to_string(),
                    };
                    eprintln!("\t📅 {}", time_range);
                    eprintln!(
                        "\t📊 {} events ({} from you)",
                        stats.total_events, stats.user_events
                    );
                }
                (room, stats_res, pb)
            }
        })
        .buffer_unordered(MAX_CONCURRENCY);

    while let Some((room, stats_res, _pb)) = stream.next().await {
        match stats_res {
            Ok(stats) => {
                if let Err(e) = db.update_room_metadata(
                    &stats.room_id,
                    stats.oldest_event_id,
                    stats.oldest_ts,
                    stats.newest_event_id,
                    stats.newest_ts,
                    stats.fully_crawled,
                ) {
                    error_count += 1;
                    if let Some(ref pb) = overall_pb {
                        pb.println(format!("  ✗ {} ({})", stats.room_name, e));
                    } else {
                        eprintln!("  ✗ {} ({})", stats.room_name, e);
                    }
                } else {
                    success_count += 1;
                }
            }
            Err(e) => {
                error_count += 1;
                let room_name = room.room_id().to_string();
                if let Some(ref pb) = overall_pb {
                    pb.println(format!("  ✗ {} ({})", room_name, e));
                } else {
                    eprintln!("  ✗ {} ({})", room_name, e);
                }
            }
        }

        // Increment overall progress
        if let Some(ref pb) = overall_pb {
            pb.inc(1);
        }
    }

    // Finish overall progress bar
    if let Some(pb) = overall_pb {
        pb.finish_and_clear();
    }

    (success_count, error_count, skipped_count)
}

// Removed old paginate_room; replaced by paginate_room_until + DB update in crawl_rooms_parallel.

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> anyhow::Result<(crawl_db::CrawlDb, tempfile::TempDir)> {
        let tmp = tempfile::tempdir()?;
        let db = crawl_db::CrawlDb::init(tmp.path())?;
        Ok((db, tmp))
    }

    #[test]
    fn needs_crawl_when_no_metadata() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, None)?;
        assert!(needs, "rooms without metadata must be crawled");
        Ok(())
    }

    #[test]
    fn needs_crawl_when_newest_before_window_start() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest".to_owned()),
            Some(500),
            Some("newest".to_owned()),
            Some(1_000),
            false,
        )?;

        let needs = should_crawl_room(&db, "!room", Some(2_000), 3_000, None)?;
        assert!(needs, "stale newest timestamp should trigger a crawl");
        Ok(())
    }

    #[test]
    fn skips_when_window_covered() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest".to_owned()),
            Some(500),
            Some("newest".to_owned()),
            Some(3_000),
            true,
        )?;

        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, None)?;
        assert!(!needs, "window fully covered should skip crawling");
        Ok(())
    }

    #[test]
    fn skips_when_latest_matches_db() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest".to_owned()),
            Some(500),
            Some("evt1".to_owned()),
            Some(1_500),
            true,
        )?;

        let latest = ("evt1".to_owned(), 1_500);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(!needs, "matching newest event should not trigger crawl");
        Ok(())
    }

    #[test]
    fn skips_virgin_room_outside_window() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Virgin room with latest event before window start
        let latest = ("evt1".to_owned(), 500);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(
            !needs,
            "virgin room with events before window should be skipped"
        );
        Ok(())
    }

    #[test]
    fn crawls_virgin_room_with_events_in_window() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Virgin room with latest event in window
        let latest = ("evt1".to_owned(), 1_500);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(needs, "virgin room with events in window should be crawled");
        Ok(())
    }

    #[test]
    fn needs_crawl_when_latest_matches_but_old_end_not_covered() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Room already has newest matching server, but oldest doesn't reach window start and not fully crawled
        db.update_room_metadata(
            "!room",
            Some("oldest_evt".to_owned()),
            Some(1_500), // oldest known is after window_start (1000), so we still need to crawl older
            Some("evt_match".to_owned()),
            Some(2_000),
            false, // not fully crawled
        )?;

        let latest_from_server = ("evt_match".to_owned(), 2_000);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 3_000, Some(&latest_from_server))?;
        // We expect true because we haven't covered the old end of the window.
        // Current implementation returns false due to an early return when newest matches.
        assert!(
            needs,
            "should still crawl to cover older messages even if newest matches"
        );
        Ok(())
    }

    #[test]
    fn skips_when_fully_crawled_and_window_end_covered() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Room is fully crawled (reached creation) and newest is at or beyond window end
        db.update_room_metadata(
            "!room",
            Some("oldest_ever".to_owned()),
            Some(100), // very old, at room creation
            Some("newest_evt".to_owned()),
            Some(3_000),
            true, // fully crawled to creation
        )?;

        let latest = ("newest_evt".to_owned(), 3_000);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(!needs, "fully crawled with window end covered should skip");
        Ok(())
    }

    #[test]
    fn skips_virgin_room_when_latest_before_window_start() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Virgin room but latest event is before the window we care about
        let latest = ("old_evt".to_owned(), 500);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(!needs, "virgin room with events before window should skip");
        Ok(())
    }

    #[test]
    fn crawls_with_window_start_none_when_not_fully_crawled() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Requesting full history (window_start = None) but room not fully crawled
        db.update_room_metadata(
            "!room",
            Some("oldest".to_owned()),
            Some(1_000),
            Some("newest".to_owned()),
            Some(2_000),
            false, // not fully crawled
        )?;

        let latest = ("newest".to_owned(), 2_000);
        let needs = should_crawl_room(&db, "!room", None, 3_000, Some(&latest))?;
        assert!(needs, "window_start=None should crawl if not fully_crawled");
        Ok(())
    }

    #[test]
    fn skips_with_window_start_none_when_fully_crawled() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Requesting full history and room is fully crawled (reached creation)
        db.update_room_metadata(
            "!room",
            Some("oldest_ever".to_owned()),
            Some(1),
            Some("newest".to_owned()),
            Some(2_000),
            true, // fully crawled
        )?;

        let latest = ("newest".to_owned(), 2_000);
        let needs = should_crawl_room(&db, "!room", None, 3_000, Some(&latest))?;
        assert!(!needs, "window_start=None should skip if fully_crawled");
        Ok(())
    }

    #[test]
    fn crawls_when_newest_ts_before_window_end() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Newest event in DB is older than the window end, and no newer event from server
        db.update_room_metadata(
            "!room",
            Some("oldest".to_owned()),
            Some(500),
            Some("old_newest".to_owned()),
            Some(1_500), // older than window_end (2000)
            false,
        )?;

        // Simulate server has a newer event than what we know
        let latest = ("newer_event".to_owned(), 1_750);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(needs, "should crawl when newest_ts < window_end");
        Ok(())
    }

    #[test]
    fn sequential_crawls_first_then_second_window() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        // Simulate: first crawl 2024, then crawl 2025
        // After crawling 2024 (window: 2024-01-01 to 2024-12-31)
        db.update_room_metadata(
            "!room",
            Some("evt_2024_jan".to_owned()),
            Some(1_704_067_200_000), // 2024-01-01
            Some("evt_2024_dec".to_owned()),
            Some(1_735_689_599_999), // 2024-12-31
            false,                   // not back to creation, but covered 2024
        )?;

        // Now trying to crawl 2025 (2025-01-01 to 2025-12-31)
        let window_2025_start = 1_735_689_600_000i64; // 2025-01-01
        let window_2025_end = 1_767_225_599_999i64; // 2025-12-31
        let latest_2025 = ("evt_2025_dec".to_owned(), window_2025_end);

        let needs = should_crawl_room(
            &db,
            "!room",
            Some(window_2025_start),
            window_2025_end,
            Some(&latest_2025),
        )?;
        // We should crawl because newest in DB (2024-12-31) is before window_end (2025-12-31)
        assert!(needs, "should crawl 2025 window even after crawling 2024");
        Ok(())
    }
}
