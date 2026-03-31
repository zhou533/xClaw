# TranscriptRecord v2 设计

> 日期：2026-03-31 | 状态：Proposed

## 1. 背景

当前 `TranscriptRecord`（`xclaw-memory/src/session/types.rs`）使用裸字符串 role、单一 String content、以及 metadata 万能口袋。存在以下问题：

- 无消息标识，无法建立回复链
- role 是裸字符串，无类型安全
- content 无法表达 tool_call、thinking 等复合内容块
- Usage/model/finishReason 散落在 metadata 中

## 2. 设计目标

1. 增加 `id` 和 `parent_id`，建立消息继承链
2. 增加类型安全的 `TranscriptRole` 枚举
3. 将 `content` 扩展为 `Vec<ContentBlock>`，统一表达多种内容块
4. 提升 Usage、model、stop_reason 为一等字段
5. 保持向后兼容（旧 JSONL 文件可无损读取）

## 3. 类型定义

### 3.1 RecordId

8-10 字符 nanoid（base62 字母表），碰撞概率极低（62^8 = 218 万亿）。选择 nanoid 而非 UUID（太长）或自增 ID（需读取历史）。

```rust
pub type RecordId = String;
```

### 3.2 TranscriptRole

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TranscriptRole {
    System,
    User,
    Assistant,
    Tool,
    Developer,
}
```

独立于 `xclaw-provider::types::Role` 定义在 memory crate 中，通过 `From` trait 互转。memory 是存储层，不应依赖 provider（通信层）。

### 3.3 ContentBlock

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },

    Thinking {
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        thinking_id: Option<String>,
    },

    ToolCall {
        call_id: String,
        name: String,
        arguments: String,
    },

    ToolResult {
        call_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        content: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        is_error: bool,
    },

    Image {
        media_type: String,
        source: ImageSource,
    },

    Unknown {
        original_type: String,
        data: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSource {
    Base64 { data: String },
    Url { url: String },
}
```

#### 类型完整性评估

| 类型 | OpenAI | Claude | MiniMax | 必要性 |
|------|--------|--------|---------|--------|
| `Text` | content 字段 | text block | content 字段 | 必须 |
| `Thinking` | o1/o3 reasoning_content | thinking block | 无 | 前瞻必须 |
| `ToolCall` | tool_calls[] | tool_use block | tool_calls[] | 必须 |
| `ToolResult` | role=tool message | tool_result block | role=tool message | 必须 |
| `Image` | image_url content part | image block | image_url content part | 建议（多模态趋势） |
| `Unknown` | -- | -- | -- | 必须（前向兼容） |

暂不纳入的类型：Audio/Video（格式未稳定）、Citation（频率低）、RedactedThinking（可后续扩展）。均由 `Unknown` 承接。

### 3.4 TokenUsage

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
}
```

字段名用 `input_tokens`/`output_tokens` 而非 `prompt_tokens`/`completion_tokens`，因前者更通用（Claude 原生使用，OpenAI 的 prompt/completion 是遗留命名）。

### 3.5 StopReason

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
    Other(String),
}
```

`Other(String)` 兜底，避免未来新增 stop reason 导致反序列化失败。

### 3.6 完整 TranscriptRecord

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptRecord {
    // ── Identity ──
    pub id: RecordId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<RecordId>,

    // ── Message ──
    pub role: TranscriptRole,
    pub content: Vec<ContentBlock>,
    pub timestamp: String,  // ISO 8601

    // ── Model metadata (assistant turns) ──
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<TokenUsage>,

    // ── Provider lineage ──
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,

    // ── Extensibility ──
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, serde_json::Value>,
}
```

## 4. Provider 映射

### 4.1 assistant turn (ChatResponse -> TranscriptRecord)

| Provider 字段 | TranscriptRecord 字段 |
|---|---|
| `response.id` | `metadata["provider_message_id"]` |
| `response.model` | `model` |
| `choice.finish_reason` | `stop_reason` (via `From`) |
| `response.usage` | `usage` (via `From`) |
| `choice.message.content` | `content: [ContentBlock::Text { text }]` |
| `choice.message.tool_calls` | `content: [.., ContentBlock::ToolCall { .. }]` |
| Claude thinking block | `content: [ContentBlock::Thinking { .. }]` |

### 4.2 user turn

| 输入 | TranscriptRecord 字段 |
|---|---|
| 用户文本 | `content: [ContentBlock::Text { text }]` |
| `role` | `TranscriptRole::User` |
| model/usage/stop_reason | `None` |

### 4.3 tool result turn

| 输入 | TranscriptRecord 字段 |
|---|---|
| tool output | `content: [ContentBlock::ToolResult { .. }]` |
| `role` | `TranscriptRole::Tool` |
| `parent_id` | 指向发出 ToolCall 的 assistant record |

### 4.4 回放到 LLM

- Thinking blocks 在回放时应**过滤掉**（LLM 不接受用户注入 thinking）
- Text blocks 合并为 content 字符串
- ToolCall blocks 映射为 provider ToolCall 结构
- ToolResult blocks 映射为 role=tool message

## 5. 被删除字段的处理

> 注意：不保留 V1 向后兼容性。旧 JSONL 文件不兼容是预期行为，用户需清除旧会话数据。

| 旧字段 | 处理 |
|--------|------|
| `tool_call_id: Option<String>` | 移入 `ContentBlock::ToolResult.call_id` |
| `tool_name: Option<String>` | 移入 `ContentBlock::ToolResult.name` / `ContentBlock::ToolCall.name` |
| `metadata["tool_calls"]` | 不再需要，直接作为 `ContentBlock::ToolCall` |
| `metadata["finish_reason"]` | 提升为一等字段 `stop_reason` |

## 6. 影响范围

| 文件 | 变更类型 |
|------|---------|
| `crates/xclaw-memory/src/session/types.rs` | 重写类型定义 |
| `crates/xclaw-agent/src/session.rs` | 重写转换函数 |
| `crates/xclaw-agent/src/engine.rs` | parent_id 传递 |
| `crates/xclaw-agent/src/prompt.rs` | 可能更新 |
| `crates/xclaw-agent/src/test_support.rs` | 更新测试 fixtures |
| `crates/xclaw-memory/src/session/fs_store_tests.rs` | 更新测试 |
| `crates/xclaw-memory/tests/session_integration.rs` | 更新集成测试 |

建议实施顺序：
1. 在 `xclaw-memory` 中定义新类型
2. 在 `xclaw-agent/src/session.rs` 中实现新转换函数
3. 迁移测试
4. 后续独立 PR：扩展 Claude provider 支持 thinking block

## 7. 字段命名取舍

| 决策 | 选择 | 理由 | 备选 |
|------|------|------|------|
| id 格式 | 8-10 char nanoid | 无需读历史，人眼可读 | UUID（太长）、自增 u64（需读历史） |
| `parent_id` vs `reply_to` | `parent_id` | 树状结构语义清晰 | `reply_to`（暗示仅双向） |
| role 类型 | 独立枚举 | memory 不依赖 provider | 复用 provider Role（反向依赖） |
| `stop_reason` vs `finish_reason` | `stop_reason` | 与 Claude 一致，语义直观 | `finish_reason`（OpenAI 命名） |
| `input_tokens` vs `prompt_tokens` | `input_tokens` | 更通用 | `prompt_tokens`（OpenAI 遗留） |
| timestamp 格式 | ISO 8601 | 人可读，含时区 | Unix 秒（当前格式） |
