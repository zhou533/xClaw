//! Debug formatting for assembled prompts.
//!
//! Provides colored, human-readable output of `ChatRequest` contents
//! for debugging prompt assembly in CLI debug mode.

use std::io::IsTerminal;

use xclaw_provider::types::{ChatRequest, Role};

// ─── ANSI color constants ──────────────────────────────────────────────────

const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const BLUE: &str = "\x1b[34m";
const MAGENTA: &str = "\x1b[35m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

/// Return the color code if stderr is a terminal, otherwise empty string.
fn color(code: &str) -> &str {
    if std::io::stderr().is_terminal() {
        code
    } else {
        ""
    }
}

// ─── Public API ────────────────────────────────────────────────────────────

/// Format a `ChatRequest` as a colored, human-readable debug string.
///
/// ANSI colors are only emitted when stderr is a terminal.
/// Outputs system prompt, conversation history, current user message,
/// tool definitions, and model metadata — each with distinct colors.
pub fn format_request_debug(request: &ChatRequest) -> String {
    let c = color(CYAN);
    let r = color(RESET);
    let mut out = String::new();

    // Title
    out.push_str(&format!("{c}═══ DEBUG: Assembled Prompt ═══{r}\n"));

    // Messages by role
    for msg in &request.messages {
        let (label, msg_color) = match msg.role {
            Role::System => ("SYSTEM", color(YELLOW)),
            Role::User => ("USER", color(GREEN)),
            Role::Assistant => ("ASSISTANT", color(BLUE)),
            Role::Tool => ("TOOL_RESULT", color(MAGENTA)),
            Role::Developer => ("DEVELOPER", color(YELLOW)),
        };

        out.push_str(&format!("{msg_color}── [{label}] ──{r}\n"));

        if let Some(content) = &msg.content {
            out.push_str(content);
            out.push('\n');
        }

        // Tool calls within assistant messages
        let m = color(MAGENTA);
        for tc in &msg.tool_calls {
            out.push_str(&format!(
                "{m}[TOOL_CALL]{r} {}: {}\n",
                tc.function.name, tc.function.arguments
            ));
        }

        // Tool result ID
        let d = color(DIM);
        if let Some(id) = &msg.tool_call_id {
            out.push_str(&format!("{d}tool_call_id: {id}{r}\n"));
        }
    }

    // Tools
    if !request.tools.is_empty() {
        let m = color(MAGENTA);
        out.push_str(&format!(
            "{c}── Tools ({} definitions) ──{r}\n",
            request.tools.len()
        ));
        for tool in &request.tools {
            out.push_str(&format!("{m}- {}{r}: {}\n", tool.name, tool.description));
        }
    }

    // Metadata
    let d = color(DIM);
    let temp_str = request
        .temperature
        .map(|t| format!("{t}"))
        .unwrap_or_else(|| "default".into());
    let tokens_str = request
        .max_tokens
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "default".into());
    out.push_str(&format!(
        "{d}── Model: {} | Temperature: {} | MaxTokens: {} ──{r}\n",
        request.model, temp_str, tokens_str
    ));

    out.push_str(&format!("{c}═══════════════════════════════{r}\n"));

    out
}

/// Maximum characters for tool arguments before truncation.
const ARG_TRUNCATE_LEN: usize = 200;
/// Maximum characters for tool result content before truncation.
const CONTENT_TRUNCATE_LEN: usize = 300;

/// Truncate a string to `max_len` characters, appending `...` if truncated.
///
/// Uses character-based indexing (not byte-based) so multibyte UTF-8 is safe.
fn truncate(s: &str, max_len: usize) -> String {
    let mut iter = s.char_indices();
    match iter.nth(max_len) {
        None => s.to_string(),
        Some((byte_pos, _)) => format!("{}...", &s[..byte_pos]),
    }
}

/// Format a debug line shown *before* a tool call is executed.
///
/// Includes tool name, call ID, and a truncated argument summary.
/// The returned string always ends with a newline (`\n`).
pub fn format_tool_call_detail(tool_name: &str, call_id: &str, arguments: &str) -> String {
    let m = color(MAGENTA);
    let d = color(DIM);
    let r = color(RESET);
    let args_display = truncate(arguments, ARG_TRUNCATE_LEN);
    format!("{m}[TOOL_EXEC]{r} {tool_name} (id: {call_id})\n{d}  args: {args_display}{r}\n")
}

/// Format a debug line shown *after* a tool call completes.
///
/// Success results get a green `[TOOL_OK]` tag; errors get red `[TOOL_ERR]`.
/// The returned string always ends with a newline (`\n`).
pub fn format_tool_result_detail(
    tool_name: &str,
    call_id: &str,
    is_error: bool,
    content: &str,
) -> String {
    let r = color(RESET);
    let content_display = truncate(content, CONTENT_TRUNCATE_LEN);
    if is_error {
        let red = color(RED);
        format!("{red}[TOOL_ERR]{r} {tool_name} (id: {call_id}): {content_display}\n")
    } else {
        let g = color(GREEN);
        format!("{g}[TOOL_OK]{r} {tool_name} (id: {call_id}): {content_display}\n")
    }
}

/// Format a one-line summary for subsequent tool-loop rounds.
pub fn format_tool_round_summary(round: u32, tool_count: usize) -> String {
    let d = color(DIM);
    let r = color(RESET);
    format!("{d}── debug: round {round}, {tool_count} tool call(s) executed ──{r}\n")
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use xclaw_provider::types::{
        ChatRequest, FunctionCall, Message, Role, ToolCall, ToolDefinition,
    };

    fn make_simple_request() -> ChatRequest {
        ChatRequest {
            model: "gpt-4o".into(),
            messages: vec![
                Message {
                    role: Role::System,
                    content: Some("You are helpful.".into()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                Message {
                    role: Role::User,
                    content: Some("Hello!".into()),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
            ],
            tools: vec![],
            temperature: Some(0.7),
            max_tokens: Some(4096),
            stream: false,
        }
    }

    #[test]
    fn contains_title_and_footer() {
        let out = format_request_debug(&make_simple_request());
        assert!(out.contains("DEBUG: Assembled Prompt"));
        assert!(out.contains("═══════════════════════════════"));
    }

    #[test]
    fn contains_system_label() {
        let out = format_request_debug(&make_simple_request());
        assert!(out.contains("[SYSTEM]"));
        assert!(out.contains("You are helpful."));
    }

    #[test]
    fn contains_user_label() {
        let out = format_request_debug(&make_simple_request());
        assert!(out.contains("[USER]"));
        assert!(out.contains("Hello!"));
    }

    #[test]
    fn contains_model_metadata() {
        let out = format_request_debug(&make_simple_request());
        assert!(out.contains("Model: gpt-4o"));
        assert!(out.contains("Temperature: 0.7"));
        assert!(out.contains("MaxTokens: 4096"));
    }

    #[test]
    fn defaults_shown_when_no_temperature_or_max_tokens() {
        let req = ChatRequest {
            model: "test".into(),
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let out = format_request_debug(&req);
        assert!(out.contains("Temperature: default"));
        assert!(out.contains("MaxTokens: default"));
    }

    #[test]
    fn shows_tool_definitions() {
        let req = ChatRequest {
            model: "test".into(),
            messages: vec![],
            tools: vec![
                ToolDefinition {
                    name: "file_read".into(),
                    description: "Read a file".into(),
                    parameters: serde_json::json!({}),
                },
                ToolDefinition {
                    name: "file_write".into(),
                    description: "Write a file".into(),
                    parameters: serde_json::json!({}),
                },
            ],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let out = format_request_debug(&req);
        assert!(out.contains("Tools (2 definitions)"));
        assert!(out.contains("file_read"));
        assert!(out.contains("Read a file"));
        assert!(out.contains("file_write"));
    }

    #[test]
    fn no_tools_section_when_empty() {
        let out = format_request_debug(&make_simple_request());
        assert!(!out.contains("Tools ("));
    }

    #[test]
    fn shows_assistant_message_with_tool_calls() {
        let req = ChatRequest {
            model: "test".into(),
            messages: vec![Message {
                role: Role::Assistant,
                content: None,
                tool_calls: vec![ToolCall {
                    id: "call_1".into(),
                    function: FunctionCall {
                        name: "echo".into(),
                        arguments: r#"{"text":"hi"}"#.into(),
                    },
                }],
                tool_call_id: None,
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let out = format_request_debug(&req);
        assert!(out.contains("[ASSISTANT]"));
        assert!(out.contains("[TOOL_CALL]"));
        assert!(out.contains("echo"));
    }

    #[test]
    fn shows_tool_result_message() {
        let req = ChatRequest {
            model: "test".into(),
            messages: vec![Message {
                role: Role::Tool,
                content: Some("result data".into()),
                tool_calls: vec![],
                tool_call_id: Some("call_1".into()),
            }],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let out = format_request_debug(&req);
        assert!(out.contains("[TOOL_RESULT]"));
        assert!(out.contains("result data"));
        assert!(out.contains("tool_call_id: call_1"));
    }

    #[test]
    fn no_ansi_codes_when_not_terminal() {
        // Tests run without a real terminal, so no ANSI codes should appear
        let out = format_request_debug(&make_simple_request());
        assert!(!out.contains(CYAN));
        assert!(!out.contains(RESET));
    }

    #[test]
    fn tool_round_summary_contains_round_and_count() {
        let out = format_tool_round_summary(2, 3);
        assert!(out.contains("round 2"));
        assert!(out.contains("3 tool call(s)"));
        // No ANSI codes in test context (not a terminal)
        assert!(!out.contains(DIM));
    }

    // ── format_tool_call_detail tests ─────────────────────────────────

    #[test]
    fn tool_call_detail_contains_name_and_id() {
        let out = format_tool_call_detail("file_read", "call_42", r#"{"path":"/tmp"}"#);
        assert!(out.contains("[TOOL_EXEC]"));
        assert!(out.contains("file_read"));
        assert!(out.contains("call_42"));
        assert!(out.contains(r#"{"path":"/tmp"}"#));
    }

    #[test]
    fn tool_call_detail_truncates_long_args() {
        let long_args = "x".repeat(250);
        let out = format_tool_call_detail("echo", "c1", &long_args);
        // Should contain first 200 chars + "..."
        assert!(out.contains(&"x".repeat(200)));
        assert!(out.contains("..."));
        // Should NOT contain the full 250-char string
        assert!(!out.contains(&"x".repeat(250)));
    }

    #[test]
    fn tool_call_detail_no_truncation_when_short() {
        let short_args = "x".repeat(200);
        let out = format_tool_call_detail("echo", "c1", &short_args);
        assert!(out.contains(&short_args));
        assert!(!out.contains("..."));
    }

    // ── format_tool_result_detail tests ────────────────────────────────

    #[test]
    fn tool_result_detail_success() {
        let out = format_tool_result_detail("echo", "c1", false, "hello world");
        assert!(out.contains("[TOOL_OK]"));
        assert!(out.contains("echo"));
        assert!(out.contains("c1"));
        assert!(out.contains("hello world"));
        assert!(!out.contains("[TOOL_ERR]"));
    }

    #[test]
    fn tool_result_detail_error() {
        let out = format_tool_result_detail("fail", "c2", true, "something broke");
        assert!(out.contains("[TOOL_ERR]"));
        assert!(out.contains("fail"));
        assert!(out.contains("c2"));
        assert!(out.contains("something broke"));
        assert!(!out.contains("[TOOL_OK]"));
    }

    #[test]
    fn tool_result_detail_truncates_long_content() {
        let long_content = "y".repeat(400);
        let out = format_tool_result_detail("echo", "c1", false, &long_content);
        assert!(out.contains(&"y".repeat(300)));
        assert!(out.contains("..."));
        assert!(!out.contains(&"y".repeat(400)));
    }

    #[test]
    fn tool_result_detail_no_truncation_when_short() {
        let content = "y".repeat(300);
        let out = format_tool_result_detail("echo", "c1", true, &content);
        assert!(out.contains(&content));
        assert!(!out.contains("..."));
    }

    #[test]
    fn tool_call_detail_no_ansi_in_test() {
        let out = format_tool_call_detail("echo", "c1", "{}");
        assert!(!out.contains(MAGENTA));
        assert!(!out.contains(DIM));
        assert!(!out.contains(RESET));
    }

    #[test]
    fn tool_result_detail_no_ansi_in_test() {
        let out = format_tool_result_detail("echo", "c1", false, "ok");
        assert!(!out.contains(GREEN));
        assert!(!out.contains(RED));
        assert!(!out.contains(RESET));
    }

    // ── truncate tests ─────────────────────────────────────────────────

    #[test]
    fn truncate_no_op_when_within_limit() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_appends_ellipsis() {
        assert_eq!(truncate("abcdef", 5), "abcde...");
    }

    #[test]
    fn truncate_handles_multibyte_chars() {
        // "日本語" is 3 chars, each 3 bytes — byte-slicing would panic
        let s = "日本語abc";
        let out = truncate(s, 3);
        assert_eq!(out, "日本語...");
    }

    #[test]
    fn handles_empty_request() {
        let req = ChatRequest {
            model: "empty".into(),
            messages: vec![],
            tools: vec![],
            temperature: None,
            max_tokens: None,
            stream: false,
        };
        let out = format_request_debug(&req);
        assert!(out.contains("DEBUG: Assembled Prompt"));
        assert!(out.contains("Model: empty"));
    }
}
