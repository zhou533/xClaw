//! Memory file loader.
//!
//! Reads/writes structured context files (AGENTS.md, SOUL.md, MEMORY.md, etc.)
//! from the role directory. All files are optional.

use std::collections::HashMap;
use std::path::PathBuf;

use xclaw_core::types::RoleId;

use crate::error::MemoryError;
use crate::workspace::types::{MemoryFileKind, MemorySnapshot};

/// Loader for memory files (workspace files + long-term memory).
pub trait MemoryFileLoader: Send + Sync {
    /// Load a single memory file. Returns `Ok(None)` if the file doesn't exist.
    fn load_file(
        &self,
        role: &RoleId,
        kind: MemoryFileKind,
    ) -> impl std::future::Future<Output = Result<Option<String>, MemoryError>> + Send;

    /// Write a memory file (atomic write).
    fn save_file(
        &self,
        role: &RoleId,
        kind: MemoryFileKind,
        content: &str,
    ) -> impl std::future::Future<Output = Result<(), MemoryError>> + Send;

    /// Load all memory files as a snapshot.
    fn load_snapshot(
        &self,
        role: &RoleId,
    ) -> impl std::future::Future<Output = Result<MemorySnapshot, MemoryError>> + Send;
}

/// Filesystem-backed memory file loader.
pub struct FsMemoryFileLoader {
    base_dir: PathBuf,
}

impl FsMemoryFileLoader {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn file_path(&self, role: &RoleId, kind: MemoryFileKind) -> PathBuf {
        self.base_dir
            .join("roles")
            .join(role.as_str())
            .join(kind.filename())
    }
}

impl MemoryFileLoader for FsMemoryFileLoader {
    async fn load_file(
        &self,
        role: &RoleId,
        kind: MemoryFileKind,
    ) -> Result<Option<String>, MemoryError> {
        let path = self.file_path(role, kind);
        if !path.exists() {
            return Ok(None);
        }
        let content = tokio::fs::read_to_string(&path).await?;
        Ok(Some(content))
    }

    async fn save_file(
        &self,
        role: &RoleId,
        kind: MemoryFileKind,
        content: &str,
    ) -> Result<(), MemoryError> {
        let path = self.file_path(role, kind);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        // Atomic write: unique temp file then rename
        let parent = path.parent().ok_or_else(|| {
            MemoryError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("memory file path has no parent: {}", path.display()),
            ))
        })?;
        let tmp = tempfile::NamedTempFile::new_in(parent)?;
        tokio::fs::write(tmp.path(), content).await?;
        tmp.persist(&path).map_err(|e| e.error)?;
        Ok(())
    }

    async fn load_snapshot(&self, role: &RoleId) -> Result<MemorySnapshot, MemoryError> {
        let mut files = HashMap::new();
        for &kind in MemoryFileKind::all() {
            let content = self.load_file(role, kind).await?;
            files.insert(kind, content);
        }
        Ok(MemorySnapshot { files })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(tmp: &std::path::Path) -> FsMemoryFileLoader {
        FsMemoryFileLoader::new(tmp)
    }

    #[tokio::test]
    async fn load_nonexistent_returns_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let result = loader
            .load_file(&RoleId::default(), MemoryFileKind::Soul)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_then_load() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::default();

        loader
            .save_file(&role, MemoryFileKind::Soul, "# Persona\nFriendly")
            .await
            .unwrap();
        let content = loader.load_file(&role, MemoryFileKind::Soul).await.unwrap();
        assert_eq!(content.as_deref(), Some("# Persona\nFriendly"));
    }

    #[tokio::test]
    async fn save_overwrites() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::default();

        loader
            .save_file(&role, MemoryFileKind::Agents, "v1")
            .await
            .unwrap();
        loader
            .save_file(&role, MemoryFileKind::Agents, "v2")
            .await
            .unwrap();
        let content = loader
            .load_file(&role, MemoryFileKind::Agents)
            .await
            .unwrap();
        assert_eq!(content.as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn load_snapshot_all_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let snap = loader.load_snapshot(&RoleId::default()).await.unwrap();
        assert_eq!(snap.files.len(), 8);
        assert!(snap.files.values().all(|v| v.is_none()));
    }

    #[tokio::test]
    async fn load_snapshot_partial() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::default();

        loader
            .save_file(&role, MemoryFileKind::Soul, "persona")
            .await
            .unwrap();
        loader
            .save_file(&role, MemoryFileKind::User, "prefs")
            .await
            .unwrap();

        let snap = loader.load_snapshot(&role).await.unwrap();
        assert_eq!(
            snap.files[&MemoryFileKind::Soul].as_deref(),
            Some("persona")
        );
        assert_eq!(snap.files[&MemoryFileKind::User].as_deref(), Some("prefs"));
        assert!(snap.files[&MemoryFileKind::Agents].is_none());
        assert!(snap.files[&MemoryFileKind::Bootstrap].is_none());
    }

    #[tokio::test]
    async fn save_creates_parent_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::new("newrole").unwrap();

        loader
            .save_file(&role, MemoryFileKind::Identity, "self")
            .await
            .unwrap();
        assert!(tmp.path().join("roles/newrole/IDENTITY.md").exists());
    }

    // ─── LongTerm via MemoryFileLoader ──────────────────────────────────────

    #[tokio::test]
    async fn long_term_load_nonexistent_returns_none() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let result = loader
            .load_file(&RoleId::default(), MemoryFileKind::LongTerm)
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn long_term_save_then_load() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::default();

        loader
            .save_file(
                &role,
                MemoryFileKind::LongTerm,
                "# Key Decisions\n- Use Rust",
            )
            .await
            .unwrap();
        let content = loader
            .load_file(&role, MemoryFileKind::LongTerm)
            .await
            .unwrap();
        assert_eq!(content.as_deref(), Some("# Key Decisions\n- Use Rust"));
    }

    #[tokio::test]
    async fn long_term_save_overwrites() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::default();

        loader
            .save_file(&role, MemoryFileKind::LongTerm, "v1")
            .await
            .unwrap();
        loader
            .save_file(&role, MemoryFileKind::LongTerm, "v2")
            .await
            .unwrap();
        let content = loader
            .load_file(&role, MemoryFileKind::LongTerm)
            .await
            .unwrap();
        assert_eq!(content.as_deref(), Some("v2"));
    }

    #[tokio::test]
    async fn long_term_save_creates_parent_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::new("newrole").unwrap();

        loader
            .save_file(&role, MemoryFileKind::LongTerm, "hello")
            .await
            .unwrap();
        assert!(tmp.path().join("roles/newrole/MEMORY.md").exists());
    }

    #[tokio::test]
    async fn snapshot_includes_long_term() {
        let tmp = tempfile::TempDir::new().unwrap();
        let loader = setup(tmp.path());
        let role = RoleId::default();

        loader
            .save_file(&role, MemoryFileKind::LongTerm, "knowledge")
            .await
            .unwrap();
        loader
            .save_file(&role, MemoryFileKind::Soul, "persona")
            .await
            .unwrap();

        let snap = loader.load_snapshot(&role).await.unwrap();
        assert_eq!(snap.files.len(), 8);
        assert_eq!(
            snap.files[&MemoryFileKind::LongTerm].as_deref(),
            Some("knowledge")
        );
        assert_eq!(
            snap.files[&MemoryFileKind::Soul].as_deref(),
            Some("persona")
        );
    }
}
