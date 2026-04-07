# 实施计划：移除 history_content_kinds 可配置过滤器，硬编码排除 Thinking

## 需求

- 加载 history 时始终包含 Text + ToolCall + ToolResult
- 始终排除 Thinking（以及 Image、Unknown）
- 从 `AgentConfig` 中移除 `history_content_kinds` 字段及相关 builder/serde 逻辑
- `session.rs` 中 `transcript_to_messages` 不再接受外部 filter 参数，改为内部硬编码
- `prompt.rs` 中 `with_history_filtered` 方法移除，统一为 `with_history`

## 实施步骤

### 阶段 1：移除配置字段 (`config.rs`)

1. 删除 `history_content_kinds` 字段、`default_history_content_kinds()` 函数、`with_history_content_kinds()` builder 方法
2. 删除相关 import（`BTreeSet`、`ContentBlockKind`）
3. 删除 4 个相关测试：
   - `new_defaults_history_content_kinds_to_text_tool_call_tool_result`
   - `with_history_content_kinds_sets_filter`
   - `serde_missing_history_content_kinds_uses_default`
   - `serde_explicit_history_content_kinds_roundtrip`

### 阶段 2：硬编码过滤逻辑 (`session.rs`)

4. 修改 `transcript_to_messages`、`record_to_message` 签名：移除 `filter` 参数
5. 将 `block_passes` 改为硬编码：`!matches!(kind, Thinking | Image | Unknown)`
6. 修改 `filtered_text_content`：移除 `filter` 参数，始终包含 Text
7. 更新测试：
   - 删除不再适用的 filter 可配置测试
   - 新增 `thinking_blocks_excluded_from_history`
   - 新增 `text_tool_call_tool_result_all_preserved`
   - 其余测试仅移除 filter 参数

### 阶段 3：更新调用方 (`prompt.rs` + `engine.rs`)

8. 删除 `with_history_filtered` 方法，简化 `with_history` 不再传 filter
9. engine.rs 调用改为 `.with_history(&history)`
10. 重写 engine.rs 中 `history_uses_configured_content_kinds` → `history_excludes_thinking_blocks`

## 风险

- 低：纯删除 + 简化操作，编译器捕获所有断裂引用
- 仅 engine.rs 一处调用 `with_history_filtered`

## 成功标准

- [x] `AgentConfig` 不再包含 `history_content_kinds` 字段
- [x] `transcript_to_messages` 不再接受 `filter` 参数
- [x] `with_history_filtered` 方法已移除
- [x] Thinking block 被排除，Text/ToolCall/ToolResult 全部保留
- [x] `cargo build` + `cargo test -p xclaw-agent` + `cargo clippy -- -D warnings` 全部通过
