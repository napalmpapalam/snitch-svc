#![allow(dead_code)]

use std::path::Path;

use eyre::{Result, WrapErr};
use secrecy::SecretString;
use serde::Deserialize;
use serenity::model::id::{ChannelId, GuildId};
use teloxide_core::types::ChatId;
use tracing::level_filters::LevelFilter;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub log: LogConfig,
    pub telegram: TelegramConfig,
    pub discord: DiscordConfig,
}

#[derive(Debug, Deserialize)]
pub struct LogConfig {
    #[serde(deserialize_with = "deserialize_level_filter")]
    pub level: LevelFilter,
}

#[derive(Debug, Deserialize)]
pub struct TelegramConfig {
    pub chat_id: ChatId,
    pub state_chat_id: ChatId,
    #[serde(skip)]
    pub token: SecretString,
}

#[derive(Debug, Deserialize)]
pub struct DiscordConfig {
    #[serde(deserialize_with = "deserialize_guild_id")]
    pub target_guild_id: GuildId,
    #[serde(deserialize_with = "deserialize_channel_ids")]
    pub tracked_channels: Vec<ChannelId>,
    #[serde(skip)]
    pub token: SecretString,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = std::env::var("CONFIG").unwrap_or_else(|_| "config.yaml".to_owned());
        Self::load_from(&path)
    }

    fn load_from(path: &str) -> Result<Self> {
        let content = std::fs::read_to_string(Path::new(path))
            .wrap_err_with(|| format!("failed to read config file: {path}"))?;

        let mut config: Config =
            serde_yaml::from_str(&content).wrap_err("failed to parse config file")?;

        config.discord.token = read_env_secret("DISCORD_TOKEN")?;
        config.telegram.token = read_env_secret("TELEGRAM_TOKEN")?;

        Ok(config)
    }
}

fn read_env_secret(name: &str) -> Result<SecretString> {
    std::env::var(name)
        .map(SecretString::from)
        .wrap_err_with(|| format!("{name} environment variable must be set"))
}

fn deserialize_level_filter<'de, D>(deserializer: D) -> Result<LevelFilter, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<LevelFilter>().map_err(serde::de::Error::custom)
}

fn deserialize_guild_id<'de, D>(deserializer: D) -> Result<GuildId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let id: u64 = s.parse().map_err(serde::de::Error::custom)?;
    Ok(GuildId::new(id))
}

fn deserialize_channel_ids<'de, D>(deserializer: D) -> Result<Vec<ChannelId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let strings: Vec<String> = Vec::deserialize(deserializer)?;
    strings
        .iter()
        .map(|s| {
            let id: u64 = s.parse().map_err(serde::de::Error::custom)?;
            Ok(ChannelId::new(id))
        })
        .collect()
}
