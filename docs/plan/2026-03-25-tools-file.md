# 实施计划：xclaw-tools file 工具 (file_read / file_write / file_edit)

> 日期：2026-03-25 | 状态：Approved

## 概览

在 `xclaw-tools` crate 中实现 `Tool` trait、`ToolContext`、`ToolRegistry` 等基础设施，以及 `file_read`、`file_write`、`file_edit` 三个文件操作工具。包含路径穿越防护、fs_allowlist 校验、超时控制等安全机制，并提供启动时注册的入口函数。

## 架构变更

| 操作 | 文件 | 说明 |
|------|------|------|
| 新增 | `crates/xclaw-tools/src/error.rs` | `ToolError` 枚举 |
| 新增 | `crates/xclaw-tools/src/traits.rs` | `Tool` trait、`ToolContext`、`ToolOutput`、`ToolSchema`、`WorkspaceScope` |
| 新增 | `crates/xclaw-tools/src/security.rs` | 路径校验工具函数 |
| 重写 | `crates/xclaw-tools/src/registry.rs` | `ToolRegistry` 完整实现 |
| 重写 | `crates/xclaw-tools/src/file.rs` | `FileReadTool`、`FileWriteTool`、`FileEditTool` + `register_file_tools` |
| 修改 | `crates/xclaw-tools/src/lib.rs` | 模块声明与 re-export |
| 修改 | `crates/xclaw-tools/Cargo.toml` | 新增 `async-trait` 依赖 |
| 修改 | `crates/xclaw-core/src/error.rs` | 新增 `Tool` 错误变体 |

## 实施阶段

### 阶段 1：核心类型与 trait 定义

1. **ToolError 枚举**（`crates/xclaw-tools/src/error.rs`）
   - 变体：`PathDenied`、`PathTraversal`、`IoError`、`Timeout`、`InvalidParams`、`EditNotFound`、`Internal`
   - 使用 thiserror 派生

2. **Tool trait + 关联类型**（`crates/xclaw-tools/src/traits.rs`）
   - `WorkspaceScope { workspace_root: PathBuf }`
   - `ToolContext { scope, fs_allowlist, net_allowlist, timeout }`
   - `ToolOutput { content: String, is_error: bool }`
   - `ToolSchema { name, description, parameters: serde_json::Value }`
   - `#[async_trait] trait Tool: Send + Sync { name, description, parameters_schema, execute }`

3. **更新 Cargo.toml** — 新增 `async-trait` 依赖

### 阶段 2：安全基础设施

4. **路径安全校验模块**（`crates/xclaw-tools/src/security.rs`）
   - `validate_path(path, ctx) -> Result<PathBuf>` — canonicalize + allowlist
   - `validate_path_for_write(path, ctx) -> Result<PathBuf>` — 处理文件尚不存在场景
   - 使用 `Path::starts_with` 避免 Windows UNC 路径问题

### 阶段 3：ToolRegistry 实现

5. **ToolRegistry**（`crates/xclaw-tools/src/registry.rs`）
   - `new()` / `register()` / `get()` / `list_schemas()`
   - 重名 warn + 覆盖

### 阶段 4：file 工具实现

6. **FileReadTool** — `file_read`
   - params: `{ path, offset?, limit? }`
   - 10MB 上限保护
   - tokio::fs + timeout

7. **FileWriteTool** — `file_write`
   - params: `{ path, content }`
   - 自动 create_dir_all
   - validate_path_for_write

8. **FileEditTool** — `file_edit`
   - params: `{ path, edits: [{ search, replace }] }`
   - 默认仅替换第一处匹配
   - 未匹配返回 EditNotFound

### 阶段 5：注册入口与集成

9. **register_file_tools(registry)** — 批量注册三个 file 工具

10. **lib.rs 更新** — 模块声明 + re-export + `register_builtin_tools` 顶层入口

11. **XClawError 扩展** — 新增 `Tool(String)` 变体

## 测试策略

- **单元测试**（各模块 `#[cfg(test)]`）：
  - error.rs — Display 格式
  - traits.rs — 构造与序列化
  - security.rs — 合法路径通过、穿越拒绝、符号链接拒绝、allowlist 外拒绝
  - registry.rs — register/get/list_schemas、重名覆盖
  - file.rs — tempdir 场景：read/write/edit 正常流程 + 路径越界拒绝

- **集成测试**（`crates/xclaw-tools/tests/`）：
  - 构造 ToolRegistry + ToolContext，通过 registry.get 执行工具端到端

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| async_trait vs 原生 async fn in trait | 先尝试原生，dyn dispatch 仍需 async_trait |
| canonicalize 要求路径存在 (file_write) | validate_path_for_write 向上查找已存在祖先 |
| file_edit search 多次匹配 | 默认仅替换第一处，输出报告行号 |
| Windows UNC 路径 | 使用 Path::starts_with |
| 大文件读取 OOM | 10MB 上限 + offset/limit |
