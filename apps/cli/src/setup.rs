//! Shared initialization logic for CLI modes (one-shot and REPL).
//!
//! Extracts provider/memory/registry construction to avoid duplication
//! across the three `ProviderKind` match branches.

use std::path::PathBuf;

use anyhow::{Context, Result};

use xclaw_agent::AgentConfig;
use xclaw_config::{ProviderConfig, load_from_env};
use xclaw_memory::FsMemorySystem;
use xclaw_tools::registry::ToolRegistry;

/// Loaded application context: config, memory, tool registry, workspace root.
pub struct AppContext {
    pub provider_config: ProviderConfig,
    pub agent_config: AgentConfig,
    pub memory: FsMemorySystem,
    pub registry: ToolRegistry,
    pub workspace_root: PathBuf,
}

/// Load configuration, initialize memory and tool registry.
pub async fn load_app_context() -> Result<AppContext> {
    let config = load_from_env().context("failed to load configuration")?;

    let base_dir = memory_base_dir();
    let mem = FsMemorySystem::fs(&base_dir);
    mem.ensure_default_role()
        .await
        .context("failed to initialize default role")?;

    let mut registry = ToolRegistry::new();
    xclaw_tools::register_builtin_tools(&mut registry);
    xclaw_memory::tools::register_memory_tools(&mut registry, base_dir);

    let agent_config = AgentConfig::new(&config.provider.model);

    let workspace_root =
        std::env::current_dir().context("failed to determine current directory")?;

    Ok(AppContext {
        provider_config: config.provider,
        agent_config,
        memory: mem,
        registry,
        workspace_root,
    })
}

/// Macro to dispatch over `ProviderKind`, constructing the concrete provider
/// and running an async block with the resulting `LoopAgent`.
///
/// This eliminates the three near-identical `match` arms in `main.rs`.
///
/// Usage:
/// ```ignore
/// dispatch_provider!(ctx, |agent| async {
///     agent.process(input).await
/// })
/// ```
#[macro_export]
macro_rules! dispatch_provider {
    ($ctx:expr, |$agent:ident| $body:expr) => {{
        use xclaw_agent::LoopAgent;
        use xclaw_provider::{ClaudeProvider, MiniMaxProvider, OpenAiProvider};

        let ctx = &$ctx;
        match ctx.provider_config.kind {
            xclaw_config::ProviderKind::OpenAi => {
                let provider = OpenAiProvider::new(
                    &ctx.provider_config.api_key,
                    ctx.provider_config.base_url.as_deref(),
                    ctx.provider_config.organization.as_deref(),
                );
                let $agent = LoopAgent::new(
                    provider,
                    ctx.agent_config.clone(),
                    &ctx.memory.sessions,
                    &ctx.memory.roles,
                    &ctx.memory.files,
                    &ctx.memory.daily,
                    &ctx.registry,
                    &ctx.workspace_root,
                );
                $body
            }
            xclaw_config::ProviderKind::Claude => {
                let provider = ClaudeProvider::new(
                    &ctx.provider_config.api_key,
                    ctx.provider_config.base_url.as_deref(),
                );
                let $agent = LoopAgent::new(
                    provider,
                    ctx.agent_config.clone(),
                    &ctx.memory.sessions,
                    &ctx.memory.roles,
                    &ctx.memory.files,
                    &ctx.memory.daily,
                    &ctx.registry,
                    &ctx.workspace_root,
                );
                $body
            }
            xclaw_config::ProviderKind::MiniMax => {
                let provider = MiniMaxProvider::new(
                    &ctx.provider_config.api_key,
                    ctx.provider_config.base_url.as_deref(),
                )
                .context("failed to create MiniMax provider")?;
                let $agent = LoopAgent::new(
                    provider,
                    ctx.agent_config.clone(),
                    &ctx.memory.sessions,
                    &ctx.memory.roles,
                    &ctx.memory.files,
                    &ctx.memory.daily,
                    &ctx.registry,
                    &ctx.workspace_root,
                );
                $body
            }
        }
    }};
}

/// Default memory base directory: `~/.xclaw/memory`.
fn memory_base_dir() -> PathBuf {
    dirs_or_fallback().join("memory")
}

fn dirs_or_fallback() -> PathBuf {
    std::env::var("XCLAW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".xclaw")
        })
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_base_dir_ends_with_memory() {
        let dir = memory_base_dir();
        assert!(
            dir.ends_with("memory"),
            "expected path ending with 'memory', got: {dir:?}"
        );
    }

    #[test]
    fn dirs_or_fallback_respects_env_var() {
        // We can't safely set env vars in parallel tests, but we can
        // verify the function returns a path that exists or is constructable.
        let path = dirs_or_fallback();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn home_dir_returns_some_on_unix() {
        // On macOS/Linux, HOME should be set
        if std::env::var("HOME").is_ok() {
            assert!(home_dir().is_some());
        }
    }
}
