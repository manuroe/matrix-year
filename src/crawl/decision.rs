/// Core crawl decision logic.
///
/// Determines which rooms need pagination based on window bounds, prior crawl state,
/// and freshness information from the room list sync.
use anyhow::Result;

use crate::crawl_db;

/// Decides whether a given room should be crawled based on window coverage and metadata.
///
/// Returns `Ok(true)` if the room needs pagination, `Ok(false)` if it can be skipped.
///
/// # Decision Logic
///
/// For **virgin rooms** (no metadata): crawl if the latest event is in/after the window start.
///
/// For **known rooms**: crawl if:
/// - The old end of coverage (reaching window start or room creation) is incomplete, OR
/// - The new end (reaching window end) is incomplete
///
/// If the latest event from discovery exactly matches what's in the database, the new end
/// is considered complete and only the old end matters.
///
/// # Arguments
///
/// * `db` - Crawl metadata database
/// * `room_id` - Matrix room ID
/// * `window_start_ts` - Window start (None = beginning of time)
/// * `window_end_ts` - Window end timestamp
/// * `latest_event` - Latest event info from room list sync (event_id, timestamp)
pub fn should_crawl_room(
    db: &crawl_db::CrawlDb,
    room_id: &str,
    window_start_ts: Option<i64>,
    window_end_ts: i64,
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

/// Filters joined rooms to find which ones need crawling for the given window.
///
/// Iterates through all joined rooms, checking each against the crawl decision logic.
/// Rooms that fail the database lookup are skipped with an error message (not counted as failures).
///
/// # Arguments
///
/// * `joined_rooms` - All rooms the user is currently joined to
/// * `db` - Crawl metadata database
/// * `window_start_ts` - Window start (None = beginning of time)
/// * `window_end_ts` - Window end timestamp
/// * `latest_events` - Latest event info for each room from room list sync
pub fn select_rooms_to_crawl(
    joined_rooms: &[matrix_sdk::Room],
    db: &crawl_db::CrawlDb,
    window_start_ts: Option<i64>,
    window_end_ts: Option<i64>,
    latest_events: &std::collections::HashMap<String, (String, i64)>,
) -> Vec<matrix_sdk::Room> {
    let mut rooms = Vec::new();
    for room in joined_rooms.iter() {
        let room_id_str = room.room_id().to_string();
        let needs_crawl = match should_crawl_room(
            db,
            &room_id_str,
            window_start_ts,
            window_end_ts.expect("window_end_ts required"),
            latest_events.get(&room_id_str),
        ) {
            Ok(value) => value,
            Err(err) => {
                eprintln!(
                    "Error determining whether to crawl room {}: {}",
                    room_id_str, err
                );
                false
            }
        };
        if needs_crawl {
            rooms.push(room.clone());
        }
    }
    rooms
}

/// Records virgin rooms that were skipped as having no events in the target window.
///
/// For rooms that weren't selected for crawling but have event metadata from discovery,
/// we record them in the database to avoid re-checking them on subsequent crawl runs.
///
/// # Arguments
///
/// * `db` - Crawl metadata database
/// * `joined_rooms` - All rooms the user is joined to
/// * `rooms_to_crawl` - Rooms that were selected for crawling
/// * `latest_events` - Latest event info from room list sync
///
/// # Errors
///
/// Returns an error if database updates fail. This is treated as a hard error
/// since it indicates a database problem that should be surfaced.
pub fn record_skipped_virgin_rooms(
    db: &crawl_db::CrawlDb,
    joined_rooms: &[matrix_sdk::Room],
    rooms_to_crawl: &[matrix_sdk::Room],
    latest_events: &std::collections::HashMap<String, (String, i64)>,
) -> Result<()> {
    for room in joined_rooms {
        let room_id_str = room.room_id().to_string();
        if let Ok(None) = db.get_room_metadata(&room_id_str) {
            // Virgin room not selected for crawl
            if !rooms_to_crawl.iter().any(|r| r.room_id() == room.room_id()) {
                // This room was skipped, record that we've seen it
                if let Some((event_id, event_ts)) = latest_events.get(&room_id_str) {
                    db.update_room_metadata(
                        &room_id_str,
                        Some(event_id.clone()),
                        Some(*event_ts),
                        Some(event_id.clone()),
                        Some(*event_ts),
                        false,
                    )
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to record skipped virgin room {}: {}",
                            room_id_str,
                            e
                        )
                    })?;
                }
            }
        }
    }
    Ok(())
}

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
        let latest = ("evt1".to_owned(), 1_500);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(needs, "virgin room with events in window should be crawled");
        Ok(())
    }

    #[test]
    fn needs_crawl_when_latest_matches_but_old_end_not_covered() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest_evt".to_owned()),
            Some(1_500),
            Some("evt_match".to_owned()),
            Some(2_000),
            false,
        )?;

        let latest_from_server = ("evt_match".to_owned(), 2_000);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 3_000, Some(&latest_from_server))?;
        assert!(
            needs,
            "should still crawl to cover older messages even if newest matches"
        );
        Ok(())
    }

    #[test]
    fn skips_when_fully_crawled_and_window_end_covered() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest_ever".to_owned()),
            Some(100),
            Some("newest_evt".to_owned()),
            Some(3_000),
            true,
        )?;

        let latest = ("newest_evt".to_owned(), 3_000);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(!needs, "fully crawled with window end covered should skip");
        Ok(())
    }

    #[test]
    fn skips_virgin_room_when_latest_before_window_start() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        let latest = ("old_evt".to_owned(), 500);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(!needs, "virgin room with events before window should skip");
        Ok(())
    }

    #[test]
    fn crawls_with_window_start_none_when_not_fully_crawled() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest".to_owned()),
            Some(1_000),
            Some("newest".to_owned()),
            Some(2_000),
            false,
        )?;

        let latest = ("newest".to_owned(), 2_000);
        let needs = should_crawl_room(&db, "!room", None, 3_000, Some(&latest))?;
        assert!(needs, "window_start=None should crawl if not fully_crawled");
        Ok(())
    }

    #[test]
    fn skips_with_window_start_none_when_fully_crawled() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest_ever".to_owned()),
            Some(1),
            Some("newest".to_owned()),
            Some(2_000),
            true,
        )?;

        let latest = ("newest".to_owned(), 2_000);
        let needs = should_crawl_room(&db, "!room", None, 3_000, Some(&latest))?;
        assert!(!needs, "window_start=None should skip if fully_crawled");
        Ok(())
    }

    #[test]
    fn crawls_when_newest_ts_before_window_end() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("oldest".to_owned()),
            Some(500),
            Some("old_newest".to_owned()),
            Some(1_500),
            false,
        )?;

        let latest = ("newer_event".to_owned(), 1_750);
        let needs = should_crawl_room(&db, "!room", Some(1_000), 2_000, Some(&latest))?;
        assert!(needs, "should crawl when newest_ts < window_end");
        Ok(())
    }

    #[test]
    fn sequential_crawls_first_then_second_window() -> anyhow::Result<()> {
        let (db, _dir) = setup_db()?;
        db.update_room_metadata(
            "!room",
            Some("evt_2024_jan".to_owned()),
            Some(1_704_067_200_000),
            Some("evt_2024_dec".to_owned()),
            Some(1_735_689_599_999),
            false,
        )?;

        let window_2025_start = 1_735_689_600_000i64;
        let window_2025_end = 1_767_225_599_999i64;
        let latest_2025 = ("evt_2025_dec".to_owned(), window_2025_end);

        let needs = should_crawl_room(
            &db,
            "!room",
            Some(window_2025_start),
            window_2025_end,
            Some(&latest_2025),
        )?;
        assert!(needs, "should crawl 2025 window even after crawling 2024");
        Ok(())
    }
}
