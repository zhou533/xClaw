//! Configuration data structures.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

// ─── Constants ──────────────────────────────────────────────────────────────

pub const DEFAULT_OPENAI_MODEL: &str = "gpt-4o";
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-sonnet-4-5-20250929";
pub const DEFAULT_MINIMAX_MODEL: &str = "MiniMax-M2";

// ─── ProviderKind ───────────────────────────────────────────────────────────

/// Supported LLM provider backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    OpenAi,
    Claude,
    MiniMax,
}

impl ProviderKind {
    /// Returns the default model identifier for this provider.
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::OpenAi => DEFAULT_OPENAI_MODEL,
            Self::Claude => DEFAULT_CLAUDE_MODEL,
            Self::MiniMax => DEFAULT_MINIMAX_MODEL,
        }
    }
}

impl fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::OpenAi => "openai",
            Self::Claude => "claude",
            Self::MiniMax => "minimax",
        };
        f.write_str(s)
    }
}

impl FromStr for ProviderKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "openai" => Ok(Self::OpenAi),
            "claude" => Ok(Self::Claude),
            "minimax" => Ok(Self::MiniMax),
            other => Err(format!(
                "unknown provider: '{other}'. Expected one of: openai, claude, minimax"
            )),
        }
    }
}

// ─── ProviderConfig ─────────────────────────────────────────────────────────

/// Configuration for the active LLM provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub kind: ProviderKind,
    pub api_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization: Option<String>,
}

// ─── AppConfig ──────────────────────────────────────────────────────────────

/// Top-level application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub provider: ProviderConfig,
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ProviderKind::from_str ──────────────────────────────────────────

    #[test]
    fn parses_openai_lowercase() {
        let kind: ProviderKind = "openai".parse().unwrap();
        assert_eq!(kind, ProviderKind::OpenAi);
    }

    #[test]
    fn parses_claude_lowercase() {
        let kind: ProviderKind = "claude".parse().unwrap();
        assert_eq!(kind, ProviderKind::Claude);
    }

    #[test]
    fn parses_minimax_lowercase() {
        let kind: ProviderKind = "minimax".parse().unwrap();
        assert_eq!(kind, ProviderKind::MiniMax);
    }

    #[test]
    fn parses_case_insensitive() {
        assert_eq!(
            "OpenAI".parse::<ProviderKind>().unwrap(),
            ProviderKind::OpenAi
        );
        assert_eq!(
            "CLAUDE".parse::<ProviderKind>().unwrap(),
            ProviderKind::Claude
        );
        assert_eq!(
            "MiniMax".parse::<ProviderKind>().unwrap(),
            ProviderKind::MiniMax
        );
    }

    #[test]
    fn rejects_unknown_provider() {
        let result = "gemini".parse::<ProviderKind>();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("unknown provider"));
        assert!(err.contains("gemini"));
    }

    // ── ProviderKind::default_model ─────────────────────────────────────

    #[test]
    fn default_model_openai() {
        assert_eq!(ProviderKind::OpenAi.default_model(), "gpt-4o");
    }

    #[test]
    fn default_model_claude() {
        assert_eq!(
            ProviderKind::Claude.default_model(),
            "claude-sonnet-4-5-20250929"
        );
    }

    #[test]
    fn default_model_minimax() {
        assert_eq!(ProviderKind::MiniMax.default_model(), "MiniMax-M2");
    }

    // ── Display ─────────────────────────────────────────────────────────

    #[test]
    fn display_matches_parse_input() {
        for kind in [
            ProviderKind::OpenAi,
            ProviderKind::Claude,
            ProviderKind::MiniMax,
        ] {
            let s = kind.to_string();
            let back: ProviderKind = s.parse().unwrap();
            assert_eq!(back, kind);
        }
    }

    // ── Serde round-trip ────────────────────────────────────────────────

    #[test]
    fn provider_kind_serde_round_trip() {
        for kind in [
            ProviderKind::OpenAi,
            ProviderKind::Claude,
            ProviderKind::MiniMax,
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            let back: ProviderKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
    }

    #[test]
    fn provider_config_serde_round_trip() {
        let config = ProviderConfig {
            kind: ProviderKind::Claude,
            api_key: "sk-test".to_string(),
            base_url: Some("https://example.com".to_string()),
            model: "claude-sonnet-4-5-20250929".to_string(),
            organization: None,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, ProviderKind::Claude);
        assert_eq!(back.api_key, "sk-test");
        assert_eq!(back.base_url, Some("https://example.com".to_string()));
        assert!(back.organization.is_none());
    }

    #[test]
    fn app_config_serde_round_trip() {
        let config = AppConfig {
            provider: ProviderConfig {
                kind: ProviderKind::OpenAi,
                api_key: "key".to_string(),
                base_url: None,
                model: "gpt-4o".to_string(),
                organization: Some("org-123".to_string()),
            },
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.provider.kind, ProviderKind::OpenAi);
        assert_eq!(back.provider.organization, Some("org-123".to_string()));
    }
}
