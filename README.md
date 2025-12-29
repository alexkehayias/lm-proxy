# lm-proxy

AI GENERATED, DO NOT USE

A Rust-based HTTP proxy server that forwards requests to upstream APIs (such as OpenAI's API) while optionally tracking usage statistics from completion and embedding responses. The proxy sits between clients and API endpoints to provide request forwarding with usage monitoring capabilities.

## Features

- **Request Proxying**: Forwards any HTTP request (GET, POST, PUT, DELETE, etc.) to an upstream API
- **Usage Tracking**: Automatically monitors and logs usage statistics (prompt tokens, completion tokens, total tokens) from:
  - Chat completion endpoints
  - Regular completion endpoints
  - Embedding endpoints
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

The proxy is configured via environment variables:

| Variable       | Default                     | Description                                           |
|----------------|-----------------------------|-------------------------------------------------------|
| `UPSTREAM_URL` | `https://api.openai.com/v1` | The base URL of the upstream API to proxy requests to |
| `LISTEN_ADDR`  | `0.0.0.0:3000`              | The address and port the proxy should listen on       |

### Example

```bash
# Proxy requests to OpenAI's API (default)
cargo run

# Proxy requests to a custom upstream server
UPSTREAM_URL=http://localhost:8080/api LISTEN_ADDR=localhost:3000 cargo run
```

## Usage

### Starting the Server

```bash
cargo run
```

The server will start listening on the configured address (default: `http://0.0.0.0:3000`).

### Making Requests

All requests to the proxy are forwarded to the upstream API, preserving the path:

```bash
# Proxy a chat completion request (with usage tracking)
curl -X POST http://localhost:3000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "gpt-4",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'

# Proxy an embeddings request (with usage tracking)
curl -X POST http://localhost:3000/v1/embeddings \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer YOUR_API_KEY" \
  -d '{
    "model": "text-embedding-ada-002",
    "input": "Hello world"
  }'
```

### Usage Tracking

Usage statistics are automatically logged for tracked endpoints. When processing responses from completion or embedding endpoints, the proxy parses and logs:

```
[USAGE] prompt_tokens=10 completion_tokens=20 total_tokens=30
```

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
   - Constructs the upstream URL by combining `UPSTREAM_URL` with the request path
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
[INFO] Proxy configured: upstream=https://api.openai.com/v1 listen=0.0.0.0:3000
[INFO] Listening on 0.0.0.0:3000
[USAGE] prompt_tokens=15 completion_tokens=42 total_tokens=57
```

## License

This project is provided as-is for educational and development purposes.

---

For questions or issues, please refer to the project repository.
