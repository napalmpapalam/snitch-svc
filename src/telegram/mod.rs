use eyre::Result;
use tokio::sync::mpsc;

use crate::config::TelegramConfig;
use crate::events::VoiceEvent;

#[tracing::instrument(name = "telegram", skip_all)]
pub async fn run(config: TelegramConfig, mut rx: mpsc::Receiver<VoiceEvent>) -> Result<()> {
    tracing::info!("telegram task started");

    let _ = &config;

    while let Some(event) = rx.recv().await {
        handle_event(&event);
    }

    tracing::info!("channel closed, telegram task shutting down");
    Ok(())
}

fn handle_event(event: &VoiceEvent) {
    match event {
        VoiceEvent::Joined(update) => {
            tracing::info!(
                user = %update.user_name,
                channel = %update.channel_name,
                "user joined voice channel"
            );
        }
        VoiceEvent::Left(update) => {
            tracing::info!(
                user = %update.user_name,
                channel = %update.channel_name,
                "user left voice channel"
            );
        }
    }
}
