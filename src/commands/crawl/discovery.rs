/// Room discovery via Matrix sliding sync.
///
/// Discovers joined rooms and fetches their latest event information
/// in a single, efficient sync operation. Does not paginate events.
use anyhow::{Context, Result};
use futures_util::StreamExt;
use matrix_sdk::ruma::events::StateEventType;
use std::path::Path;

use super::types::{RoomInfo, RoomJoinState};

/// State event types needed for room list sync.
/// Inspired by: https://github.com/matrix-org/matrix-rust-sdk/blob/matrix-sdk-ui-0.16.0/crates/matrix-sdk-ui/src/room_list_service/mod.rs#L81
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

/// Batch size for sliding sync room discovery (rooms per batch).
const SLIDING_SYNC_BATCH_SIZE: usize = 50;

/// Initializes the account's client and database.
///
/// Restores an existing SDK session from the account directory and initializes
/// the crawl metadata database.
///
/// # Arguments
///
/// * `account_id` - Matrix user ID (e.g., "@alice:example.org")
/// * `account_dir` - Path to the account directory
pub async fn setup_account(
    account_id: &str,
    account_dir: &Path,
) -> Result<(std::path::PathBuf, matrix_sdk::Client, super::db::CrawlDb)> {
    if !account_dir.exists() {
        anyhow::bail!("Account directory not found: {}", account_dir.display());
    }

    let db = super::db::CrawlDb::init(account_dir)
        .context("Failed to initialize crawl metadata database")?;

    let client = crate::sdk::restore_client_for_account(account_dir, account_id)
        .await
        .context("Failed to restore client")?;

    Ok((account_dir.to_path_buf(), client, db))
}

/// Discovers joined rooms and their latest event via sliding sync.
///
/// Uses growing-mode sliding sync to fetch all joined rooms in batches,
/// requesting only the latest event from each room. This is a fast, deterministic
/// operation that provides room metadata and freshness information for the
/// crawl decision logic.
///
/// # Operation
///
/// 1. Sets up sliding sync in growing mode with batch size of 50 rooms
/// 2. Requests only 1 timeline event per room (the latest)
/// 3. Waits for sync completion (typically 1-2 batches)
/// 4. Extracts room list with latest event ID and timestamp
///
/// # Returns
///
/// A vector of `RoomInfo` containing room ID, latest event ID/timestamp, and join state.
pub async fn fetch_room_list_via_sliding_sync(
    client: &matrix_sdk::Client,
) -> Result<Vec<RoomInfo>> {
    use matrix_sdk::sliding_sync::{SlidingSyncList, SlidingSyncListLoadingState, SlidingSyncMode};

    // Prepare a list builder in growing mode with a reasonable batch size.
    let list_builder = SlidingSyncList::builder("all_rooms")
        .sync_mode(SlidingSyncMode::new_growing(SLIDING_SYNC_BATCH_SIZE as u32))
        .timeline_limit(1) // Only fetch the latest event per room
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

    // Do one final sync iteration to ensure pagination sync state is updated with latest events
    if let Some(result) = sync_stream.next().await {
        result.context("Final sync iteration failed")?;
        eprintln!("  🔄 Final sync iteration completed");
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
