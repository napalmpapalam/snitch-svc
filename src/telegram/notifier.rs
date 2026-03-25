use std::collections::HashMap;

use eyre::{Result, WrapErr};
use serenity::model::id::ChannelId;
use teloxide_core::Bot;
use teloxide_core::payloads::SendMessageSetters;
use teloxide_core::prelude::Requester;
use teloxide_core::types::{ChatId, MessageId, ParseMode};

use crate::emoji;
use crate::events::{MemberInfo, VoiceEvent};

pub struct Notifier {
    bot: Bot,
    chat_id: ChatId,
    state_chat_id: ChatId,
    state_message_id: Option<MessageId>,
    message_cache: HashMap<ChannelId, MessageId>,
}

impl Notifier {
    pub async fn new(bot: Bot, chat_id: ChatId, state_chat_id: ChatId) -> Result<Self> {
        let mut notifier = Self {
            bot,
            chat_id,
            state_chat_id,
            state_message_id: None,
            message_cache: HashMap::new(),
        };
        notifier.load_state().await?;
        Ok(notifier)
    }

    pub async fn handle_event(&mut self, event: &VoiceEvent) -> Result<()> {
        let update = event.update();
        let action = event.action();

        tracing::info!(
            action,
            username = %update.username,
            display_name = %update.display_name,
            channel = %update.channel_name,
            channel_id = %update.channel_id,
            "voice event"
        );

        // Skip initial state entirely — only send on actual join/leave
        if event.is_initial_state() {
            tracing::debug!(channel_id = %update.channel_id, "skipping initial state");
            return Ok(());
        }

        let message = format_notification(event);

        // Delete old message for this channel
        if let Some(old_msg_id) = self.message_cache.remove(&update.channel_id) {
            tracing::debug!(
                channel_id = %update.channel_id,
                old_msg_id = old_msg_id.0,
                "deleting old notification"
            );
            self.delete_message(old_msg_id).await;
        }

        // Send new message
        if let Some(msg_id) = self.send_message(&message).await? {
            tracing::debug!(
                channel_id = %update.channel_id,
                msg_id = msg_id.0,
                "stored new message in cache"
            );
            self.message_cache.insert(update.channel_id, msg_id);
        }

        // Persist state
        self.persist_state().await?;

        Ok(())
    }

    async fn send_message(&self, text: &str) -> Result<Option<MessageId>> {
        let bot = &self.bot;
        tracing::debug!(chat_id = %self.chat_id, "sending telegram message");
        match bot
            .send_message(self.chat_id, text)
            .parse_mode(ParseMode::Html)
            .await
        {
            Ok(msg) => {
                tracing::info!(chat_id = %self.chat_id, msg_id = msg.id.0, "sent telegram message");
                Ok(Some(msg.id))
            }
            Err(err) => {
                tracing::error!(chat_id = %self.chat_id, %err, "failed to send telegram message");
                Ok(None)
            }
        }
    }

    async fn delete_message(&self, message_id: MessageId) {
        let bot = &self.bot;
        match bot.delete_message(self.chat_id, message_id).await {
            Ok(_) => {
                tracing::debug!(
                    chat_id = %self.chat_id,
                    msg_id = message_id.0,
                    "deleted telegram message"
                );
            }
            Err(err) => {
                tracing::warn!(
                    chat_id = %self.chat_id,
                    msg_id = message_id.0,
                    %err,
                    "failed to delete telegram message"
                );
            }
        }
    }

    async fn load_state(&mut self) -> Result<()> {
        let bot = &self.bot;

        tracing::info!(state_chat_id = %self.state_chat_id, "loading state from telegram");

        let chat = bot
            .get_chat(self.state_chat_id)
            .await
            .wrap_err_with(|| format!("failed to get state chat {}", self.state_chat_id))?;

        if let Some(pinned) = chat.pinned_message {
            self.state_message_id = Some(pinned.id);

            let text = pinned.text().unwrap_or("{}");
            self.message_cache = parse_state(text);

            tracing::info!(
                state_msg_id = pinned.id.0,
                channels = self.message_cache.len(),
                state = %text,
                "loaded state from pinned message"
            );
        } else {
            tracing::info!(
                state_chat_id = %self.state_chat_id,
                "no pinned message found, initializing state"
            );

            let msg = bot
                .send_message(self.state_chat_id, "{}")
                .await
                .wrap_err_with(|| {
                    format!(
                        "failed to send initial state message to {}",
                        self.state_chat_id
                    )
                })?;

            tracing::debug!(
                state_chat_id = %self.state_chat_id,
                msg_id = msg.id.0,
                "sent initial state message, pinning"
            );

            bot.pin_chat_message(self.state_chat_id, msg.id)
                .await
                .wrap_err_with(|| {
                    format!(
                        "failed to pin state message {} in {}",
                        msg.id.0, self.state_chat_id
                    )
                })?;

            tracing::info!(
                state_chat_id = %self.state_chat_id,
                state_msg_id = msg.id.0,
                "pinned initial state message"
            );

            self.state_message_id = Some(msg.id);
        }

        Ok(())
    }

    async fn persist_state(&self) -> Result<()> {
        let bot = &self.bot;
        let msg_id = match self.state_message_id {
            Some(id) => id,
            None => return Ok(()),
        };

        let state = serialize_state(&self.message_cache);

        tracing::debug!(
            state_msg_id = msg_id.0,
            state = %state,
            "persisting state"
        );

        bot.edit_message_text(self.state_chat_id, msg_id, &state)
            .await
            .wrap_err_with(|| {
                format!(
                    "failed to persist state to message {} in {}",
                    msg_id.0, self.state_chat_id
                )
            })?;

        tracing::debug!(state_msg_id = msg_id.0, "state persisted");

        Ok(())
    }
}

fn format_notification(event: &VoiceEvent) -> String {
    let update = event.update();
    let mut msg = String::from("<blockquote>");

    // Action line
    let display = format_display_name(&update.username, &update.display_name);
    msg.push_str(&format!(
        "{} <b>{}</b> {}\n\n",
        event.icon(),
        html_escape(&display),
        event.verb()
    ));

    // Channel info
    msg.push_str(&format!(
        "🎙 <b>Channel:</b> {}\n",
        html_escape(&update.channel_name)
    ));

    if update.members.is_empty() {
        msg.push_str("💤 No one in channel");
    } else {
        msg.push_str(&format!("👤 <b>Members:</b> {}\n\n", update.members.len()));
        msg.push_str("📋 <b>Now in channel:</b>\n\n");
        for member in &update.members {
            let formatted = format_member_name(member);
            msg.push_str(&format!("• {}\n", html_escape(&formatted)));
        }
    }

    msg.push_str("</blockquote>");
    msg
}

fn format_display_name(username: &str, display_name: &str) -> String {
    let emoji = emoji::for_user(username);

    if username == "lodanorely" {
        return format!("{emoji} таракан {emoji}");
    }

    if display_name != username {
        format!("{emoji} {display_name} ({username}) {emoji}")
    } else {
        format!("{emoji} {username}")
    }
}

fn format_member_name(member: &MemberInfo) -> String {
    let emoji = emoji::for_user(&member.username);
    format!("{emoji} {}", member.display_name)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn parse_state(text: &str) -> HashMap<ChannelId, MessageId> {
    let raw: HashMap<String, i32> = serde_json::from_str(text).unwrap_or_default();
    raw.into_iter()
        .filter_map(|(k, v)| {
            let channel_id = k.parse::<u64>().ok()?;
            Some((ChannelId::new(channel_id), MessageId(v)))
        })
        .collect()
}

fn serialize_state(cache: &HashMap<ChannelId, MessageId>) -> String {
    let raw: HashMap<String, i32> = cache
        .iter()
        .map(|(k, v)| (k.get().to_string(), v.0))
        .collect();
    serde_json::to_string(&raw).unwrap_or_else(|_| "{}".to_owned())
}
