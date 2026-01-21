/// Statistics aggregation and generation module.
///
/// Combines room-level statistics into account-level Stats structures.
/// Computes peaks, rankings, and aggregates temporal data.
use anyhow::Result;
use std::collections::HashMap;

use crate::crawl::types::DetailedPaginationStats;
use crate::crawl::RoomType;
use crate::stats::*;
use crate::window::WindowScope;

/// Input data for building stats for a single room.
pub struct RoomStatsInput {
    pub room_id: String,
    pub room_name: Option<String>,
    pub room_type: RoomType,
    pub stats: DetailedPaginationStats,
}

// ============================================================================
// Intermediate Aggregation Structs (private, internal to stats_builder)
// ============================================================================

/// Aggregated temporal data across all rooms (private).
struct TemporalAggregates {
    by_year: HashMap<String, i32>,
    by_month: HashMap<String, i32>,
    by_week: HashMap<String, i32>,
    by_weekday: HashMap<String, i32>,
    by_day: HashMap<String, i32>,
    by_hour: HashMap<String, i32>,
}

impl TemporalAggregates {
    fn new() -> Self {
        Self {
            by_year: HashMap::new(),
            by_month: HashMap::new(),
            by_week: HashMap::new(),
            by_weekday: HashMap::new(),
            by_day: HashMap::new(),
            by_hour: HashMap::new(),
        }
    }

    fn aggregate_from(&mut self, other: &DetailedPaginationStats) {
        for (key, count) in &other.by_year {
            *self.by_year.entry(key.clone()).or_insert(0) += count;
        }
        for (key, count) in &other.by_month {
            *self.by_month.entry(key.clone()).or_insert(0) += count;
        }
        for (key, count) in &other.by_week {
            *self.by_week.entry(key.clone()).or_insert(0) += count;
        }
        for (key, count) in &other.by_weekday {
            *self.by_weekday.entry(key.clone()).or_insert(0) += count;
        }
        for (key, count) in &other.by_day {
            *self.by_day.entry(key.clone()).or_insert(0) += count;
        }
        for (key, count) in &other.by_hour {
            *self.by_hour.entry(key.clone()).or_insert(0) += count;
        }
    }
}

/// Aggregated reaction data across all rooms (private).
struct ReactionAggregates {
    by_emoji: HashMap<String, i32>,
    by_message: HashMap<String, i32>,
}

impl ReactionAggregates {
    fn new() -> Self {
        Self {
            by_emoji: HashMap::new(),
            by_message: HashMap::new(),
        }
    }

    fn aggregate_from(&mut self, other: &DetailedPaginationStats) {
        for (emoji, count) in &other.reactions_by_emoji {
            *self.by_emoji.entry(emoji.clone()).or_insert(0) += count;
        }
        for (msg_id, count) in &other.reactions_by_message {
            *self.by_message.entry(msg_id.clone()).or_insert(0) += count;
        }
    }
}

/// Room type distribution metrics (private).
struct RoomTypeMetrics {
    dm_count: i32,
    public_count: i32,
    private_count: i32,
    dm_messages: i32,
    public_messages: i32,
    private_messages: i32,
}

impl RoomTypeMetrics {
    fn new() -> Self {
        Self {
            dm_count: 0,
            public_count: 0,
            private_count: 0,
            dm_messages: 0,
            public_messages: 0,
            private_messages: 0,
        }
    }

    fn record(&mut self, room_type: RoomType, message_count: i32) {
        match room_type {
            RoomType::Dm => {
                self.dm_count += 1;
                self.dm_messages += message_count;
            }
            RoomType::Public => {
                self.public_count += 1;
                self.public_messages += message_count;
            }
            RoomType::Private => {
                self.private_count += 1;
                self.private_messages += message_count;
            }
        }
    }

    fn total_messages(&self) -> i32 {
        self.dm_messages + self.public_messages + self.private_messages
    }
}

/// Created room metrics (private).
struct CreatedRoomMetrics {
    total: i32,
    dm: i32,
    public: i32,
    private: i32,
}

impl CreatedRoomMetrics {
    fn new() -> Self {
        Self {
            total: 0,
            dm: 0,
            public: 0,
            private: 0,
        }
    }

    fn record(&mut self, room_type: RoomType) {
        self.total += 1;
        match room_type {
            RoomType::Dm => self.dm += 1,
            RoomType::Public => self.public += 1,
            RoomType::Private => self.private += 1,
        }
    }
}

/// Coverage bounds tracking (private).
struct CoverageBounds {
    oldest_ts: Option<i64>,
    newest_ts: Option<i64>,
    active_dates: HashMap<String, bool>,
}

impl CoverageBounds {
    fn new() -> Self {
        Self {
            oldest_ts: None,
            newest_ts: None,
            active_dates: HashMap::new(),
        }
    }

    fn update_from(&mut self, other: &DetailedPaginationStats) {
        if let Some(ts) = other.oldest_ts {
            self.oldest_ts = Some(self.oldest_ts.map_or(ts, |old| old.min(ts)));
        }
        if let Some(ts) = other.newest_ts {
            self.newest_ts = Some(self.newest_ts.map_or(ts, |new| new.max(ts)));
        }
        for date in other.active_dates.keys() {
            self.active_dates.insert(date.clone(), true);
        }
    }
}

/// Builds account-level Stats from room-level detailed statistics.
///
/// Aggregates data from all crawled rooms:
/// - Combines temporal buckets
/// - Computes peaks (strongest periods)
/// - Ranks top rooms, emojis, and messages
/// - Calculates room type distributions
/// - Generates coverage information
///
/// # Arguments
///
/// * `room_inputs` - Statistics from each crawled room
/// * `account_id` - Matrix user ID
/// * `account_display_name` - User's display name (if available)
/// * `account_avatar_url` - User's avatar MXC URL (if available)
/// * `window_scope` - Time window being analyzed
/// * `total_rooms` - Total number of joined rooms for the account
pub fn build_stats(
    room_inputs: Vec<RoomStatsInput>,
    account_id: &str,
    account_display_name: Option<String>,
    account_avatar_url: Option<String>,
    window_scope: &WindowScope,
    total_rooms: usize,
) -> Result<Stats> {
    // Initialize aggregation structures
    let mut temporal = TemporalAggregates::new();
    let mut reactions = ReactionAggregates::new();
    let mut room_types = RoomTypeMetrics::new();
    let mut created_rooms = CreatedRoomMetrics::new();
    let mut coverage = CoverageBounds::new();

    // Track room-level metrics for ranking
    let mut room_message_counts: Vec<(String, Option<String>, RoomType, i32)> = Vec::new();
    let mut active_rooms_count = 0;

    // Aggregate stats from each room
    for room_input in &room_inputs {
        let room_stats = &room_input.stats;
        let user_messages = room_stats.user_events as i32;

        // Skip rooms where user sent no messages (for active rooms count)
        if user_messages == 0 {
            continue;
        }

        active_rooms_count += 1;

        // Aggregate temporal data
        temporal.aggregate_from(room_stats);

        // Aggregate reactions
        reactions.aggregate_from(room_stats);

        // Track room type distribution
        room_types.record(room_input.room_type, user_messages);

        // Track room creation
        if room_stats.room_created_by_user {
            created_rooms.record(room_input.room_type);
        }

        // Update coverage bounds and active dates
        coverage.update_from(room_stats);

        // Collect room info for ranking
        room_message_counts.push((
            room_input.room_id.clone(),
            room_input.room_name.clone(),
            room_input.room_type,
            user_messages,
        ));
    }

    // Calculate total messages sent
    let messages_sent = room_types.total_messages();

    // Compute peaks
    let peaks = compute_peaks(
        &temporal.by_year,
        &temporal.by_month,
        &temporal.by_week,
        &temporal.by_day,
        &temporal.by_hour,
    )?;

    // Rank top rooms
    let top_rooms = rank_top_rooms(&mut room_message_counts, messages_sent)?;

    // Rank top emojis
    let top_emojis = rank_top_emojis(reactions.by_emoji)?;

    // Rank top messages
    let top_messages = rank_top_messages(reactions.by_message)?;

    // Calculate total reactions
    let total_reactions: i32 = top_emojis.iter().map(|e| e.count).sum();

    // Build coverage information
    let (coverage_from, coverage_to, days_active) =
        compute_coverage_bounds(&coverage, window_scope)?;

    // Build activity section early to consume temporal struct
    let activity = build_activity_section(temporal, messages_sent)?;

    // Build Stats struct
    let stats = Stats {
        schema_version: 1,
        scope: Scope {
            kind: window_scope.scope_type,
            key: window_scope.key.clone(),
            label: None,
        },
        generated_at: chrono::Local::now().format("%Y-%m-%d").to_string(),
        account: Account {
            user_id: account_id.to_string(),
            display_name: account_display_name,
            avatar_url: account_avatar_url,
            rooms_total: total_rooms as i32,
        },
        coverage: Coverage {
            from: coverage_from,
            to: coverage_to,
            days_active,
        },
        summary: Summary {
            messages_sent,
            active_rooms: active_rooms_count,
            dm_rooms: if room_types.dm_count > 0 {
                Some(room_types.dm_count)
            } else {
                None
            },
            public_rooms: if room_types.public_count > 0 {
                Some(room_types.public_count)
            } else {
                None
            },
            private_rooms: if room_types.private_count > 0 {
                Some(room_types.private_count)
            } else {
                None
            },
            peaks,
        },
        activity,
        rooms: build_rooms_section(top_rooms, &room_types, active_rooms_count)?,
        reactions: build_reactions_section(top_emojis, top_messages, total_reactions)?,
        created_rooms: build_created_rooms_section(&created_rooms)?,
        fun: None, // TODO: Implement fun stats later
    };

    Ok(stats)
}

// ============================================================================
// Helper Functions for Building Sections
// ============================================================================

/// Builds the Activity section of stats from temporal aggregates (private).
fn build_activity_section(
    temporal: TemporalAggregates,
    messages_sent: i32,
) -> Result<Option<Activity>> {
    if messages_sent == 0 {
        return Ok(None);
    }

    Ok(Some(Activity {
        by_year: if !temporal.by_year.is_empty() {
            Some(temporal.by_year)
        } else {
            None
        },
        by_month: if !temporal.by_month.is_empty() {
            Some(temporal.by_month)
        } else {
            None
        },
        by_week: if !temporal.by_week.is_empty() {
            Some(temporal.by_week)
        } else {
            None
        },
        by_weekday: if !temporal.by_weekday.is_empty() {
            Some(temporal.by_weekday)
        } else {
            None
        },
        by_day: if !temporal.by_day.is_empty() {
            Some(temporal.by_day)
        } else {
            None
        },
        by_hour: if !temporal.by_hour.is_empty() {
            Some(temporal.by_hour)
        } else {
            None
        },
    }))
}

/// Builds the Rooms section of stats (private).
fn build_rooms_section(
    top_rooms: Vec<RoomEntry>,
    room_types: &RoomTypeMetrics,
    active_rooms_count: i32,
) -> Result<Option<Rooms>> {
    if active_rooms_count == 0 {
        return Ok(None);
    }

    Ok(Some(Rooms {
        total: active_rooms_count,
        top: if !top_rooms.is_empty() {
            Some(top_rooms)
        } else {
            None
        },
        messages_by_room_type: Some(MessagesByRoomType {
            dm: if room_types.dm_messages > 0 {
                Some(room_types.dm_messages)
            } else {
                None
            },
            public: if room_types.public_messages > 0 {
                Some(room_types.public_messages)
            } else {
                None
            },
            private: if room_types.private_messages > 0 {
                Some(room_types.private_messages)
            } else {
                None
            },
        }),
    }))
}

/// Builds the Reactions section of stats (private).
fn build_reactions_section(
    top_emojis: Vec<EmojiEntry>,
    top_messages: Vec<MessageReactionEntry>,
    total_reactions: i32,
) -> Result<Option<Reactions>> {
    if total_reactions == 0 {
        return Ok(None);
    }

    Ok(Some(Reactions {
        total: Some(total_reactions),
        top_emojis: if !top_emojis.is_empty() {
            Some(top_emojis)
        } else {
            None
        },
        top_messages: if !top_messages.is_empty() {
            Some(top_messages)
        } else {
            None
        },
    }))
}

/// Builds the CreatedRooms section of stats (private).
fn build_created_rooms_section(created_rooms: &CreatedRoomMetrics) -> Result<Option<CreatedRooms>> {
    if created_rooms.total == 0 {
        return Ok(None);
    }

    Ok(Some(CreatedRooms {
        total: created_rooms.total,
        dm_rooms: if created_rooms.dm > 0 {
            Some(created_rooms.dm)
        } else {
            None
        },
        public_rooms: if created_rooms.public > 0 {
            Some(created_rooms.public)
        } else {
            None
        },
        private_rooms: if created_rooms.private > 0 {
            Some(created_rooms.private)
        } else {
            None
        },
    }))
}

// ============================================================================
// Helper Functions for Ranking
// ============================================================================

/// Ranks top rooms by message count (private).
fn rank_top_rooms(
    room_message_counts: &mut [(String, Option<String>, RoomType, i32)],
    messages_sent: i32,
) -> Result<Vec<RoomEntry>> {
    room_message_counts.sort_by(|a, b| b.3.cmp(&a.3));

    Ok(room_message_counts
        .iter()
        .take(5)
        .map(|(room_id, room_name, _room_type, count)| {
            let percentage = if messages_sent > 0 {
                Some((*count as f64 / messages_sent as f64) * 100.0)
            } else {
                None
            };

            RoomEntry {
                name: room_name.clone(),
                messages: *count,
                percentage,
                permalink: format!("https://matrix.to/#/{}", room_id),
            }
        })
        .collect())
}

/// Ranks top emojis by reaction count (private).
fn rank_top_emojis(emojis: HashMap<String, i32>) -> Result<Vec<EmojiEntry>> {
    let mut emoji_vec: Vec<_> = emojis.into_iter().collect();
    emoji_vec.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(emoji_vec
        .into_iter()
        .take(5)
        .map(|(emoji, count)| EmojiEntry { emoji, count })
        .collect())
}

/// Ranks top messages by reaction count (private).
fn rank_top_messages(messages: HashMap<String, i32>) -> Result<Vec<MessageReactionEntry>> {
    let mut message_vec: Vec<_> = messages.into_iter().collect();
    message_vec.sort_by(|a, b| b.1.cmp(&a.1));

    Ok(message_vec
        .into_iter()
        .take(5)
        .map(|(event_id, count)| MessageReactionEntry {
            permalink: format!("https://matrix.to/#/{}", event_id),
            reaction_count: count,
        })
        .collect())
}

/// Computes coverage bounds from timestamps and window scope (private).
fn compute_coverage_bounds(
    coverage: &CoverageBounds,
    window_scope: &WindowScope,
) -> Result<(String, String, Option<i32>)> {
    let (coverage_from, coverage_to) =
        if let (Some(oldest), Some(newest)) = (coverage.oldest_ts, coverage.newest_ts) {
            use chrono::{Local, TimeZone};
            let from_dt = Local.timestamp_millis_opt(oldest).single();
            let to_dt = Local.timestamp_millis_opt(newest).single();

            (
                from_dt
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| window_scope.from.format("%Y-%m-%d").to_string()),
                to_dt
                    .map(|dt| dt.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| window_scope.to.format("%Y-%m-%d").to_string()),
            )
        } else {
            (
                window_scope.from.format("%Y-%m-%d").to_string(),
                window_scope.to.format("%Y-%m-%d").to_string(),
            )
        };

    let days_active = if !coverage.active_dates.is_empty() {
        Some(coverage.active_dates.len() as i32)
    } else {
        None
    };

    Ok((coverage_from, coverage_to, days_active))
}

/// Computes peak activity periods from temporal buckets.
fn compute_peaks(
    by_year: &HashMap<String, i32>,
    by_month: &HashMap<String, i32>,
    by_week: &HashMap<String, i32>,
    by_day: &HashMap<String, i32>,
    by_hour: &HashMap<String, i32>,
) -> Result<Option<Peaks>> {
    let peak_year = by_year
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(year, &messages)| PeakYear {
            year: year.clone(),
            messages,
        });

    let peak_month = by_month
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(month, &messages)| PeakMonth {
            month: month.clone(),
            messages,
        });

    let peak_week = by_week
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(week, &messages)| PeakWeek {
            week: week.clone(),
            messages,
        });

    let peak_day = by_day
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(day, &messages)| PeakDay {
            day: day.clone(),
            messages,
        });

    let peak_hour = by_hour
        .iter()
        .max_by_key(|(_, &count)| count)
        .map(|(hour, &messages)| PeakHour {
            hour: hour.clone(),
            messages,
            date: None,
        });

    if peak_year.is_none()
        && peak_month.is_none()
        && peak_week.is_none()
        && peak_day.is_none()
        && peak_hour.is_none()
    {
        return Ok(None);
    }

    Ok(Some(Peaks {
        year: peak_year,
        month: peak_month,
        week: peak_week,
        day: peak_day,
        hour: peak_hour,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::ScopeKind;

    fn create_test_window_scope() -> WindowScope {
        WindowScope {
            scope_type: ScopeKind::Year,
            key: "2025".to_string(),
            from: chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            to: chrono::NaiveDate::from_ymd_opt(2025, 12, 31).unwrap(),
        }
    }

    fn create_test_room_stats() -> DetailedPaginationStats {
        let mut by_year = HashMap::new();
        by_year.insert("2025".to_string(), 10);

        let mut by_month = HashMap::new();
        by_month.insert("01".to_string(), 5);
        by_month.insert("02".to_string(), 5);

        let mut by_week = HashMap::new();
        by_week.insert("2025-W01".to_string(), 3);
        by_week.insert("2025-W02".to_string(), 7);

        let mut by_weekday = HashMap::new();
        by_weekday.insert("1".to_string(), 4);
        by_weekday.insert("2".to_string(), 6);

        let mut by_day = HashMap::new();
        by_day.insert("2025-01-15".to_string(), 5);
        by_day.insert("2025-02-20".to_string(), 5);

        let mut by_hour = HashMap::new();
        by_hour.insert("09".to_string(), 3);
        by_hour.insert("14".to_string(), 7);

        let mut active_dates = HashMap::new();
        active_dates.insert("2025-01-15".to_string(), true);
        active_dates.insert("2025-02-20".to_string(), true);

        DetailedPaginationStats {
            fully_crawled: true,
            oldest_event_id: Some("$oldest".to_string()),
            oldest_ts: Some(1735689600000), // 2024-12-31 23:00:00 UTC (2025-01-01 in some timezones)
            newest_event_id: Some("$newest".to_string()),
            newest_ts: Some(1767225599999), // 2025-12-31 23:59:59.999 UTC
            total_events: 20,
            user_events: 10,
            by_year,
            by_month,
            by_week,
            by_weekday,
            by_day,
            by_hour,
            user_message_ids: HashMap::new(),
            reactions_by_emoji: HashMap::new(),
            reactions_by_message: HashMap::new(),
            room_created_by_user: false,
            active_dates,
        }
    }

    #[test]
    fn test_build_stats_single_room() {
        let room_stats = create_test_room_stats();
        let room_input = RoomStatsInput {
            room_id: "!room1:example.org".to_string(),
            room_name: Some("Test Room".to_string()),
            room_type: RoomType::Private,
            stats: room_stats,
        };

        let window_scope = create_test_window_scope();

        let stats = build_stats(
            vec![room_input],
            "@user:example.org",
            Some("Test User".to_string()),
            None,
            &window_scope,
            5,
        )
        .unwrap();

        assert_eq!(stats.summary.messages_sent, 10);
        assert_eq!(stats.summary.active_rooms, 1);
        assert_eq!(stats.summary.private_rooms, Some(1));
        assert_eq!(stats.account.rooms_total, 5);
        assert_eq!(stats.coverage.days_active, Some(2));

        // Check activity data exists
        assert!(stats.activity.is_some());
        let activity = stats.activity.unwrap();
        assert!(activity.by_year.is_some());
        assert!(activity.by_month.is_some());

        // Check rooms data
        assert!(stats.rooms.is_some());
        let rooms = stats.rooms.unwrap();
        assert_eq!(rooms.total, 1);
        assert!(rooms.top.is_some());
        assert_eq!(rooms.top.unwrap().len(), 1);
    }

    #[test]
    fn test_build_stats_multiple_rooms() {
        let mut room1_stats = create_test_room_stats();
        room1_stats.user_events = 15;

        let mut room2_stats = create_test_room_stats();
        room2_stats.user_events = 25;

        let room1 = RoomStatsInput {
            room_id: "!room1:example.org".to_string(),
            room_name: Some("Room 1".to_string()),
            room_type: RoomType::Dm,
            stats: room1_stats,
        };

        let room2 = RoomStatsInput {
            room_id: "!room2:example.org".to_string(),
            room_name: Some("Room 2".to_string()),
            room_type: RoomType::Public,
            stats: room2_stats,
        };

        let window_scope = create_test_window_scope();

        let stats = build_stats(
            vec![room1, room2],
            "@user:example.org",
            None,
            None,
            &window_scope,
            10,
        )
        .unwrap();

        assert_eq!(stats.summary.messages_sent, 40);
        assert_eq!(stats.summary.active_rooms, 2);
        assert_eq!(stats.summary.dm_rooms, Some(1));
        assert_eq!(stats.summary.public_rooms, Some(1));

        // Check room type distribution
        let rooms = stats.rooms.unwrap();
        let room_type_dist = rooms.messages_by_room_type.unwrap();
        assert_eq!(room_type_dist.dm, Some(15));
        assert_eq!(room_type_dist.public, Some(25));
    }

    #[test]
    fn test_build_stats_room_creation() {
        let mut room_stats = create_test_room_stats();
        room_stats.room_created_by_user = true;

        let room_input = RoomStatsInput {
            room_id: "!room1:example.org".to_string(),
            room_name: Some("Created Room".to_string()),
            room_type: RoomType::Dm,
            stats: room_stats,
        };

        let window_scope = create_test_window_scope();

        let stats = build_stats(
            vec![room_input],
            "@user:example.org",
            None,
            None,
            &window_scope,
            1,
        )
        .unwrap();

        assert!(stats.created_rooms.is_some());
        let created = stats.created_rooms.unwrap();
        assert_eq!(created.total, 1);
        assert_eq!(created.dm_rooms, Some(1));
        assert_eq!(created.public_rooms, None);
        assert_eq!(created.private_rooms, None);
    }

    #[test]
    fn test_build_stats_with_reactions() {
        let mut room_stats = create_test_room_stats();

        let mut reactions_by_emoji = HashMap::new();
        reactions_by_emoji.insert("😂".to_string(), 10);
        reactions_by_emoji.insert("❤️".to_string(), 8);
        reactions_by_emoji.insert("👍".to_string(), 5);

        let mut reactions_by_message = HashMap::new();
        reactions_by_message.insert("$msg1".to_string(), 15);
        reactions_by_message.insert("$msg2".to_string(), 8);

        room_stats.reactions_by_emoji = reactions_by_emoji;
        room_stats.reactions_by_message = reactions_by_message;

        let room_input = RoomStatsInput {
            room_id: "!room1:example.org".to_string(),
            room_name: Some("Reaction Room".to_string()),
            room_type: RoomType::Private,
            stats: room_stats,
        };

        let window_scope = create_test_window_scope();

        let stats = build_stats(
            vec![room_input],
            "@user:example.org",
            None,
            None,
            &window_scope,
            1,
        )
        .unwrap();

        assert!(stats.reactions.is_some());
        let reactions = stats.reactions.unwrap();
        assert_eq!(reactions.total, Some(23));
        assert!(reactions.top_emojis.is_some());
        assert!(reactions.top_messages.is_some());

        let top_emojis = reactions.top_emojis.unwrap();
        assert_eq!(top_emojis.len(), 3);
        assert_eq!(top_emojis[0].emoji, "😂");
        assert_eq!(top_emojis[0].count, 10);
    }

    #[test]
    fn test_build_stats_empty_rooms() {
        let mut room_stats = create_test_room_stats();
        room_stats.user_events = 0;

        let room_input = RoomStatsInput {
            room_id: "!room1:example.org".to_string(),
            room_name: Some("Empty Room".to_string()),
            room_type: RoomType::Private,
            stats: room_stats,
        };

        let window_scope = create_test_window_scope();

        let stats = build_stats(
            vec![room_input],
            "@user:example.org",
            None,
            None,
            &window_scope,
            1,
        )
        .unwrap();

        assert_eq!(stats.summary.messages_sent, 0);
        assert_eq!(stats.summary.active_rooms, 0);
        assert!(stats.activity.is_none());
        assert!(stats.rooms.is_none());
    }

    #[test]
    fn test_compute_peaks() {
        let mut by_year = HashMap::new();
        by_year.insert("2024".to_string(), 100);
        by_year.insert("2025".to_string(), 150);

        let mut by_month = HashMap::new();
        by_month.insert("01".to_string(), 50);
        by_month.insert("03".to_string(), 75);

        let mut by_week = HashMap::new();
        by_week.insert("2025-W10".to_string(), 30);
        by_week.insert("2025-W15".to_string(), 45);

        let mut by_day = HashMap::new();
        by_day.insert("2025-03-15".to_string(), 25);
        by_day.insert("2025-03-20".to_string(), 30);

        let mut by_hour = HashMap::new();
        by_hour.insert("09".to_string(), 10);
        by_hour.insert("14".to_string(), 20);

        let peaks = compute_peaks(&by_year, &by_month, &by_week, &by_day, &by_hour)
            .unwrap()
            .unwrap();

        assert_eq!(peaks.year.as_ref().unwrap().year, "2025");
        assert_eq!(peaks.year.as_ref().unwrap().messages, 150);

        assert_eq!(peaks.month.as_ref().unwrap().month, "03");
        assert_eq!(peaks.month.as_ref().unwrap().messages, 75);

        assert_eq!(peaks.week.as_ref().unwrap().week, "2025-W15");
        assert_eq!(peaks.week.as_ref().unwrap().messages, 45);

        assert_eq!(peaks.day.as_ref().unwrap().day, "2025-03-20");
        assert_eq!(peaks.day.as_ref().unwrap().messages, 30);

        assert_eq!(peaks.hour.as_ref().unwrap().hour, "14");
        assert_eq!(peaks.hour.as_ref().unwrap().messages, 20);
    }

    #[test]
    fn test_compute_peaks_empty() {
        let by_year = HashMap::new();
        let by_month = HashMap::new();
        let by_week = HashMap::new();
        let by_day = HashMap::new();
        let by_hour = HashMap::new();

        let peaks = compute_peaks(&by_year, &by_month, &by_week, &by_day, &by_hour).unwrap();
        assert!(peaks.is_none());
    }

    #[test]
    fn test_top_rooms_ranking() {
        let mut room1_stats = create_test_room_stats();
        room1_stats.user_events = 100;

        let mut room2_stats = create_test_room_stats();
        room2_stats.user_events = 200;

        let mut room3_stats = create_test_room_stats();
        room3_stats.user_events = 50;

        let rooms = vec![
            RoomStatsInput {
                room_id: "!room1:example.org".to_string(),
                room_name: Some("Room 1".to_string()),
                room_type: RoomType::Private,
                stats: room1_stats,
            },
            RoomStatsInput {
                room_id: "!room2:example.org".to_string(),
                room_name: Some("Room 2".to_string()),
                room_type: RoomType::Private,
                stats: room2_stats,
            },
            RoomStatsInput {
                room_id: "!room3:example.org".to_string(),
                room_name: Some("Room 3".to_string()),
                room_type: RoomType::Private,
                stats: room3_stats,
            },
        ];

        let window_scope = create_test_window_scope();

        let stats = build_stats(rooms, "@user:example.org", None, None, &window_scope, 3).unwrap();

        let top_rooms = stats.rooms.unwrap().top.unwrap();
        assert_eq!(top_rooms.len(), 3);

        // Should be sorted by message count descending
        assert_eq!(top_rooms[0].name, Some("Room 2".to_string()));
        assert_eq!(top_rooms[0].messages, 200);

        assert_eq!(top_rooms[1].name, Some("Room 1".to_string()));
        assert_eq!(top_rooms[1].messages, 100);

        assert_eq!(top_rooms[2].name, Some("Room 3".to_string()));
        assert_eq!(top_rooms[2].messages, 50);
    }
}
