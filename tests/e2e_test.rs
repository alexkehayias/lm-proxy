use axum::body::to_bytes;
use hyper::header::{HeaderMap, HeaderValue};
use lm_proxy::config::Config;
use lm_proxy::handler::ProxyService;
use reqwest::StatusCode;

/// Helper function to create a test config with the mock server URL
fn create_test_config(upstream_url: String) -> Config {
    Config {
        upstream_url,
        listen_addr: std::net::SocketAddr::from(([0, 0, 0, 0], 3000)),
    }
}

#[tokio::test]
async fn test_proxy_forwards_get_request() {
    let mut server = mockito::Server::new_async().await;

    // Set up a mock endpoint on the server
    let mock = server
        .mock("GET", "/test/endpoint")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"message": "success"}"#)
        .create_async()
        .await;

    // Create proxy service configured to forward to mock server
    let config = create_test_config(server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);

    // Make request through the proxy
    let uri = "http://proxy.example.com/test/endpoint"
        .parse::<hyper::Uri>()
        .unwrap();
    let headers = HeaderMap::new();

    let response = proxy
        .forward_request(hyper::Method::GET, uri, headers, vec![])
        .await
        .expect("Request should succeed");

    // Verify response status and body
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("Should be able to read body");
    assert_eq!(body, r#"{"message": "success"}"#);

    // Verify the mock server received exactly one request
    mock.assert_async().await;
}

#[tokio::test]
async fn test_proxy_forwards_post_request_with_body() {
    let mut server = mockito::Server::new_async().await;

    // Set up a mock endpoint expecting POST with specific body
    let mock = server
        .mock("POST", "/api/create")
        .with_status(201)
        .with_header("content-type", "application/json")
        .with_body(r#"{"id": 123, "status": "created"}"#)
        .match_body(r#"{"name": "test"}"#)
        .create_async()
        .await;

    // Create proxy service
    let config = create_test_config(server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);

    // Make POST request through the proxy
    let uri = "http://proxy.example.com/api/create"
        .parse::<hyper::Uri>()
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("application/json"),
    );

    let body_bytes = r#"{"name": "test"}"#.as_bytes().to_vec();

    let response = proxy
        .forward_request(hyper::Method::POST, uri, headers, body_bytes)
        .await
        .expect("Request should succeed");

    // Verify response
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("Should be able to read body");
    assert_eq!(body, r#"{"id": 123, "status": "created"}"#);

    // Verify the mock server received exactly one request
    mock.assert_async().await;
}

#[tokio::test]
async fn test_proxy_forwards_headers() {
    let mut server = mockito::Server::new_async().await;

    // Set up a mock endpoint expecting specific headers
    let mock = server
        .mock("GET", "/api/authorized")
        .with_status(200)
        .match_header("authorization", "Bearer secret-token")
        .match_header("x-custom-header", "custom-value")
        .create_async()
        .await;

    // Create proxy service
    let config = create_test_config(server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);

    // Make request with custom headers
    let uri = "http://proxy.example.com/api/authorized"
        .parse::<hyper::Uri>()
        .unwrap();
    let mut headers = HeaderMap::new();
    headers.insert(
        "authorization",
        HeaderValue::from_static("Bearer secret-token"),
    );
    headers.insert(
        "x-custom-header",
        HeaderValue::from_static("custom-value"),
    );

    let response = proxy
        .forward_request(hyper::Method::GET, uri, headers, vec![])
        .await
        .expect("Request should succeed");

    // Verify response status
    assert_eq!(response.status(), StatusCode::OK);

    // Verify the mock server received exactly one request with correct headers
    mock.assert_async().await;
}

#[tokio::test]
async fn test_proxy_forwards_query_parameters() {
    let mut server = mockito::Server::new_async().await;

    // Set up a mock endpoint expecting specific query parameters
    let mock = server
        .mock("GET", "/api/search")
        .match_query(mockito::Matcher::UrlEncoded("q".into(), "test".into()))
        .match_query(mockito::Matcher::UrlEncoded("limit".into(), "10".into()))
        .with_status(200)
        .with_body(r#"{"results": []}"#)
        .create_async()
        .await;

    // Create proxy service
    let config = create_test_config(server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);

    // Make request with query parameters
    let uri = "http://proxy.example.com/api/search?q=test&limit=10"
        .parse::<hyper::Uri>()
        .unwrap();
    let headers = HeaderMap::new();

    let response = proxy
        .forward_request(hyper::Method::GET, uri, headers, vec![])
        .await
        .expect("Request should succeed");

    // Verify response
    assert_eq!(response.status(), StatusCode::OK);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("Should be able to read body");
    assert_eq!(body, r#"{"results": []}"#);

    // Verify the mock server received exactly one request
    mock.assert_async().await;
}

#[tokio::test]
async fn test_proxy_handles_upstream_error() {
    let mut server = mockito::Server::new_async().await;

    // Set up a mock endpoint that returns an error
    let mock = server
        .mock("GET", "/api/error")
        .with_status(500)
        .with_body(r#"{"error": "internal server error"}"#)
        .create_async()
        .await;

    // Create proxy service
    let config = create_test_config(server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);

    // Make request through the proxy
    let uri = "http://proxy.example.com/api/error"
        .parse::<hyper::Uri>()
        .unwrap();
    let headers = HeaderMap::new();

    let response = proxy
        .forward_request(hyper::Method::GET, uri, headers, vec![])
        .await
        .expect("Request should succeed (even with upstream error)");

    // Verify the proxy forwards the error status
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    let body = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("Should be able to read body");
    assert_eq!(body, r#"{"error": "internal server error"}"#);

    // Verify the mock server received exactly one request
    mock.assert_async().await;
}

#[tokio::test]
async fn test_proxy_handles_different_http_methods() {
    let mut server = mockito::Server::new_async().await;

    // Test PUT method
    let put_mock = server
        .mock("PUT", "/api/update")
        .with_status(200)
        .create_async()
        .await;

    let config = create_test_config(server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client.clone(), config);

    let uri = "http://proxy.example.com/api/update"
        .parse::<hyper::Uri>()
        .unwrap();

    let response = proxy
        .forward_request(hyper::Method::PUT, uri.clone(), HeaderMap::new(), vec![])
        .await
        .expect("PUT request should succeed");
    assert_eq!(response.status(), StatusCode::OK);
    put_mock.assert_async().await;

    // Test DELETE method
    let delete_mock = server
        .mock("DELETE", "/api/delete")
        .with_status(204)
        .create_async()
        .await;

    let config = create_test_config(server.url());
    let proxy = ProxyService::new(client, config);

    let uri = "http://proxy.example.com/api/delete"
        .parse::<hyper::Uri>()
        .unwrap();

    let response = proxy
        .forward_request(hyper::Method::DELETE, uri, HeaderMap::new(), vec![])
        .await
        .expect("DELETE request should succeed");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    delete_mock.assert_async().await;
}

#[tokio::test]
async fn test_proxy_filters_hop_by_hop_headers() {
    let mut server = mockito::Server::new_async().await;

    // Set up a mock that verifies hop-by-hop headers are NOT forwarded
    let mock = server
        .mock("GET", "/api/test")
        .match_header("x-custom-header", "present")
        .with_status(200)
        .create_async()
        .await;

    // Create proxy service
    let config = create_test_config(server.url());
    let client = reqwest::Client::new();
    let proxy = ProxyService::new(client, config);

    // Make request with both hop-by-hop and custom headers
    let uri = "http://proxy.example.com/api/test"
        .parse::<hyper::Uri>()
        .unwrap();
    let mut headers = HeaderMap::new();

    // Add hop-by-hop headers (should be filtered out)
    headers.insert(
        "connection",
        HeaderValue::from_static("keep-alive"),
    );
    headers.insert(
        "transfer-encoding",
        HeaderValue::from_static("chunked"),
    );

    // Add custom header (should be forwarded)
    headers.insert(
        "x-custom-header",
        HeaderValue::from_static("present"),
    );

    let response = proxy
        .forward_request(hyper::Method::GET, uri, headers, vec![])
        .await
        .expect("Request should succeed");

    // Verify the mock server received exactly one request (which means hop-by-hop headers were filtered)
    assert_eq!(response.status(), StatusCode::OK);
    mock.assert_async().await;
}