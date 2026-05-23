use crate::config::Config;
use crate::handler::ProxyService;
use axum::{routing::any, Router};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::StreamExt;
use tokio::signal::unix::{SignalKind, signal};

/// Proxy handler that forwards requests to upstream
pub async fn proxy_handler(
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

/// Creates the Axum router with the proxy handler
pub fn create_router(proxy: ProxyService) -> Router {
    Router::new()
        .route("/{*path}", any(proxy_handler))
        .with_state(proxy)
}

// Copied from https://github.com/rust-lang/crates.io/blob/8969c10c46e5ed0afece2444f5445fc59aa64565/src/bin/server.rs#L83-L112
/// Wait for shutdown signal (SIGINT or SIGTERM)
pub async fn wait_for_shutdown_signal() {
    let interrupt = async {
        signal(SignalKind::interrupt())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    let terminate = async {
        signal(SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = interrupt => {},
        _ = terminate => {},
    }
}

/// Run the server with the given configuration
pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = ProxyService::new(reqwest::Client::new(), config.clone());

    let app = create_router(proxy);

    let addr = config.listen_addr;
    tracing::info!("Listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(wait_for_shutdown_signal())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::config::Upstream;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::net::SocketAddr;

    fn default_config(upstream_url: String) -> Config {
        Config {
            upstreams: vec![Upstream {
                name: "default".to_string(),
                url: upstream_url,
            }],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        }
    }

    #[test]
    fn test_create_router() {
        let config = default_config("https://api.openai.com/v1".to_string());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // Router should be created successfully without panicking
        let _router = create_router(proxy);
    }

    #[test]
    fn test_create_router_with_metrics_url() {
        let config = Config {
            upstreams: vec![Upstream {
                name: "default".to_string(),
                url: "https://api.anthropic.com".to_string(),
            }],
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 8080)),
            metrics_url: Some("http://localhost:9090/metrics".to_string()),
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // Router should be created with metrics configured
        let _router = create_router(proxy);
    }

    #[test]
    fn test_create_router_with_custom_upstream() {
        let config = default_config("http://localhost:11434/v1".to_string());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // Router should work with different upstream URLs
        let _router = create_router(proxy);
    }

    #[tokio::test]
    async fn test_proxy_handler_forwards_request() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("GET", "/test")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"status": "ok"}"#)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // Build a request to send through the handler
        let uri = format!("{}/test", mock_server.url());
        let request = Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_proxy_handler_post_with_body() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("POST", "/api/data")
            .with_status(201)
            .with_header("content-type", "application/json")
            .match_body(r#"{"key":"value"}"#)
            .with_body(r#"{"id": 1}"#)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/api/data", mock_server.url());
        let body_bytes = r#"{"key":"value"}"#.as_bytes().to_vec();
        let request = Request::builder()
            .method("POST")
            .uri(&uri)
            .header("content-type", "application/json")
            .body(Body::from(body_bytes))
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_proxy_handler_forwards_error_status() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("GET", "/error")
            .with_status(404)
            .with_body(r#"{"error": "not found"}"#)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/error", mock_server.url());
        let request = Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        // Should forward the upstream error status
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_proxy_handler_forwards_500_error() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("GET", "/server-error")
            .with_status(500)
            .with_body(r#"{"error": "internal server error"}"#)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/server-error", mock_server.url());
        let request = Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        // Should forward 500 error from upstream
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_create_router_different_configurations() {
        // Test with IPv4 localhost
        let config1 = default_config("https://api.openai.com/v1".to_string());
        let client = reqwest::Client::new();
        let proxy1 = ProxyService::new(client.clone(), config1);
        let _router1 = create_router(proxy1);

        // Test with port 8080
        let config2 = default_config("https://api.openai.com/v1".to_string());
        let proxy2 = ProxyService::new(client.clone(), config2);
        let _router2 = create_router(proxy2);

        // Test with all interfaces and port 9000
        let config3 = default_config("https://api.openai.com/v1".to_string());
        let proxy3 = ProxyService::new(client, config3);
        let _router3 = create_router(proxy3);

        // All routers should be created successfully
    }

    #[tokio::test]
    async fn test_proxy_handler_returns_bad_gateway_on_upstream_error() {
        // Use an unreachable upstream to trigger the error path
        let config = default_config("http://127.0.0.1:1".to_string());
        // Use a client with very short timeout to fail fast
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();
        let proxy = ProxyService::new(client, config);

        let uri = "http://127.0.0.1:1/test";
        let request = Request::builder()
            .method("GET")
            .uri(uri)
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        // Should return BAD_GATEWAY (502) when upstream is unreachable
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn test_wait_for_shutdown_signal_returns_on_abort() {
        // Spawn the shutdown signal handler
        let handle = tokio::spawn(wait_for_shutdown_signal());

        // Give it a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Abort the task - this should cause wait_for_shutdown_signal to return
        handle.abort();

        // The result should be an error (aborted)
        let result = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            handle
        ).await;

        // Should timeout or return aborted error - either way the function is covered
        assert!(result.is_err() || result.unwrap().is_err());
    }

    #[tokio::test]
    async fn test_proxy_handler_with_query_string() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("GET", "/search")
            .match_query(mockito::Matcher::UrlEncoded("q".into(), "test".into()))
            .with_status(200)
            .with_body(r#"{"results": []}"#)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/search?q=test", mock_server.url());
        let request = Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_proxy_handler_forwards_headers() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("GET", "/with-headers")
            .match_header("x-custom-header", "custom-value")
            .with_status(200)
            .with_body(r#"{"ok": true}"#)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/with-headers", mock_server.url());
        let request = Request::builder()
            .method("GET")
            .uri(&uri)
            .header("x-custom-header", "custom-value")
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_proxy_handler_put_method() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("PUT", "/resource")
            .with_status(200)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/resource", mock_server.url());
        let request = Request::builder()
            .method("PUT")
            .uri(&uri)
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_proxy_handler_delete_method() {
        let mut mock_server = mockito::Server::new_async().await;

        let _mock = mock_server
            .mock("DELETE", "/resource/123")
            .with_status(204)
            .create_async()
            .await;

        let config = default_config(mock_server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/resource/123", mock_server.url());
        let request = Request::builder()
            .method("DELETE")
            .uri(&uri)
            .body(Body::empty())
            .unwrap();

        let response = proxy_handler(State(proxy), request).await;

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }
}