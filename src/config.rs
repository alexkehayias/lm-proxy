use std::net::SocketAddr;
use std::str::FromStr;

/// A named upstream target
#[derive(Debug, Clone)]
pub struct Upstream {
    pub name: String,
    pub url: String,
}

impl Upstream {
    /// Returns the full URL for a given API path (e.g., "/chat/completions")
    pub fn url_for_path(&self, path: &str) -> String {
        format!("{}{}", self.url.trim_end_matches('/'), path)
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub upstreams: Vec<Upstream>,
    pub listen_addr: SocketAddr,
    pub metrics_url: Option<String>,
}

impl Config {
    /// Find an upstream by name
    pub fn find_upstream(&self, name: &str) -> Option<&Upstream> {
        self.upstreams.iter().find(|u| u.name == name)
    }

    /// Get the default upstream (named "default", or the only one if there's exactly one)
    pub fn default_upstream(&self) -> Option<&Upstream> {
        self.find_upstream("default")
            .or_else(|| if self.upstreams.len() == 1 { self.upstreams.first() } else { None })
    }
}

/// CLI arguments using clap
#[derive(Debug, Default, clap::Parser)]
#[command(name = "lm-proxy")]
#[command(about = "A proxy server for forwarding HTTP requests to upstream APIs", long_about = None)]
pub struct Args {
    /// Upstream API URL(s). Repeatable: --upstream name=https://... or just --upstream https://...
    /// Without a name, the upstream is registered as "default".
    #[arg(long)]
    pub upstream: Vec<String>,

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

        let upstreams = if self.upstream.is_empty() {
            vec![Upstream {
                name: "default".to_string(),
                url: "https://api.openai.com/v1".to_string(),
            }]
        } else {
            self.upstream
                .into_iter()
                .map(|entry| {
                    if let Some(eq_pos) = entry.find('=') {
                        let name = entry[..eq_pos].to_string();
                        let url = entry[eq_pos + 1..].to_string();
                        Upstream { name, url }
                    } else {
                        Upstream {
                            name: "default".to_string(),
                            url: entry,
                        }
                    }
                })
                .collect()
        };

        Ok(Config {
            upstreams,
            listen_addr,
            metrics_url: self.metrics_url,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_upstream(name: &str, url: &str) -> Upstream {
        Upstream {
            name: name.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn test_upstream_url_for_path() {
        let upstream = test_upstream("default", "https://api.openai.com/v1");

        assert_eq!(
            upstream.url_for_path("/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );

        let upstream2 = test_upstream("default", "http://localhost:8080/v1");

        assert_eq!(
            upstream2.url_for_path("/chat/completions"),
            "http://localhost:8080/v1/chat/completions"
        );
    }

    #[test]
    fn test_upstream_url_for_path_trims_trailing_slash() {
        let upstream = test_upstream("default", "https://api.openai.com/v1/");

        assert_eq!(
            upstream.url_for_path("/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_find_upstream() {
        let config = Config {
            upstreams: vec![
                test_upstream("openai", "https://api.openai.com/v1"),
                test_upstream("anthropic", "https://api.anthropic.com/v1"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };

        assert_eq!(config.find_upstream("openai").unwrap().url, "https://api.openai.com/v1");
        assert_eq!(config.find_upstream("anthropic").unwrap().url, "https://api.anthropic.com/v1");
        assert!(config.find_upstream("nonexistent").is_none());
    }

    #[test]
    fn test_default_upstream_named_default() {
        let config = Config {
            upstreams: vec![
                test_upstream("default", "https://api.openai.com/v1"),
                test_upstream("other", "https://other.example.com"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };

        assert_eq!(config.default_upstream().unwrap().url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_default_upstream_single() {
        let config = Config {
            upstreams: vec![
                test_upstream("openai", "https://api.openai.com/v1"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };

        assert_eq!(config.default_upstream().unwrap().name, "openai");
    }

    #[test]
    fn test_default_upstream_none() {
        let config = Config {
            upstreams: vec![
                test_upstream("openai", "https://api.openai.com/v1"),
                test_upstream("anthropic", "https://api.anthropic.com/v1"),
            ],
            listen_addr: SocketAddr::from(([0, 0, 0, 0], 3000)),
            metrics_url: None,
        };

        assert!(config.default_upstream().is_none());
    }

    #[test]
    fn test_args_with_single_unnamed_upstream() {
        let args = Args {
            upstream: vec!["https://api.anthropic.com".to_string()],
            host: "127.0.0.1".to_string(),
            port: 8080,
            metrics_url: Some("http://localhost:9090/metrics".to_string()),
        };

        let config = args.into_config().expect("should create config");
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.upstreams[0].name, "default");
        assert_eq!(config.upstreams[0].url, "https://api.anthropic.com");
        assert_eq!(config.listen_addr.to_string(), "127.0.0.1:8080");
        assert_eq!(config.metrics_url, Some("http://localhost:9090/metrics".to_string()));
    }

    #[test]
    fn test_args_with_named_upstreams() {
        let args = Args {
            upstream: vec![
                "openai=https://api.openai.com/v1".to_string(),
                "anthropic=https://api.anthropic.com/v1".to_string(),
            ],
            host: "0.0.0.0".to_string(),
            port: 3000,
            metrics_url: None,
        };

        let config = args.into_config().expect("should create config");
        assert_eq!(config.upstreams.len(), 2);
        assert_eq!(config.upstreams[0].name, "openai");
        assert_eq!(config.upstreams[0].url, "https://api.openai.com/v1");
        assert_eq!(config.upstreams[1].name, "anthropic");
        assert_eq!(config.upstreams[1].url, "https://api.anthropic.com/v1");
    }

    #[test]
    fn test_args_empty_upstream_defaults() {
        let args = Args {
            upstream: vec![],
            host: "0.0.0.0".to_string(),
            port: 3000,
            metrics_url: None,
        };

        let config = args.into_config().expect("should create config");
        assert_eq!(config.upstreams.len(), 1);
        assert_eq!(config.upstreams[0].name, "default");
        assert_eq!(config.upstreams[0].url, "https://api.openai.com/v1");
    }

    #[test]
    fn test_into_config_invalid_ip() {
        let args = Args {
            upstream: vec!["https://api.openai.com/v1".to_string()],
            host: "not-an-ip".to_string(),
            port: 3000,
            metrics_url: None,
        };

        let result = args.into_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_into_config_port_zero() {
        let args = Args {
            upstream: vec!["https://api.openai.com/v1".to_string()],
            host: "0.0.0.0".to_string(),
            port: 0,
            metrics_url: None,
        };

        let result = args.into_config();
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_with_metrics_url() {
        let config = Config {
            upstreams: vec![test_upstream("default", "https://api.openai.com/v1")],
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            metrics_url: Some("http://localhost:8080/metrics".to_string()),
        };

        assert!(config.metrics_url.is_some());
        assert_eq!(config.metrics_url.unwrap(), "http://localhost:8080/metrics");
    }

    #[test]
    fn test_config_without_metrics_url() {
        let config = Config {
            upstreams: vec![test_upstream("default", "https://api.openai.com/v1")],
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 3000)),
            metrics_url: None,
        };

        assert!(config.metrics_url.is_none());
    }

    #[test]
    fn test_config_ipv4_loopback() {
        let args = Args {
            upstream: vec!["https://api.openai.com/v1".to_string()],
            host: "127.0.0.1".to_string(),
            port: 8080,
            metrics_url: None,
        };

        let config = args.into_config().expect("should create config");
        assert_eq!(config.listen_addr.to_string(), "127.0.0.1:8080");
    }

    #[test]
    fn test_args_multiple_upstreams_mixed_format() {
        let args = Args {
            upstream: vec![
                "default=https://api.openai.com/v1".to_string(),
                "https://other.example.com/api".to_string(), // no name -> becomes default too (overwrites conceptually, but Vec allows duplicates)
            ],
            host: "0.0.0.0".to_string(),
            port: 3000,
            metrics_url: None,
        };

        let config = args.into_config().expect("should create config");
        assert_eq!(config.upstreams.len(), 2);
        assert_eq!(config.upstreams[0].name, "default");
        assert_eq!(config.upstreams[1].name, "default");
    }
}