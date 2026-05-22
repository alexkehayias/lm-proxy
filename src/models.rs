use serde::{Deserialize, Serialize};

/// Usage statistics from API responses
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Usage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completion_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u32>,
}

impl Usage {
    /// Returns a formatted string for logging
    pub fn log_format(&self) -> String {
        format!(
            "prompt_tokens={:?} completion_tokens={:?} total_tokens={:?}",
            self.prompt_tokens, self.completion_tokens, self.total_tokens
        )
    }

    /// Returns the total token count
    pub fn total(&self) -> Option<u32> {
        self.total_tokens.or_else(|| {
            self.prompt_tokens.and_then(|p| self.completion_tokens.map(|c| p + c))
        })
    }
}

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
    pub usage: Option<Usage>,
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
    pub usage: Option<Usage>,
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
    pub usage: Option<Usage>,
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

/// Anthropic usage statistics (normalized to common Usage)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnthropicUsage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit_input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_miss_input_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speculative_token_count: Option<u32>,
}

impl AnthropicUsage {
    /// Normalize Anthropic usage to common Usage format
    pub fn normalize(&self) -> Usage {
        Usage {
            prompt_tokens: self.input_tokens,
            completion_tokens: self.output_tokens,
            total_tokens: self
                .input_tokens
                .and_then(|i| self.output_tokens.map(|o| i + o)),
        }
    }
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
    if let Ok(chunk) = serde_json::from_str::<CompletionChunk>(json) {
        return chunk.usage;
    }

    // OpenAI completion without streaming
    if let Ok(response) = serde_json::from_str::<CompletionResponse>(json) {
        return response.usage;
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
pub fn try_parse_usage_from_body(body: &[u8]) -> Option<Usage> {
    // OpenAI completion
    if let Ok(response) = serde_json::from_slice::<CompletionResponse>(body) {
        return response.usage;
    }

    // OpenAI embeddings
    if let Ok(response) = serde_json::from_slice::<EmbeddingsResponse>(body) {
        return response.usage;
    }

    // Anthropic messages
    if let Ok(response) = serde_json::from_slice::<AnthropicMessageResponse>(body) {
        return response.usage.map(|u| u.normalize());
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

    #[test]
    fn test_usage_log_format() {
        let usage = Usage {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
        };
        assert_eq!(
            usage.log_format(),
            "prompt_tokens=Some(100) completion_tokens=Some(50) total_tokens=Some(150)"
        );
    }

    #[test]
    fn test_usage_log_format_with_none() {
        let usage = Usage::default();
        assert_eq!(
            usage.log_format(),
            "prompt_tokens=None completion_tokens=None total_tokens=None"
        );
    }

    #[test]
    fn test_usage_total_with_explicit_total() {
        let usage = Usage {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: Some(150),
        };
        assert_eq!(usage.total(), Some(150));
    }

    #[test]
    fn test_usage_total_calculated_from_prompt_and_completion() {
        let usage = Usage {
            prompt_tokens: Some(100),
            completion_tokens: Some(50),
            total_tokens: None,
        };
        assert_eq!(usage.total(), Some(150));
    }

    #[test]
    fn test_usage_total_with_only_prompt() {
        let usage = Usage {
            prompt_tokens: Some(100),
            completion_tokens: None,
            total_tokens: None,
        };
        assert_eq!(usage.total(), None);
    }

    #[test]
    fn test_usage_total_with_only_completion() {
        let usage = Usage {
            prompt_tokens: None,
            completion_tokens: Some(50),
            total_tokens: None,
        };
        assert_eq!(usage.total(), None);
    }

    #[test]
    fn test_anthropic_usage_normalize() {
        let anthropic_usage = AnthropicUsage {
            input_tokens: Some(200),
            output_tokens: Some(100),
            cache_hit_input_tokens: None,
            cache_miss_input_tokens: None,
            speculative_token_count: None,
        };

        let normalized = anthropic_usage.normalize();
        assert_eq!(normalized.prompt_tokens, Some(200));
        assert_eq!(normalized.completion_tokens, Some(100));
        assert_eq!(normalized.total_tokens, Some(300));
    }

    #[test]
    fn test_anthropic_usage_normalize_with_none() {
        let anthropic_usage = AnthropicUsage::default();
        let normalized = anthropic_usage.normalize();
        assert_eq!(normalized.prompt_tokens, None);
        assert_eq!(normalized.completion_tokens, None);
        assert_eq!(normalized.total_tokens, None);
    }

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

    #[test]
    fn test_try_parse_usage_from_chunk_openai_completion() {
        let chunk = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","created":1677652288,"model":"gpt-4","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(10));
        assert_eq!(usage.completion_tokens, Some(5));
        assert_eq!(usage.total_tokens, Some(15));
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
        assert_eq!(usage.prompt_tokens, Some(10));
        assert_eq!(usage.completion_tokens, Some(5));
    }

    #[test]
    fn test_try_parse_usage_from_body_embeddings() {
        let body = br#"{"object":"list","data":[{"object":"embedding","embedding":[0.1,0.2],"index":0}],"model":"text-embedding-ada-002","usage":{"prompt_tokens":8,"completion_tokens":0,"total_tokens":8}}"#;

        let usage = try_parse_usage_from_body(body);
        assert!(usage.is_some());
        let usage = usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(8));
    }

    #[test]
    fn test_try_parse_usage_from_body_no_match() {
        let body = br#"{"unknown":"response"}"#;
        let usage = try_parse_usage_from_body(body);
        assert!(usage.is_none());
    }

    #[test]
    fn test_try_parse_usage_from_chunk_anthropic_message_start() {
        // Anthropic streaming message_start event
        let chunk = r#"event: message_start
data: {"type": "message_start", "message": {"id": "msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY", "type": "message", "role": "assistant", "content": [], "model": "claude-opus-4-6", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some(), "Should parse usage from Anthropic message_start event");
        let usage = usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(25), "input_tokens should be 25");
        assert_eq!(usage.completion_tokens, Some(1), "output_tokens should be 1");
    }

    #[test]
    fn test_try_parse_usage_from_chunk_anthropic_message_delta() {
        // Anthropic streaming message_delta event with usage
        let chunk = r#"event: message_delta
data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence": null}, "usage": {"output_tokens": 15}}"#;

        let usage = try_parse_usage_from_chunk(chunk);
        assert!(usage.is_some(), "Should parse usage from Anthropic message_delta event");
        let usage = usage.unwrap();
        assert_eq!(usage.completion_tokens, Some(15), "output_tokens should be 15");
    }
}
