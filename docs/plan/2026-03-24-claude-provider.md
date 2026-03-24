# 实施计划：ClaudeProvider — Anthropic Claude Messages API

## 概览

在 `xclaw-provider` crate 中实现 `ClaudeProvider`，遵循现有 `OpenAiProvider` 的架构模式（专有 serde 类型 + From 转换 + SSE 解析），同时处理 Claude Messages API 与 OpenAI Chat Completions API 之间的结构性差异。

## 需求重述

- 实现 `ClaudeProvider` struct，满足 `LlmProvider` trait 全部 4 个方法：`name()`, `chat()`, `chat_stream()`, `list_models()`
- 正确映射 Claude Messages API 的请求/响应格式到统一类型
- 处理 Claude 特有的 SSE 事件协议（6 种事件类型）
- 认证使用 `x-api-key` header + `anthropic-version` header
- 支持 tool use（`input_schema`、`tool_result` content blocks）
- 遵循 TDD 风格，测试先于实现
- 在 `lib.rs` 中导出 `ClaudeProvider`

---

## Trait 与 Claude API 的关键差异（风险分析）

| 差异点 | trait/统一类型 | Claude API | 映射策略 | 风险 |
|--------|---------------|-----------|---------|------|
| System message | `Role::System` 在 messages 数组中 | 顶级 `system` 参数 | 提取 System messages 合并到 `system` 字段 | **中** — 多个 system message 需合并 |
| Developer role | `Role::Developer` | 不存在 | 合并到 system prompt 中 | **中** — 语义损失 |
| Tool role | `Role::Tool` + `tool_call_id` | `tool_result` content block 在 user message 中 | 将连续 Tool messages 折叠为 user message 的 content blocks | **高** — 消息边界重组 |
| Tool 定义 | `parameters` 字段 | `input_schema` 字段 | 序列化时 rename | 低 |
| 响应结构 | `choices[]` 数组 | 单个 message（无 choices） | 包装为单元素 choices 数组 | 低 |
| stop_reason | `stop`/`tool_calls`/`length`/`content_filter` | `end_turn`/`tool_use`/`max_tokens`/`stop_sequence` | 枚举映射 | 低 |
| Usage | `prompt_tokens`/`completion_tokens`/`total_tokens` | `input_tokens`/`output_tokens`（无 total） | total = input + output | 低 |
| 流式协议 | OpenAI 风格 `data: {json}` + `[DONE]` | 多种 `event: xxx\ndata: {json}` | **有状态 SSE 解析器** | **高** |
| max_tokens | `Option<u32>` | **必填** | 缺省时设默认值 4096 | **中** |
| Content | `Option<String>` | `content: [{type, text}]` 数组 | 请求侧包装为 text block；响应侧拼接 | 低 |
| 认证 | Bearer token | `x-api-key` + `anthropic-version` | 构造器和 header 方法不同 | 低 |

---

## 实施阶段

### 阶段 1：Claude 专有 serde 类型与 From 转换

1. **定义 Claude 请求 serde 类型** — `ClaudeRequestMessage`, `ClaudeContentBlock`（text/tool_use/tool_result tagged enum）, `ClaudeToolDef`（`input_schema`）, `ClaudeChatRequest`（含顶级 `system`, `max_tokens`）
2. **定义 Claude 响应 serde 类型** — `ClaudeResponse`, `ClaudeResponseContentBlock`, `ClaudeUsage`（`input_tokens`/`output_tokens`）, `ClaudeStopReason` 枚举
3. **定义 Claude 流式 serde 类型** — `ClaudeStreamEvent` 覆盖 6 种事件类型及各 data payload 类型
4. **实现 From 转换：Claude 响应 -> 统一类型** — stop_reason 映射、usage 计算 total、content blocks 分离为 text + tool_calls、包装为单元素 choices
5. **实现消息转换：统一 Message -> Claude 请求格式** — `convert_messages()` 函数：提取 System/Developer 合并为 system 参数；Tool messages 折叠为 tool_result content blocks；Assistant + tool_calls 转为 tool_use content blocks

### 阶段 2：ClaudeProvider 核心实现

6. **实现 `ClaudeProvider` struct 与构造器** — `{ api_key, base_url, client, default_max_tokens }`，`x-api-key` + `anthropic-version: 2023-06-01` headers
7. **实现 HTTP 错误映射** — Claude 错误格式 `{"type":"error","error":{"type":"...","message":"..."}}` -> ProviderError
8. **实现 `name()` + `chat()` + `list_models()`**

### 阶段 3：流式实现

9. **实现有状态 Claude SSE 解析器** — 维护 `StreamState`（message_id, model, current_block_index），将 6 种事件类型转换为 `ChatStreamDelta` 流
10. **实现 `chat_stream()`** — POST with `stream: true`，使用 SSE 解析器消费字节流

### 阶段 4：集成导出

11. **更新 `lib.rs`** — 添加 `pub use claude::ClaudeProvider;`

---

## 测试策略

全部测试位于 `claude.rs` 顶部 `#[cfg(test)] mod tests`（遵循 OpenAI 实现的 TDD 模式）：

- **单元测试**：`convert_messages`（system 提取、Developer 处理、Tool 折叠）、From 转换（stop_reason、usage、响应）、SSE 行解析
- **集成测试（mockito）**：chat 正常/错误映射、header 验证、tool_calls、max_tokens 默认值、stream 文本/tool_use、list_models
- **覆盖率目标**：80%+

---

## 复杂度预估

| 阶段 | 复杂度 |
|------|--------|
| 阶段 1：serde 类型 + 转换 | 中-高（消息转换是核心难点） |
| 阶段 2：核心实现 | 中（遵循 OpenAI 模式） |
| 阶段 3：流式实现 | 高（有状态 SSE 解析器） |
| 阶段 4：集成导出 | 低 |

预估代码量：约 600-800 行（含测试）

---

## 成功标准

- [ ] `ClaudeProvider` 实现 `LlmProvider` trait 全部方法
- [ ] 文本聊天补全（非流式）正确工作
- [ ] 流式聊天补全正确产出 `ChatStreamDelta` 序列
- [ ] Tool use 请求/响应正确映射（定义、调用、结果）
- [ ] System/Developer messages 正确提取到 Claude `system` 参数
- [ ] Tool messages 正确折叠为 `tool_result` content blocks
- [ ] HTTP 错误正确映射到 `ProviderError` 变体
- [ ] `max_tokens` 缺省时使用合理默认值
- [ ] `list_models()` 正确返回模型列表
- [ ] 全部测试通过，覆盖率 80%+
- [ ] `ClaudeProvider` 在 `lib.rs` 中正确导出
