//! Role lifecycle management.

use std::path::PathBuf;

use xclaw_core::types::RoleId;

use crate::error::MemoryError;
use crate::role::config::RoleConfig;

/// Role lifecycle manager.
pub trait RoleManager: Send + Sync {
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
/// Manages `{base_dir}/roles/{name}/` directories.
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

    fn role_dir(&self, role: &RoleId) -> PathBuf {
        self.roles_dir().join(role.as_str())
    }

    fn role_yaml_path(&self, role: &RoleId) -> PathBuf {
        self.role_dir(role).join("role.yaml")
    }
}

impl RoleManager for FsRoleManager {
    async fn create_role(&self, config: RoleConfig) -> Result<(), MemoryError> {
        let role_id = RoleId::new(&config.name)
            .map_err(|_| MemoryError::InvalidRoleId(config.name.clone()))?;
        let role_dir = self.role_dir(&role_id);

        if role_dir.exists() {
            return Err(MemoryError::RoleAlreadyExists(config.name.clone()));
        }

        // Create role directory and memory subdirectory
        tokio::fs::create_dir_all(role_dir.join("memory")).await?;

        // Write role.yaml
        let yaml = config.to_yaml()?;
        tokio::fs::write(self.role_yaml_path(&role_id), yaml).await?;

        tracing::info!(role = config.name, "created role");
        Ok(())
    }

    async fn get_role(&self, role: &RoleId) -> Result<RoleConfig, MemoryError> {
        let yaml_path = self.role_yaml_path(role);
        if !yaml_path.exists() {
            return Err(MemoryError::RoleNotFound(role.to_string()));
        }
        let content = tokio::fs::read_to_string(&yaml_path).await?;
        RoleConfig::from_yaml(&content)
    }

    async fn list_roles(&self) -> Result<Vec<RoleConfig>, MemoryError> {
        let roles_dir = self.roles_dir();
        if !roles_dir.exists() {
            return Ok(vec![]);
        }

        let mut configs = Vec::new();
        let mut entries = tokio::fs::read_dir(&roles_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_dir() {
                continue;
            }
            let yaml_path = entry.path().join("role.yaml");
            if yaml_path.exists() {
                let content = tokio::fs::read_to_string(&yaml_path).await?;
                match RoleConfig::from_yaml(&content) {
                    Ok(cfg) => configs.push(cfg),
                    Err(e) => tracing::warn!(
                        path = %yaml_path.display(),
                        error = %e,
                        "skipping role with unparseable role.yaml"
                    ),
                }
            }
        }
        configs.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(configs)
    }

    async fn delete_role(&self, role: &RoleId) -> Result<(), MemoryError> {
        if role.as_str() == "default" {
            return Err(MemoryError::InvalidRoleId(
                "cannot delete the default role".to_string(),
            ));
        }

        let role_dir = self.role_dir(role);
        if !role_dir.exists() {
            return Err(MemoryError::RoleNotFound(role.to_string()));
        }

        // Safety: verify the path is actually inside our roles directory
        let canonical = tokio::fs::canonicalize(&role_dir).await?;
        let roles_canonical = tokio::fs::canonicalize(self.roles_dir()).await?;
        if !canonical.starts_with(&roles_canonical) {
            return Err(MemoryError::InvalidRoleId(format!(
                "path escape detected: {}",
                role.as_str()
            )));
        }

        tokio::fs::remove_dir_all(&role_dir).await?;
        tracing::info!(role = role.as_str(), "deleted role");
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
        };
        mgr.create_role(config).await.unwrap();

        let loaded = mgr
            .get_role(&RoleId::new("secretary").unwrap())
            .await
            .unwrap();
        assert_eq!(loaded.name, "secretary");
        assert_eq!(loaded.tools, vec!["shell"]);

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
    async fn delete_role_removes_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        let config = RoleConfig {
            name: "temp".to_string(),
            ..RoleConfig::default_config()
        };
        mgr.create_role(config).await.unwrap();
        assert!(tmp.path().join("roles/temp").is_dir());

        mgr.delete_role(&RoleId::new("temp").unwrap())
            .await
            .unwrap();
        assert!(!tmp.path().join("roles/temp").exists());
    }

    #[tokio::test]
    async fn delete_default_is_forbidden() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mgr = test_manager(tmp.path());

        // Create default so directory exists
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
}
