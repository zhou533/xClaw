# ADR-008: Memory File 安全写入改造

> 日期：2026-04-06 | 状态：Accepted

## 背景

当前 `memory_file_write` 工具接受 `content` 参数，调用 `FsMemoryFileLoader::save_file()` 进行整文件覆写。LLM 调用时如果遗漏原有段落或产生幻觉内容，有价值的记忆将永久丢失。这是一个不可逆的数据损失风险。

## 决策

移除 `memory_file_write` 工具，替换为 `memory_file_append` 和 `memory_file_edit` 两个安全工具。所有写入操作通过 content_hash 强制先读后写，确保 LLM 在修改前看到并处理了当前内容。

## 设计

### 工具拆分

| 旧工具 | 新工具 | 操作语义 |
|---|---|---|
| `memory_file_write` (删除) | `memory_file_append` | 在文件末尾追加内容 |
| — | `memory_file_edit` | 基于行号定位，替换或插入 |

### content_hash 乐观并发控制

#### 读取阶段

`memory_file_read` 返回内容时附带行号和 content_hash（文件内容的摘要前 16 hex chars）：

```
---
content_hash: a1b2c3d4e5f67890
---
1: ## Memory
2:
3: - User prefers Rust
4: - Project uses edition 2024
5:
6: ## Key Facts
7:
8: - Workspace is Cargo-based
```

#### 写入阶段

`memory_file_append` 和 `memory_file_edit` 都要求传入 `content_hash` 参数：

- 工具执行时先读取当前文件，计算 hash
- 若传入的 hash 与当前 hash 不匹配 → 返回错误，要求重新 read
- 若匹配 → 执行操作
- 若文件不存在且 hash 为 `"__new__"` → 允许创建（仅 append）

#### LLM 使用流程

```
1. LLM 调用 memory_file_read(kind="long_term")
   → 获得带行号的内容 + content_hash: "a1b2c3d4"

2. LLM 推理：需要在第 8 行之后插入新知识

3. LLM 调用 memory_file_edit(
     kind="long_term",
     content_hash="a1b2c3d4",
     line_start=8,
     operation="insert_after",
     content="- Tests run single-threaded"
   )
   → 工具验证 hash，执行操作
```

### memory_file_append 参数

```json
{
  "role": "string (default: 'default')",
  "kind": "string (enum: agents, soul, tools, identity, user, heartbeat, bootstrap, long_term)",
  "content": "string (要追加的内容)",
  "content_hash": "string (从 memory_file_read 获得，或 '__new__' 表示创建新文件)"
}
```

语义：在文件末尾追加 `\n\n{content}`。若文件不存在且 hash 为 `__new__` 则创建。

### memory_file_edit 参数

```json
{
  "role": "string (default: 'default')",
  "kind": "string (enum: agents, soul, tools, identity, user, heartbeat, bootstrap, long_term)",
  "content_hash": "string (从 memory_file_read 获得)",
  "line_start": "integer (起始行号，含，从 1 开始)",
  "line_end": "integer (结束行号，含，可选 — 省略则等于 line_start)",
  "operation": "replace | insert_before | insert_after",
  "content": "string (新内容)"
}
```

#### 操作语义

| operation | 行为 |
|---|---|
| `replace` | 将 `line_start..=line_end` 的行替换为 `content` |
| `insert_before` | 在 `line_start` 之前插入 `content`（忽略 line_end） |
| `insert_after` | 在 `line_end`（或 line_start）之后插入 `content`（忽略另一个） |

#### 边界校验

- `line_start < 1` 或 `line_start > total_lines` → 错误
- `line_end < line_start` → 错误
- `line_end > total_lines` → 错误
- content_hash 不匹配 → 错误（要求重新 read）

### 错误类型扩展

`MemoryError` 新增变体：

```rust
#[error("content hash mismatch: file changed since last read")]
StaleContent { expected: String, actual: String },

#[error("line out of range: {line} (file has {total} lines)")]
LineOutOfRange { line: usize, total: usize },

#[error("invalid line range: start {start} > end {end}")]
InvalidLineRange { start: usize, end: usize },
```

### MemoryFileLoader trait 扩展

```rust
/// Append content to a memory file (creates if absent).
fn append_file(
    &self, role: &RoleId, kind: MemoryFileKind, content: &str,
) -> impl Future<Output = Result<(), MemoryError>> + Send;
```

`save_file` 保留但仅供内部使用（edit 操作在验证 hash → 修改行 → 写回时仍需调用），不暴露为 LLM 工具。

### 工具注册变更

```rust
pub fn register_memory_tools(registry: &mut ToolRegistry, base_dir: PathBuf) {
    // ...existing role tools & daily tools...
    registry.register(MemoryFileReadTool::new(&base_dir));
    registry.register(MemoryFileAppendTool::new(&base_dir));  // 新增
    registry.register(MemoryFileEditTool::new(&base_dir));    // 新增
    registry.register(MemoryFileDeleteTool::new(&base_dir));
    // MemoryFileWriteTool 不再注册
}
```

工具数量：9 → 10（删 write，加 append + edit）。

### 系统提示词配合

工具描述需清晰引导 LLM：

- **memory_file_append**: "Append content to the end of a role's memory file. REQUIRES content_hash from a prior memory_file_read call — read the file first, then append. Pass content_hash='__new__' only when creating a new file."
- **memory_file_edit**: "Edit a role's memory file by line number. REQUIRES content_hash from a prior memory_file_read call — you must read the file first to see its current content with line numbers, then edit. Returns error if line out of range or content_hash is stale."

## 取舍

### 为什么用 content_hash 而非工具内部先读后写？

先读后写的"中间"是 LLM 的推理环节——LLM 必须先看到当前内容，思考后再操作。如果工具内部黑盒完成读-写，LLM 无法参与决策，无法避免语义层面的覆盖错误。content_hash 将 LLM 强制纳入读-思考-写的流程。

### 为什么用行号而非 heading？

- 行号从 read 结果直接可见，LLM 无需额外解析
- 对任何格式的文件都适用，不仅限 Markdown
- 更精确：可定位到任意连续行范围
- 更简单：不需要 Markdown section 解析逻辑

### 为什么保留 delete 工具？

Delete 是显式的破坏性操作，语义明确且可审计。与 write 不同，delete 不会产生"部分覆盖"的模糊风险。

### 向后兼容性

- Breaking change：`memory_file_write` 工具被移除
- 底层 `MemoryFileLoader::save_file()` 不变，仅工具层不再暴露
- 集成测试中 write 相关测试需改为使用 append/edit
- 早期开发阶段，影响可控

## 文件变更清单

| 文件 | 变更 |
|---|---|
| `crates/xclaw-memory/src/tools/memory_file_tools.rs` | `MemoryFileReadTool` 返回行号 + hash；删除 `MemoryFileWriteTool`；新增 `MemoryFileAppendTool` + `MemoryFileEditTool`；新增 hash 计算辅助函数 |
| `crates/xclaw-memory/src/tools/mod.rs` | 更新 re-exports 和 `register_memory_tools` |
| `crates/xclaw-memory/src/workspace/loader.rs` | trait 新增 `append_file`，`FsMemoryFileLoader` 实现 |
| `crates/xclaw-memory/src/error.rs` | 新增 `StaleContent` + `LineOutOfRange` + `InvalidLineRange` |
| `crates/xclaw-memory/tests/memory_system_integration.rs` | 更新 write 相关测试为 append/edit |
| `crates/xclaw-memory/tests/memory_files_integration.rs` | 新增 append/edit/hash/行号边界集成测试 |
