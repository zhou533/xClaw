# 评估：load_transcript_tail 返回类型对 LLM 消费场景的适配性

> 日期：2026-04-04
> 状态：评估完成，结论为保持现有架构

## 背景

用户提出改造 `SessionStore::load_transcript_tail`：
1. 支持按 ContentBlock 类型多选过滤
2. 返回值从 `Vec<TranscriptRecord>` 改为 `Vec<String>`

经评估，返回的字符串数组需要给 LLM 处理（注入 prompt 作为历史上下文），因此需要评估格式是否满足 LLM 消费需求。

## 当前架构（已正确工作）

```
load_transcript_tail() → Vec<TranscriptRecord>
    ↓
ChatRequestBuilder::with_history()
    ↓ 调用 transcript_to_messages()
    ↓
Vec<Message> → 注入 ChatRequest.messages
```

- `transcript_to_messages()`（`crates/xclaw-agent/src/session.rs`）精确映射 `TranscriptRole` → `Role`
- `text_content()` 自动跳过 Thinking 块
- Tool call/result 通过 `call_id` 保持配对

## 提议的 `Vec<String>` 格式

```
[{timestamp}] {role} ({content_type}): {formatted_body}
```

## 逐项评估

### 1. 角色保留 — 当前满足，Vec<String> 不满足

LLM API（OpenAI/Claude）要求 messages 数组中每条消息有明确 `role` 字段。当前 `transcript_to_messages()` 对 `TranscriptRole` 做穷尽 match：

- User → Role::User
- Assistant → Role::Assistant（含 tool_calls 提取）
- Tool → Role::Tool（含 call_id 提取）
- System → Role::System
- Developer → Role::Developer

改为纯字符串后需要在消费端重新解析 role——先序列化再反序列化是反模式。

### 2. Token 效率 — 当前满足，Vec<String> 不满足

当前实现不向 LLM 发送 timestamp、model、usage、provider 等元数据。`[2026-04-04T10:00:00Z]` 占约 15 tokens，50 条历史 = 750 tokens 浪费。

### 3. 工具调用配对 — 当前满足，Vec<String> 不满足

当前实现完整保留 tool_call → tool_result 的 call_id 关联：
- Assistant 记录中 `ContentBlock::ToolCall { call_id, name, arguments }` → `ToolCall { id, function }`
- Tool 记录中 `ContentBlock::ToolResult { call_id, content }` → `Message { role: Tool, tool_call_id }`

纯文本格式丢失此结构化关联。

### 4. Thinking 过滤 — 当前满足

`text_content()` 只收集 `ContentBlock::Text`，自动跳过 Thinking 块。测试 `thinking_blocks_filtered_in_replay` 验证了这一点。

### 5. Image 处理 — 两种方案均不满足

`Message.content` 是 `Option<String>`，不支持 multimodal content array。需独立修复 `Message` 类型以支持结构化 content blocks。

## 评估矩阵

| 维度 | Vec<TranscriptRecord> | Vec<String> |
|---|---|---|
| 角色保留 | 满足 | 不满足 |
| Token 效率 | 满足 | 不满足 |
| 工具调用配对 | 满足 | 不满足 |
| Thinking 过滤 | 满足 | 部分满足 |
| Image 处理 | 不满足 | 不满足 |

## 结论

**当前设计（返回 `Vec<TranscriptRecord>` + `transcript_to_messages` 转换）是正确的架构选择。不应改为 `Vec<String>`。**

`Vec<String>` 适合的场景是人类可读的日志展示/debug 输出/CLI 回放，不适合 LLM 注入。

## 建议后续方向

1. **`load_transcript_tail` 保持返回 `Vec<TranscriptRecord>`** — 存储层的正确职责
2. **如需内容过滤**，在 `transcript_to_messages` 转换层添加 `ContentBlockKind` 过滤逻辑
3. **如需人类可读格式**，新增独立函数 `format_transcript_for_display()`
4. **Image 支持**需独立修复 `Message` 类型（扩展为 `MessageContent` 枚举支持 multimodal）
