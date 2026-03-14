mod config;
mod handler;
mod models;
mod server;

use clap::Parser;
use config::{Args, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let config: Config = args.into_config()?;

    tracing::info!("Starting lm-proxy...");
    tracing::info!(
        "Proxy configured: upstream={} listen={}",
        config.upstream_url,
        config.listen_addr
    );

    server::run(config).await
}