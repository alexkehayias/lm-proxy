# lm-proxy

AI GENERATED, DO NOT USE

A Rust-based HTTP proxy server that forwards requests to upstream APIs (such as OpenAI's API) while optionally tracking usage statistics from completion and embedding responses. The proxy sits between clients and API endpoints to provide request forwarding with usage monitoring capabilities.

## Features

- **Request Proxying**: Forwards any HTTP request (GET, POST, PUT, DELETE, etc.) to an upstream API
- **Usage Tracking**: Automatically monitors and logs normalized usage statistics (input, output, cache read, cache write) from:
  - OpenAI chat completion endpoints
  - OpenAI completion endpoints
  - OpenAI embedding endpoints
  - Anthropic `/v1/messages` API (both streaming and non-streaming)
- **Metrics Posting**: Optionally posts token counts to an external metrics endpoint for aggregation
- **Streaming Support**: Handles both streaming (SSE) and non-streaming responses correctly
- **Header Filtering**: Automatically filters out hop-by-hop headers that shouldn't be forwarded
- **Preserves Metadata**: Maintains HTTP methods, query parameters, and custom headers during proxying

## Installation

### Prerequisites

- Rust 1.91.1 or later
- Cargo (comes with Rust)

### Building

```bash
cargo build --release
```

## Configuration

The proxy can be configured via command-line arguments:

### Arguments

| Flag            | Default                     | Description                                                      |
|-----------------|-----------------------------|------------------------------------------------------------------|
| `--upstream`    | `https://api.openai.com/v1` | Repeatable: `name=url` or bare URL. See examples below.          |
| `--host`        | `0.0.0.0`                   | The host address the proxy should listen on                      |
| `--port`        | `3000`                      | The port the proxy should listen on                              |
| `--metrics-url` | (none)                      | Optional URL to post usage metrics to                            |

### Multi-Upstream Routing

When multiple `--upstream` values are configured, the proxy routes requests based on the first path segment:

```bash
# Configure two upstreams
cargo run -- \
  --upstream openai=https://api.openai.com/v1 \
  --upstream anthropic=https://api.anthropic.com/v1
```

Requests are then routed as follows:

```
POST /openai/chat/completions     →  https://api.openai.com/v1/chat/completions
POST /anthropic/v1/messages       →  https://api.anthropic.com/v1/messages
```

If no upstream name prefix matches, the proxy falls back to an upstream named `"default"`, or the single configured upstream if there is exactly one.

#### Backward Compatibility

Single `--upstream` usage works exactly as before:

```bash
# Single unnamed upstream → name is "default"
cargo run -- --upstream https://api.openai.com/v1

# No --upstream → defaults to --upstream default=https://api.openai.com/v1
cargo run
```

## Running behind Nginx

If you run a reverse proxy in front of `lm-proxy`:
- Set the `max_body_size` to `10m` or higher (default is `1m`) depending on expected usage
- Set `proxy_buffering` to `off` to avoid malformed SSE events

### Examples

```bash
# Proxy requests to OpenAI's API (default)
cargo run

# Proxy requests to a custom upstream server
cargo run -- --upstream http://localhost:8080/api --host localhost --port 3000

# With metrics posting to an external endpoint
cargo run -- --metrics-url http://localhost:8080/metrics

# Multiple upstreams (path-based routing)
cargo run -- \
  --upstream openai=https://api.openai.com/v1 \
  --upstream anthropic=https://api.anthropic.com/v1 \
  --upstream ollama=http://localhost:11434/v1
```

## Usage

### Starting the Server

```bash
cargo run
```

The server will start listening on the configured address (default: `http://0.0.0.0:3000`).

### Making Requests

All requests to the proxy are forwarded to the upstream API, preserving the path.

With a single upstream (backward compatible), the path is forwarded as-is:

```bash
# Proxy a chat completion request (with usage tracking)
curl -X POST http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

With multiple upstreams, prefix the path with the upstream name:

```bash
# Route to the "openai" upstream
curl -X POST http://localhost:3000/openai/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Route to the "anthropic" upstream
curl -X POST http://localhost:3000/anthropic/v1/messages \
  -H "Content-Type: application/json" \
  -H "x-api-key: YOUR_ANTHROPIC_KEY" \
  -d '{
    "model": "claude-sonnet-4-6",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

### Usage Tracking

Usage statistics are automatically logged for tracked endpoints (completions, embeddings, Anthropic messages). The proxy normalizes both OpenAI and Anthropic usage into a common four-bucket shape:

| Bucket       | OpenAI source                                              | Anthropic source                       |
|--------------|------------------------------------------------------------|----------------------------------------|
| `input`      | `prompt_tokens - cached_tokens` (fresh, non-cached input) | `input_tokens`                         |
| `output`     | `completion_tokens` (includes reasoning tokens)            | `output_tokens`                        |
| `cache_read`  | `prompt_tokens_details.cached_tokens`                      | `cache_read_input_tokens`              |
| `cache_write` | always 0 (OpenAI auto-caches; writes aren't billed)       | `cache_creation_input_tokens`          |
| `reasoning`   | `completion_tokens_details.reasoning_tokens` (optional)    | `output_tokens_details.thinking_tokens` (optional) |

Each bucket is billed at a different rate, so preserving all four lets downstream cost estimators apply provider-specific pricing. The log format is:

```
[USAGE] input=10 output=42 cache_read=5 cache_write=0 reasoning=None
```

#### Metrics Posting

When `--metrics-url` is configured, the proxy POSTs a JSON payload to the specified endpoint after each request. The payload carries all four buckets (plus optional `reasoning`):

```json
{
  "input": 10,
  "output": 42,
  "cache_read": 5,
  "cache_write": 0,
  "reasoning": null
}
```

Metrics are posted asynchronously (fire-and-forget) to avoid impacting request latency.

#### Streaming Caveat: Per-Chunk Double-Counting

For **streaming** responses, the proxy parses usage from each SSE chunk independently and fires a metrics POST for every chunk that contains a `usage` field. This is simple but introduces two caveats specific to Anthropic streams:

1. **Two events per request.** Anthropic emits usage on `message_start` (carrying `input_tokens`) and again on the final `message_delta` (carrying cumulative `output_tokens`). Downstream sees ~2 metrics events per Anthropic stream, not 1.

2. **Small overcount on `output`.** The final `message_delta`'s `output_tokens` is *cumulative* for the whole response (it already includes the placeholder `1` from `message_start`). Summing both events' `output` overcounts by ~1 token (bounded, sub-percent error on typical responses).

OpenAI streams do not have this issue — usage is emitted only on the final chunk.

Non-streaming responses are unaffected: a single metrics event is posted per request with the complete `Usage`.

If accurate per-request accounting matters more to you than simplicity, a future change could accumulate across chunks and post one event when the stream ends. For now, downstream consumers should treat Anthropic streaming events as **additive across `input`** (only one event carries input) but **non-additive across `output`** (the final `message_delta` is authoritative; earlier values are placeholders).

## Development

### Running Tests

```bash
# Run all tests (including integration tests)
cargo test

# Run only unit tests
cargo test --lib

# Run only end-to-end tests
cargo test --test e2e_test

# Run tests with output
cargo test -- --nocapture
```

### Project Structure

```
lm-proxy/
├── src/
│   ├── main.rs      # Entry point and server setup
│   ├── handler.rs   # ProxyService implementation
│   ├── models.rs    # Data structures for API responses and usage tracking
│   ├── config.rs    # Configuration management
│   └── lib.rs       # Library exports (for integration tests)
├── tests/
│   └── e2e_test.rs  # End-to-end integration tests
├── Cargo.toml       # Project dependencies and metadata
└── README.md        # This file
```

### Code Quality

```bash
# Run Clippy for linting
cargo clippy

# Check code without building
cargo check
```

## Architecture

The proxy server follows a simple request forwarding architecture:

1. **Request Reception**: Axum HTTP server receives incoming requests
2. **Processing**:
   - Extracts method, URI, headers, and body from the request
   - Filters out hop-by-hop headers (connection, keep-alive, transfer-encoding, etc.)
   - Constructs the upstream URL by combining the configured upstream URL with the request path
3. **Forwarding**: Uses reqwest HTTP client to forward the request to upstream
4. **Response Handling**:
   - For non-streaming responses: Tracks usage from response body if applicable
   - For streaming responses (text/event-stream): Parses usage data on-the-fly from SSE chunks and logs it
   - Returns the upstream response (status, headers, body) to the client

### Hop-by-hop Header Filtering

The proxy automatically filters out hop-by-hop headers that should not be forwarded according to HTTP specifications:

- `connection`
- `keep-alive`
- `proxy-authenticate`
- `proxy-authorization`
- `te`
- `trailers`
- `transfer-encoding`
- `upgrade`

## Example Output

When serving requests with usage tracking:

```
[INFO] Starting lm-proxy...
[INFO] Upstream configured: name=openai url=https://api.openai.com/v1
[INFO] Upstream configured: name=anthropic url=https://api.anthropic.com/v1
[INFO] Listening on 0.0.0.0:3000
[USAGE] input=10 output=42 cache_read=5 cache_write=0 reasoning=None
```

## License

This project is provided as-is for educational and development purposes.

---

For questions or issues, please refer to the project repository.
