use std::net::SocketAddr;
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Config {
    pub upstream_url: String,
    pub listen_addr: SocketAddr,
    pub metrics_url: Option<String>,
}

impl Config {

    /// Returns the full URL for a given API path (e.g., "/chat/completions")
    pub fn upstream_url_for_path(&self, path: &str) -> String {
        format!("{}{}", self.upstream_url.trim_end_matches('/'), path)
    }
}

/// CLI arguments using clap
#[derive(Debug, clap::Parser)]
#[command(name = "lm-proxy")]
#[command(about = "A proxy server for forwarding HTTP requests to upstream APIs", long_about = None)]
pub struct Args {
    /// Upstream API URL (e.g., https://api.openai.com/v1)
    #[arg(long, default_value = "https://api.openai.com/v1")]
    pub upstream: String,

    /// Host address to listen on (e.g., 0.0.0.0 or 127.0.0.1)
    #[arg(long, default_value = "0.0.0.0")]
    pub host: String,

    /// Port to listen on
    #[arg(short, long, default_value_t = 3000)]
    pub port: u16,

    /// URL to post usage metrics (e.g., http://localhost:8080/metrics)
    #[arg(long)]
    pub metrics_url: Option<String>,
}

impl Args {
    /// Convert CLI args to Config
    pub fn into_config(self) -> Result<Config, Box<dyn std::error::Error>> {
        let listen_addr_str = format!("{}:{}", self.host, self.port);
        let listen_addr = SocketAddr::from_str(&listen_addr_str)?;

        Ok(Config {
            upstream_url: self.upstream,
            listen_addr,
            metrics_url: self.metrics_url,
        })
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
            metrics_url: None,
        };

        assert_eq!(
            config.upstream_url_for_path("/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );

        let config2 = Config {
            upstream_url: "http://localhost:8080/v1".to_string(),
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };

        assert_eq!(
            config2.upstream_url_for_path("/chat/completions"),
            "http://localhost:8080/v1/chat/completions"
        );
    }
}