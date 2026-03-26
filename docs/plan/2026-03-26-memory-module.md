# 实施计划：xclaw-memory 角色管理与记忆持久化

> 日期：2026-03-26 | 状态：Approved

## 概览

在 `xclaw-memory` crate 中实现 Role 管理、长期记忆（MEMORY.md）、日常记忆（memory/YYYY-MM-DD.md）、工作区记忆文件（AGENTS.md / SOUL.md 等）的 trait 定义与文件系统实现，并注册为 Tool 供 LLM 调用。所有 trait 使用 `impl Future` 返回值（非 dyn-safe），与 `LlmProvider`、`AgentLoop` 等既有 trait 风格一致。SQLite 向量搜索仅预留 trait，不实现。

## 需求

- 定义 `RoleId` newtype 到 `xclaw-core`（snake_case 校验，内置 `default`）
- 定义 `RoleConfig` 结构体，AIOS 兼容，serde_yaml 解析 role.yaml
- 实现 `RoleManager` trait + `FsRoleManager`：CRUD 角色目录与 role.yaml
- 实现 `LongTermMemory` trait + `FsLongTermMemory`：读写 MEMORY.md
- 实现 `DailyMemory` trait + `FsDailyMemory`：append-only memory/YYYY-MM-DD.md
- 实现 `WorkspaceMemoryLoader` trait + `FsWorkspaceLoader`：读写工作区 .md 文件
- 预留 `MemorySearcher` trait（空 trait，无实现）
- 提供 `MemorySystem` 门面结构体，供 `xclaw-agent` 统一消费
- 注册 Memory Tools 到 `ToolRegistry`，供 LLM 通过 function calling 调用
- 保留现有 `MemoryStore` trait 不变
- 添加 `serde_yaml` workspace 依赖

## 架构变更

| 操作 | 文件 | 说明 |
|------|------|------|
| 修改 | `Cargo.toml`（workspace root） | 添加 `serde_yaml` workspace 依赖 |
| 修改 | `crates/xclaw-core/src/types.rs` | 添加 `RoleId` newtype |
| 修改 | `crates/xclaw-core/src/lib.rs` | re-export `RoleId` |
| 修改 | `crates/xclaw-memory/Cargo.toml` | 添加 `serde_yaml`、`xclaw-tools`、`tempfile`（dev）、`async-trait` |
| 重写 | `crates/xclaw-memory/src/lib.rs` | 新模块声明与 re-export |
| 保留 | `crates/xclaw-memory/src/traits.rs` | `MemoryStore` + `MemoryEntry` 不变 |
| 新增 | `crates/xclaw-memory/src/error.rs` | `MemoryError` 枚举（thiserror） |
| 新增 | `crates/xclaw-memory/src/role/mod.rs` | role 子模块聚合 |
| 新增 | `crates/xclaw-memory/src/role/config.rs` | `RoleConfig` + `RoleMeta` 结构体 |
| 新增 | `crates/xclaw-memory/src/role/manager.rs` | `RoleManager` trait + `FsRoleManager` |
| 新增 | `crates/xclaw-memory/src/role/long_term.rs` | `LongTermMemory` trait + `FsLongTermMemory` |
| 新增 | `crates/xclaw-memory/src/role/daily.rs` | `DailyMemory` trait + `FsDailyMemory` |
| 新增 | `crates/xclaw-memory/src/workspace/mod.rs` | workspace 子模块聚合 |
| 新增 | `crates/xclaw-memory/src/workspace/types.rs` | `WorkspaceFileKind` + `WorkspaceSnapshot` |
| 新增 | `crates/xclaw-memory/src/workspace/loader.rs` | `WorkspaceMemoryLoader` trait + `FsWorkspaceLoader` |
| 新增 | `crates/xclaw-memory/src/tools/mod.rs` | memory tools 模块聚合 + `register_memory_tools()` |
| 新增 | `crates/xclaw-memory/src/tools/role_tools.rs` | `RoleCreateTool`, `RoleListTool`, `RoleGetTool`, `RoleDeleteTool` |
| 新增 | `crates/xclaw-memory/src/tools/memory_tools.rs` | `MemoryReadTool`, `MemorySaveTool`, `MemoryAppendTool`, `MemoryDailyReadTool` |
| 新增 | `crates/xclaw-memory/src/tools/workspace_tools.rs` | `WorkspaceReadTool`, `WorkspaceWriteTool` |
| 重写 | `crates/xclaw-memory/src/search.rs` | `MemorySearcher` + `SearchResult`（仅 trait，无实现） |
| 新增 | `crates/xclaw-memory/src/facade.rs` | `MemorySystem` 门面：组合所有子系统 |
| 删除 | `crates/xclaw-memory/src/store.rs` | 已被 role/ 和 workspace/ 取代 |
| 删除 | `crates/xclaw-memory/src/sqlite.rs` | 本期不实现 SQLite 层 |

## 实施步骤

### 阶段 1：基础类型（3 个文件）

1. **添加 `RoleId` 到 xclaw-core**（文件：`crates/xclaw-core/src/types.rs`）
   - 新增 `RoleId` newtype（`pub struct RoleId(String)`），包含：
     - `new(id: impl Into<String>) -> Result<Self, XClawError>`：校验 snake_case（正则 `^[a-z][a-z0-9_]*$`）
     - `default() -> Self`：返回 `RoleId("default")`，实现 `Default` trait
     - `as_str() -> &str`
     - 派生 `Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize`
   - 依赖：无
   - 风险：低

2. **re-export RoleId**（文件：`crates/xclaw-core/src/lib.rs`）
   - 在已有 re-export 后添加 `pub use types::RoleId;`
   - 依赖：步骤 1
   - 风险：低

3. **创建 MemoryError 枚举**（文件：`crates/xclaw-memory/src/error.rs`）
   - 定义 `MemoryError`，变体包括：
     - `Io(#[from] std::io::Error)` — 文件系统操作失败
     - `YamlParse(String)` — role.yaml 解析失败
     - `RoleNotFound(String)` — 角色目录不存在
     - `RoleAlreadyExists(String)` — 创建时角色已存在
     - `InvalidRoleId(String)` — 非法角色名
     - `InvalidDate(String)` — 日期格式非 YYYY-MM-DD
   - 实现 `From<MemoryError> for XClawError`（映射到 `XClawError::Memory`）
   - 依赖：无
   - 风险：低

### 阶段 2：Role 配置与管理（3 个文件）

4. **RoleConfig 结构体**（文件：`crates/xclaw-memory/src/role/config.rs`）
   - 定义以下结构体，全部 `#[derive(Debug, Clone, Serialize, Deserialize)]`：
     ```rust
     pub struct RoleConfig {
         pub name: String,
         pub description: Vec<String>,
         #[serde(default)]
         pub system_prompt: String,
         #[serde(default)]
         pub tools: Vec<String>,
         #[serde(default)]
         pub meta: RoleMeta,
     }

     pub struct RoleMeta {
         #[serde(default = "default_author")]
         pub author: String,
         #[serde(default = "default_version")]
         pub version: String,
         #[serde(default = "default_license")]
         pub license: String,
         pub created_at: Option<String>,
     }
     ```
   - 提供 `RoleConfig::default_config() -> Self`：构建 `default` 角色配置
   - 提供 `RoleConfig::from_yaml(content: &str) -> Result<Self, MemoryError>`
   - 提供 `RoleConfig::to_yaml(&self) -> Result<String, MemoryError>`
   - 依赖：步骤 3
   - 风险：低

5. **RoleManager trait + FsRoleManager**（文件：`crates/xclaw-memory/src/role/manager.rs`）
   - 定义 `RoleManager` trait（4 个方法，`impl Future` 返回，非 dyn-safe）
   - 实现 `FsRoleManager { base_dir: PathBuf }`：
     - `new(base_dir: impl Into<PathBuf>) -> Self`
     - `create_role`：创建 `base_dir/roles/{name}/` 目录 + 写入 `role.yaml` + 创建 `memory/` 子目录
     - `get_role`：读取并解析 `roles/{name}/role.yaml`
     - `list_roles`：遍历 `roles/` 目录，读取每个子目录的 `role.yaml`
     - `delete_role`：删除整个 `roles/{name}/` 目录（禁止删除 `default`）
   - 路径拼接使用 `PathBuf::join`，不拼接字符串
   - 依赖：步骤 3、4
   - 风险：中 — `delete_role` 执行 `remove_dir_all`，需确保只删除目标目录

6. **role 模块聚合**（文件：`crates/xclaw-memory/src/role/mod.rs`）
   - 声明 `pub mod config;`、`pub mod manager;`、`pub mod long_term;`、`pub mod daily;`
   - re-export 关键类型
   - 依赖：步骤 4、5
   - 风险：低

### 阶段 3：记忆 trait 与实现（2 个文件）

7. **LongTermMemory trait + FsLongTermMemory**（文件：`crates/xclaw-memory/src/role/long_term.rs`）
   - 定义 `LongTermMemory` trait（`load`、`save`，`impl Future` 返回）
   - 实现 `FsLongTermMemory { base_dir: PathBuf }`：
     - `load`：读取 `roles/{role}/MEMORY.md`，文件不存在返回空字符串
     - `save`：覆盖写入 `roles/{role}/MEMORY.md`（先写 `.tmp` 再 rename，原子写入）
   - 依赖：步骤 3
   - 风险：低

8. **DailyMemory trait + FsDailyMemory**（文件：`crates/xclaw-memory/src/role/daily.rs`）
   - 定义 `DailyMemory` trait（`append`、`load_day`、`list_days`，`impl Future` 返回）
   - 实现 `FsDailyMemory { base_dir: PathBuf }`：
     - `append`：追加到 `roles/{role}/memory/{date}.md`（目录不存在则创建），每条 entry 前加 `\n` 分隔
     - `load_day`：读取对应日期文件，不存在返回空字符串
     - `list_days`：扫描 `memory/` 目录，匹配 `YYYY-MM-DD.md`，返回日期字符串 vec（排序）
   - `date` 参数校验：正则 `^\d{4}-\d{2}-\d{2}$`
   - 依赖：步骤 3
   - 风险：低

### 阶段 4：工作区记忆（3 个文件）

9. **WorkspaceFileKind + WorkspaceSnapshot**（文件：`crates/xclaw-memory/src/workspace/types.rs`）
   - `WorkspaceFileKind` 枚举：`Agents`, `Soul`, `Tools`, `Identity`, `User`, `Heartbeat`, `Bootstrap`
     - 派生 `Debug, Clone, Copy, PartialEq, Eq, Hash`
     - 实现 `fn filename(&self) -> &'static str`：映射到文件名（如 `Agents` -> `"AGENTS.md"`）
     - 实现 `fn all() -> &'static [WorkspaceFileKind]`：返回所有变体
   - `WorkspaceSnapshot { pub files: HashMap<WorkspaceFileKind, Option<String>> }`
     - 派生 `Debug, Clone`
   - 依赖：无
   - 风险：低

   **各文件语义**：

   | 文件 | 枚举值 | 语义 | 读取场景 | 写入场景 |
   |------|--------|------|----------|----------|
   | `AGENTS.md` | `Agents` | 协作护栏、代码风格 | 每次构建 Prompt | Agent 更新协作规范 |
   | `SOUL.md` | `Soul` | AI 人设与语调 | 每次构建 Prompt | Agent 调整人设 |
   | `TOOLS.md` | `Tools` | 额外工具说明 | 每次构建 Prompt | Agent 注册新工具说明 |
   | `IDENTITY.md` | `Identity` | AI 自我认同 | 每次构建 Prompt | Agent 更新自我认知 |
   | `USER.md` | `User` | 用户偏好画像 | 每次构建 Prompt | Agent 学到用户偏好 |
   | `HEARTBEAT.md` | `Heartbeat` | 心跳应对动作 | 心跳 tick / 会话初始化 | Agent 调整心跳行为 |
   | `BOOTSTRAP.md` | `Bootstrap` | 新工作区引导 | 新工作区首次会话 | Agent 更新引导流程 |

   所有文件均为可选——不存在则 `load_file` 返回 `Ok(None)`。

10. **WorkspaceMemoryLoader trait + FsWorkspaceLoader**（文件：`crates/xclaw-memory/src/workspace/loader.rs`）
    - 定义 `WorkspaceMemoryLoader` trait（`load_file`、`save_file`、`load_snapshot`，`impl Future` 返回）
    - 实现 `FsWorkspaceLoader { base_dir: PathBuf }`：
      - `load_file`：读取 `roles/{role}/{kind.filename()}`，不存在返回 `Ok(None)`
      - `save_file`：原子写入对应文件
      - `load_snapshot`：遍历 `WorkspaceFileKind::all()`，逐个 `load_file`，汇总为 `WorkspaceSnapshot`
    - 依赖：步骤 9
    - 风险：低

11. **workspace 模块聚合**（文件：`crates/xclaw-memory/src/workspace/mod.rs`）
    - 声明 `pub mod types;`、`pub mod loader;`，re-export 关键类型
    - 依赖：步骤 9、10
    - 风险：低

### 阶段 5：搜索预留 + 门面 + Tools（6 个文件）

12. **MemorySearcher 预留 trait**（文件：`crates/xclaw-memory/src/search.rs`，重写）
    - 定义 `SearchResult { pub content: String, pub score: f64, pub source: String }`
    - 定义 `MemorySearcher` trait（`search`、`index`，`impl Future` 返回）
    - 不提供任何实现
    - 依赖：无
    - 风险：低

13. **MemorySystem 门面**（文件：`crates/xclaw-memory/src/facade.rs`）
    - 定义泛型 `MemorySystem<R, L, D, W>` 结构体：
      ```rust
      pub struct MemorySystem<R, L, D, W>
      where
          R: RoleManager,
          L: LongTermMemory,
          D: DailyMemory,
          W: WorkspaceMemoryLoader,
      {
          pub roles: R,
          pub long_term: L,
          pub daily: D,
          pub workspace: W,
      }
      ```
    - 提供 `MemorySystem::fs(base_dir: impl Into<PathBuf>) -> MemorySystem<FsRoleManager, FsLongTermMemory, FsDailyMemory, FsWorkspaceLoader>`
    - 提供 `ensure_default_role(&self) -> Result<(), MemoryError>`：幂等创建 default 角色
    - 依赖：步骤 5、7、8、10
    - 风险：低

14. **Memory Tools — Role 管理**（文件：`crates/xclaw-memory/src/tools/role_tools.rs`）
    - 实现 4 个 Tool（`#[async_trait]` dyn-safe，与 file tools 一致）：
      - `RoleCreateTool { base_dir: PathBuf }` — 参数：`name`, `description`, `system_prompt`, `tools`
      - `RoleListTool { base_dir: PathBuf }` — 无参数，返回角色列表 JSON
      - `RoleGetTool { base_dir: PathBuf }` — 参数：`name`
      - `RoleDeleteTool { base_dir: PathBuf }` — 参数：`name`（禁止删除 default）
    - 每个 tool 内部构建 `FsRoleManager` 并调用对应 trait 方法
    - 依赖：步骤 5
    - 风险：低

15. **Memory Tools — 记忆读写**（文件：`crates/xclaw-memory/src/tools/memory_tools.rs`）
    - 实现 4 个 Tool：
      - `MemoryReadTool` — 参数：`role`（可选，默认 "default"），读取 MEMORY.md
      - `MemorySaveTool` — 参数：`role`（可选）、`content`，覆盖写入 MEMORY.md
      - `MemoryAppendTool` — 参数：`role`（可选）、`entry`、`date`（可选，默认今天），追加日常记忆
      - `MemoryDailyReadTool` — 参数：`role`（可选）、`date`，读取某天记忆
    - 依赖：步骤 7、8
    - 风险：低

16. **Memory Tools — 工作区文件**（文件：`crates/xclaw-memory/src/tools/workspace_tools.rs`）
    - 实现 2 个 Tool：
      - `WorkspaceReadTool` — 参数：`role`（可选）、`kind`（枚举字符串如 "soul"、"agents"）
      - `WorkspaceWriteTool` — 参数：`role`（可选）、`kind`、`content`
    - 依赖：步骤 10
    - 风险：低

17. **Tools 模块聚合 + 注册函数**（文件：`crates/xclaw-memory/src/tools/mod.rs`）
    - 声明子模块，提供统一注册入口：
      ```rust
      pub fn register_memory_tools(registry: &mut ToolRegistry, base_dir: PathBuf) {
          // Role tools
          registry.register(RoleCreateTool::new(&base_dir));
          registry.register(RoleListTool::new(&base_dir));
          registry.register(RoleGetTool::new(&base_dir));
          registry.register(RoleDeleteTool::new(&base_dir));
          // Memory tools
          registry.register(MemoryReadTool::new(&base_dir));
          registry.register(MemorySaveTool::new(&base_dir));
          registry.register(MemoryAppendTool::new(&base_dir));
          registry.register(MemoryDailyReadTool::new(&base_dir));
          // Workspace tools
          registry.register(WorkspaceReadTool::new(&base_dir));
          registry.register(WorkspaceWriteTool::new(&base_dir));
      }
      ```
    - 依赖：步骤 14、15、16
    - 风险：低

18. **更新 lib.rs 与 Cargo.toml**
    - `lib.rs`：声明所有模块，re-export 公共 API
    - `Cargo.toml`：添加 `serde_yaml`、`xclaw-tools`、`async-trait`；dev 添加 `tempfile`
    - 删除 `store.rs`、`sqlite.rs`
    - workspace `Cargo.toml`：添加 `serde_yaml = "0.9"`、`async-trait = "0.1"`
    - 依赖：步骤 1-17
    - 风险：低

### 阶段 6：集成测试（3 个文件）

19. **Role 管理集成测试**（文件：`crates/xclaw-memory/tests/role_manager_integration.rs`）
    - 使用 `tempfile::TempDir`，测试：
      - 创建角色 → 读取 role.yaml → 验证字段
      - list_roles 包含新创建角色
      - 删除角色 → 目录已移除
      - 禁止删除 default 角色
      - 创建已存在角色返回 RoleAlreadyExists
      - 获取不存在角色返回 RoleNotFound
    - 依赖：步骤 5

20. **记忆文件集成测试**（文件：`crates/xclaw-memory/tests/memory_files_integration.rs`）
    - 测试：
      - LongTermMemory：save 后 load 返回相同内容；load 不存在文件返回空
      - DailyMemory：append 多条 → load_day 全部可见；list_days 返回排序日期；无效日期报错
      - WorkspaceMemoryLoader：save_file + load_file 往返；load_snapshot 返回所有文件；不存在返回 None
    - 依赖：步骤 7、8、10

21. **MemorySystem 门面 + Tools 集成测试**（文件：`crates/xclaw-memory/tests/memory_system_integration.rs`）
    - 测试：
      - `MemorySystem::fs()` 自动创建 default 角色
      - `register_memory_tools()` 注册 10 个 tools
      - 通过 ToolRegistry 调用 `role_create` → 验证角色目录已生成
      - 通过 ToolRegistry 调用 `memory_append` → 验证文件已写入
    - 依赖：步骤 13、17

## 两类调用路径

| 路径 | 调用者 | 机制 | 举例 |
|------|--------|------|------|
| **Prompt 构建时读取** | Agent Rust 代码 | 直接调 trait 方法 | `load_snapshot()`, `long_term.load()` — 构建 prompt 前自动加载 |
| **运行时读写** | LLM 通过 tool call | `ToolRegistry` 派发 | LLM 决定"创建角色/记住这个" → 调用对应 tool |

## 完整 Tool 清单（10 个）

| 分类 | Tool 名称 | 说明 |
|------|-----------|------|
| Role 管理 | `role_create` | 创建新角色（目录 + role.yaml） |
| Role 管理 | `role_list` | 列出所有角色 |
| Role 管理 | `role_get` | 查看角色配置详情 |
| Role 管理 | `role_delete` | 删除角色（禁止删除 default） |
| 记忆读写 | `memory_read` | 读取长期记忆 MEMORY.md |
| 记忆读写 | `memory_save` | 覆盖写入长期记忆 |
| 记忆读写 | `memory_append` | 追加日常记忆 |
| 记忆读写 | `memory_daily_read` | 读取某天的日常记忆 |
| 工作区文件 | `workspace_read` | 读取工作区文件（SOUL/AGENTS/...） |
| 工作区文件 | `workspace_write` | 写入工作区文件 |

## Agent 消费方式

```rust
// 启动时构建
let base_dir = dirs::home_dir().unwrap().join(".xclaw");
let memory = MemorySystem::fs(&base_dir);

// 注册 tools（与 file tools 一起）
let mut registry = ToolRegistry::new();
xclaw_tools::register_builtin_tools(&mut registry);
xclaw_memory::tools::register_memory_tools(&mut registry, base_dir.clone());

// Prompt 构建时（Agent 代码直接调用 trait）
let long_term = memory.long_term.load(&role_id).await?;
let snapshot = memory.workspace.load_snapshot(&role_id).await?;
if let Some(soul) = &snapshot.files[&WorkspaceFileKind::Soul] {
    prompt.push_system(format!("## Persona\n{soul}"));
}

// LLM 运行时通过 tool call 写入（由 ToolRegistry 派发）
// 例：用户说"帮我创建一个 secretary 角色"
// LLM 调用 role_create tool → FsRoleManager 创建目录
// LLM 调用 workspace_write tool → 写入 SOUL.md

// Agent loop 中随时可由代码调用
memory.daily.append(&role_id, "用户提到偏好 dark mode").await?;
memory.long_term.save(&role_id, &refined_knowledge).await?;
```

## 依赖变更

| 范围 | 包 | 版本 | 用途 |
|------|-----|------|------|
| workspace 新增 | `serde_yaml` | `0.9` | 解析 role.yaml |
| workspace 新增 | `async-trait` | `0.1` | Memory tools 的 Tool trait 实现 |
| xclaw-memory 新增 | `serde_yaml` | workspace | role.yaml 读写 |
| xclaw-memory 新增 | `xclaw-tools` | workspace | Tool trait + ToolRegistry |
| xclaw-memory 新增 | `async-trait` | workspace | Tool 实现 |
| xclaw-memory dev 新增 | `tempfile` | `3` | 集成测试临时目录 |

## 测试策略

- **单元测试**：每个源文件内 `#[cfg(test)] mod tests`
  - `types.rs`：RoleId 校验（合法/非法名称、default）
  - `config.rs`：RoleConfig YAML 往返序列化
  - `workspace/types.rs`：WorkspaceFileKind filename 映射
  - `error.rs`：MemoryError -> XClawError 转换
  - `search.rs`：SearchResult 结构体构造
- **集成测试**：`tests/` 目录
  - `role_manager_integration.rs`：FsRoleManager CRUD 完整流程
  - `memory_files_integration.rs`：三种记忆文件读写
  - `memory_system_integration.rs`：门面 + Tools 注册与调用
- **覆盖率目标**：80%+

## 风险与缓解

- **`remove_dir_all` 误删**：校验目标路径必须在 `base_dir/roles/` 下；禁止删除 `default`；canonicalize 后比对前缀
- **并发写入**：原子写入（write-to-tmp + rename）；DailyMemory 使用 `OpenOptions::append(true)`；本期单实例
- **serde_yaml 边界情况**：字段全部 `#[serde(default)]`，宽容解析
- **文件系统权限**：所有 fs 操作用 `?` 传播 io::Error，MemoryError::Io 提供上下文

## 成功标准

- [ ] `RoleId` 在 xclaw-core 中定义，snake_case 校验通过
- [ ] `RoleConfig` 可正确解析/生成 AIOS 兼容的 role.yaml
- [ ] `FsRoleManager` 可创建、读取、列出、删除角色
- [ ] `FsLongTermMemory` 可覆盖写入/读取 MEMORY.md
- [ ] `FsDailyMemory` 可追加/按日期读取/列出日常记忆
- [ ] `FsWorkspaceLoader` 可读写所有 7 种工作区文件 + 加载快照
- [ ] `MemorySearcher` trait 已定义（无实现）
- [ ] `MemorySystem::fs()` 一站式构建且自动确保 default 角色存在
- [ ] 10 个 Memory Tools 注册到 ToolRegistry 并可通过 JSON 参数调用
- [ ] 现有 `MemoryStore` trait 和测试不受影响
- [ ] `cargo test -p xclaw-memory` 全部通过
- [ ] `cargo test -p xclaw-core` 全部通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] 测试覆盖率 80%+
