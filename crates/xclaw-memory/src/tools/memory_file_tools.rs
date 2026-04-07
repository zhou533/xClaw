//! Unified memory file read/write tools for LLM function calling.

use std::path::PathBuf;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use xclaw_tools::error::ToolError;
use xclaw_tools::traits::{Tool, ToolContext, ToolOutput};

use super::{parse_role, to_tool_error};
use crate::error::MemoryError;
use crate::workspace::loader::{FsMemoryFileLoader, MemoryFileLoader};
use crate::workspace::types::MemoryFileKind;

/// Compute a short content hash (first 16 hex chars of SHA-256, 64 bits).
///
/// Provides sufficient collision resistance for optimistic concurrency control
/// (read-then-write safety) but is NOT suitable for security-critical integrity
/// verification.
pub(crate) fn compute_content_hash(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    let hex = format!("{digest:x}");
    hex[..16].to_owned()
}

/// Format file content with YAML front matter containing the hash,
/// followed by numbered lines.
pub(crate) fn format_with_line_numbers(content: &str, hash: &str) -> String {
    let header = format!("---\ncontent_hash: {hash}\n---\n");
    let body: String = content
        .lines()
        .enumerate()
        .map(|(i, line)| format!("{:>4} | {line}\n", i + 1))
        .collect();
    header + &body
}

fn parse_kind(params: &serde_json::Value) -> Result<MemoryFileKind, ToolError> {
    let kind_str = params["kind"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidParams("kind is required".into()))?;
    MemoryFileKind::from_str_name(kind_str).ok_or_else(|| {
        ToolError::InvalidParams(format!(
            "invalid memory file kind: '{kind_str}' \
             (valid: agents, soul, tools, identity, user, heartbeat, bootstrap, long_term)"
        ))
    })
}

// ─── MemoryFileReadTool ─────────────────────────────────────────────────────

pub struct MemoryFileReadTool {
    base_dir: PathBuf,
}

impl MemoryFileReadTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryFileReadTool {
    fn name(&self) -> &str {
        "memory_file_read"
    }

    fn description(&self) -> &str {
        "Read a role's memory file by kind. Returns a YAML front matter block with \
         content_hash, followed by numbered lines (e.g. '   1 | content'). Pass the \
         content_hash to memory_file_append or memory_file_edit to ensure safe writes. \
         Returns a plain message if the file does not exist."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind: agents=collaboration rules, soul=AI persona, tools=tool guidance, identity=self-identity, user=user preferences, heartbeat=action reference, bootstrap=workspace bootstrap, long_term=distilled knowledge (MEMORY.md)"
                }
            },
            "required": ["kind"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let kind = parse_kind(&params)?;

        let loader = FsMemoryFileLoader::new(&self.base_dir);
        let content = loader.load_file(&role, kind).await.map_err(to_tool_error)?;

        match content {
            Some(c) => {
                let hash = compute_content_hash(&c);
                Ok(ToolOutput::success(format_with_line_numbers(&c, &hash)))
            }
            None => Ok(ToolOutput::success(format!(
                "{} does not exist for role '{}'",
                kind.filename(),
                role
            ))),
        }
    }
}

// ─── MemoryFileDeleteTool ───────────────────────────────────────────────────

pub struct MemoryFileDeleteTool {
    base_dir: PathBuf,
}

impl MemoryFileDeleteTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryFileDeleteTool {
    fn name(&self) -> &str {
        "memory_file_delete"
    }

    fn description(&self) -> &str {
        "Delete a role's memory file by kind. Primarily used to remove BOOTSTRAP.md after workspace bootstrap is complete. Other memory file kinds can also be deleted."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind: agents=collaboration rules, soul=AI persona, tools=tool guidance, identity=self-identity, user=user preferences, heartbeat=action reference, bootstrap=workspace bootstrap, long_term=distilled knowledge (MEMORY.md)"
                }
            },
            "required": ["kind"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let kind = parse_kind(&params)?;

        let loader = FsMemoryFileLoader::new(&self.base_dir);
        let deleted = loader
            .delete_file(&role, kind)
            .await
            .map_err(to_tool_error)?;

        if deleted {
            Ok(ToolOutput::success(format!(
                "{} deleted for role '{}'",
                kind.filename(),
                role
            )))
        } else {
            Ok(ToolOutput::success(format!(
                "{} does not exist for role '{}'",
                kind.filename(),
                role
            )))
        }
    }
}

// ─── apply_line_edit (pure function) ────────────────────────────────────────

/// Apply a line-based edit to a slice of lines.
///
/// Line numbers are 1-based. Supported operations: `replace`, `insert_before`,
/// `insert_after`.
pub(crate) fn apply_line_edit(
    lines: &[&str],
    line_start: usize,
    line_end: usize,
    operation: &str,
    new_content: &str,
) -> Result<String, MemoryError> {
    let total = lines.len();

    if line_start == 0 || line_start > total {
        return Err(MemoryError::LineOutOfRange {
            line: line_start,
            total,
        });
    }
    if line_end < line_start {
        return Err(MemoryError::InvalidLineRange {
            start: line_start,
            end: line_end,
        });
    }
    if line_end > total {
        return Err(MemoryError::LineOutOfRange {
            line: line_end,
            total,
        });
    }

    let before = &lines[..line_start - 1];
    let after = &lines[line_end..];
    let new_lines: Vec<&str> = new_content.lines().collect();

    let result = match operation {
        "replace" => [before, new_lines.as_slice(), after].concat(),
        "insert_before" => {
            let middle = &lines[line_start - 1..line_end];
            [before, new_lines.as_slice(), middle, after].concat()
        }
        "insert_after" => {
            let middle = &lines[line_start - 1..line_end];
            [before, middle, new_lines.as_slice(), after].concat()
        }
        other => {
            return Err(MemoryError::UnknownOperation(other.to_owned()));
        }
    };

    Ok(result.join("\n") + "\n")
}

// ─── MemoryFileAppendTool ────────────────────────────────────────────────────

pub struct MemoryFileAppendTool {
    base_dir: PathBuf,
}

impl MemoryFileAppendTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryFileAppendTool {
    fn name(&self) -> &str {
        "memory_file_append"
    }

    fn description(&self) -> &str {
        "Append content to a role's memory file. Requires a content_hash obtained \
         from memory_file_read to prevent overwriting concurrent changes. Use \
         content_hash='__new__' when creating a file that does not yet exist."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind"
                },
                "content": { "type": "string", "description": "Markdown content to append." },
                "content_hash": {
                    "type": "string",
                    "description": "Hash from memory_file_read. Use '__new__' when the file does not yet exist."
                }
            },
            "required": ["kind", "content", "content_hash"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let kind = parse_kind(&params)?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("content is required".into()))?;
        let provided_hash = params["content_hash"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("content_hash is required".into()))?;

        let loader = FsMemoryFileLoader::new(&self.base_dir);
        let existing = loader.load_file(&role, kind).await.map_err(to_tool_error)?;

        match existing {
            Some(ref current) => {
                let actual_hash = compute_content_hash(current);
                if provided_hash != actual_hash {
                    return Err(to_tool_error(MemoryError::StaleContent {
                        expected: provided_hash.to_owned(),
                        actual: actual_hash,
                    }));
                }
                loader
                    .append_file(&role, kind, content)
                    .await
                    .map_err(to_tool_error)?;
            }
            None => {
                if provided_hash != "__new__" {
                    return Err(to_tool_error(MemoryError::StaleContent {
                        expected: provided_hash.to_owned(),
                        actual: "__new__".to_owned(),
                    }));
                }
                loader
                    .save_file(&role, kind, content)
                    .await
                    .map_err(to_tool_error)?;
            }
        }

        Ok(ToolOutput::success(format!(
            "{} appended for role '{}'",
            kind.filename(),
            role
        )))
    }
}

// ─── MemoryFileEditTool ──────────────────────────────────────────────────────

pub struct MemoryFileEditTool {
    base_dir: PathBuf,
}

impl MemoryFileEditTool {
    pub fn new(base_dir: &std::path::Path) -> Self {
        Self {
            base_dir: base_dir.to_path_buf(),
        }
    }
}

#[async_trait]
impl Tool for MemoryFileEditTool {
    fn name(&self) -> &str {
        "memory_file_edit"
    }

    fn description(&self) -> &str {
        "Edit specific lines of a role's memory file. Requires a content_hash from \
         memory_file_read. Operations: replace (overwrite lines), insert_before (add \
         before line_start), insert_after (add after line_end). Line numbers are 1-based."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "role": { "type": "string", "description": "Role identifier in snake_case (default: 'default')" },
                "kind": {
                    "type": "string",
                    "enum": ["agents", "soul", "tools", "identity", "user", "heartbeat", "bootstrap", "long_term"],
                    "description": "Memory file kind"
                },
                "content_hash": { "type": "string", "description": "Hash from memory_file_read." },
                "line_start": { "type": "integer", "description": "First line to edit (1-based)." },
                "line_end": { "type": "integer", "description": "Last line to edit inclusive (defaults to line_start)." },
                "operation": {
                    "type": "string",
                    "enum": ["replace", "insert_before", "insert_after"],
                    "description": "Edit operation."
                },
                "content": { "type": "string", "description": "New content for the operation." }
            },
            "required": ["kind", "content_hash", "line_start", "operation", "content"]
        })
    }

    async fn execute(
        &self,
        _ctx: &ToolContext,
        params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        let role = parse_role(&params)?;
        let kind = parse_kind(&params)?;
        let provided_hash = params["content_hash"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("content_hash is required".into()))?;
        let line_start = params["line_start"]
            .as_u64()
            .ok_or_else(|| ToolError::InvalidParams("line_start is required".into()))?
            as usize;
        let line_end = params["line_end"]
            .as_u64()
            .map(|v| v as usize)
            .unwrap_or(line_start);
        let operation = params["operation"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("operation is required".into()))?;
        let content = params["content"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("content is required".into()))?;

        let loader = FsMemoryFileLoader::new(&self.base_dir);
        let current = loader
            .load_file(&role, kind)
            .await
            .map_err(to_tool_error)?
            .ok_or_else(|| {
                ToolError::InvalidParams(format!(
                    "{} does not exist for role '{}'; use memory_file_append to create it",
                    kind.filename(),
                    role
                ))
            })?;

        let actual_hash = compute_content_hash(&current);
        if provided_hash != actual_hash {
            return Err(to_tool_error(MemoryError::StaleContent {
                expected: provided_hash.to_owned(),
                actual: actual_hash,
            }));
        }

        let lines: Vec<&str> = current.lines().collect();
        let new_content = apply_line_edit(&lines, line_start, line_end, operation, content)
            .map_err(to_tool_error)?;

        loader
            .save_file(&role, kind, &new_content)
            .await
            .map_err(to_tool_error)?;

        Ok(ToolOutput::success(format!(
            "{} edited for role '{}'",
            kind.filename(),
            role
        )))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── compute_content_hash ─────────────────────────────────────────────────

    #[test]
    fn compute_hash_deterministic() {
        let h1 = compute_content_hash("hello world");
        let h2 = compute_content_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn compute_hash_different_for_different_content() {
        let h1 = compute_content_hash("content A");
        let h2 = compute_content_hash("content B");
        assert_ne!(h1, h2);
    }

    #[test]
    fn compute_hash_length_is_16() {
        let h = compute_content_hash("any content here");
        assert_eq!(h.len(), 16);
    }

    // ── format_with_line_numbers ─────────────────────────────────────────────

    #[test]
    fn format_with_line_numbers_output() {
        let result = format_with_line_numbers("line one\nline two", "abc1234567890123");
        assert!(result.contains("content_hash: abc1234567890123"));
        assert!(result.contains("   1 | line one"));
        assert!(result.contains("   2 | line two"));
    }

    #[test]
    fn format_with_line_numbers_has_yaml_front_matter() {
        let result = format_with_line_numbers("hello", "deadbeef01234567");
        assert!(result.starts_with("---\n"));
        assert!(result.contains("---\n"));
    }

    // ── apply_line_edit ──────────────────────────────────────────────────────

    #[test]
    fn apply_replace_single_line() {
        let lines = vec!["line1", "line2", "line3"];
        let result = apply_line_edit(&lines, 2, 2, "replace", "REPLACED").unwrap();
        assert_eq!(result, "line1\nREPLACED\nline3\n");
    }

    #[test]
    fn apply_replace_range() {
        let lines = vec!["a", "b", "c", "d"];
        let result = apply_line_edit(&lines, 2, 3, "replace", "X\nY").unwrap();
        assert_eq!(result, "a\nX\nY\nd\n");
    }

    #[test]
    fn apply_insert_before() {
        let lines = vec!["a", "b", "c"];
        let result = apply_line_edit(&lines, 2, 2, "insert_before", "NEW").unwrap();
        assert_eq!(result, "a\nNEW\nb\nc\n");
    }

    #[test]
    fn apply_insert_after() {
        let lines = vec!["a", "b", "c"];
        let result = apply_line_edit(&lines, 2, 2, "insert_after", "NEW").unwrap();
        assert_eq!(result, "a\nb\nNEW\nc\n");
    }

    #[test]
    fn apply_line_out_of_range_start() {
        let lines = vec!["a", "b"];
        let err = apply_line_edit(&lines, 5, 5, "replace", "X").unwrap_err();
        assert!(matches!(err, MemoryError::LineOutOfRange { .. }));
    }

    #[test]
    fn apply_invalid_line_range() {
        let lines = vec!["a", "b", "c"];
        let err = apply_line_edit(&lines, 3, 1, "replace", "X").unwrap_err();
        assert!(matches!(err, MemoryError::InvalidLineRange { .. }));
    }

    #[test]
    fn apply_unknown_operation() {
        let lines = vec!["a", "b"];
        let err = apply_line_edit(&lines, 1, 1, "bad_op", "X").unwrap_err();
        assert!(matches!(err, MemoryError::UnknownOperation(_)));
    }

    #[test]
    fn apply_replace_last_line() {
        let lines = vec!["first", "last"];
        let result = apply_line_edit(&lines, 2, 2, "replace", "LAST_REPLACED").unwrap();
        assert_eq!(result, "first\nLAST_REPLACED\n");
    }
}
