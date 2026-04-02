# CLI Debug Mode — Tool Call 异常显示

## 概览

在现有 CLI `--debug` 模式基础上，增加对工具调用错误/异常的可视化输出。当 debug 模式启用时，每个工具调用的结果（成功或失败）都会以彩色格式输出到 stderr，使开发者能够即时看到工具执行的详细状态，包括错误类型、错误消息和调用参数。

## 需求

- debug 模式下，每个工具调用结果（成功/失败）在 stderr 上输出
- 错误结果用红色高亮，成功结果用绿色（与现有颜色方案一致）
- 显示工具名称、call_id、参数摘要、执行结果或错误消息
- 区分四类错误：工具未找到、参数解析失败、工具执行异常（`ToolError`）、工具返回错误输出（`ToolOutput.is_error`）
- 非 debug 模式下行为完全不变
- 零新依赖

## 修改范围

| 文件 | 变更 |
|------|------|
| `crates/xclaw-agent/src/debug_fmt.rs` | 新增 `format_tool_call_detail` + `format_tool_result_detail` + `RED` 常量 |
| `crates/xclaw-agent/src/dispatch.rs` | `ToolDispatcher` 增加 `debug: bool` 参数，执行前后输出 debug 信息 |
| `crates/xclaw-agent/src/engine.rs` | 传递 `self.config.debug` 到 `ToolDispatcher` |

## 实施阶段

### 阶段 1：Debug 格式化函数（debug_fmt.rs）

1. 新增 `RED` 颜色常量：`const RED: &str = "\x1b[31m";`

2. 新增 `format_tool_call_detail(tool_name, call_id, arguments) -> String`
   - 输出格式：`{MAGENTA}[TOOL_EXEC]{RESET} {tool_name} (id: {call_id})\n{DIM}  args: {truncated_arguments}{RESET}\n`
   - 参数超过 200 字符时截断并追加 `...`

3. 新增 `format_tool_result_detail(tool_name, call_id, is_error, content) -> String`
   - 成功：`{GREEN}[TOOL_OK]{RESET} {tool_name}: {truncated_content}`
   - 错误：`{RED}[TOOL_ERR]{RESET} {tool_name}: {error_message}`
   - 内容超过 300 字符时截断

### 阶段 2：ToolDispatcher 集成（dispatch.rs）

4. `ToolDispatcher::new` 签名改为 `pub fn new(registry: &'a ToolRegistry, debug: bool) -> Self`
   - 新增 `debug: bool` 字段
   - `execute_tool_calls` 中，每个工具调用前后，当 `self.debug == true` 时调用格式化函数并 `eprint!`

5. 更新所有现有 `ToolDispatcher::new(&reg)` 调用为 `ToolDispatcher::new(&reg, false)`

### 阶段 3：Engine 层传递（engine.rs）

6. 将 `ToolDispatcher::new(self.tool_registry)` 改为 `ToolDispatcher::new(self.tool_registry, self.config.debug)`

### 阶段 4：测试

7. `debug_fmt` 单元测试：
   - `format_tool_call_detail` 输出包含工具名和 call_id
   - 参数截断行为（>200 字符时追加 `...`）
   - `format_tool_result_detail` 成功和失败场景
   - 包含 `[TOOL_OK]` / `[TOOL_ERR]` 标签
   - 内容截断行为

8. `dispatch` 集成测试：
   - `debug_mode_does_not_affect_results`：验证 `debug: true` 返回结果与 `debug: false` 一致

## 风险

| 级别 | 风险 | 缓解措施 |
|------|------|----------|
| 低 | 签名变更导致编译失败 | 变更范围小，仅 engine.rs 和测试调用点 |
| 中 | debug 输出暴露敏感参数 | 参数截断 200 字符；仅 `--debug` 时启用；输出到 stderr |
| 低 | 大量工具调用时输出过多 | 每个调用仅 2-3 行摘要；内容截断 300 字符 |

## 成功标准

- [ ] `--debug` 模式下，每个工具调用前显示工具名、call_id、参数摘要
- [ ] 工具执行成功时显示绿色 `[TOOL_OK]` 标签和结果摘要
- [ ] 工具执行失败时显示红色 `[TOOL_ERR]` 标签和错误消息
- [ ] 四种错误类型均正确展示
- [ ] 非 debug 模式下无任何额外输出
- [ ] 所有现有测试通过
- [ ] 新增测试覆盖格式化函数和 debug/non-debug 行为一致性
- [ ] `cargo clippy -- -D warnings` 无警告
