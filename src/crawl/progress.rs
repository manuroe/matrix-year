/// Progress reporting UI for crawl operations.
///
/// Handles progress bar creation, updates, and result display.
/// Can operate in TTY mode (with animated spinners) or non-TTY mode (text logging).
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::io::IsTerminal;

use crate::timefmt::format_timestamp_opt;

/// Maximum width for room names in progress display.
const ROOM_NAME_WIDTH: usize = 38;

/// Type alias for progress callback function.
/// Called with (room_name, oldest_ts, newest_ts, total_events).
pub type ProgressCallback = Box<dyn Fn(&str, Option<i64>, Option<i64>, usize)>;

/// Truncates a string to a maximum width with middle ellipsis if needed.
/// Preserves the start and end of the string for better readability.
///
/// # Examples
/// - "Short name" (10 chars) → "Short name" (unchanged)
/// - "Very long room name that exceeds limit" (40 chars, limit 20) → "Very long...eds limit" (20 chars)
fn truncate_middle(s: &str, max_width: usize) -> String {
    // Use character count, not byte length, because display width is measured in characters.
    // Room names may contain emoji or other multi-byte Unicode, where .len() counts bytes
    // but visual width is counted in characters (and format!("{:<width$}") aligns by characters).
    let char_count = s.chars().count();
    if char_count <= max_width {
        // Pad to max_width for alignment
        format!("{:<width$}", s, width = max_width)
    } else {
        // Truncate with middle ellipsis
        let ellipsis = "…";
        // Ellipsis is a single Unicode character, not 3 bytes.
        let ellipsis_len = ellipsis.chars().count();
        if max_width <= ellipsis_len {
            // Edge case: max_width too small, just truncate end
            let truncated: String = s.chars().take(max_width).collect();
            format!("{:<width$}", truncated, width = max_width)
        } else {
            let available = max_width - ellipsis_len;
            let start_len = available.div_ceil(2);
            let end_len = available / 2;
            // Collect characters instead of slicing bytes: s.chars().take/skip creates
            // valid UTF-8 strings, whereas byte slicing with &s[..n] can panic on multi-byte boundaries.
            let start: String = s.chars().take(start_len).collect();
            let end: String = s.chars().skip(char_count - end_len).collect();
            format!("{}{}{}", start, ellipsis, end)
        }
    }
}

/// Formats a completed room result on a single line.
///
/// Example output:
/// `Element iOS (OLD)                        12329 events from 2024-12-29 02:27 (5 from you) 💯`
pub fn format_completed_room(
    room_name: &str,
    total_events: usize,
    user_events: usize,
    oldest_ts: Option<i64>,
    newest_ts: Option<i64>,
    fully_crawled: bool,
) -> String {
    let truncated_name = truncate_middle(room_name, ROOM_NAME_WIDTH);
    let creation_marker = if fully_crawled { " 💯" } else { "" };

    if let (Some(oldest), _) = (oldest_ts, newest_ts) {
        // Format timestamp and truncate to minute precision
        let oldest_full = format_timestamp_opt(Some(oldest));
        let oldest_short = if oldest_full.len() >= 16 {
            &oldest_full[..16]
        } else {
            &oldest_full
        };

        let user_events_str = if user_events > 0 {
            format!(" ({} from you)", user_events)
        } else {
            String::new()
        };

        format!(
            "{} {:>5} events from {}{}{}",
            truncated_name, total_events, oldest_short, user_events_str, creation_marker
        )
    } else {
        truncated_name.to_string()
    }
}

/// Progress tracking for the entire crawl operation.
///
/// Manages the overall progress bar and creates room-level progress bars.
/// Handles both TTY (animated) and non-TTY (text) modes transparently.
#[derive(Clone)]
pub struct CrawlProgress {
    multi: Option<MultiProgress>,
    overall: Option<ProgressBar>,
    is_tty: bool,
}

impl CrawlProgress {
    /// Creates progress bars for a crawl operation.
    ///
    /// If the output is a TTY, creates animated progress bars.
    /// Otherwise, progress is reported via text output only.
    pub fn new(total_rooms: usize) -> Self {
        let is_tty = std::io::stderr().is_terminal();

        if is_tty {
            let mp = MultiProgress::new();
            let overall_style = ProgressStyle::default_bar()
                .template("[{bar:40.cyan/blue}] {pos}/{len} rooms ({percent}%)")
                .unwrap()
                .progress_chars("█▓░");
            let overall = mp.add(ProgressBar::new(total_rooms as u64));
            overall.set_style(overall_style);
            CrawlProgress {
                multi: Some(mp),
                overall: Some(overall),
                is_tty: true,
            }
        } else {
            CrawlProgress {
                multi: None,
                overall: None,
                is_tty: false,
            }
        }
    }

    /// Creates a progress callback for a single room's pagination.
    ///
    /// Returns a tuple of (callback, optional_spinner).
    /// The callback updates progress as events are paginated.
    /// The spinner (if present) should be finished when the room completes.
    pub fn make_callback(&self, room_name: String) -> (ProgressCallback, Option<ProgressBar>) {
        let multi = self.multi.clone();
        let overall = self.overall.clone();

        if let Some(ref mp) = multi {
            let style = ProgressStyle::default_spinner()
                .template("  {spinner:.green} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");
            let pb = if let Some(ref overall_bar) = overall {
                mp.insert_before(overall_bar, ProgressBar::new_spinner())
            } else {
                mp.add(ProgressBar::new_spinner())
            };
            pb.set_style(style);
            pb.set_message(room_name.clone());
            pb.enable_steady_tick(std::time::Duration::from_millis(100));

            let pb_for_cb = pb.clone();
            let room_name_for_cb = room_name.clone();
            let callback = Box::new(
                move |_name: &str, oldest: Option<i64>, _newest: Option<i64>, events: usize| {
                    let truncated_name: String =
                        truncate_middle(&room_name_for_cb, ROOM_NAME_WIDTH);
                    let msg = if let Some(ts) = oldest {
                        // Format timestamp and truncate to minute precision (remove seconds)
                        let full_timestamp = format_timestamp_opt(Some(ts));
                        let timestamp = if full_timestamp.len() >= 16 {
                            &full_timestamp[..16] // "2025-03-26 15:20"
                        } else {
                            &full_timestamp
                        };
                        format!("{} {:>5} events from {}", truncated_name, events, timestamp)
                    } else {
                        format!("{} {:>5} events", truncated_name, events)
                    };
                    pb_for_cb.set_message(msg);
                },
            );
            (callback, Some(pb))
        } else {
            // Non-TTY mode: no-op callback
            let callback = Box::new(
                |_name: &str, _oldest: Option<i64>, _newest: Option<i64>, _events: usize| {},
            );
            (callback, None)
        }
    }

    /// Increments the overall progress bar.
    pub fn inc(&self) {
        if let Some(ref overall) = self.overall {
            overall.inc(1);
        }
    }

    /// Finishes and hides the overall progress bar.
    pub fn finish(&self) {
        if let Some(ref overall) = self.overall {
            overall.finish_and_clear();
        }
    }

    /// Print a line without breaking/redrawing the progress bars.
    /// Uses `MultiProgress::println` when available, otherwise falls back to `eprintln!`.
    pub fn println(&self, msg: &str) {
        if let Some(ref mp) = self.multi {
            // MultiProgress::println is safe to call from any thread and will
            // render the message above the progress bars without causing
            // duplicate lines.
            let _ = mp.println(msg);
        } else {
            eprintln!("{}", msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_creation() {
        let progress = CrawlProgress::new(5);
        assert_eq!(progress.is_tty, std::io::stderr().is_terminal());
    }

    #[test]
    fn test_callback_creation() {
        let progress = CrawlProgress::new(5);
        let _callback = progress.make_callback("Test Room".to_string());
        // Callback should be callable without panicking
    }

    #[test]
    fn test_truncate_middle_short() {
        let s = "Short name";
        let out = super::truncate_middle(s, 38);
        assert!(out.starts_with("Short name"));
        assert_eq!(out.len(), 38);
    }

    #[test]
    fn test_truncate_middle_long_unicode() {
        let s = "Very long room name 🚀 that exceeds limit";
        let out = super::truncate_middle(s, 20);
        assert!(out.contains('…'));
        assert_eq!(out.chars().count(), 20);
    }

    #[test]
    fn test_format_completed_room_basic() {
        let out =
            super::format_completed_room("Room", 123, 0, Some(1_735_689_600_000), None, false);
        assert!(out.starts_with("Room"));
        assert!(out.contains("123 events"));
        assert!(!out.contains("from you"));
    }

    #[test]
    fn test_format_completed_room_with_user_events_and_creation() {
        let out = super::format_completed_room("Room", 5, 2, Some(1_735_689_600_000), None, true);
        assert!(out.contains("(2 from you)"));
        assert!(out.contains("💯"));
    }
}
