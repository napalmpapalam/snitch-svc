use serenity::async_trait;
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId, UserId};
use serenity::model::voice::VoiceState;
use serenity::prelude::{Context, EventHandler};
use tokio::sync::mpsc;

use crate::config::DiscordConfig;
use crate::events::{ChannelName, ChannelUpdate, DisplayName, Username, VoiceEvent};

pub struct Handler {
    pub config: DiscordConfig,
    pub tx: mpsc::Sender<VoiceEvent>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        tracing::info!(user = %ready.user.name, "connected to discord");
    }

    async fn cache_ready(&self, ctx: Context, _guilds: Vec<GuildId>) {
        tracing::info!("guild cache populated, sending initial state");
        self.send_initial_state(&ctx).await;
    }

    async fn voice_state_update(&self, ctx: Context, old: Option<VoiceState>, new: VoiceState) {
        if new.guild_id != Some(self.config.target_guild_id) {
            return;
        }

        let old_channel = old.as_ref().and_then(|s| s.channel_id);
        let new_channel = new.channel_id;

        if old_channel == new_channel {
            return;
        }

        let (username, display_name) =
            resolve_display_names(&ctx, self.config.target_guild_id, &new);

        // Left old channel
        if let Some(channel_id) = old_channel
            && self.is_tracked(channel_id)
        {
            let update = build_channel_update(
                &ctx,
                self.config.target_guild_id,
                channel_id,
                &username,
                &display_name,
            );
            self.send_event(VoiceEvent::Left(update)).await;
        }

        // Joined new channel
        if let Some(channel_id) = new_channel
            && self.is_tracked(channel_id)
        {
            let update = build_channel_update(
                &ctx,
                self.config.target_guild_id,
                channel_id,
                &username,
                &display_name,
            );
            self.send_event(VoiceEvent::Joined(update)).await;
        }
    }
}

impl Handler {
    fn is_tracked(&self, channel_id: ChannelId) -> bool {
        self.config.tracked_channels.contains(&channel_id)
    }

    async fn send_event(&self, event: VoiceEvent) {
        if let Err(err) = self.tx.send(event).await {
            tracing::error!(%err, "failed to send voice event");
        }
    }

    async fn send_initial_state(&self, ctx: &Context) {
        let guild = match ctx.cache.guild(self.config.target_guild_id) {
            Some(g) => g.clone(),
            None => {
                tracing::warn!("guild not found in cache on ready");
                return;
            }
        };

        for &channel_id in &self.config.tracked_channels {
            let channel_name = guild
                .channels
                .get(&channel_id)
                .map(|c| ChannelName::new(&c.name))
                .unwrap_or_else(|| ChannelName::new(channel_id.to_string()));

            let voice_states: Vec<_> = guild
                .voice_states
                .values()
                .filter(|vs| vs.channel_id == Some(channel_id))
                .collect();

            for vs in voice_states {
                let (username, display_name) = resolve_display_names_from_guild(&guild, vs.user_id);

                tracing::info!(
                    channel = %channel_name,
                    username = %username,
                    "sending initial state for user"
                );

                let update = ChannelUpdate {
                    username,
                    display_name,
                    channel_name: channel_name.clone(),
                    channel_id,
                };
                self.send_event(VoiceEvent::InitialState(update)).await;
            }
        }
    }
}

/// Returns (username, display_name) from the guild cache.
fn resolve_display_names(
    ctx: &Context,
    guild_id: GuildId,
    voice_state: &VoiceState,
) -> (Username, DisplayName) {
    ctx.cache
        .guild(guild_id)
        .and_then(|guild| {
            guild.members.get(&voice_state.user_id).map(|member| {
                let username = Username::new(&member.user.name);
                let display = member.nick.as_deref().unwrap_or(&member.user.name);
                (username, DisplayName::new(display))
            })
        })
        .unwrap_or_else(|| {
            let id = voice_state.user_id.to_string();
            (Username::new(&id), DisplayName::new(&id))
        })
}

/// Returns (username, display_name) from a guild's member cache.
fn resolve_display_names_from_guild(
    guild: &serenity::model::guild::Guild,
    user_id: UserId,
) -> (Username, DisplayName) {
    guild
        .members
        .get(&user_id)
        .map(|member| {
            let username = Username::new(&member.user.name);
            let display = member.nick.as_deref().unwrap_or(&member.user.name);
            (username, DisplayName::new(display))
        })
        .unwrap_or_else(|| {
            let id = user_id.to_string();
            (Username::new(&id), DisplayName::new(&id))
        })
}

fn build_channel_update(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    username: &Username,
    display_name: &DisplayName,
) -> ChannelUpdate {
    let channel_name = ctx
        .cache
        .guild(guild_id)
        .and_then(|g| {
            g.channels
                .get(&channel_id)
                .map(|c| ChannelName::new(&c.name))
        })
        .unwrap_or_else(|| ChannelName::new(channel_id.to_string()));

    ChannelUpdate {
        username: username.clone(),
        display_name: display_name.clone(),
        channel_name,
        channel_id,
    }
}
