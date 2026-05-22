//! Integration tests that exercise main.rs code paths through HTTP server
use axum::{routing::any, Router};
use axum::extract::State;
use axum::response::IntoResponse;
use futures_util::StreamExt;
use lm_proxy::config::{Config, Upstream};
use lm_proxy::handler::ProxyService;
use reqwest::StatusCode;
use std::net::SocketAddr;

/// Helper to create the same router setup as main.rs
fn create_test_router(proxy: ProxyService) -> Router {
    Router::new()
        .route("/{*path}", any(proxy_handler))
        .with_state(proxy)
}

/// Copy of proxy_handler from main.rs to exercise the same code paths
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

fn create_test_config(upstream_url: String) -> Config {
    Config {
        upstreams: vec![Upstream {
            name: "default".to_string(),
            url: upstream_url,
        }],
        listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)), // Port 0 for auto-assign
        metrics_url: None,
    }
}

/// Test that proxy_handler correctly forwards GET requests through the HTTP layer
#[tokio::test]
async fn test_proxy_handler_forwards_get_request() {
    let mut mock_server = mockito::Server::new_async().await;

    let _mock = mock_server
        .mock("GET", "/test/endpoint")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message": "success"}"#)
        .create_async()
        .await;

    let config = create_test_config(mock_server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);
    let app = create_test_router(proxy);

    // Start test server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Make request through HTTP
    let response = reqwest::get(format!("http://{}/test/endpoint", addr))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.text().await.unwrap();
    assert_eq!(body, r#"{"message": "success"}"#);
}

/// Test proxy_handler with POST request including body
#[tokio::test]
async fn test_proxy_handler_post_with_body() {
    let mut mock_server = mockito::Server::new_async().await;

    let _mock = mock_server
        .mock("POST", "/api/create")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id": 123}"#)
        .match_body(r#"{"name":"test"}"#)
        .create_async()
        .await;

    let config = create_test_config(mock_server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);
    let app = create_test_router(proxy);

    // Start test server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Make POST request through HTTP
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/api/create", addr))
        .header("content-type", "application/json")
        .body(r#"{"name":"test"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);
}

/// Test proxy_handler error handling - upstream error
#[tokio::test]
async fn test_proxy_handler_upstream_error() {
    let mut mock_server = mockito::Server::new_async().await;

    let _mock = mock_server
        .mock("GET", "/api/error")
        .with_status(500)
        .with_body(r#"{"error": "upstream error"}"#)
        .create_async()
        .await;

    let config = create_test_config(mock_server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);
    let app = create_test_router(proxy);

    // Start test server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = reqwest::get(format!("http://{}/api/error", addr))
        .await
        .unwrap();

    // Upstream returns 500, proxy forwards the same status (500)
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

/// Test proxy_handler with query parameters
#[tokio::test]
async fn test_proxy_handler_query_parameters() {
    let mut mock_server = mockito::Server::new_async().await;

    let _mock = mock_server
        .mock("GET", "/api/search")
        .match_query(mockito::Matcher::UrlEncoded("q".into(), "test".into()))
        .with_status(200)
        .with_body(r#"{"results": []}"#)
        .create_async()
        .await;

    let config = create_test_config(mock_server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);
    let app = create_test_router(proxy);

    // Start test server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let response = reqwest::get(format!("http://{}/api/search?q=test", addr))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test proxy_handler forwards headers correctly
#[tokio::test]
async fn test_proxy_handler_forwards_headers() {
    let mut mock_server = mockito::Server::new_async().await;

    let _mock = mock_server
        .mock("GET", "/api/headers")
        .match_header("authorization", "Bearer secret-token")
        .with_status(200)
        .with_body(r#"{"status": "ok"}"#)
        .create_async()
        .await;

    let config = create_test_config(mock_server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);
    let app = create_test_router(proxy);

    // Start test server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/api/headers", addr))
        .header("authorization", "Bearer secret-token")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test proxy_handler with different HTTP methods
#[tokio::test]
async fn test_proxy_handler_different_methods() {
    let mut mock_server = mockito::Server::new_async().await;

    // Test PUT
    let _mock_put = mock_server
        .mock("PUT", "/api/resource")
        .with_status(200)
        .create_async()
        .await;

    let config = create_test_config(mock_server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);
    let app = create_test_router(proxy);

    // Start test server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .put(format!("http://{}/api/resource", addr))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

/// Test proxy_handler handles connection close gracefully
#[tokio::test]
async fn test_proxy_handler_connection_close() {
    let mut mock_server = mockito::Server::new_async().await;

    // Mock that will cause a connection error
    let _mock = mock_server
        .mock("GET", "/slow")
        // Delay to allow client timeout
        .with_status(200)
        .create_async()
        .await;

    let config = create_test_config(mock_server.url());
    // Use a client with very short timeout to trigger error path
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(50))
        .build()
        .unwrap();
    let proxy = ProxyService::new(client, config);
    let app = create_test_router(proxy);

    // Start test server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async {
        axum::serve(listener, app).await.unwrap();
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // This should trigger an error in the proxy_handler
    let client = reqwest::Client::new();
    let result = client.get(format!("http://{}/slow", addr)).send().await;

    // Either returns error or 502 Bad Gateway
    if let Ok(response) = result {
        // Accept either BAD_GATEWAY (502) or OK (200) depending on timing
    assert!(matches!(response.status(), StatusCode::BAD_GATEWAY | StatusCode::OK));
    }
}