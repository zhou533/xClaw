//! Prompt construction and template management.
//!
//! Assembles multi-layer system prompts from role config, workspace memory
//! files, and daily memory. Builds `ChatRequest` with history, tools, and
//! the current user message.

use xclaw_memory::role::config::RoleConfig;
use xclaw_memory::session::types::TranscriptRecord;
use xclaw_memory::workspace::types::{MemoryFileKind, MemorySnapshot};
use xclaw_provider::types::{ChatRequest, Message, Role, ToolDefinition};
use xclaw_tools::traits::ToolSchema;

use crate::session::transcript_to_messages;

// ─── SystemPromptBuilder ────────────────────────────────────────────────────

/// Assembles a multi-layer system prompt from role config and memory files.
///
/// Layers (in order):
/// 1. Base system prompt from `RoleConfig.system_prompt`
/// 2. Persona (`SOUL.md`)
/// 3. Guidelines (`AGENTS.md`)
/// 4. Tool guidance (`TOOLS.md`)
/// 5. Long-term memory (`MEMORY.md`)
/// 6. Daily memory (today's notes)
///
/// Each section is only included if its content is non-empty.
pub struct SystemPromptBuilder {
    sections: Vec<String>,
}

impl Default for SystemPromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemPromptBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    /// Add the base system prompt from role config.
    ///
    /// Priority: BOOTSTRAP.md content (if present and non-empty in the snapshot)
    /// takes precedence over `config.system_prompt`. Falls back to generating
    /// a default prompt when both are absent/empty.
    pub fn with_role_config(mut self, config: &RoleConfig, snapshot: &MemorySnapshot) -> Self {
        let bootstrap = snapshot
            .files
            .get(&MemoryFileKind::Bootstrap)
            .and_then(|opt| opt.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());

        let base = if let Some(content) = bootstrap {
            content.to_string()
        } else if config.system_prompt.is_empty() {
            format!(
                "You are xClaw, a helpful AI assistant. Your role is: {}.",
                config.name
            )
        } else {
            config.system_prompt.clone()
        };
        self.sections.push(base);
        self
    }

    /// Add workspace memory files (SOUL, AGENTS, TOOLS, MEMORY).
    pub fn with_memory_snapshot(mut self, snapshot: &MemorySnapshot) -> Self {
        let layers: &[(MemoryFileKind, &str)] = &[
            (MemoryFileKind::Soul, "Persona"),
            (MemoryFileKind::Agents, "Guidelines"),
            (MemoryFileKind::Tools, "Tool Guidance"),
            (MemoryFileKind::LongTerm, "Long-term Memory"),
        ];

        for (kind, heading) in layers {
            if let Some(Some(content)) = snapshot.files.get(kind) {
                let trimmed = content.trim();
                if !trimmed.is_empty() {
                    self.sections.push(format!("\n## {heading}\n{trimmed}"));
                }
            }
        }
        self
    }

    /// Add today's daily memory notes.
    pub fn with_daily_memory(mut self, content: Option<&str>) -> Self {
        if let Some(c) = content {
            let trimmed = c.trim();
            if !trimmed.is_empty() {
                self.sections.push(format!("\n## Today's Notes\n{trimmed}"));
            }
        }
        self
    }

    /// Build the final system prompt string.
    pub fn build(self) -> String {
        self.sections.join("\n")
    }
}

// ─── ChatRequestBuilder ─────────────────────────────────────────────────────

/// Builds a `ChatRequest` with system prompt, history, user message, and tools.
pub struct ChatRequestBuilder {
    model: String,
    system_prompt: Option<String>,
    history_messages: Vec<Message>,
    user_message: Option<String>,
    tools: Vec<ToolDefinition>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
    stream: bool,
}

impl ChatRequestBuilder {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            system_prompt: None,
            history_messages: Vec::new(),
            user_message: None,
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            stream: false,
        }
    }

    pub fn with_system_prompt(self, prompt: &str) -> Self {
        Self {
            system_prompt: Some(prompt.to_string()),
            ..self
        }
    }

    /// Add conversation history from transcript records.
    ///
    /// Includes Text, ToolCall, and ToolResult blocks.
    /// Excludes Thinking, Image, Unknown, and any future block kinds.
    pub fn with_history(self, records: &[TranscriptRecord]) -> Self {
        Self {
            history_messages: transcript_to_messages(records),
            ..self
        }
    }

    /// Add pre-converted history messages directly.
    pub fn with_history_messages(self, messages: Vec<Message>) -> Self {
        Self {
            history_messages: messages,
            ..self
        }
    }

    pub fn with_user_message(self, content: &str) -> Self {
        Self {
            user_message: Some(content.to_string()),
            ..self
        }
    }

    /// Add tool definitions from `ToolSchema` list (from `ToolRegistry::list_schemas`).
    pub fn with_tool_schemas(self, schemas: &[ToolSchema]) -> Self {
        let tools = schemas
            .iter()
            .map(|s| ToolDefinition {
                name: s.name.clone(),
                description: s.description.clone(),
                parameters: s.parameters.clone(),
            })
            .collect();
        Self { tools, ..self }
    }

    pub fn with_temperature(self, t: Option<f32>) -> Self {
        Self {
            temperature: t,
            ..self
        }
    }

    pub fn with_max_tokens(self, n: Option<u32>) -> Self {
        Self {
            max_tokens: n,
            ..self
        }
    }

    pub fn with_stream(self, stream: bool) -> Self {
        Self { stream, ..self }
    }

    /// Build the final `ChatRequest`.
    pub fn build(self) -> ChatRequest {
        let mut messages = Vec::new();

        // System message
        if let Some(prompt) = self.system_prompt {
            messages.push(Message {
                role: Role::System,
                content: Some(prompt),
                tool_calls: vec![],
                tool_call_id: None,
            });
        }

        // Conversation history
        messages.extend(self.history_messages);

        // Current user message
        if let Some(content) = self.user_message {
            messages.push(Message {
                role: Role::User,
                content: Some(content),
                tool_calls: vec![],
                tool_call_id: None,
            });
        }

        ChatRequest {
            model: self.model,
            messages,
            tools: self.tools,
            temperature: self.temperature,
            max_tokens: self.max_tokens,
            stream: self.stream,
        }
    }
}

/// Legacy helper — build a minimal `ChatRequest` for a single user message.
///
/// Kept for backward compatibility with `SimpleAgent`.
pub fn build_chat_request(model: &str, user_content: &str) -> ChatRequest {
    ChatRequestBuilder::new(model)
        .with_system_prompt("You are xClaw, a helpful AI assistant.")
        .with_user_message(user_content)
        .build()
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use xclaw_memory::role::config::RoleMeta;

    // ── SystemPromptBuilder ─────────────────────────────────────────────

    #[test]
    fn empty_builder_produces_empty_string() {
        let prompt = SystemPromptBuilder::new().build();
        assert!(prompt.is_empty());
    }

    #[test]
    fn with_role_config_uses_system_prompt() {
        let config = RoleConfig {
            name: "secretary".into(),
            description: vec![],
            system_prompt: "You are a secretary.".into(),
            tools: vec![],
            meta: RoleMeta::default(),
            memory_dir: String::new(),
        };
        let snapshot = MemorySnapshot {
            files: HashMap::new(),
        };
        let prompt = SystemPromptBuilder::new()
            .with_role_config(&config, &snapshot)
            .build();
        assert_eq!(prompt, "You are a secretary.");
    }

    #[test]
    fn with_role_config_empty_prompt_generates_default() {
        let config = RoleConfig::default_config();
        let snapshot = MemorySnapshot {
            files: HashMap::new(),
        };
        let prompt = SystemPromptBuilder::new()
            .with_role_config(&config, &snapshot)
            .build();
        assert!(prompt.contains("xClaw"));
        assert!(prompt.contains("default"));
    }

    #[test]
    fn with_role_config_prefers_bootstrap_over_system_prompt() {
        let config = RoleConfig {
            name: "secretary".into(),
            description: vec![],
            system_prompt: "You are a secretary.".into(),
            tools: vec![],
            meta: RoleMeta::default(),
            memory_dir: String::new(),
        };
        let mut files = HashMap::new();
        files.insert(
            MemoryFileKind::Bootstrap,
            Some("Bootstrap instructions here.".to_string()),
        );
        let snapshot = MemorySnapshot { files };
        let prompt = SystemPromptBuilder::new()
            .with_role_config(&config, &snapshot)
            .build();
        assert_eq!(prompt, "Bootstrap instructions here.");
        assert!(!prompt.contains("secretary"));
    }

    #[test]
    fn with_role_config_falls_back_when_bootstrap_empty() {
        let config = RoleConfig {
            name: "secretary".into(),
            description: vec![],
            system_prompt: "You are a secretary.".into(),
            tools: vec![],
            meta: RoleMeta::default(),
            memory_dir: String::new(),
        };
        let mut files = HashMap::new();
        files.insert(MemoryFileKind::Bootstrap, Some("   ".to_string()));
        let snapshot = MemorySnapshot { files };
        let prompt = SystemPromptBuilder::new()
            .with_role_config(&config, &snapshot)
            .build();
        assert_eq!(prompt, "You are a secretary.");
    }

    #[test]
    fn with_role_config_falls_back_when_bootstrap_none() {
        let config = RoleConfig {
            name: "secretary".into(),
            description: vec![],
            system_prompt: "You are a secretary.".into(),
            tools: vec![],
            meta: RoleMeta::default(),
            memory_dir: String::new(),
        };
        let mut files = HashMap::new();
        files.insert(MemoryFileKind::Bootstrap, None);
        let snapshot = MemorySnapshot { files };
        let prompt = SystemPromptBuilder::new()
            .with_role_config(&config, &snapshot)
            .build();
        assert_eq!(prompt, "You are a secretary.");
    }

    #[test]
    fn with_memory_snapshot_injects_sections() {
        let mut files = HashMap::new();
        files.insert(MemoryFileKind::Soul, Some("I am friendly.".to_string()));
        files.insert(
            MemoryFileKind::Agents,
            Some("Follow coding standards.".to_string()),
        );
        files.insert(MemoryFileKind::Tools, None);
        files.insert(MemoryFileKind::LongTerm, Some("".to_string())); // empty → skip
        let snapshot = MemorySnapshot { files };

        let prompt = SystemPromptBuilder::new()
            .with_memory_snapshot(&snapshot)
            .build();
        assert!(prompt.contains("## Persona"));
        assert!(prompt.contains("I am friendly."));
        assert!(prompt.contains("## Guidelines"));
        assert!(prompt.contains("Follow coding standards."));
        assert!(!prompt.contains("Tool Guidance")); // None → skip
        assert!(!prompt.contains("Long-term Memory")); // empty → skip
    }

    #[test]
    fn with_daily_memory_injects_section() {
        let prompt = SystemPromptBuilder::new()
            .with_daily_memory(Some("Had a meeting at 10am."))
            .build();
        assert!(prompt.contains("## Today's Notes"));
        assert!(prompt.contains("Had a meeting at 10am."));
    }

    #[test]
    fn with_daily_memory_none_skips() {
        let prompt = SystemPromptBuilder::new().with_daily_memory(None).build();
        assert!(!prompt.contains("Today's Notes"));
    }

    #[test]
    fn with_daily_memory_empty_skips() {
        let prompt = SystemPromptBuilder::new()
            .with_daily_memory(Some("  "))
            .build();
        assert!(!prompt.contains("Today's Notes"));
    }

    #[test]
    fn full_system_prompt_ordering() {
        let config = RoleConfig {
            name: "coder".into(),
            description: vec![],
            system_prompt: "You are a coder.".into(),
            tools: vec![],
            meta: RoleMeta::default(),
            memory_dir: String::new(),
        };
        let mut files = HashMap::new();
        files.insert(MemoryFileKind::Soul, Some("creative".to_string()));
        files.insert(MemoryFileKind::LongTerm, Some("remember X".to_string()));
        let snapshot = MemorySnapshot { files };

        let prompt = SystemPromptBuilder::new()
            .with_role_config(&config, &snapshot)
            .with_memory_snapshot(&snapshot)
            .with_daily_memory(Some("daily note"))
            .build();

        // Verify ordering: base → persona → long-term → daily
        let base_pos = prompt.find("You are a coder.").unwrap();
        let persona_pos = prompt.find("creative").unwrap();
        let lt_pos = prompt.find("remember X").unwrap();
        let daily_pos = prompt.find("daily note").unwrap();
        assert!(base_pos < persona_pos);
        assert!(persona_pos < lt_pos);
        assert!(lt_pos < daily_pos);
    }

    // ── ChatRequestBuilder ──────────────────────────────────────────────

    #[test]
    fn minimal_request_has_model() {
        let req = ChatRequestBuilder::new("gpt-4o").build();
        assert_eq!(req.model, "gpt-4o");
        assert!(req.messages.is_empty());
        assert!(req.tools.is_empty());
        assert!(!req.stream);
    }

    #[test]
    fn with_system_prompt_adds_system_message() {
        let req = ChatRequestBuilder::new("gpt-4o")
            .with_system_prompt("You are helpful.")
            .build();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, Role::System);
        assert_eq!(req.messages[0].content.as_deref(), Some("You are helpful."));
    }

    #[test]
    fn with_user_message_adds_user_message() {
        let req = ChatRequestBuilder::new("gpt-4o")
            .with_user_message("hello")
            .build();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, Role::User);
    }

    #[test]
    fn message_ordering_system_history_user() {
        use crate::session::{assistant_output_to_transcript, user_input_to_transcript};
        let history = vec![
            user_input_to_transcript("q1"),
            assistant_output_to_transcript("a1"),
        ];
        let req = ChatRequestBuilder::new("gpt-4o")
            .with_system_prompt("sys")
            .with_history(&history)
            .with_user_message("q2")
            .build();
        assert_eq!(req.messages.len(), 4);
        assert_eq!(req.messages[0].role, Role::System);
        assert_eq!(req.messages[1].role, Role::User); // history q1
        assert_eq!(req.messages[2].role, Role::Assistant); // history a1
        assert_eq!(req.messages[3].role, Role::User); // current q2
    }

    #[test]
    fn with_tool_schemas_converts_to_definitions() {
        let schemas = vec![
            ToolSchema {
                name: "file_read".into(),
                description: "Read a file".into(),
                parameters: serde_json::json!({"type": "object"}),
            },
            ToolSchema {
                name: "file_write".into(),
                description: "Write a file".into(),
                parameters: serde_json::json!({"type": "object"}),
            },
        ];
        let req = ChatRequestBuilder::new("gpt-4o")
            .with_tool_schemas(&schemas)
            .build();
        assert_eq!(req.tools.len(), 2);
        assert_eq!(req.tools[0].name, "file_read");
        assert_eq!(req.tools[1].name, "file_write");
    }

    #[test]
    fn with_temperature_and_max_tokens() {
        let req = ChatRequestBuilder::new("gpt-4o")
            .with_temperature(Some(0.7))
            .with_max_tokens(Some(4096))
            .build();
        assert_eq!(req.temperature, Some(0.7));
        assert_eq!(req.max_tokens, Some(4096));
    }

    #[test]
    fn with_stream_flag() {
        let req = ChatRequestBuilder::new("gpt-4o").with_stream(true).build();
        assert!(req.stream);
    }

    // ── Legacy build_chat_request ───────────────────────────────────────

    #[test]
    fn legacy_request_has_correct_model() {
        let req = build_chat_request("gpt-4o", "hello");
        assert_eq!(req.model, "gpt-4o");
    }

    #[test]
    fn legacy_request_contains_system_and_user_messages() {
        let req = build_chat_request("gpt-4o", "hello");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, Role::System);
        assert_eq!(req.messages[1].role, Role::User);
    }

    #[test]
    fn legacy_system_message_is_xclaw_prompt() {
        let req = build_chat_request("gpt-4o", "hello");
        let content = req.messages[0].content.as_deref().unwrap();
        assert!(content.contains("xClaw"));
    }

    #[test]
    fn legacy_user_message_has_provided_content() {
        let req = build_chat_request("gpt-4o", "what is 2+2?");
        assert_eq!(req.messages[1].content, Some("what is 2+2?".to_string()));
    }

    #[test]
    fn legacy_request_has_no_tools() {
        let req = build_chat_request("gpt-4o", "hello");
        assert!(req.tools.is_empty());
    }

    #[test]
    fn legacy_request_is_not_streaming() {
        let req = build_chat_request("gpt-4o", "hello");
        assert!(!req.stream);
    }

    #[test]
    fn legacy_handles_unicode_content() {
        let req = build_chat_request("claude-sonnet-4-5-20250929", "你好世界 🌍");
        assert_eq!(req.messages[1].content, Some("你好世界 🌍".to_string()));
    }

    #[test]
    fn legacy_handles_empty_content() {
        let req = build_chat_request("gpt-4o", "");
        assert_eq!(req.messages[1].content, Some(String::new()));
    }
}
