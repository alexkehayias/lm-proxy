use serde::{Deserialize, Serialize};

/// Usage statistics from OpenAI API responses
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
}

/// Non-streaming completion response
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

/// Attempts to parse usage from a chunk of SSE data
/// Returns None if the chunk doesn't contain usage (most chunks don't)
pub fn try_parse_usage_from_chunk(chunk: &str) -> Option<Usage> {
    // Skip SSE marker lines
    let json = chunk.trim_start_matches("data: ");

    if json == "[DONE]" {
        return None;
    }

    // Try to parse as a CompletionChunk
    if let Ok(chunk) = serde_json::from_str::<CompletionChunk>(json) {
        return chunk.usage;
    }

    // Try to parse as a CompletionResponse (non-streaming case embedded in stream)
    if let Ok(response) = serde_json::from_str::<CompletionResponse>(json) {
        return response.usage;
    }

    None
}

/// Attempts to parse usage from a complete JSON body (non-streaming)
pub fn try_parse_usage_from_body(body: &[u8]) -> Option<Usage> {
    // Try CompletionResponse first
    if let Ok(response) = serde_json::from_slice::<CompletionResponse>(body) {
        return response.usage;
    }

    // Try EmbeddingsResponse
    if let Ok(response) = serde_json::from_slice::<EmbeddingsResponse>(body) {
        return response.usage;
    }

    None
}

/// Check if a request path should have usage tracked (completions/embeddings)
pub fn is_usage_tracked_path(path: &str) -> bool {
    path.contains("/chat/completions")
        || path.ends_with("completions")
        || path.ends_with("/embeddings")
}