# 2026-03-31: TranscriptRecord v2

## 变更类型
设计提案

## 变更内容
- 新增 ADR-007: TranscriptRecord v2 结构化消息记录
- 新增设计文档: transcript-record-v2.md

## 关键决策
- TranscriptRecord 增加 id (nanoid) 和 parent_id 形成消息回复链
- role 从裸字符串改为 TranscriptRole 枚举 (System/User/Assistant/Tool/Developer)
- content 从 String 扩展为 Vec<ContentBlock>，支持 Text/Thinking/ToolCall/ToolResult/Image/Unknown
- 提升 model、stop_reason、usage 为一等字段
- 向后兼容旧 JSONL 格式

## 影响的 crate
- xclaw-memory（类型定义）
- xclaw-agent（转换逻辑）
