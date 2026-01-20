use anyhow::{Context, Result};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[cfg(test)]
use anyhow::{anyhow, bail};
#[cfg(test)]
use jsonschema::{Draft, JSONSchema};

#[derive(Debug, Deserialize, Serialize)]
pub struct Stats {
    pub schema_version: i32,
    pub scope: Scope,
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

#[derive(Debug, Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScopeKind {
    Year,
    Month,
    Week,
    Day,
    Life,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Scope {
    #[serde(rename = "type")]
    pub kind: ScopeKind,
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
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
    pub peaks: Option<Peaks>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MessagesByRoomType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Peaks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<PeakYear>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub month: Option<PeakMonth>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub week: Option<PeakWeek>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<PeakDay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hour: Option<PeakHour>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeakMonth {
    pub month: String,
    pub messages: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeakYear {
    pub year: String,
    pub messages: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeakWeek {
    pub week: String,
    pub messages: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeakDay {
    pub day: String,
    pub messages: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PeakHour {
    pub hour: String,
    pub messages: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Activity {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_week: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_weekday: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_hour: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_day: Option<HashMap<String, i32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_year: Option<HashMap<String, i32>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Rooms {
    pub total: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top: Option<Vec<RoomEntry>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages_by_room_type: Option<MessagesByRoomType>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RoomEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub messages: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f64>,
    pub permalink: String,
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
    pub fields: IndexMap<String, serde_json::Value>,
}

impl Stats {
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read stats file: {}", path.display()))?;

        let stats: Stats = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse JSON from: {}", path.display()))?;

        Ok(stats)
    }

    #[cfg(test)]
    /// Validate stats JSON against the JSON schema
    pub fn validate_with_schema(stats_json: &serde_json::Value, schema: &JSONSchema) -> Result<()> {
        match schema.validate(stats_json) {
            Ok(_) => Ok(()),
            Err(errors) => {
                let error_messages: Vec<String> = errors
                    .map(|e| format!("  - {}: {}", e.instance_path, e))
                    .collect();
                bail!("Stats validation failed:\n{}", error_messages.join("\n"))
            }
        }
    }

    #[cfg(test)]
    /// Load and compile the JSON schema
    pub fn load_schema(schema_path: &Path) -> Result<JSONSchema> {
        let schema_content = std::fs::read_to_string(schema_path)
            .with_context(|| format!("Failed to read schema file: {}", schema_path.display()))?;

        let schema_json: serde_json::Value =
            serde_json::from_str(&schema_content).with_context(|| {
                format!(
                    "Failed to parse schema JSON from: {}",
                    schema_path.display()
                )
            })?;

        JSONSchema::options()
            .with_draft(Draft::Draft7)
            .compile(&schema_json)
            .map_err(|e| anyhow!("Failed to compile JSON schema: {}", e))
    }

    #[cfg(test)]
    /// Load stats from file and validate against schema
    pub fn load_and_validate(stats_path: &Path, schema_path: &Path) -> Result<Self> {
        // Load stats JSON
        let stats_content = std::fs::read_to_string(stats_path)
            .with_context(|| format!("Failed to read stats file: {}", stats_path.display()))?;

        let stats_json: serde_json::Value =
            serde_json::from_str(&stats_content).with_context(|| {
                format!("Failed to parse stats JSON from: {}", stats_path.display())
            })?;

        // Load and compile schema
        let schema = Self::load_schema(schema_path)?;

        // Validate
        Self::validate_with_schema(&stats_json, &schema)?;

        // Deserialize to Stats struct
        let stats: Stats = serde_json::from_value(stats_json).with_context(|| {
            format!("Failed to deserialize stats from: {}", stats_path.display())
        })?;

        Ok(stats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn get_schema_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stats_schema.json")
    }

    fn get_example_stats_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/example-stats.json")
    }

    #[test]
    fn test_load_schema() {
        let schema_path = get_schema_path();
        let result = Stats::load_schema(&schema_path);
        assert!(result.is_ok(), "Failed to load schema: {:?}", result.err());
    }

    #[test]
    fn test_validate_example_stats() {
        let schema_path = get_schema_path();
        let stats_path = get_example_stats_path();

        // Load and validate
        let result = Stats::load_and_validate(&stats_path, &schema_path);
        assert!(
            result.is_ok(),
            "Example stats validation failed: {:?}",
            result.err()
        );

        // Verify the loaded stats
        let stats = result.unwrap();
        assert_eq!(stats.schema_version, 1);
        assert_eq!(stats.scope.kind, ScopeKind::Year);
        assert_eq!(stats.scope.key, "2025");
    }

    #[test]
    fn test_validate_missing_required_field() {
        let schema_path = get_schema_path();
        let schema = Stats::load_schema(&schema_path).expect("Failed to load schema");

        // Missing 'scope' field
        let invalid_stats = json!({
            "schema_version": 1,
            "generated_at": "2025-12-31",
            "account": {
                "user_id": "@test:example.org",
                "rooms_total": 10
            },
            "coverage": {
                "from": "2025-01-01",
                "to": "2025-12-31"
            },
            "summary": {
                "messages_sent": 100,
                "active_rooms": 5
            }
        });

        let result = Stats::validate_with_schema(&invalid_stats, &schema);
        assert!(
            result.is_err(),
            "Should fail validation for missing 'scope'"
        );
        let err_msg = format!("{:?}", result.err().unwrap());
        assert!(
            err_msg.contains("scope"),
            "Error should mention missing field"
        );
    }

    #[test]
    fn test_validate_invalid_date_format() {
        let schema_path = get_schema_path();
        let schema = Stats::load_schema(&schema_path).expect("Failed to load schema");

        // Invalid date format
        let invalid_stats = json!({
            "schema_version": 1,
            "scope": {"type": "year", "key": "2025"},
            "generated_at": "not-a-date",
            "account": {
                "user_id": "@test:example.org",
                "rooms_total": 10
            },
            "coverage": {
                "from": "2025-01-01",
                "to": "2025-12-31"
            },
            "summary": {
                "messages_sent": 100,
                "active_rooms": 5
            }
        });

        let result = Stats::validate_with_schema(&invalid_stats, &schema);
        assert!(
            result.is_err(),
            "Should fail validation for invalid date format"
        );
    }

    #[test]
    fn test_validate_negative_count() {
        let schema_path = get_schema_path();
        let schema = Stats::load_schema(&schema_path).expect("Failed to load schema");

        // Negative messages_sent
        let invalid_stats = json!({
            "schema_version": 1,
            "scope": {"type": "year", "key": "2025"},
            "generated_at": "2025-12-31",
            "account": {
                "user_id": "@test:example.org",
                "rooms_total": 10
            },
            "coverage": {
                "from": "2025-01-01",
                "to": "2025-12-31"
            },
            "summary": {
                "messages_sent": -100,
                "active_rooms": 5
            }
        });

        let result = Stats::validate_with_schema(&invalid_stats, &schema);
        assert!(result.is_err(), "Should fail validation for negative count");
    }

    #[test]
    fn test_validate_additional_properties() {
        let schema_path = get_schema_path();
        let schema = Stats::load_schema(&schema_path).expect("Failed to load schema");

        // Extra field in account object
        let invalid_stats = json!({
            "schema_version": 1,
            "scope": {"type": "year", "key": "2025"},
            "generated_at": "2025-12-31",
            "account": {
                "user_id": "@test:example.org",
                "rooms_total": 10,
                "unexpected_field": "should fail"
            },
            "coverage": {
                "from": "2025-01-01",
                "to": "2025-12-31"
            },
            "summary": {
                "messages_sent": 100,
                "active_rooms": 5
            }
        });

        let result = Stats::validate_with_schema(&invalid_stats, &schema);
        assert!(
            result.is_err(),
            "Should fail validation for additional properties"
        );
    }

    #[test]
    fn test_validate_percentage_range() {
        let schema_path = get_schema_path();
        let schema = Stats::load_schema(&schema_path).expect("Failed to load schema");

        // Percentage > 100
        let invalid_stats = json!({
            "schema_version": 1,
            "scope": {"type": "year", "key": "2025"},
            "generated_at": "2025-12-31",
            "account": {
                "user_id": "@test:example.org",
                "rooms_total": 10
            },
            "coverage": {
                "from": "2025-01-01",
                "to": "2025-12-31"
            },
            "summary": {
                "messages_sent": 100,
                "active_rooms": 5
            },
            "rooms": {
                "total": 3,
                "top": [
                    {
                        "messages": 50,
                        "percentage": 150.0,
                        "permalink": "https://matrix.to/#/!room:test/$evt"
                    }
                ]
            }
        });

        let result = Stats::validate_with_schema(&invalid_stats, &schema);
        assert!(
            result.is_err(),
            "Should fail validation for percentage > 100"
        );
    }

    #[test]
    fn test_validate_scope_types() {
        let schema_path = get_schema_path();
        let schema = Stats::load_schema(&schema_path).expect("Failed to load schema");

        // Invalid scope type
        let invalid_stats = json!({
            "schema_version": 1,
            "scope": {"type": "invalid", "key": "2025"},
            "generated_at": "2025-12-31",
            "account": {
                "user_id": "@test:example.org",
                "rooms_total": 10
            },
            "coverage": {
                "from": "2025-01-01",
                "to": "2025-12-31"
            },
            "summary": {
                "messages_sent": 100,
                "active_rooms": 5
            }
        });

        let result = Stats::validate_with_schema(&invalid_stats, &schema);
        assert!(
            result.is_err(),
            "Should fail validation for invalid scope type"
        );
    }
}
