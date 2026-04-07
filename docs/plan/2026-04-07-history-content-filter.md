# 实施计划：历史会话内容块类型过滤配置

## 概览

当前 `LoopAgent` 在组装 `ChatRequest` 时调用 `with_history()`，该方法传入空的 `BTreeSet<ContentBlockKind>` 作为过滤器，意味着历史记录中所有内容块类型（Text、Thinking、ToolCall、ToolResult、Image、Unknown）全部发送给 LLM。本计划将默认行为改为"仅包含 Text"，同时允许通过 `AgentConfig` 配置额外需要包含的内容块类型。

## 需求

- 历史 session 默认只包含 `Text` 类型的内容块
- 其他类型（`ToolCall`、`ToolResult`、`Thinking`、`Image`、`Unknown`）可通过配置开启
- 配置位于 `AgentConfig`（`xclaw-agent/src/config.rs`），可序列化/反序列化
- `ContentBlockKind` 需要支持 serde 以用于配置
- 向后兼容：现有无此配置字段的 JSON 应使用默认值（仅 Text）

## 架构变更

- **修改** `crates/xclaw-memory/src/session/types.rs`：为 `ContentBlockKind` 添加 `Serialize`/`Deserialize` 派生
- **修改** `crates/xclaw-agent/src/config.rs`：新增 `history_content_kinds` 字段（`BTreeSet<ContentBlockKind>`）
- **修改** `crates/xclaw-agent/src/engine.rs`：在 `load_context_and_build_request` 中使用 `with_history_filtered` 替代 `with_history`

## 实施步骤

### 阶段 1：数据类型扩展（1 个文件）

1. **为 ContentBlockKind 添加 serde 支持**（文件：`crates/xclaw-memory/src/session/types.rs`）
   - 操作：为 `ContentBlockKind` 枚举添加 `#[derive(Serialize, Deserialize)]` 和 `#[serde(rename_all = "snake_case")]`
   - 原因：`AgentConfig` 需要序列化/反序列化 `BTreeSet<ContentBlockKind>`，而 `ContentBlockKind` 目前缺少 serde 派生。添加 `rename_all = "snake_case"` 确保 JSON/YAML 中使用 `text`、`tool_call`、`tool_result` 等小写下划线格式，与 `ContentBlock` 的 `#[serde(tag = "type", rename_all = "snake_case")]` 保持一致
   - 依赖：无
   - 风险：低 — 纯增量变更，现有代码无需修改

### 阶段 2：配置层扩展（1 个文件）

2. **在 AgentConfig 中新增 history_content_kinds 字段**（文件：`crates/xclaw-agent/src/config.rs`）
   - 操作：
     - 新增字段 `pub history_content_kinds: BTreeSet<ContentBlockKind>`
     - 使用 `#[serde(default = "default_history_content_kinds")]` 指定默认值函数
     - 默认值函数返回 `BTreeSet::from([ContentBlockKind::Text])`（仅包含 Text）
     - 新增 builder 方法 `with_history_content_kinds(self, kinds: BTreeSet<ContentBlockKind>) -> Self`
     - 在 `new()` 构造函数中使用默认值
   - 原因：将过滤策略作为配置项，而非硬编码在引擎中。默认值为"仅 Text"满足需求中的"默认只包含 text"要求
   - 依赖：步骤 1（依赖 `ContentBlockKind` 的 serde 支持）
   - 风险：低 — `BTreeSet` 默认反序列化为空集，需用 `serde(default = ...)` 确保缺省时使用正确默认值

### 阶段 3：引擎层接入（1 个文件）

3. **在 LoopAgent 中使用配置的过滤器**（文件：`crates/xclaw-agent/src/engine.rs`）
   - 操作：在 `load_context_and_build_request` 方法中，将 `.with_history(&history)` 替换为 `.with_history_filtered(&history, &self.config.history_content_kinds)`
   - 原因：将已有的过滤基础设施（`with_history_filtered` 和 `transcript_to_messages` 的 filter 参数）与配置连接起来
   - 依赖：步骤 2（依赖 `AgentConfig` 中的新字段）
   - 风险：低 — `with_history_filtered` 已有充分测试覆盖

### 阶段 4：测试（2-3 个文件）

4. **ContentBlockKind serde 测试**（文件：`crates/xclaw-memory/src/session/types.rs`）
   - `content_block_kind_serde_roundtrip`：验证所有变体序列化为 `"text"`、`"thinking"`、`"tool_call"`、`"tool_result"`、`"image"`、`"unknown"` 并可反序列化
   - `content_block_kind_btreeset_serde_roundtrip`：验证 `BTreeSet<ContentBlockKind>` 的序列化/反序列化

5. **AgentConfig 新字段测试**（文件：`crates/xclaw-agent/src/config.rs`）
   - `new_defaults_history_content_kinds_to_text_only`：验证 `AgentConfig::new("gpt-4o").history_content_kinds` 仅含 `Text`
   - `with_history_content_kinds_sets_filter`：验证 builder 方法正确设置
   - `serde_missing_history_content_kinds_uses_default`：验证 `{"model":"gpt-4o"}` 反序列化后 `history_content_kinds` 为 `{Text}`
   - `serde_explicit_history_content_kinds_roundtrip`：验证含 `Text + ToolCall` 的配置序列化/反序列化

6. **引擎集成测试**（文件：`crates/xclaw-agent/src/engine.rs`）
   - `history_uses_configured_content_kinds`：使用 stub 注入含 `Text + ToolCall` 内容块的历史记录，配置仅 `{Text}`，验证发送给 provider 的请求中历史消息不含 ToolCall

## 风险与缓解

- **风险**：默认从"全部包含"变为"仅 Text"可能影响依赖 ToolCall/ToolResult 上下文的对话质量
  - 缓解措施：这是需求明确要求的行为变更。用户可通过配置 `history_content_kinds: ["text", "tool_call", "tool_result"]` 恢复旧行为
- **风险**：`ContentBlockKind` 的 `#[non_exhaustive]` 属性可能影响 serde 反序列化未知变体
  - 缓解措施：`#[non_exhaustive]` 仅影响外部 crate 的 match 语句，不影响 serde 行为

## 成功标准

- [ ] `ContentBlockKind` 可序列化为 snake_case 字符串并可反序列化
- [ ] `AgentConfig::new()` 创建的配置中 `history_content_kinds` 默认为 `{Text}`
- [ ] 缺少 `history_content_kinds` 字段的 JSON 反序列化后使用默认值 `{Text}`
- [ ] `LoopAgent` 使用 `AgentConfig.history_content_kinds` 过滤历史记录
- [ ] 配置为 `{Text, ToolCall}` 时，历史中的 ToolCall 被保留、Thinking 被排除
- [ ] `cargo test` 全部通过
- [ ] `cargo clippy -- -D warnings` 无警告
