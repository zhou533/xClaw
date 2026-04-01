# 实施计划：MemoryFileLoader delete_file 支持

## 概览

为 `MemoryFileLoader` trait 新增 `delete_file` 方法，允许按 `RoleId` + `MemoryFileKind` 删除单个记忆文件。同时新增 `MemoryFileDeleteTool` 供 LLM 工具调用，并将其注册到工具链中。

## 需求

- `MemoryFileLoader` trait 新增 `delete_file(&self, role, kind) -> Result<bool, MemoryError>` 方法
- `FsMemoryFileLoader` 实现该方法：删除磁盘文件，文件不存在时返回 `Ok(false)`
- 新增 `MemoryFileDeleteTool`，遵循现有 read/write tool 的模式
- 将新 tool 注册到 `register_memory_tools`
- 路径必须限制在 `base_dir/roles/{role}/` 下，不允许路径遍历
- 单元测试 + 集成测试覆盖正常路径与边界情况

## 变更文件

| 文件 | 操作 |
|------|------|
| `crates/xclaw-memory/src/workspace/loader.rs` | trait 新增方法 + FsMemoryFileLoader 实现 + 单元测试 |
| `crates/xclaw-memory/src/tools/memory_file_tools.rs` | 新增 MemoryFileDeleteTool |
| `crates/xclaw-memory/src/tools/mod.rs` | 导出新 tool 并注册 |
| `crates/xclaw-memory/tests/memory_files_integration.rs` | 新增集成测试 |

## 实施阶段

### 阶段 1：Trait 与核心实现（1 个文件）

1. **在 MemoryFileLoader trait 中新增 delete_file 方法**
   - 文件：`crates/xclaw-memory/src/workspace/loader.rs`
   - 操作：在 `MemoryFileLoader` trait 中添加方法签名：
     ```rust
     fn delete_file(
         &self,
         role: &RoleId,
         kind: MemoryFileKind,
     ) -> impl std::future::Future<Output = Result<bool, MemoryError>> + Send;
     ```
     返回 `bool` 表示文件是否实际存在并被删除（`true`），还是本来就不存在（`false`）。
   - 依赖：无
   - 风险：低

2. **在 FsMemoryFileLoader 中实现 delete_file**
   - 文件：`crates/xclaw-memory/src/workspace/loader.rs`
   - 操作：使用 `self.file_path(role, kind)` 得到路径，先检查 `path.exists()`，若不存在返回 `Ok(false)`，若存在则调用 `tokio::fs::remove_file(&path).await?` 后返回 `Ok(true)`。
   - 依赖：步骤 1
   - 风险：低

3. **新增 delete_file 单元测试**
   - 文件：`crates/xclaw-memory/src/workspace/loader.rs`（`#[cfg(test)] mod tests` 内）
   - 新增测试：
     - `delete_nonexistent_returns_false` -- 删除不存在的文件，断言返回 `Ok(false)`
     - `save_then_delete_returns_true` -- 先保存再删除，断言返回 `Ok(true)`，且 `load_file` 返回 `None`
     - `delete_is_idempotent` -- 连续删除两次同一文件，第一次 `true`，第二次 `false`
   - 依赖：步骤 2
   - 风险：低

### 阶段 2：LLM Tool 层（2 个文件）

4. **新增 MemoryFileDeleteTool**
   - 文件：`crates/xclaw-memory/src/tools/memory_file_tools.rs`
   - 操作：仿照 `MemoryFileWriteTool` 的结构创建，`name()` 返回 `"memory_file_delete"`，接受 `role`（可选）和 `kind`（必需）参数
   - 依赖：步骤 2
   - 风险：低

5. **注册新 tool 并导出**
   - 文件：`crates/xclaw-memory/src/tools/mod.rs`
   - 操作：在导出和 `register_memory_tools` 中添加 `MemoryFileDeleteTool`
   - 依赖：步骤 4
   - 风险：低

### 阶段 3：集成测试（1 个文件）

6. **新增 delete_file 集成测试**
   - 文件：`crates/xclaw-memory/tests/memory_files_integration.rs`
   - 新增测试：
     - `workspace_delete_existing_file` -- 保存后删除，验证返回 `true`，再 load 验证返回 `None`
     - `workspace_delete_nonexistent_returns_false` -- 删除不存在的文件，验证返回 `false`
     - `workspace_delete_then_snapshot_excludes_deleted` -- 保存多个文件，删除其中一个，验证 snapshot 中被删除的为 `None`
   - 依赖：步骤 2
   - 风险：低

## 风险与缓解

- **删除不可逆** — 与现有 `save_file` 覆盖写入行为一致，后续可按需添加 trash/undo
- **并发竞争** — 与 load/save 策略一致，单用户 CLI 场景并发概率极低
- **trait 新增方法** — 仅 crate 内部 `FsMemoryFileLoader` 一个实现，无下游影响

## 成功标准

- [ ] `MemoryFileLoader` trait 包含 `delete_file` 方法
- [ ] `FsMemoryFileLoader` 正确实现文件删除，文件不存在时返回 `Ok(false)`
- [ ] `MemoryFileDeleteTool` 可通过 LLM tool 调用删除记忆文件
- [ ] 新 tool 已注册到 `register_memory_tools`
- [ ] 所有新增单元测试和集成测试通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] `cargo fmt --check` 通过

## 复杂度：低
