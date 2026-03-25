//! Multi-source configuration loading.

use std::env;

use xclaw_core::error::XClawError;

use crate::model::{AppConfig, ProviderConfig, ProviderKind};

/// Load application config from environment variables.
///
/// | Variable | Required | Default |
/// |---|---|---|
/// | `XCLAW_PROVIDER` | no | `"openai"` |
/// | `XCLAW_API_KEY` | **yes** | — |
/// | `XCLAW_MODEL` | no | per-provider default |
/// | `XCLAW_BASE_URL` | no | — |
/// | `XCLAW_ORGANIZATION` | no | — |
pub fn load_from_env() -> Result<AppConfig, XClawError> {
    let kind_str = env::var("XCLAW_PROVIDER").unwrap_or_else(|_| "openai".to_string());
    let kind: ProviderKind = kind_str
        .parse()
        .map_err(|e: String| XClawError::Config(e))?;

    let api_key = env::var("XCLAW_API_KEY").map_err(|_| {
        XClawError::Config("XCLAW_API_KEY environment variable is required".to_string())
    })?;

    if api_key.trim().is_empty() {
        return Err(XClawError::Config(
            "XCLAW_API_KEY must not be empty".to_string(),
        ));
    }

    let model = env::var("XCLAW_MODEL").unwrap_or_else(|_| kind.default_model().to_string());
    let base_url = env::var("XCLAW_BASE_URL").ok();
    let organization = env::var("XCLAW_ORGANIZATION").ok();

    Ok(AppConfig {
        provider: ProviderConfig {
            kind,
            api_key,
            base_url,
            model,
            organization,
        },
    })
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Environment variable tests must run serially because they mutate
    // shared process state. We use a mutex to prevent interleaving.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Helper: clear all XCLAW_ env vars, set the given ones, run `f`, then restore.
    fn with_env(vars: &[(&str, &str)], f: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap();
        let all_keys = [
            "XCLAW_PROVIDER",
            "XCLAW_API_KEY",
            "XCLAW_MODEL",
            "XCLAW_BASE_URL",
            "XCLAW_ORGANIZATION",
        ];
        // Save originals
        let originals: Vec<_> = all_keys.iter().map(|k| (*k, env::var(k).ok())).collect();
        // SAFETY: tests must run single-threaded (enforced via .cargo/config.toml
        // `test-threads = 1`) and the mutex prevents concurrent `with_env` calls.
        unsafe {
            // Clear all
            for key in &all_keys {
                env::remove_var(key);
            }
            // Set requested
            for (k, v) in vars {
                env::set_var(k, v);
            }
        }
        f();
        // SAFETY: same invariants as above — mutex held, single-threaded execution.
        unsafe {
            for (k, original) in &originals {
                match original {
                    Some(v) => env::set_var(k, v),
                    None => env::remove_var(k),
                }
            }
        }
    }

    // ── Missing API key ─────────────────────────────────────────────────

    #[test]
    fn errors_when_api_key_missing() {
        with_env(&[], || {
            let result = load_from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("XCLAW_API_KEY"), "error: {err}");
        });
    }

    #[test]
    fn errors_when_api_key_empty() {
        with_env(&[("XCLAW_API_KEY", "  ")], || {
            let result = load_from_env();
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("empty"), "error: {err}");
        });
    }

    // ── Defaults ────────────────────────────────────────────────────────

    #[test]
    fn defaults_to_openai_provider() {
        with_env(&[("XCLAW_API_KEY", "sk-test")], || {
            let config = load_from_env().unwrap();
            assert_eq!(config.provider.kind, ProviderKind::OpenAi);
        });
    }

    #[test]
    fn defaults_to_provider_default_model() {
        with_env(
            &[("XCLAW_API_KEY", "sk-test"), ("XCLAW_PROVIDER", "claude")],
            || {
                let config = load_from_env().unwrap();
                assert_eq!(config.provider.model, "claude-sonnet-4-5-20250929");
            },
        );
    }

    #[test]
    fn base_url_defaults_to_none() {
        with_env(&[("XCLAW_API_KEY", "sk-test")], || {
            let config = load_from_env().unwrap();
            assert!(config.provider.base_url.is_none());
        });
    }

    #[test]
    fn organization_defaults_to_none() {
        with_env(&[("XCLAW_API_KEY", "sk-test")], || {
            let config = load_from_env().unwrap();
            assert!(config.provider.organization.is_none());
        });
    }

    // ── Explicit values ─────────────────────────────────────────────────

    #[test]
    fn reads_all_env_vars() {
        with_env(
            &[
                ("XCLAW_PROVIDER", "minimax"),
                ("XCLAW_API_KEY", "mm-key-123"),
                ("XCLAW_MODEL", "MiniMax-Text-01"),
                ("XCLAW_BASE_URL", "https://custom.api.example.com"),
                ("XCLAW_ORGANIZATION", "org-abc"),
            ],
            || {
                let config = load_from_env().unwrap();
                assert_eq!(config.provider.kind, ProviderKind::MiniMax);
                assert_eq!(config.provider.api_key, "mm-key-123");
                assert_eq!(config.provider.model, "MiniMax-Text-01");
                assert_eq!(
                    config.provider.base_url,
                    Some("https://custom.api.example.com".to_string())
                );
                assert_eq!(config.provider.organization, Some("org-abc".to_string()));
            },
        );
    }

    // ── Invalid provider ────────────────────────────────────────────────

    #[test]
    fn errors_on_invalid_provider() {
        with_env(
            &[("XCLAW_API_KEY", "sk-test"), ("XCLAW_PROVIDER", "gemini")],
            || {
                let result = load_from_env();
                assert!(result.is_err());
                let err = result.unwrap_err().to_string();
                assert!(err.contains("unknown provider"), "error: {err}");
            },
        );
    }

    // ── Custom model overrides provider default ─────────────────────────

    #[test]
    fn custom_model_overrides_default() {
        with_env(
            &[
                ("XCLAW_API_KEY", "sk-test"),
                ("XCLAW_PROVIDER", "openai"),
                ("XCLAW_MODEL", "gpt-3.5-turbo"),
            ],
            || {
                let config = load_from_env().unwrap();
                assert_eq!(config.provider.model, "gpt-3.5-turbo");
            },
        );
    }
}
