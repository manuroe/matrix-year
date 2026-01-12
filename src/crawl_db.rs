/// Crawl metadata database
///
/// Tracks crawl progress per room to enable resumable and incremental crawling.
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

/// Represents crawl metadata for a single room
#[derive(Debug)]
#[allow(dead_code)]
pub struct RoomCrawlMetadata {
    pub room_id: String,
    pub oldest_event_id: Option<String>, // Event ID of the oldest message crawled
    pub oldest_event_ts: Option<i64>,    // Unix timestamp in milliseconds
    pub newest_event_id: Option<String>, // Event ID of the newest message crawled
    pub newest_event_ts: Option<i64>,    // Unix timestamp in milliseconds
    pub fully_crawled: bool,             // True if back-paginated to room creation
}

/// Database handle for crawl metadata operations
///
/// This abstracts the underlying database implementation (currently SQLite)
pub struct CrawlDb {
    conn: Connection,
}

impl CrawlDb {
    /// Initialize or open the crawl metadata database
    pub fn init(account_dir: &Path) -> Result<Self> {
        let db_path = account_dir.join("db.sqlite");
        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database at {}", db_path.display()))?;

        // Create schema if it doesn't exist
        conn.execute(
            "CREATE TABLE IF NOT EXISTS room_crawl_metadata (
                room_id TEXT NOT NULL PRIMARY KEY,
                oldest_event_id TEXT,
                oldest_event_ts INTEGER,
                newest_event_id TEXT,
                newest_event_ts INTEGER,
                fully_crawled INTEGER NOT NULL DEFAULT 0
            )",
            [],
        )
        .context("Failed to create room_crawl_metadata table")?;

        Ok(Self { conn })
    }

    /// Update room crawl metadata after successful pagination
    pub fn update_room_metadata(
        &self,
        room_id: &str,
        oldest_event_id: Option<String>,
        oldest_event_ts: Option<i64>,
        newest_event_id: Option<String>,
        newest_event_ts: Option<i64>,
        fully_crawled: bool,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO room_crawl_metadata (room_id, oldest_event_id, oldest_event_ts, newest_event_id, newest_event_ts, fully_crawled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(room_id) DO UPDATE SET
                oldest_event_id = CASE 
                    WHEN excluded.oldest_event_id IS NOT NULL THEN excluded.oldest_event_id
                    ELSE oldest_event_id
                END,
                oldest_event_ts = CASE
                    WHEN excluded.oldest_event_ts IS NOT NULL THEN
                        COALESCE(MIN(excluded.oldest_event_ts, oldest_event_ts), excluded.oldest_event_ts)
                    ELSE oldest_event_ts
                END,
                newest_event_id = CASE
                    WHEN excluded.newest_event_id IS NOT NULL THEN excluded.newest_event_id
                    ELSE newest_event_id
                END,
                newest_event_ts = CASE
                    WHEN excluded.newest_event_ts IS NOT NULL THEN
                        COALESCE(MAX(excluded.newest_event_ts, newest_event_ts), excluded.newest_event_ts)
                    ELSE newest_event_ts
                END,
                fully_crawled = fully_crawled OR excluded.fully_crawled",
            params![room_id, oldest_event_id, oldest_event_ts, newest_event_id, newest_event_ts, fully_crawled],
        )?;

        Ok(())
    }

    /// Get crawl metadata for a room
    #[allow(dead_code)]
    pub fn get_room_metadata(&self, room_id: &str) -> Result<Option<RoomCrawlMetadata>> {
        let mut stmt = self.conn.prepare(
            "SELECT room_id, oldest_event_id, oldest_event_ts, newest_event_id, newest_event_ts, fully_crawled
             FROM room_crawl_metadata
             WHERE room_id = ?1",
        )?;

        let result = stmt
            .query_row(params![room_id], |row| {
                Ok(RoomCrawlMetadata {
                    room_id: row.get(0)?,
                    oldest_event_id: row.get(1)?,
                    oldest_event_ts: row.get(2)?,
                    newest_event_id: row.get(3)?,
                    newest_event_ts: row.get(4)?,
                    fully_crawled: row.get(5)?,
                })
            })
            .optional()?;

        Ok(result)
    }

    /// Get the number of rooms with crawl metadata
    pub fn room_count(&self) -> Result<usize> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM room_crawl_metadata")?;
        let count: usize = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    /// Get the number of rooms that have been crawled back to creation
    pub fn fully_crawled_room_count(&self) -> Result<usize> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM room_crawl_metadata WHERE fully_crawled = 1")?;
        let count: usize = stmt.query_row([], |row| row.get(0))?;
        Ok(count)
    }

    /// Get the global time window available from crawled data
    ///
    /// Returns (window_start_ts, window_end_ts, account_creation_ts) in milliseconds since epoch.
    ///
    /// Window start logic:
    /// - If all rooms are fully_crawled, return None (account creation)
    /// - Otherwise, return the newest oldest_event_ts among non-fully-crawled rooms
    ///
    /// Window end: newest (latest) message across all rooms (MAX newest_event_ts)
    /// Account creation: oldest message across all rooms (MIN oldest_event_ts)
    #[allow(clippy::type_complexity)]
    pub fn get_time_window(&self) -> Result<Option<(Option<i64>, Option<i64>, Option<i64>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(*), SUM(CASE WHEN fully_crawled = 0 THEN 1 ELSE 0 END)
             FROM room_crawl_metadata",
        )?;
        let (total_rooms, non_fully_crawled): (usize, usize) = stmt.query_row([], |row| {
            Ok((row.get(0)?, row.get::<_, Option<usize>>(1)?.unwrap_or(0)))
        })?;

        if total_rooms == 0 {
            return Ok(None);
        }

        let window_start = if non_fully_crawled == 0 {
            // All rooms fully crawled: window starts at account creation (None)
            None
        } else {
            // Find newest oldest_event_ts among non-fully-crawled rooms
            let mut stmt = self.conn.prepare(
                "SELECT MAX(oldest_event_ts)
                 FROM room_crawl_metadata
                 WHERE fully_crawled = 0 AND oldest_event_ts IS NOT NULL",
            )?;
            stmt.query_row([], |row| row.get(0))?
        };

        // Window end: newest (latest) message across all rooms
        let mut stmt = self.conn.prepare(
            "SELECT MAX(newest_event_ts)
             FROM room_crawl_metadata
             WHERE newest_event_ts IS NOT NULL",
        )?;
        let window_end: Option<i64> = stmt.query_row([], |row| row.get(0))?;

        // Account creation: oldest message across all rooms
        let mut stmt = self.conn.prepare(
            "SELECT MIN(oldest_event_ts)
             FROM room_crawl_metadata
             WHERE oldest_event_ts IS NOT NULL",
        )?;
        let account_creation_ts: Option<i64> = stmt.query_row([], |row| row.get(0))?;

        Ok(Some((window_start, window_end, account_creation_ts)))
    }
}
