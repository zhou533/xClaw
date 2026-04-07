//! Agent engine configuration and runtime context.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use xclaw_memory::role::daily::DailyMemory;
use xclaw_memory::role::manager::RoleManager;
use xclaw_memory::session::store::SessionStore;
use xclaw_memory::session::types::ContentBlockKind;
use xclaw_memory::workspace::loader::MemoryFileLoader;
use xclaw_tools::registry::ToolRegistry;

/// Configuration for the agent loop engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// LLM model identifier (e.g. "gpt-4o", "claude-sonnet-4-5-20250929").
    pub model: String,
    /// Maximum tool-call rounds per conversation turn (loop protection).
    #[serde(default = "default_max_tool_rounds")]
    pub max_tool_rounds: u32,
    /// Number of recent transcript records to load as history.
    #[serde(default = "default_transcript_tail")]
    pub transcript_tail_size: usize,
    /// Optional temperature override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Optional max_tokens override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Content block kinds to include from historical sessions.
    /// Defaults to `{Text}` — other types (ToolCall, ToolResult, etc.) are
    /// excluded unless explicitly added here.
    ///
    /// **Note:** an empty set means "no filtering" (all types included),
    /// not "exclude everything". Always include at least `Text`.
    #[serde(default = "default_history_content_kinds")]
    pub history_content_kinds: BTreeSet<ContentBlockKind>,
    /// Enable debug output (print assembled prompt to stderr).
    /// Skipped from serde — this is a CLI-only runtime flag, never persisted.
    #[serde(skip)]
    pub debug: bool,
}

fn default_max_tool_rounds() -> u32 {
    10
}

fn default_transcript_tail() -> usize {
    20
}

fn default_history_content_kinds() -> BTreeSet<ContentBlockKind> {
    BTreeSet::from([ContentBlockKind::Text])
}

impl AgentConfig {
    /// Create a config with required fields; optional fields use defaults.
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            max_tool_rounds: default_max_tool_rounds(),
            transcript_tail_size: default_transcript_tail(),
            history_content_kinds: default_history_content_kinds(),
            temperature: None,
            max_tokens: None,
            debug: false,
        }
    }

    /// Builder-style setter for max_tool_rounds.
    pub fn with_max_tool_rounds(self, n: u32) -> Self {
        Self {
            max_tool_rounds: n,
            ..self
        }
    }

    /// Builder-style setter for transcript_tail_size.
    pub fn with_transcript_tail(self, n: usize) -> Self {
        Self {
            transcript_tail_size: n,
            ..self
        }
    }

    /// Builder-style setter for history_content_kinds.
    pub fn with_history_content_kinds(self, kinds: BTreeSet<ContentBlockKind>) -> Self {
        Self {
            history_content_kinds: kinds,
            ..self
        }
    }

    /// Builder-style setter for temperature.
    pub fn with_temperature(self, t: f32) -> Self {
        Self {
            temperature: Some(t),
            ..self
        }
    }

    /// Builder-style setter for max_tokens.
    pub fn with_max_tokens(self, n: u32) -> Self {
        Self {
            max_tokens: Some(n),
            ..self
        }
    }

    /// Builder-style setter for debug mode.
    pub fn with_debug(self, enabled: bool) -> Self {
        Self {
            debug: enabled,
            ..self
        }
    }
}

/// Runtime context that holds references to memory subsystems and tools.
///
/// Generic over trait implementations so tests can substitute stubs.
/// Currently unused by `LoopAgent` (which takes subsystems individually).
/// Will be wired in when `RoleOrchestrator` is implemented.
#[allow(dead_code)]
pub(crate) struct AgentContext<'a, S, R, F, D>
where
    S: SessionStore,
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
{
    pub sessions: &'a S,
    pub roles: &'a R,
    pub files: &'a F,
    pub daily: &'a D,
    pub tool_registry: &'a ToolRegistry,
}

impl<'a, S, R, F, D> AgentContext<'a, S, R, F, D>
where
    S: SessionStore,
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
{
    #[allow(dead_code)]
    pub fn new(
        sessions: &'a S,
        roles: &'a R,
        files: &'a F,
        daily: &'a D,
        tool_registry: &'a ToolRegistry,
    ) -> Self {
        Self {
            sessions,
            roles,
            files,
            daily,
            tool_registry,
        }
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use xclaw_memory::session::types::ContentBlockKind;

    use super::*;

    // ── AgentConfig ─────────────────────────────────────────────────────

    #[test]
    fn new_uses_defaults() {
        let cfg = AgentConfig::new("gpt-4o");
        assert_eq!(cfg.model, "gpt-4o");
        assert_eq!(cfg.max_tool_rounds, 10);
        assert_eq!(cfg.transcript_tail_size, 20);
        assert!(cfg.temperature.is_none());
        assert!(cfg.max_tokens.is_none());
    }

    #[test]
    fn builder_sets_max_tool_rounds() {
        let cfg = AgentConfig::new("gpt-4o").with_max_tool_rounds(5);
        assert_eq!(cfg.max_tool_rounds, 5);
    }

    #[test]
    fn builder_sets_transcript_tail() {
        let cfg = AgentConfig::new("gpt-4o").with_transcript_tail(50);
        assert_eq!(cfg.transcript_tail_size, 50);
    }

    #[test]
    fn builder_sets_temperature() {
        let cfg = AgentConfig::new("gpt-4o").with_temperature(0.7);
        assert_eq!(cfg.temperature, Some(0.7));
    }

    #[test]
    fn builder_sets_max_tokens() {
        let cfg = AgentConfig::new("gpt-4o").with_max_tokens(4096);
        assert_eq!(cfg.max_tokens, Some(4096));
    }

    #[test]
    fn builder_chains() {
        let cfg = AgentConfig::new("claude-sonnet-4-5-20250929")
            .with_max_tool_rounds(3)
            .with_transcript_tail(10)
            .with_temperature(0.5)
            .with_max_tokens(2048);
        assert_eq!(cfg.model, "claude-sonnet-4-5-20250929");
        assert_eq!(cfg.max_tool_rounds, 3);
        assert_eq!(cfg.transcript_tail_size, 10);
        assert_eq!(cfg.temperature, Some(0.5));
        assert_eq!(cfg.max_tokens, Some(2048));
    }

    #[test]
    fn serde_roundtrip() {
        let cfg = AgentConfig::new("gpt-4o").with_temperature(0.8);
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, "gpt-4o");
        assert_eq!(back.temperature, Some(0.8));
        assert_eq!(back.max_tool_rounds, 10);
    }

    #[test]
    fn serde_uses_defaults_for_missing_fields() {
        let json = r#"{"model":"gpt-4o"}"#;
        let cfg: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.max_tool_rounds, 10);
        assert_eq!(cfg.transcript_tail_size, 20);
        assert!(cfg.temperature.is_none());
    }

    #[test]
    fn serde_skips_none_optional_fields() {
        let cfg = AgentConfig::new("gpt-4o");
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("temperature"));
        assert!(!json.contains("max_tokens"));
    }

    // ── debug field ─────────────────────────────────────────────────────

    #[test]
    fn debug_defaults_to_false() {
        let cfg = AgentConfig::new("gpt-4o");
        assert!(!cfg.debug);
    }

    #[test]
    fn with_debug_sets_flag() {
        let cfg = AgentConfig::new("gpt-4o").with_debug(true);
        assert!(cfg.debug);
    }

    #[test]
    fn with_debug_chains_with_other_builders() {
        let cfg = AgentConfig::new("gpt-4o")
            .with_debug(true)
            .with_temperature(0.5);
        assert!(cfg.debug);
        assert_eq!(cfg.temperature, Some(0.5));
    }

    #[test]
    fn serde_debug_defaults_to_false_when_missing() {
        let json = r#"{"model":"gpt-4o"}"#;
        let cfg: AgentConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.debug);
    }

    #[test]
    fn serde_skips_debug_field() {
        let cfg = AgentConfig::new("gpt-4o").with_debug(true);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(!json.contains("debug"));
        // Deserialized config always has debug=false (skipped field)
        let back: AgentConfig = serde_json::from_str(&json).unwrap();
        assert!(!back.debug);
    }

    // ── history_content_kinds ───────────────────────────────────────────

    #[test]
    fn new_defaults_history_content_kinds_to_text_only() {
        let cfg = AgentConfig::new("gpt-4o");
        assert_eq!(cfg.history_content_kinds.len(), 1);
        assert!(cfg.history_content_kinds.contains(&ContentBlockKind::Text));
    }

    #[test]
    fn with_history_content_kinds_sets_filter() {
        let kinds: BTreeSet<ContentBlockKind> =
            [ContentBlockKind::Text, ContentBlockKind::ToolCall]
                .into_iter()
                .collect();
        let cfg = AgentConfig::new("gpt-4o").with_history_content_kinds(kinds.clone());
        assert_eq!(cfg.history_content_kinds, kinds);
    }

    #[test]
    fn serde_missing_history_content_kinds_uses_default() {
        let json = r#"{"model":"gpt-4o"}"#;
        let cfg: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.history_content_kinds.len(), 1);
        assert!(cfg.history_content_kinds.contains(&ContentBlockKind::Text));
    }

    #[test]
    fn serde_explicit_history_content_kinds_roundtrip() {
        let kinds: BTreeSet<ContentBlockKind> =
            [ContentBlockKind::Text, ContentBlockKind::ToolCall]
                .into_iter()
                .collect();
        let cfg = AgentConfig::new("gpt-4o").with_history_content_kinds(kinds.clone());
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.history_content_kinds, kinds);
    }
}
