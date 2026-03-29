# Agent Engine Implementation Plan

> Date: 2026-03-29
> Scope: xclaw-agent crate — full agent loop with tool dispatch, memory injection, session management
> Reference: ARCHITECTURE.md §4.2, rig framework (0xPlaygrounds/rig)

## Requirements

Based on ARCHITECTURE.md §4.2, implement the full Agent Loop engine:
- Core loop: message input → session load → memory injection → prompt build → LLM call → tool dispatch → result injection → memory persistence → response
- Skip Role routing (RoleRouter) and multi-Role orchestration (RoleOrchestrator) for now
- Session/Memory depends on xclaw-memory (FsMemorySystem)
- Agent Loop inspired by rig framework (message-driven, tool call loop, builder pattern)

## System Prompt Strategy

Prompt assembled from layers, injected into `ChatRequest.messages`:

| Layer | Source | Injection Point | Notes |
|-------|--------|-----------------|-------|
| **Base System Prompt** | `RoleConfig.system_prompt` | `Role::System` first | Role persona, e.g. "You are xClaw..." |
| **Persona (SOUL.md)** | `MemoryFileKind::Soul` | `Role::System` append | AI personality, tone, boundaries |
| **Guidelines (AGENTS.md)** | `MemoryFileKind::Agents` | `Role::System` append | Collaboration standards |
| **Tool Guidance (TOOLS.md)** | `MemoryFileKind::Tools` | `Role::System` append | Tool usage guide |
| **Long-term Memory (MEMORY.md)** | `MemoryFileKind::LongTerm` | `Role::System` append | Cross-session persistent memory |
| **Daily Memory** | `DailyMemory::load_day(today)` | `Role::System` append | Today's notes |
| **Conversation History** | `SessionStore::load_transcript_tail(N)` | `Role::User`/`Role::Assistant`/`Role::Tool` alternating | Last N transcript records |
| **Tool Definitions** | `ToolRegistry` registered tools | `ChatRequest.tools` | JSON Schema descriptions |
| **User Message** | Current user input | `Role::User` last | Current turn user message |

Each section injected only when the corresponding file exists; missing files are skipped.

Template (pseudocode):

```
{role_config.system_prompt}

## Persona
{soul_md_content}

## Guidelines
{agents_md_content}

## Tool Guidance
{tools_md_content}

## Long-term Memory
{memory_md_content}

## Today's Notes
{daily_memory_content}
```

## Implementation Phases

### Phase 0: Type Foundation — `AgentConfig` & `AgentContext`

**File**: `crates/xclaw-agent/src/config.rs` (new)

```rust
pub struct AgentConfig {
    pub model: String,
    pub max_tool_rounds: u32,        // loop protection, default 10
    pub transcript_tail_size: usize, // load last N history, default 20
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
}
```

```rust
pub struct AgentContext<'a, S: SessionStore, R: RoleManager, F: MemoryFileLoader, D: DailyMemory> {
    pub sessions: &'a S,
    pub roles: &'a R,
    pub files: &'a F,
    pub daily: &'a D,
    pub tool_registry: &'a ToolRegistry,
}
```

**Complexity**: Low

### Phase 1: Prompt Builder Refactor

**File**: `crates/xclaw-agent/src/prompt.rs` (rewrite)

1. **`SystemPromptBuilder`** — assemble system prompt layers
   - `with_role_config(config: &RoleConfig)` → base system prompt
   - `with_memory_snapshot(snapshot: &MemorySnapshot)` → SOUL/AGENTS/TOOLS/MEMORY
   - `with_daily_memory(content: Option<&str>)` → today's notes
   - `build() -> String` → concatenated full system prompt

2. **`ChatRequestBuilder`** — build full `ChatRequest`
   - `with_system_prompt(prompt: &str)`
   - `with_history(records: &[TranscriptRecord])` → convert to `Vec<Message>`
   - `with_user_message(content: &str)`
   - `with_tools(registry: &ToolRegistry)` → extract `Vec<ToolDefinition>`
   - `with_model(model: &str)`
   - `with_temperature(t: Option<f32>)`
   - `with_max_tokens(n: Option<u32>)`
   - `build() -> ChatRequest`

3. **`transcript_to_messages(records: &[TranscriptRecord]) -> Vec<Message>`** — convert JSONL records to provider Message types

**Complexity**: Medium

### Phase 2: Tool Dispatch Implementation

**File**: `crates/xclaw-agent/src/dispatch.rs` (fill)

```rust
pub struct ToolDispatcher<'a> {
    registry: &'a ToolRegistry,
}

impl<'a> ToolDispatcher<'a> {
    pub async fn execute_tool_calls(
        &self,
        tool_calls: &[ToolCall],
        context: &ToolContext,
    ) -> Vec<ToolCallResult>;
}

pub struct ToolCallResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub output: Result<String, String>,
}
```

- Iterate `ToolCall`, lookup and execute via `ToolRegistry`
- Convert `ToolOutput` to `ToolCallResult`
- Unknown tools return error result (no panic)
- tracing logs

**Complexity**: Medium

### Phase 3: Core Agent Loop — `LoopAgent`

**File**: `crates/xclaw-agent/src/engine.rs` (new)

```rust
pub struct LoopAgent<P, S, R, F, D>
where
    P: LlmProvider,
    S: SessionStore,
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
{
    provider: P,
    config: AgentConfig,
    context: AgentContext<S, R, F, D>,
}
```

**`process` implementation (core loop)**:

```
1. session = sessions.get_or_create(session_key)
2. role_config = roles.get_role("default")
3. snapshot = files.load_snapshot("default")
4. daily = daily.load_day("default", today)
5. history = sessions.load_transcript_tail(role, session_id, N)

6. system_prompt = SystemPromptBuilder::new()
       .with_role_config(&role_config)
       .with_memory_snapshot(&snapshot)
       .with_daily_memory(daily.as_deref())
       .build()

7. messages = transcript_to_messages(&history) + [user_message]
8. request = ChatRequestBuilder::new()
       .with_system_prompt(&system_prompt)
       .with_history_messages(messages)
       .with_tools(tool_definitions)
       .with_model(&config.model)
       .build()

9. loop (max_tool_rounds) {
     response = provider.chat(&request)
     choice = response.choices[0]

     if choice.finish_reason == ToolCalls {
       results = dispatcher.execute_tool_calls(&choice.tool_calls, &tool_ctx)
       // append assistant message + tool results to request.messages
       continue
     } else {
       break  // final text response
     }
   }

10. // Memory persistence (crash-safe: before reply)
    sessions.append_transcript(role, session_id, user_record)
    sessions.append_transcript(role, session_id, assistant_record)

11. return AgentResponse { content, tool_calls_count }
```

**Loop protection**: exceed `max_tool_rounds` → return current content or error.

**Complexity**: High

### Phase 4: Session Management Module

**File**: `crates/xclaw-agent/src/session.rs` (fill)

```rust
/// Derive SessionKey from UserInput
/// Hardcoded "default:cli" for now; dynamic after role routing
pub fn resolve_session_key(input: &UserInput) -> SessionKey;

/// Convert TranscriptRecord list to provider Message list
pub fn transcript_to_messages(records: &[TranscriptRecord]) -> Vec<Message>;

/// Build TranscriptRecord from assistant response
pub fn response_to_transcript(response: &ChatResponse) -> Vec<TranscriptRecord>;
```

**Complexity**: Low

### Phase 5: Exports & CLI Integration

**File**: `crates/xclaw-agent/src/lib.rs` (update)
- Export `LoopAgent`, `AgentConfig`, `AgentContext`, `ToolDispatcher`
- Retain `SimpleAgent` as lightweight fallback

**File**: `apps/cli/src/main.rs` (update)
- Build `FsMemorySystem`
- Build `ToolRegistry` + register builtin + memory tools
- Build `LoopAgent` replacing `SimpleAgent`
- Pass `AgentConfig` and `AgentContext`

**Complexity**: Medium

## Module Dependency Graph

```
AgentConfig ← (no deps)
     ↓
SystemPromptBuilder ← xclaw-memory (RoleConfig, MemorySnapshot, DailyMemory)
     ↓
ChatRequestBuilder ← xclaw-provider (ChatRequest, Message, ToolDefinition)
     ↓                xclaw-tools (ToolRegistry → tool definitions)
ToolDispatcher ← xclaw-tools (ToolRegistry, ToolContext, ToolOutput)
     ↓
LoopAgent ← all above + xclaw-memory (SessionStore) + xclaw-provider (LlmProvider)
     ↓
CLI main ← LoopAgent + xclaw-config + xclaw-memory (FsMemorySystem)
```

## Risk Assessment

| Risk | Level | Mitigation |
|------|-------|------------|
| ToolRegistry tool definition extraction API may be insufficient | Medium | Phase 2: verify ToolRegistry API first, extend if needed |
| TranscriptRecord ↔ Message conversion for tool calls serialization | Medium | Store tool_calls JSON in TranscriptRecord.metadata |
| Too many generic params → type signature explosion | Medium | Use FsMemorySystem type alias to simplify, or consider Arc<dyn Trait> wrappers |
| SessionKey generation hardcoded for now | Low | Extend after role routing phase |
| Memory persistence failure should not block reply | Low | Degrade persistence errors to tracing::warn, async retry later |

## Complexity Estimate

| Phase | Complexity | Files |
|-------|-----------|-------|
| Phase 0: AgentConfig/Context | Low | 1 new |
| Phase 1: Prompt Builder | Medium | 1 rewrite |
| Phase 2: Tool Dispatch | Medium | 1 fill |
| Phase 3: LoopAgent | High | 1 new |
| Phase 4: Session Management | Low | 1 fill |
| Phase 5: Exports & CLI | Medium | 2 update |

**Total**: ~7 file changes, all within `xclaw-agent` and `apps/cli`.
