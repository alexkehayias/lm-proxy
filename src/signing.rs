//! HMAC signing for outbound webhook requests.
//!
//! The scheme: sign `<unix timestamp>.<raw body>` with HMAC-SHA256 and send
//! the digest as `sha256=<hex>` in `X-Webhook-Signature`, with the timestamp in
//! `X-Webhook-Timestamp`. Receivers reject timestamps more than ±5 minutes from
//! now (replay protection) and signatures that don't match the body.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Header carrying the Unix timestamp the body was signed with.
pub const TIMESTAMP_HEADER: &str = "X-Webhook-Timestamp";

/// Header carrying the `sha256=<hex>` HMAC digest.
pub const SIGNATURE_HEADER: &str = "X-Webhook-Signature";

/// Sign `timestamp.body` with HMAC-SHA256, returning `sha256=<hex>`.
pub fn sign(secret: &str, timestamp: u64, body: &[u8]) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts keys of any length");
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vector computed with:
    /// `printf '%s.%s' 1700000000 '<body>' | openssl dgst -sha256 -hmac test-secret -hex`
    #[test]
    fn test_sign_matches_openssl_vector() {
        let body = br#"{"input":1,"output":2,"cache_read":3,"cache_write":4}"#;

        assert_eq!(
            sign("test-secret", 1700000000, body),
            "sha256=9aa257677513e088c1d2c62b77dc092d8fd046c76ccbedcd99cb37b2523f4078"
        );
    }

    #[test]
    fn test_sign_covers_timestamp_and_body() {
        let body = br#"{"input":1}"#;

        assert_ne!(
            sign("test-secret", 1700000000, body),
            sign("test-secret", 1700000001, body)
        );
        assert_ne!(
            sign("test-secret", 1700000000, body),
            sign("test-secret", 1700000000, br#"{"input":2}"#)
        );
        assert_ne!(
            sign("test-secret", 1700000000, body),
            sign("other-secret", 1700000000, body)
        );
    }

    #[test]
    fn test_sign_accepts_empty_secret_and_body() {
        assert_eq!(sign("", 0, b""), sign("", 0, b""));
    }
}
