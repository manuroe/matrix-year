use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Deserialize, Serialize)]
pub struct Stats {
    pub schema_version: i32,
    pub year: i32,
    pub generated_at: String,
    pub account: Account,
    pub coverage: Coverage,
    pub summary: Summary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<Activity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rooms: Option<Rooms>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reactions: Option<Reactions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_rooms: Option<CreatedRooms>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fun: Option<Fun>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Account {
    pub user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub rooms_total: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Coverage {
    pub from: String,
    pub to: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days_active: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Summary {
    pub messages_sent: i32,
    pub active_rooms: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_rooms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_rooms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_rooms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_month: Option<PeakMonth>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeakMonth {
    pub month: String,
    pub messages: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Activity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_weekday: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_hour: Option<HashMap<String, i32>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Rooms {
    pub total: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<Vec<RoomEntry>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RoomEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub messages: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permalink: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Reactions {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_emojis: Option<Vec<EmojiEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_messages: Option<Vec<MessageReactionEntry>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EmojiEntry {
    pub emoji: String,
    pub count: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MessageReactionEntry {
    pub permalink: String,
    pub reaction_count: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatedRooms {
    pub total: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_rooms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_rooms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_rooms: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Fun {
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
}

impl Stats {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read stats file: {}", path.display()))?;
        
        let stats: Stats = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON from: {}", path.display()))?;
        
        Ok(stats)
    }
}
