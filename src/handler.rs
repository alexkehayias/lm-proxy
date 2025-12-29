use crate::{config::Config, models};
use axum::{
    body::Body,
    response::Response,
    http::{self, HeaderName},
};
use futures_util::StreamExt;

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

        // Check if we should track usage for this endpoint
        let tracking_usage = models::is_usage_tracked_path(&path);

        // Build upstream URL
        let upstream_url = self.config.upstream_url_for_path(&full_path);

        // Prepare headers - filter out hop-by-hop headers
        let mut filtered_headers = http::HeaderMap::new();
        for (name, value) in headers {
            if let Some(name) = name {
                if !is_hop_by_hop_header(&name) {
                    filtered_headers.insert(name, value);
                }
            }
        }

        // Forward the request
        let mut upstream_request = self.client.request(method, &upstream_url);
        upstream_request = upstream_request.headers(filtered_headers);

        if !body_bytes.is_empty() {
            upstream_request = upstream_request.body(body_bytes);
        }

        let upstream_response = upstream_request.send().await.map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error + Send + Sync>
        })?;

        // Build response
        let status = upstream_response.status();
        let mut builder = http::Response::builder().status(status);

        // Copy response headers
        for (name, value) in upstream_response.headers() {
            if !is_hop_by_hop_header(name) {
                builder = builder.header(name, value);
            }
        }

        let content_type = upstream_response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok());
        let is_streaming = content_type.map_or(false, |ct| ct.contains("text/event-stream"));

        // For streaming responses: track in the stream
        let upstream_stream = Box::pin(upstream_response.bytes_stream().map(move |result| {
            if tracking_usage && is_streaming {
                match &result {
                    Ok(chunk) => {
                        let text = std::str::from_utf8(chunk).ok();
                        if let Some(t) = text {
                            if let Some(usage) = models::try_parse_usage_from_chunk(t) {
                                log::info!("[USAGE] {}", usage.log_format());
                            }
                        }
                    }
                    _ => {}
                };
            }
            result.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }));

        let body = Body::from_stream(upstream_stream);
        Ok(builder.body(body).unwrap())
    }
}

/// Check if a header is hop-by-hop and should not be forwarded
fn is_hop_by_hop_header(name: &HeaderName) -> bool {
    name.as_str() == "connection"
        || name.as_str() == "keep-alive"
        || name.as_str() == "proxy-authenticate"
        || name.as_str() == "proxy-authorization"
        || name.as_str() == "te"
        || name.as_str() == "trailers"
        || name.as_str() == "transfer-encoding"
        || name.as_str() == "upgrade"
}