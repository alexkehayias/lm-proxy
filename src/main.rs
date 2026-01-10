mod config;
mod handler;
mod models;

use axum::{routing::any, Router};
use axum::extract::State;
use axum::response::IntoResponse;
use clap::Parser;
use config::{Args, Config};
use handler::ProxyService;
use futures_util::StreamExt;

async fn proxy_handler(
    State(proxy): State<ProxyService>,
    mut req: axum::extract::Request,
) -> axum::response::Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let headers = std::mem::take(req.headers_mut());

    // Collect request body bytes
    let mut body_stream = req.into_body().into_data_stream();
    let mut body_bytes: Vec<u8> = vec![];
    while let Some(chunk_result) = body_stream.next().await {
        match chunk_result {
            Ok(bytes) => body_bytes.extend_from_slice(&bytes),
            Err(e) => {
                return (axum::http::StatusCode::BAD_REQUEST, format!("Failed to read body: {}", e))
                    .into_response();
            }
        }
    }

    match proxy.forward_request(method, uri, headers, body_bytes).await {
        Ok(resp) => resp,
        Err(e) => (axum::http::StatusCode::BAD_GATEWAY, format!("Proxy error: {}", e)).into_response(),
    }
}

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

    let proxy = ProxyService::new(reqwest::Client::new(), config.clone());

    let app = Router::new()
        .route("/{*path}", any(proxy_handler))
        .with_state(proxy);

    let addr = config.listen_addr;
    log::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}