# 实施计划：SystemPromptBuilder BOOTSTRAP.md 优先填充

## 概览

调整 `SystemPromptBuilder::with_role_config` 方法的填充逻辑，使其优先使用 `MemorySnapshot` 中 `BOOTSTRAP.md` 的内容作为基础系统提示。仅当 BOOTSTRAP.md 不存在或内容为空时，才回退到 `RoleConfig.system_prompt` 的原有逻辑。

## 需求

- `with_role_config` 优先读取 BOOTSTRAP.md 内容作为基础系统提示
- 当 BOOTSTRAP.md 不存在（`None`）或内容为空时，回退到现有的 `config.system_prompt` 逻辑
- 保持现有的 prompt 分层顺序不变（base -> persona -> guidelines -> tool guidance -> long-term -> daily）
- `with_memory_snapshot` 不应重复注入 Bootstrap 内容

## 架构变更

- **修改**：`crates/xclaw-agent/src/prompt.rs` — `with_role_config` 方法签名变更，新增接收 `MemorySnapshot` 参数
- **修改**：`crates/xclaw-agent/src/engine.rs` — 调用处适配新签名

## 实施步骤

### 阶段 1：修改 SystemPromptBuilder（1 个文件）

1. **变更 `with_role_config` 方法签名与逻辑**（文件：`crates/xclaw-agent/src/prompt.rs`）
   - 操作：将 `with_role_config(mut self, config: &RoleConfig)` 改为 `with_role_config(mut self, config: &RoleConfig, snapshot: &MemorySnapshot)`。在方法内部，先检查 `snapshot.files.get(&MemoryFileKind::Bootstrap)`：如果存在且 `trim()` 非空，则使用该内容作为 `base`；否则执行原有的 `config.system_prompt` 判空回退逻辑。
   - 原因：BOOTSTRAP.md 是新角色首次启动时的引导指令，语义上应优先于 `system_prompt`。当用户完成引导流程并删除 BOOTSTRAP.md 后，自然回退到 `system_prompt`。
   - 依赖：无
   - 风险：中 — 签名变更会影响所有调用点，需同步修改

2. **确保 `with_memory_snapshot` 不重复注入 Bootstrap**（文件：`crates/xclaw-agent/src/prompt.rs`）
   - 操作：确认 `with_memory_snapshot` 的 `layers` 数组中不包含 `MemoryFileKind::Bootstrap`（当前已满足，无需改动，仅做验证）
   - 原因：Bootstrap 内容已在 `with_role_config` 中作为 base 层注入，不应重复出现
   - 依赖：步骤 1
   - 风险：低

### 阶段 2：适配调用方（1 个文件）

3. **更新 engine.rs 调用**（文件：`crates/xclaw-agent/src/engine.rs`）
   - 操作：将 `.with_role_config(&role_config)` 改为 `.with_role_config(&role_config, &snapshot)`。`snapshot` 变量已在上方加载，无需额外获取。
   - 原因：传入 snapshot 以供 `with_role_config` 读取 BOOTSTRAP.md 内容
   - 依赖：步骤 1
   - 风险：低

### 阶段 3：更新测试（1 个文件）

4. **更新现有测试并新增覆盖用例**（文件：`crates/xclaw-agent/src/prompt.rs`）
   - 操作：
     - 修改 `with_role_config_uses_system_prompt` — 传入不含 Bootstrap 的 snapshot，断言使用 `system_prompt`
     - 修改 `with_role_config_empty_prompt_generates_default` — 传入不含 Bootstrap 的 snapshot，断言生成默认提示
     - 修改 `full_system_prompt_ordering` — 传入不含 Bootstrap 的 snapshot，保持原有断言
     - 新增 `with_role_config_prefers_bootstrap_over_system_prompt` — snapshot 含 Bootstrap 内容且 `system_prompt` 非空，断言使用 Bootstrap 内容
     - 新增 `with_role_config_falls_back_when_bootstrap_empty` — snapshot 含 Bootstrap 但内容为空字符串，断言回退到 `system_prompt`
     - 新增 `with_role_config_falls_back_when_bootstrap_none` — snapshot 中 Bootstrap 为 `None`，断言回退到 `system_prompt`
   - 原因：覆盖优先级逻辑的三条路径（Bootstrap 存在、Bootstrap 为空、Bootstrap 不存在）
   - 依赖：步骤 1
   - 风险：低

## 测试策略

- 单元测试：`crates/xclaw-agent/src/prompt.rs` 中的 `#[cfg(test)] mod tests` — 覆盖 `with_role_config` 的三条分支路径
- 集成测试：通过 `cargo test -p xclaw-agent` 确认编译与全量测试通过
- E2E 测试：手动验证 — 创建含 BOOTSTRAP.md 的角色目录，启动 CLI 确认首次对话使用 bootstrap 提示；删除 BOOTSTRAP.md 后重启，确认回退到 system_prompt

## 风险与缓解

- **风险**：`with_role_config` 签名变更导致遗漏调用点
  - 缓解措施：Rust 编译器会在所有未适配的调用点报错，通过 `cargo check` 即可发现全部遗漏
- **风险**：Bootstrap 内容包含占位符模板文本，用户未编辑就投入使用
  - 缓解措施：Bootstrap 模板本身的语义就是「引导指令」，设计意图即首次对话使用，完成后用户会删除该文件

## 成功标准

- [ ] 当 BOOTSTRAP.md 存在且非空时，系统提示使用其内容作为 base 层
- [ ] 当 BOOTSTRAP.md 不存在或为空时，回退到 `config.system_prompt` 的原有逻辑
- [ ] `with_memory_snapshot` 不重复注入 Bootstrap 内容
- [ ] 所有现有测试适配后通过
- [ ] 新增 3 个测试覆盖优先级分支
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] `cargo fmt --check` 通过
