//! Core agent loop engine.
//!
//! `LoopAgent` implements the full agent loop: session loading, memory
//! injection, prompt building, LLM calling, tool dispatch, and memory
//! persistence.

use std::path::PathBuf;
use std::time::Duration;

use xclaw_core::error::XClawError;
use xclaw_core::types::{RoleId, SessionId};
use xclaw_memory::role::daily::DailyMemory;
use xclaw_memory::role::manager::RoleManager;
use xclaw_memory::session::record_id::RecordId;
use xclaw_memory::session::store::SessionStore;
use xclaw_memory::session::types::SessionEntry;
use xclaw_memory::workspace::loader::MemoryFileLoader;
use xclaw_provider::traits::LlmProvider;
use xclaw_provider::types::{ChatRequest, FinishReason, Message, Role};
use xclaw_tools::registry::ToolRegistry;
use xclaw_tools::traits::{ToolContext, WorkspaceScope};

use crate::config::AgentConfig;
use crate::dispatch::{ToolCallResult, ToolDispatcher};
use crate::prompt::{ChatRequestBuilder, SystemPromptBuilder};
use crate::session::{
    resolve_session_key, response_to_transcript, tool_result_to_transcript,
    user_input_to_transcript,
};
use crate::traits::{AgentLoop, AgentResponse, UserInput};

/// Full agent loop engine with tool dispatch and memory integration.
///
/// Generic over `LlmProvider` and the four memory subsystem traits.
/// Use with `FsMemorySystem` for filesystem-backed memory.
pub struct LoopAgent<'a, P, S, R, F, D>
where
    P: LlmProvider,
    S: SessionStore,
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
{
    provider: P,
    config: AgentConfig,
    sessions: &'a S,
    roles: &'a R,
    files: &'a F,
    daily: &'a D,
    tool_registry: &'a ToolRegistry,
    /// Workspace root for tool execution context.
    workspace_root: PathBuf,
}

impl<'a, P, S, R, F, D> LoopAgent<'a, P, S, R, F, D>
where
    P: LlmProvider,
    S: SessionStore,
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider: P,
        config: AgentConfig,
        sessions: &'a S,
        roles: &'a R,
        files: &'a F,
        daily: &'a D,
        tool_registry: &'a ToolRegistry,
        workspace_root: impl Into<PathBuf>,
    ) -> Self {
        Self {
            provider,
            config,
            sessions,
            roles,
            files,
            daily,
            tool_registry,
            workspace_root: workspace_root.into(),
        }
    }
}

impl<P, S, R, F, D> AgentLoop for LoopAgent<'_, P, S, R, F, D>
where
    P: LlmProvider,
    S: SessionStore,
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
{
    async fn process(&self, input: UserInput) -> Result<AgentResponse, XClawError> {
        let (session, role_id, request) = self.load_context_and_build_request(&input).await?;

        // Debug: print assembled prompt to stderr
        if self.config.debug {
            eprint!("{}", crate::debug_fmt::format_request_debug(&request));
        }

        // Persist user message before LLM call (crash-safe ordering)
        let user_rec = user_input_to_transcript(&input.content);
        if let Err(e) = self
            .sessions
            .append_transcript(&role_id, &session.session_id, &user_rec)
            .await
        {
            tracing::warn!(error = %e, "failed to persist user transcript");
        }

        // Run the tool loop
        let (final_content, total_tool_calls) = self
            .run_tool_loop(request, &role_id, &session.session_id)
            .await?;

        // Persist final assistant response
        self.persist_assistant_response(&role_id, &session.session_id, &final_content)
            .await;

        Ok(AgentResponse {
            content: final_content,
            tool_calls_count: total_tool_calls,
        })
    }
}

// ─── Private methods ────────────────────────────────────────────────────────

impl<P, S, R, F, D> LoopAgent<'_, P, S, R, F, D>
where
    P: LlmProvider,
    S: SessionStore,
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
{
    /// Steps 1–7: load session, role, memory, history; build the initial request.
    async fn load_context_and_build_request(
        &self,
        input: &UserInput,
    ) -> Result<(SessionEntry, RoleId, ChatRequest), XClawError> {
        let session_key = resolve_session_key(input, Some(input.session_id.as_str()))?;
        // TODO(role-routing): derive role_id from input once RoleRouter exists
        let role_id = RoleId::default();

        let session = self
            .sessions
            .get_or_create(&session_key)
            .await
            .map_err(|e| XClawError::Session(e.to_string()))?;

        let role_config = self
            .roles
            .get_role(&role_id)
            .await
            .map_err(|e| XClawError::Memory(e.to_string()))?;

        let snapshot = self
            .files
            .load_snapshot(&role_id)
            .await
            .map_err(|e| XClawError::Memory(e.to_string()))?;

        let today = today_date_string();
        let daily_content = self.daily.load_day(&role_id, &today).await.ok();

        let history = match self
            .sessions
            .load_transcript_tail(
                &role_id,
                &session.session_id,
                self.config.transcript_tail_size,
            )
            .await
        {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(error = %e, "failed to load transcript history, proceeding without");
                vec![]
            }
        };

        let system_prompt = SystemPromptBuilder::new()
            .with_role_config(&role_config, &snapshot)
            .with_memory_snapshot(&snapshot)
            .with_daily_memory(daily_content.as_deref())
            .build();

        let tool_schemas = self.tool_registry.list_schemas();
        let request = ChatRequestBuilder::new(&self.config.model)
            .with_system_prompt(&system_prompt)
            .with_history_filtered(&history, &self.config.history_content_kinds)
            .with_user_message(&input.content)
            .with_tool_schemas(&tool_schemas)
            .with_temperature(self.config.temperature)
            .with_max_tokens(self.config.max_tokens)
            .build();

        Ok((session, role_id, request))
    }

    /// Step 8: the core tool-call loop. Returns `(final_content, total_tool_calls)`.
    async fn run_tool_loop(
        &self,
        mut request: ChatRequest,
        role_id: &RoleId,
        session_id: &SessionId,
    ) -> Result<(String, u32), XClawError> {
        let dispatcher = ToolDispatcher::new(self.tool_registry, self.config.debug);
        let tool_ctx = ToolContext::new(
            WorkspaceScope {
                workspace_root: self.workspace_root.clone(),
            },
            Duration::from_secs(30),
        );

        let mut total_tool_calls: u32 = 0;

        for round in 0..self.config.max_tool_rounds {
            let response = self
                .provider
                .chat(&request)
                .await
                .map_err(|e| XClawError::Agent(e.to_string()))?;

            let choice = response
                .choices
                .first()
                .ok_or_else(|| XClawError::Agent("empty response from provider".to_string()))?;

            let has_tool_calls = matches!(choice.finish_reason, Some(FinishReason::ToolCalls))
                || !choice.message.tool_calls.is_empty();

            if !has_tool_calls {
                return Ok((
                    choice.message.content.clone().unwrap_or_default(),
                    total_tool_calls,
                ));
            }

            let tool_calls = &choice.message.tool_calls;
            total_tool_calls += tool_calls.len() as u32;

            tracing::info!(round, tool_count = tool_calls.len(), "executing tool calls");

            // Debug: print tool-loop round summary
            if self.config.debug {
                eprint!(
                    "{}",
                    crate::debug_fmt::format_tool_round_summary(round + 1, tool_calls.len())
                );
            }

            let results = dispatcher.execute_tool_calls(tool_calls, &tool_ctx).await;

            // Inject assistant + tool results into request for next round
            request.messages.push(choice.message.clone());
            for result in &results {
                request.messages.push(tool_result_to_message(result));
            }

            // Persist intermediate records (warn on failure, don't block)
            let assistant_records = response_to_transcript(&response);
            let last_assistant_id: Option<RecordId> =
                assistant_records.last().map(|r| r.id.clone());
            for rec in &assistant_records {
                if let Err(e) = self
                    .sessions
                    .append_transcript(role_id, session_id, rec)
                    .await
                {
                    tracing::warn!(error = %e, "failed to persist tool-loop assistant transcript");
                }
            }
            for result in &results {
                let rec = tool_result_to_transcript(
                    &result.tool_call_id,
                    &result.tool_name,
                    result.content(),
                    last_assistant_id.as_ref(),
                );
                if let Err(e) = self
                    .sessions
                    .append_transcript(role_id, session_id, &rec)
                    .await
                {
                    tracing::warn!(error = %e, "failed to persist tool result transcript");
                }
            }
        }

        tracing::warn!(
            max_rounds = self.config.max_tool_rounds,
            total_tool_calls,
            "tool call loop reached maximum rounds without final text response"
        );
        Ok((String::new(), total_tool_calls))
    }

    /// Step 9b: persist the final assistant response to transcript.
    async fn persist_assistant_response(
        &self,
        role_id: &RoleId,
        session_id: &SessionId,
        content: &str,
    ) {
        if content.is_empty() {
            return;
        }
        let rec = crate::session::assistant_output_to_transcript(content);
        if let Err(e) = self
            .sessions
            .append_transcript(role_id, session_id, &rec)
            .await
        {
            tracing::warn!(error = %e, "failed to persist assistant transcript");
        }
    }
}

/// Convert a `ToolCallResult` into a provider `Message` for injection
/// back into the conversation.
fn tool_result_to_message(result: &ToolCallResult) -> Message {
    Message {
        role: Role::Tool,
        content: Some(result.content().to_string()),
        tool_calls: vec![],
        tool_call_id: Some(result.tool_call_id.clone()),
    }
}

/// Get today's date as `YYYY-MM-DD` string.
fn today_date_string() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Simple epoch → date calculation (UTC)
    let days = secs / 86400;
    epoch_days_to_ymd(days)
}

/// Convert epoch days to `YYYY-MM-DD` (Howard Hinnant's algorithm).
fn epoch_days_to_ymd(days: u64) -> String {
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use xclaw_provider::error::ProviderError;
    use xclaw_provider::traits::ChatStream;
    use xclaw_provider::types::{
        ChatResponse, Choice, FinishReason, FunctionCall, Message, ModelInfo, Role, ToolCall,
    };
    use xclaw_tools::registry::ToolRegistry;

    // ── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn simple_text_response() {
        let sessions = StubSessionStore::new();
        let roles = StubRoleManager;
        let files = StubMemoryFileLoader;
        let daily = StubDailyMemory;
        let registry = ToolRegistry::new();

        let agent = LoopAgent::new(
            TextProvider::new("Hello!"),
            make_config(),
            &sessions,
            &roles,
            &files,
            &daily,
            &registry,
            "/tmp",
        );

        let resp = agent.process(make_input("hi")).await.unwrap();
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.tool_calls_count, 0);
    }

    #[tokio::test]
    async fn persists_transcript_after_response() {
        let sessions = StubSessionStore::new();
        let roles = StubRoleManager;
        let files = StubMemoryFileLoader;
        let daily = StubDailyMemory;
        let registry = ToolRegistry::new();

        let agent = LoopAgent::new(
            TextProvider::new("reply"),
            make_config(),
            &sessions,
            &roles,
            &files,
            &daily,
            &registry,
            "/tmp",
        );

        agent.process(make_input("question")).await.unwrap();

        // Should have persisted: user message + assistant message
        assert_eq!(sessions.transcript_count(), 2);
    }

    #[tokio::test]
    async fn tool_call_loop_executes_and_returns() {
        let sessions = StubSessionStore::new();
        let roles = StubRoleManager;
        let files = StubMemoryFileLoader;
        let daily = StubDailyMemory;
        let registry = make_registry_with_echo();

        let agent = LoopAgent::new(
            ToolThenTextProvider::new(),
            make_config(),
            &sessions,
            &roles,
            &files,
            &daily,
            &registry,
            "/tmp",
        );

        let resp = agent.process(make_input("use echo")).await.unwrap();
        assert_eq!(resp.content, "final answer");
        assert_eq!(resp.tool_calls_count, 1);
    }

    #[tokio::test]
    async fn tool_call_persists_all_records() {
        let sessions = StubSessionStore::new();
        let roles = StubRoleManager;
        let files = StubMemoryFileLoader;
        let daily = StubDailyMemory;
        let registry = make_registry_with_echo();

        let agent = LoopAgent::new(
            ToolThenTextProvider::new(),
            make_config(),
            &sessions,
            &roles,
            &files,
            &daily,
            &registry,
            "/tmp",
        );

        agent.process(make_input("use echo")).await.unwrap();

        // Records: tool-loop assistant + tool result + final user + final assistant
        let count = sessions.transcript_count();
        assert!(
            count >= 4,
            "expected at least 4 transcript records, got {count}"
        );
    }

    #[tokio::test]
    async fn provider_error_propagates() {
        let sessions = StubSessionStore::new();
        let roles = StubRoleManager;
        let files = StubMemoryFileLoader;
        let daily = StubDailyMemory;
        let registry = ToolRegistry::new();

        let agent = LoopAgent::new(
            ErrorProvider,
            make_config(),
            &sessions,
            &roles,
            &files,
            &daily,
            &registry,
            "/tmp",
        );

        let result = agent.process(make_input("hi")).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bad key"), "error: {err}");
    }

    #[tokio::test]
    async fn max_tool_rounds_protection() {
        // Provider that always returns tool calls (infinite loop scenario)
        struct AlwaysToolCallProvider;

        impl LlmProvider for AlwaysToolCallProvider {
            fn name(&self) -> &str {
                "always-tool"
            }

            async fn chat(&self, _req: &ChatRequest) -> Result<ChatResponse, ProviderError> {
                Ok(ChatResponse {
                    id: "resp".into(),
                    model: "stub".into(),
                    choices: vec![Choice {
                        index: 0,
                        message: Message {
                            role: Role::Assistant,
                            content: None,
                            tool_calls: vec![ToolCall {
                                id: "call".into(),
                                function: FunctionCall {
                                    name: "echo".into(),
                                    arguments: r#"{"text":"loop"}"#.into(),
                                },
                            }],
                            tool_call_id: None,
                        },
                        finish_reason: Some(FinishReason::ToolCalls),
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

        let sessions = StubSessionStore::new();
        let roles = StubRoleManager;
        let files = StubMemoryFileLoader;
        let daily = StubDailyMemory;
        let registry = make_registry_with_echo();

        let agent = LoopAgent::new(
            AlwaysToolCallProvider,
            AgentConfig::new("test").with_max_tool_rounds(3),
            &sessions,
            &roles,
            &files,
            &daily,
            &registry,
            "/tmp",
        );

        let resp = agent.process(make_input("loop")).await.unwrap();
        // Should exit after max_tool_rounds with empty content
        assert_eq!(resp.tool_calls_count, 3);
    }

    // ── epoch_days_to_ymd ───────────────────────────────────────────────

    #[test]
    fn epoch_days_to_ymd_known_dates() {
        assert_eq!(epoch_days_to_ymd(0), "1970-01-01");
        assert_eq!(epoch_days_to_ymd(365), "1971-01-01");
        assert_eq!(epoch_days_to_ymd(18_993), "2022-01-01");
        assert_eq!(epoch_days_to_ymd(20_541), "2026-03-29");
    }

    #[test]
    fn tool_result_to_message_formats_correctly() {
        let result = ToolCallResult {
            tool_call_id: "c1".into(),
            tool_name: "echo".into(),
            output: Ok("hello".into()),
        };
        let msg = tool_result_to_message(&result);
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.content.as_deref(), Some("hello"));
        assert_eq!(msg.tool_call_id.as_deref(), Some("c1"));
    }

    #[tokio::test]
    async fn history_uses_configured_content_kinds() {
        use std::collections::BTreeSet;
        use xclaw_memory::session::types::{
            ContentBlock, ContentBlockKind, TranscriptRecord, TranscriptRole,
        };

        // Seed history with a user text + assistant record containing Text + ToolCall
        let history = vec![
            TranscriptRecord {
                id: "rec1".into(),
                parent_id: None,
                role: TranscriptRole::User,
                content: vec![ContentBlock::Text {
                    text: "use a tool".into(),
                }],
                timestamp: "2026-04-07T00:00:00Z".into(),
                model: None,
                stop_reason: None,
                usage: None,
                provider: None,
                metadata: std::collections::HashMap::new(),
            },
            TranscriptRecord {
                id: "rec2".into(),
                parent_id: Some("rec1".into()),
                role: TranscriptRole::Assistant,
                content: vec![
                    ContentBlock::Text {
                        text: "I will call a tool".into(),
                    },
                    ContentBlock::ToolCall {
                        call_id: "call_old".into(),
                        name: "echo".into(),
                        arguments: r#"{"text":"hi"}"#.into(),
                    },
                ],
                timestamp: "2026-04-07T00:00:01Z".into(),
                model: Some("stub".into()),
                stop_reason: None,
                usage: None,
                provider: None,
                metadata: std::collections::HashMap::new(),
            },
        ];

        let sessions = StubSessionStore::new().with_history(history);
        let roles = StubRoleManager;
        let files = StubMemoryFileLoader;
        let daily = StubDailyMemory;
        let registry = ToolRegistry::new();

        // Config: only Text in history (default)
        let config = AgentConfig::new("test-model")
            .with_max_tool_rounds(5)
            .with_transcript_tail(10);
        assert_eq!(
            config.history_content_kinds,
            BTreeSet::from([ContentBlockKind::Text])
        );

        let provider = CapturingProvider::new("ok");
        let captured = provider.captured.clone();

        let agent = LoopAgent::new(
            provider, config, &sessions, &roles, &files, &daily, &registry, "/tmp",
        );

        agent.process(make_input("hello")).await.unwrap();

        // The first captured request should have history messages with no tool calls
        let reqs = captured.lock().unwrap();
        assert!(!reqs.is_empty(), "provider should have been called");
        let req = &reqs[0];

        // Find the assistant history message (not the current user message)
        let assistant_msgs: Vec<_> = req
            .messages
            .iter()
            .filter(|m| m.role == Role::Assistant)
            .collect();
        // History should contain the assistant message with text only (no tool calls)
        for msg in &assistant_msgs {
            assert!(
                msg.tool_calls.is_empty(),
                "history assistant messages should have no tool_calls when filter is Text-only, \
                 but got: {:?}",
                msg.tool_calls
            );
        }
    }

    #[test]
    fn tool_result_error_to_message() {
        let result = ToolCallResult {
            tool_call_id: "c2".into(),
            tool_name: "fail".into(),
            output: Err("not found".into()),
        };
        let msg = tool_result_to_message(&result);
        assert_eq!(msg.role, Role::Tool);
        assert_eq!(msg.content.as_deref(), Some("not found"));
    }
}
