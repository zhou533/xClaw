//! Interactive REPL loop for multi-turn conversations.
//!
//! Provides `run_repl` which reads user input in a loop, delegates to
//! the agent, and prints responses until the user exits.

use anyhow::Result;

use xclaw_agent::UserInput;
use xclaw_agent::traits::{AgentLoop, AgentResponse};
use xclaw_core::types::SessionId;

/// Commands that cause the REPL to exit.
const EXIT_COMMANDS: &[&str] = &["/exit", "/quit"];

/// Check whether `input` is an exit command.
pub fn is_exit_command(input: &str) -> bool {
    let trimmed = input.trim();
    EXIT_COMMANDS.contains(&trimmed)
}

/// Check whether `input` should be skipped (empty or whitespace-only).
pub fn is_skip_input(input: &str) -> bool {
    input.trim().is_empty()
}

/// Build a `UserInput` for the given session and content.
pub fn build_user_input(session_id: &SessionId, content: &str) -> UserInput {
    UserInput {
        session_id: session_id.clone(),
        content: content.trim().to_string(),
    }
}

/// Print the agent response to stdout.
pub fn print_response(resp: &AgentResponse) {
    if resp.tool_calls_count > 0 {
        eprintln!("[tools: {}]", resp.tool_calls_count);
    }
    println!("{}", resp.content);
}

/// Print the REPL welcome banner.
pub fn print_welcome() {
    eprintln!("xClaw interactive mode. Type /exit or /quit to leave, Ctrl+D for EOF.");
}

/// Run the interactive REPL loop.
///
/// Reads lines via `rustyline`, sends them to the agent, and prints
/// responses. Returns `Ok(())` on clean exit (EOF or exit command).
pub async fn run_repl<A: AgentLoop>(agent: &A, session_id: &SessionId) -> Result<()> {
    use rustyline::DefaultEditor;
    use rustyline::error::ReadlineError;

    print_welcome();

    let mut rl = DefaultEditor::new()?;

    loop {
        let readline = rl.readline("> ");
        match readline {
            Ok(line) => {
                if is_skip_input(&line) {
                    continue;
                }
                if is_exit_command(&line) {
                    eprintln!("Goodbye.");
                    break;
                }

                let _ = rl.add_history_entry(&line);
                let input = build_user_input(session_id, &line);

                match agent.process(input).await {
                    Ok(resp) => print_response(&resp),
                    Err(e) => eprintln!("Error: {e}"),
                }
            }
            Err(ReadlineError::Interrupted) => {
                // Ctrl+C: clear current line, continue
                eprintln!("[interrupted]");
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl+D: exit
                eprintln!("Goodbye.");
                break;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    Ok(())
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_exit_command ─────────────────────────────────────────────────

    #[test]
    fn exit_command_detects_exit() {
        assert!(is_exit_command("/exit"));
        assert!(is_exit_command("/quit"));
    }

    #[test]
    fn exit_command_trims_whitespace() {
        assert!(is_exit_command("  /exit  "));
        assert!(is_exit_command("\t/quit\n"));
    }

    #[test]
    fn exit_command_rejects_normal_input() {
        assert!(!is_exit_command("hello"));
        assert!(!is_exit_command("/help"));
        assert!(!is_exit_command("exit"));
        assert!(!is_exit_command(""));
    }

    // ── is_skip_input ───────────────────────────────────────────────────

    #[test]
    fn skip_empty_input() {
        assert!(is_skip_input(""));
        assert!(is_skip_input("   "));
        assert!(is_skip_input("\t\n"));
    }

    #[test]
    fn does_not_skip_non_empty() {
        assert!(!is_skip_input("hello"));
        assert!(!is_skip_input("  hi  "));
    }

    // ── build_user_input ────────────────────────────────────────────────

    #[test]
    fn builds_input_with_trimmed_content() {
        let sid = SessionId::new("repl-123");
        let input = build_user_input(&sid, "  hello world  ");
        assert_eq!(input.content, "hello world");
        assert_eq!(input.session_id.as_str(), "repl-123");
    }

    #[test]
    fn builds_input_preserves_session_id() {
        let sid = SessionId::new("s1");
        let i1 = build_user_input(&sid, "first");
        let i2 = build_user_input(&sid, "second");
        assert_eq!(i1.session_id.as_str(), i2.session_id.as_str());
    }
}
