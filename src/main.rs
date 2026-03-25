mod config;
mod discord;
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

    let discord_handle = tokio::spawn(discord::run(config.discord, tx, cancel.clone()));
    let telegram_handle = tokio::spawn(telegram::run(config.telegram, rx));

    tokio::signal::ctrl_c().await?;
    tracing::info!("received shutdown signal");

    // Signal the discord task to shut down gracefully;
    // it drops the sender, closing the channel so the telegram task drains and exits
    cancel.cancel();

    if let Err(err) = discord_handle.await? {
        tracing::error!(%err, "discord task failed");
    }
    if let Err(err) = telegram_handle.await? {
        tracing::error!(%err, "telegram task failed");
    }

    tracing::info!("shutdown complete");
    Ok(())
}
