use std::fmt;

use serde::{Deserialize, Serialize};
use serenity::model::id::ChannelId;

/// Newtype wrapper for Discord usernames. Prevents mixing up with display names.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Username(String);

impl Username {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl fmt::Display for Username {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Username {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Newtype wrapper for Discord channel display names.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelName(String);

impl ChannelName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl fmt::Display for ChannelName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ChannelName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Newtype wrapper for user display names (guild nickname or username fallback).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DisplayName(String);

impl DisplayName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

impl fmt::Display for DisplayName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for DisplayName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

pub struct ChannelUpdate {
    pub username: Username,
    pub display_name: DisplayName,
    pub channel_name: ChannelName,
    pub channel_id: ChannelId,
}

pub enum VoiceEvent {
    Joined(ChannelUpdate),
    Left(ChannelUpdate),
    InitialState(ChannelUpdate),
}

impl VoiceEvent {
    pub fn update(&self) -> &ChannelUpdate {
        match self {
            Self::Joined(u) | Self::Left(u) | Self::InitialState(u) => u,
        }
    }

    pub fn action(&self) -> &'static str {
        match self {
            Self::Joined(_) => "joined",
            Self::Left(_) => "left",
            Self::InitialState(_) => "initial_state",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            Self::Joined(_) | Self::InitialState(_) => "🎉",
            Self::Left(_) => "👋",
        }
    }

    pub fn verb(&self) -> &'static str {
        match self {
            Self::Joined(_) | Self::InitialState(_) => "joined",
            Self::Left(_) => "left",
        }
    }

    pub fn is_initial_state(&self) -> bool {
        matches!(self, Self::InitialState(_))
    }
}
