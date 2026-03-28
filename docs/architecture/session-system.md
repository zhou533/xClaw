# 架构设计：Session System (xclaw-memory)

> 版本：1.0 | 日期：2026-03-28 | 状态：Approved

## 背景

xClaw 当前的 `xclaw-memory` crate 提供了 Role 管理（`RoleManager`）、工作区记忆（`MemoryFileLoader`）和日常记忆（`DailyMemory`），但缺少**会话（Session）管理**能力。现有的 `SessionId`（`xclaw-core::types`）仅是一个不透明的字符串包装类型，没有配套的持久化、索引和转录记录机制。

当前缺失：
1. 会话索引：没有机制跟踪哪些会话存在、属于哪个 Role
2. 转录记录：没有结构化的对话历史存储格式
3. sessionKey 语义：没有定义 `{roleId}:{scope}` 的解析和验证规则
4. Memory 桥接接口：Agent 层需要从 Session 提炼记忆，但 Session 层没有暴露相应数据访问接口

## 需求

### 功能性

- **F1** - 根据 sessionKey 查找已有会话或创建新会话
- **F2** - 向指定 Session 追加对话记录（JSONL 格式）
- **F3** - 按 Session 读取完整或部分（尾部 N 条）对话历史
- **F4** - 每个 Role 目录下维护 `sessions/sessions.json` 索引
- **F5** - 列出指定 Role 下的所有会话
- **F6** - 暴露转录数据的结构化访问接口，供 Agent 层进行 Memory Extraction/Injection
- **F7** - 解析和验证 sessionKey 的各组成部分
- **F8** - 扩展通过明确字段定义，新增 `Option<T>` 字段 + `#[serde(default)]` 保证向后兼容
- **F9** - 删除指定 Session（从索引和转录文件中清理）

### 非功能性

- **NF1** - 文件优先：JSON/JSONL 存储，人类可读可编辑
- **NF2** - 不可变模式：数据结构不可变，write 操作返回更新后的新实例
- **NF3** - 并发安全：Agent 层保证同 sessionKey 串行，Session 层索引写入原子性
- **NF4** - 性能：索引读取 O(n)，转录追加 O(1)
- **NF5** - `impl Future` 风格（非 dyn-safe），与项目其他 trait 一致

## 方案设计

### 1. 磁盘目录结构

```
~/.xclaw/roles/{role_name}/
├── role.yaml
├── memory/
├── MEMORY.md
├── SOUL.md
└── sessions/
    ├── sessions.json           # 会话索引
    ├── {sessionID}.jsonl       # 转录文件（每个会话一个）
    └── ...
```

### 2. 数据结构

#### SessionKey（`xclaw-core::types`）

结构化标识符，格式为 `{role_id}:{scope}`，以第一个 `:` 为分隔符。

`SessionKey` 是**外部语义标识符**，由 channel adapter 用于查找或创建会话。**内部存储标识符**是 `SessionId`（UUID），由 `SessionStore::get_or_create` 分配，用于文件名和后续操作。二者通过 `SessionEntry` 关联。

```rust
/// Structured session key: `{role_id}:{scope}`.
///
/// Examples:
///   - `assistant:whatsapp:direct:+1234567890`
///   - `support:telegram:group:-1001234567890`
///   - `coder:cli:local:default`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionKey {
    role_id: RoleId,
    scope: String,
}
```

- `role_id`：使用 `RoleId` 类型，复用其 snake_case 验证规则（`^[a-z][a-z0-9_]*$`）
- `scope`：冒号之后的全部内容，由 channel adapter 自由定义，不为空
- 提供 `parse(raw: &str) -> Result<Self, XClawError>` 方法，内部使用 `RoleId::new()` 验证 role_id 部分
- `role_id(&self) -> &RoleId`：getter
- `scope(&self) -> &str`：getter
- `Display` impl：`{role_id}:{scope}`

#### SessionEntry（索引条目，`xclaw-memory::session::types`）

```rust
/// A single entry in the session index (sessions.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    /// Unique identifier (UUID v4).
    pub session_id: SessionId,
    /// Structured session key: `{roleId}:{scope}`.
    pub session_key: SessionKey,
    /// Relative path to the transcript file: `{session_id}.jsonl`.
    pub transcript_path: String,
    /// ISO 8601 timestamp of last update (UTC).
    pub updated_at: String,
    /// ISO 8601 timestamp of creation (UTC).
    pub created_at: String,
}
```

#### SessionIndex（`sessions.json` 根对象）

```rust
/// The complete session index for a role, serialized as sessions.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionIndex {
    /// Schema version for forward compatibility.
    pub version: u32,
    /// All sessions for this role.
    pub sessions: Vec<SessionEntry>,
}
```

`version` 字段（初始值 `1`）支持未来索引格式升级时的迁移。

#### TranscriptRecord（JSONL 每行，`xclaw-memory::session::types`）

```rust
/// A single message in the transcript file (one JSON line).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptRecord {
    /// Message role: "user", "assistant", "system", "tool".
    pub role: String,
    /// Message content (text).
    pub content: String,
    /// ISO 8601 timestamp (UTC).
    pub timestamp: String,
    /// Optional: tool call ID for tool-role messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Optional: tool name for tool-role messages.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Extensible metadata (token count, model name, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}
```

JSONL 示例：

```jsonl
{"role":"user","content":"你好","timestamp":"2026-03-27T10:00:00Z"}
{"role":"assistant","content":"你好！有什么可以帮你的？","timestamp":"2026-03-27T10:00:01Z","metadata":{"model":"gpt-4o","tokens":15}}
```

#### SessionSummary（Memory 桥接用元数据）

```rust
/// Metadata about a session useful for memory operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub session_key: SessionKey,
    pub message_count: usize,
    pub first_message_at: Option<String>,
    pub last_message_at: Option<String>,
}
```

### 3. SessionStore Trait（`xclaw-memory::session::store`）

```rust
/// Session storage and transcript persistence.
///
/// Not dyn-safe (uses `impl Future`). Use generics or concrete types.
///
/// **Concurrency contract**: callers MUST serialize operations on the same
/// `SessionKey`. The store does NOT hold internal locks.
pub trait SessionStore: Send + Sync {
    fn get_or_create(&self, role: &RoleId, key: &SessionKey)
        -> impl Future<Output = Result<SessionEntry, MemoryError>> + Send;

    fn get_by_id(&self, role: &RoleId, session_id: &SessionId)
        -> impl Future<Output = Result<Option<SessionEntry>, MemoryError>> + Send;

    fn get_by_key(&self, role: &RoleId, key: &SessionKey)
        -> impl Future<Output = Result<Option<SessionEntry>, MemoryError>> + Send;

    fn list_sessions(&self, role: &RoleId)
        -> impl Future<Output = Result<Vec<SessionEntry>, MemoryError>> + Send;

    fn append_transcript(&self, role: &RoleId, session_id: &SessionId, record: &TranscriptRecord)
        -> impl Future<Output = Result<SessionEntry, MemoryError>> + Send;

    fn load_transcript(&self, role: &RoleId, session_id: &SessionId)
        -> impl Future<Output = Result<Vec<TranscriptRecord>, MemoryError>> + Send;

    fn load_transcript_tail(&self, role: &RoleId, session_id: &SessionId, n: usize)
        -> impl Future<Output = Result<Vec<TranscriptRecord>, MemoryError>> + Send;

    fn session_summary(&self, role: &RoleId, session_id: &SessionId)
        -> impl Future<Output = Result<SessionSummary, MemoryError>> + Send;

    fn delete_session(&self, role: &RoleId, session_id: &SessionId)
        -> impl Future<Output = Result<(), MemoryError>> + Send;
}
```

### 4. FsSessionStore 实现（`xclaw-memory::session::fs_store`）

- `sessions_dir(role)` → `{base_dir}/roles/{role}/sessions/`
- `index_path(role)` → `{sessions_dir}/sessions.json`
- `transcript_path(role, session_id)` → `{sessions_dir}/{session_id}.jsonl`

关键实现细节：
- **索引写入原子性**：使用 `tempfile::NamedTempFile::new_in(sessions_dir)` + `persist(index_path)`
- **转录追加**：`OpenOptions::new().create(true).append(true)` + 写入一行 JSON + `\n`
- **UUID 生成**：使用 `uuid` crate 的 `Uuid::new_v4()`，包装为 `SessionId`
- **时间戳**：ISO 8601 UTC 格式
- **JSONL 容错**（Phase 1 基础版）：解析时遇到末行 JSON 失败则跳过并 `tracing::warn!`
- **`delete_session`**：从索引中移除条目 + 删除 JSONL 文件（如果存在）

### 5. Memory 桥接

Session 层无需了解 Agent 如何使用数据，只需暴露：
- `load_transcript()` / `load_transcript_tail()` → 为 Memory Extraction 提供数据源
- `SessionSummary` → 让 Agent 决定提取策略
- 具体 Extraction/Injection 逻辑由 `xclaw-agent` 实现

### 6. 错误扩展

`MemoryError` 新增变体：

```rust
#[error("session not found: {0}")]
SessionNotFound(String),

#[error("invalid session key: {0}")]
InvalidSessionKey(String),

#[error("transcript parse error at line {line}: {message}")]
TranscriptParse { line: usize, message: String },

#[error("session index corrupted: {0}")]
IndexCorrupted(String),

#[error("JSON parse error: {0}")]
JsonParse(String),
```

### 7. 模块组织

```
crates/xclaw-memory/src/
├── session/
│   ├── mod.rs           # pub mod + re-exports
│   ├── types.rs         # SessionEntry, SessionIndex, TranscriptRecord, SessionSummary
│   ├── store.rs         # SessionStore trait
│   └── fs_store.rs      # FsSessionStore implementation
├── role/                # 不变
├── workspace/           # 不变
├── error.rs             # 新增 5 个变体
├── facade.rs            # 扩展为 <R, F, D, S>
├── traits.rs            # 不变
├── search.rs            # 不变
├── tools/               # 不变
└── lib.rs               # 新增 session mod + re-exports
```

### 8. MemorySystem Facade 扩展

```rust
pub struct MemorySystem<R, F, D, S>
where
    R: RoleManager,
    F: MemoryFileLoader,
    D: DailyMemory,
    S: SessionStore,
{
    pub roles: R,
    pub files: F,
    pub daily: D,
    pub sessions: S,
    base_dir: PathBuf,
}

pub type FsMemorySystem = MemorySystem<
    FsRoleManager,
    FsMemoryFileLoader,
    FsDailyMemory,
    FsSessionStore,
>;

impl FsMemorySystem {
    pub fn fs(base_dir: impl Into<PathBuf>) -> Self {
        let base_dir = base_dir.into();
        Self {
            roles: FsRoleManager::new(&base_dir),
            files: FsMemoryFileLoader::new(&base_dir),
            daily: FsDailyMemory::new(&base_dir),
            sessions: FsSessionStore::new(&base_dir),
            base_dir,
        }
    }
}
```

### 9. Workspace 依赖

`Cargo.toml`（workspace）新增：
```toml
uuid = { version = "1", features = ["v4"] }
```

`crates/xclaw-memory/Cargo.toml` 新增：
```toml
uuid = { workspace = true }
```

### 10. 待定事项

#### Session 注册为 Tool

是否将 Session 读写暴露为 LLM 可调用的 Tool，取决于后续 Agent 层设计。预留扩展点：在 `register_memory_tools()` 中添加条件注册。

#### SessionPolicy（Compaction 与 Skill 沉淀）

Phase 3 再引入，避免过早抽象：

```rust
/// Extension point for session lifecycle policies (Phase 3).
pub trait SessionPolicy: Send + Sync {
    fn needs_compaction(&self, summary: &SessionSummary)
        -> impl Future<Output = Result<bool, MemoryError>> + Send;

    fn compact(&self, role: &RoleId, session_id: &str)
        -> impl Future<Output = Result<(), MemoryError>> + Send;
}
```

## 取舍分析

| 决策点 | 方案 A | 方案 B | 选择 | 理由 |
|--------|--------|--------|------|------|
| 索引格式 | `sessions.json` | SQLite | A | 文件优先原则；会话数有限；人类可读 |
| 转录格式 | JSONL | SQLite / JSON 数组 | A | 追加 O(1)；流式友好；人类可读 |
| SessionKey 位置 | `xclaw-core::types` | `xclaw-memory` | A | 与 SessionId、RoleId 同级，跨 crate 共享 |
| Trait 风格 | `impl Future`（非 dyn-safe） | `#[async_trait]` | A | 与项目其他 trait 一致 |
| 扩展字段 | 明确字段定义 + `Option<T>` | `#[serde(flatten)] HashMap` | A | 类型安全、透明可审查；`#[serde(default)]` 保证向后兼容 |
| 索引写入 | tempfile + rename | 直接 overwrite | A | 防止中断导致损坏 |
| SessionKey.role_id 类型 | `RoleId` 类型 | 裸 `String` + 手动验证 | A | 复用验证逻辑，类型安全 |
| Phase 1 含 delete_session | 包含 | 延后至 Phase 2 | A | trait 接口一旦发布修改成本高，基础 CRUD 应完整 |
| TranscriptRecord metadata | 含 `Option<serde_json::Value>` | 不含，按需加字段 | A | JSONL 格式生产使用后逐字段迁移成本高 |

## 风险与缓解

| 风险 | 缓解措施 |
|------|----------|
| 并发写入索引冲突 | Agent 层保证同 sessionKey 串行；单用户场景概率极低 |
| JSONL 部分写入损坏 | 解析时末行 JSON 失败则跳过，记录 `tracing::warn!` |
| 大转录文件 | `load_transcript_tail()` + 未来 Compaction |
| 索引与 JSONL 不一致 | 文件不存在时返回空 Vec；Phase 2 提供 `repair_index()` |
| SessionKey scope 无约束 | 各 channel adapter 文档约定格式；SessionKey 仅验证非空 |
| SessionKey.role_id 与 RoleId 不一致 | 使用 `RoleId` 类型，parse 时复用 `RoleId::new()` 验证 |

## 实施阶段

### Phase 1：核心数据结构与持久化

1. 在 `xclaw-core::types` 中新增 `SessionKey`（parse、validate、Display、getters）
2. 在 `xclaw-memory::session::types` 中定义 `SessionEntry`、`SessionIndex`、`TranscriptRecord`（含 metadata）、`SessionSummary`
3. 在 `xclaw-memory::session::store` 中定义 `SessionStore` trait（含 `delete_session`）
4. 在 `xclaw-memory::session::fs_store` 中实现 `FsSessionStore`
5. 在 `xclaw-memory::error` 中新增 5 个错误变体（含 `JsonParse`）
6. 扩展 `MemorySystem` facade 为四泛型参数
7. 更新 `lib.rs` re-exports
8. workspace 新增 `uuid` 依赖
9. 单元测试 + 集成测试（TDD）

### Phase 2：健壮性与工具集成

1. JSONL 容错解析（跳过损坏行）
2. 索引修复（`repair_index`）
3. Session Tool 注册（如果确认需要）
4. `load_transcript_tail` 优化（倒序读取而非全量加载后截取）

### Phase 3：容量管理（待定）

1. `SessionPolicy` trait 定义与实现
2. Compaction 策略（LLM 摘要压缩）
3. Skill 沉淀管线（与 `xclaw-skill` 集成）
4. 归档策略（旧 Session 转移到冷存储目录）

## 日期

2026-03-28
