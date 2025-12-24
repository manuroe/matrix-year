use crate::stats::*;
use anyhow::Result;

/// Render stats to Markdown following md_report_layout.md
pub fn render(stats: &Stats) -> Result<String> {
    let mut output = String::new();

    // 1. Title, metadata, and account details
    render_header(&mut output, stats);

    // 2. Summary (including active days from coverage)
    render_summary(
        &mut output,
        &stats.summary,
        stats.coverage.days_active,
        &stats.scope,
    );

    // 3. Rooms
    if let Some(ref rooms) = stats.rooms {
        render_rooms(
            &mut output,
            rooms,
            stats.summary.messages_sent,
            &stats.scope,
        );
    }

    // 4. Created rooms
    if let Some(ref created_rooms) = stats.created_rooms {
        render_created_rooms(&mut output, created_rooms, &stats.scope);
    }

    // 5. Reactions
    if let Some(ref reactions) = stats.reactions {
        render_reactions(&mut output, reactions);
    }

    // 6. Activity
    if let Some(ref activity) = stats.activity {
        render_activity(&mut output, activity, &stats.scope, &stats.summary);
    }

    // 7. Fun
    if let Some(ref fun) = stats.fun {
        render_fun(&mut output, fun);
    }

    Ok(output)
}

fn render_header(output: &mut String, stats: &Stats) {
    let account = &stats.account;
    let scope_label = scope_label(&stats.scope);

    // Title with display name if available
    if let Some(ref display_name) = account.display_name {
        output.push_str(&format!(
            "# 🎉 Your Matrix {} — {}\n",
            scope_label, display_name
        ));
    } else {
        output.push_str(&format!("# 🎉 Your Matrix {}\n", scope_label));
    }

    // Account details
    output.push_str("### 🧑 Account\n");
    let user_permalink = format!("https://matrix.to/#/{}", account.user_id);
    output.push_str(&format!(
        "- **User ID:** [{}]({})\n",
        account.user_id, user_permalink
    ));
    if let Some(ref name) = account.display_name {
        output.push_str(&format!("- **Display name:** {}\n", name));
    }
    if let Some(ref avatar) = account.avatar_url {
        // Convert mxc:// URL to HTTPS media endpoint
        let avatar_https = if avatar.starts_with("mxc://") {
            let mxc_parts: Vec<&str> = avatar.strip_prefix("mxc://").unwrap().split('/').collect();
            if mxc_parts.len() >= 2 {
                format!(
                    "https://matrix.org/_matrix/media/r0/download/{}/{}",
                    mxc_parts[0], mxc_parts[1]
                )
            } else {
                avatar.clone()
            }
        } else {
            avatar.clone()
        };
        output.push_str(&format!(
            "- **Avatar:** [{}]({})\n",
            avatar_https, avatar_https
        ));
    }
    output.push_str(&format!(
        "- **Total joined rooms:** {}\n",
        account.rooms_total
    ));
    output.push('\n');
}

// Coverage section intentionally removed from rendering; active days are shown in Summary.

fn render_summary(output: &mut String, summary: &Summary, active_days: Option<i32>, scope: &Scope) {
    output.push_str("### 📊 Summary\n");
    output.push_str(&format!(
        "- 💬 **Messages sent:** {}\n",
        format_number(summary.messages_sent)
    ));
    if let Some(days) = active_days {
        output.push_str(&format!("- 🔥 **Active days:** {}\n", days));
    }

    if let Some(dm_rooms) = summary.dm_rooms {
        output.push_str(&format!("- 👥 **DM rooms:** {}\n", dm_rooms));
    }

    if let Some(public_rooms) = summary.public_rooms {
        output.push_str(&format!("- 🌐 **Public rooms:** {}\n", public_rooms));
    }

    if let Some(private_rooms) = summary.private_rooms {
        output.push_str(&format!("- 🔒 **Private rooms:** {}\n", private_rooms));
    }

    // Explicit note that the rest of the report refers to the given scope (skip for life)
    if !matches!(scope.kind, ScopeKind::Life) {
        output.push_str(&format!(
            "\n*All sections below refer to {}.*\n\n",
            scope_phrase(scope)
        ));
    } else {
        output.push('\n');
    }
}

fn render_peak_activity(output: &mut String, summary: &Summary) {
    let mut lines: Vec<String> = Vec::new();

    if let Some(peaks) = summary.peaks.as_ref() {
        if let Some(ref year) = peaks.year {
            lines.push(format!(
                "- 🗓️ **Peak year:** {} ({} messages)",
                year.year,
                format_number(year.messages)
            ));
        }

        if let Some(ref month) = peaks.month {
            lines.push(format!(
                "- 📆 **Peak month:** {} ({} messages)",
                month.month,
                format_number(month.messages)
            ));
        }

        if let Some(ref week) = peaks.week {
            lines.push(format!(
                "- 📅 **Peak week:** {} ({} messages)",
                week.week,
                format_number(week.messages)
            ));
        }

        if let Some(ref day) = peaks.day {
            lines.push(format!(
                "- 📍 **Peak day:** {} ({} messages)",
                day.day,
                format_number(day.messages)
            ));
        }

        if let Some(ref hour) = peaks.hour {
            let when = format!("{}:00 on {}", hour.hour, hour.date);

            lines.push(format!(
                "- 🕐 **Peak hour:** {} ({} messages)",
                when,
                format_number(hour.messages)
            ));
        }
    }

    if lines.is_empty() {
        return;
    }

    output.push_str("#### 🚀 Peaks\n");
    for line in lines {
        output.push_str(&line);
        output.push('\n');
    }
    output.push('\n');
}

fn render_activity(output: &mut String, activity: &Activity, scope: &Scope, summary: &Summary) {
    output.push_str("### 📈 Activity\n");

    // Peaks come first inside Activity
    render_peak_activity(output, summary);

    // By year (life scope)
    if let Some(ref by_year) = activity.by_year {
        output.push_str("#### 📆 By year\n");
        output.push_str("| Year | Messages |\n");
        output.push_str("| ---- | -------- |\n");

        let mut years: Vec<_> = by_year.keys().cloned().collect();
        years.sort();
        for year in years {
            let count = by_year.get(&year).copied().unwrap_or(0);
            output.push_str(&format!("| {} | {} |\n", year, format_number(count)));
        }
        output.push('\n');
    }

    // By month - only when meaningful for the scope (year/life)
    if matches!(scope.kind, ScopeKind::Year | ScopeKind::Life) {
        if let Some(ref by_month) = activity.by_month {
            output.push_str("#### 📆 By month\n");

            // January to June
            output.push_str("| Jan | Feb | Mar | Apr | May | Jun |\n");
            output.push_str("| --- | --- | --- | --- | --- | --- |\n");
            output.push('|');
            for month in 1..=6 {
                let month_key = format!("{:02}", month);
                let count = by_month.get(&month_key).copied().unwrap_or(0);
                output.push_str(&format!(" {} |", format_number(count)));
            }
            output.push('\n');

            // July to December
            output.push_str("\n| Jul | Aug | Sep | Oct | Nov | Dec |\n");
            output.push_str("| --- | --- | --- | --- | --- | --- |\n");
            output.push('|');
            for month in 7..=12 {
                let month_key = format!("{:02}", month);
                let count = by_month.get(&month_key).copied().unwrap_or(0);
                output.push_str(&format!(" {} |", format_number(count)));
            }
            output.push_str("\n\n");
        }
    }

    // By week (year scope)
    if matches!(scope.kind, ScopeKind::Year) {
        if let Some(ref by_week) = activity.by_week {
            output.push_str("#### 📅 By week\n");
            output.push_str("| Week | Messages |\n");
            output.push_str("| ---- | -------- |\n");

            let mut weeks: Vec<_> = by_week.keys().cloned().collect();
            weeks.sort();
            for week in weeks {
                let count = by_week.get(&week).copied().unwrap_or(0);
                output.push_str(&format!("| {} | {} |\n", week, format_number(count)));
            }
            output.push('\n');
        }
    }

    // By day (month scope)
    if matches!(scope.kind, ScopeKind::Month) {
        if let Some(ref by_day) = activity.by_day {
            output.push_str("#### 📅 By day\n");
            output.push_str(
                "| 01 | 02 | 03 | 04 | 05 | 06 | 07 | 08 | 09 | 10 | 11 | 12 | 13 | 14 | 15 |",
            );
            output.push('\n');
            output.push_str(
                "| -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- |",
            );
            output.push('\n');
            output.push('|');
            for day in 1..=15 {
                let key = format!("{:02}", day);
                let count = by_day.get(&key).copied().unwrap_or(0);
                output.push_str(&format!(" {} |", format_number(count)));
            }
            output.push('\n');

            output.push('\n');
            output.push_str(
                "| 16 | 17 | 18 | 19 | 20 | 21 | 22 | 23 | 24 | 25 | 26 | 27 | 28 | 29 | 30 | 31 |",
            );
            output.push('\n');
            output.push_str(
                "| -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- |",
            );
            output.push('\n');
            output.push('|');
            for day in 16..=31 {
                let key = format!("{:02}", day);
                let count = by_day.get(&key).copied().unwrap_or(0);
                output.push_str(&format!(" {} |", format_number(count)));
            }
            output.push_str("\n\n");
        }
    }

    // By weekday - horizontal display
    if let Some(ref by_weekday) = activity.by_weekday {
        output.push_str("#### 📅 By weekday\n");
        output.push_str("| Mon | Tue | Wed | Thu | Fri | Sat | Sun |\n");
        output.push_str("| --- | --- | --- | --- | --- | --- | --- |\n");

        output.push('|');
        let weekdays = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        for day in weekdays {
            let count = by_weekday.get(day).copied().unwrap_or(0);
            output.push_str(&format!(" {} |", format_number(count)));
        }
        output.push_str("\n\n");
    }

    // By hour - horizontal display in 2 tables (00-11 and 12-23)
    if let Some(ref by_hour) = activity.by_hour {
        output.push_str("#### 🕐 By hour (local time)\n");

        // Hours 00-11
        output.push_str("| 00 | 01 | 02 | 03 | 04 | 05 | 06 | 07 | 08 | 09 | 10 | 11 |\n");
        output.push_str("| -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- |\n");
        output.push('|');
        for hour in 0..12 {
            let hour_key = format!("{:02}", hour);
            let count = by_hour.get(&hour_key).copied().unwrap_or(0);
            output.push_str(&format!(" {} |", format_number(count)));
        }
        output.push('\n');

        // Hours 12-23
        output.push_str("\n| 12 | 13 | 14 | 15 | 16 | 17 | 18 | 19 | 20 | 21 | 22 | 23 |\n");
        output.push_str("| -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | -- |\n");
        output.push('|');
        for hour in 12..24 {
            let hour_key = format!("{:02}", hour);
            let count = by_hour.get(&hour_key).copied().unwrap_or(0);
            output.push_str(&format!(" {} |", format_number(count)));
        }
        output.push_str("\n\n");
    }
}

fn render_rooms(output: &mut String, rooms: &Rooms, messages_sent: i32, _scope: &Scope) {
    output.push_str("### 🏘️ Rooms\n");
    output.push_str(&format!(
        "You sent {} messages in **{}** rooms.\n\n",
        format_number(messages_sent),
        rooms.total
    ));

    if let Some(ref top) = rooms.top {
        if !top.is_empty() {
            output.push_str("Your most active rooms:\n\n");
            output.push_str("| Rank | Name | Messages | % of total |\n");
            output.push_str("| ---- | ---- | -------- | ---------- |\n");

            for (i, room) in top.iter().take(5).enumerate() {
                let rank = i + 1;
                let name = room.name.as_deref().unwrap_or("(unnamed room)");
                let percentage_str = if let Some(pct) = room.percentage {
                    format!("{:.1}", pct)
                } else {
                    String::from("-")
                };

                // Clickable room name with permalink
                let name_display = format!("[{}]({})", name, room.permalink);

                output.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    rank,
                    name_display,
                    format_number(room.messages),
                    percentage_str
                ));
            }
            output.push('\n');
        }
    }
}

fn render_reactions(output: &mut String, reactions: &Reactions) {
    output.push_str("### 😊 Reactions\n");

    if let Some(total) = reactions.total {
        output.push_str(&format!(
            "You made people smile with **{}** reactions on your messages!\n\n",
            format_number(total)
        ));
    }

    // Top emojis
    if let Some(ref top_emojis) = reactions.top_emojis {
        if !top_emojis.is_empty() {
            output.push_str("**Top reactions**\n\n");
            output.push_str("| Rank | Emoji | Count |\n");
            output.push_str("| ---- | ----- | ----- |\n");

            for (i, emoji_entry) in top_emojis.iter().take(5).enumerate() {
                let rank = i + 1;
                output.push_str(&format!(
                    "| {} | {} | {} |\n",
                    rank,
                    emoji_entry.emoji,
                    format_number(emoji_entry.count)
                ));
            }
            output.push('\n');
        }
    }

    // Top messages
    if let Some(ref top_messages) = reactions.top_messages {
        if !top_messages.is_empty() {
            output.push_str("**Most reacted messages**\n\n");
            output.push_str("| Rank | Link | Reactions |\n");
            output.push_str("| ---- | ---- | --------- |\n");

            for (i, msg_entry) in top_messages.iter().take(5).enumerate() {
                let rank = i + 1;
                output.push_str(&format!(
                    "| {} | [view]({}) | {} |\n",
                    rank,
                    msg_entry.permalink,
                    format_number(msg_entry.reaction_count)
                ));
            }
            output.push('\n');
        }
    }
}

fn render_created_rooms(output: &mut String, created_rooms: &CreatedRooms, scope: &Scope) {
    output.push_str("### 🏗️ Rooms You Created\n");

    // Add contextual sentence based on scope
    let scope_context = match scope.kind {
        ScopeKind::Year => "this year",
        ScopeKind::Month => "this month",
        ScopeKind::Week => "this week",
        ScopeKind::Day => "today",
        ScopeKind::Life => "in your lifetime",
    };
    output.push_str(&format!(
        "You created **{}** rooms {}.\n\n",
        format_number(created_rooms.total),
        scope_context
    ));

    if let Some(dm_rooms) = created_rooms.dm_rooms {
        output.push_str(&format!("- 👥 **DM rooms:** {}\n", format_number(dm_rooms)));
    }

    if let Some(public_rooms) = created_rooms.public_rooms {
        output.push_str(&format!(
            "- 🌐 **Public rooms:** {}\n",
            format_number(public_rooms)
        ));
    }

    if let Some(private_rooms) = created_rooms.private_rooms {
        output.push_str(&format!(
            "- 🔒 **Private rooms:** {}\n",
            format_number(private_rooms)
        ));
    }

    output.push('\n');
}

fn render_fun(output: &mut String, fun: &Fun) {
    if fun.fields.is_empty() {
        return;
    }

    output.push_str("### 🎪 Fun Facts\n");

    // Render each field with human-friendly formatting using insertion order from IndexMap
    for (key, value) in &fun.fields {
        let formatted_key = key.replace('_', " ");
        let formatted_key = uppercase_first_char(&formatted_key);
        let display_key = if key == "sent_encrypted_messages_ratio" {
            "Encrypted messages".to_string()
        } else {
            formatted_key.clone()
        };

        let formatted_value = match value {
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    // Special handling for crawl duration
                    if key == "crawl_duration_seconds" {
                        let seconds = i;
                        if seconds < 60 {
                            format!("{} seconds", seconds)
                        } else if seconds < 3600 {
                            let mins = seconds / 60;
                            let secs = seconds % 60;
                            if secs > 0 {
                                format!("{} min {} sec", mins, secs)
                            } else {
                                format!("{} min", mins)
                            }
                        } else {
                            let hours = seconds / 3600;
                            let mins = (seconds % 3600) / 60;
                            if mins > 0 {
                                format!("{} hr {} min", hours, mins)
                            } else {
                                format!("{} hr", hours)
                            }
                        }
                    } else {
                        format_number(i as i32)
                    }
                } else if let Some(f) = n.as_f64() {
                    // Special handling for reactions_per_message
                    if key == "reactions_per_message" {
                        if f > 0.0 {
                            let messages_per_reaction = 1.0 / f;
                            format!("every {:.0} sent messages", messages_per_reaction)
                        } else {
                            "never".to_string()
                        }
                    } else if key.ends_with("_per_message") || key.ends_with("_ratio") {
                        format!("{:.1}%", f * 100.0)
                    } else {
                        format!("{:.2}", f)
                    }
                } else {
                    n.to_string()
                }
            }
            serde_json::Value::String(s) => s.clone(),
            _ => value.to_string(),
        };

        // Add emoji based on field type
        let emoji = match key.as_str() {
            "longest_message_chars" => "📝",
            "favorite_weekday" => "📅",
            "peak_hour" => "🕐",
            "longest_streak_days" => "🔥",
            "reactions_per_message" => "😊",
            "edits_per_message" => "✏️",
            "crawl_duration_seconds" => "⏱️",
            "lurking_rooms" => "👀",
            "sent_encrypted_messages_ratio" => "🔐",
            _ => "✨",
        };

        // Special formatting for reactions_per_message
        let formatted_line = if key == "reactions_per_message" {
            format!("- {} You react on {}\n", emoji, formatted_value)
        } else {
            format!("- {} **{}:** {}\n", emoji, display_key, formatted_value)
        };

        output.push_str(&formatted_line);
    }

    output.push('\n');
}

fn scope_label(scope: &Scope) -> String {
    if let Some(label) = &scope.label {
        return label.clone();
    }

    match scope.kind {
        ScopeKind::Year => format!("Year {}", scope.key),
        ScopeKind::Month => format!("Month {}", scope.key),
        ScopeKind::Week => format!("Week {}", scope.key),
        ScopeKind::Day => format!("Day {}", scope.key),
        ScopeKind::Life => "Life-to-date".to_string(),
    }
}

fn scope_phrase(scope: &Scope) -> String {
    if let Some(label) = &scope.label {
        return label.clone();
    }

    match scope.kind {
        ScopeKind::Year => format!("the year {}", scope.key),
        ScopeKind::Month => format!("the month {}", scope.key),
        ScopeKind::Week => format!("the week {}", scope.key),
        ScopeKind::Day => format!("the day {}", scope.key),
        ScopeKind::Life => "your life on Matrix so far".to_string(),
    }
}

/// Format a number with thousand separators (raw integers, no abbreviation)
fn format_number(n: i32) -> String {
    let is_negative = n < 0;
    // Work with absolute value as i64 to safely handle i32::MIN
    let abs_str = (n as i64).abs().to_string();
    let mut grouped_rev = String::new();

    // Insert commas every three digits, starting from the right
    for (count, ch) in abs_str.chars().rev().enumerate() {
        if count > 0 && count.is_multiple_of(3) {
            grouped_rev.push(',');
        }
        grouped_rev.push(ch);
    }

    // Reverse back to normal order
    let mut formatted: String = grouped_rev.chars().rev().collect();
    if is_negative {
        formatted.insert(0, '-');
    }
    formatted
}

/// Uppercase the first character of a string
fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}
