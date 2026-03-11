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
#[derive(Debug, Default, clap::Parser)]
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

    #[test]
    fn test_upstream_url_for_path_trims_trailing_slash() {
        let config = Config {
            upstream_url: "https://api.openai.com/v1/".to_string(),
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };

        assert_eq!(
            config.upstream_url_for_path("/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_args_with_custom_values() {
        let args = Args {
            upstream: "https://api.anthropic.com".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            metrics_url: Some("http://localhost:9090/metrics".to_string()),
        };

        let config = args.into_config().expect("should create config");
        assert_eq!(config.upstream_url, "https://api.anthropic.com");
        assert_eq!(config.listen_addr.to_string(), "127.0.0.1:8080");
        assert_eq!(config.metrics_url, Some("http://localhost:9090/metrics".to_string()));
    }

    #[test]
    fn test_into_config_invalid_ip() {
        let args = Args {
            upstream: "https://api.openai.com/v1".to_string(),
            host: "not-an-ip".to_string(),
            port: 3000,
            metrics_url: None,
        };

        let result = args.into_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_into_config_port_zero() {
        // Port 0 is actually valid in Rust (OS assigns a port)
        let args = Args {
            upstream: "https://api.openai.com/v1".to_string(),
            host: "0.0.0.0".to_string(),
            port: 0,
            metrics_url: None,
        };

        let result = args.into_config();
        // Port 0 is valid - the OS assigns an ephemeral port
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_with_metrics_url() {
        let config = Config {
            upstream_url: "https://api.openai.com/v1".to_string(),
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            metrics_url: Some("http://localhost:8080/metrics".to_string()),
        };

        assert!(config.metrics_url.is_some());
        assert_eq!(config.metrics_url.unwrap(), "http://localhost:8080/metrics");
    }

    #[test]
    fn test_config_without_metrics_url() {
        let config = Config {
            upstream_url: "https://api.openai.com/v1".to_string(),
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            metrics_url: None,
        };

        assert!(config.metrics_url.is_none());
    }

    #[test]
    fn test_config_ipv4_loopback() {
        let args = Args {
            upstream: "https://api.openai.com/v1".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            metrics_url: None,
        };

        let config = args.into_config().expect("should create config");
        assert_eq!(config.listen_addr.to_string(), "127.0.0.1:8080");
    }
}