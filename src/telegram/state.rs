use std::collections::HashMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use chrono_tz::Europe::Kyiv;
use serde::{Deserialize, Serialize};
use serenity::model::id::ChannelId;

use crate::events::{ChannelName, Username};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionInfo {
    pub display_name: String,
    #[serde(serialize_with = "ser_channel_id", deserialize_with = "de_channel_id")]
    pub channel_id: ChannelId,
    pub joined_at: DateTime<Utc>,
}

/// Accumulated voice time for a single user within a week.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct UserStats {
    #[serde(default)]
    pub total_seconds: i64,
    #[serde(default)]
    pub prev_week_seconds: i64,
}

/// Per-week voice time stats for all users.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WeeklyStats {
    pub week_start: NaiveDate,
    #[serde(default)]
    pub users: HashMap<Username, UserStats>,
}

impl Default for WeeklyStats {
    fn default() -> Self {
        Self {
            week_start: current_week_start(),
            users: HashMap::new(),
        }
    }
}

/// Maps Discord channel IDs to their human-readable names.
pub(crate) type ChannelNames = HashMap<ChannelId, ChannelName>;

/// Returns the Monday of the current week in Ukraine time.
pub(crate) fn current_week_start() -> NaiveDate {
    let now_kyiv = Utc::now().with_timezone(&Kyiv);
    let date = now_kyiv.date_naive();
    date - chrono::Duration::days(i64::from(date.weekday().num_days_from_monday()))
}

/// Active voice sessions keyed by username.
pub(crate) type Sessions = HashMap<Username, SessionInfo>;

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct PersistentState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i32>,
    #[serde(default)]
    pub sessions: Sessions,
    #[serde(default)]
    pub weekly_stats: Option<WeeklyStats>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[serde(
        serialize_with = "ser_channel_names",
        deserialize_with = "de_channel_names"
    )]
    pub channel_names: ChannelNames,
}

fn ser_channel_id<S: serde::Serializer>(id: &ChannelId, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&id.get().to_string())
}

fn de_channel_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ChannelId, D::Error> {
    let s = String::deserialize(d)?;
    let id: u64 = s.parse().map_err(serde::de::Error::custom)?;
    Ok(ChannelId::new(id))
}

fn ser_channel_names<S: serde::Serializer>(names: &ChannelNames, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = s.serialize_map(Some(names.len()))?;
    for (id, name) in names {
        map.serialize_entry(&id.get().to_string(), name)?;
    }
    map.end()
}

fn de_channel_names<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ChannelNames, D::Error> {
    let raw: HashMap<String, String> = HashMap::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|(k, v)| {
            let id: u64 = k.parse().ok()?;
            Some((ChannelId::new(id), ChannelName::new(v)))
        })
        .collect())
}
