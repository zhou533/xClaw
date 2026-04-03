//! Role lifecycle management.
//!
//! All role configs are stored in a single `{base_dir}/roles.yaml` file.
//! Role memory directories remain at `{base_dir}/roles/{name}/`.

use std::path::PathBuf;

use xclaw_core::types::RoleId;

use crate::error::MemoryError;
use crate::role::config::{RoleConfig, RolesFile, parse_roles_file, serialize_roles_file};
use crate::workspace::templates::ensure_bootstrap_templates;

/// Role lifecycle manager.
pub trait RoleManager: Send + Sync {
    /// Return the directory path for a given role.
    fn role_dir(&self, role: &RoleId) -> PathBuf;

    fn create_role(
        &self,
        config: RoleConfig,
    ) -> impl std::future::Future<Output = Result<(), MemoryError>> + Send;

    fn get_role(
        &self,
        role: &RoleId,
    ) -> impl std::future::Future<Output = Result<RoleConfig, MemoryError>> + Send;

    fn list_roles(
        &self,
    ) -> impl std::future::Future<Output = Result<Vec<RoleConfig>, MemoryError>> + Send;

    fn delete_role(
        &self,
        role: &RoleId,
    ) -> impl std::future::Future<Output = Result<(), MemoryError>> + Send;
}

/// Filesystem-backed role manager.
///
/// Stores all role configs in `{base_dir}/roles.yaml`.
/// Role memory directories live at `{base_dir}/roles/{name}/`.
pub struct FsRoleManager {
    base_dir: PathBuf,
}

impl FsRoleManager {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    fn roles_dir(&self) -> PathBuf {
        self.base_dir.join("roles")
    }

    fn roles_yaml_path(&self) -> PathBuf {
        self.base_dir.join("roles.yaml")
    }

    async fn load_roles_file(&self) -> Result<RolesFile, MemoryError> {
        let path = self.roles_yaml_path();
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => parse_roles_file(&content),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(RolesFile::new()),
            Err(e) => Err(MemoryError::Io(e)),
        }
    }

    async fn save_roles_file(&self, roles: &RolesFile) -> Result<(), MemoryError> {
        let path = self.roles_yaml_path();
        let yaml = serialize_roles_file(roles)?;

        // Atomic write: unique temp file then rename
        let parent = path.parent().unwrap_or(std::path::Path::new("."));
        let tmp = tempfile::NamedTempFile::new_in(parent).map_err(MemoryError::Io)?;
        tokio::fs::write(tmp.path(), &yaml).await?;
        tmp.persist(&path).map_err(|e| MemoryError::Io(e.error))?;
        Ok(())
    }
}

impl RoleManager for FsRoleManager {
    fn role_dir(&self, role: &RoleId) -> PathBuf {
        self.roles_dir().join(role.as_str())
    }

    async fn create_role(&self, mut config: RoleConfig) -> Result<(), MemoryError> {
        let role_id = RoleId::new(&config.name)
            .map_err(|_| MemoryError::InvalidRoleId(config.name.clone()))?;

        let mut roles = self.load_roles_file().await?;

        if roles.contains_key(role_id.as_str()) {
            return Err(MemoryError::RoleAlreadyExists(config.name.clone()));
        }

        // Set memory_dir default if empty
        if config.memory_dir.is_empty() {
            config.memory_dir = format!("roles/{}", role_id.as_str());
        }

        let role_dir = self.role_dir(&role_id);

        // Create role directory and memory subdirectory
        tokio::fs::create_dir_all(role_dir.join("memory")).await?;

        // Insert into roles map and persist
        roles.insert(role_id.as_str().to_string(), config.clone());
        self.save_roles_file(&roles).await?;

        // Seed bootstrap template files (idempotent; failures are only warnings)
        ensure_bootstrap_templates(&role_dir).await;

        tracing::info!(role = config.name, "created role");
        Ok(())
    }

    async fn get_role(&self, role: &RoleId) -> Result<RoleConfig, MemoryError> {
        let roles = self.load_roles_file().await?;
        roles
            .get(role.as_str())
            .cloned()
            .ok_or_else(|| MemoryError::RoleNotFound(role.to_string()))
    }

    async fn list_roles(&self) -> Result<Vec<RoleConfig>, MemoryError> {
        let roles = self.load_roles_file().await?;
        // BTreeMap is already sorted by key
        Ok(roles.into_values().collect())
    }

    async fn delete_role(&self, role: &RoleId) -> Result<(), MemoryError> {
        if role.as_str() == "default" {
            return Err(MemoryError::InvalidRoleId(
                "cannot delete the default role".to_string(),
            ));
        }

        let mut roles = self.load_roles_file().await?;

        if roles.remove(role.as_str()).is_none() {
            return Err(MemoryError::RoleNotFound(role.to_string()));
        }

        self.save_roles_file(&roles).await?;

        // Directory is intentionally preserved (memory files, daily memory, etc.)
        tracing::info!(role = role.as_str(), "deleted role from roles.yaml");
        Ok(())
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_manager(dir: &std::path::Path) -> FsRoleManager {
        FsRoleManager::new(dir)
    }

    #[tokio::test]
    async fn create_and_get_role() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        let config = RoleConfig {
            name: "secretary".to_string(),
            description: vec!["test".to_string()],
            system_prompt: "hello".to_string(),
            tools: vec!["shell".to_string()],
            meta: Default::default(),
            memory_dir: "roles/secretary".to_string(),
        };
        mgr.create_role(config).await.unwrap();

        let loaded = mgr
            .get_role(&RoleId::new("secretary").unwrap())
            .await
            .unwrap();
        assert_eq!(loaded.name, "secretary");
        assert_eq!(loaded.tools, vec!["shell"]);
        assert_eq!(loaded.memory_dir, "roles/secretary");

        // roles.yaml should exist at base_dir level
        assert!(tmp.path().join("roles.yaml").exists());
        // memory/ subdirectory should exist
        assert!(tmp.path().join("roles/secretary/memory").is_dir());
    }

    #[tokio::test]
    async fn create_duplicate_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        let config = RoleConfig {
            name: "dup".to_string(),
            ..RoleConfig::default_config()
        };
        mgr.create_role(config.clone()).await.unwrap();
        let result = mgr.create_role(config).await;
        assert!(matches!(result, Err(MemoryError::RoleAlreadyExists(_))));
    }

    #[tokio::test]
    async fn get_nonexistent_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());
        let result = mgr.get_role(&RoleId::new("ghost").unwrap()).await;
        assert!(matches!(result, Err(MemoryError::RoleNotFound(_))));
    }

    #[tokio::test]
    async fn list_roles_returns_sorted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        for name in ["coder", "admin", "secretary"] {
            let config = RoleConfig {
                name: name.to_string(),
                memory_dir: format!("roles/{name}"),
                ..RoleConfig::default_config()
            };
            mgr.create_role(config).await.unwrap();
        }

        let roles = mgr.list_roles().await.unwrap();
        let names: Vec<&str> = roles.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["admin", "coder", "secretary"]);
    }

    #[tokio::test]
    async fn list_empty_returns_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());
        let roles = mgr.list_roles().await.unwrap();
        assert!(roles.is_empty());
    }

    #[tokio::test]
    async fn delete_role_preserves_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        let config = RoleConfig {
            name: "temp".to_string(),
            memory_dir: "roles/temp".to_string(),
            ..RoleConfig::default_config()
        };
        mgr.create_role(config).await.unwrap();
        assert!(tmp.path().join("roles/temp").is_dir());

        mgr.delete_role(&RoleId::new("temp").unwrap())
            .await
            .unwrap();

        // Role removed from roles.yaml
        let result = mgr.get_role(&RoleId::new("temp").unwrap()).await;
        assert!(matches!(result, Err(MemoryError::RoleNotFound(_))));

        // But directory is preserved
        assert!(tmp.path().join("roles/temp").is_dir());
    }

    #[tokio::test]
    async fn delete_default_is_forbidden() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.create_role(RoleConfig::default_config()).await.unwrap();

        let result = mgr.delete_role(&RoleId::default()).await;
        assert!(matches!(result, Err(MemoryError::InvalidRoleId(_))));
    }

    #[tokio::test]
    async fn delete_nonexistent_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());
        let result = mgr.delete_role(&RoleId::new("ghost").unwrap()).await;
        assert!(matches!(result, Err(MemoryError::RoleNotFound(_))));
    }

    #[tokio::test]
    async fn create_with_invalid_name_returns_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());
        let config = RoleConfig {
            name: "INVALID".to_string(),
            ..RoleConfig::default_config()
        };
        let result = mgr.create_role(config).await;
        assert!(matches!(result, Err(MemoryError::InvalidRoleId(_))));
    }

    #[tokio::test]
    async fn roles_yaml_persists_multiple_roles() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        mgr.create_role(RoleConfig::default_config()).await.unwrap();
        mgr.create_role(RoleConfig {
            name: "coder".to_string(),
            memory_dir: "roles/coder".to_string(),
            ..RoleConfig::default_config()
        })
        .await
        .unwrap();

        // Read roles.yaml directly and verify both entries
        let content = tokio::fs::read_to_string(tmp.path().join("roles.yaml"))
            .await
            .unwrap();
        let roles: crate::role::config::RolesFile = serde_yml::from_str(&content).unwrap();
        assert_eq!(roles.len(), 2);
        assert!(roles.contains_key("default"));
        assert!(roles.contains_key("coder"));
    }

    #[tokio::test]
    async fn create_role_sets_memory_dir_default() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        let config = RoleConfig {
            name: "writer".to_string(),
            memory_dir: String::new(), // empty — should be set by create_role
            ..RoleConfig::default_config()
        };
        mgr.create_role(config).await.unwrap();

        let loaded = mgr.get_role(&RoleId::new("writer").unwrap()).await.unwrap();
        assert_eq!(loaded.memory_dir, "roles/writer");
    }

    // ── Bootstrap template seeding ────────────────────────────────────────────

    #[tokio::test]
    async fn create_role_seeds_bootstrap_templates() {
        use crate::workspace::templates::bootstrap_template;
        use crate::workspace::types::MemoryFileKind;

        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        let config = RoleConfig {
            name: "writer".to_string(),
            memory_dir: "roles/writer".to_string(),
            ..RoleConfig::default_config()
        };
        mgr.create_role(config).await.unwrap();

        let role_dir = tmp.path().join("roles/writer");

        for kind in MemoryFileKind::all() {
            if bootstrap_template(*kind).is_none() {
                continue;
            }
            let path = role_dir.join(kind.filename());
            assert!(
                path.exists(),
                "expected bootstrap file {} to exist after create_role",
                kind.filename()
            );
        }
    }

    #[tokio::test]
    async fn create_role_does_not_create_heartbeat_template() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        let config = RoleConfig {
            name: "checker".to_string(),
            memory_dir: "roles/checker".to_string(),
            ..RoleConfig::default_config()
        };
        mgr.create_role(config).await.unwrap();

        let heartbeat = tmp.path().join("roles/checker/HEARTBEAT.md");
        assert!(
            !heartbeat.exists(),
            "HEARTBEAT.md must not be written (no template)"
        );
    }
}
