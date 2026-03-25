//! Path security validation for tool execution.
//!
//! All file-system tools must validate paths through this module before
//! performing any I/O. Guards against path traversal and allowlist violations.

use std::path::{Path, PathBuf};

use crate::error::ToolError;
use crate::traits::ToolContext;

/// Validate that a path is safe to read within the given context.
///
/// 1. Canonicalizes the path (resolves symlinks, `..`, `.`)
/// 2. Checks the canonical path is under `ctx.scope.workspace_root`
///    or within one of `ctx.fs_allowlist` entries
///
/// Returns the canonicalized path on success.
pub fn validate_path(path: &Path, ctx: &ToolContext) -> Result<PathBuf, ToolError> {
    let canonical = std::fs::canonicalize(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::PathTraversal(path.to_path_buf())
        } else {
            ToolError::Io(e)
        }
    })?;

    if is_within_allowlist(&canonical, ctx) {
        Ok(canonical)
    } else {
        Err(ToolError::PathDenied(path.to_path_buf()))
    }
}

/// Validate that a path is safe to write within the given context.
///
/// Unlike [`validate_path`], the target file may not yet exist.
/// We canonicalize the nearest existing ancestor, then verify the
/// full reconstructed path falls within the allowlist.
pub fn validate_path_for_write(path: &Path, ctx: &ToolContext) -> Result<PathBuf, ToolError> {
    // Check for suspicious components before any filesystem access
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return Err(ToolError::PathTraversal(path.to_path_buf()));
        }
    }

    // Try direct canonicalize first (file already exists)
    if let Ok(canonical) = std::fs::canonicalize(path) {
        if is_within_allowlist(&canonical, ctx) {
            return Ok(canonical);
        }
        return Err(ToolError::PathDenied(path.to_path_buf()));
    }

    // File doesn't exist — find nearest existing ancestor
    let (existing_ancestor, remaining) = find_existing_ancestor(path)?;
    let canonical_ancestor = std::fs::canonicalize(&existing_ancestor).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ToolError::PathTraversal(path.to_path_buf())
        } else {
            ToolError::Io(e)
        }
    })?;

    let target = canonical_ancestor.join(remaining);

    if is_within_allowlist(&target, ctx) {
        Ok(target)
    } else {
        Err(ToolError::PathDenied(path.to_path_buf()))
    }
}

/// Check if `path` starts with any entry in the allowlist.
///
/// Allowlist entries are pre-canonicalized at `ToolContext` construction time,
/// so no filesystem syscalls are needed here.
fn is_within_allowlist(path: &Path, ctx: &ToolContext) -> bool {
    ctx.fs_allowlist()
        .iter()
        .any(|allowed| path.starts_with(allowed))
}

/// Walk up from `path` until we find an ancestor that exists.
/// Returns (existing_ancestor, remaining_components).
fn find_existing_ancestor(path: &Path) -> Result<(PathBuf, PathBuf), ToolError> {
    let mut current = path.to_path_buf();
    let mut remaining_parts: Vec<std::ffi::OsString> = Vec::new();

    loop {
        if current.exists() {
            let remaining: PathBuf = remaining_parts.iter().rev().collect();
            return Ok((current, remaining));
        }
        match current.file_name() {
            Some(name) => {
                remaining_parts.push(name.to_os_string());
                current = current
                    .parent()
                    .ok_or_else(|| ToolError::PathTraversal(path.to_path_buf()))?
                    .to_path_buf();
            }
            None => return Err(ToolError::PathTraversal(path.to_path_buf())),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::{ToolContext, WorkspaceScope};
    use std::time::Duration;
    use tempfile::TempDir;

    fn test_ctx(workspace: &Path) -> ToolContext {
        ToolContext::new(WorkspaceScope::new(workspace), Duration::from_secs(30))
    }

    #[test]
    fn validate_path_accepts_file_inside_workspace() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("hello.txt");
        std::fs::write(&file, "content").unwrap();

        let ctx = test_ctx(tmp.path());
        let result = validate_path(&file, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_path_rejects_file_outside_workspace() {
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let file = outside.path().join("secret.txt");
        std::fs::write(&file, "secret").unwrap();

        let ctx = test_ctx(tmp.path());
        let result = validate_path(&file, &ctx);
        assert!(matches!(result, Err(ToolError::PathDenied(_))));
    }

    #[test]
    fn validate_path_rejects_traversal_via_dotdot() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();

        // Create a file outside workspace
        let outside_file = tmp.path().join("../../../etc/passwd");
        let ctx = test_ctx(&sub);
        let result = validate_path(&outside_file, &ctx);
        // Should either fail to canonicalize or be denied
        assert!(result.is_err());
    }

    #[test]
    fn validate_path_for_write_accepts_new_file_in_workspace() {
        let tmp = TempDir::new().unwrap();
        let new_file = tmp.path().join("new_file.txt");

        let ctx = test_ctx(tmp.path());
        let result = validate_path_for_write(&new_file, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_path_for_write_accepts_nested_new_file() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("a/b/c/deep.txt");

        let ctx = test_ctx(tmp.path());
        let result = validate_path_for_write(&nested, &ctx);
        assert!(result.is_ok());
    }

    #[test]
    fn validate_path_for_write_rejects_dotdot_traversal() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        std::fs::create_dir(&sub).unwrap();

        let evil = sub.join("../../../etc/evil.txt");
        let ctx = test_ctx(&sub);
        let result = validate_path_for_write(&evil, &ctx);
        assert!(matches!(result, Err(ToolError::PathTraversal(_))));
    }

    #[test]
    fn validate_path_for_write_rejects_outside_workspace() {
        let tmp = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let file = outside.path().join("evil.txt");
        std::fs::write(&file, "x").unwrap();

        let ctx = test_ctx(tmp.path());
        let result = validate_path_for_write(&file, &ctx);
        assert!(matches!(result, Err(ToolError::PathDenied(_))));
    }

    #[test]
    fn validate_path_accepts_file_in_extra_allowlist_entry() {
        let tmp = TempDir::new().unwrap();
        let extra = TempDir::new().unwrap();
        let file = extra.path().join("allowed.txt");
        std::fs::write(&file, "ok").unwrap();

        let ctx = test_ctx(tmp.path()).with_extra_paths(vec![extra.path().to_path_buf()]);

        let result = validate_path(&file, &ctx);
        assert!(result.is_ok());
    }
}
