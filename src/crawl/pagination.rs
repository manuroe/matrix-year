/// Event pagination and aggregation logic.
///
/// Handles backward pagination through a room's timeline, aggregating event
/// statistics and respecting window boundaries.
use anyhow::{Context, Result};
use chrono::{Datelike, Local, TimeZone, Timelike};
use matrix_sdk::ruma::events::{AnySyncMessageLikeEvent, AnySyncTimelineEvent};
use std::collections::HashMap;
use std::sync::Arc;

use super::types::{DetailedPaginationStats, RoomCrawlStats};

/// Batch size for event pagination (events per fetch).
/// Determined by Matrix SDK and server limits.
const PAGINATION_BATCH_SIZE: usize = 100;

/// Sets up the event cache for a room without fetching events.
///
/// Prepares the event cache and returns it so callers can query in-memory
/// events (e.g., via `get_latest_event`) without performing pagination.
///
/// **Important**: Returns drop handles that must be kept alive for the event
/// cache to remain subscribed to live updates. Drop handles are automatically
/// held by the pagination functions.
pub async fn setup_event_cache(
    room: &matrix_sdk::Room,
) -> Result<(
    matrix_sdk::event_cache::RoomEventCache,
    Arc<matrix_sdk::event_cache::EventCacheDropHandles>,
)> {
    let (room_event_cache, drop_handles) = room
        .event_cache()
        .await
        .context("Failed to get event cache")?;

    Ok((room_event_cache, drop_handles))
}

/// Paginates events backward and collects detailed statistics for stats generation.
///
/// Similar to `paginate_and_aggregate_stats` but collects comprehensive analytics:
/// - Temporal buckets (year, month, week, weekday, day, hour) using local timezone
/// - User message IDs for reaction filtering
/// - Reaction tracking (emojis and per-message counts)
/// - Room creation detection
/// - Active dates for days_active calculation
///
/// Stops when:
/// - The room's creation is reached (`reached_start`), OR
/// - No more events are returned, OR
/// - The window start is reached (if specified)
///
/// # Returns
///
/// Returns a tuple of (RoomCrawlStats, DetailedPaginationStats):
/// - RoomCrawlStats: Basic stats for DB updates (oldest/newest timestamps, event counts)
/// - DetailedPaginationStats: Detailed temporal buckets, reactions, etc. for stats building
///
/// # Callback
///
/// Invoked after each batch with (`room_name`, `oldest_ts`, `newest_ts`, `processed_events`)
/// for progress reporting. `processed_events` counts all events seen (including those
/// outside the window), so the number monotonically increases as pagination proceeds.
#[allow(clippy::too_many_arguments)]
pub async fn paginate_and_collect_detailed_stats<F>(
    room: &matrix_sdk::Room,
    room_event_cache: &matrix_sdk::event_cache::RoomEventCache,
    window_start_ts: Option<i64>,
    window_end_ts: i64,
    user_id: &str,
    room_name: &str,
    newest_event_id_initial: Option<String>,
    newest_ts_initial: Option<i64>,
    progress_callback: F,
) -> Result<(RoomCrawlStats, DetailedPaginationStats)>
where
    F: Fn(&str, Option<i64>, Option<i64>, usize),
{
    let pagination = room_event_cache.pagination();

    let room_id = room.room_id().to_string();

    let mut stats = DetailedPaginationStats {
        fully_crawled: false,
        oldest_event_id: None,
        oldest_ts: None,
        newest_event_id: newest_event_id_initial,
        newest_ts: newest_ts_initial,
        total_events: 0,
        user_events: 0,
        by_year: HashMap::new(),
        by_month: HashMap::new(),
        by_week: HashMap::new(),
        by_weekday: HashMap::new(),
        by_day: HashMap::new(),
        by_hour: HashMap::new(),
        user_message_ids: HashMap::new(),
        reactions_by_emoji: HashMap::new(),
        reactions_by_message: HashMap::new(),
        room_created_by_user: false,
        active_dates: HashMap::new(),
    };

    // Tracks the number of events processed (for progress only). This includes
    // events outside the requested window to ensure the spinner count
    // monotonically increases as we load more history.
    let mut progress_events: usize = 0;

    let mut stop_at_window = false;

    // Load all events currently in the cache before starting backward pagination
    let cached_events = room_event_cache.events().await?;

    // Process all cached events first
    for event in cached_events.iter() {
        let event_id_str = event.event_id().map(|id| id.to_string());
        let ts_millis_opt: Option<i64> = event.timestamp().map(|ts| ts.get().into());

        let Some(ts_millis) = ts_millis_opt else {
            continue;
        };

        // Track oldest/newest events for metadata (regardless of window)
        if stats.oldest_ts.is_none_or(|old_ts| ts_millis < old_ts) {
            stats.oldest_ts = Some(ts_millis);
            stats.oldest_event_id = event_id_str.clone();
        }
        if stats.newest_ts.is_none_or(|new_ts| ts_millis > new_ts) {
            stats.newest_ts = Some(ts_millis);
            stats.newest_event_id = event_id_str.clone();
        }

        // Count event for progress regardless of window inclusion
        progress_events += 1;

        // Skip events outside the window for statistics aggregation
        if let Some(start) = window_start_ts {
            if ts_millis < start {
                continue;
            }
        }
        if ts_millis > window_end_ts {
            continue;
        }

        stats.total_events += 1;

        // Convert timestamp to local datetime for bucketing
        let dt = Local.timestamp_millis_opt(ts_millis).single();
        let Some(dt) = dt else {
            continue;
        };

        // Deserialize event for detailed processing
        let Ok(deserialized) = event.raw().deserialize() else {
            continue;
        };

        let sender = deserialized.sender();
        let is_user_event = sender == user_id;

        // Process different event types
        match deserialized {
            AnySyncTimelineEvent::MessageLike(msg_event) => {
                match msg_event {
                    AnySyncMessageLikeEvent::RoomMessage(_)
                    | AnySyncMessageLikeEvent::RoomEncrypted(_) => {
                        if is_user_event {
                            stats.user_events += 1;

                            // Temporal bucketing (only for user's messages)
                            let year = dt.year().to_string();
                            let month = format!("{:02}", dt.month());
                            let iso_week = dt.iso_week();
                            let week = format!("{}-W{:02}", iso_week.year(), iso_week.week());
                            let weekday = dt.weekday().number_from_monday().to_string();
                            let day = dt.format("%Y-%m-%d").to_string();
                            let hour = format!("{:02}", dt.hour());

                            *stats.by_year.entry(year).or_insert(0) += 1;
                            *stats.by_month.entry(month).or_insert(0) += 1;
                            *stats.by_week.entry(week).or_insert(0) += 1;
                            *stats.by_weekday.entry(weekday).or_insert(0) += 1;
                            *stats.by_day.entry(day.clone()).or_insert(0) += 1;
                            *stats.by_hour.entry(hour).or_insert(0) += 1;

                            // Track active dates
                            stats.active_dates.insert(day, true);

                            // Store user's message ID for reaction filtering
                            if let Some(ref event_id) = event_id_str {
                                stats
                                    .user_message_ids
                                    .insert(event_id.clone(), room_id.clone());
                            }
                        }
                    }
                    AnySyncMessageLikeEvent::Reaction(r) => {
                        // Track reactions
                        let content = r.as_original().map(|o| &o.content);
                        if let Some(content) = content {
                            // Extract emoji from annotation
                            let emoji = content.relates_to.key.clone();
                            let event_id = content.relates_to.event_id.to_string();

                            // Only track reactions on user's messages
                            if stats.user_message_ids.contains_key(&event_id) {
                                *stats.reactions_by_emoji.entry(emoji).or_insert(0) += 1;
                                *stats.reactions_by_message.entry(event_id).or_insert(0) += 1;
                            }
                        }
                    }
                    _ => {
                        // Other message-like events (edits, redactions, etc.) - ignore for now
                    }
                }
            }
            AnySyncTimelineEvent::State(state_event) => {
                // Check for room creation by this user
                if matches!(
                    state_event,
                    matrix_sdk::ruma::events::AnySyncStateEvent::RoomCreate(_)
                ) && is_user_event
                {
                    stats.room_created_by_user = true;
                }
            }
        }
    }

    loop {
        let outcome = pagination
            .run_backwards_once(PAGINATION_BATCH_SIZE as u16)
            .await
            .context("Pagination failed")?;

        if outcome.events.is_empty() {
            if outcome.reached_start {
                stats.fully_crawled = true;
            }
            break;
        }

        // Mark as fully crawled but still process these final events
        if outcome.reached_start {
            stats.fully_crawled = true;
        }

        for event in outcome.events.iter() {
            let event_id_str = event.event_id().map(|id| id.to_string());

            let ts_millis_opt: Option<i64> = event.timestamp().map(|ts| ts.get().into());

            // If there's no timestamp, we cannot bucket or filter; skip further processing
            let Some(ts_millis) = ts_millis_opt else {
                continue;
            };

            // Track oldest/newest events for metadata (regardless of window)
            if stats.oldest_ts.is_none_or(|old_ts| ts_millis < old_ts) {
                stats.oldest_ts = Some(ts_millis);
                stats.oldest_event_id = event_id_str.clone();
            }
            if stats.newest_ts.is_none_or(|new_ts| ts_millis > new_ts) {
                stats.newest_ts = Some(ts_millis);
                stats.newest_event_id = event_id_str.clone();
            }

            // Count event for progress regardless of window inclusion
            progress_events += 1;

            // Skip events outside the window for statistics aggregation
            if let Some(start) = window_start_ts {
                if ts_millis < start {
                    stop_at_window = true;
                    continue;
                }
            }
            if ts_millis > window_end_ts {
                continue;
            }

            stats.total_events += 1;

            // Convert timestamp to local datetime for bucketing
            let dt = Local.timestamp_millis_opt(ts_millis).single();
            let Some(dt) = dt else {
                continue;
            };

            // Deserialize event for detailed processing
            let Ok(deserialized) = event.raw().deserialize() else {
                continue;
            };

            let sender = deserialized.sender();
            let is_user_event = sender == user_id;

            // Process different event types
            match deserialized {
                AnySyncTimelineEvent::MessageLike(msg_event) => {
                    match msg_event {
                        AnySyncMessageLikeEvent::RoomMessage(_)
                        | AnySyncMessageLikeEvent::RoomEncrypted(_) => {
                            if is_user_event {
                                stats.user_events += 1;

                                // Temporal bucketing (only for user's messages)
                                let year = dt.year().to_string();
                                let month = format!("{:02}", dt.month());
                                let iso_week = dt.iso_week();
                                let week = format!("{}-W{:02}", iso_week.year(), iso_week.week());
                                let weekday = dt.weekday().number_from_monday().to_string();
                                let day = dt.format("%Y-%m-%d").to_string();
                                let hour = format!("{:02}", dt.hour());

                                *stats.by_year.entry(year).or_insert(0) += 1;
                                *stats.by_month.entry(month).or_insert(0) += 1;
                                *stats.by_week.entry(week).or_insert(0) += 1;
                                *stats.by_weekday.entry(weekday).or_insert(0) += 1;
                                *stats.by_day.entry(day.clone()).or_insert(0) += 1;
                                *stats.by_hour.entry(hour).or_insert(0) += 1;

                                // Track active dates
                                stats.active_dates.insert(day, true);

                                // Store user's message ID for reaction filtering
                                if let Some(ref event_id) = event_id_str {
                                    stats
                                        .user_message_ids
                                        .insert(event_id.clone(), room_id.clone());
                                }
                            }
                        }
                        AnySyncMessageLikeEvent::Reaction(r) => {
                            // Track reactions
                            let content = r.as_original().map(|o| &o.content);
                            if let Some(content) = content {
                                // Extract emoji from annotation
                                let emoji = content.relates_to.key.clone();
                                let event_id = content.relates_to.event_id.to_string();

                                // Only track reactions on user's messages
                                if stats.user_message_ids.contains_key(&event_id) {
                                    *stats.reactions_by_emoji.entry(emoji).or_insert(0) += 1;
                                    *stats.reactions_by_message.entry(event_id).or_insert(0) += 1;
                                }
                            }
                        }
                        _ => {
                            // Other message-like events (edits, redactions, etc.) - ignore for now
                        }
                    }
                }
                AnySyncTimelineEvent::State(state_event) => {
                    // Check for room creation by this user
                    if matches!(
                        state_event,
                        matrix_sdk::ruma::events::AnySyncStateEvent::RoomCreate(_)
                    ) && is_user_event
                    {
                        stats.room_created_by_user = true;
                    }
                }
            }
        }

        progress_callback(room_name, stats.oldest_ts, stats.newest_ts, progress_events);

        if stop_at_window || stats.fully_crawled {
            break;
        }
    }

    // Build RoomCrawlStats for DB updates
    let crawl_stats = RoomCrawlStats {
        room_id: room.room_id().to_string(),
        oldest_event_id: stats.oldest_event_id.clone(),
        oldest_ts: stats.oldest_ts,
        newest_event_id: stats.newest_event_id.clone(),
        newest_ts: stats.newest_ts,
        fully_crawled: stats.fully_crawled,
        room_name: room_name.to_string(),
        total_events: stats.total_events,
        user_events: stats.user_events,
    };

    Ok((crawl_stats, stats))
}
