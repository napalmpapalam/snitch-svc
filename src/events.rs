#![allow(dead_code)]

use serenity::model::id::ChannelId;

pub struct ChannelUpdate {
    pub user_name: String,
    pub channel_name: String,
    pub channel_id: ChannelId,
    pub members: Vec<String>,
}

pub enum VoiceEvent {
    Joined(ChannelUpdate),
    Left(ChannelUpdate),
}
