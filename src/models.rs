use serde::{Deserialize, Serialize};

/// Normalized usage statistics — canonical four-bucket model for cost tracking.
///
/// Buckets:
/// - `input`: fresh (non-cached) input tokens, billed at standard rate
/// - `output`: completion tokens (includes reasoning if any)
/// - `cache_read`: tokens served from a cached prefix, billed at discounted rate
/// - `cache_write`: tokens written to cache (Anthropic only), billed at premium
///
/// `reasoning` is an optional detail of `output` (thinking tokens are a subset
/// of output, not additive).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Usage {
    #[serde(default)]
    pub input: u32,
    #[serde(default)]
    pub output: u32,
    #[serde(default)]
    pub cache_read: u32,
    #[serde(default)]
    pub cache_write: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u32>,
}

impl Usage {
    /// Returns a formatted string for logging
    pub fn log_format(&self) -> String {
        format!(
            "input={} output={} cache_read={} cache_write={} reasoning={:?}",
            self.input, self.output, self.cache_read, self.cache_write, self.reasoning
        )
    }

    /// Returns the total token count (sum of all four buckets).
    /// Note: cache_read and cache_write are billed at different rates than
    /// fresh input, but for raw token counting they still count toward the total.
    pub fn total(&self) -> u32 {
        self.input + self.output + self.cache_read + self.cache_write
    }
}

// ============================================================
// OpenAI incoming usage shape (as it appears in API responses)
// ============================================================

/// OpenAI `usage` object from chat completions / embeddings responses.
///
/// Note: on OpenAI, `prompt_tokens` is the TOTAL input (inclusive of cached
/// tokens), so `input` (fresh) = `prompt_tokens - cached_tokens`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAIUsage {
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTokensDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionTokensDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_tokens: Option<u32>,
}

impl OpenAIUsage {
    /// Normalize to canonical `Usage`. On OpenAI:
    /// - Fresh input = prompt_tokens - cached_tokens (cache hits are billed at a discount)
    /// - No explicit cache write bucket (OpenAI auto-caches; writes aren't billed separately)
    /// - `reasoning_tokens` is a detail breakdown of completion tokens (subset, not additive)
    pub fn normalize(&self) -> Usage {
        let cached = self
            .prompt_tokens_details
            .as_ref()
            .and_then(|d| d.cached_tokens)
            .unwrap_or(0);
        Usage {
            input: self.prompt_tokens.saturating_sub(cached),
            output: self.completion_tokens,
            cache_read: cached,
            cache_write: 0,
            reasoning: self
                .completion_tokens_details
                .as_ref()
                .and_then(|d| d.reasoning_tokens),
        }
    }
}

// ============================================================
// Anthropic incoming usage shape
// ============================================================

/// Anthropic `usage` object from Messages API responses.
///
/// Note: on Anthropic, `input_tokens` is the FRESH (non-cached) portion —
/// cache reads and writes are reported in separate fields, billed at their own rates.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<AnthropicOutputTokensDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicOutputTokensDetails {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_tokens: Option<u32>,
}

impl AnthropicUsage {
    /// Normalize to canonical `Usage`. On Anthropic:
    /// - `input_tokens` is already fresh (cache hits/writes are in separate fields)
    /// - `cache_read_input_tokens` = cache hits (billed at discount)
    /// - `cache_creation_input_tokens` = cache writes (billed at premium)
    /// - `thinking_tokens` is a detail breakdown of output tokens (subset, not additive)
    pub fn normalize(&self) -> Usage {
        Usage {
            input: self.input_tokens.unwrap_or(0),
            output: self.output_tokens.unwrap_or(0),
            cache_read: self.cache_read_input_tokens.unwrap_or(0),
            cache_write: self.cache_creation_input_tokens.unwrap_or(0),
            reasoning: self
                .output_tokens_details
                .as_ref()
                .and_then(|d| d.thinking_tokens),
        }
    }
}

// ============================================================
// Response types
// ============================================================

/// Non-streaming completion response (OpenAI-style)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAIUsage>,
}

/// Streaming completion chunk (Server-Sent Event format)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionChunk {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Choice>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAIUsage>,
}

/// Individual choice in a streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<Delta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// Delta content in streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Delta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// Embeddings response (also has usage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsResponse {
    pub data: Vec<EmbeddingData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<OpenAIUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
}

/// Anthropic non-streaming response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessageResponse {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Vec<AnthropicContentBlock>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AnthropicUsage>,
}

/// Anthropic content block in message response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicContentBlock {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Anthropic streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessageChunk {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delta: Option<AnthropicDelta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AnthropicUsage>,
}

/// Anthropic delta in streaming chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicDelta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// Anthropic message_start event - usage is nested in the message field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessageStartEvent {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<AnthropicMessageWithUsage>,
}

/// Anthropic message with usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessageWithUsage {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AnthropicUsage>,
}

/// Anthropic message_delta event - usage is at the top level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicMessageDeltaEvent {
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<AnthropicUsage>,
}

/// Attempts to parse usage from a chunk of SSE data
/// Returns None if the chunk doesn't contain usage (most chunks don't)
pub fn try_parse_usage_from_chunk(chunk: &str) -> Option<Usage> {
    // Handle SSE format: "event: <type>\ndata: <json>" or just "data: <json>"
    let json = if let Some(pos) = chunk.find("\ndata: ") {
        // Has event line, extract JSON after "data: "
        &chunk[pos + 7..]
    } else {
        // No event line, just strip "data: " prefix
        chunk.trim_start_matches("data: ")
    };

    if json == "[DONE]" {
        return None;
    }

    // OpenAI streaming
    if let Ok(chunk) = serde_json::from_str::<CompletionChunk>(json)
        && let Some(usage) = chunk.usage
    {
        return Some(usage.normalize());
    }

    // OpenAI completion without streaming
    if let Ok(response) = serde_json::from_str::<CompletionResponse>(json)
        && let Some(usage) = response.usage
    {
        return Some(usage.normalize());
    }

    // Anthropic message_start event - usage is nested in message
    if let Ok(event) = serde_json::from_str::<AnthropicMessageStartEvent>(json)
        && let Some(message) = event.message
    {
        return message.usage.map(|u| u.normalize());
    }

    // Anthropic message_delta event - usage is at top level
    if let Ok(event) = serde_json::from_str::<AnthropicMessageDeltaEvent>(json) {
        return event.usage.map(|u| u.normalize());
    }

    // Legacy Anthropic streaming (for backwards compatibility)
    if let Ok(chunk) = serde_json::from_str::<AnthropicMessageChunk>(json) {
        return chunk.usage.map(|u| u.normalize());
    }

    None
}

/// Attempts to parse usage from a complete JSON body (non-streaming)
///
/// Detects the provider by inspecting the raw JSON for provider-specific
/// field names in the usage object. This is necessary because both OpenAI
/// and Anthropic responses have an `id` field, so struct-based detection
/// alone is ambiguous — we need to check for `input_tokens` (Anthropic)
/// vs `prompt_tokens` (OpenAI) to route to the correct parser.
pub fn try_parse_usage_from_body(body: &[u8]) -> Option<Usage> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    let usage_obj = value.get("usage")?;

    // Detect Anthropic by presence of `input_tokens` in the usage object
    let is_anthropic = usage_obj
        .as_object()
        .map(|obj| obj.contains_key("input_tokens"))
        .unwrap_or(false);

    if is_anthropic {
        let response: AnthropicMessageResponse = serde_json::from_value(value).ok()?;
        return response.usage.map(|u| u.normalize());
    }

    // OpenAI completion
    if let Ok(response) = serde_json::from_value::<CompletionResponse>(value.clone())
        && let Some(usage) = response.usage
    {
        return Some(usage.normalize());
    }

    // OpenAI embeddings
    if let Ok(response) = serde_json::from_value::<EmbeddingsResponse>(value)
        && let Some(usage) = response.usage
    {
        return Some(usage.normalize());
    }

    None
}

/// Check if a request path should have usage tracked (completions/embeddings/responses/messages)
pub fn is_usage_tracked_path(path: &str) -> bool {
    path.contains("/chat/completions")
        || path.ends_with("completions")
        || path.ends_with("/embeddings")
        || path.contains("/responses")
        || path.contains("/v1/messages")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- Usage ----------

    #[test]
    fn test_usage_log_format() {
        let usage = Usage {
            input: 100,
            output: 50,
            cache_read: 30,
            cache_write: 10,
            reasoning: Some(20),
        };
        assert_eq!(
            usage.log_format(),
            "input=100 output=50 cache_read=30 cache_write=10 reasoning=Some(20)"
        );
    }

    #[test]
    fn test_usage_log_format_with_defaults() {
        let usage = Usage::default();
        assert_eq!(
            usage.log_format(),
            "input=0 output=0 cache_read=0 cache_write=0 reasoning=None"
        );
    }

    #[test]
    fn test_usage_total_sums_all_buckets() {
        let usage = Usage {
            input: 100,
            output: 50,
            cache_read: 30,
            cache_write: 10,
            reasoning: None,
        };
        // reasoning is a subset of output, not additive
        assert_eq!(usage.total(), 100 + 50 + 30 + 10);
    }

    // ---------- OpenAIUsage::normalize ----------

    #[test]
    fn test_openai_usage_normalize_no_cache() {
        let usage = OpenAIUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: Some(150),
            prompt_tokens_details: None,
            completion_tokens_details: None,
        };
        let normalized = usage.normalize();
        assert_eq!(normalized.input, 100);
        assert_eq!(normalized.output, 50);
        assert_eq!(normalized.cache_read, 0);
        assert_eq!(normalized.cache_write, 0);
        assert_eq!(normalized.reasoning, None);
    }

    #[test]
    fn test_openai_usage_normalize_with_cache_read() {
        // OpenAI's prompt_tokens is TOTAL input (includes cached).
        // fresh = 100 - 80 = 20, cache_read = 80
        let usage = OpenAIUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: Some(150),
            prompt_tokens_details: Some(PromptTokensDetails {
                cached_tokens: Some(80),
                audio_tokens: None,
            }),
            completion_tokens_details: None,
        };
        let normalized = usage.normalize();
        assert_eq!(normalized.input, 20);
        assert_eq!(normalized.cache_read, 80);
        assert_eq!(normalized.output, 50);
        assert_eq!(normalized.cache_write, 0);
    }

    #[test]
    fn test_openai_usage_normalize_with_reasoning() {
        let usage = OpenAIUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: Some(150),
            prompt_tokens_details: None,
            completion_tokens_details: Some(CompletionTokensDetails {
                reasoning_tokens: Some(20),
                audio_tokens: None,
            }),
        };
        let normalized = usage.normalize();
        // reasoning is a detail of output, not additive
        assert_eq!(normalized.output, 50);
        assert_eq!(normalized.reasoning, Some(20));
    }

    // ---------- AnthropicUsage::normalize ----------

    #[test]
    fn test_anthropic_usage_normalize_no_cache() {
        let usage = AnthropicUsage {
            input_tokens: Some(200),
            output_tokens: Some(100),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            output_tokens_details: None,
        };
        let normalized = usage.normalize();
        assert_eq!(normalized.input, 200);
        assert_eq!(normalized.output, 100);
        assert_eq!(normalized.cache_read, 0);
        assert_eq!(normalized.cache_write, 0);
        assert_eq!(normalized.reasoning, None);
    }

    #[test]
    fn test_anthropic_usage_normalize_with_cache() {
        // On Anthropic, input_tokens is FRESH only (unlike OpenAI).
        // cache_read and cache_creation are reported in separate fields.
        let usage = AnthropicUsage {
            input_tokens: Some(200),
            output_tokens: Some(100),
            cache_read_input_tokens: Some(50),
            cache_creation_input_tokens: Some(25),
            output_tokens_details: None,
        };
        let normalized = usage.normalize();
        assert_eq!(normalized.input, 200);
        assert_eq!(normalized.cache_read, 50);
        assert_eq!(normalized.cache_write, 25);
        assert_eq!(normalized.output, 100);
    }

    #[test]
    fn test_anthropic_usage_normalize_with_thinking() {
        let usage = AnthropicUsage {
            input_tokens: Some(200),
            output_tokens: Some(100),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            output_tokens_details: Some(AnthropicOutputTokensDetails {
                thinking_tokens: Some(30),
            }),
        };
        let normalized = usage.normalize();
        // thinking is a detail of output, not additive
        assert_eq!(normalized.output, 100);
        assert_eq!(normalized.reasoning, Some(30));
    }

    // ---------- is_usage_tracked_path ----------

    #[test]
    fn test_is_usage_tracked_path_chat_completions() {
        assert!(is_usage_tracked_path("/v1/chat/completions"));
        assert!(is_usage_tracked_path("https://api.example.com/v1/chat/completions"));
    }

    #[test]
    fn test_is_usage_tracked_path_completions() {
        assert!(is_usage_tracked_path("/v1/completions"));
        assert!(is_usage_tracked_path("completions"));
    }

    #[test]
    fn test_is_usage_tracked_path_embeddings() {
        assert!(is_usage_tracked_path("/v1/embeddings"));
        assert!(is_usage_tracked_path("https://api.example.com/v1/embeddings"));
    }

    #[test]
    fn test_is_usage_tracked_path_responses() {
        assert!(is_usage_tracked_path("/v1/responses"));
        assert!(is_usage_tracked_path("https://api.example.com/v1/responses"));
    }

    #[test]
    fn test_is_usage_tracked_path_anthropic_messages() {
        assert!(is_usage_tracked_path("/v1/messages"));
        assert!(is_usage_tracked_path("https://api.anthropic.com/v1/messages"));
    }

    #[test]
    fn test_is_usage_tracked_path_non_tracked() {
        assert!(!is_usage_tracked_path("/v1/models"));
        assert!(!is_usage_tracked_path("/health"));
        assert!(!is_usage_tracked_path(""));
    }

    // ---------- try_parse_usage_from_chunk ----------

    #[test]
    fn test_try_parse_usage_from_chunk_openai_completion() {
        let chunk = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input, 10);
        assert_eq!(usage.output, 5);
    }

    #[test]
    fn test_try_parse_usage_from_chunk_openai_with_cache() {
        // OpenAI response with prompt_tokens_details.cached_tokens
        let chunk = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}],"usage":{"prompt_tokens":100,"completion_tokens":50,"total_tokens":150,"prompt_tokens_details":{"cached_tokens":80}}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        // Fresh input = 100 - 80 = 20; cache_read = 80
        assert_eq!(usage.input, 20);
        assert_eq!(usage.cache_read, 80);
        assert_eq!(usage.output, 50);
    }

    #[test]
    fn test_try_parse_usage_from_chunk_openai_with_reasoning() {
        let chunk = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"o1","choices":[{"index":0,"delta":{"content":""},"finish_reason":null}],"usage":{"prompt_tokens":10,"completion_tokens":50,"total_tokens":60,"completion_tokens_details":{"reasoning_tokens":30}}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.output, 50);
        assert_eq!(usage.reasoning, Some(30));
    }

    #[test]
    fn test_try_parse_usage_from_chunk_done() {
        let chunk = "data: [DONE]";
        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_none());
    }

    #[test]
    fn test_try_parse_usage_from_chunk_no_usage() {
        let chunk = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#;
        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_none());
    }

    #[test]
    fn test_try_parse_usage_from_body_openai_completion() {
        let body = br#"{"id":"chatcmpl-123","object":"chat.completion","created":1677652288,"model":"gpt-4","choices":[{"index":0,"message":{"role":"assistant","content":"Hello"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
        let usage = try_parse_usage_from_body(body);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input, 10);
        assert_eq!(usage.output, 5);
    }

    #[test]
    fn test_try_parse_usage_from_body_embeddings() {
        let body = br#"{"object":"list","data":[{"object":"embedding","embedding":[0.1,0.2],"index":0}],"model":"text-embedding-ada-002","usage":{"prompt_tokens":8,"completion_tokens":0,"total_tokens":8}}"#;
        let usage = try_parse_usage_from_body(body);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input, 8);
    }

    #[test]
    fn test_try_parse_usage_from_body_no_match() {
        let body = br#"{"unknown":"response"}"#;
        let usage = try_parse_usage_from_body(body);
        assert!(usage.is_none());
    }

    #[test]
    fn test_try_parse_usage_from_body_anthropic_with_thinking() {
        // Anthropic non-streaming response with thinking tokens.
        // This is the critical test: before the format-detection fix, this
        // response was incorrectly parsed as an OpenAI CompletionResponse
        // (because both have `id`), which dropped the thinking_tokens field.
        let body = br#"{"id":"msg_0123","type":"message","role":"assistant","content":[{"type":"text","text":"Hello"}],"model":"claude-opus-4-7","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":5,"output_tokens_details":{"thinking_tokens":30}}}"#;
        let usage = try_parse_usage_from_body(body);
        assert!(usage.is_some(), "Should parse Anthropic response with thinking tokens");
        let usage = usage.unwrap();
        assert_eq!(usage.input, 100);
        assert_eq!(usage.output, 50);
        assert_eq!(usage.cache_read, 10);
        assert_eq!(usage.cache_write, 5);
        assert_eq!(
            usage.reasoning,
            Some(30),
            "thinking_tokens should be captured in reasoning field, not None"
        );
    }

    #[test]
    fn test_try_parse_usage_from_body_anthropic_no_thinking() {
        // Anthropic response without thinking enabled — output_tokens_details absent
        let body = br#"{"id":"msg_0123","type":"message","role":"assistant","content":[{"type":"text","text":"Hello"}],"model":"claude-opus-4-7","stop_reason":"end_turn","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let usage = try_parse_usage_from_body(body);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input, 100);
        assert_eq!(usage.output, 50);
        assert_eq!(
            usage.reasoning,
            None,
            "reasoning should be None when thinking is not enabled"
        );
    }

    #[test]
    fn test_try_parse_usage_from_chunk_anthropic_message_start() {
        // Anthropic streaming message_start event
        let chunk = r#"event: message_start
data: {"type": "message_start", "message": {"id": "msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY", "type": "message", "role": "assistant", "content": [], "model": "claude-opus-4-6", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some(), "Should parse usage from Anthropic message_start event");
        let usage = usage.unwrap();
        assert_eq!(usage.input, 25);
        // output_tokens=1 on message_start (will be superseded by final count on message_delta)
        assert_eq!(usage.output, 1);
    }

    #[test]
    fn test_try_parse_usage_from_chunk_anthropic_message_delta() {
        // Anthropic streaming message_delta event with usage
        let chunk = r#"event: message_delta
data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": 15}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some(), "Should parse usage from Anthropic message_delta event");
        let usage = usage.unwrap();
        assert_eq!(usage.output, 15);
    }

    #[test]
    fn test_try_parse_usage_from_chunk_anthropic_message_delta_with_thinking() {
        // message_delta event with thinking_tokens (appears on final streaming event)
        let chunk = r#"event: message_delta
data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": 100, "output_tokens_details": {"thinking_tokens": 30}}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.output, 100);
        assert_eq!(usage.reasoning, Some(30));
    }

    #[test]
    fn test_try_parse_usage_from_chunk_anthropic_with_cache() {
        // Anthropic response with cache_read and cache_creation fields
        let chunk = r#"event: message_start
data: {"type": "message_start", "message": {"id": "msg_1", "type": "message", "role": "assistant", "content": [], "model": "claude-opus-4-6", "usage": {"input_tokens": 200, "output_tokens": 1, "cache_read_input_tokens": 50, "cache_creation_input_tokens": 25}}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.input, 200);
        assert_eq!(usage.cache_read, 50);
        assert_eq!(usage.cache_write, 25);
    }
}