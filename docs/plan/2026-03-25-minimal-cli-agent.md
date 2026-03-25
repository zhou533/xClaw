# 实施计划：最小化 CLI + Agent 一问一答交互

> 日期：2026-03-25 | 状态：Approved

## 概览

实现从 CLI 到 LLM 回复的最短路径：`xclaw chat "你好"` → Agent 调用 Provider → stdout 打印回复。共涉及 3 个 crate 的 7 个文件改动。

## 需求

- CLI 支持 `xclaw chat "消息"` 单次对话命令
- 通过环境变量 `XCLAW_PROVIDER`（openai/claude/minimax）选择 provider
- 通过环境变量 `XCLAW_API_KEY` 提供 API Key
- 通过环境变量 `XCLAW_MODEL` 指定模型（可选，有默认值）
- Agent 实现 `AgentLoop` trait，将用户消息封装为 `ChatRequest` 发送给 provider
- 返回 LLM 文本回复并打印到 stdout
- 不实现工具调用、记忆、技能、流式输出

## 实施步骤

### 阶段 1：配置层（xclaw-config，2 个文件）

#### 步骤 1：定义配置数据结构

- **文件**：`crates/xclaw-config/src/model.rs`
- **操作**：
  - 定义 `ProviderKind` 枚举：`OpenAi`, `Claude`, `MiniMax`
  - 为 `ProviderKind` 实现 `FromStr`（接受 `"openai"`, `"claude"`, `"minimax"`）
  - 定义 `ProviderConfig` 结构体：`kind`, `api_key`, `base_url: Option<String>`, `model`, `organization: Option<String>`
  - 定义 `AppConfig` 结构体：`provider: ProviderConfig`
  - 默认模型常量：OpenAI → `gpt-4o`，Claude → `claude-sonnet-4-5-20250929`，MiniMax → `MiniMax-M2`
- **依赖**：无

#### 步骤 2：实现环境变量配置加载

- **文件**：`crates/xclaw-config/src/loader.rs`
- **操作**：
  - `pub fn load_from_env() -> Result<AppConfig, XClawError>`
  - 读取 `XCLAW_PROVIDER`（默认 `"openai"`）、`XCLAW_API_KEY`（必填）、`XCLAW_MODEL`（按 provider 选默认值）、`XCLAW_BASE_URL`（可选）、`XCLAW_ORGANIZATION`（可选）
- **依赖**：步骤 1

### 阶段 2：Agent 核心逻辑（xclaw-agent，3 个文件）

#### 步骤 3：实现 prompt 构建

- **文件**：`crates/xclaw-agent/src/prompt.rs`
- **操作**：
  - `pub fn build_chat_request(model: &str, user_content: &str) -> ChatRequest`
  - 包含 system message + user message，`stream: false`

#### 步骤 4：实现 SimpleAgent

- **文件**：`crates/xclaw-agent/src/loop.rs`
- **操作**：
  - `pub struct SimpleAgent<P: LlmProvider> { provider: P, model: String }`
  - 实现 `AgentLoop` trait：调用 `build_chat_request()` → `provider.chat()` → 提取回复
  - 空 choices 返回 `XClawError::Agent("empty response")`
  - `ProviderError` 映射为 `XClawError::Agent`

#### 步骤 5：更新 lib.rs 导出

- **文件**：`crates/xclaw-agent/src/lib.rs`
- **操作**：添加 `pub use r#loop::SimpleAgent;`

### 阶段 3：CLI 入口（1 个文件）

#### 步骤 6：实现 CLI 主程序

- **文件**：`apps/cli/src/main.rs`
- **操作**：
  - clap derive 定义 `Cli` + `Commands::Chat { message: String }`
  - 初始化 tracing，解析参数
  - `load_from_env()` 加载配置
  - match `ProviderKind` 构造对应 provider + `SimpleAgent` + 调用 `process()`
  - 注意：MiniMax 的 `new()` 返回 `Result`，需额外处理
  - 成功 `println!`，失败 `eprintln!` + 非零退出码

## 关键设计决策

- **泛型 vs dyn dispatch**：`LlmProvider` 使用 `impl Future` 返回类型，不是 dyn-safe。`SimpleAgent` 使用泛型 `<P: LlmProvider>`，CLI 中 match 分支分别构造具体类型
- **配置来源**：MVP 阶段仅环境变量，未来叠加 TOML 文件和 CLI 参数

## Provider 构造签名

| Provider | 构造器 | 返回类型 |
|----------|--------|----------|
| OpenAI | `OpenAiProvider::new(api_key, base_url, org)` | 直接值 |
| Claude | `ClaudeProvider::new(api_key, base_url)` | 直接值 |
| MiniMax | `MiniMaxProvider::new(api_key, base_url)` | `Result<Self, ProviderError>` |

## 测试策略

- **xclaw-config**: `ProviderKind::from_str` 解析三种 provider + 无效值、`load_from_env` 各变量映射和缺失校验
- **xclaw-agent**: stub provider 验证 `SimpleAgent::process`、空 choices 错误、provider 错误映射、`build_chat_request` 正确构建
- **E2E**: 手动验证 `XCLAW_API_KEY=<key> cargo run -p xclaw-cli -- chat "Hello"`

## 风险

| 风险 | 级别 | 缓解 |
|------|------|------|
| `LlmProvider` 非 dyn-safe | 中 | match 分支分别处理 |
| agent crate 依赖空壳 crate | 低 | 不 use 即不报错 |
| API key 缺失 | 低 | 清晰错误消息 + 非零退出码 |

## 成功标准

- [ ] `cargo build -p xclaw-cli` 编译通过
- [ ] `cargo test -p xclaw-config` 全部通过
- [ ] `cargo test -p xclaw-agent` 全部通过
- [ ] 三种 provider 均能正常完成一问一答
- [ ] 缺少 API key / 无效 provider 时显示友好错误
