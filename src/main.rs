mod config;

use clap::{Parser, Subcommand};
use eyre::Result;
use tracing_subscriber::EnvFilter;

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

    let config = config::Config::load()?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(config.log.level.into())
                .from_env_lossy(),
        )
        .init();

    match cli.command {
        Command::Run {
            service: Service::All | Service::Snitch,
        } => {
            tracing::info!("starting snitch service");
            tracing::debug!(?config, "loaded configuration");
            tokio::signal::ctrl_c().await?;
            tracing::info!("shutting down");
        }
    }

    Ok(())
}
