# 架构评审：xclaw-memory tools 内聚性

## 现状分析

### 架构概述

`xclaw-memory` crate 包含三组 LLM-callable tools，分布在三个文件中：

| 文件 | Tools | 底层 trait | 操作目标 |
|------|-------|-----------|---------|
| `tools/role_tools.rs` (267 行) | `role_create`, `role_list`, `role_get`, `role_delete` | `RoleManager` | `roles/{name}/role.yaml` |
| `tools/memory_tools.rs` (226 行) | `memory_read`, `memory_save`, `memory_append`, `memory_daily_read` | `LongTermMemory`, `DailyMemory` | `roles/{name}/MEMORY.md`, `roles/{name}/memory/YYYY-MM-DD.md` |
| `tools/workspace_tools.rs` (152 行) | `workspace_read`, `workspace_write` | `WorkspaceMemoryLoader` | `roles/{name}/SOUL.md`, `AGENTS.md` 等 7 种 |

所有 10 个 tools 共享相同的结构模式：

1. 持有 `base_dir: PathBuf`
2. 在 `execute` 中构造对应的 `Fs*` 实现（每次调用都 new 一个）
3. 解析 JSON params 中的 `role`（可选，默认 "default"）
4. 调用 trait 方法
5. 将 `MemoryError` 转换为 `ToolError`

### 磁盘布局

```
{base_dir}/
  roles/
    {role_name}/
      role.yaml          <- RoleManager
      MEMORY.md          <- LongTermMemory
      AGENTS.md          <- WorkspaceMemoryLoader
      SOUL.md            <- WorkspaceMemoryLoader
      TOOLS.md           <- WorkspaceMemoryLoader
      IDENTITY.md        <- WorkspaceMemoryLoader
      USER.md            <- WorkspaceMemoryLoader
      HEARTBEAT.md       <- WorkspaceMemoryLoader
      BOOTSTRAP.md       <- WorkspaceMemoryLoader
      memory/
        2026-03-25.md    <- DailyMemory
        2026-03-26.md    <- DailyMemory
```

### 技术债务

1. **每次 execute 都重新构造 Fs 实例**。概念上 tool 层绕过了 `MemorySystem` facade，直接耦合到具体的 `Fs*` 实现类型。
2. **MemorySystem facade 与 tools 层完全断裂**。facade 组合了 4 个子系统 trait，但 tools 层不使用 facade。
3. **重复的错误转换模式**。`.map_err(|e| ToolError::Internal(e.to_string()))` 在 10 个 tools 中出现至少 10 次。

### 可扩展性评估

当前设计在 10 个 tools 的规模下可以工作。如果要添加一个新的记忆类别（如 project memory、team memory），当前的 trait 拆分方式会导致需要再新增 trait + Fs 实现 + tool struct + tool 注册，整条链路重复。

## 发现

### 优势

1. **职责清晰的 trait 分层**。四个 trait 各管各的域概念，符合 trait ownership 原则。
2. **原子写入一致**。save 使用 `tempfile + persist`，append 使用 `OpenOptions::append(true)`。
3. **文件尺寸控制良好**。所有文件都在 150-270 行范围。
4. **单一注册入口**。`register_memory_tools()` 提供了清晰的批量注册。
5. **测试覆盖扎实**。每个源文件都有单元测试，tests/ 目录下有集成测试。

### 问题

#### HIGH: memory_tools 和 workspace_tools 的底层操作高度同构

| 操作 | memory_tools | workspace_tools |
|------|-------------|----------------|
| 读取 .md 文件 | `memory_read` 读 `MEMORY.md` | `workspace_read` 读 `SOUL.md` 等 |
| 覆盖 .md 文件 | `memory_save` 写 `MEMORY.md` | `workspace_write` 写 `SOUL.md` 等 |
| 追加 .md 文件 | `memory_append` 写 `memory/YYYY-MM-DD.md` | (无) |
| 按日期读取 | `memory_daily_read` | (无) |

从 LLM 的角度看，`MEMORY.md` 完全可以视为 `WorkspaceFileKind` 的又一个变体。

#### MEDIUM: 每次 execute 构造新的 Fs 实例，绕过 facade

如果将来切换后端，需要修改所有 10 个 tool 文件。facade 与 tools 层之间的一致性无法由编译器保证。

#### MEDIUM: MEMORY.md 在类型系统中的孤立地位

`MEMORY.md` 与 `SOUL.md`、`AGENTS.md` 在同一级目录下，格式相同，但由独立的 `LongTermMemory` trait 管理，成为类型系统中的孤儿。

#### LOW: 重复的 boilerplate

每个 tool struct 都有相同的 `base_dir: PathBuf` + `new(base_dir: &Path)` + `parse_role` + error mapping。

## 建议：统一 MemoryFileKind，合并 read/write tools

### 核心变更

**1. 扩展 WorkspaceFileKind -> MemoryFileKind**

```rust
pub enum MemoryFileKind {
    // 原有 workspace 文件
    Agents,
    Soul,
    Tools,
    Identity,
    User,
    Heartbeat,
    Bootstrap,
    // 新增
    LongTerm,  // MEMORY.md
}
```

**2. 合并 trait：WorkspaceMemoryLoader 吸收 LongTermMemory**

```rust
pub trait MemoryFileLoader: Send + Sync {
    fn load_file(&self, role: &RoleId, kind: MemoryFileKind)
        -> impl Future<Output = Result<Option<String>, MemoryError>> + Send;

    fn save_file(&self, role: &RoleId, kind: MemoryFileKind, content: &str)
        -> impl Future<Output = Result<(), MemoryError>> + Send;

    fn load_snapshot(&self, role: &RoleId)
        -> impl Future<Output = Result<MemorySnapshot, MemoryError>> + Send;
}
```

**3. 合并 tools：4 个 -> 2 个**

| 删除 | 替代为 |
|------|--------|
| `memory_read` | `memory_file_read(role, kind="long_term")` |
| `memory_save` | `memory_file_write(role, kind="long_term", content)` |
| `workspace_read` | `memory_file_read(role, kind="soul")` |
| `workspace_write` | `memory_file_write(role, kind="soul", content)` |

最终 tool 清单从 10 个变为 8 个：

| Tool | 说明 |
|------|------|
| `role_create` | 创建角色 |
| `role_list` | 列出角色 |
| `role_get` | 查看角色配置 |
| `role_delete` | 删除角色 |
| `memory_file_read` | 读取任意记忆文件（kind: long_term/soul/agents/...） |
| `memory_file_write` | 写入任意记忆文件 |
| `memory_daily_append` | 追加日常记忆 |
| `memory_daily_read` | 读取某日日常记忆 |

### 取舍分析

| 维度 | 优点 | 缺点 |
|------|------|------|
| 内聚性 | `MEMORY.md` 不再是类型系统中的孤儿 | 需要迁移现有 `LongTermMemory` 调用点 |
| LLM 认知负担 | 8 个 tools 比 10 个更容易理解 | `kind` 参数增加了一层间接 |
| 可扩展性 | 添加新文件类型只需在枚举加变体 | 枚举变更需重新编译 |
| 向后兼容 | 破坏性变更：tool 名称变了 | 早期开发阶段，影响可控 |
| 代码量 | 减少约 200 行重复代码 | 重构工作量约 1-2 小时 |

### 不推荐的方案

- **方案 B（保守）**：仅消除 tool 层 boilerplate，不合并 trait。不解决核心问题。
- **方案 C（激进）**：将所有 .md 视为 KV store。失去类型安全，append 语义无法表达。

## 结论

推荐方案 A。核心收敛路径：

1. `WorkspaceFileKind` -> `MemoryFileKind`，增加 `LongTerm` 变体
2. `WorkspaceMemoryLoader` -> `MemoryFileLoader`，吸收 `LongTermMemory` 的职责
3. 合并 4 个 read/write tools 为 2 个，tool 总数从 10 减至 8
4. `DailyMemory` trait 保持独立（语义不同：append-only + 按日期）
5. `RoleManager` trait 保持独立（管理 YAML 配置，非 Markdown 内容）

日期：2026-03-26
