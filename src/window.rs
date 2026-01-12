/// Window parsing module for temporal scope handling
///
/// Parses window strings (e.g., '2025', '2025-03', '2025-W12', '2025-03-15', 'life')
/// into temporal boundaries for crawling and stats generation.
use anyhow::{anyhow, Result};
use chrono::{Datelike, Local, NaiveDate};

/// Represents a parsed temporal window with date range
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WindowScope {
    /// Window key as provided by user (e.g., "2025", "2025-03", "2025-W12", "2025-03-15", "life")
    pub key: String,
    /// Scope type (year, month, week, day, life)
    pub scope_type: crate::stats::ScopeKind,
    /// Start date (inclusive, UTC)
    pub from: NaiveDate,
    /// End date (inclusive, UTC)
    pub to: NaiveDate,
}

impl WindowScope {
    /// Parse a window string into a WindowScope
    ///
    /// Supported formats:
    /// - "2025" → entire year 2025
    /// - "2025-03" → March 2025
    /// - "2025-W12" → ISO week 12 of 2025
    /// - "2025-03-15" → specific day
    /// - "life" → from epoch to today
    pub fn parse(window: &str) -> Result<Self> {
        let window = window.trim();

        if window == "life" {
            let today = Local::now().naive_utc().date();
            return Ok(WindowScope {
                key: "life".to_string(),
                scope_type: crate::stats::ScopeKind::Life,
                from: NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
                to: today,
            });
        }

        // Try year: "YYYY"
        if let Ok(year) = window.parse::<i32>() {
            if (1970..=2099).contains(&year) {
                let from = NaiveDate::from_ymd_opt(year, 1, 1)
                    .ok_or_else(|| anyhow!("Invalid year: {}", year))?;
                let to = NaiveDate::from_ymd_opt(year, 12, 31)
                    .ok_or_else(|| anyhow!("Invalid year: {}", year))?;
                return Ok(WindowScope {
                    key: window.to_string(),
                    scope_type: crate::stats::ScopeKind::Year,
                    from,
                    to,
                });
            }
        }

        // Try month: "YYYY-MM"
        if let Some((year_str, month_str)) = window.split_once('-') {
            if let (Ok(year), Ok(month)) = (year_str.parse::<i32>(), month_str.parse::<u32>()) {
                if (1970..=2099).contains(&year) && (1..=12).contains(&month) {
                    let from = NaiveDate::from_ymd_opt(year, month, 1)
                        .ok_or_else(|| anyhow!("Invalid month: {}-{:02}", year, month))?;
                    // Last day of month
                    let to = if month == 12 {
                        NaiveDate::from_ymd_opt(year + 1, 1, 1)
                            .unwrap()
                            .pred_opt()
                            .unwrap()
                    } else {
                        NaiveDate::from_ymd_opt(year, month + 1, 1)
                            .unwrap()
                            .pred_opt()
                            .unwrap()
                    };
                    return Ok(WindowScope {
                        key: window.to_string(),
                        scope_type: crate::stats::ScopeKind::Month,
                        from,
                        to,
                    });
                }
            }
        }

        // Try week: "YYYY-WXX"
        if let Some(pos) = window.find("-W") {
            let year_str = &window[..pos];
            let week_str = &window[pos + 2..];

            if let (Ok(year), Ok(week)) = (year_str.parse::<i32>(), week_str.parse::<u32>()) {
                if (1970..=2099).contains(&year) && (1..=53).contains(&week) {
                    // ISO week date: find the Monday of week 1 for the year
                    let jan_4 = NaiveDate::from_ymd_opt(year, 1, 4)
                        .ok_or_else(|| anyhow!("Invalid year: {}", year))?;
                    let week_1_monday = jan_4
                        - chrono::Duration::days(jan_4.weekday().number_from_monday() as i64 - 1);

                    let from = week_1_monday + chrono::Duration::days((week as i64 - 1) * 7);
                    let to = from + chrono::Duration::days(6);

                    // Validate that the calculated dates are actually in the requested year
                    if from.year() != year && to.year() != year {
                        return Err(anyhow!("Invalid week for year: {}-W{:02}", year, week));
                    }

                    return Ok(WindowScope {
                        key: window.to_string(),
                        scope_type: crate::stats::ScopeKind::Week,
                        from,
                        to,
                    });
                }
            }
        }

        // Try day: "YYYY-MM-DD"
        if let Ok(date) = NaiveDate::parse_from_str(window, "%Y-%m-%d") {
            return Ok(WindowScope {
                key: window.to_string(),
                scope_type: crate::stats::ScopeKind::Day,
                from: date,
                to: date,
            });
        }

        Err(anyhow!(
            "Invalid window format: '{}'. Expected: '2025', '2025-03', '2025-W12', '2025-03-15', or 'life'",
            window
        ))
    }

    /// Check if this window includes today's date
    #[allow(dead_code)]
    pub fn covers_now(&self) -> bool {
        let today = Local::now().naive_utc().date();
        self.from <= today && today <= self.to
    }

    /// Convert window to Unix timestamp range in milliseconds
    ///
    /// Returns (start_ts, end_ts) where:
    /// - start_ts is None for "life" scope (beginning of time), otherwise midnight UTC of from date
    /// - end_ts is end of day UTC (23:59:59.999) of to date
    #[allow(clippy::type_complexity)]
    pub fn to_timestamp_range(&self) -> (Option<i64>, i64) {
        let start_ts = if self.scope_type == crate::stats::ScopeKind::Life {
            None
        } else {
            Some(
                self.from
                    .and_hms_opt(0, 0, 0)
                    .unwrap()
                    .and_utc()
                    .timestamp_millis(),
            )
        };

        let end_ts = self
            .to
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc()
            .timestamp_millis()
            + 999; // Add milliseconds to get end of day

        (start_ts, end_ts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_year() {
        let ws = WindowScope::parse("2025").unwrap();
        assert_eq!(ws.key, "2025");
        assert_eq!(ws.scope_type, crate::stats::ScopeKind::Year);
        assert_eq!(ws.from, NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        assert_eq!(ws.to, NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn test_parse_month() {
        let ws = WindowScope::parse("2025-03").unwrap();
        assert_eq!(ws.key, "2025-03");
        assert_eq!(ws.scope_type, crate::stats::ScopeKind::Month);
        assert_eq!(ws.from, NaiveDate::from_ymd_opt(2025, 3, 1).unwrap());
        assert_eq!(ws.to, NaiveDate::from_ymd_opt(2025, 3, 31).unwrap());
    }

    #[test]
    fn test_parse_month_december() {
        let ws = WindowScope::parse("2025-12").unwrap();
        assert_eq!(ws.from, NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());
        assert_eq!(ws.to, NaiveDate::from_ymd_opt(2025, 12, 31).unwrap());
    }

    #[test]
    fn test_parse_day() {
        let ws = WindowScope::parse("2025-03-15").unwrap();
        assert_eq!(ws.key, "2025-03-15");
        assert_eq!(ws.scope_type, crate::stats::ScopeKind::Day);
        assert_eq!(ws.from, NaiveDate::from_ymd_opt(2025, 3, 15).unwrap());
        assert_eq!(ws.to, NaiveDate::from_ymd_opt(2025, 3, 15).unwrap());
    }

    #[test]
    fn test_parse_week() {
        let ws = WindowScope::parse("2025-W12").unwrap();
        assert_eq!(ws.key, "2025-W12");
        assert_eq!(ws.scope_type, crate::stats::ScopeKind::Week);
        // Week 12 of 2025: Monday should be around March 17
        assert!(ws.from >= NaiveDate::from_ymd_opt(2025, 3, 17).unwrap());
        assert!(ws.to <= NaiveDate::from_ymd_opt(2025, 3, 24).unwrap());
    }

    #[test]
    fn test_parse_life() {
        let ws = WindowScope::parse("life").unwrap();
        assert_eq!(ws.key, "life");
        assert_eq!(ws.scope_type, crate::stats::ScopeKind::Life);
        assert_eq!(ws.from, NaiveDate::from_ymd_opt(1970, 1, 1).unwrap());
        // to should be today or recent
        assert!(ws.to >= NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
    }

    #[test]
    fn test_invalid_window() {
        assert!(WindowScope::parse("invalid").is_err());
        assert!(WindowScope::parse("2025-W99").is_err());
        assert!(WindowScope::parse("2025-13").is_err());
        assert!(WindowScope::parse("2025-02-30").is_err());
    }
}
