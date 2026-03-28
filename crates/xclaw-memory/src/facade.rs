//! `MemorySystem` — single entry point for all memory subsystems.

use std::path::PathBuf;

use crate::error::MemoryError;
use crate::role::config::RoleConfig;
use crate::role::daily::{DailyMemory, FsDailyMemory};
use crate::role::manager::{FsRoleManager, RoleManager};
use crate::session::FsSessionStore;
use crate::session::store::SessionStore;
use crate::workspace::loader::{FsMemoryFileLoader, MemoryFileLoader};

/// Unified access to all memory subsystems.
///
/// Generic over trait implementations so tests can substitute stubs.
/// Use `MemorySystem::fs()` for the standard filesystem backend.
pub struct MemorySystem<R, F, D, S>
where
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
    S: SessionStore,
{
    pub roles: R,
    pub files: F,
    pub daily: D,
    pub sessions: S,
    base_dir: PathBuf,
}

/// Type alias for the filesystem-backed memory system.
pub type FsMemorySystem =
    MemorySystem<FsRoleManager, FsMemoryFileLoader, FsDailyMemory, FsSessionStore>;

impl FsMemorySystem {
    /// Build a filesystem-backed `MemorySystem`.
    ///
    /// Does **not** create the default role — call `ensure_default_role()` after.
    pub fn fs(base_dir: impl Into<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        Self {
            roles: FsRoleManager::new(&base_dir),
            files: FsMemoryFileLoader::new(&base_dir),
            daily: FsDailyMemory::new(&base_dir),
            sessions: FsSessionStore::new(&base_dir),
            base_dir,
        }
    }
}

impl<R, F, D, S> MemorySystem<R, F, D, S>
where
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
    S: SessionStore,
{
    /// Ensure the `default` role exists (idempotent).
    pub async fn ensure_default_role(&self) -> Result<(), MemoryError> {
        let default_id = xclaw_core::types::RoleId::default();
        match self.roles.get_role(&default_id).await {
            Ok(_) => Ok(()),
            Err(MemoryError::RoleNotFound(_)) => {
                self.roles.create_role(RoleConfig::default_config()).await
            }
            Err(e) => Err(e),
        }
    }

    /// The base directory for this memory system.
    pub fn base_dir(&self) -> &std::path::Path {
        &self.base_dir
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::types::MemoryFileKind;
    use xclaw_core::types::RoleId;

    #[tokio::test]
    async fn fs_creates_subsystems() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mem = FsMemorySystem::fs(tmp.path());
        assert_eq!(mem.base_dir(), tmp.path());
    }

    #[tokio::test]
    async fn ensure_default_role_creates_on_first_call() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mem = FsMemorySystem::fs(tmp.path());

        mem.ensure_default_role().await.unwrap();
        let cfg = mem.roles.get_role(&RoleId::default()).await.unwrap();
        assert_eq!(cfg.name, "default");
    }

    #[tokio::test]
    async fn ensure_default_role_idempotent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mem = FsMemorySystem::fs(tmp.path());

        mem.ensure_default_role().await.unwrap();
        mem.ensure_default_role().await.unwrap(); // should not error
        let cfg = mem.roles.get_role(&RoleId::default()).await.unwrap();
        assert_eq!(cfg.name, "default");
    }

    #[tokio::test]
    async fn facade_accesses_all_subsystems() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mem = FsMemorySystem::fs(tmp.path());
        let role = RoleId::default();

        mem.ensure_default_role().await.unwrap();

        // Long-term memory via files
        mem.files
            .save_file(&role, MemoryFileKind::LongTerm, "knowledge")
            .await
            .unwrap();
        let lt = mem
            .files
            .load_file(&role, MemoryFileKind::LongTerm)
            .await
            .unwrap();
        assert_eq!(lt.as_deref(), Some("knowledge"));

        // Daily memory
        mem.daily.append(&role, "note").await.unwrap();

        // Workspace files via files
        mem.files
            .save_file(&role, MemoryFileKind::Soul, "persona")
            .await
            .unwrap();
        let soul = mem
            .files
            .load_file(&role, MemoryFileKind::Soul)
            .await
            .unwrap();
        assert_eq!(soul.as_deref(), Some("persona"));
    }
}
