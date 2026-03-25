mod config;
mod discord;
mod emoji;
mod events;
mod telegram;

use clap::{Parser, Subcommand};
use eyre::Result;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

const EVENT_CHANNEL_CAPACITY: usize = 32;

#[derive(Parser)]
#[command(name = "snitch")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the service
    Run {
        #[command(subcommand)]
        service: Service,
    },
}

#[derive(Subcommand)]
enum Service {
    /// Run all services
    All,
    /// Run snitch service
    Snitch,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    let config = Config::load()?;

    init_logging(&config.log);

    match cli.command {
        Command::Run {
            service: Service::All | Service::Snitch,
        } => run_service(config).await?,
    }

    Ok(())
}

fn init_logging(log_config: &config::LogConfig) {
    let mut filter = EnvFilter::builder()
        .with_default_directive(log_config.level.into())
        .from_env_lossy();

    for directive in &log_config.filter {
        if let Ok(d) = directive.parse() {
            filter = filter.add_directive(d);
        } else {
            eprintln!("invalid log filter directive: '{directive}'");
        }
    }

    tracing_subscriber::fmt().with_env_filter(filter).init();
}

async fn run_service(config: Config) -> Result<()> {
    tracing::info!("starting snitch service");
    tracing::debug!(?config, "loaded configuration");

    let (tx, rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
    let cancel = CancellationToken::new();

    let mut discord_handle = tokio::spawn(discord::run(config.discord, tx, cancel.clone()));
    let mut telegram_handle = tokio::spawn(telegram::run(config.telegram, rx));

    // Wait for shutdown signal or early task failure
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received shutdown signal");
        }
        result = &mut discord_handle => {
            match result {
                Ok(Err(err)) => tracing::error!(%err, "discord task crashed"),
                Err(err) => tracing::error!(%err, "discord task panicked"),
                Ok(Ok(())) => tracing::warn!("discord task exited unexpectedly"),
            }
        }
        result = &mut telegram_handle => {
            match result {
                Ok(Err(err)) => tracing::error!(%err, "telegram task crashed"),
                Err(err) => tracing::error!(%err, "telegram task panicked"),
                Ok(Ok(())) => tracing::warn!("telegram task exited unexpectedly"),
            }
        }
    }

    // Signal the discord task to shut down gracefully;
    // it drops the sender, closing the channel so the telegram task drains and exits
    cancel.cancel();

    // Wait for remaining tasks (ignore errors — already logged above)
    let _ = discord_handle.await;
    let _ = telegram_handle.await;

    tracing::info!("shutdown complete");
    Ok(())
}
