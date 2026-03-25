//! Tool registration and dispatch.

use std::collections::HashMap;

use crate::traits::{Tool, ToolSchema};

/// Central registry for all available tools.
///
/// Tools are registered at startup and looked up by name during
/// agent loop execution.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

// Compile-time assertion: ToolRegistry must be Send + Sync for async agent loops.
#[allow(dead_code)]
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    fn check() {
        assert_send_sync::<ToolRegistry>();
    }
};

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. If a tool with the same name already exists,
    /// it is replaced and a warning is logged.
    pub fn register(&mut self, tool: impl Tool + 'static) {
        let name = tool.name().to_string();
        if self.tools.contains_key(&name) {
            tracing::warn!(tool_name = %name, "overwriting previously registered tool");
        }
        self.tools.insert(name, Box::new(tool));
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// Return schemas for all registered tools, sorted by name.
    pub fn list_schemas(&self) -> Vec<ToolSchema> {
        let mut schemas: Vec<ToolSchema> = self
            .tools
            .values()
            .map(|t| ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters_schema(),
            })
            .collect();
        schemas.sort_by(|a, b| a.name.cmp(&b.name));
        schemas
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ToolError;
    use crate::traits::{ToolContext, ToolOutput};
    use async_trait::async_trait;

    struct FakeTool {
        tool_name: &'static str,
    }

    #[async_trait]
    impl Tool for FakeTool {
        fn name(&self) -> &str {
            self.tool_name
        }
        fn description(&self) -> &str {
            "A fake tool for testing"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(
            &self,
            _ctx: &ToolContext,
            _params: serde_json::Value,
        ) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::success("fake"))
        }
    }

    #[test]
    fn new_registry_is_empty() {
        let reg = ToolRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn register_and_get_tool() {
        let mut reg = ToolRegistry::new();
        reg.register(FakeTool { tool_name: "alpha" });

        let tool = reg.get("alpha");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name(), "alpha");
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let reg = ToolRegistry::new();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn register_overwrites_duplicate_name() {
        let mut reg = ToolRegistry::new();
        reg.register(FakeTool { tool_name: "dup" });
        reg.register(FakeTool { tool_name: "dup" });
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn list_schemas_returns_sorted() {
        let mut reg = ToolRegistry::new();
        reg.register(FakeTool { tool_name: "zebra" });
        reg.register(FakeTool { tool_name: "alpha" });
        reg.register(FakeTool {
            tool_name: "middle",
        });

        let schemas = reg.list_schemas();
        assert_eq!(schemas.len(), 3);
        assert_eq!(schemas[0].name, "alpha");
        assert_eq!(schemas[1].name, "middle");
        assert_eq!(schemas[2].name, "zebra");
    }

    #[test]
    fn list_schemas_empty_registry() {
        let reg = ToolRegistry::new();
        assert!(reg.list_schemas().is_empty());
    }

    #[test]
    fn default_creates_empty_registry() {
        let reg = ToolRegistry::default();
        assert!(reg.is_empty());
    }
}
