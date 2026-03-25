mod handler;

use eyre::{Result, WrapErr};
use secrecy::ExposeSecret;
use serenity::Client;
use serenity::model::gateway::GatewayIntents;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::config::DiscordConfig;
use crate::events::DiscordEvent;

use self::handler::Handler;

#[tracing::instrument(name = "discord", skip_all)]
pub async fn run(
    config: DiscordConfig,
    tx: mpsc::Sender<DiscordEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    let intents = GatewayIntents::GUILDS
        | GatewayIntents::GUILD_VOICE_STATES
        | GatewayIntents::GUILD_MESSAGES;

    let mut client = Client::builder(config.token.expose_secret(), intents)
        .event_handler(Handler { config, tx })
        .await
        .wrap_err("failed to create discord client")?;

    tracing::info!("discord task started");

    tokio::select! {
        result = client.start() => {
            result.wrap_err("discord client error")?;
        }
        () = cancel.cancelled() => {
            tracing::info!("discord task shutting down");
            client.shard_manager.shutdown_all().await;
        }
    }

    Ok(())
}
