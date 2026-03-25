use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use xclaw_agent::{AgentLoop, SimpleAgent, UserInput};
use xclaw_config::{ProviderKind, load_from_env};
use xclaw_core::types::SessionId;
use xclaw_provider::{ClaudeProvider, MiniMaxProvider, OpenAiProvider};

#[derive(Parser)]
#[command(name = "xclaw", about = "xClaw AI assistant CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Send a message and get a response
    Chat {
        /// The message to send
        message: String,
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
        Commands::Chat { message } => run_chat(&message).await,
    }
}

async fn run_chat(message: &str) -> Result<()> {
    let config = load_from_env().context("failed to load configuration")?;
    let input = UserInput {
        session_id: SessionId::new("cli-oneshot"),
        content: message.to_string(),
    };

    // LlmProvider uses `impl Future` return types (not dyn-safe),
    // so we must construct the concrete type per provider variant.
    let response = match config.provider.kind {
        ProviderKind::OpenAi => {
            let provider = OpenAiProvider::new(
                &config.provider.api_key,
                config.provider.base_url.as_deref(),
                config.provider.organization.as_deref(),
            );
            let agent = SimpleAgent::new(provider, &config.provider.model);
            agent.process(input).await
        }
        ProviderKind::Claude => {
            let provider = ClaudeProvider::new(
                &config.provider.api_key,
                config.provider.base_url.as_deref(),
            );
            let agent = SimpleAgent::new(provider, &config.provider.model);
            agent.process(input).await
        }
        ProviderKind::MiniMax => {
            let provider = MiniMaxProvider::new(
                &config.provider.api_key,
                config.provider.base_url.as_deref(),
            )
            .context("failed to create MiniMax provider")?;
            let agent = SimpleAgent::new(provider, &config.provider.model);
            agent.process(input).await
        }
    };

    let resp = response.context("agent failed to process message")?;
    println!("{}", resp.content);
    Ok(())
}
