use crate::stats::*;
use anyhow::Result;

/// Render stats to Markdown following md_report_layout.md
pub fn render(stats: &Stats) -> Result<String> {
    let mut output = String::new();
    
    // 1. Title, metadata, and account details
    render_header(&mut output, stats);
    
    // 2. Summary (including active days from coverage)
    render_summary(&mut output, &stats.summary, stats.coverage.days_active, stats.year);
    
    // 3. Rooms
    if let Some(ref rooms) = stats.rooms {
        render_rooms(&mut output, rooms, stats.year);
    }
    
    // 4. Created rooms
    if let Some(ref created_rooms) = stats.created_rooms {
        render_created_rooms(&mut output, created_rooms);
    }
    
    // 5. Reactions
    if let Some(ref reactions) = stats.reactions {
        render_reactions(&mut output, reactions);
    }
    
    // 6. Activity
    if let Some(ref activity) = stats.activity {
        render_activity(&mut output, activity);
    }
    
    // 7. Fun
    if let Some(ref fun) = stats.fun {
        render_fun(&mut output, fun);
    }
    
    Ok(output)
}

fn render_header(output: &mut String, stats: &Stats) {
    let account = &stats.account;
    
    // Title with display name if available
    if let Some(ref display_name) = account.display_name {
        output.push_str(&format!("# 🎉 Matrix Year {} — {}\n", stats.year, display_name));
    } else {
        output.push_str(&format!("# 🎉 Matrix Year {}\n", stats.year));
    }
    
    // Account details
    output.push_str("### 🧑 Account\n");
    let user_permalink = format!("https://matrix.to/#/{}", account.user_id);
    output.push_str(&format!("- **User ID:** [{}]({})\n", account.user_id, user_permalink));
    if let Some(ref name) = account.display_name {
        output.push_str(&format!("- **Display name:** {}\n", name));
    }
    if let Some(ref avatar) = account.avatar_url {
        output.push_str(&format!("- **Avatar (MXC):** {}\n", avatar));
    }
    output.push_str(&format!("- **Total joined rooms:** {}\n", account.rooms_total));
    output.push('\n');
}

// Coverage section intentionally removed from rendering; active days are shown in Summary.

fn render_summary(output: &mut String, summary: &Summary, active_days: Option<i32>, year: i32) {
    output.push_str("### 📊 Summary\n");
    output.push_str(&format!("- 💬 **Messages sent:** {}\n", format_number(summary.messages_sent)));
    output.push_str(&format!("- 🏠 **Active rooms:** {}\n", summary.active_rooms));
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
    
    if let Some(ref peak_month) = summary.peak_month {
        output.push_str(&format!("- 📈 **Peak month:** {} ({} messages) 🚀\n", 
            peak_month.month, format_number(peak_month.messages)));
    }
    
    // Explicit note that the rest of the report refers to the given year
    output.push_str(&format!("\n*All sections below refer to year {}.*\n\n", year));
}

fn render_activity(output: &mut String, activity: &Activity) {
    output.push_str("### 📈 Activity\n");
    
    // By month - horizontal display in 2 rows
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

fn render_rooms(output: &mut String, rooms: &Rooms, _year: i32) {
    output.push_str("### 🏘️ Rooms (private + DMs)\n");
    output.push_str(&format!("Total non-public rooms: **{}**\n\n", rooms.total));
    
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
                
                // Create clickable room name if permalink is available
                let name_display = if let Some(ref permalink) = room.permalink {
                    format!("[{}]({})", name, permalink)
                } else {
                    name.to_string()
                };
                
                output.push_str(&format!("| {} | {} | {} | {} |\n", 
                    rank, name_display, format_number(room.messages), percentage_str));
            }
            output.push('\n');
        }
    }
}

fn render_reactions(output: &mut String, reactions: &Reactions) {
    output.push_str("### 😊 Reactions\n");
    
    if let Some(total) = reactions.total {
        output.push_str(&format!("You made people smile with **{}** reactions on your messages!\n\n", format_number(total)));
    }
    
    // Top emojis
    if let Some(ref top_emojis) = reactions.top_emojis {
        if !top_emojis.is_empty() {
            output.push_str("**Top reactions**\n\n");
            output.push_str("| Rank | Emoji | Count |\n");
            output.push_str("| ---- | ----- | ----- |\n");
            
            for (i, emoji_entry) in top_emojis.iter().take(5).enumerate() {
                let rank = i + 1;
                output.push_str(&format!("| {} | {} | {} |\n", 
                    rank, emoji_entry.emoji, format_number(emoji_entry.count)));
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
                output.push_str(&format!("| {} | [view]({}) | {} |\n", 
                    rank, msg_entry.permalink, format_number(msg_entry.reaction_count)));
            }
            output.push('\n');
        }
    }
}

fn render_created_rooms(output: &mut String, created_rooms: &CreatedRooms) {
    output.push_str("### 🏗️ Rooms You Created\n");
    output.push_str(&format!("- **Total:** {}\n", format_number(created_rooms.total)));
    
    if let Some(dm_rooms) = created_rooms.dm_rooms {
        output.push_str(&format!("- 👥 **DM rooms:** {}\n", format_number(dm_rooms)));
    }
    
    if let Some(public_rooms) = created_rooms.public_rooms {
        output.push_str(&format!("- 🌐 **Public rooms:** {}\n", format_number(public_rooms)));
    }
    
    if let Some(private_rooms) = created_rooms.private_rooms {
        output.push_str(&format!("- 🔒 **Private rooms:** {}\n", format_number(private_rooms)));
    }
    
    output.push('\n');
}

fn render_fun(output: &mut String, fun: &Fun) {
    if fun.fields.is_empty() {
        return;
    }
    
    output.push_str("### 🎪 Fun Facts\n");
    
    // Render each field with human-friendly formatting in a deterministic order
    let mut keys: Vec<&String> = fun.fields.keys().collect();
    keys.sort();
    
    for key in keys {
        let value = fun.fields.get(key).expect("fun.fields must contain key from keys vector");
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

/// Format a number with thousand separators (raw integers, no abbreviation)
fn format_number(n: i32) -> String {
    let is_negative = n < 0;
    // Work with absolute value as i64 to safely handle i32::MIN
    let abs_str = (n as i64).abs().to_string();
    let mut grouped_rev = String::new();
    let mut count: usize = 0;
    
    // Insert commas every three digits, starting from the right
    for ch in abs_str.chars().rev() {
        if count > 0 && count.is_multiple_of(3) {
            grouped_rev.push(',');
        }
        grouped_rev.push(ch);
        count += 1;
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
