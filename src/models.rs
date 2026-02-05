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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

/// Attempts to parse usage from a chunk of SSE data
/// Returns None if the chunk doesn't contain usage (most chunks don't)
pub fn try_parse_usage_from_chunk(chunk: &str) -> Option<Usage> {
    // Skip SSE marker lines
    let json = chunk.trim_start_matches("data: ");

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

    // Anthropic streaming
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
