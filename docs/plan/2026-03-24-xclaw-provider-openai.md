# 实施计划：xclaw-provider OpenAI 接口对接

> 创建日期：2026-03-24
> 状态：已确认

## 需求重述

在 `xclaw-provider` crate 中实现：

1. **通用 LLM Provider trait 抽象** — 定义 `LlmProvider` trait 及相关类型，使 OpenAI / Claude / Ollama 等后端可以统一接口对接
2. **OpenAI Chat Completions 实现** — 对接 `POST /v1/chat/completions`，支持普通请求和 SSE 流式响应，支持 tool_calls（function calling）
3. **类型定义** — 符合 OpenAI API 规范的请求/响应结构体（Message, Choice, Usage, ToolCall 等）

## 现状分析

- `xclaw-provider` crate 已存在，所有模块均为空骨架（仅 doc comment）
- Rust edition 2024 — 原生支持 `async fn in trait`，无需 `async-trait` 宏
- 已有依赖：serde, serde_json, tokio, anyhow, thiserror, tracing, xclaw-config
- **缺少依赖**：`reqwest`（HTTP 客户端）、`futures`（Stream trait）、`tokio-stream`（流式处理）

## 实施阶段

### 阶段 1：添加依赖（Cargo.toml）

workspace 级别添加：

- `reqwest = { version = "0.12", features = ["json", "stream"] }`
- `futures = "0.3"`
- `tokio-stream = "0.1"`

xclaw-provider 的 Cargo.toml 引入这三个依赖。

### 阶段 2：通用类型定义（新建 `types.rs`）

定义 provider 无关的统一类型，所有 provider 的输入输出都转换为这些类型：

```rust
// 核心类型（provider 无关）
pub enum Role { System, User, Assistant, Tool, Developer }

pub struct Message {
    pub role: Role,
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

pub struct FunctionCall {
    pub name: String,
    pub arguments: String,  // JSON string
}

pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,  // JSON Schema
}

pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub stream: bool,
}

pub struct ChatResponse {
    pub id: String,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

pub struct Choice {
    pub index: u32,
    pub message: Message,
    pub finish_reason: Option<FinishReason>,
}

pub enum FinishReason { Stop, ToolCalls, Length, ContentFilter }

pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// 流式 chunk
pub struct ChatStreamDelta {
    pub id: String,
    pub model: String,
    pub choices: Vec<DeltaChoice>,
    pub usage: Option<Usage>,
}

pub struct DeltaChoice {
    pub index: u32,
    pub delta: DeltaMessage,
    pub finish_reason: Option<FinishReason>,
}

pub struct DeltaMessage {
    pub role: Option<Role>,
    pub content: Option<String>,
    pub tool_calls: Vec<DeltaToolCall>,
}

pub struct DeltaToolCall {
    pub index: u32,
    pub id: Option<String>,
    pub function: Option<DeltaFunctionCall>,
}

pub struct DeltaFunctionCall {
    pub name: Option<String>,
    pub arguments: Option<String>,
}

// 模型信息
pub struct ModelInfo {
    pub id: String,
    pub owned_by: String,
    pub created: i64,
}
```

### 阶段 3：错误类型（新建 `error.rs`）

```rust
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("authentication failed: {0}")]
    Auth(String),

    #[error("rate limited")]
    RateLimit { retry_after: Option<std::time::Duration> },

    #[error("invalid request: {0}")]
    InvalidRequest(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("server error (status {status}): {body}")]
    ServerError { status: u16, body: String },

    #[error("stream closed unexpectedly")]
    StreamClosed,

    #[error("deserialization error: {0}")]
    Deserialize(String),
}
```

### 阶段 4：Trait 抽象（`traits.rs`）

```rust
use std::pin::Pin;
use futures::Stream;

pub trait LlmProvider: Send + Sync {
    /// Provider 名称标识（如 "openai", "claude", "ollama"）
    fn name(&self) -> &str;

    /// 非流式 Chat Completions 请求
    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, ProviderError>;

    /// 流式 Chat Completions 请求，返回 Stream
    async fn chat_stream(
        &self,
        request: &ChatRequest,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<ChatStreamDelta, ProviderError>> + Send>>,
        ProviderError,
    >;

    /// 列出可用模型
    async fn list_models(&self) -> Result<Vec<ModelInfo>, ProviderError>;
}
```

### 阶段 5：OpenAI 实现（`openai.rs`）

分为以下部分：

1. **OpenAI 专用 serde 类型** — 与 OpenAI API JSON 完全对应的结构体，用于 HTTP 序列化/反序列化
2. **类型转换** — OpenAI serde 类型 ↔ 通用类型的 `From`/`Into` impl
3. **`OpenAiProvider` 结构体** — 实现 `LlmProvider` trait：
   - 构造函数接受 `api_key`、`base_url`（默认 `https://api.openai.com/v1`）、可选 `organization`
   - `chat()` — POST JSON 到 `/chat/completions`，解析响应并转换为通用类型
   - `chat_stream()` — POST 带 `stream: true`，手动解析 SSE（`data: {...}` / `data: [DONE]`），逐 chunk 转换为 `ChatStreamDelta`
   - `list_models()` — GET `/models`
   - 错误码映射：401→Auth, 429→RateLimit, 4xx→InvalidRequest, 5xx→ServerError

### 阶段 6：模块注册与导出（`lib.rs`）

- 新增 `pub mod types;` 和 `pub mod error;`
- 重新导出核心类型和 trait

## 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `Cargo.toml`（workspace） | 修改 | 添加 reqwest, futures, tokio-stream |
| `crates/xclaw-provider/Cargo.toml` | 修改 | 引入新依赖 |
| `crates/xclaw-provider/src/types.rs` | **新建** | 通用类型定义 |
| `crates/xclaw-provider/src/error.rs` | **新建** | ProviderError 定义 |
| `crates/xclaw-provider/src/traits.rs` | 重写 | LlmProvider trait |
| `crates/xclaw-provider/src/openai.rs` | 重写 | OpenAI 实现 |
| `crates/xclaw-provider/src/lib.rs` | 修改 | 添加模块，重新导出 |

## 依赖关系

```
types.rs ← error.rs ← traits.rs ← openai.rs
                                  ← claude.rs（后续）
                                  ← ollama.rs（后续）
```

## 风险评估

| 风险 | 级别 | 应对 |
|------|------|------|
| SSE 流式解析边界情况（partial chunks, 多行 data） | 中 | 手动逐行解析 SSE，不依赖第三方 SSE crate，保持可控 |
| OpenAI API 版本变化（tool_calls 格式演进） | 低 | serde 使用 `#[serde(default)]`，容忍未知字段 |
| `base_url` 可自定义，兼容 Azure OpenAI / 第三方代理 | 低 | 构造函数接受 `base_url` 参数 |
| edition 2024 的 async trait 兼容性 | 低 | 已确认 Rust edition 2024 原生支持 |
| `tool_calls[].function.arguments` 可能是无效 JSON | 低 | 保持为 String 类型，不自动解析 |

## 可扩展性设计

- `LlmProvider` trait 是纯抽象的，Claude/Ollama 只需实现同一 trait
- 通用类型（`Message`, `ChatRequest` 等）是 provider 无关的中间层
- 每个 provider 内部维护自己的 serde 类型并实现 `From` 转换
- `router.rs` 后续可基于 `Box<dyn LlmProvider>` 实现路由和 failover

## 复杂度预估：中

## OpenAI API 关键细节备注

- **认证**：`Authorization: Bearer $OPENAI_API_KEY`
- **`tool_calls[].function.arguments`** 是 JSON 字符串，非解析对象
- **流式 `delta`** 中所有字段都是 `Option`，每个 chunk 只携带增量部分
- **`Usage`** 在流式模式下仅在设置 `stream_options.include_usage: true` 时出现在最后一个 chunk
- **`finish_reason`** 在流式 chunk 中为 `null`，仅最终 chunk 有值
- **SSE 格式**：`data: {json}\n\n`，终止符为 `data: [DONE]`
- **Role** 包含：`system`, `user`, `assistant`, `tool`, `developer`
