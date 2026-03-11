mod config;
mod handler;
mod models;
mod server;

use clap::Parser;
use config::{Args, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let config: Config = args.into_config()?;

    env_logger::init();
    log::info!("Starting lm-proxy...");
    log::info!(
        "Proxy configured: upstream={} listen={}",
        config.upstream_url,
        config.listen_addr
    );

    server::run(config).await
}