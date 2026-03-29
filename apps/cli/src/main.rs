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
        Commands::Chat { message, session } => {
            let ctx = setup::load_app_context().await?;

            match message {
                Some(msg) => run_oneshot(&ctx, &msg, session.as_deref()).await,
                None => run_interactive(&ctx, session.as_deref()).await,
            }
        }
    }
}

/// One-shot mode: send a single message and print the response.
async fn run_oneshot(
    ctx: &setup::AppContext,
    message: &str,
    session_scope: Option<&str>,
) -> Result<()> {
    let session_id = session_id_from_scope(session_scope);
    let input = UserInput {
        session_id,
        content: message.to_string(),
    };

    let resp = dispatch_provider!(ctx, |agent| {
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
async fn run_interactive(ctx: &setup::AppContext, session_scope: Option<&str>) -> Result<()> {
    let default_scope = format!("repl-{}", uuid::Uuid::new_v4());
    let scope = session_scope.unwrap_or(&default_scope);
    let session_id = SessionId::new(scope);

    dispatch_provider!(ctx, |agent| repl::run_repl(&agent, &session_id).await)
}

/// Derive a `SessionId` from an optional scope string.
fn session_id_from_scope(scope: Option<&str>) -> SessionId {
    match scope {
        Some(s) if !s.is_empty() => SessionId::new(s),
        _ => SessionId::new("cli-oneshot"),
    }
}
