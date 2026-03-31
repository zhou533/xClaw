# 实施计划：TranscriptRecord V2 重构

> 日期：2026-03-31 | 复杂度：中 | 设计文档：[transcript-record-v2.md](../architecture/transcript-record-v2.md)

## 概览

将 `TranscriptRecord` 从扁平字符串结构升级为结构化枚举类型，新增 `RecordId`、`ContentBlock`、`TranscriptRole`、`TokenUsage`、`StopReason` 等类型，并同步更新 agent 层的转换函数与测试。不保留 V1 格式兼容性。

## 需求

- `role` 从 `String` 改为 `TranscriptRole` 枚举（System/User/Assistant/Tool/Developer）
- `content` 从 `String` 改为 `Vec<ContentBlock>`，支持 Text/Thinking/ToolCall/ToolResult/Image/Unknown
- 新增 `id: RecordId`（nanoid 8-10 字符）和 `parent_id: Option<RecordId>`（消息回复链）
- 新增 `model: Option<String>`、`usage: Option<TokenUsage>`、`stop_reason: Option<StopReason>`、`provider: Option<String>`
- 移除旧字段 `tool_call_id`、`tool_name`，信息内聚到 `ContentBlock` 变体中
- `TranscriptRecord` 保持 `#[derive(Serialize, Deserialize)]`
- 不保留 V1 向后兼容性（旧 JSONL 文件不兼容是预期行为）

## 实施步骤

### 阶段 1：新类型定义（xclaw-memory，3 个文件）

#### 步骤 1：添加 nanoid 依赖

- **文件**：`Cargo.toml` + `crates/xclaw-memory/Cargo.toml`
- **操作**：workspace `[workspace.dependencies]` 添加 `nanoid = "0.4"`，xclaw-memory 引用 `nanoid.workspace = true`
- **依赖**：无
- **风险**：低

#### 步骤 2：创建 record_id 模块

- **文件**：`crates/xclaw-memory/src/session/record_id.rs`
- **操作**：定义 `pub type RecordId = String`，实现 `pub fn generate_record_id() -> RecordId`（nanoid 8 字符，base62 字母表）
- **依赖**：步骤 1
- **风险**：低
- **TDD**：验证 ID 长度为 8、字符范围 `[a-zA-Z0-9]`、两次调用不重复

#### 步骤 3：重写 types.rs 类型定义

- **文件**：`crates/xclaw-memory/src/session/types.rs`
- **操作**：
  - 新增 `TranscriptRole` 枚举（System/User/Assistant/Tool/Developer），`#[serde(rename_all = "lowercase")]`
  - 新增 `ContentBlock` 枚举（Text/Thinking/ToolCall/ToolResult/Image/Unknown），`#[serde(tag = "type", rename_all = "snake_case")]`
  - 新增 `ImageSource` 枚举（Base64/Url），`#[serde(tag = "type", rename_all = "snake_case")]`
  - 新增 `TokenUsage` 结构体（input_tokens/output_tokens/total_tokens/thinking_tokens?/cache_read_tokens?）
  - 新增 `StopReason` 枚举（Stop/ToolCalls/Length/ContentFilter/Other(String)），`#[serde(rename_all = "snake_case")]`
  - 重写 `TranscriptRecord`：id、parent_id、role、content、timestamp、model、stop_reason、usage、provider、metadata
  - 所有类型 derive `Serialize, Deserialize, Debug, Clone`
- **依赖**：步骤 2
- **风险**：中
- **TDD**：所有新类型的 serde 往返测试；skip_serializing_if 行为验证

#### 步骤 4：更新 mod.rs 导出

- **文件**：`crates/xclaw-memory/src/session/mod.rs`
- **操作**：添加 `pub mod record_id;`，重新导出新类型
- **依赖**：步骤 2、3
- **风险**：低

### 阶段 2：转换函数重写（xclaw-agent，5 个文件）

#### 步骤 5：重写 session.rs 构建函数

- **文件**：`crates/xclaw-agent/src/session.rs`
- **操作**：
  - `user_input_to_transcript(content)` → `TranscriptRole::User`，`vec![ContentBlock::Text { text }]`，生成 `RecordId`
  - `assistant_output_to_transcript(content)` → `TranscriptRole::Assistant`，同上
  - `tool_result_to_transcript(call_id, tool_name, output, parent_id)` → `TranscriptRole::Tool`，`vec![ContentBlock::ToolResult { .. }]`，设置 `parent_id`
  - `response_to_transcript(response)` → 从 `ChatResponse` 构建，提取 model/usage/stop_reason，tool_calls 转为 `ContentBlock::ToolCall`
  - `transcript_to_messages(records)` → 从新类型映射回 `Message`，过滤 Thinking blocks
- **依赖**：步骤 3、4
- **风险**：高 — 核心转换逻辑
- **TDD**：每个函数的输入输出验证；transcript_to_messages 多轮对话顺序保持；Thinking blocks 过滤

#### 步骤 6：添加 From trait 转换

- **文件**：`crates/xclaw-agent/src/session.rs`
- **操作**：
  - `impl From<FinishReason> for StopReason`
  - `impl From<Usage> for TokenUsage`
- **依赖**：步骤 3
- **风险**：低
- **TDD**：每个变体的映射测试

#### 步骤 7：更新 engine.rs 调用签名

- **文件**：`crates/xclaw-agent/src/engine.rs`
- **操作**：`response_to_transcript` 返回值中提取 `RecordId`，传递给 `tool_result_to_transcript` 的 `parent_id`；在 `run_tool_loop` 中追踪 last_assistant_record_id
- **依赖**：步骤 5
- **风险**：中 — parent_id 传递链

#### 步骤 8：更新 prompt.rs

- **文件**：`crates/xclaw-agent/src/prompt.rs`
- **操作**：适配新 `TranscriptRecord` 字段访问
- **依赖**：步骤 3
- **风险**：低

#### 步骤 9：更新 test_support.rs

- **文件**：`crates/xclaw-agent/src/test_support.rs`
- **操作**：更新 `TranscriptRecord` 构造方式
- **依赖**：步骤 3
- **风险**：低

### 阶段 3：测试迁移与验证

#### 步骤 10：更新 fs_store_tests.rs

- **文件**：`crates/xclaw-memory/src/session/fs_store_tests.rs`
- **操作**：`record()` helper 改用新类型构造；断言改用枚举匹配
- **依赖**：步骤 3
- **风险**：中

#### 步骤 11：更新 session_integration.rs

- **文件**：`crates/xclaw-memory/tests/session_integration.rs`
- **操作**：`make_record()` helper 改用新格式；反序列化验证适配新结构
- **依赖**：步骤 3
- **风险**：低

#### 步骤 12：更新 agent 层测试

- **文件**：`crates/xclaw-agent/src/session.rs` tests + `crates/xclaw-agent/src/prompt.rs` tests
- **操作**：构造方式和断言全部适配新类型
- **依赖**：步骤 5、6
- **风险**：中

#### 步骤 13：运行全 workspace 测试

- **操作**：`cargo test` + `cargo clippy -- -D warnings` + `cargo fmt --check`
- **依赖**：步骤 10-12
- **风险**：低

### 阶段 4：清理（可选）

#### 步骤 14：升级时间戳为 ISO 8601

- **文件**：`crates/xclaw-agent/src/session.rs`
- **操作**：替换 `unix_secs_now()` 为 ISO 8601 格式
- **风险**：低

#### 步骤 15：添加辅助方法

- **文件**：`crates/xclaw-memory/src/session/types.rs`
- **操作**：`text_content()`, `tool_calls()`, `has_tool_calls()`, `new_user()`, `new_assistant()`
- **风险**：低

## 风险与缓解

| 风险 | 级别 | 缓解措施 |
|------|------|---------|
| ContentBlock serde 标签设计不当 | 中 | 先写 serde 往返测试，确认 JSON 格式后再写实现 |
| transcript_to_messages 遗漏 ContentBlock 变体 | 中 | 使用穷尽 match，编译器强制覆盖所有变体 |
| engine.rs 中 parent_id 传递链断裂 | 中 | 集成测试验证 tool loop 场景 parent_id 非 None |
| 旧 JSONL 文件不兼容 | 预期 | 设计文档明确说明，用户需清除旧会话数据 |

## 成功标准

- [ ] TranscriptRecord 使用 TranscriptRole 枚举替代字符串 role
- [ ] content 为 Vec<ContentBlock>，支持六种变体
- [ ] 每条记录有唯一 RecordId，tool_result 通过 parent_id 关联 assistant
- [ ] model/usage/stop_reason 为一等字段且 assistant 记录正确填充
- [ ] 所有转换函数正确处理新类型
- [ ] `cargo test` 全量通过
- [ ] `cargo clippy -- -D warnings` 无警告
