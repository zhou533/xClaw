//! Shared test stubs for the agent crate.
//!
//! Provides stub implementations of memory subsystems, providers,
//! and tools for use in unit and integration tests.

#![cfg(test)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use xclaw_core::types::{RoleId, SessionId};
use xclaw_memory::error::MemoryError;
use xclaw_memory::role::config::{RoleConfig, RoleMeta};
use xclaw_memory::role::daily::DailyMemory;
use xclaw_memory::role::manager::RoleManager;
use xclaw_memory::session::store::SessionStore;
use xclaw_memory::session::types::{SessionEntry, SessionSummary, TranscriptRecord};
use xclaw_memory::workspace::loader::MemoryFileLoader;
use xclaw_memory::workspace::types::{MemoryFileKind, MemorySnapshot};
use xclaw_provider::error::ProviderError;
use xclaw_provider::traits::{ChatStream, LlmProvider};
use xclaw_provider::types::{
    ChatRequest, ChatResponse, Choice, FinishReason, FunctionCall, Message, ModelInfo, Role,
    ToolCall,
};
use xclaw_tools::error::ToolError;
use xclaw_tools::registry::ToolRegistry;
use xclaw_tools::traits::{Tool, ToolContext, ToolOutput};

use crate::config::AgentConfig;
use crate::traits::UserInput;

// ─── Stub Memory Implementations ───────────────────────────────────────────

pub struct StubSessionStore {
    pub transcripts: Arc<Mutex<Vec<TranscriptRecord>>>,
}

impl StubSessionStore {
    pub fn new() -> Self {
        Self {
            transcripts: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn transcript_count(&self) -> usize {
        self.transcripts.lock().unwrap().len()
    }
}

impl SessionStore for StubSessionStore {
    async fn get_or_create(
        &self,
        key: &xclaw_core::types::SessionKey,
    ) -> Result<SessionEntry, MemoryError> {
        Ok(SessionEntry {
            session_id: SessionId::new("test-session"),
            session_key: key.clone(),
            transcript_path: "test.jsonl".into(),
            created_at: "2026-03-29T00:00:00Z".into(),
            updated_at: "2026-03-29T00:00:00Z".into(),
        })
    }

    async fn get_by_id(
        &self,
        _role_id: &RoleId,
        _session_id: &SessionId,
    ) -> Result<Option<SessionEntry>, MemoryError> {
        Ok(None)
    }

    async fn get_by_key(
        &self,
        _key: &xclaw_core::types::SessionKey,
    ) -> Result<Option<SessionEntry>, MemoryError> {
        Ok(None)
    }

    async fn list_sessions(&self, _role_id: &RoleId) -> Result<Vec<SessionEntry>, MemoryError> {
        Ok(vec![])
    }

    async fn append_transcript(
        &self,
        _role_id: &RoleId,
        _session_id: &SessionId,
        record: &TranscriptRecord,
    ) -> Result<(), MemoryError> {
        self.transcripts.lock().unwrap().push(record.clone());
        Ok(())
    }

    async fn load_transcript(
        &self,
        _role_id: &RoleId,
        _session_id: &SessionId,
    ) -> Result<Vec<TranscriptRecord>, MemoryError> {
        Ok(vec![])
    }

    async fn load_transcript_tail(
        &self,
        _role_id: &RoleId,
        _session_id: &SessionId,
        _n: usize,
    ) -> Result<Vec<TranscriptRecord>, MemoryError> {
        Ok(vec![])
    }

    async fn session_summary(
        &self,
        _role_id: &RoleId,
        _session_id: &SessionId,
    ) -> Result<SessionSummary, MemoryError> {
        Err(MemoryError::SessionNotFound("stub".into()))
    }

    async fn reset_session(
        &self,
        key: &xclaw_core::types::SessionKey,
    ) -> Result<SessionEntry, MemoryError> {
        Ok(SessionEntry {
            session_id: SessionId::new("reset-session"),
            session_key: key.clone(),
            transcript_path: "reset.jsonl".into(),
            created_at: "2026-03-30T00:00:00Z".into(),
            updated_at: "2026-03-30T00:00:00Z".into(),
        })
    }

    async fn delete_session(
        &self,
        _role_id: &RoleId,
        _session_id: &SessionId,
    ) -> Result<(), MemoryError> {
        Ok(())
    }
}

pub struct StubRoleManager;

impl RoleManager for StubRoleManager {
    fn role_dir(&self, role: &RoleId) -> std::path::PathBuf {
        std::path::PathBuf::from(format!("/stub/roles/{}", role.as_str()))
    }

    async fn create_role(&self, _config: RoleConfig) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn get_role(&self, _role: &RoleId) -> Result<RoleConfig, MemoryError> {
        Ok(RoleConfig {
            name: "default".into(),
            description: vec!["Test role".into()],
            system_prompt: "You are xClaw test agent.".into(),
            tools: vec![],
            meta: RoleMeta::default(),
            memory_dir: "roles/default".into(),
        })
    }

    async fn list_roles(&self) -> Result<Vec<RoleConfig>, MemoryError> {
        Ok(vec![])
    }

    async fn delete_role(&self, _role: &RoleId) -> Result<(), MemoryError> {
        Ok(())
    }
}

pub struct StubMemoryFileLoader;

impl MemoryFileLoader for StubMemoryFileLoader {
    async fn load_file(
        &self,
        _role: &RoleId,
        _kind: MemoryFileKind,
    ) -> Result<Option<String>, MemoryError> {
        Ok(None)
    }

    async fn save_file(
        &self,
        _role: &RoleId,
        _kind: MemoryFileKind,
        _content: &str,
    ) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn delete_file(
        &self,
        _role: &RoleId,
        _kind: MemoryFileKind,
    ) -> Result<bool, MemoryError> {
        Ok(false)
    }

    async fn load_snapshot(&self, _role: &RoleId) -> Result<MemorySnapshot, MemoryError> {
        Ok(MemorySnapshot {
            files: HashMap::new(),
        })
    }
}

pub struct StubDailyMemory;

impl DailyMemory for StubDailyMemory {
    async fn append(&self, _role: &RoleId, _entry: &str) -> Result<(), MemoryError> {
        Ok(())
    }

    async fn load_day(&self, _role: &RoleId, _date: &str) -> Result<String, MemoryError> {
        Err(MemoryError::InvalidDate("stub".into()))
    }

    async fn list_days(&self, _role: &RoleId) -> Result<Vec<String>, MemoryError> {
        Ok(vec![])
    }
}

// ─── Stub Providers ────────────────────────────────────────────────────────

/// Provider that always returns a fixed text reply.
pub struct TextProvider {
    reply: String,
}

impl TextProvider {
    pub fn new(reply: &str) -> Self {
        Self {
            reply: reply.to_string(),
        }
    }
}

impl LlmProvider for TextProvider {
    fn name(&self) -> &str {
        "text-stub"
    }

    async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Ok(ChatResponse {
            id: "resp-1".into(),
            model: "stub".into(),
            choices: vec![Choice {
                index: 0,
                message: Message {
                    role: Role::Assistant,
                    content: Some(self.reply.clone()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                finish_reason: Some(FinishReason::Stop),
            }],
            usage: None,
        })
    }

    async fn chat_stream(&self, _req: &ChatRequest) -> Result<ChatStream, ProviderError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![])
    }
}

/// Provider that first returns a tool call, then returns text.
pub struct ToolThenTextProvider {
    call_count: Arc<Mutex<u32>>,
}

impl ToolThenTextProvider {
    pub fn new() -> Self {
        Self {
            call_count: Arc::new(Mutex::new(0)),
        }
    }
}

impl LlmProvider for ToolThenTextProvider {
    fn name(&self) -> &str {
        "tool-then-text-stub"
    }

    async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        let mut count = self.call_count.lock().unwrap();
        *count += 1;
        let current = *count;
        drop(count);

        if current == 1 {
            Ok(ChatResponse {
                id: "resp-tc".into(),
                model: "stub".into(),
                choices: vec![Choice {
                    index: 0,
                    message: Message {
                        role: Role::Assistant,
                        content: None,
                        tool_calls: vec![ToolCall {
                            id: "call_1".into(),
                            function: FunctionCall {
                                name: "echo".into(),
                                arguments: r#"{"text":"tool-output"}"#.into(),
                            },
                        }],
                        tool_call_id: None,
                    },
                    finish_reason: Some(FinishReason::ToolCalls),
                }],
                usage: None,
            })
        } else {
            Ok(ChatResponse {
                id: "resp-text".into(),
                model: "stub".into(),
                choices: vec![Choice {
                    index: 0,
                    message: Message {
                        role: Role::Assistant,
                        content: Some("final answer".into()),
                        tool_calls: vec![],
                        tool_call_id: None,
                    },
                    finish_reason: Some(FinishReason::Stop),
                }],
                usage: None,
            })
        }
    }

    async fn chat_stream(&self, _req: &ChatRequest) -> Result<ChatStream, ProviderError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![])
    }
}

/// Provider that always returns an auth error.
pub struct ErrorProvider;

impl LlmProvider for ErrorProvider {
    fn name(&self) -> &str {
        "error-stub"
    }

    async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
        Err(ProviderError::Auth("bad key".into()))
    }

    async fn chat_stream(&self, _req: &ChatRequest) -> Result<ChatStream, ProviderError> {
        Ok(Box::pin(futures::stream::empty()))
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError> {
        Ok(vec![])
    }
}

// ─── Stub Tool ─────────────────────────────────────────────────────────────

pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "Echoes text"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {"text": {"type": "string"}}})
    }
    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let text = params
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("(no text)");
        Ok(ToolOutput::success(text))
    }
}

// ─── Helpers ───────────────────────────────────────────────────────────────

pub fn make_config() -> AgentConfig {
    AgentConfig::new("test-model").with_max_tool_rounds(5)
}

pub fn make_registry_with_echo() -> ToolRegistry {
    let mut reg = ToolRegistry::new();
    reg.register(EchoTool);
    reg
}

pub fn make_input(content: &str) -> UserInput {
    UserInput {
        session_id: SessionId::new("test-session"),
        content: content.to_string(),
    }
}
