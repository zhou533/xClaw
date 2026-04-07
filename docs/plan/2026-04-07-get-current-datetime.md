# 实施计划：get_current_datetime 工具

## 概览

在 xClaw 工具系统中新增 `get_current_datetime` 内置工具，返回宿主机本地日期时间。该工具无需文件系统访问，无需网络访问，属于纯计算型工具，复杂度极低。

## 需求

- 提供名为 `get_current_datetime` 的工具，LLM 可通过 function-calling 调用
- 返回宿主机本地时间（非 UTC），格式为 ISO 8601（如 `2026-04-07T14:30:00+08:00`）
- 支持可选参数 `format`，允许自定义输出格式（默认 ISO 8601）
- 支持可选参数 `timezone`，可选 `local`（默认）或 `utc`
- 不依赖文件系统或网络，不需要安全路径校验
- 遵循现有工具实现模式（`async_trait`、`serde` 参数解析、`ToolOutput`）

## 架构变更

- 新增文件：`crates/xclaw-tools/src/datetime.rs` — `GetCurrentDatetimeTool` 实现
- 修改文件：`crates/xclaw-tools/src/lib.rs` — 添加 `datetime` 模块声明与注册调用
- 新增依赖：`chrono` crate（workspace 级别），用于时区感知的日期时间格式化

## 实施步骤

### 阶段 1：依赖准备（1 个文件）

1. **添加 chrono 依赖**（文件：`Cargo.toml`（workspace 根）+ `crates/xclaw-tools/Cargo.toml`）
   - 操作：在 workspace `[workspace.dependencies]` 中添加 `chrono = { version = "0.4", default-features = false, features = ["clock", "std"] }`；在 `xclaw-tools/Cargo.toml` 的 `[dependencies]` 中添加 `chrono = { workspace = true }`
   - 原因：`std::time` 不提供时区感知格式化；`chrono` 是 Rust 生态中处理本地时间的标准 crate，且 `chrono::Local::now()` 直接返回宿主机本地时间
   - 依赖：无
   - 风险：低 — chrono 是成熟且广泛使用的 crate

### 阶段 2：工具实现（1 个新文件）

2. **创建 datetime 模块**（文件：`crates/xclaw-tools/src/datetime.rs`）
   - 操作：
     - 定义 `GetCurrentDatetimeParams` 结构体，包含可选字段 `format: Option<String>` 和 `timezone: Option<String>`
     - 实现 `GetCurrentDatetimeTool` 结构体
     - 为 `GetCurrentDatetimeTool` 实现 `Tool` trait（`#[async_trait]`）
       - `name()` → `"get_current_datetime"`
       - `description()` → `"Get the current date and time on the host machine."`
       - `parameters_schema()` → JSON Schema 描述 `format` 和 `timezone` 可选参数
       - `execute()` → 解析参数，根据 `timezone` 获取 `Local::now()` 或 `Utc::now()`，按 `format`（默认 `%Y-%m-%dT%H:%M:%S%:z`）格式化，返回 `ToolOutput::success`
     - 对无效 `timezone` 值返回 `ToolError::InvalidParams`
     - 定义 `pub fn register_datetime_tools(registry: &mut ToolRegistry)` 注册函数
   - 原因：遵循 `file.rs` 的模式——同一文件包含结构体、trait 实现、注册函数和单元测试
   - 依赖：阶段 1（chrono 依赖）
   - 风险：低

3. **单元测试**（文件：`crates/xclaw-tools/src/datetime.rs`，`#[cfg(test)] mod tests`）
   - 测试用例：
     - `returns_datetime_in_default_format` — 无参数调用，验证返回非空字符串且 `is_error == false`
     - `returns_utc_when_timezone_is_utc` — `timezone: "utc"`，验证返回包含 `+00:00` 或 `Z`
     - `returns_local_when_timezone_is_local` — `timezone: "local"`，验证成功返回
     - `rejects_invalid_timezone` — `timezone: "mars"`，验证返回 `ToolError::InvalidParams`
     - `custom_format_works` — `format: "%Y-%m-%d"`，验证返回仅包含日期部分
     - `tool_name_is_correct` — 验证 `name()` 返回 `"get_current_datetime"`
     - `parameters_schema_is_valid_json` — 验证 schema 包含 `type: "object"` 和正确 properties

### 阶段 3：注册集成（1 个文件修改）

4. **注册新工具**（文件：`crates/xclaw-tools/src/lib.rs`）
   - 操作：
     - 添加 `pub mod datetime;` 模块声明
     - 在 `register_builtin_tools()` 函数中追加 `datetime::register_datetime_tools(registry);`

### 阶段 4：集成测试（1 个新文件）

5. **集成测试**（文件：`crates/xclaw-tools/tests/datetime_integration.rs`）
   - 操作：
     - 通过 `ToolRegistry` 注册并查找 `get_current_datetime`
     - 构造 `ToolContext`，执行工具，验证输出可解析为有效日期时间
     - 验证 `register_builtin_tools()` 注册后 registry 包含该工具

### 阶段 5：格式化与检查

6. **运行 cargo fmt + clippy + test**
   - 操作：`cargo fmt`、`cargo clippy -- -D warnings`、`cargo test -p xclaw-tools`

## 风险与缓解

- **风险**：chrono 依赖增加编译时间
  - 缓解措施：使用 `default-features = false` 仅启用 `clock` 和 `std` feature

- **风险**：时间测试的不确定性（返回值每次不同）
  - 缓解措施：测试仅验证输出格式合法性（如可解析为 `chrono::DateTime`），不比较具体时间值

- **风险**：自定义 format 字符串中的无效占位符
  - 缓解措施：chrono 的 `format()` 对无效占位符不会 panic，只会忽略或原样输出，可安全使用

## 成功标准

- [ ] `cargo test -p xclaw-tools` 全部通过
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] `get_current_datetime` 工具在 `ToolRegistry` 中可被发现
- [ ] 无参数调用返回 ISO 8601 本地时间
- [ ] `timezone: "utc"` 返回 UTC 时间
- [ ] `timezone: "invalid"` 返回 `InvalidParams` 错误
- [ ] 自定义 `format` 参数正确格式化输出
- [ ] 测试覆盖率 80%+

## 复杂度预估

**整体复杂度：低**。约 150-200 行新代码（含测试），不涉及异步 I/O、文件系统、网络或安全校验。核心逻辑仅为调用 `chrono::Local::now()` 并格式化输出。
