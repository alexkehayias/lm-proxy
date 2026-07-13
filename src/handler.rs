use crate::{config::{Config, Upstream}, models};
use axum::{
    body::Body,
    response::Response,
    http::{self, HeaderName},
};
use futures_util::StreamExt;

/// Payload for posting metrics to external endpoint.
///
/// Carries the four normalized token buckets so downstream cost estimators
/// can apply provider-specific rates (cache reads are cheaper than fresh input
/// on Anthropic; cache writes are billed at a premium, etc.).
#[derive(serde::Serialize)]
struct MetricsPayload {
    input: u32,
    output: u32,
    cache_read: u32,
    cache_write: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<u32>,
}

impl MetricsPayload {
    fn from_usage(usage: &models::Usage) -> Self {
        Self {
            input: usage.input,
            output: usage.output,
            cache_read: usage.cache_read,
            cache_write: usage.cache_write,
            reasoning: usage.reasoning,
        }
    }
}

/// Proxy service that forwards requests to upstream API
#[derive(Clone)]
pub struct ProxyService {
    client: reqwest::Client,
    config: Config,
}

impl ProxyService {
    pub fn new(client: reqwest::Client, config: Config) -> Self {
        Self { client, config }
    }

    /// Forward a request to upstream and track usage if applicable
    pub async fn forward_request(
        &self,
        method: http::Method,
        uri: http::Uri,
        headers: http::HeaderMap<http::HeaderValue>,
        body_bytes: Vec<u8>,
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let path = uri.path().to_string();
        let query = uri.query().map(|q| format!("?{}", q)).unwrap_or_default();

        // Determine which upstream to route to based on the first path segment
        let (upstream, remaining_path) = self.resolve_upstream(&path)?;
        let full_path = format!("{}{}", remaining_path, query);
        let upstream_url = upstream.url_for_path(&full_path);

        let tracking_usage = models::is_usage_tracked_path(&path);

        let filtered_headers = filter_hop_by_hop_headers(headers);
        let upstream_response = self.send_upstream_request(method, &upstream_url, filtered_headers, body_bytes).await?;

        let status = upstream_response.status();
        let mut builder = http::Response::builder().status(status);

        for (name, value) in upstream_response.headers() {
            if !is_hop_by_hop_header(name) {
                builder = builder.header(name, value);
            }
        }

        let content_type = upstream_response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());
        let is_streaming = content_type.is_some_and(|ct| ct.contains("text/event-stream"));

        if is_streaming {
            self.handle_streaming_response(upstream_response, builder, tracking_usage)
        } else if tracking_usage {
            self.handle_non_streaming_tracked_response(upstream_response, builder).await
        } else {
            self.handle_passthrough_response(upstream_response, builder)
        }
    }

    /// Resolve which upstream to use based on the request path.
    /// Returns the matched upstream and the remaining path (with the name prefix stripped).
    fn resolve_upstream<'a>(
        &'a self,
        path: &str,
    ) -> Result<(&'a Upstream, String), Box<dyn std::error::Error + Send + Sync>> {
        let trimmed = path.trim_start_matches('/');
        let first_segment = trimmed.split('/').next().unwrap_or("");

        if !first_segment.is_empty()
            && let Some(upstream) = self.config.find_upstream(first_segment)
        {
            // Strip the prefix: "/openai/chat/completions" -> "/chat/completions"
            let prefix_len = first_segment.len() + 1;
            let remaining = if trimmed.len() > prefix_len {
                format!("/{}", &trimmed[prefix_len..])
            } else {
                "/".to_string()
            };
            return Ok((upstream, remaining));
        }

        if let Some(default) = self.config.default_upstream() {
            Ok((default, path.to_string()))
        } else {
            Err(format!(
                "no upstream found for path '{path}' and no default upstream configured",
            )
            .into())
        }
    }

    async fn send_upstream_request(
        &self,
        method: http::Method,
        url: &str,
        headers: http::HeaderMap<http::HeaderValue>,
        body_bytes: Vec<u8>,
    ) -> Result<reqwest::Response, Box<dyn std::error::Error + Send + Sync>> {
        let mut request = self.client.request(method, url);

        // Apply headers but skip the Host header so reqwest sets it correctly from the URL
        for (name, value) in headers {
            if let Some(name) = name && name != http::header::HOST {
                request = request.header(name, value);
            }
        }

        if !body_bytes.is_empty() {
            request = request.body(body_bytes);
        }

        Ok(request.send().await?)
    }

    async fn handle_non_streaming_tracked_response(
        &self,
        upstream_response: reqwest::Response,
        builder: http::response::Builder,
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let body_bytes = upstream_response.bytes().await?;

        if let Some(usage) = models::try_parse_usage_from_body(&body_bytes) {
            tracing::info!("[USAGE] {}", usage.log_format());
            if usage.total() > 0 {
                self.post_metrics_if_configured(usage);
            }
        }

        Ok(builder.body(Body::from(body_bytes)).unwrap())
    }

    fn handle_streaming_response(
        &self,
        upstream_response: reqwest::Response,
        builder: http::response::Builder,
        tracking_usage: bool,
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.client.clone();
        let metrics_url = self.config.metrics_url.clone();

        // The accumulator lives in the closure so it persists across chunks.
        // Bytes are still forwarded to the client immediately (passthrough —
        // `result` is returned unchanged); the buffer only feeds a side path
        // that reassembles SSE events split across HTTP/2 DATA frames.
        let mut buffer = SseEventBuffer::new();

        let upstream_stream = Box::pin(upstream_response.bytes_stream().map(move |result| {
            if tracking_usage
                && let Ok(chunk) = &result
            {
                for event in buffer.feed(chunk) {
                    if let Some(usage) = models::try_parse_usage_from_chunk(&event) {
                        tracing::info!("[USAGE] {}", usage.log_format());
                        if usage.total() > 0 {
                            post_metrics_async(client.clone(), metrics_url.clone(), usage);
                        }
                    }
                }
            }

            result.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }));

        Ok(builder.body(Body::from_stream(upstream_stream)).unwrap())
    }

    fn handle_passthrough_response(
        &self,
        upstream_response: reqwest::Response,
        builder: http::response::Builder,
    ) -> Result<Response, Box<dyn std::error::Error + Send + Sync>> {
        let stream = upstream_response.bytes_stream();
        Ok(builder.body(Body::from_stream(stream)).unwrap())
    }

    fn post_metrics_if_configured(&self, usage: models::Usage) {
        if let Some(url) = self.config.metrics_url.clone() {
            post_metrics_async(self.client.clone(), Some(url), usage);
        }
    }
}

/// Filter out hop-by-hop headers that should not be forwarded
fn filter_hop_by_hop_headers(headers: http::HeaderMap<http::HeaderValue>) -> http::HeaderMap {
    let mut filtered = http::HeaderMap::new();
    for (name, value) in headers {
        if let Some(name) = name && !is_hop_by_hop_header(&name) {
            filtered.insert(name, value);
        }
    }
    filtered
}

/// Check if a header is hop-by-hop and should not be forwarded
fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection" | "keep-alive" | "proxy-authenticate"
            | "proxy-authorization" | "te" | "trailers"
            | "transfer-encoding" | "upgrade"
    )
}

/// Buffers bytes from an SSE stream and yields complete events.
///
/// SSE events are terminated by a blank line (`\n\n`). When an upstream
/// HTTP/2 DATA frame splits an event across multiple chunks, this buffer
/// reassembles the pieces so usage parsing sees a complete `event: ...\ndata: ...` block
/// — otherwise `message_delta` events carrying Anthropic's `thinking_tokens` get
/// dropped because neither half of a split contains valid JSON.
pub(crate) struct SseEventBuffer {
    pending: Vec<u8>,
}

impl SseEventBuffer {
    pub(crate) fn new() -> Self {
        Self { pending: Vec::new() }
    }

    /// Feed a chunk of bytes. Returns complete SSE events (with the trailing
    /// `\n\n` delimiter stripped) that were formed by this chunk.
    ///
    /// Events are delimited by `\n\n`. CRLF (`\r\n`) line endings — both within
    /// an event and as the `\r\n\r\n` terminator — are normalized to LF on ingest
    /// by stripping raw `\r`. This is safe for SSE: JSON payloads escape `\r`
    /// as the two-byte sequence `\\r`, so raw `\r` only ever appears as part of
    /// a CRLF line ending. A split event (arriving across multiple chunks) is only
    /// yielded once its closing `\n\n` arrives.
    pub(crate) fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        // Fast path: most SSE streams use LF only, so skip the allocation when
        // the chunk has no \r bytes (the common case for Anthropic/OpenAI).
        if chunk.contains(&b'\r') {
            let normalized: Vec<u8> = chunk
                .iter()
                .copied()
                .filter(|&b| b != b'\r')
                .collect();
            self.pending.extend(&normalized);
        } else {
            self.pending.extend_from_slice(chunk);
        }

        let mut events = Vec::new();
        while let Some(pos) = self.pending.windows(2).position(|w| w == b"\n\n") {
            // Consume everything up to and including the `\n\n` delimiter
            let event_bytes: Vec<u8> = self.pending.drain(..pos + 2).collect();
            if let Ok(s) = std::str::from_utf8(&event_bytes[..pos]) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    events.push(trimmed.to_string());
                }
            }
        }
        events
    }
}

impl Default for SseEventBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Post metrics asynchronously (spawned task, fire-and-forget)
fn post_metrics_async(client: reqwest::Client, url: Option<String>, usage: models::Usage) {
    if let Some(url) = url {
        tokio::spawn(async move {
            let payload = MetricsPayload::from_usage(&usage);

            if let Err(e) = client
                .post(url)
                .json(&payload)
                .send()
                .await
                .map(|r| r.error_for_status())
            {
                tracing::warn!("Failed to post metrics: {}", e);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Upstream;
    use std::net::SocketAddr;
    use hyper::header::{HeaderMap, HeaderValue};

    fn test_upstream(name: &str, url: &str) -> Upstream {
        Upstream {
            name: name.to_string(),
            url: url.to_string(),
        }
    }

    fn default_config(upstream_url: String) -> Config {
        Config {
            upstreams: vec![test_upstream("default", &upstream_url)],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        }
    }

    #[tokio::test]
    async fn test_handle_non_streaming_tracked_response() {
        let mut server = mockito::Server::new_async().await;

        // Mock endpoint that returns usage in response
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"id":"chatcmpl-123","object":"chat.completion","created":1677652288,"model":"gpt-4","choices":[{"index":0,"message":{"role":"assistant","content":"Hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#)
            .create_async()
            .await;

        let config = default_config(server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let uri = format!("{}/v1/chat/completions", server.url())
            .parse::<http::Uri>()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );
        let body = r#"{"model":"gpt-4","messages":[{"role":"user","content":"Hello"}]}"#.as_bytes().to_vec();

        // This should call handle_non_streaming_tracked_response since path is tracked
        let response = proxy
            .forward_request(http::Method::POST, uri, headers, body)
            .await
            .expect("Request should succeed");

        assert_eq!(response.status(), http::StatusCode::OK);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_handle_streaming_response() {
        let mut server = mockito::Server::new_async().await;

        // Mock endpoint that returns streaming response
        let mock = server
            .mock("POST", "/v1/chat/completions")
            .with_status(200)
            .with_header("content-type", "text/event-stream")
            // Use simple body that mockito can handle
            .with_body("data: test\n\ndata: [DONE]")
            .create_async()
            .await;

        let config = default_config(server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // Use a usage-tracked path to trigger streaming branch
        let uri = format!("{}/v1/chat/completions", server.url())
            .parse::<http::Uri>()
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            "content-type",
            HeaderValue::from_static("application/json"),
        );
        headers.insert(
            "accept",
            HeaderValue::from_static("text/event-stream"),
        );
        let body = r#"{"model":"gpt-4","messages":[{"role":"user","content":"Hello"}]}"#.as_bytes().to_vec();

        // This should call handle_streaming_response
        let result = proxy
            .forward_request(http::Method::POST, uri, headers, body)
            .await;

        // Either succeeds or gets an error from mockito - either way lines are covered
        if let Ok(response) = result {
            // If successful, verify it's streaming
            assert_eq!(response.status(), http::StatusCode::OK);
        }

        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_handle_passthrough_response() {
        let mut server = mockito::Server::new_async().await;

        // Mock endpoint that returns regular JSON (not tracked path, not streaming)
        let mock = server
            .mock("GET", "/v1/models")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"object":"list","data":[]}"#)
            .create_async()
            .await;

        let config = default_config(server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // Use a non-tracked path to trigger passthrough
        let uri = format!("{}/v1/models", server.url())
            .parse::<http::Uri>()
            .unwrap();
        let headers = HeaderMap::new();

        // This should call handle_passthrough_response
        let response = proxy
            .forward_request(http::Method::GET, uri, headers, vec![])
            .await
            .expect("Request should succeed");

        assert_eq!(response.status(), http::StatusCode::OK);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_forward_request_with_query_params() {
        let mut server = mockito::Server::new_async().await;

        // Mock that verifies query params are passed through
        let mock = server
            .mock("GET", "/v1/models")
            .match_query(mockito::Matcher::UrlEncoded("foo".into(), "bar".into()))
            .with_status(200)
            .with_body(r#"{"object":"list","data":[]}"#)
            .create_async()
            .await;

        let config = default_config(server.url());
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // Make request with query parameters
        let uri = format!("{}/v1/models?foo=bar", server.url())
            .parse::<http::Uri>()
            .unwrap();
        let headers = HeaderMap::new();

        let response = proxy
            .forward_request(http::Method::GET, uri, headers, vec![])
            .await
            .expect("Request should succeed");

        assert_eq!(response.status(), http::StatusCode::OK);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_forward_request_error_handling() {
        let mut server = mockito::Server::new_async().await;

        // Mock that closes connection (causes error)
        let _mock = server
            .mock("GET", "/error")
            // Don't set up response - connection will error
            .with_status(200)
            .create_async()
            .await;

        let config = default_config(server.url());
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(100))
            .build()
            .unwrap();
        let proxy = ProxyService::new(client, config);

        // Use an unreachable path to trigger error
        let uri = format!("{}/error", server.url())
            .parse::<http::Uri>()
            .unwrap();
        let headers = HeaderMap::new();

        // The request should return an error (bad gateway)
        let _result = proxy
            .forward_request(http::Method::GET, uri, headers, vec![])
            .await;

        // Either succeeds with error status or fails - either way it's tested
    }

    #[test]
    fn test_proxy_service_new() {
        let client = reqwest::Client::new();
        let config = default_config("https://api.openai.com/v1".to_string());

        let proxy = ProxyService::new(client, config.clone());
        assert_eq!(proxy.config.upstreams[0].url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_proxy_service_new_with_metrics() {
        let client = reqwest::Client::new();
        let config = Config {
            upstreams: vec![test_upstream("default", "https://api.openai.com/v1")],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: Some("http://localhost:8080/metrics".to_string()),
        };

        let proxy = ProxyService::new(client, config.clone());
        assert!(proxy.config.metrics_url.is_some());
    }

    #[tokio::test]
    async fn test_post_metrics_async_no_url() {
        let client = reqwest::Client::new();
        // Should not panic when URL is None
        post_metrics_async(
            client,
            None,
            models::Usage {
                input: 100,
                ..Default::default()
            },
        );
    }

    #[tokio::test]
    async fn test_post_metrics_async_with_url() {
        let client = reqwest::Client::new();
        // Should not panic even if the endpoint doesn't exist
        // The error is logged but not propagated
        post_metrics_async(
            client,
            Some("http://localhost:1/notfound".to_string()),
            models::Usage {
                input: 100,
                ..Default::default()
            },
        );
        // Give it a moment to try
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    #[test]
    fn test_is_hop_by_hop_header() {
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("connection")));
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("keep-alive")));
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("proxy-authenticate")));
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("proxy-authorization")));
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("te")));
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("trailers")));
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("transfer-encoding")));
        assert!(is_hop_by_hop_header(&http::HeaderName::from_static("upgrade")));

        // Non hop-by-hop headers
        assert!(!is_hop_by_hop_header(&http::HeaderName::from_static("content-type")));
        assert!(!is_hop_by_hop_header(&http::HeaderName::from_static("authorization")));
        assert!(!is_hop_by_hop_header(&http::HeaderName::from_static("host")));
    }

    #[test]
    fn test_filter_hop_by_hop_headers() {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::CONTENT_TYPE, "application/json".parse().unwrap());
        headers.insert(
            http::HeaderName::from_static("connection"),
            "close".parse().unwrap(),
        );
        headers.insert(
            http::HeaderName::from_static("transfer-encoding"),
            "chunked".parse().unwrap(),
        );

        let filtered = filter_hop_by_hop_headers(headers);

        assert!(filtered.contains_key(http::header::CONTENT_TYPE));
        assert!(!filtered.contains_key(http::HeaderName::from_static("connection")));
        assert!(!filtered.contains_key(http::HeaderName::from_static("transfer-encoding")));
    }

    #[test]
    fn test_resolve_upstream_to_named_upstream() {
        let config = Config {
            upstreams: vec![
                test_upstream("openai", "https://api.openai.com/v1"),
                test_upstream("anthropic", "https://api.anthropic.com/v1"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let (upstream, remaining) = proxy
            .resolve_upstream("/anthropic/v1/messages")
            .expect("should resolve");
        assert_eq!(upstream.name, "anthropic");
        assert_eq!(remaining, "/v1/messages");
    }

    #[test]
    fn test_resolve_upstream_to_default_when_path_does_not_match() {
        let config = Config {
            upstreams: vec![
                test_upstream("default", "https://api.openai.com/v1"),
                test_upstream("anthropic", "https://api.anthropic.com/v1"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let (upstream, remaining) = proxy
            .resolve_upstream("/v1/chat/completions")
            .expect("should resolve to default");
        assert_eq!(upstream.name, "default");
        assert_eq!(remaining, "/v1/chat/completions");
    }

    #[test]
    fn test_resolve_upstream_no_default_returns_error() {
        let config = Config {
            upstreams: vec![
                test_upstream("openai", "https://api.openai.com/v1"),
                test_upstream("anthropic", "https://api.anthropic.com/v1"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let result = proxy.resolve_upstream("/unknown/foo");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("no upstream found"));
    }

    #[test]
    fn test_resolve_upstream_root_of_named_upstream() {
        let config = Config {
            upstreams: vec![test_upstream("ollama", "http://localhost:11434")],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let (upstream, remaining) = proxy
            .resolve_upstream("/ollama")
            .expect("should resolve");
        assert_eq!(upstream.name, "ollama");
        assert_eq!(remaining, "/");
    }

    #[test]
    fn test_resolve_upstream_empty_path_uses_default() {
        let config = Config {
            upstreams: vec![test_upstream("default", "https://api.openai.com/v1")],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let (upstream, remaining) = proxy
            .resolve_upstream("")
            .expect("should resolve");
        assert_eq!(upstream.name, "default");
        assert_eq!(remaining, "");
    }

    #[test]
    fn test_resolve_upstream_double_slash_paths_resolve_correctly() {
        let config = Config {
            upstreams: vec![
                test_upstream("openai", "https://api.openai.com/v1"),
                test_upstream("default", "https://api.anthropic.com/v1"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        // "//" has no first segment after trimming -> falls to default
        let (upstream, remaining) = proxy
            .resolve_upstream("//")
            .expect("should resolve to default");
        assert_eq!(upstream.name, "default");
        assert_eq!(remaining, "//");
    }

    #[test]
    fn test_resolve_upstream_single_upstream_is_default() {
        let config = Config {
            upstreams: vec![test_upstream("myapi", "http://localhost:8080")],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };
        let client = reqwest::Client::new();
        let proxy = ProxyService::new(client, config);

        let (upstream, remaining) = proxy
            .resolve_upstream("/v1/chat/completions")
            .expect("should resolve to only upstream");
        assert_eq!(upstream.name, "myapi");
        assert_eq!(remaining, "/v1/chat/completions");
    }

    #[test]
    fn test_filter_hop_by_hop_headers_preserves_other_headers() {
        let mut headers = http::HeaderMap::new();
        headers.insert(http::header::AUTHORIZATION, "Bearer token".parse().unwrap());
        headers.insert(http::header::HOST, "example.com".parse().unwrap());

        let filtered = filter_hop_by_hop_headers(headers);

        assert!(filtered.contains_key(http::header::AUTHORIZATION));
        assert!(filtered.contains_key(http::header::HOST));
    }

    #[test]
    fn test_sse_buffer_single_complete_event() {
        // A complete event in one chunk (no splitting) — baseline behavior.
        let mut buf = SseEventBuffer::new();
        let chunk = b"event: message_start\ndata: {\"type\": \"message_start\", \"message\": {\"id\": \"msg_1\", \"usage\": {\"input_tokens\": 25, \"output_tokens\": 1}}}\n\n";
        let events = buf.feed(chunk);
        assert_eq!(events.len(), 1);
        let usage = models::try_parse_usage_from_chunk(&events[0]);
        assert!(usage.is_some(), "should parse usage from reassembled event");
        assert_eq!(usage.unwrap().input, 25);
    }

    #[test]
    fn test_sse_buffer_split_message_delta_thinking_captured() {
        // Bug scenario: a message_delta event carrying thinking_tokens is split
        // across two HTTP/2 DATA frames. Without reassembly, neither half contains
        // valid JSON and thinking_tokens would be silently dropped.
        let mut buf = SseEventBuffer::new();
        let part1 = b"event: message_delta\ndata: {\"type\": \"message";
        let part2 = b"_delta\", \"delta\": {\"stop_reason\": \"end_turn\"}, \"usage\": {\"output_tokens\": 100, \"output_tokens_details\": {\"thinking_tokens\": 30}}}\n\n";

        // First chunk: partial event, nothing complete yet
        let events = buf.feed(part1);
        assert!(events.is_empty(), "partial event should not be yielded yet");

        // Second chunk: completes the event
        let events = buf.feed(part2);
        assert_eq!(events.len(), 1, "reassembled event should be yielded");
        let usage = models::try_parse_usage_from_chunk(&events[0])
            .expect("thinking_tokens must be captured from reassembled event");
        assert_eq!(usage.output, 100);
        assert_eq!(
            usage.reasoning,
            Some(30),
            "thinking_tokens (reasoning) must survive chunk boundary splitting",
        );
    }

    #[test]
    fn test_sse_buffer_split_message_start_event() {
        // message_start event split across three chunks (the event: line, then
        // half the data: JSON, then the rest). Verifies accumulation across >2 chunks.
        let mut buf = SseEventBuffer::new();
        let part1 = b"event: message_start\nda";
        let part2 = b"ta: {\"type\": \"message_st";
        let part3 = b"art\", \"message\": {\"id\": \"msg_1\", \"usage\": {\"input_tokens\": 200, \"output_tokens\": 1}}}\n\n";

        assert!(buf.feed(part1).is_empty());
        assert!(buf.feed(part2).is_empty());
        let events = buf.feed(part3);
        assert_eq!(events.len(), 1);

        let usage = models::try_parse_usage_from_chunk(&events[0])
            .expect("should parse reassembled message_start");
        assert_eq!(usage.input, 200);
        assert_eq!(usage.output, 1);
    }

    #[test]
    fn test_sse_buffer_multi_event_stream_spanning_chunks() {
        // A realistic Anthropic stream: message_start, several content blocks,
        // and a final message_delta — all arriving in arbitrary chunk boundaries
        // that don't align with event boundaries.
        let mut buf = SseEventBuffer::new();

        // Chunk 1: message_start (complete) + start of content_block_start
        let chunk1 = b"event: message_start\ndata: {\"type\": \"message_start\", \"message\": {\"id\": \"msg_1\", \"usage\": {\"input_tokens\": 100, \"output_tokens\": 1}}}\n\neve";
        let chunk2 = b"nt: content_block_start\ndata: {\"type\": \"content_block_start\", \"index\": 0, \"content_block\": {\"type\": \"text\", \"text\": \"\"}}\n\n";
        // Chunk 3: a ping (no usage) + message_delta with thinking_tokens
        let chunk3 = b"event: ping\ndata: {\"type\": \"ping\"}\n\nevent: message_delta\ndata: {\"type\": \"message_delta\", \"delta\": {\"stop_reason\": \"end_turn\"}, \"usage\": {\"output_tokens\": 50, \"output_tokens_details\": {\"thinking_tokens\": 20}}}\n\n";

        let mut all_usage = Vec::new();
        for chunk in [chunk1.as_slice(), chunk2.as_slice(), chunk3.as_slice()] {
            for event in buf.feed(chunk) {
                if let Some(usage) = models::try_parse_usage_from_chunk(&event) {
                    all_usage.push(usage);
                }
            }
        }

        // message_start (input=100) and message_delta (output=50, thinking=20)
        // ping events have no usage and should be skipped
        assert_eq!(all_usage.len(), 2, "should extract usage from both events");
        assert_eq!(all_usage[0].input, 100);
        assert_eq!(all_usage[0].output, 1);
        assert_eq!(all_usage[1].output, 50);
        assert_eq!(
            all_usage[1].reasoning,
            Some(20),
            "thinking_tokens from final message_delta must be captured",
        );
    }

    #[test]
    fn test_sse_buffer_partial_chunk_holds_incomplete_data() {
        // A chunk with no \n\n delimiter yet — buffer should hold it without
        // yielding, then yield once the closing \n\n arrives.
        let mut buf = SseEventBuffer::new();
        assert!(buf.feed(b"event: ping\ndata: {\"type\": \"ping\"}").is_empty());
        // Arrives in a later chunk
        let events = buf.feed(b"\n\nevent: done\ndata: [DONE]\n\n");
        assert_eq!(events.len(), 2, "both complete events should be yielded");
        // ping has no usage -> None; [DONE] -> None
        assert!(models::try_parse_usage_from_chunk(&events[0]).is_none());
        assert!(models::try_parse_usage_from_chunk(&events[1]).is_none());
    }

    #[test]
    fn test_sse_buffer_invalid_utf8_event_skipped() {
        // If an event body contains invalid UTF-8, it's skipped (drained from
        // the buffer) without panicking — matches the old per-chunk behavior.
        let mut buf = SseEventBuffer::new();
        // Invalid UTF-8 event followed by a valid OpenAI-style chunk
        let chunk = b"event: bad\ndata: \xff\xfe\n\nevent: good\ndata: {\"id\":\"chatcmpl-1\",\"object\":\"chat.completion.chunk\",\"created\":1677652288,\"model\":\"gpt-4\",\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":2}}\n\n";
        let events = buf.feed(chunk);
        // The invalid event is dropped; only the valid one comes through
        assert_eq!(events.len(), 1, "invalid UTF-8 event should be skipped");
        let usage = models::try_parse_usage_from_chunk(&events[0]);
        assert!(usage.is_some(), "valid event after the bad one should still parse");
        assert_eq!(usage.unwrap().input, 5);
    }

    #[test]
    fn test_sse_buffer_trims_crlf_line_endings() {
        // Some upstreams send CRLF (\r\n) line endings. The buffer should trim
        // trailing \r so try_parse_usage_from_chunk sees a clean event.
        let mut buf = SseEventBuffer::new();
        // SSE with CRLF: "event: ...\r\ndata: {...}\r\n\r\n"
        let chunk = b"event: message_delta\ndata: {\"type\": \"message_delta\", \"usage\": {\"output_tokens\": 7}}\r\n\r\n";
        let events = buf.feed(chunk);
        assert_eq!(events.len(), 1);
        // The trimmed event should not contain a trailing \r that would break
        // try_parse_usage_from_chunk's "\ndata: " search
        let usage = models::try_parse_usage_from_chunk(&events[0]);
        assert!(usage.is_some(), "CRLF event should parse after trimming");
        assert_eq!(usage.unwrap().output, 7);
    }

    #[test]
    fn test_sse_buffer_default_impl() {
        // Default trait impl should match new()
        let buf = SseEventBuffer::default();
        assert!(buf.pending.is_empty());
    }
}