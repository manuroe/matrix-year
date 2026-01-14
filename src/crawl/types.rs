/// Data structures for the crawl module.
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

/// Aggregated statistics from the pagination loop.
///
/// Internal structure used by the pagination logic to track event metrics
/// as events are fetched in batches. Later converted to `RoomCrawlStats`.
pub struct PaginationAggregates {
    pub fully_crawled: bool,
    pub oldest_event_id: Option<String>,
    pub oldest_ts: Option<i64>,
    pub newest_event_id: Option<String>,
    pub newest_ts: Option<i64>,
    pub total_events: usize,
    pub user_events: usize,
}
