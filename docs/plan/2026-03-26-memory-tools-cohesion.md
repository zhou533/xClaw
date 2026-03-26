# 实施计划：xclaw-memory 统一 MemoryFileKind，合并 read/write tools

> 日期：2026-03-26 | 状态：Approved
> 依据：docs/architecture/changelog/2026-03-26-memory-tools-cohesion.md

## 概览

按照架构评审文档方案 A，将 `WorkspaceFileKind` 重命名为 `MemoryFileKind` 并增加 `LongTerm` 变体，合并 `WorkspaceMemoryLoader` 与 `LongTermMemory` 为新的 `MemoryFileLoader` trait，将 4 个 read/write tools 合并为 2 个（`memory_file_read` + `memory_file_write`），tool 总数从 10 减至 8。同时修复技术债务：tools 层绕过 facade 的问题和重复的错误转换模式。

## 需求

- `WorkspaceFileKind` 重命名为 `MemoryFileKind`，增加 `LongTerm` 变体（映射 `MEMORY.md`）
- `WorkspaceMemoryLoader` trait 吸收 `LongTermMemory` 职责，重命名为 `MemoryFileLoader`
- `WorkspaceSnapshot` 重命名为 `MemorySnapshot`，包含 8 种文件
- 删除 `memory_read`、`memory_save`、`workspace_read`、`workspace_write` 四个 tools
- 新增 `memory_file_read`、`memory_file_write` 两个 tools
- `DailyMemory` trait 保持独立（append-only + 按日期语义不同）
- `RoleManager` trait 保持独立（YAML 配置管理）
- 提取公共的 `MemoryError -> ToolError` 转换辅助函数，消除 boilerplate
- `MemorySystem` facade 更新：用 `MemoryFileLoader` 替代 `LongTermMemory + WorkspaceMemoryLoader`
- 所有现有测试迁移到新 API，覆盖率不低于现状

## 架构变更

| 操作 | 文件 | 说明 |
|------|------|------|
| 修改 | `crates/xclaw-memory/src/workspace/types.rs` | `WorkspaceFileKind` → `MemoryFileKind`，增加 `LongTerm` |
| 修改 | `crates/xclaw-memory/src/workspace/loader.rs` | `WorkspaceMemoryLoader` → `MemoryFileLoader`，`FsWorkspaceLoader` → `FsMemoryFileLoader` |
| 修改 | `crates/xclaw-memory/src/workspace/mod.rs` | 更新 re-exports |
| 删除 | `crates/xclaw-memory/src/role/long_term.rs` | 职责合并入 `MemoryFileLoader` |
| 修改 | `crates/xclaw-memory/src/role/mod.rs` | 移除 long_term 模块 |
| 修改 | `crates/xclaw-memory/src/facade.rs` | 泛型 `<R, L, D, W>` → `<R, F, D>` |
| 新增 | `crates/xclaw-memory/src/tools/memory_file_tools.rs` | `MemoryFileReadTool` + `MemoryFileWriteTool` |
| 修改 | `crates/xclaw-memory/src/tools/memory_tools.rs` | 仅保留 daily tools，重命名 |
| 删除 | `crates/xclaw-memory/src/tools/workspace_tools.rs` | 合并入 memory_file_tools.rs |
| 修改 | `crates/xclaw-memory/src/tools/mod.rs` | 更新注册 + 新增错误转换辅助函数 |
| 修改 | `crates/xclaw-memory/src/tools/role_tools.rs` | 使用错误转换辅助函数 |
| 修改 | `crates/xclaw-memory/src/lib.rs` | 更新 re-exports |
| 修改 | `crates/xclaw-memory/tests/*.rs` | 迁移到新 API |

## 实施步骤

### 阶段 1：类型层变更（无破坏性，纯新增）

1. **重命名 `WorkspaceFileKind` -> `MemoryFileKind` 并增加 `LongTerm` 变体**（文件：`crates/xclaw-memory/src/workspace/types.rs`）
   - 操作：将枚举名从 `WorkspaceFileKind` 改为 `MemoryFileKind`；增加 `LongTerm` 变体，`filename()` 返回 `"MEMORY.md"`；`all()` 返回 8 种；`from_str_name` 增加 `"long_term"` 匹配；将 `WorkspaceSnapshot` 重命名为 `MemorySnapshot`，HashMap 的 key 类型跟随变更
   - 原因：`MEMORY.md` 与其他 .md 文件同构，纳入统一枚举消除类型系统中的孤儿
   - 依赖：无
   - 风险：低 — 纯类型变更，编译器会捕获所有漏改点

2. **更新 workspace/mod.rs 的 re-exports**（文件：`crates/xclaw-memory/src/workspace/mod.rs`）
   - 操作：将 `WorkspaceFileKind`/`WorkspaceSnapshot` 的 re-export 改为 `MemoryFileKind`/`MemorySnapshot`
   - 依赖：步骤 1
   - 风险：低

### 阶段 2：trait 合并

3. **重命名 `WorkspaceMemoryLoader` -> `MemoryFileLoader` 并扩展实现**（文件：`crates/xclaw-memory/src/workspace/loader.rs`）
   - 操作：trait 名从 `WorkspaceMemoryLoader` 改为 `MemoryFileLoader`；struct 名从 `FsWorkspaceLoader` 改为 `FsMemoryFileLoader`；`file_path` 方法处理 `MemoryFileKind::LongTerm` 时返回 `roles/{name}/MEMORY.md`（与原 `FsLongTermMemory::memory_path` 逻辑一致）；`load_snapshot` 遍历 `MemoryFileKind::all()`（8 种）
   - 原因：吸收 `LongTermMemory` 的 load/save 职责，统一到一个 trait
   - 依赖：步骤 1
   - 风险：中 — `LongTermMemory::load` 返回空字符串而非 `None`，需要在调用点处理语义差异

4. **删除 `LongTermMemory` trait 和 `FsLongTermMemory`**（文件：`crates/xclaw-memory/src/role/long_term.rs`）
   - 操作：删除整个文件
   - 依赖：步骤 3
   - 风险：低 — 编译器会报告所有引用

5. **更新 `role/mod.rs`**（文件：`crates/xclaw-memory/src/role/mod.rs`）
   - 操作：移除 `pub mod long_term;` 及对应的 re-exports
   - 依赖：步骤 4
   - 风险：低

### 阶段 3：facade 简化

6. **更新 `MemorySystem` facade**（文件：`crates/xclaw-memory/src/facade.rs`）
   - 操作：泛型参数从 `<R, L, D, W>` 变为 `<R, F, D>`，其中 `F: MemoryFileLoader` 替代原先的 `L: LongTermMemory` + `W: WorkspaceMemoryLoader`；字段从 `long_term` + `workspace` 合并为 `files: F`；`FsMemorySystem` 类型别名更新为 `MemorySystem<FsRoleManager, FsMemoryFileLoader, FsDailyMemory>`；`FsMemorySystem::fs()` 构造器只创建 3 个子系统
   - 依赖：步骤 3, 5
   - 风险：中 — facade 是主入口，需确保所有调用点迁移

### 阶段 4：tools 层重构

7. **新增 `MemoryError -> ToolError` 辅助函数**（文件：`crates/xclaw-memory/src/tools/mod.rs`）
   - 操作：新增 `pub(crate) fn to_tool_error(e: MemoryError) -> ToolError`，处理语义映射（`RoleNotFound`/`InvalidRoleId`/`RoleAlreadyExists` → `ToolError::InvalidParams`，其他 → `ToolError::Internal`）
   - 原因：消除 10+ 处重复的 `.map_err(|e| ToolError::Internal(e.to_string()))` 模式
   - 依赖：无
   - 风险：低

8. **新增 `memory_file_tools.rs`**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
   - 操作：实现 `MemoryFileReadTool` 和 `MemoryFileWriteTool`，两者持有 `base_dir: PathBuf`；`kind` 参数接受 `"agents"`, `"soul"`, `"tools"`, `"identity"`, `"user"`, `"heartbeat"`, `"bootstrap"`, `"long_term"` 共 8 种；tool names 分别为 `"memory_file_read"` 和 `"memory_file_write"`；使用步骤 7 的辅助函数做错误转换
   - 依赖：步骤 3, 7
   - 风险：低

9. **重构 `memory_tools.rs`：仅保留 daily tools**（文件：`crates/xclaw-memory/src/tools/memory_tools.rs`）
   - 操作：删除 `MemoryReadTool` 和 `MemorySaveTool`；保留并重命名 `MemoryAppendTool` → `MemoryDailyAppendTool`（tool name `"memory_daily_append"`）和 `MemoryDailyReadTool`；使用错误转换辅助函数
   - 依赖：步骤 7, 8
   - 风险：低

10. **删除 `workspace_tools.rs`**（文件：`crates/xclaw-memory/src/tools/workspace_tools.rs`）
    - 操作：删除整个文件
    - 依赖：步骤 8
    - 风险：低

11. **更新 `tools/mod.rs` 注册逻辑**（文件：`crates/xclaw-memory/src/tools/mod.rs`）
    - 操作：新增 `pub mod memory_file_tools;`；移除 `pub mod workspace_tools;`；更新 re-exports 和 `register_memory_tools` 注册 8 个 tools
    - 依赖：步骤 8, 9, 10
    - 风险：低

12. **更新 `role_tools.rs` 使用错误转换辅助函数**（文件：`crates/xclaw-memory/src/tools/role_tools.rs`）
    - 操作：将 4 个 tool 中的 `.map_err(...)` 替换为 `to_tool_error`
    - 依赖：步骤 7
    - 风险：低

### 阶段 5：顶层 re-exports 清理

13. **更新 `lib.rs` re-exports**（文件：`crates/xclaw-memory/src/lib.rs`）
    - 操作：移除旧类型 re-exports，更新为新命名
    - 依赖：步骤 2, 5, 6, 11
    - 风险：低

### 阶段 6：集成测试迁移

14. **迁移 `tests/memory_files_integration.rs`**
    - `LongTermMemory` 测试改为 `MemoryFileLoader` + `MemoryFileKind::LongTerm`
    - 注意 `load` 返回值从 `String` 变为 `Option<String>`
    - 依赖：步骤 3, 13

15. **迁移 `tests/memory_system_integration.rs`**
    - facade 字段 `long_term`/`workspace` → `files`
    - tool 注册数 10 → 8，名称更新
    - 依赖：步骤 6, 11, 13

16. **`tests/role_manager_integration.rs` 无需变更**

### 阶段 7：内联单元测试更新

17. **`workspace/types.rs` 测试** — `all()` 断言 8 种，增加 `LongTerm` 测试用例
18. **`workspace/loader.rs` 测试** — `snapshot` 断言 8 种，增加 `LongTerm` save/load 测试
19. **`facade.rs` 测试** — 字段名和 API 更新

## 最终 Tool 清单（8 个）

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

## 风险与缓解

| 风险 | 级别 | 缓解 |
|------|------|------|
| `LongTermMemory::load` 返回 `String` vs `Option<String>` 语义差异 | 中 | 统一为所有 kind 不存在时返回提示信息 |
| tool 名称破坏性变更 | 低 | 早期阶段，无外部消费者 |
| `workspace/` 目录名与新命名不一致 | 低 | 暂不重命名目录，仅内部类型重命名 |
| facade 泛型参数变更影响下游 crate | 低 | 无外部消费者，影响可控 |

## 成功标准

- [ ] `MemoryFileKind` 枚举包含 8 个变体（含 `LongTerm`）
- [ ] `MemoryFileLoader` trait 统一了原 `WorkspaceMemoryLoader` + `LongTermMemory` 职责
- [ ] `LongTermMemory` trait 和 `FsLongTermMemory` 已删除
- [ ] `MemorySystem` facade 从 4 个泛型参数简化为 3 个
- [ ] tool 总数从 10 减至 8
- [ ] tool 名称：`memory_file_read`, `memory_file_write`, `memory_daily_append`, `memory_daily_read`, `role_create`, `role_list`, `role_get`, `role_delete`
- [ ] 重复的 `.map_err(...)` 模式被 `to_tool_error` 辅助函数替代
- [ ] `cargo test -p xclaw-memory` 全部通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] `cargo fmt --check` 通过
