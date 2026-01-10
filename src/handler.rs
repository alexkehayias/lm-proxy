use crate::{config::Config, models};
use axum::{
    body::Body,
    response::Response,
    http::{self, HeaderName},
};
use futures_util::StreamExt;

/// Payload for posting metrics to external endpoint
#[derive(serde::Serialize)]
struct MetricsPayload {
    name: String,
    value: u32,
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
        let full_path = format!("{}{}", path, query);

        let tracking_usage = models::is_usage_tracked_path(&path);
        let upstream_url = self.config.upstream_url_for_path(&full_path);

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

    async fn send_upstream_request(
        &self,
        method: http::Method,
        url: &str,
        headers: http::HeaderMap<http::HeaderValue>,
        body_bytes: Vec<u8>,
    ) -> Result<reqwest::Response, Box<dyn std::error::Error + Send + Sync>> {
        let mut request = self.client.request(method, url).headers(headers);

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
            log::info!("[USAGE] {}", usage.log_format());
            if let Some(total_tokens) = usage.total_tokens {
                self.post_metrics_if_configured(total_tokens);
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

        let upstream_stream = Box::pin(upstream_response.bytes_stream().map(move |result| {
            if tracking_usage
                && let Ok(chunk) = &result
                && let Some(usage) = parse_usage_from_sse_chunk(chunk)
            {
                log::info!("[USAGE] {}", usage.log_format());
                if let Some(total_tokens) = usage.total_tokens {
                    post_metrics_async(client.clone(), metrics_url.clone(), total_tokens);
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

    fn post_metrics_if_configured(&self, total_tokens: u32) {
        if let Some(url) = self.config.metrics_url.clone() {
            post_metrics_async(self.client.clone(), Some(url), total_tokens);
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

/// Parse usage from an SSE chunk
fn parse_usage_from_sse_chunk(chunk: &[u8]) -> Option<models::Usage> {
    let text = std::str::from_utf8(chunk).ok()?;
    let text = text.trim().strip_prefix("data: ")?;

    // Skip [DONE] marker
    if text == "[DONE]" {
        return None;
    }

    models::try_parse_usage_from_chunk(text)
}

/// Post metrics asynchronously (spawned task, fire-and-forget)
fn post_metrics_async(client: reqwest::Client, url: Option<String>, total_tokens: u32) {
    if let Some(url) = url {
        tokio::spawn(async move {
            let payload = MetricsPayload {
                name: "token-count".to_string(),
                value: total_tokens,
            };

            if let Err(e) = client
                .post(url)
                .json(&payload)
                .send()
                .await
                .map(|r| r.error_for_status())
            {
                log::warn!("Failed to post metrics: {}", e);
            }
        });
    }
}