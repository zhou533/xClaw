# CLI Debug Mode - Display Assembled Prompt

## Overview

Add `--debug` flag to CLI `chat` command. When enabled, display the fully assembled prompt (system prompt + memory context + history + user message + tool definitions) to stderr before sending to LLM, with colored output for readability.

## Requirements

- `chat` subcommand gains `--debug` flag
- REPL mode: each turn prints full prompt to stderr before LLM call
- One-shot mode: same behavior for consistency
- Colored output using raw ANSI escape codes (zero new dependencies)
- Tool loop subsequent rounds show one-line summary only

## Implementation Phases

### Phase 1: Config Layer (2 files)

1. **`AgentConfig` add `debug: bool`** — `crates/xclaw-agent/src/config.rs`
   - Add `pub debug: bool` field, default `false`, `#[serde(default)]`
   - Add `with_debug(self, enabled: bool) -> Self` builder method
   - Risk: low

2. **CLI add `--debug` flag** — `apps/cli/src/main.rs`
   - Add `#[arg(long)] debug: bool` to `Commands::Chat`
   - Pass to `run_oneshot` / `run_interactive`
   - Build `agent_config.with_debug(debug)` before dispatch
   - Risk: low

### Phase 2: Debug Formatting & Output (2 files)

3. **Create `debug_fmt.rs`** — `crates/xclaw-agent/src/debug_fmt.rs` (new file)
   - ANSI color constants (~10 lines): `CYAN`, `YELLOW`, `GREEN`, `BLUE`, `MAGENTA`, `DIM`, `RESET`
   - `pub fn format_request_debug(request: &ChatRequest) -> String`
   - Color scheme:

     | Section | Color |
     |---------|-------|
     | Title / separators | Cyan |
     | `[SYSTEM]` label | Yellow |
     | `[USER]` label | Green |
     | `[ASSISTANT]` label | Blue |
     | `[TOOL]` / `[TOOL_RESULT]` label | Magenta |
     | Metadata (model/temperature) | Dim |

   - Output format:
     ```
     ═══ DEBUG: Assembled Prompt ═══
     ── System ──
     {system_prompt_content}
     ── History ({n} messages) ──
     [User]: ...
     [Assistant]: ...
     ── Current User Message ──
     {user_message}
     ── Tools ({n} definitions) ──
     - tool_name_1: description
     - tool_name_2: description
     ── Model: {model} | Temperature: {t} | MaxTokens: {n} ──
     ═══════════════════════════════
     ```
   - Risk: low

4. **Engine conditional output** — `crates/xclaw-agent/src/engine.rs`
   - After `load_context_and_build_request` returns, before `provider.chat()` call
   - If `self.config.debug == true`: `eprintln!("{}", format_request_debug(&request))`
   - Risk: medium — ensure only first round outputs full prompt

### Phase 3: dispatch_provider Macro Adaptation (1 file)

5. **Macro accepts debug-enabled agent_config** — `apps/cli/src/main.rs` + `apps/cli/src/setup.rs`
   - Modify `run_oneshot` / `run_interactive` signatures to accept `debug: bool`
   - Build local `agent_config` with debug flag before `dispatch_provider!`
   - Adjust macro to use external config variable
   - Risk: medium

### Phase 4: Tool Loop Incremental Output (1 file)

6. **Subsequent rounds one-line summary** — `crates/xclaw-agent/src/engine.rs`
   - When debug=true and round > 1: `[debug] Round {n}: {tool_count} tool calls executed`
   - Use Dim color for summary lines
   - Risk: low

## Risks

| Risk | Level | Mitigation |
|------|-------|------------|
| Debug output exposes sensitive content | Medium | Only output prompt text to stderr, no raw JSON serialization |
| `dispatch_provider!` macro change breaks compilation | Medium | Minimal change, immediate `cargo check` |
| Tool loop output floods terminal | Low | Full output first round only, summary for subsequent |

## Testing

- **Unit**: `AgentConfig::with_debug` builder, `format_request_debug` output format for various `ChatRequest` shapes
- **Integration**: stub provider, verify stderr output when debug=true
- **E2E**: manual `cargo run -p xclaw-cli -- chat --debug`

## Success Criteria

- [ ] `cargo run -p xclaw-cli -- chat --debug` starts REPL normally
- [ ] Colored debug output appears on stderr before LLM response
- [ ] System prompt, history, user message, tools, metadata all shown with distinct colors
- [ ] Without `--debug`, behavior is unchanged
- [ ] One-shot `chat --debug "hello"` also outputs debug info
- [ ] All existing tests pass + new tests cover debug functionality
- [ ] `cargo clippy -- -D warnings` clean
