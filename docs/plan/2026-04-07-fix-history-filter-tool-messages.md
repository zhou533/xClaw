# 修复 transcript_to_messages 过滤后产生非法 Tool 消息

> 日期：2026-04-07
> 状态：已确认
> 复杂度：低

## 概览

当 `history_content_kinds` 默认为 `{Text}` 时，`transcript_to_messages` 仍然为每条 `TranscriptRole::Tool` 记录生成 `Role::Tool` 消息，但 `ToolResult` 块被过滤掉后 `tool_call_id` 为 `None`，导致 MiniMax API 报错 `invalid params, tool result's tool id() not found (2013)`。修复方案：将过滤从 block 级别提升到 record 级别，过滤后无效的消息整条丢弃。

## 架构变更

- **修改** `crates/xclaw-agent/src/session.rs`：`record_to_message` 返回 `Option<Message>`，`transcript_to_messages` 改用 `filter_map`

## 实施步骤

### 阶段 1：核心修复（1 个文件）

1. **将 `record_to_message` 返回类型改为 `Option<Message>`**（文件：`crates/xclaw-agent/src/session.rs`，第 86-166 行）
   - 操作：
     - 函数签名改为 `fn record_to_message(...) -> Option<Message>`
     - `TranscriptRole::Tool` 分支：过滤后无 `ToolResult` 块时返回 `None`
     - `TranscriptRole::Assistant` 分支：过滤后 text 为空且 tool_calls 为空时返回 `None`
     - `User`/`System`/`Developer` 分支：保持返回 `Some(...)`
   - 原因：过滤后的 Tool 消息缺少 `tool_call_id` 会导致 MiniMax 等 API 报错
   - 依赖：无
   - 风险：中 — 需确保不误丢有效消息

2. **将 `transcript_to_messages` 改为使用 `filter_map`**（文件：`crates/xclaw-agent/src/session.rs`，第 69-77 行）
   - 操作：`.map(|r| record_to_message(r, filter)).collect()` → `.filter_map(|r| record_to_message(r, filter)).collect()`
   - 依赖：步骤 1

### 阶段 2：测试更新与新增（1 个文件）

3. **修复现有测试 `filter_excludes_tool_result_from_tool_role_record`**
   - 操作：改为断言 `msgs.is_empty()`（旧预期就是 bug 本身）

4. **新增 `filter_drops_empty_assistant_message`**
   - 只含 `ToolCall` 块的 assistant 记录 + `{Text}` filter → 返回空 vec

5. **新增 `filter_text_only_drops_tool_cycle_preserves_text`**
   - 多轮对话 [User(text) → Assistant(text+tool_call) → Tool(tool_result) → Assistant(text)]
   - `{Text}` filter → [User, Assistant(text only), Assistant(text only)]，Tool 记录被丢弃

6. **新增 `filter_with_tool_kinds_preserves_tool_cycle`**
   - `{Text, ToolCall, ToolResult}` filter → 所有消息保留，tool_call_id 不为 None

7. **新增 `empty_filter_preserves_tool_messages`**
   - 空 filter → Tool 记录生成有效 `Role::Tool` 消息，tool_call_id 不为 None

## 风险与缓解

- **风险**：丢弃 Tool 记录后上游 provider 配对检查
  - 缓解：ToolCall 不在 filter 中时 assistant 的 tool_calls 已为空，不会产生配对不一致
- **风险**：assistant 同时有 Text 和 ToolCall，过滤后仅保留 Text 丢失上下文
  - 缓解：这是 `history_content_kinds` 的设计意图

## 成功标准

- [ ] `TranscriptRole::Tool` 在 `ToolResult` 不在 filter 中时不生成消息
- [ ] `TranscriptRole::Assistant` 过滤后内容全空时不生成消息
- [ ] 空 filter 行为不变
- [ ] `cargo test -p xclaw-agent` 通过
- [ ] `cargo clippy -- -D warnings` 无警告
