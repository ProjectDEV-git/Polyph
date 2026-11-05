//! `polyph-cloud serve`: the hosted relay and the account API.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "polyph-cloud", version, about = "Polyph's hosted service")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the account API (and the hosted relay when `[relay]` is set).
    Serve {
        #[arg(long, env = "POLYPH_CLOUD_CONFIG")]
        config: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .json()
        .init();
    match Cli::parse().cmd {
        Cmd::Serve { config } => {
            let cfg = polyph_cloud::CloudConfig::load(config.as_deref())?;
            let h = polyph_cloud::serve(cfg, Default::default()).await?;
            tracing::info!(api = %h.api_addr, relay = ?h.relay.as_ref().map(|r| r.addr), "polyph-cloud running");
            tokio::signal::ctrl_c().await?;
            h.shutdown().await;
            Ok(())
        }
    }
}
