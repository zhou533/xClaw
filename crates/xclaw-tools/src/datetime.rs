//! Tool for retrieving the current date and time on the host machine.

use async_trait::async_trait;
use chrono::{Local, Utc};
use serde::Deserialize;

use crate::error::ToolError;
use crate::registry::ToolRegistry;
use crate::traits::{Tool, ToolContext, ToolOutput};

/// Maximum length of the `format` parameter to prevent unbounded output.
const MAX_FORMAT_LEN: usize = 256;

// ─── Params ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct GetCurrentDatetimeParams {
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    timezone: Option<String>,
}

// ─── GetCurrentDatetimeTool ─────────────────────────────────────────────────

pub struct GetCurrentDatetimeTool;

#[async_trait]
impl Tool for GetCurrentDatetimeTool {
    fn name(&self) -> &str {
        "get_current_datetime"
    }

    fn description(&self) -> &str {
        "Get the current date and time on the host machine."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "format": {
                    "type": "string",
                    "maxLength": 256,
                    "description": "strftime format string (default: ISO 8601)"
                },
                "timezone": {
                    "type": "string",
                    "enum": ["local", "utc"],
                    "description": "Timezone to use: 'local' (default) or 'utc'"
                }
            }
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let p: GetCurrentDatetimeParams =
            serde_json::from_value(params).map_err(|e| ToolError::InvalidParams(e.to_string()))?;

        let tz = p.timezone.as_deref().unwrap_or("local");
        let fmt = p.format.as_deref().unwrap_or("%Y-%m-%dT%H:%M:%S%:z");

        if fmt.is_empty() {
            return Err(ToolError::InvalidParams(
                "format string must not be empty".to_string(),
            ));
        }
        if fmt.len() > MAX_FORMAT_LEN {
            return Err(ToolError::InvalidParams(format!(
                "format string too long: {} chars (max {MAX_FORMAT_LEN})",
                fmt.len()
            )));
        }

        match tz {
            "local" => {
                let now = Local::now();
                Ok(ToolOutput::success(now.format(fmt).to_string()))
            }
            "utc" => {
                let now = Utc::now();
                Ok(ToolOutput::success(now.format(fmt).to_string()))
            }
            other => Err(ToolError::InvalidParams(format!(
                "invalid timezone '{other}': must be 'local' or 'utc'"
            ))),
        }
    }
}

// ─── Registration ───────────────────────────────────────────────────────────

/// Register datetime tools with the given registry.
pub fn register_datetime_tools(registry: &mut ToolRegistry) {
    registry.register(GetCurrentDatetimeTool);
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::WorkspaceScope;
    use std::time::Duration;
    use tempfile::TempDir;

    fn test_ctx(workspace: &std::path::Path) -> ToolContext {
        ToolContext::new(WorkspaceScope::new(workspace), Duration::from_secs(30))
    }

    #[test]
    fn tool_name_is_correct() {
        let tool = GetCurrentDatetimeTool;
        assert_eq!(tool.name(), "get_current_datetime");
    }

    #[test]
    fn parameters_schema_is_valid_json() {
        let tool = GetCurrentDatetimeTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["format"].is_object());
        assert!(schema["properties"]["timezone"].is_object());
    }

    #[tokio::test]
    async fn returns_datetime_in_default_format() {
        let tmp = TempDir::new().unwrap();
        let tool = GetCurrentDatetimeTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({});

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        assert!(!result.content.is_empty());
        // Should be parseable as ISO 8601 with timezone offset
        assert!(
            chrono::DateTime::parse_from_str(&result.content, "%Y-%m-%dT%H:%M:%S%:z").is_ok(),
            "output '{}' is not valid ISO 8601",
            result.content
        );
    }

    #[tokio::test]
    async fn returns_local_when_timezone_is_local() {
        let tmp = TempDir::new().unwrap();
        let tool = GetCurrentDatetimeTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({ "timezone": "local" });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        assert!(chrono::DateTime::parse_from_str(&result.content, "%Y-%m-%dT%H:%M:%S%:z").is_ok());
    }

    #[tokio::test]
    async fn returns_utc_when_timezone_is_utc() {
        let tmp = TempDir::new().unwrap();
        let tool = GetCurrentDatetimeTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({ "timezone": "utc" });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        let parsed = chrono::DateTime::parse_from_str(&result.content, "%Y-%m-%dT%H:%M:%S%:z")
            .expect("UTC output should parse as ISO 8601");
        assert_eq!(
            parsed.offset().local_minus_utc(),
            0,
            "UTC offset should be zero"
        );
    }

    #[tokio::test]
    async fn rejects_invalid_timezone() {
        let tmp = TempDir::new().unwrap();
        let tool = GetCurrentDatetimeTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({ "timezone": "mars" });

        let result = tool.execute(&ctx, params).await;
        assert!(matches!(result, Err(ToolError::InvalidParams(_))));
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("mars"));
    }

    #[tokio::test]
    async fn custom_format_works() {
        let tmp = TempDir::new().unwrap();
        let tool = GetCurrentDatetimeTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({ "format": "%Y-%m-%d" });

        let result = tool.execute(&ctx, params).await.unwrap();
        assert!(!result.is_error);
        // Should match YYYY-MM-DD pattern
        assert_eq!(
            result.content.len(),
            10,
            "date-only format should be 10 chars"
        );
        assert!(result.content.contains('-'));
    }

    #[tokio::test]
    async fn rejects_empty_format() {
        let tmp = TempDir::new().unwrap();
        let tool = GetCurrentDatetimeTool;
        let ctx = test_ctx(tmp.path());
        let params = serde_json::json!({ "format": "" });

        let result = tool.execute(&ctx, params).await;
        assert!(matches!(result, Err(ToolError::InvalidParams(_))));
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[tokio::test]
    async fn rejects_too_long_format() {
        let tmp = TempDir::new().unwrap();
        let tool = GetCurrentDatetimeTool;
        let ctx = test_ctx(tmp.path());
        let long_fmt = "%Y".repeat(200);
        let params = serde_json::json!({ "format": long_fmt });

        let result = tool.execute(&ctx, params).await;
        assert!(matches!(result, Err(ToolError::InvalidParams(_))));
        assert!(result.unwrap_err().to_string().contains("too long"));
    }

    #[test]
    fn register_datetime_tools_adds_tool() {
        let mut reg = ToolRegistry::new();
        register_datetime_tools(&mut reg);
        assert!(reg.get("get_current_datetime").is_some());
    }
}
