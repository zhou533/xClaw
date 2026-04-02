//! Tool and skill dispatch coordination.
//!
//! When the LLM requests a tool or skill invocation, this module
//! routes the call to the appropriate handler and feeds the result
//! back into the agent loop.

use serde::{Deserialize, Serialize};

use xclaw_provider::types::ToolCall;
use xclaw_tools::registry::ToolRegistry;
use xclaw_tools::traits::ToolContext;

/// Result of executing a single tool call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub tool_call_id: String,
    pub tool_name: String,
    /// Success content (Ok) or error message (Err).
    pub output: Result<String, String>,
}

impl ToolCallResult {
    pub fn is_error(&self) -> bool {
        self.output.is_err()
    }

    /// Get the content string regardless of success/error.
    pub fn content(&self) -> &str {
        match &self.output {
            Ok(s) => s,
            Err(s) => s,
        }
    }
}

/// Dispatches tool calls from LLM responses to the tool registry.
pub struct ToolDispatcher<'a> {
    registry: &'a ToolRegistry,
    debug: bool,
}

impl<'a> ToolDispatcher<'a> {
    pub fn new(registry: &'a ToolRegistry, debug: bool) -> Self {
        Self { registry, debug }
    }

    /// Execute a batch of tool calls and return results.
    ///
    /// Each tool call is executed sequentially. Unknown tools return
    /// an error result (no panic). Tool execution errors are captured
    /// as error results, not propagated.
    pub async fn execute_tool_calls(
        &self,
        tool_calls: &[ToolCall],
        context: &ToolContext,
    ) -> Vec<ToolCallResult> {
        let mut results = Vec::with_capacity(tool_calls.len());

        for tc in tool_calls {
            let tool_name = &tc.function.name;
            tracing::info!(tool = %tool_name, call_id = %tc.id, "dispatching tool call");

            if self.debug {
                eprint!(
                    "{}",
                    crate::debug_fmt::format_tool_call_detail(
                        tool_name,
                        &tc.id,
                        &tc.function.arguments
                    )
                );
            }

            let result = match self.registry.get(tool_name) {
                None => {
                    tracing::warn!(tool = %tool_name, "tool not found in registry");
                    ToolCallResult {
                        tool_call_id: tc.id.clone(),
                        tool_name: tool_name.clone(),
                        output: Err(format!("tool not found: {tool_name}")),
                    }
                }
                Some(tool) => {
                    let params: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                tool = %tool_name,
                                error = %e,
                                "failed to parse tool arguments, using null"
                            );
                            serde_json::Value::Null
                        });

                    match tool.execute(context, params).await {
                        Ok(tool_output) => {
                            tracing::info!(
                                tool = %tool_name,
                                is_error = tool_output.is_error,
                                "tool execution completed"
                            );
                            if tool_output.is_error {
                                ToolCallResult {
                                    tool_call_id: tc.id.clone(),
                                    tool_name: tool_name.clone(),
                                    output: Err(tool_output.content),
                                }
                            } else {
                                ToolCallResult {
                                    tool_call_id: tc.id.clone(),
                                    tool_name: tool_name.clone(),
                                    output: Ok(tool_output.content),
                                }
                            }
                        }
                        Err(e) => {
                            tracing::error!(
                                tool = %tool_name,
                                error = %e,
                                "tool execution failed"
                            );
                            ToolCallResult {
                                tool_call_id: tc.id.clone(),
                                tool_name: tool_name.clone(),
                                output: Err(format!("tool error: {e}")),
                            }
                        }
                    }
                }
            };

            if self.debug {
                eprint!(
                    "{}",
                    crate::debug_fmt::format_tool_result_detail(
                        &result.tool_name,
                        &result.tool_call_id,
                        result.is_error(),
                        result.content(),
                    )
                );
            }

            results.push(result);
        }

        results
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    use async_trait::async_trait;
    use xclaw_provider::types::FunctionCall;
    use xclaw_tools::error::ToolError;
    use xclaw_tools::traits::{Tool, ToolOutput, WorkspaceScope};

    // ── Stub tools ──────────────────────────────────────────────────────

    struct EchoTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "Echoes the input"
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

    struct FailTool;

    #[async_trait]
    impl Tool for FailTool {
        fn name(&self) -> &str {
            "fail"
        }
        fn description(&self) -> &str {
            "Always fails"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(
            &self,
            _ctx: &ToolContext,
            _params: serde_json::Value,
        ) -> Result<ToolOutput, ToolError> {
            Err(ToolError::Internal("something broke".into()))
        }
    }

    struct ErrorOutputTool;

    #[async_trait]
    impl Tool for ErrorOutputTool {
        fn name(&self) -> &str {
            "error_output"
        }
        fn description(&self) -> &str {
            "Returns error output (not a thrown error)"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(
            &self,
            _ctx: &ToolContext,
            _params: serde_json::Value,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::error("file not found"))
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    fn make_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool);
        reg.register(FailTool);
        reg.register(ErrorOutputTool);
        reg
    }

    fn make_context() -> ToolContext {
        ToolContext::new(
            WorkspaceScope {
                workspace_root: PathBuf::from("/tmp"),
            },
            Duration::from_secs(30),
        )
    }

    fn make_tool_call(id: &str, name: &str, args: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            function: FunctionCall {
                name: name.to_string(),
                arguments: args.to_string(),
            },
        }
    }

    // ── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn dispatches_known_tool_successfully() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, false);
        let ctx = make_context();

        let calls = vec![make_tool_call("c1", "echo", r#"{"text":"hello"}"#)];
        let results = dispatcher.execute_tool_calls(&calls, &ctx).await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool_call_id, "c1");
        assert_eq!(results[0].tool_name, "echo");
        assert_eq!(results[0].output, Ok("hello".to_string()));
        assert!(!results[0].is_error());
    }

    #[tokio::test]
    async fn unknown_tool_returns_error_result() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, false);
        let ctx = make_context();

        let calls = vec![make_tool_call("c2", "nonexistent", "{}")];
        let results = dispatcher.execute_tool_calls(&calls, &ctx).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_error());
        assert!(results[0].content().contains("tool not found"));
    }

    #[tokio::test]
    async fn tool_execution_error_captured_as_error_result() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, false);
        let ctx = make_context();

        let calls = vec![make_tool_call("c3", "fail", "{}")];
        let results = dispatcher.execute_tool_calls(&calls, &ctx).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_error());
        assert!(results[0].content().contains("something broke"));
    }

    #[tokio::test]
    async fn tool_error_output_mapped_correctly() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, false);
        let ctx = make_context();

        let calls = vec![make_tool_call("c4", "error_output", "{}")];
        let results = dispatcher.execute_tool_calls(&calls, &ctx).await;

        assert_eq!(results.len(), 1);
        assert!(results[0].is_error());
        assert_eq!(results[0].content(), "file not found");
    }

    #[tokio::test]
    async fn multiple_tool_calls_executed_in_order() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, false);
        let ctx = make_context();

        let calls = vec![
            make_tool_call("c1", "echo", r#"{"text":"first"}"#),
            make_tool_call("c2", "echo", r#"{"text":"second"}"#),
        ];
        let results = dispatcher.execute_tool_calls(&calls, &ctx).await;

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].output, Ok("first".to_string()));
        assert_eq!(results[1].output, Ok("second".to_string()));
    }

    #[tokio::test]
    async fn empty_tool_calls_returns_empty() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, false);
        let ctx = make_context();

        let results = dispatcher.execute_tool_calls(&[], &ctx).await;
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn invalid_json_arguments_uses_null() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, false);
        let ctx = make_context();

        let calls = vec![make_tool_call("c5", "echo", "not json")];
        let results = dispatcher.execute_tool_calls(&calls, &ctx).await;

        assert_eq!(results.len(), 1);
        // EchoTool defaults to "(no text)" when params don't have "text"
        assert_eq!(results[0].output, Ok("(no text)".to_string()));
    }

    #[tokio::test]
    async fn debug_mode_does_not_affect_results() {
        let reg = make_registry();
        let dispatcher = ToolDispatcher::new(&reg, true);
        let ctx = make_context();

        let calls = vec![
            make_tool_call("c1", "echo", r#"{"text":"hello"}"#),
            make_tool_call("c2", "nonexistent", "{}"),
            make_tool_call("c3", "fail", "{}"),
            make_tool_call("c4", "error_output", "{}"),
        ];
        let results = dispatcher.execute_tool_calls(&calls, &ctx).await;

        assert_eq!(results.len(), 4);
        // Success
        assert_eq!(results[0].output, Ok("hello".to_string()));
        assert!(!results[0].is_error());
        // Tool not found
        assert!(results[1].is_error());
        assert!(results[1].content().contains("tool not found"));
        // Execution error
        assert!(results[2].is_error());
        assert!(results[2].content().contains("something broke"));
        // Error output
        assert!(results[3].is_error());
        assert_eq!(results[3].content(), "file not found");
    }

    #[test]
    fn tool_call_result_content_method() {
        let ok = ToolCallResult {
            tool_call_id: "c1".into(),
            tool_name: "echo".into(),
            output: Ok("success".into()),
        };
        assert_eq!(ok.content(), "success");
        assert!(!ok.is_error());

        let err = ToolCallResult {
            tool_call_id: "c2".into(),
            tool_name: "fail".into(),
            output: Err("failed".into()),
        };
        assert_eq!(err.content(), "failed");
        assert!(err.is_error());
    }
}
