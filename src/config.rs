use std::path::Path;

use chrono::{Datelike, NaiveDate};
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
    #[serde(default)]
    pub filter: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramConfig {
    pub chat_id: ChatId,
    pub state_chat_id: ChatId,
    #[serde(default = "default_tick_minutes")]
    pub duration_tick_minutes: u64,
    #[serde(default)]
    pub birthdays: Vec<Birthday>,
    #[serde(skip)]
    pub token: SecretString,
}

fn default_tick_minutes() -> u64 {
    5
}

/// A member's birthday: their Discord username plus the month/day it falls on.
///
/// `username` doubles as the lookup key for the cached display name.
#[derive(Debug, Clone, Deserialize)]
pub struct Birthday {
    #[serde(deserialize_with = "deserialize_trimmed")]
    pub username: String,
    #[serde(deserialize_with = "deserialize_month_day")]
    pub date: (u32, u32),
}

impl Birthday {
    /// Whether this birthday falls on `today` in the caller's timezone.
    pub fn is_today(&self, today: NaiveDate) -> bool {
        let (month, day) = self.date;
        today.month() == month && today.day() == day
    }
}

#[derive(Debug, Deserialize)]
pub struct DiscordConfig {
    #[serde(deserialize_with = "deserialize_guild_id")]
    pub target_guild_id: GuildId,
    #[serde(deserialize_with = "deserialize_channel_ids")]
    pub tracked_channels: Vec<ChannelId>,
    #[serde(default)]
    pub text_channels: Vec<TextChannelConfig>,
    #[serde(skip)]
    pub token: SecretString,
}

/// A Discord text channel mirrored into Telegram.
#[derive(Debug, Deserialize)]
pub struct TextChannelConfig {
    #[serde(deserialize_with = "deserialize_channel_id")]
    pub id: ChannelId,
    /// Title substring an embed must contain. Absent forwards every message.
    #[serde(default)]
    pub filter: Option<String>,
    /// Header shown on the forwarded Telegram message.
    #[serde(default = "default_label", deserialize_with = "deserialize_trimmed")]
    pub label: String,
}

fn default_label() -> String {
    "News".to_owned()
}

impl TextChannelConfig {
    /// Whether an embed title passes this channel's filter. No filter accepts everything.
    pub fn matches(&self, title: Option<&str>) -> bool {
        let Some(filter) = &self.filter else {
            return true;
        };

        title.is_some_and(|t| t.to_lowercase().contains(&filter.to_lowercase()))
    }
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

fn deserialize_channel_id<'de, D>(deserializer: D) -> Result<ChannelId, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let id: u64 = s.parse().map_err(serde::de::Error::custom)?;
    Ok(ChannelId::new(id))
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

fn deserialize_trimmed<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(s.trim().to_owned())
}

/// Parses a `"MM-DD"` birthday into a validated `(month, day)` pair.
fn deserialize_month_day<'de, D>(deserializer: D) -> Result<(u32, u32), D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    let (month, day) = s
        .trim()
        .split_once('-')
        .ok_or_else(|| serde::de::Error::custom(format!("expected MM-DD, got {s:?}")))?;

    let month: u32 = month.parse().map_err(serde::de::Error::custom)?;
    let day: u32 = day.parse().map_err(serde::de::Error::custom)?;

    // 2000 is a leap year, so 02-29 is accepted (it just never fires off-cycle).
    NaiveDate::from_ymd_opt(2000, month, day)
        .ok_or_else(|| serde::de::Error::custom(format!("invalid month/day: {s:?}")))?;

    Ok((month, day))
}

#[cfg(test)]
mod tests {
    use super::{Birthday, TextChannelConfig};

    fn parse(date: &str) -> Result<Birthday, serde_yaml::Error> {
        serde_yaml::from_str(&format!("username: someone\ndate: \"{date}\""))
    }

    #[test]
    fn parses_valid_month_day() {
        let birthday = parse("07-24").expect("valid date");
        assert_eq!(birthday.date, (7, 24));
        assert_eq!(birthday.username, "someone");
    }

    #[test]
    fn accepts_leap_day() {
        assert_eq!(parse("02-29").expect("leap day").date, (2, 29));
    }

    #[test]
    fn rejects_out_of_range_month() {
        assert!(parse("13-01").is_err());
    }

    #[test]
    fn rejects_day_beyond_month_length() {
        assert!(parse("02-30").is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("not-a-date").is_err());
        assert!(parse("0724").is_err());
    }

    #[test]
    fn matches_only_its_own_month_and_day() {
        let birthday = parse("07-24").expect("valid date");
        let date = |y, m, d| chrono::NaiveDate::from_ymd_opt(y, m, d).expect("valid date");

        assert!(birthday.is_today(date(2026, 7, 24)));
        assert!(birthday.is_today(date(2027, 7, 24)));
        assert!(!birthday.is_today(date(2026, 7, 25)));
        assert!(!birthday.is_today(date(2026, 8, 24)));
    }

    fn channel(yaml: &str) -> TextChannelConfig {
        serde_yaml::from_str(yaml).expect("valid channel config")
    }

    #[test]
    fn unfiltered_channel_accepts_every_title() {
        let tc = channel("id: \"1\"");
        assert!(tc.matches(Some("anything at all")));
        assert!(tc.matches(None));
        assert_eq!(tc.label, "News");
    }

    #[test]
    fn filtered_channel_matches_case_insensitively() {
        let tc = channel("id: \"1\"\nfilter: \"Class Tuning\"\nlabel: \"Blue Post\"");
        assert!(tc.matches(Some("CLASS TUNING Incoming")));
        assert!(!tc.matches(Some("Hotfixes")));
        assert!(!tc.matches(None));
        assert_eq!(tc.label, "Blue Post");
    }
}
