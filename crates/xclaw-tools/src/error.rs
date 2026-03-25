//! Error types for the xclaw-tools crate.

use std::path::PathBuf;

/// Errors that can occur during tool execution.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("path denied: {0} is not within the allowed workspace")]
    PathDenied(PathBuf),

    #[error("path traversal detected: {0}")]
    PathTraversal(PathBuf),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("tool execution timed out")]
    Timeout,

    #[error("invalid parameters: {0}")]
    InvalidParams(String),

    #[error("edit target not found: search text not matched in file")]
    EditNotFound,

    #[error("internal tool error: {0}")]
    Internal(String),
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_denied_display() {
        let e = ToolError::PathDenied(PathBuf::from("/etc/passwd"));
        assert_eq!(
            e.to_string(),
            "path denied: /etc/passwd is not within the allowed workspace"
        );
    }

    #[test]
    fn path_traversal_display() {
        let e = ToolError::PathTraversal(PathBuf::from("../../../etc/passwd"));
        assert_eq!(
            e.to_string(),
            "path traversal detected: ../../../etc/passwd"
        );
    }

    #[test]
    fn io_error_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let e = ToolError::from(io_err);
        assert!(e.to_string().contains("file not found"));
    }

    #[test]
    fn timeout_display() {
        let e = ToolError::Timeout;
        assert_eq!(e.to_string(), "tool execution timed out");
    }

    #[test]
    fn invalid_params_display() {
        let e = ToolError::InvalidParams("missing 'path' field".to_string());
        assert_eq!(e.to_string(), "invalid parameters: missing 'path' field");
    }

    #[test]
    fn edit_not_found_display() {
        let e = ToolError::EditNotFound;
        assert_eq!(
            e.to_string(),
            "edit target not found: search text not matched in file"
        );
    }

    #[test]
    fn internal_display() {
        let e = ToolError::Internal("unexpected state".to_string());
        assert_eq!(e.to_string(), "internal tool error: unexpected state");
    }
}
