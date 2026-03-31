# ADR-007: TranscriptRecord v2 — 结构化消息记录

> 日期：2026-03-31 | 状态：Proposed

## 背景

当前 `TranscriptRecord` 使用裸字符串 role、单一 String content、metadata 万能口袋。无法表达 tool_call/thinking 等复合内容块，缺少消息标识和回复链，Usage/model/finishReason 无类型安全。

## 决策

将 `TranscriptRecord` 升级为 v2 结构化格式：

1. **ID 体系**：nanoid（8-10 字符）+ parent_id 形成消息回复链
2. **TranscriptRole 枚举**：System/User/Assistant/Tool/Developer，独立于 provider Role
3. **Vec\<ContentBlock\>**：Text/Thinking/ToolCall/ToolResult/Image/Unknown 六种变体
4. **一等元数据字段**：model、stop_reason（StopReason enum）、usage（TokenUsage struct）、provider
5. **向后兼容**：自定义反序列化器处理旧格式（String content -> Vec\<ContentBlock\>）

## 备选方案

1. **直接复用 provider Message 类型**：否决 — memory 层不应依赖 provider 层
2. **serde_json::Value 作为 content**：否决 — 无类型安全
3. **UUID 作为 ID**：否决 — 36 字符太长，nanoid 8-10 字符足够
4. **移除 metadata 字段**：否决 — 保留适度扩展性

## 影响

- `xclaw-memory/src/session/types.rs`：类型定义重写
- `xclaw-agent/src/session.rs`：转换函数重写
- 现有 JSONL 文件通过兼容反序列化器无损读取

## 详细设计

见 [transcript-record-v2.md](../transcript-record-v2.md)
