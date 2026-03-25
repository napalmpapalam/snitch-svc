use serenity::async_trait;
use serenity::model::gateway::Ready;
use serenity::model::id::{ChannelId, GuildId};
use serenity::model::voice::VoiceState;
use serenity::prelude::{Context, EventHandler};
use tokio::sync::mpsc;

use crate::config::DiscordConfig;
use crate::events::{ChannelUpdate, VoiceEvent};

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

        // Same channel or no channel change → mute/deaf/video toggle, ignore
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
            let members: Vec<String> = guild
                .voice_states
                .values()
                .filter(|vs| vs.channel_id == Some(channel_id))
                .map(|vs| resolve_member_name(&guild, vs.user_id))
                .collect();

            if members.is_empty() {
                continue;
            }

            let channel_name = guild
                .channels
                .get(&channel_id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| channel_id.to_string());

            tracing::info!(
                channel = %channel_name,
                member_count = members.len(),
                "sending initial state for channel"
            );

            let update = ChannelUpdate {
                username: String::new(),
                display_name: String::new(),
                channel_name,
                channel_id,
                members,
            };
            self.send_event(VoiceEvent::InitialState(update)).await;
        }
    }
}

/// Returns (raw_username, display_name)
fn resolve_display_names(
    ctx: &Context,
    guild_id: GuildId,
    voice_state: &VoiceState,
) -> (String, String) {
    ctx.cache
        .guild(guild_id)
        .and_then(|guild| {
            guild.members.get(&voice_state.user_id).map(|member| {
                let username = member.user.name.clone();
                let display = member.nick.as_deref().unwrap_or(&username).to_owned();
                (username, display)
            })
        })
        .unwrap_or_else(|| {
            let id = voice_state.user_id.to_string();
            (id.clone(), id)
        })
}

fn resolve_member_name(
    guild: &serenity::model::guild::Guild,
    user_id: serenity::model::id::UserId,
) -> String {
    guild
        .members
        .get(&user_id)
        .map(|member| {
            member
                .nick
                .as_deref()
                .unwrap_or(&member.user.name)
                .to_owned()
        })
        .unwrap_or_else(|| user_id.to_string())
}

fn build_channel_update(
    ctx: &Context,
    guild_id: GuildId,
    channel_id: ChannelId,
    username: &str,
    display_name: &str,
) -> ChannelUpdate {
    let guild = ctx.cache.guild(guild_id);

    let channel_name = guild
        .as_ref()
        .and_then(|g| g.channels.get(&channel_id))
        .map(|c| c.name.clone())
        .unwrap_or_else(|| channel_id.to_string());

    let members: Vec<String> = guild
        .as_ref()
        .map(|g| {
            g.voice_states
                .values()
                .filter(|vs| vs.channel_id == Some(channel_id))
                .map(|vs| resolve_member_name(g, vs.user_id))
                .collect()
        })
        .unwrap_or_default();

    ChannelUpdate {
        username: username.to_owned(),
        display_name: display_name.to_owned(),
        channel_name,
        channel_id,
        members,
    }
}
