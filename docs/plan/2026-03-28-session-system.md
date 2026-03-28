# 实施计划：Session System (xclaw-memory) — Phase 1

> 日期：2026-03-28 | 状态：Approved | 复杂度：中-高

## 概览

在 `xclaw-memory` crate 中新增 Session 子系统，提供基于文件系统的会话索引（`sessions.json`）和转录持久化（JSONL）。包含 `SessionKey` 类型（`xclaw-core`）、数据结构、`SessionStore` trait 及 `FsSessionStore` 实现，并将 `MemorySystem` facade 从三泛型参数扩展为四泛型参数。

## 需求

### 功能性

- **F1** 根据 `SessionKey` 查找已有会话或创建新会话（`get_or_create`）
- **F2** 向指定 Session 追加对话记录（JSONL 格式）
- **F3** 按 Session 读取完整或尾部 N 条对话历史
- **F4** 每个 Role 目录下维护 `sessions/sessions.json` 索引
- **F5** 列出指定 Role 下的所有会话
- **F6** 暴露 `SessionSummary` 供 Agent 层 Memory Extraction 使用
- **F7** 解析和验证 `SessionKey`（`{role_id}:{scope}`）
- **F8** 删除指定 Session（索引移除 + JSONL 文件删除）
- **F9** 扩展 `MemoryError` 新增 5 个错误变体

### 非功能性

- **NF1** 文件优先：JSON/JSONL 存储，人类可读
- **NF2** 不可变模式：write 操作返回新实例
- **NF3** 索引写入原子性（tempfile + persist）
- **NF4** `impl Future` 风格（非 dyn-safe）
- **NF5** TDD：先写测试，80%+ 覆盖率

## 架构变更

| 变更 | 文件路径 | 说明 |
|------|----------|------|
| 新增类型 | `crates/xclaw-core/src/types.rs` | 新增 `SessionKey` struct + parse/Display/getters |
| 新增模块 | `crates/xclaw-memory/src/session/mod.rs` | Session 子模块入口 + re-exports |
| 新增类型 | `crates/xclaw-memory/src/session/types.rs` | `SessionEntry`, `SessionIndex`, `TranscriptRecord`, `SessionSummary` |
| 新增 trait | `crates/xclaw-memory/src/session/store.rs` | `SessionStore` trait（9 个方法） |
| 新增实现 | `crates/xclaw-memory/src/session/fs_store.rs` | `FsSessionStore` 文件系统实现 |
| 修改 | `crates/xclaw-memory/src/error.rs` | 新增 5 个错误变体 |
| 修改 | `crates/xclaw-memory/src/facade.rs` | 泛型参数 `<R, F, D>` → `<R, F, D, S>` |
| 修改 | `crates/xclaw-memory/src/lib.rs` | 新增 `pub mod session` + re-exports |
| 修改 | `Cargo.toml`（workspace） | 新增 `uuid` workspace 依赖 |
| 修改 | `crates/xclaw-memory/Cargo.toml` | 新增 `uuid = { workspace = true }` |
| 新增测试 | `crates/xclaw-memory/tests/session_integration.rs` | Session 系统集成测试 |

## 实施步骤

### 阶段 1：依赖与错误基础（3 步）

**步骤 1** — 新增 uuid workspace 依赖

- 文件：`Cargo.toml`（workspace）
- 操作：在 `[workspace.dependencies]` 中添加 `uuid = { version = "1", features = ["v4"] }`
- 依赖：无
- 风险：低

**步骤 2** — 新增 uuid 到 xclaw-memory 依赖

- 文件：`crates/xclaw-memory/Cargo.toml`
- 操作：在 `[dependencies]` 中添加 `uuid = { workspace = true }`
- 依赖：步骤 1
- 风险：低

**步骤 3** — 扩展 MemoryError

- 文件：`crates/xclaw-memory/src/error.rs`
- 操作：新增 5 个变体到 `MemoryError` 枚举
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
- 操作：为每个新变体添加单元测试（display 格式 + XClawError 转换）
- 依赖：无
- 风险：低 — 纯追加，不修改现有变体

### 阶段 2：核心类型（2 步）

**步骤 4** — 新增 SessionKey 到 xclaw-core

- 文件：`crates/xclaw-core/src/types.rs`
- 操作：
  - 定义 `SessionKey` struct（`role_id: RoleId`, `scope: String`），derive `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`
  - 实现 `SessionKey::new(role_id: RoleId, scope: impl Into<String>) -> Result<Self, XClawError>`：验证 scope 非空
  - 实现 `SessionKey::parse(raw: &str) -> Result<Self, XClawError>`：以第一个 `:` 分隔，role_id 部分经 `RoleId::new()` 验证，scope 部分非空
  - 实现 getter：`role_id(&self) -> &RoleId`、`scope(&self) -> &str`
  - 实现 `Display` for `SessionKey`：格式 `{role_id}:{scope}`
- TDD 测试用例（9 个）：
  - `session_key_parse_valid`
  - `session_key_parse_multiple_colons`：scope 含冒号
  - `session_key_parse_no_colon`：返回错误
  - `session_key_parse_empty_scope`：返回错误
  - `session_key_parse_invalid_role_id`：返回错误
  - `session_key_display_roundtrip`
  - `session_key_new_valid`
  - `session_key_new_empty_scope`：返回错误
  - `session_key_serde_roundtrip`
- 依赖：无（`RoleId` 和 `XClawError` 已存在）
- 风险：低

**步骤 5** — 新增 Session 数据类型

- 文件：`crates/xclaw-memory/src/session/types.rs`
- 操作：定义 4 个 struct，全部 derive `Debug, Clone, Serialize, Deserialize`
  - `SessionEntry`：`session_id`, `session_key`, `transcript_path`, `updated_at`, `created_at`
  - `SessionIndex`：`version`, `sessions` + `fn empty() -> Self`
  - `TranscriptRecord`：`role`, `content`, `timestamp`, `tool_call_id`(opt), `tool_name`(opt), `metadata`(opt)
  - `SessionSummary`：`session_id`, `session_key`, `message_count`, `first_message_at`(opt), `last_message_at`(opt)
- TDD 测试用例（6 个）：
  - `session_index_empty`
  - `session_entry_serde_roundtrip`
  - `transcript_record_serde_minimal`
  - `transcript_record_serde_full`
  - `transcript_record_skip_none_fields`
  - `session_summary_serde_roundtrip`
- 依赖：步骤 4（`SessionKey`）
- 风险：低

### 阶段 3：Trait 定义（2 步）

**步骤 6** — 新增 SessionStore trait

- 文件：`crates/xclaw-memory/src/session/store.rs`
- 操作：定义 `SessionStore` trait（`Send + Sync`），9 个方法全部使用 `impl Future` 风格
  - `get_or_create`, `get_by_id`, `get_by_key`, `list_sessions`
  - `append_transcript`, `load_transcript`, `load_transcript_tail`
  - `session_summary`, `delete_session`
- 在 trait doc comment 中注明并发契约
- 依赖：步骤 3（错误类型）、步骤 5（数据类型）
- 风险：低

**步骤 7** — 新增 session/mod.rs

- 文件：`crates/xclaw-memory/src/session/mod.rs`
- 操作：`pub mod` + re-exports
- 依赖：步骤 5、步骤 6
- 风险：低

### 阶段 4：FsSessionStore 实现（1 步，最复杂）

**步骤 8** — 实现 FsSessionStore

- 文件：`crates/xclaw-memory/src/session/fs_store.rs`
- 操作：
  - 定义 `FsSessionStore` struct，持有 `base_dir: PathBuf`
  - 实现私有辅助方法：
    - `sessions_dir(role)` → `{base_dir}/roles/{role}/sessions/`
    - `index_path(role)` → `{sessions_dir}/sessions.json`
    - `transcript_path(role, session_id)` → `{sessions_dir}/{session_id}.jsonl`
    - `read_index(role)` → 文件不存在返回 `SessionIndex::empty()`
    - `write_index(role, index)` → tempfile + persist 原子写入
    - `now_utc()` → ISO 8601 UTC 时间戳
  - 实现 `SessionStore for FsSessionStore`（9 个 async 方法）：
    - **`get_or_create`**：read_index → 查找匹配 → 未找到则 UUID v4 创建
    - **`get_by_id`** / **`get_by_key`**：read_index → 按条件查找
    - **`list_sessions`**：read_index → 返回 Vec
    - **`append_transcript`**：验证 session 存在 → append JSONL → 更新 updated_at
    - **`load_transcript`**：逐行解析 JSONL，末行失败 warn 跳过
    - **`load_transcript_tail`**：全量加载后取尾部 N 条（Phase 1 简单实现）
    - **`session_summary`**：load_transcript → 计算统计
    - **`delete_session`**：索引移除 + 删除 JSONL 文件
- TDD 测试用例（22 个）：
  - `new_creates_instance`
  - `sessions_dir_path_correct`, `index_path_correct`, `transcript_path_correct`
  - `read_index_returns_empty_when_no_file`
  - `write_and_read_index_roundtrip`, `write_index_is_atomic`
  - `get_or_create_new_session`, `get_or_create_existing_session`
  - `get_by_id_found`, `get_by_id_not_found`
  - `get_by_key_found`, `get_by_key_not_found`
  - `list_sessions_empty`, `list_sessions_multiple`
  - `append_and_load_transcript`, `load_transcript_empty_file`
  - `load_transcript_tolerates_corrupt_last_line`
  - `load_transcript_tail`
  - `session_summary_counts`
  - `delete_session_removes_entry_and_file`, `delete_session_not_found`
  - `append_to_nonexistent_session`
- 依赖：步骤 2（uuid）、步骤 3（错误）、步骤 5（类型）、步骤 6（trait）
- 风险：**高** — 原子写入、JSONL 容错、session 存在性验证

### 阶段 5：Facade 扩展与 Re-exports（2 步）

**步骤 9** — 扩展 MemorySystem facade

- 文件：`crates/xclaw-memory/src/facade.rs`
- 操作：
  - 泛型参数 `<R, F, D>` → `<R, F, D, S>`，新增 `S: SessionStore` 约束
  - 新增 `pub sessions: S` 字段
  - 更新 `FsMemorySystem` 类型别名包含 `FsSessionStore`
  - `FsMemorySystem::fs()` 新增 `sessions: FsSessionStore::new(&base_dir)`
- 更新现有单元测试
- 依赖：步骤 7、步骤 8
- 风险：**中** — 修改泛型签名，但已确认仅 xclaw-memory 内部使用

**步骤 10** — 更新 lib.rs re-exports

- 文件：`crates/xclaw-memory/src/lib.rs`
- 操作：添加 `pub mod session` + re-exports
- 依赖：步骤 7
- 风险：低

### 阶段 6：集成测试（2 步）

**步骤 11** — 修复现有集成测试

- 文件：`crates/xclaw-memory/tests/memory_system_integration.rs`
- 操作：如因 `FsMemorySystem` 类型变更编译失败，添加必要 import
- 依赖：步骤 9
- 风险：低

**步骤 12** — 新增 Session 集成测试

- 文件：`crates/xclaw-memory/tests/session_integration.rs`
- 测试用例（8 个）：
  - `session_create_and_retrieve_via_facade`
  - `session_transcript_append_and_load`
  - `session_summary_via_facade`
  - `session_delete_via_facade`
  - `session_index_file_exists`
  - `session_transcript_jsonl_file_readable`
  - `session_with_role_workflow`
  - `multiple_sessions_same_role`
- 依赖：步骤 9、步骤 10
- 风险：低

### 阶段 7：验证（1 步）

**步骤 13** — 运行全量测试与 lint

- 操作：`cargo test` + `cargo clippy -- -D warnings` + `cargo fmt --check`
- 依赖：步骤 1-12 全部完成
- 风险：低

## 风险与缓解

| 风险 | 级别 | 缓解措施 |
|------|------|----------|
| facade 泛型变更导致编译失败 | 中 | 已确认仅 xclaw-memory 内部使用；`FsMemorySystem::fs()` 接口不变 |
| 索引写入非原子导致文件损坏 | 高 | 强制 tempfile + persist；单元测试验证 |
| JSONL 末行部分写入导致 panic | 中 | 末行容错 warn 跳过；单元测试覆盖 |
| 时间戳格式不统一 | 低 | 抽取 `now_utc()` 辅助函数统一格式 |
| uuid 版本冲突 | 低 | uuid v1 稳定；workspace 统一管理 |
| append 到不存在的 session | 中 | 先 read_index 验证存在性 |

## 文件变更预估

| 文件 | 操作 | 预估行数 | 复杂度 |
|------|------|----------|--------|
| `Cargo.toml`（workspace） | 修改 | +1 | 低 |
| `crates/xclaw-memory/Cargo.toml` | 修改 | +1 | 低 |
| `crates/xclaw-core/src/types.rs` | 修改 | +100 | 低 |
| `crates/xclaw-memory/src/error.rs` | 修改 | +50 | 低 |
| `crates/xclaw-memory/src/session/types.rs` | 新增 | ~150 | 低 |
| `crates/xclaw-memory/src/session/store.rs` | 新增 | ~50 | 低 |
| `crates/xclaw-memory/src/session/mod.rs` | 新增 | ~15 | 低 |
| `crates/xclaw-memory/src/session/fs_store.rs` | 新增 | ~500 | **高** |
| `crates/xclaw-memory/src/facade.rs` | 修改 | ~20 | 中 |
| `crates/xclaw-memory/src/lib.rs` | 修改 | +3 | 低 |
| `crates/xclaw-memory/tests/session_integration.rs` | 新增 | ~200 | 中 |

**总计**：约 1100 行（含 ~600 行测试）

## 成功标准

- [ ] `SessionKey::parse` 正确解析 `{role_id}:{scope}` 格式，拒绝非法输入
- [ ] `FsSessionStore::get_or_create` 首次创建、重复返回已有
- [ ] `append_transcript` + `load_transcript` 完整读写 JSONL
- [ ] `load_transcript_tail(n)` 返回最后 n 条
- [ ] `session_summary` 返回正确统计
- [ ] `delete_session` 移除索引条目 + 删除 JSONL 文件
- [ ] 索引写入使用 tempfile + persist 原子操作
- [ ] JSONL 末行损坏不 panic，仅 warn 跳过
- [ ] `MemorySystem<R, F, D, S>` facade 可通过 `mem.sessions` 访问
- [ ] `cargo test` 全部通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] Session 新增代码测试覆盖率 >= 80%

## 相关文档

- 架构设计：[session-system.md](../architecture/session-system.md)
- ADR：[ADR-006-session-system.md](../architecture/adr/ADR-006-session-system.md)
- 变更日志：[2026-03-28-session-system.md](../architecture/changelog/2026-03-28-session-system.md)
