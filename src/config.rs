use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Config {
    pub upstream_url: String,
    pub listen_addr: SocketAddr,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            upstream_url: std::env::var("UPSTREAM_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            listen_addr: std::env::var("LISTEN_ADDR")
                .ok()
                .and_then(|addr| SocketAddr::from_str(&addr).ok())
                .unwrap_or_else(|| SocketAddr::from(([0, 0, 0, 0], 3000))),
        }
    }
}

impl Config {
    pub fn from_env() -> Self {
        Self::default()
    }

    /// Returns the full URL for a given API path (e.g., "/chat/completions")
    pub fn upstream_url_for_path(&self, path: &str) -> String {
        format!("{}{}", self.upstream_url.trim_end_matches('/'), path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upstream_url_for_path() {
        let config = Config {
            upstream_url: "https://api.openai.com/v1".to_string(),
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
        };

        assert_eq!(
            config.upstream_url_for_path("/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );

        let config2 = Config {
            upstream_url: "http://localhost:8080/v1".to_string(),
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
        };

        assert_eq!(
            config2.upstream_url_for_path("/chat/completions"),
            "http://localhost:8080/v1/chat/completions"
        );
    }
}