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
    for upstream in &config.upstreams {
        tracing::info!(
            "Upstream configured: name={} url={}",
            upstream.name,
            upstream.url
        );
    }
    tracing::info!("Listening on {}", config.listen_addr);

    server::run(config).await
}