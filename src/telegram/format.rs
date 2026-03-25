use std::collections::HashMap;

use serenity::model::id::ChannelId;

use crate::emoji;
use crate::events::VoiceEvent;

use super::state::SessionInfo;

/// Formats a combined message for a voice event (join/leave) showing all active channels.
pub(crate) fn format_event_message(
    event: &VoiceEvent,
    sessions: &HashMap<String, SessionInfo>,
    tracked_channels: &[ChannelId],
) -> String {
    let update = event.update();
    let mut msg = String::from("<blockquote>");

    // Action line
    let display = format_display_name(&update.username, &update.display_name);
    msg.push_str(&format!(
        "{} <b>{}</b> {}\n",
        event.icon(),
        html_escape(&display),
        event.verb()
    ));

    // Check if anyone is in voice
    if sessions.is_empty() {
        msg.push_str("\n💤 No one in voice");
        msg.push_str("</blockquote>");
        return msg;
    }

    // Render each tracked channel that has members
    for &channel_id in tracked_channels {
        let channel_members: Vec<_> = sessions
            .iter()
            .filter(|(_, s)| s.channel_id == channel_id)
            .collect();

        if channel_members.is_empty() {
            continue;
        }

        // Use channel name from the event if it matches, otherwise fall back to ID
        let channel_name = if update.channel_id == channel_id {
            update.channel_name.clone()
        } else {
            channel_id.to_string()
        };

        msg.push_str(&format!(
            "\n🎙 <b>Channel:</b> {}\n",
            html_escape(&channel_name)
        ));
        msg.push_str(&format!("👤 <b>Members:</b> {}\n\n", channel_members.len()));
        msg.push_str("📋 <b>Now in channel:</b>\n\n");

        for (_, session) in &channel_members {
            let emoji = emoji::random();
            msg.push_str(&format!(
                "• {} {}\n",
                emoji,
                html_escape(&session.display_name)
            ));
        }
    }

    msg.push_str("</blockquote>");
    msg
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
