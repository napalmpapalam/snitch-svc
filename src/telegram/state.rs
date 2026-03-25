use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serenity::model::id::ChannelId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SessionInfo {
    pub display_name: String,
    #[serde(serialize_with = "ser_channel_id", deserialize_with = "de_channel_id")]
    pub channel_id: ChannelId,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct PersistentState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<i32>,
    #[serde(default)]
    pub sessions: HashMap<String, SessionInfo>,
}

fn ser_channel_id<S: serde::Serializer>(id: &ChannelId, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&id.get().to_string())
}

fn de_channel_id<'de, D: serde::Deserializer<'de>>(d: D) -> Result<ChannelId, D::Error> {
    let s = String::deserialize(d)?;
    let id: u64 = s.parse().map_err(serde::de::Error::custom)?;
    Ok(ChannelId::new(id))
}
