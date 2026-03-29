# CLI 多轮对话改造

> 日期：2026-03-29
> 状态：已确认

## 概览

将 CLI 模块从当前的 one-shot 模式（`xclaw chat "message"`）改造为支持交互式多轮对话的 REPL 模式。CLI 通过对接 `xclaw-agent` 的 `LoopAgent` 引擎，复用已有的 session/transcript 持久化能力，在同一个 session 内持续接收用户输入并返回 LLM 响应。

## 需求

- 新增 `xclaw chat` 子命令（无 message 参数），进入交互式 REPL 循环
- 保留 `xclaw chat "message"` 作为 one-shot 模式的向后兼容
- 多轮对话共享同一个 `SessionId`，对话历史通过 `SessionStore` 持久化并在每轮注入
- 支持用户通过 `/exit`、`/quit` 或 Ctrl+D 退出 REPL
- 支持 `--session <id>` 参数恢复历史会话
- REPL 模式下打印提示符（如 `> `），支持基本的行编辑

## 架构变更

- **修改** `apps/cli/src/main.rs` — 拆分为多个模块，新增 REPL 循环逻辑
- **新增** `apps/cli/src/repl.rs` — REPL 循环实现（读取输入、调用 agent、打印输出）
- **新增** `apps/cli/src/setup.rs` — 提取 provider/memory/registry 初始化逻辑（消除 `match provider` 重复）
- **修改** `apps/cli/Cargo.toml` — 新增 `rustyline` 依赖用于行编辑
- **修改** `crates/xclaw-agent/src/session.rs` — `resolve_session_key` 支持动态 session id
- **修改** `crates/xclaw-agent/src/traits.rs` — `UserInput` 可选地携带 session_key

## 实施步骤

### 阶段 1：CLI 命令行参数重构

1. **重构 `Commands::Chat` 定义**（文件：`apps/cli/src/main.rs`）
   - 将 `message: String` 改为 `message: Option<String>`
   - 新增 `--session <id>` 可选参数
   - `message` 为 `None` 时进入 REPL 模式，`Some` 时保持 one-shot 模式
   - 依赖：无 | 风险：低

### 阶段 2：初始化逻辑提取

2. **提取 `setup` 模块**（文件：`apps/cli/src/setup.rs`，新增）
   - 将 `main.rs` 中的 `memory_base_dir()`、`dirs_or_fallback()`、`home_dir()`、provider 构建、registry 构建逻辑提取到独立模块
   - 创建泛型辅助函数或宏，内部 `match config.provider.kind` 构造具体 provider 后调用回调，消除三段重复的 `match` 分支
   - 依赖：无 | 风险：中（`LlmProvider` 非 dyn-safe，需要泛型回调或宏）

### 阶段 3：Session Key 动态化

3. **修改 `resolve_session_key` 支持自定义 session 标识**（文件：`crates/xclaw-agent/src/session.rs`）
   - 签名改为接受可选 `session_scope: Option<&str>` 参数
   - 提供时使用 `SessionKey::parse(&format!("default:{scope}"))`，否则保持默认 `"default:cli"`
   - REPL 模式使用 `"default:repl-{uuid}"` 格式的 session key
   - 依赖：无 | 风险：低

### 阶段 4：REPL 循环实现

4. **实现 REPL 模块**（文件：`apps/cli/src/repl.rs`，新增）
   - 核心逻辑：
     1. 初始化 `rustyline::DefaultEditor`
     2. 打印欢迎信息与使用提示
     3. 循环：读取用户输入 → 跳过空行 → 检测退出命令（`/exit`、`/quit`、Ctrl+D/EOF） → 构建 `UserInput`（共享同一 `session_id`） → 调用 `agent.process(input).await` → 打印响应
     4. 退出时打印告别信息
   - 依赖：阶段 2、阶段 3 | 风险：中

5. **处理 Ctrl+C 信号**（文件：`apps/cli/src/repl.rs`）
   - Ctrl+C 中断当前等待的 LLM 调用或清除当前输入行，而非退出整个程序
   - 使用 `tokio::select!` 配合 `tokio::signal::ctrl_c()` 实现
   - 依赖：步骤 4 | 风险：中

### 阶段 5：主入口整合

6. **整合 `main.rs` 入口**（文件：`apps/cli/src/main.rs`）
   - 根据 `message` 是否为 `Some`，分发到 one-shot 或 REPL
   - 两者共享 `setup` 模块的初始化逻辑
   - session_id 在 REPL 模式下生成一次并传入
   - 依赖：阶段 1-4 | 风险：低

### 阶段 6：依赖

7. **新增 `rustyline` 依赖**（文件：`apps/cli/Cargo.toml`）
   - 添加 `rustyline = "15"`（或最新稳定版本）
   - 依赖：无 | 风险：低

### 阶段 7：测试

8. **REPL 逻辑单元测试**（文件：`apps/cli/src/repl.rs`）
   - 测试退出命令检测、空行跳过、`UserInput` 构建逻辑、session_id 一致性

9. **setup 模块单元测试**（文件：`apps/cli/src/setup.rs`）
   - 测试 `memory_base_dir` 回退逻辑、provider 构建函数

10. **session key 动态化测试**（文件：`crates/xclaw-agent/src/session.rs`）
    - 验证带自定义 scope 的 session key 解析

11. **集成测试：多轮对话流程**（文件：`apps/cli/tests/multi_turn_test.rs`，新增）
    - 使用 stub provider 模拟多轮对话
    - 验证同一 session 内 transcript 累积、历史消息注入、tool call 跨轮正确

## 风险与缓解

| 风险 | 级别 | 缓解措施 |
|------|------|----------|
| `LlmProvider` 非 dyn-safe，泛型回调模式复杂 | 中 | 使用宏消除三段 `match` 重复 |
| `rustyline` 跨平台兼容性 | 低 | 提供 `--no-readline` 回退，使用 `std::io::stdin` |
| Ctrl+C 无法中断 LLM HTTP 请求 | 中 | `tokio::select!` + drop 连接，打印 `[interrupted]` |
| 长会话 transcript 过大 | 低 | `AgentConfig.transcript_tail_size` 默认 20 条已截断 |
| REPL 与 one-shot session key 冲突 | 低 | REPL 使用 `"default:repl-{uuid}"` 格式隔离 |

## 成功标准

- [ ] `xclaw chat "hello"` 保持 one-shot 行为不变（向后兼容）
- [ ] `xclaw chat` 进入交互式 REPL，显示提示符并等待用户输入
- [ ] REPL 内多轮对话共享同一 session，历史消息正确注入
- [ ] `/exit`、`/quit`、Ctrl+D 可正常退出 REPL
- [ ] Ctrl+C 不会直接终止程序，而是中断当前轮次
- [ ] `--session <id>` 可恢复历史会话的上下文
- [ ] `main.rs` 中三段重复的 provider match 被消除或封装
- [ ] 所有新增代码的测试覆盖率 >= 80%
- [ ] `cargo clippy -- -D warnings` 通过
- [ ] `cargo fmt --check` 通过
