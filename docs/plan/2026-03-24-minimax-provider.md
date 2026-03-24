# 实施计划：MiniMax Provider 接口对接

## 需求重述

在 `xclaw-provider` 模块中新增 `MiniMaxProvider`，实现 `LlmProvider` trait，对接 MiniMax 的 Chat Completions API。要求：
- 符合 MiniMax API 接口定义
- 满足 `LlmProvider` trait 抽象（`name`、`chat`、`chat_stream`、`list_models`）
- 保持对后续其他 LLM provider 的扩展兼容

## 关键发现

**MiniMax API 是 OpenAI 兼容的。** 官方文档明确指出可以直接使用 OpenAI SDK，只需将 `OPENAI_BASE_URL` 设为 `https://api.minimax.io/v1`。这意味着：
- 相同的 `/chat/completions` 端点
- 相同的请求/响应 JSON 格式（含 tool calling）
- 相同的 SSE 流式格式
- Bearer Token 认证

## 实施方案

### 推荐方案：组合复用 `OpenAiProvider`

由于 MiniMax 完全兼容 OpenAI 协议，`MiniMaxProvider` 内部组合一个 `OpenAiProvider`（base_url 为 `https://api.minimax.io/v1`），仅在以下方面做差异化处理：

1. `name()` 返回 `"minimax"`
2. `list_models()` 返回硬编码模型列表（MiniMax 无 `/models` 端点）
3. 保留未来扩展 MiniMax 特有参数的能力（如 `mask_sensitive_info`）

## 实施阶段

### 阶段 1：创建 `minimax.rs` 模块骨架
- 新建 `crates/xclaw-provider/src/minimax.rs`
- 定义 `MiniMaxProvider` 结构体，内部持有 `OpenAiProvider`
- `new()` 构造函数：接收 `api_key` 和可选 `base_url`（默认 `https://api.minimax.io/v1`）

### 阶段 2：实现 `LlmProvider` trait
- `name()` → `"minimax"`
- `chat()` → 委托给内部 `OpenAiProvider::chat()`
- `chat_stream()` → 委托给内部 `OpenAiProvider::chat_stream()`
- `list_models()` → 返回硬编码模型列表：`MiniMax-M1`、`MiniMax-M2`、`MiniMax-M2.1`

### 阶段 3：注册到模块系统
- 在 `lib.rs` 中添加 `pub mod minimax;` 和 re-export

### 阶段 4：编写测试
- 单元测试：`name()`、构造函数、`list_models()`
- 集成测试（mockito）：`chat()` 正常响应、错误映射、`chat_stream()` SSE 解析

## 风险点

| 风险 | 级别 | 说明 | 应对 |
|------|------|------|------|
| **响应缺少 `id` 字段** | 中 | MiniMax 原生 API 示例中 response 无 `id` 字段，而 `ChatResponse` 要求 `id: String` | MiniMax 的 OpenAI 兼容端点应包含 `id`；如果不包含，需在 serde 层添加 `#[serde(default)]` 或在转换层生成默认值 |
| **无 `/models` 端点** | 低 | MiniMax 没有标准的模型列表 API | `list_models()` 返回硬编码列表，后续可从配置或 API 更新 |
| **MiniMax 特有参数** | 低 | `mask_sensitive_info`、`stream_options.include_usage`、message `name` 字段等当前 trait 无法表达 | 当前不需要支持；如后续需要，可在 `ChatRequest` 中增加 `extra: Option<serde_json::Value>` 扩展字段，或在 `MiniMaxProvider` 层拦截并注入 |
| **trait 不是 dyn-compatible** | 低 | `LlmProvider` 使用 `impl Future` 而非 `async_trait`，无法做 `Box<dyn LlmProvider>` | 这是已知设计决策（`traits.rs:126-128` 注释），使用泛型或 `enum dispatch`；对 MiniMax 实现无影响 |
| **`Role::Developer` 映射** | 低 | MiniMax 文档仅提及 `system/user/assistant`，无 `developer` role | 当前 `OpenAiProvider` 已按原样传递 role 字符串，MiniMax 可能忽略未知 role 或报错；需实际验证 |

## 复杂度预估：低

- 由于完全复用 `OpenAiProvider`，核心实现约 50-80 行
- 测试约 150-200 行
- 预计 1 个文件新增 + 1 个文件修改

## 文件变更清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `crates/xclaw-provider/src/minimax.rs` | 新建 | MiniMaxProvider 实现 |
| `crates/xclaw-provider/src/lib.rs` | 修改 | 添加 `pub mod minimax` 和 re-export |
