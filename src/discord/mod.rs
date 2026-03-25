use eyre::Result;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::config::DiscordConfig;
use crate::events::VoiceEvent;

#[tracing::instrument(name = "discord", skip_all)]
pub async fn run(
    config: DiscordConfig,
    tx: mpsc::Sender<VoiceEvent>,
    cancel: CancellationToken,
) -> Result<()> {
    tracing::info!("discord task started");

    // TODO: connect to Discord gateway and listen for voice state updates
    let _ = &config;
    let _ = &tx;
    cancel.cancelled().await;

    tracing::info!("discord task shutting down");
    Ok(())
}
