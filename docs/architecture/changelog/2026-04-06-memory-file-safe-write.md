# 架构变更：Memory File 安全写入改造

> 日期：2026-04-06 | ADR：[ADR-008](../adr/ADR-008-memory-file-safe-write.md)

## 变更摘要

移除 `memory_file_write` 工具，替换为 `memory_file_append` 和 `memory_file_edit`。通过 content_hash 乐观并发控制强制先读后写流程，防止 LLM 覆写导致记忆丢失。

## 变更内容

### 工具变更

| 变更类型 | 工具 | 说明 |
|---|---|---|
| 删除 | `memory_file_write` | 整文件覆写，存在数据丢失风险 |
| 新增 | `memory_file_append` | 文件末尾追加，需 content_hash |
| 新增 | `memory_file_edit` | 基于行号定位替换/插入，需 content_hash |
| 修改 | `memory_file_read` | 返回值增加行号和 content_hash |

### content_hash 机制

- `memory_file_read` 返回内容时附带 content_hash
- 所有写入操作（append/edit）必须传入 content_hash
- hash 不匹配时拒绝写入，要求重新 read
- 确保 LLM 在写入前看到并处理了当前内容

### memory_file_edit 行号定位

- 使用 `line_start` + `line_end`（可选）定位目标行
- 支持 `replace`、`insert_before`、`insert_after` 三种操作
- 行号从 `memory_file_read` 的带行号输出中直接获取

### 错误类型扩展

`MemoryError` 新增：`StaleContent`、`LineOutOfRange`、`InvalidLineRange`

### MemoryFileLoader trait

新增 `append_file` 方法。`save_file` 保留供内部使用，不暴露为 LLM 工具。

## 影响范围

- `crates/xclaw-memory/src/tools/memory_file_tools.rs`
- `crates/xclaw-memory/src/tools/mod.rs`
- `crates/xclaw-memory/src/workspace/loader.rs`
- `crates/xclaw-memory/src/error.rs`
- `crates/xclaw-memory/tests/` 下相关集成测试

## 动机

防止 LLM 意外覆写有价值的记忆。将先读后写从约定提升为工具层面的强制约束，content_hash 保证 LLM 必须经历"读取 → 推理 → 写入"的完整流程。
