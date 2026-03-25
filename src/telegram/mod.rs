mod format;
mod notifier;
mod state;

use eyre::Result;
use secrecy::ExposeSecret;
use serenity::model::id::ChannelId;
use teloxide_core::Bot;
use tokio::sync::mpsc;

use crate::config::TelegramConfig;
use crate::events::VoiceEvent;

use self::notifier::Notifier;

#[tracing::instrument(name = "telegram", skip_all)]
pub async fn run(
    config: TelegramConfig,
    mut rx: mpsc::Receiver<VoiceEvent>,
    tracked_channels: Vec<ChannelId>,
) -> Result<()> {
    tracing::info!("telegram task started");
    let bot = Bot::new(config.token.expose_secret());
    let mut notifier =
        Notifier::new(bot, config.chat_id, config.state_chat_id, tracked_channels).await?;

    while let Some(event) = rx.recv().await {
        if let Err(err) = notifier.handle_event(&event).await {
            tracing::error!(%err, "failed to handle voice event");
        }
    }

    tracing::info!("channel closed, telegram task shutting down");
    Ok(())
}
