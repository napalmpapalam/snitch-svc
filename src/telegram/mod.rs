mod format;
mod service;
mod state;

use std::time::Duration;

use eyre::Result;
use secrecy::ExposeSecret;
use serenity::model::id::ChannelId;
use teloxide_core::Bot;
use tokio::sync::mpsc;

use crate::config::TelegramConfig;
use crate::events::VoiceEvent;

use self::service::TelegramService;

#[tracing::instrument(name = "telegram", skip_all)]
pub async fn run(
    config: TelegramConfig,
    mut rx: mpsc::Receiver<VoiceEvent>,
    tracked_channels: Vec<ChannelId>,
) -> Result<()> {
    tracing::info!("telegram task started");
    let bot = Bot::new(config.token.expose_secret());
    let mut svc =
        TelegramService::new(bot, config.chat_id, config.state_chat_id, tracked_channels).await?;

    let mut tick = tokio::time::interval(Duration::from_secs(config.duration_tick_minutes * 60));
    // First tick fires immediately — skip it
    tick.tick().await;

    loop {
        tokio::select! {
            event = rx.recv() => {
                match event {
                    Some(event) => {
                        if let Err(err) = svc.handle_event(&event).await {
                            tracing::error!(%err, "failed to handle voice event");
                        }
                    }
                    None => break,
                }
            }
            _ = tick.tick() => {
                if let Err(err) = svc.handle_tick().await {
                    tracing::error!(%err, "failed to handle duration tick");
                }
            }
        }
    }

    tracing::info!("channel closed, telegram task shutting down");
    Ok(())
}
