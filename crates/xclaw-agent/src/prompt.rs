//! Prompt construction and template management.

use xclaw_provider::types::{ChatRequest, Message, Role};

const SYSTEM_PROMPT: &str = "You are xClaw, a helpful AI assistant.";

/// Build a minimal `ChatRequest` for a single user message.
///
/// Includes a system prompt and one user message. No tools, no streaming.
pub fn build_chat_request(model: &str, user_content: &str) -> ChatRequest {
    ChatRequest {
        model: model.to_string(),
        messages: vec![
            Message {
                role: Role::System,
                content: Some(SYSTEM_PROMPT.to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
            Message {
                role: Role::User,
                content: Some(user_content.to_string()),
                tool_calls: vec![],
                tool_call_id: None,
            },
        ],
        tools: vec![],
        temperature: None,
        max_tokens: None,
        stream: false,
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_has_correct_model() {
        let req = build_chat_request("gpt-4o", "hello");
        assert_eq!(req.model, "gpt-4o");
    }

    #[test]
    fn request_contains_system_and_user_messages() {
        let req = build_chat_request("gpt-4o", "hello");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, Role::System);
        assert_eq!(req.messages[1].role, Role::User);
    }

    #[test]
    fn system_message_is_xclaw_prompt() {
        let req = build_chat_request("gpt-4o", "hello");
        let content = req.messages[0].content.as_deref().unwrap();
        assert!(content.contains("xClaw"));
    }

    #[test]
    fn user_message_has_provided_content() {
        let req = build_chat_request("gpt-4o", "what is 2+2?");
        assert_eq!(req.messages[1].content, Some("what is 2+2?".to_string()));
    }

    #[test]
    fn request_has_no_tools() {
        let req = build_chat_request("gpt-4o", "hello");
        assert!(req.tools.is_empty());
    }

    #[test]
    fn request_is_not_streaming() {
        let req = build_chat_request("gpt-4o", "hello");
        assert!(!req.stream);
    }

    #[test]
    fn handles_unicode_content() {
        let req = build_chat_request("claude-sonnet-4-5-20250929", "你好世界 🌍");
        assert_eq!(req.messages[1].content, Some("你好世界 🌍".to_string()));
    }

    #[test]
    fn handles_empty_content() {
        let req = build_chat_request("gpt-4o", "");
        assert_eq!(req.messages[1].content, Some(String::new()));
    }
}
