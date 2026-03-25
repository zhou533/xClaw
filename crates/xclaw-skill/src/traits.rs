//! Skill traits and associated types.

use serde::{Deserialize, Serialize};

use xclaw_core::error::XClawError;

/// Output produced by executing a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOutput {
    pub content: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Metadata describing a registered skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
}

/// A reusable, high-level skill that can be invoked by the agent.
pub trait Skill: Send + Sync {
    /// Unique skill name.
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema describing the parameters this skill accepts.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the skill with the given parameters.
    fn execute(
        &self,
        params: serde_json::Value,
    ) -> impl std::future::Future<Output = Result<SkillOutput, XClawError>> + Send;
}

/// Registry for discovering and executing skills.
///
/// `Skill` uses RPITIT and is not dyn-compatible, so the registry
/// provides `execute_skill` to dispatch by name without requiring
/// `&dyn Skill`.
pub trait SkillRegistry: Send + Sync {
    /// Look up a skill by name and execute it with the given params.
    fn execute_skill(
        &self,
        name: &str,
        params: serde_json::Value,
    ) -> impl std::future::Future<Output = Result<SkillOutput, XClawError>> + Send;

    /// List all registered skills.
    fn list(&self) -> Vec<SkillInfo>;
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct StubSkill;

    impl Skill for StubSkill {
        fn name(&self) -> &str {
            "stub"
        }

        fn description(&self) -> &str {
            "A stub skill"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

        async fn execute(&self, _params: serde_json::Value) -> Result<SkillOutput, XClawError> {
            Ok(SkillOutput {
                content: "done".to_string(),
                metadata: serde_json::Value::Null,
            })
        }
    }

    #[test]
    fn skill_output_with_empty_metadata() {
        let out = SkillOutput {
            content: "done".to_string(),
            metadata: serde_json::Value::Null,
        };
        assert_eq!(out.content, "done");
    }

    #[test]
    fn skill_info_round_trips() {
        let info = SkillInfo {
            name: "summarize".to_string(),
            description: "Summarize text".to_string(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: SkillInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "summarize");
    }

    #[tokio::test]
    async fn stub_skill_executes() {
        let skill = StubSkill;
        assert_eq!(skill.name(), "stub");
        let out = skill.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(out.content, "done");
    }
}
