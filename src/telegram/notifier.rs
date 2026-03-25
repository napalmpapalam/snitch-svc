use std::collections::HashMap;

use eyre::{Result, WrapErr};
use serenity::model::id::ChannelId;
use teloxide_core::Bot;
use teloxide_core::payloads::SendMessageSetters;
use teloxide_core::prelude::Requester;
use teloxide_core::types::{ChatId, MessageId, ParseMode};

use crate::events::VoiceEvent;

use super::format::format_event_message;
use super::state::{PersistentState, SessionInfo};

pub struct Notifier {
    bot: Bot,
    chat_id: ChatId,
    state_chat_id: ChatId,
    tracked_channels: Vec<ChannelId>,
    state_message_id: Option<MessageId>,
    message_id: Option<MessageId>,
    sessions: HashMap<String, SessionInfo>,
}

impl Notifier {
    pub async fn new(
        bot: Bot,
        chat_id: ChatId,
        state_chat_id: ChatId,
        tracked_channels: Vec<ChannelId>,
    ) -> Result<Self> {
        let mut notifier = Self {
            bot,
            chat_id,
            state_chat_id,
            tracked_channels,
            state_message_id: None,
            message_id: None,
            sessions: HashMap::new(),
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

        // Update sessions
        match event {
            VoiceEvent::Joined(u) | VoiceEvent::InitialState(u) => {
                self.sessions.insert(
                    u.username.clone(),
                    SessionInfo {
                        display_name: u.display_name.clone(),
                        channel_id: u.channel_id,
                    },
                );
            }
            VoiceEvent::Left(u) => {
                self.sessions.remove(&u.username);
            }
        }

        // Don't send messages for initial state — just populate sessions
        if event.is_initial_state() {
            tracing::debug!(
                username = %update.username,
                channel_id = %update.channel_id,
                "populated session from initial state"
            );
            return Ok(());
        }

        // Format combined message
        let message = format_event_message(event, &self.sessions, &self.tracked_channels);

        // Delete old message, send new one (keeps it at bottom of chat)
        if let Some(old_msg_id) = self.message_id.take() {
            self.delete_message(old_msg_id).await;
        }

        if let Some(msg_id) = self.send_message(&message).await? {
            self.message_id = Some(msg_id);
        }

        self.persist_state().await?;

        Ok(())
    }

    async fn send_message(&self, text: &str) -> Result<Option<MessageId>> {
        tracing::debug!(chat_id = %self.chat_id, "sending telegram message");
        match self
            .bot
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
        match self.bot.delete_message(self.chat_id, message_id).await {
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
        tracing::info!(state_chat_id = %self.state_chat_id, "loading state from telegram");

        let chat = self
            .bot
            .get_chat(self.state_chat_id)
            .await
            .wrap_err_with(|| format!("failed to get state chat {}", self.state_chat_id))?;

        if let Some(pinned) = chat.pinned_message {
            self.state_message_id = Some(pinned.id);

            let text = pinned.text().unwrap_or("{}");
            let state: PersistentState = serde_json::from_str(text).unwrap_or_default();

            self.message_id = state.message_id.map(MessageId);
            self.sessions = state.sessions;

            tracing::info!(
                state_msg_id = pinned.id.0,
                sessions = self.sessions.len(),
                state = %text,
                "loaded state from pinned message"
            );
        } else {
            tracing::info!(
                state_chat_id = %self.state_chat_id,
                "no pinned message found, initializing state"
            );

            let msg = self
                .bot
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

            self.bot
                .pin_chat_message(self.state_chat_id, msg.id)
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
        let msg_id = match self.state_message_id {
            Some(id) => id,
            None => return Ok(()),
        };

        let state = PersistentState {
            message_id: self.message_id.map(|m| m.0),
            sessions: self.sessions.clone(),
        };
        let text = serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_owned());

        tracing::debug!(
            state_msg_id = msg_id.0,
            state = %text,
            "persisting state"
        );

        self.bot
            .edit_message_text(self.state_chat_id, msg_id, &text)
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
