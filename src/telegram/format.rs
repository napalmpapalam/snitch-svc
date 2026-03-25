use chrono::{TimeDelta, Utc};
use serenity::model::id::ChannelId;

use crate::emoji;
use crate::events::VoiceEvent;

use super::achievements::Achievement;
use super::state::{ChannelNames, Sessions};

/// Formats a combined message for a voice event (join/leave) showing all active channels.
pub(crate) fn format_event_message(
    event: &VoiceEvent,
    sessions: &Sessions,
    tracked_channels: &[ChannelId],
    channel_names: &ChannelNames,
    achievements: &[Achievement],
) -> String {
    let update = event.update();
    let mut msg = String::from("<blockquote>");

    // Action line
    let display = format_display_name(update.username.as_ref(), update.display_name.as_ref());
    msg.push_str(&format!(
        "{} <b>{}</b> {}\n",
        event.icon(),
        html_escape(&display),
        event.verb()
    ));

    // Check if anyone is in voice
    if sessions.is_empty() {
        msg.push_str("\n💤 No one in voice");
        format_achievements(&mut msg, achievements);
        msg.push_str("</blockquote>");
        return msg;
    }

    format_channels(&mut msg, sessions, tracked_channels, channel_names);
    format_achievements(&mut msg, achievements);

    msg.push_str("</blockquote>");
    msg
}

/// Formats a status message (no action line) for duration tick updates.
pub(crate) fn format_status_message(
    sessions: &Sessions,
    tracked_channels: &[ChannelId],
    channel_names: &ChannelNames,
    achievements: &[Achievement],
) -> String {
    let mut msg = String::from("<blockquote>");

    if sessions.is_empty() {
        msg.push_str("💤 No one in voice");
        msg.push_str("</blockquote>");
        return msg;
    }

    format_channels(&mut msg, sessions, tracked_channels, channel_names);
    format_achievements(&mut msg, achievements);

    msg.push_str("</blockquote>");
    msg
}

/// Renders channel listings with members and durations into the message buffer.
fn format_channels(
    msg: &mut String,
    sessions: &Sessions,
    tracked_channels: &[ChannelId],
    channel_names: &ChannelNames,
) {
    let now = Utc::now();

    for &channel_id in tracked_channels {
        let channel_members: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| s.channel_id == channel_id)
            .collect();

        if channel_members.is_empty() {
            continue;
        }

        let channel_name = channel_names
            .get(&channel_id)
            .map(|n| n.to_string())
            .unwrap_or_else(|| channel_id.to_string());

        msg.push_str(&format!(
            "\n🎙 <b>Channel:</b> {}\n",
            html_escape(&channel_name)
        ));
        msg.push_str(&format!("👤 <b>Members:</b> {}\n\n", channel_members.len()));
        msg.push_str("📋 <b>Now in channel:</b>\n\n");

        for (_, session) in &channel_members {
            let emoji = emoji::random();
            let duration = format_duration(now - session.joined_at);
            msg.push_str(&format!(
                "• {} {} ({})\n",
                emoji,
                html_escape(session.display_name.as_ref()),
                duration,
            ));
        }
    }
}

/// Renders achievements at the bottom of the message.
fn format_achievements(msg: &mut String, achievements: &[Achievement]) {
    for achievement in achievements {
        msg.push_str(&format!(
            "\n{} {} — {}!",
            achievement.kind.emoji(),
            html_escape(achievement.display_name.as_ref()),
            achievement.kind.label(),
        ));
    }
}

/// Formats a time delta into a human-readable duration string.
///
/// ```text
/// 1h 30m  → "1h 30m"
///     45m → "45m"
///     30s → "<1m"
/// ```
pub(crate) fn format_duration(delta: TimeDelta) -> String {
    let total_minutes = delta.num_minutes();
    if total_minutes < 1 {
        return "&lt;1m".to_owned();
    }

    let hours = delta.num_hours();
    let minutes = total_minutes % 60;

    if hours > 0 && minutes > 0 {
        format!("{hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h")
    } else {
        format!("{minutes}m")
    }
}

fn format_display_name(username: &str, display_name: &str) -> String {
    let emoji = emoji::random();

    if display_name != username {
        format!("{emoji} {display_name} ({username}) {emoji}")
    } else {
        format!("{emoji} {username}")
    }
}

pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
