//! Provider error types.

/// Errors returned by any `LlmProvider` implementation.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("rate limited (retry after: {retry_after:?})")]
    RateLimit {
        retry_after: Option<std::time::Duration>,
    },

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("server error (status {status}): {body}")]
    ServerError { status: u16, body: String },

    #[error("stream closed unexpectedly")]
    StreamClosed,

    #[error("deserialization error: {0}")]
    Deserialize(String),
}

impl From<reqwest::Error> for ProviderError {
    fn from(err: reqwest::Error) -> Self {
        Self::Network(err.without_url().to_string())
    }
}

impl From<serde_json::Error> for ProviderError {
    fn from(err: serde_json::Error) -> Self {
        Self::Deserialize(err.to_string())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn auth_error_display() {
        let e = ProviderError::Auth("bad key".to_string());
        assert_eq!(e.to_string(), "authentication failed: bad key");
    }

    #[test]
    fn rate_limit_with_retry_after_display() {
        let e = ProviderError::RateLimit {
            retry_after: Some(Duration::from_secs(30)),
        };
        let s = e.to_string();
        assert!(s.contains("rate limited"));
        assert!(s.contains("30s"));
    }

    #[test]
    fn rate_limit_without_retry_after_display() {
        let e = ProviderError::RateLimit { retry_after: None };
        let s = e.to_string();
        assert!(s.contains("rate limited"));
    }

    #[test]
    fn invalid_request_display() {
        let e = ProviderError::InvalidRequest("missing model".to_string());
        assert_eq!(e.to_string(), "invalid request: missing model");
    }

    #[test]
    fn network_error_display() {
        let e = ProviderError::Network("connection refused".to_string());
        assert_eq!(e.to_string(), "network error: connection refused");
    }

    #[test]
    fn server_error_display() {
        let e = ProviderError::ServerError {
            status: 500,
            body: "internal error".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("500"));
        assert!(s.contains("internal error"));
    }

    #[test]
    fn stream_closed_display() {
        let e = ProviderError::StreamClosed;
        assert_eq!(e.to_string(), "stream closed unexpectedly");
    }

    #[test]
    fn deserialize_error_display() {
        let e = ProviderError::Deserialize("unexpected field".to_string());
        assert_eq!(e.to_string(), "deserialization error: unexpected field");
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("{bad}").unwrap_err();
        let provider_err = ProviderError::from(json_err);
        assert!(matches!(provider_err, ProviderError::Deserialize(_)));
    }
}
