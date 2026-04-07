# 实施计划：ADR-008 Memory File 安全写入改造

> 日期：2026-04-06 | ADR：[ADR-008](../architecture/adr/ADR-008-memory-file-safe-write.md)

## 概览

移除 `memory_file_write` 工具，替换为 `memory_file_append` 和 `memory_file_edit` 两个安全工具。通过 content_hash 乐观并发控制强制 LLM 先读后写，防止记忆文件被意外覆盖丢失。同时扩展 `MemoryFileLoader` trait 和 `MemoryError` 枚举以支撑新语义。

## 需求

- 删除 `MemoryFileWriteTool`，不再暴露整文件覆写能力给 LLM
- 新增 `MemoryFileAppendTool`：末尾追加，需传入 content_hash
- 新增 `MemoryFileEditTool`：基于行号定位，支持 replace / insert_before / insert_after
- `MemoryFileReadTool` 返回带行号和 content_hash 的格式化内容
- `MemoryFileLoader` trait 新增 `append_file` 方法
- `MemoryError` 新增 `StaleContent`、`LineOutOfRange`、`InvalidLineRange` 变体
- 新错误变体需正确映射到 `ToolError::InvalidParams`
- 工具注册数量从 9 变为 10
- 所有现有测试适配为新工具 API

## 架构变更

| 文件 | 变更说明 |
|---|---|
| `crates/xclaw-memory/src/error.rs` | 新增 3 个错误变体 |
| `crates/xclaw-memory/src/tools/mod.rs` | 更新 re-exports、to_tool_error 映射、register_memory_tools |
| `crates/xclaw-memory/src/workspace/loader.rs` | trait 新增 `append_file`；FsMemoryFileLoader 实现 |
| `crates/xclaw-memory/src/tools/memory_file_tools.rs` | 重写：删 WriteTool、改 ReadTool、新增 AppendTool + EditTool + hash 辅助函数 |
| `crates/xclaw-memory/tests/memory_system_integration.rs` | 适配 write 相关测试为 append/edit |
| `crates/xclaw-memory/tests/memory_files_integration.rs` | 新增 append/edit/hash 边界集成测试 |

## 实施步骤

### 阶段 1：错误类型扩展（1 个文件，可独立编译）

1. **新增 MemoryError 变体的单元测试**（文件：`crates/xclaw-memory/src/error.rs`）
   - 操作：在 `#[cfg(test)] mod tests` 中新增 3 个测试：`stale_content_display`、`line_out_of_range_display`、`invalid_line_range_display`，验证 `to_string()` 输出符合 ADR-008 定义的 `#[error(...)]` 格式
   - 原因：TDD 先写测试；确认错误消息包含关键字段（expected/actual、line/total、start/end）
   - 依赖：无
   - 风险：低

2. **新增 MemoryError 变体**（文件：`crates/xclaw-memory/src/error.rs`）
   - 操作：在 `MemoryError` 枚举中新增：
     ```rust
     #[error("content hash mismatch: file changed since last read")]
     StaleContent { expected: String, actual: String },

     #[error("line out of range: {line} (file has {total} lines)")]
     LineOutOfRange { line: usize, total: usize },

     #[error("invalid line range: start {start} > end {end}")]
     InvalidLineRange { start: usize, end: usize },
     ```
   - 原因：后续阶段的 append/edit 工具需要这些类型化错误
   - 依赖：步骤 1（测试先写）
   - 风险：低

3. **新增 to_tool_error 映射测试**（文件：`crates/xclaw-memory/src/tools/mod.rs`）
   - 操作：在 `#[cfg(test)] mod tests` 中新增 3 个测试，验证 `StaleContent` / `LineOutOfRange` / `InvalidLineRange` 都映射到 `ToolError::InvalidParams`
   - 原因：这三种错误都是 LLM 传参导致的，应映射为 InvalidParams 而非 Internal
   - 依赖：步骤 2
   - 风险：低

4. **更新 to_tool_error 映射**（文件：`crates/xclaw-memory/src/tools/mod.rs`）
   - 操作：在 `to_tool_error` 函数的 `match` 分支中，将 `StaleContent { .. }` / `LineOutOfRange { .. }` / `InvalidLineRange { .. }` 加入 `ToolError::InvalidParams` 分支
   - 原因：让步骤 3 的测试通过
   - 依赖：步骤 3
   - 风险：低

**阶段 1 验证**：`cargo test -p xclaw-memory` 全部通过。新增 6 个测试，现有测试不受影响。

### 阶段 2：MemoryFileLoader trait 扩展（1 个文件，可独立编译）

5. **新增 append_file 单元测试**（文件：`crates/xclaw-memory/src/workspace/loader.rs`）
   - 操作：在 `#[cfg(test)] mod tests` 中新增：
     - `append_to_nonexistent_creates_file`：对不存在的文件调用 append，验证文件被创建且内容正确
     - `append_to_existing_appends_content`：先 save_file 写入初始内容，再 append，验证内容为 `"{原有内容}\n\n{追加内容}"`
     - `append_multiple_times`：连续 append 3 次，验证内容按序拼接
   - 原因：TDD 先写测试，定义 append 的精确语义
   - 依赖：无（trait 方法签名先添加后测试才能编译）
   - 风险：低

6. **MemoryFileLoader trait 新增 append_file 方法**（文件：`crates/xclaw-memory/src/workspace/loader.rs`）
   - 操作：在 trait 中新增：
     ```rust
     fn append_file(
         &self, role: &RoleId, kind: MemoryFileKind, content: &str,
     ) -> impl Future<Output = Result<(), MemoryError>> + Send;
     ```
   - 原因：为 MemoryFileAppendTool 提供底层存储能力
   - 依赖：步骤 5（同步完成）
   - 风险：低

7. **FsMemoryFileLoader 实现 append_file**（文件：`crates/xclaw-memory/src/workspace/loader.rs`）
   - 操作：实现逻辑：
     1. 读取当前文件内容（若存在）
     2. 若文件存在：新内容 = `"{existing}\n\n{content}"`
     3. 若文件不存在：新内容 = `content`
     4. 调用现有 `save_file` 写入（复用原子写入逻辑）
   - 原因：复用 save_file 的原子写入，避免重复代码
   - 依赖：步骤 6
   - 风险：低

**阶段 2 验证**：`cargo test -p xclaw-memory` 全部通过。append_file 3 个新测试通过。

### 阶段 3：content_hash 辅助函数（1 个文件，可独立编译）

8. **新增 content_hash 辅助函数及其测试**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
   - 操作：
     - 新增 `pub(crate) fn compute_content_hash(content: &str) -> String`：使用 `sha2::Sha256` 对 content 计算摘要，返回前 16 个 hex 字符
     - 新增 `pub(crate) fn format_with_line_numbers(content: &str, hash: &str) -> String`：返回 ADR-008 定义的格式（YAML front matter + 行号前缀）
     - 在 `#[cfg(test)] mod tests` 中新增：
       - `compute_hash_deterministic`：同一内容多次调用返回相同 hash
       - `compute_hash_different_for_different_content`：不同内容返回不同 hash
       - `compute_hash_length_is_16`：返回值长度为 16
       - `format_with_line_numbers_output`：验证格式正确（含 front matter 和行号）
   - 原因：hash 计算是 read/append/edit 三个工具的共享基础
   - 依赖：无（需在 Cargo.toml 中添加 `sha2` 依赖）
   - 风险：中 — 需确认 `sha2` crate 兼容 edition 2024

**Cargo.toml 变更**：在 `crates/xclaw-memory/Cargo.toml` 的 `[dependencies]` 中添加 `sha2 = "0.10"`。

**阶段 3 验证**：`cargo test -p xclaw-memory` 全部通过。hash 相关 4 个新测试通过。

### 阶段 4：改造 MemoryFileReadTool（1 个文件，可独立编译）

9. **改造 MemoryFileReadTool 返回行号 + hash**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
   - 操作：修改 `MemoryFileReadTool::execute` 的成功分支：
     1. 对读取到的内容调用 `compute_content_hash`
     2. 调用 `format_with_line_numbers` 格式化输出
     3. 返回格式化后的内容（含 YAML front matter `content_hash: xxx` + 带行号的正文）
   - 原因：LLM 需要看到行号以使用 edit 工具，需要 hash 以传给 append/edit
   - 依赖：步骤 8
   - 风险：低 — 但会导致现有集成测试中 `tool_memory_file_write_and_read` 的断言失败；这些测试将在阶段 7 适配

10. **更新 MemoryFileReadTool 的 description**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
    - 操作：将 description 更新为包含行号和 content_hash 说明的文本，告知 LLM 返回格式
    - 原因：LLM 需要从工具描述理解返回格式
    - 依赖：步骤 9
    - 风险：低

**阶段 4 验证**：`cargo test -p xclaw-memory --lib` 通过（单元测试）。集成测试暂时会有部分失败，在阶段 7 修复。

### 阶段 5：新增 MemoryFileAppendTool（1 个文件）

11. **新增 MemoryFileAppendTool 单元测试**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
    - 操作：在 `#[cfg(test)] mod tests` 中新增：
      - `append_tool_name_and_schema`：验证 name 为 `"memory_file_append"`、schema 包含 content_hash 字段
    - 原因：验证工具元数据正确
    - 依赖：步骤 8

12. **实现 MemoryFileAppendTool**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
    - 操作：
      - 新增 `MemoryFileAppendTool` struct（持有 `base_dir: PathBuf`）
      - 实现 `Tool` trait：
        - `name()` → `"memory_file_append"`
        - `description()` → ADR-008 定义的引导文本
        - `parameters_schema()` → 含 role、kind、content、content_hash 四个参数
        - `execute()` 逻辑：
          1. 解析 role、kind、content、content_hash
          2. 通过 `FsMemoryFileLoader::load_file` 读取当前内容
          3. 若文件存在：计算当前 hash，与传入 hash 比较；不匹配返回 `MemoryError::StaleContent`
          4. 若文件不存在且 hash == `"__new__"`：允许创建
          5. 若文件不存在且 hash != `"__new__"`：返回 StaleContent 错误
          6. 调用 `FsMemoryFileLoader::append_file` 执行追加（新文件直接 save_file）
    - 原因：替代 write 工具的主要新建/追加场景
    - 依赖：步骤 6-8
    - 风险：中 — 需确保 hash == `"__new__"` 时文件确实不存在

### 阶段 6：新增 MemoryFileEditTool（1 个文件）

13. **新增 MemoryFileEditTool 单元测试**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
    - 操作：在 `#[cfg(test)] mod tests` 中新增：
      - `edit_tool_name_and_schema`：验证 name 为 `"memory_file_edit"`、schema 包含 line_start / line_end / operation / content_hash
    - 原因：验证工具元数据正确
    - 依赖：步骤 8

14. **新增行编辑纯函数及其测试**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
    - 操作：新增 `pub(crate) fn apply_line_edit(lines: &[&str], line_start: usize, line_end: usize, operation: &str, new_content: &str) -> Result<String, MemoryError>`
      - `replace`：将 `lines[line_start-1..=line_end-1]` 替换为 new_content 的各行
      - `insert_before`：在 `lines[line_start-1]` 之前插入
      - `insert_after`：在 `lines[line_end-1]` 之后插入
      - 边界校验：line_start < 1 / > total → `LineOutOfRange`；line_end < line_start → `InvalidLineRange`；line_end > total → `LineOutOfRange`
    - 在 `#[cfg(test)] mod tests` 中新增：
      - `apply_replace_single_line`
      - `apply_replace_range`
      - `apply_insert_before`
      - `apply_insert_after`
      - `apply_line_out_of_range`
      - `apply_invalid_line_range`
      - `apply_replace_last_line`
    - 原因：行编辑是纯逻辑，抽为函数可单元测试，不依赖文件系统
    - 依赖：步骤 2（需要 MemoryError 新变体）
    - 风险：低

15. **实现 MemoryFileEditTool**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
    - 操作：
      - 新增 `MemoryFileEditTool` struct（持有 `base_dir: PathBuf`）
      - 实现 `Tool` trait：
        - `name()` → `"memory_file_edit"`
        - `description()` → ADR-008 定义的引导文本
        - `parameters_schema()` → 含 role、kind、content_hash、line_start、line_end（可选，默认=line_start）、operation、content
        - `execute()` 逻辑：
          1. 解析所有参数；line_end 缺省时等于 line_start
          2. 通过 `FsMemoryFileLoader::load_file` 读取当前内容
          3. 若文件不存在 → 返回错误（edit 不创建文件）
          4. 计算当前 hash，与传入 hash 比较；不匹配返回 `StaleContent`
          5. 调用 `apply_line_edit` 获得新内容
          6. 调用 `FsMemoryFileLoader::save_file` 写入
    - 原因：提供基于行号的精确编辑能力
    - 依赖：步骤 14
    - 风险：中 — operation 参数需严格校验为 replace/insert_before/insert_after

### 阶段 7：删除 WriteTool + 更新注册 + 适配测试（3 个文件）

16. **删除 MemoryFileWriteTool**（文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`）
    - 操作：删除整个 `MemoryFileWriteTool` struct 及其 `impl Tool` 块（约 60 行，第 155-218 行）
    - 原因：ADR-008 决策移除此工具
    - 依赖：步骤 12、15（新工具已就位）
    - 风险：中 — 需同步更新 mod.rs 的 re-exports

17. **更新 mod.rs 注册与导出**（文件：`crates/xclaw-memory/src/tools/mod.rs`）
    - 操作：
      - 将 `pub use` 中的 `MemoryFileWriteTool` 替换为 `MemoryFileAppendTool, MemoryFileEditTool`
      - 更新 `register_memory_tools` 函数：
        - 移除 `registry.register(MemoryFileWriteTool::new(&base_dir));`
        - 新增 `registry.register(MemoryFileAppendTool::new(&base_dir));`
        - 新增 `registry.register(MemoryFileEditTool::new(&base_dir));`
    - 原因：注册新工具，工具总数从 9 变为 10
    - 依赖：步骤 16
    - 风险：低

18. **适配 memory_system_integration.rs**（文件：`crates/xclaw-memory/tests/memory_system_integration.rs`）
    - 操作：
      - `register_memory_tools_adds_9_tools` → 改名为 `register_memory_tools_adds_10_tools`，数量改为 10，expected 列表中将 `"memory_file_write"` 替换为 `"memory_file_append"` 和 `"memory_file_edit"`
      - `tool_memory_file_write_and_read` → 改为 `tool_memory_file_append_and_read`：
        1. 先通过 `memory_file_read` 读取（文件不存在时无 hash）
        2. 用 `memory_file_append` + `content_hash="__new__"` 创建文件
        3. 再通过 `memory_file_read` 读取，验证返回内容包含行号和 hash
      - `tool_memory_file_write_and_read_long_term` → 改为 `tool_memory_file_append_and_read_long_term`：同上逻辑
      - `tool_memory_file_write_missing_content_returns_error` → 删除；替换为 `tool_memory_file_append_missing_content_returns_error` 和 `tool_memory_file_append_missing_hash_returns_error`
    - 原因：适配工具 API 变更
    - 依赖：步骤 17
    - 风险：中 — 需确保 read 返回格式的断言与阶段 4 的实现一致

19. **新增 append/edit 集成测试**（文件：`crates/xclaw-memory/tests/memory_files_integration.rs`）
    - 操作：新增以下测试（直接使用 FsMemoryFileLoader，不经过 Tool 层）：
      - `append_to_nonexistent_creates_file`
      - `append_to_existing_preserves_original`
      - `append_multiple_accumulates`
    - 原因：在集成层面验证 append_file 的文件系统行为
    - 依赖：步骤 7
    - 风险：低

20. **新增工具层 edit 集成测试**（文件：`crates/xclaw-memory/tests/memory_system_integration.rs`）
    - 操作：新增以下测试：
      - `tool_memory_file_edit_replace`：创建文件 → read 获取 hash → edit replace → read 验证
      - `tool_memory_file_edit_insert_after`：创建文件 → read → edit insert_after → read 验证
      - `tool_memory_file_edit_insert_before`：创建文件 → read → edit insert_before → read 验证
      - `tool_memory_file_edit_stale_hash_rejected`：创建文件 → read 获取 hash → 直接 save_file 修改文件 → edit 传旧 hash → 验证被拒绝
      - `tool_memory_file_edit_line_out_of_range`：创建文件 → read → edit line_start=999 → 验证错误
      - `tool_memory_file_append_stale_hash_rejected`：创建文件 → read 获取 hash → 直接 save_file 修改文件 → append 传旧 hash → 验证被拒绝
      - `tool_memory_file_append_new_file_with_wrong_hash_rejected`：文件不存在 → append 传非 `__new__` 的 hash → 验证被拒绝
    - 原因：全面覆盖 content_hash 并发控制和行号边界的端到端行为
    - 依赖：步骤 17
    - 风险：中 — 测试需从 read 返回的格式化输出中解析 content_hash，需编写 helper

**阶段 7 验证**：`cargo test -p xclaw-memory` 全部通过。`cargo clippy -- -D warnings` 无警告。

## 测试策略

- **单元测试**（`#[cfg(test)]` 内联）：
  - `error.rs`：3 个新错误变体的 display 测试
  - `tools/mod.rs`：3 个新错误变体的 to_tool_error 映射测试
  - `tools/memory_file_tools.rs`：hash 计算 4 个测试、行号格式化 1 个测试、行编辑纯函数 7 个测试、工具元数据 2 个测试
  - `workspace/loader.rs`：append_file 3 个测试
- **集成测试**（`tests/` 目录）：
  - `memory_files_integration.rs`：append_file 3 个新测试
  - `memory_system_integration.rs`：工具注册 1 个更新、read+append 2 个改写、edit 各操作 3 个新测试、hash 拒绝 3 个新测试、参数缺失 2 个新测试

新增测试总计约 34 个。

## 风险与缓解

| 风险 | 等级 | 缓解措施 |
|------|------|---------|
| `sha2` crate 与 Rust edition 2024 不兼容 | 中 | 阶段 3 最先添加依赖并编译验证；不兼容可换 `blake3` |
| read 返回格式变更导致下游解析失败 | 低 | 当前仅 LLM 消费文本输出，无结构化解析依赖 |
| 行编辑的并发安全 | 低 | content_hash 提供乐观锁；当前单 agent 单线程执行 |
| 删除 WriteTool 后遗漏外部引用 | 中 | 步骤 16 前执行 `cargo build` 确认编译通过 |

## 成功标准

- [ ] `MemoryFileWriteTool` 完全删除，代码中无残留引用
- [ ] `memory_file_read` 返回带行号和 content_hash 的格式化内容
- [ ] `memory_file_append` 工具可创建新文件（hash=`__new__`）和追加内容
- [ ] `memory_file_edit` 工具支持 replace / insert_before / insert_after 三种操作
- [ ] content_hash 不匹配时，append 和 edit 均返回 StaleContent 错误
- [ ] 行号越界时返回 LineOutOfRange 或 InvalidLineRange 错误
- [ ] 工具注册数量为 10
- [ ] `cargo test -p xclaw-memory` 全部通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] `cargo fmt --check` 无格式问题
