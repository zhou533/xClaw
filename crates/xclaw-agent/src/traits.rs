//! Agent loop trait and associated types.

use serde::{Deserialize, Serialize};

use xclaw_core::error::XClawError;
use xclaw_core::types::SessionId;

/// Input from a user destined for the agent loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInput {
    pub session_id: SessionId,
    pub content: String,
}

/// Response produced by the agent loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub content: String,
    #[serde(default)]
    pub tool_calls_count: u32,
}

/// The agent loop: receives user input and produces a response,
/// potentially invoking tools and skills along the way.
pub trait AgentLoop: Send + Sync {
    fn process(
        &self,
        input: UserInput,
    ) -> impl std::future::Future<Output = Result<AgentResponse, XClawError>> + Send;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct StubAgent;

    impl AgentLoop for StubAgent {
        async fn process(&self, input: UserInput) -> Result<AgentResponse, XClawError> {
            Ok(AgentResponse {
                content: format!("echo: {}", input.content),
                tool_calls_count: 0,
            })
        }
    }

    #[test]
    fn user_input_serializes() {
        let input = UserInput {
            session_id: SessionId::new("s1"),
            content: "hi".to_string(),
        };
        let v = serde_json::to_value(&input).unwrap();
        assert_eq!(v["content"], "hi");
    }

    #[test]
    fn agent_response_defaults() {
        let resp = AgentResponse {
            content: "reply".to_string(),
            tool_calls_count: 0,
        };
        assert_eq!(resp.tool_calls_count, 0);
    }

    #[tokio::test]
    async fn stub_agent_echoes_input() {
        let agent = StubAgent;
        let input = UserInput {
            session_id: SessionId::new("s1"),
            content: "hello".to_string(),
        };
        let resp = agent.process(input).await.unwrap();
        assert_eq!(resp.content, "echo: hello");
    }
}
