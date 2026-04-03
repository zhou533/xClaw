# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

xClaw is a personal AI assistant platform built in Rust. It supports three runtime modes: CLI (one-shot chat), headless server (not yet implemented), and desktop (Tauri v2, not yet wired). The core is a Cargo workspace with pluggable LLM providers (OpenAI, Claude, MiniMax), a tool system, memory storage, and channel adapters.

## Build & Test Commands

```bash
cargo build                          # Build all workspace members
cargo test                           # Run all tests (single-threaded, see below)
cargo test -p xclaw-tools            # Test a single crate
cargo test test_name                 # Run tests matching a pattern
cargo test --test file_tools_integration  # Run a specific integration test
cargo clippy -- -D warnings          # Lint (warnings as errors)
cargo fmt --check                    # Check formatting
cargo fmt                            # Auto-format
```

**Single-threaded tests**: `.cargo/config.toml` enforces `test-threads = 1` because `xclaw-config` loader tests use `unsafe env::set_var`/`remove_var`. All `cargo test` runs are single-threaded by default.

### Running the CLI

```bash
XCLAW_API_KEY=sk-... cargo run -p xclaw-cli -- chat "hello"
```

Environment variables for config (`xclaw-config` loader):

| Variable | Required | Default |
|---|---|---|
| `XCLAW_PROVIDER` | no | `openai` |
| `XCLAW_API_KEY` | **yes** | — |
| `XCLAW_MODEL` | no | per-provider default (`gpt-4o`, `claude-sonnet-4-5-20250929`, `MiniMax-M2`) |
| `XCLAW_BASE_URL` | no | — |
| `XCLAW_ORGANIZATION` | no | — |

## Workspace Architecture

```
apps/
  cli/          — One-shot chat binary (clap CLI → SimpleAgent → provider)
  server/       — Headless server (stub)
  desktop/      — Tauri v2 app (not yet connected)

crates/
  xclaw-core/     — Shared error types (XClawError) and value types (SessionId)
  xclaw-config/   — Env-based config loading; NO business logic
  xclaw-agent/    — Agent loop + multi-agent orchestration (RoleOrchestrator)
  xclaw-provider/ — LlmProvider trait + OpenAI/Claude/MiniMax implementations
  xclaw-tools/    — Tool trait (dyn-safe via async_trait) + ToolRegistry + built-in tools (file_read/write/edit)
  xclaw-memory/   — Role management (RoleConfig, RoleManager) + MemoryStore + LongTermMemory + DailyMemory + WorkspaceMemory
                    All role configs stored in a single `roles.yaml` (YAML map keyed by role_id).
                    Role memory directories remain at `roles/{name}/`.
  xclaw-skill/    — Skill trait + loader/registry/executor (WASM sandbox planned)
  xclaw-channel/  — Channel adapters (Telegram, Discord, Slack, Webchat — stubs)
  xclaw-gateway/  — Axum HTTP/WS server modules
```

### Key Trait Locations & Dyn-Safety

Each trait lives in its **owning crate**, not in `xclaw-core`:

| Trait | Crate | Dyn-safe? |
|---|---|---|
| `LlmProvider` | `xclaw-provider::traits` | **No** — uses `impl Future` returns; use generics or concrete types |
| `AgentLoop` | `xclaw-agent::traits` | **No** — same reason |
| `RoleOrchestrator` | `xclaw-agent::orchestrator::traits` | **No** — same reason |
| `MemoryStore` | `xclaw-memory::traits` | **No** — same reason |
| `RoleManager` | `xclaw-memory::role::manager` | **No** — same reason |
| `LongTermMemory` | `xclaw-memory::role::long_term` | **No** — same reason |
| `DailyMemory` | `xclaw-memory::role::daily` | **No** — same reason |
| `WorkspaceMemoryLoader` | `xclaw-memory::workspace::loader` | **No** — same reason |
| `Tool` | `xclaw-tools::traits` | **Yes** — uses `#[async_trait]`, so `Box<dyn Tool>` works |
| `Skill` | `xclaw-skill::traits` | Check before assuming |

Because `LlmProvider` is not dyn-safe, the CLI dispatches via `match config.provider.kind` over concrete provider types rather than `Box<dyn LlmProvider>`.

### Error Strategy

- **Library crates**: typed errors via `thiserror` (e.g., `ProviderError`, `ToolError`)
- **Application crates** (cli, server): `anyhow` for flexible error context
- **Core error**: `XClawError` in `xclaw-core::error` — shared across crates for cross-module errors

### Tool System

Tools use `#[async_trait]` for dyn-safety. `ToolContext` enforces a filesystem allowlist (canonicalized paths). `ToolRegistry` holds `Box<dyn Tool>` instances. Register built-in tools via `xclaw_tools::register_builtin_tools()`. The `security` module validates paths against the allowlist.

## Architecture Principles

1. **Trait ownership**: traits belong in the crate that defines the domain concept, not in `xclaw-core`
2. **AIOS compatibility**: Role definitions align with the [AIOS](https://github.com/agiresearch/AIOS) agent config spec
3. **File-first persistence**: human-readable formats (Markdown, YAML) for user-facing data; SQLite for structured queries
4. **`xclaw-config` is config-only**: it manages program configuration, never business logic

## Rust Edition

The workspace uses **Rust edition 2024** (`edition = "2024"` in workspace Cargo.toml).
