/// Event pagination and aggregation logic.
///
/// Handles backward pagination through a room's timeline, aggregating event
/// statistics and respecting window boundaries.
use anyhow::{Context, Result};

use super::types::{PaginationAggregates, RoomCrawlStats};

/// Batch size for event pagination (events per fetch).
/// Determined by Matrix SDK and server limits.
const PAGINATION_BATCH_SIZE: usize = 100;

/// Sets up the event cache for a room without fetching events.
///
/// Prepares the event cache and returns it along with the latest event
/// currently in memory.
pub async fn setup_event_cache(
    room: &matrix_sdk::Room,
) -> Result<matrix_sdk::event_cache::RoomEventCache> {
    let (room_event_cache, _drop_handles) = room
        .event_cache()
        .await
        .context("Failed to get event cache")?;

    Ok(room_event_cache)
}

/// Extracts the latest event from the event cache without pagination.
///
/// Searches in-memory events only (no network requests) for the newest event.
pub async fn get_latest_event(
    room_event_cache: &matrix_sdk::event_cache::RoomEventCache,
) -> Result<(Option<String>, Option<i64>)> {
    let (event_id, ts) = room_event_cache
        .rfind_map_event_in_memory_by(|event, _prev| {
            let event_id = event.event_id()?;
            let ts: i64 = event.timestamp()?.get().into();
            Some((event_id.to_string(), ts))
        })
        .await
        .ok()
        .flatten()
        .unzip();

    Ok((event_id, ts))
}

/// Gets the room's display name, falling back to room ID if unavailable.
async fn get_room_display_name(room: &matrix_sdk::Room) -> String {
    room.display_name()
        .await
        .map(|name| name.to_string())
        .unwrap_or_else(|_| room.room_id().to_string())
}

/// Paginates events backward and aggregates statistics.
///
/// Continuously fetches events in batches, tracking timestamp bounds,
/// event counts, and user message counts. Stops when:
/// - The room's creation is reached (`reached_start`), OR
/// - No more events are returned, OR
/// - The window start is reached (if specified)
///
/// # Callback
///
/// Invoked after each batch with (`room_name`, `oldest_ts`, `newest_ts`, `total_events`)
/// for progress reporting.
pub async fn paginate_and_aggregate_stats<F>(
    room_event_cache: &matrix_sdk::event_cache::RoomEventCache,
    window_start_ts: Option<i64>,
    user_id: &str,
    room_name: &str,
    newest_event_id_initial: Option<String>,
    newest_ts_initial: Option<i64>,
    progress_callback: F,
) -> Result<PaginationAggregates>
where
    F: Fn(&str, Option<i64>, Option<i64>, usize),
{
    let pagination = room_event_cache.pagination();

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
            .run_backwards_once(PAGINATION_BATCH_SIZE as u16)
            .await
            .context("Pagination failed")?;

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
                continue;
            };
            let ts_millis: i64 = ts.get().into();

            total_events += 1;

            // Count events sent by this user
            if let Ok(deserialized) = event.raw().deserialize() {
                if deserialized.sender() == user_id {
                    user_events += 1;
                }
            }

            // Track oldest event
            if oldest_ts.is_none_or(|old_ts| ts_millis < old_ts) {
                oldest_ts = Some(ts_millis);
                oldest_event_id = event_id_str.clone();
            }

            // Track newest event
            if newest_ts.is_none_or(|new_ts| ts_millis > new_ts) {
                newest_ts = Some(ts_millis);
                newest_event_id = event_id_str;
            }

            // Stop pagination if we've reached the window start
            if let Some(window_start) = window_start_ts {
                if ts_millis <= window_start {
                    stop_at_window = true;
                }
            }
        }

        progress_callback(room_name, oldest_ts, newest_ts, total_events);

        if stop_at_window {
            break;
        }
    }

    Ok(PaginationAggregates {
        fully_crawled,
        oldest_event_id,
        oldest_ts,
        newest_event_id,
        newest_ts,
        total_events,
        user_events,
    })
}

/// Paginates a room's events and collects crawl statistics.
///
/// Coordinates setup and pagination for a single room, returning aggregated
/// statistics suitable for database storage.
///
/// # Arguments
///
/// * `room` - The Matrix room to crawl
/// * `window_start_ts` - Window start (None = no limit)
/// * `user_id` - User ID to count personal messages
/// * `progress_callback` - Called after each pagination batch
pub async fn crawl_room_events<F>(
    room: &matrix_sdk::Room,
    window_start_ts: Option<i64>,
    user_id: &str,
    progress_callback: F,
) -> Result<RoomCrawlStats>
where
    F: Fn(&str, Option<i64>, Option<i64>, usize),
{
    let room_name = get_room_display_name(room).await;

    let room_event_cache = setup_event_cache(room).await?;
    let (newest_event_id_initial, newest_ts_initial) = get_latest_event(&room_event_cache).await?;

    let aggregates = paginate_and_aggregate_stats(
        &room_event_cache,
        window_start_ts,
        user_id,
        &room_name,
        newest_event_id_initial,
        newest_ts_initial,
        progress_callback,
    )
    .await?;

    Ok(RoomCrawlStats {
        room_id: room.room_id().to_string(),
        oldest_event_id: aggregates.oldest_event_id,
        oldest_ts: aggregates.oldest_ts,
        newest_event_id: aggregates.newest_event_id,
        newest_ts: aggregates.newest_ts,
        fully_crawled: aggregates.fully_crawled,
        room_name,
        total_events: aggregates.total_events,
        user_events: aggregates.user_events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_aggregates_structure() {
        // Verify the structure is creatable
        let agg = PaginationAggregates {
            fully_crawled: false,
            oldest_event_id: Some("$e1".to_string()),
            oldest_ts: Some(1000),
            newest_event_id: Some("$e2".to_string()),
            newest_ts: Some(2000),
            total_events: 10,
            user_events: 5,
        };

        assert_eq!(agg.total_events, 10);
        assert_eq!(agg.user_events, 5);
        assert!(!agg.fully_crawled);
    }
}
