mod repl;
mod setup;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use xclaw_agent::{AgentLoop, UserInput};
use xclaw_core::types::SessionId;

#[derive(Parser)]
#[command(name = "xclaw", about = "xClaw AI assistant CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a message (one-shot) or start interactive mode (no message)
    Chat {
        /// The message to send (omit for interactive REPL)
        message: Option<String>,

        /// Resume a previous session by ID
        #[arg(long)]
        session: Option<String>,

        /// Print the assembled prompt to stderr before sending to LLM
        #[arg(long)]
        debug: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Chat {
            message,
            session,
            debug,
        } => {
            let ctx = setup::load_app_context().await?;

            match message {
                Some(msg) => run_oneshot(&ctx, &msg, session.as_deref(), debug).await,
                None => run_interactive(&ctx, session.as_deref(), debug).await,
            }
        }
    }
}

/// One-shot mode: send a single message and print the response.
async fn run_oneshot(
    ctx: &setup::AppContext,
    message: &str,
    session_scope: Option<&str>,
    debug: bool,
) -> Result<()> {
    let session_id = session_id_from_scope(session_scope);
    let input = UserInput {
        session_id,
        content: message.to_string(),
    };

    let agent_config = ctx.agent_config.clone().with_debug(debug);
    let resp = dispatch_provider!(ctx, agent_config, |agent| {
        agent
            .process(input)
            .await
            .context("agent failed to process message")
    })?;

    if resp.tool_calls_count > 0 {
        tracing::info!(tool_calls = resp.tool_calls_count, "tools executed");
    }

    println!("{}", resp.content);
    Ok(())
}

/// Interactive REPL mode: multi-turn conversation loop.
async fn run_interactive(
    ctx: &setup::AppContext,
    session_scope: Option<&str>,
    debug: bool,
) -> Result<()> {
    let session_id = session_id_from_scope(session_scope);
    let agent_config = ctx.agent_config.clone().with_debug(debug);
    dispatch_provider!(ctx, agent_config, |agent| repl::run_repl(
        &agent,
        &session_id
    )
    .await)
}

/// Derive a `SessionId` from an optional scope string.
///
/// When `scope` is `None` or empty, defaults to `"cli"` — the unified
/// scope for all CLI modes (one-shot and REPL).
fn session_id_from_scope(scope: Option<&str>) -> SessionId {
    match scope {
        Some(s) if !s.is_empty() => SessionId::new(s),
        _ => SessionId::new("cli"),
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── session_id_from_scope ───────────────────────────────────────────

    #[test]
    fn scope_none_defaults_to_cli() {
        let sid = session_id_from_scope(None);
        assert_eq!(sid.as_str(), "cli");
    }

    #[test]
    fn scope_empty_string_defaults_to_cli() {
        let sid = session_id_from_scope(Some(""));
        assert_eq!(sid.as_str(), "cli");
    }

    #[test]
    fn scope_custom_value_is_preserved() {
        let sid = session_id_from_scope(Some("my-session"));
        assert_eq!(sid.as_str(), "my-session");
    }
}
