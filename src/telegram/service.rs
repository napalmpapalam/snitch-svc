use chrono::Utc;
use eyre::{Result, WrapErr};
use serenity::model::id::ChannelId;
use teloxide_core::Bot;
use teloxide_core::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide_core::prelude::Requester;
use teloxide_core::types::{ChatId, MessageId, ParseMode};

use crate::events::VoiceEvent;

use super::format::{format_event_message, format_status_message};
use super::state::{ChannelNames, PersistentState, SessionInfo, Sessions, WeeklyStats};

pub struct TelegramService {
    bot: Bot,
    chat_id: ChatId,
    state_chat_id: ChatId,
    tracked_channels: Vec<ChannelId>,
    state_message_id: Option<MessageId>,
    message_id: Option<MessageId>,
    sessions: Sessions,
    weekly_stats: WeeklyStats,
    channel_names: ChannelNames,
    last_message_text: Option<String>,
}

impl TelegramService {
    pub async fn new(
        bot: Bot,
        chat_id: ChatId,
        state_chat_id: ChatId,
        tracked_channels: Vec<ChannelId>,
    ) -> Result<Self> {
        let mut svc = Self {
            bot,
            chat_id,
            state_chat_id,
            tracked_channels,
            state_message_id: None,
            message_id: None,
            sessions: Sessions::new(),
            weekly_stats: WeeklyStats::default(),
            channel_names: ChannelNames::new(),
            last_message_text: None,
        };
        svc.load_state().await?;
        Ok(svc)
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

        // Update channel name cache
        self.channel_names
            .insert(update.channel_id, update.channel_name.clone());

        self.update_sessions(event);

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
        let message = format_event_message(
            event,
            &self.sessions,
            &self.tracked_channels,
            &self.channel_names,
        );

        // Delete old message, send new one (keeps it at bottom of chat)
        if let Some(old_msg_id) = self.message_id.take() {
            self.delete_message(old_msg_id).await;
        }

        if let Some(msg_id) = self.send_message(&message).await? {
            self.message_id = Some(msg_id);
            self.last_message_text = Some(message);
        }

        self.persist_state().await?;

        Ok(())
    }

    pub async fn handle_tick(&mut self) -> Result<()> {
        // Nothing to update if no one is in voice or no message exists
        let msg_id = match self.message_id {
            Some(id) if !self.sessions.is_empty() => id,
            _ => return Ok(()),
        };

        let message =
            format_status_message(&self.sessions, &self.tracked_channels, &self.channel_names);

        // Skip edit if content hasn't changed
        if self.last_message_text.as_deref() == Some(&message) {
            return Ok(());
        }

        tracing::debug!("duration tick: updating message");

        self.edit_message(msg_id, &message).await?;
        self.last_message_text = Some(message);

        Ok(())
    }

    fn update_sessions(&mut self, event: &VoiceEvent) {
        match event {
            VoiceEvent::Joined(u) | VoiceEvent::InitialState(u) => {
                self.sessions.insert(
                    u.username.clone(),
                    SessionInfo {
                        display_name: u.display_name.clone(),
                        channel_id: u.channel_id,
                        joined_at: Utc::now(),
                    },
                );
            }
            VoiceEvent::Left(u) => {
                if let Some(session) = self.sessions.remove(&u.username) {
                    let duration_secs = (Utc::now() - session.joined_at).num_seconds();
                    if duration_secs <= 0 {
                        return;
                    }

                    let user_stats = self
                        .weekly_stats
                        .users
                        .entry(u.username.clone())
                        .or_default();
                    user_stats.total_seconds += duration_secs;

                    tracing::debug!(
                        username = %u.username,
                        duration_secs,
                        total_secs = user_stats.total_seconds,
                        "accumulated voice time"
                    );
                }
            }
        }
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

    async fn edit_message(&self, message_id: MessageId, text: &str) -> Result<()> {
        tracing::debug!(chat_id = %self.chat_id, msg_id = message_id.0, "editing telegram message");
        match self
            .bot
            .edit_message_text(self.chat_id, message_id, text)
            .parse_mode(ParseMode::Html)
            .await
        {
            Ok(_) => {
                tracing::debug!(
                    chat_id = %self.chat_id,
                    msg_id = message_id.0,
                    "edited telegram message"
                );
            }
            Err(err) => {
                tracing::warn!(
                    chat_id = %self.chat_id,
                    msg_id = message_id.0,
                    %err,
                    "failed to edit telegram message"
                );
            }
        }
        Ok(())
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
            self.weekly_stats = state.weekly_stats.unwrap_or_default();
            self.channel_names = state.channel_names;

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
            weekly_stats: Some(self.weekly_stats.clone()),
            channel_names: self.channel_names.clone(),
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
