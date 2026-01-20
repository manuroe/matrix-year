//! Data structures for the crawl module.

use std::collections::HashMap;

/// Represents the join state of a room.
#[derive(Clone, Debug)]
pub enum RoomJoinState {
    Joined,
    #[allow(dead_code)]
    Left,
    #[allow(dead_code)]
    Invited,
    #[allow(dead_code)]
    JoinedSpace,
}

/// Metadata about a room discovered during room list sync.
///
/// Contains the latest event information needed for crawl decision-making
/// and freshness checks.
#[derive(Clone, Debug)]
pub struct RoomInfo {
    pub room_id: String,
    pub last_event_id: Option<String>,
    pub last_event_ts: Option<i64>,
    pub join_state: RoomJoinState,
}

/// Statistics collected while crawling a single room's events.
///
/// Aggregates information from backward pagination to track event distribution,
/// timestamp bounds, and user message count. Used to update the crawl metadata
/// database after pagination completes.
#[derive(Debug)]
pub struct RoomCrawlStats {
    pub room_id: String,
    pub oldest_event_id: Option<String>,
    pub oldest_ts: Option<i64>,
    pub newest_event_id: Option<String>,
    pub newest_ts: Option<i64>,
    pub fully_crawled: bool,
    pub room_name: String,
    pub total_events: usize,
    pub user_events: usize,
}

/// Detailed statistics collected during pagination for stats generation.
///
/// Extends basic pagination aggregates with temporal bucketing, reaction tracking,
/// and room creation detection. All data is aggregated in-memory during event iteration.
pub struct DetailedPaginationStats {
    // Basic metadata (same as PaginationAggregates)
    pub fully_crawled: bool,
    pub oldest_event_id: Option<String>,
    pub oldest_ts: Option<i64>,
    pub newest_event_id: Option<String>,
    pub newest_ts: Option<i64>,
    pub total_events: usize,
    pub user_events: usize,

    // Temporal buckets (local timezone)
    pub by_year: HashMap<String, i32>,
    pub by_month: HashMap<String, i32>,
    pub by_week: HashMap<String, i32>,
    pub by_weekday: HashMap<String, i32>,
    pub by_day: HashMap<String, i32>,
    pub by_hour: HashMap<String, i32>,

    // User's message IDs (for filtering reactions)
    pub user_message_ids: HashMap<String, String>, // event_id -> room_id

    // Reactions tracking
    pub reactions_by_emoji: HashMap<String, i32>,
    pub reactions_by_message: HashMap<String, i32>, // event_id -> count

    // Room creation tracking
    pub room_created_by_user: bool,

    // Track unique dates for days_active calculation
    pub active_dates: HashMap<String, bool>, // YYYY-MM-DD -> true
}
