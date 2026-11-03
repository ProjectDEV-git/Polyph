//! Standalone relay binary. `polyph relay serve` runs the same code.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "polyph-relay", version, about = "Polyph relay: forwards end-to-end encrypted room traffic")]
struct Args {
    /// TOML configuration file. Environment variables (POLYPH_RELAY_*) override it.
    #[arg(long, env = "POLYPH_RELAY_CONFIG")]
    config: Option<std::path::PathBuf>,
    /// Address to listen on, e.g. 0.0.0.0:8787.
    #[arg(long)]
    bind: Option<std::net::SocketAddr>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .json()
        .init();
    let args = Args::parse();
    let mut cfg = polyph_relay::RelayConfig::load(args.config.as_deref())?;
    if let Some(b) = args.bind {
        cfg.bind = b;
    }
    let handle = polyph_relay::serve(cfg, Default::default()).await?;
    let stop = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    tokio::select! {
        _ = stop => {}
    }
    handle.shutdown().await;
    Ok(())
}
