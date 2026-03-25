use std::collections::HashSet;

use chrono::{Datelike, NaiveDate, Timelike, Utc};
use chrono_tz::Europe::Kyiv;
use eyre::{Result, WrapErr};
use serenity::model::id::ChannelId;
use teloxide_core::Bot;
use teloxide_core::payloads::{EditMessageTextSetters, SendMessageSetters};
use teloxide_core::prelude::Requester;
use teloxide_core::types::{ChatId, MessageId, ParseMode};

use crate::events::{Username, VoiceEvent};

use super::achievements::{cleanup_stale, detect_event_achievements, detect_tick_achievements};
use super::format::{format_digest, format_event_message, format_status_message};
use super::state::current_week_start;
use super::state::{
    ChannelNames, PersistentState, RecentLeave, RecentLeaves, SessionInfo, Sessions,
    ShownAchievements, WeeklyStats,
};

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
    shown_achievements: ShownAchievements,
    recent_leaves: RecentLeaves,
    last_digest_sent: Option<NaiveDate>,
    last_message_text: Option<String>,
    /// Users seen in InitialState events (transient, for reconciliation).
    initial_state_users: HashSet<Username>,
    /// Whether reconciliation has been performed after initial state.
    initial_state_done: bool,
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
            shown_achievements: ShownAchievements::new(),
            recent_leaves: RecentLeaves::new(),
            last_digest_sent: None,
            last_message_text: None,
            initial_state_users: HashSet::new(),
            initial_state_done: false,
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

        // Capture pre-update channel member count (for Party starter detection)
        let pre_channel_count = self
            .sessions
            .values()
            .filter(|s| s.channel_id == update.channel_id)
            .count();

        // Update sessions, capture removed session for Speed run detection
        let removed_session = self.update_sessions(event);

        // Don't send messages for initial state — just populate sessions
        if event.is_initial_state() {
            tracing::debug!(
                username = %update.username,
                channel_id = %update.channel_id,
                "populated session from initial state"
            );
            return Ok(());
        }

        // Reconcile on first real event after initial state
        if !self.initial_state_done {
            self.reconcile_stale_sessions();
            self.initial_state_done = true;
        }

        // Detect achievements
        let achievements = detect_event_achievements(
            event,
            &self.sessions,
            &self.recent_leaves,
            pre_channel_count,
            removed_session.as_ref(),
        );

        // Format combined message
        let message = format_event_message(
            event,
            &self.sessions,
            &self.tracked_channels,
            &self.channel_names,
            &achievements,
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
        // Clean stale data
        cleanup_stale(&mut self.shown_achievements, &mut self.recent_leaves);

        // Check weekly digest trigger
        self.check_weekly_digest().await?;

        // Update status message if anyone is in voice
        if let Some(msg_id) = self.message_id
            && !self.sessions.is_empty()
        {
            let achievements =
                detect_tick_achievements(&self.sessions, &mut self.shown_achievements);

            let message = format_status_message(
                &self.sessions,
                &self.tracked_channels,
                &self.channel_names,
                &achievements,
            );

            // Skip edit if content hasn't changed
            if self.last_message_text.as_deref() != Some(&message) {
                tracing::debug!("duration tick: updating message");
                self.edit_message(msg_id, &message).await?;
                self.last_message_text = Some(message);
            }

            if !achievements.is_empty() {
                self.persist_state().await?;
            }
        }

        Ok(())
    }

    async fn check_weekly_digest(&mut self) -> Result<()> {
        let now_kyiv = Utc::now().with_timezone(&Kyiv);
        let today = now_kyiv.date_naive();

        // Must be Monday, 9:00+ Kyiv time
        if now_kyiv.weekday() != chrono::Weekday::Mon || now_kyiv.hour() < 9 {
            return Ok(());
        }

        // Already sent today
        if self.last_digest_sent == Some(today) {
            return Ok(());
        }

        // Nobody had voice time
        let has_activity = self
            .weekly_stats
            .users
            .values()
            .any(|s| s.total_seconds > 0);
        if !has_activity {
            return Ok(());
        }

        let week_start = self.weekly_stats.week_start;
        let week_end = week_start + chrono::Duration::days(6);

        let message = format_digest(&self.weekly_stats, week_start, week_end);

        tracing::info!("sending weekly digest");
        self.send_message(&message).await?;

        // Rotate stats: total → prev, reset totals
        for user_stats in self.weekly_stats.users.values_mut() {
            user_stats.prev_week_seconds = user_stats.total_seconds;
            user_stats.total_seconds = 0;
        }
        self.weekly_stats.week_start = current_week_start();
        self.last_digest_sent = Some(today);

        self.persist_state().await?;

        Ok(())
    }

    /// Updates sessions and returns the removed session (if any, on Leave).
    fn update_sessions(&mut self, event: &VoiceEvent) -> Option<SessionInfo> {
        match event {
            VoiceEvent::InitialState(u) => {
                self.initial_state_users.insert(u.username.clone());
                // Keep persisted joined_at if user already in sessions (survived restart)
                if let Some(session) = self.sessions.get_mut(&u.username) {
                    session.display_name = u.display_name.clone();
                    session.channel_id = u.channel_id;
                } else {
                    // New user (joined while service was down)
                    self.sessions.insert(
                        u.username.clone(),
                        SessionInfo {
                            display_name: u.display_name.clone(),
                            channel_id: u.channel_id,
                            joined_at: Utc::now(),
                        },
                    );
                }
                None
            }
            VoiceEvent::Joined(u) => {
                self.sessions.insert(
                    u.username.clone(),
                    SessionInfo {
                        display_name: u.display_name.clone(),
                        channel_id: u.channel_id,
                        joined_at: Utc::now(),
                    },
                );
                None
            }
            VoiceEvent::Left(u) => {
                let removed = self.sessions.remove(&u.username);
                if let Some(ref session) = removed {
                    let duration_secs = (Utc::now() - session.joined_at).num_seconds();

                    // Track recent leave for Boomerang/Channel hopper
                    self.recent_leaves.insert(
                        u.username.clone(),
                        RecentLeave {
                            channel_id: u.channel_id,
                            left_at: Utc::now(),
                        },
                    );

                    if duration_secs <= 0 {
                        return removed;
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
                removed
            }
        }
    }

    /// Removes users from sessions who were persisted but not seen in InitialState
    /// (they left while the service was down). Accumulates their voice time.
    fn reconcile_stale_sessions(&mut self) {
        let stale_users: Vec<Username> = self
            .sessions
            .keys()
            .filter(|u| !self.initial_state_users.contains(u))
            .cloned()
            .collect();

        for username in &stale_users {
            if let Some(session) = self.sessions.remove(username) {
                let duration_secs = (Utc::now() - session.joined_at).num_seconds();
                if duration_secs > 0 {
                    let user_stats = self.weekly_stats.users.entry(username.clone()).or_default();
                    user_stats.total_seconds += duration_secs;
                }

                tracing::info!(
                    username = %username,
                    "removed stale session (user left while service was down)"
                );
            }
        }

        // Clear transient state
        self.initial_state_users.clear();
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
            self.shown_achievements = state.shown_achievements;
            self.recent_leaves = state.recent_leaves;
            self.last_digest_sent = state.last_digest_sent;

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
            shown_achievements: self.shown_achievements.clone(),
            recent_leaves: self.recent_leaves.clone(),
            last_digest_sent: self.last_digest_sent,
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
