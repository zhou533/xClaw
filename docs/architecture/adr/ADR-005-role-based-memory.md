# ADR-005: 采用 Role-based 文件优先记忆体系 + AIOS 兼容角色定义 + 分层路由与 Peer 层级

## 背景

xClaw 作为个人 AI 助手，需要跨会话保留用户偏好、关键决策和日常交互上下文。现有 `MemoryStore` trait 仅服务于 Session 级别的对话历史（热/温数据），缺乏以下能力：

1. **角色隔离**：用户可能以不同身份（秘书、编程助手、私人助理）使用 AI，各角色的记忆应互不干扰
2. **长期记忆**：经过提炼的关键信息需要持久化，跨会话可用
3. **日常记忆**：运行时上下文和流水账需要按日归档，保证时间线完整性
4. **人类可读**：用户应能直接浏览和编辑记忆文件，而非被锁定在数据库中
5. **多智能体编排**：多个 Role 需要能够协作完成复杂任务
6. **消息路由**：用户消息需要自动分派到正确的 Role，同时支持跨平台的通信实体层级（如 Discord/Slack 的 Thread → Channel → Guild）
7. **生态互操作**：Role 定义应与 AIOS 等 AI Agent 操作系统生态兼容

## 决策

### 1. Role-based 文件优先记忆体系

- 引入 `Role` 作为智能体身份与记忆隔离的基本单位，每个 Role 拥有独立的文件系统工作空间（`~/.xclaw/roles/{role_name}/`）
- 系统内置 `default` Role，用户无需手动创建
- **长期记忆**（`MEMORY.md`）：提炼后的关键信息，支持覆盖写入
- **日常记忆**（`memory/YYYY-MM-DD.md`）：运行时上下文，Append-only
- 记忆文件使用 Markdown 格式，Role 配置使用 YAML 格式
- SQLite 向量搜索作为扩展点预留

### 2. AIOS 兼容的角色定义

Role 配置（`role.yaml`）对齐 [AIOS/Cerebrum](https://github.com/agiresearch/AIOS) Agent 配置规范：

- `name`：角色标识符
- `description`：字符串数组描述
- `tools`：可用工具白名单
- `meta`：元信息（author, version, license）
- xClaw 扩展：`system_prompt`（AIOS 通过 Python 代码实现，xClaw 通过声明式配置实现）

### 3. Trait 定义归属模块（架构原则）

所有 Trait 在其所属的业务模块 crate 中定义：
- `RoleManager` / `RoleConfig` / `LongTermMemory` / `DailyMemory` / `MemoryStore` → `xclaw-memory`
- `RoleOrchestrator` → `xclaw-agent`
- `Tool` → `xclaw-tools`
- `xclaw-core` 仅存放跨模块共享的基础类型（`RoleId`、`SessionId` 等）和错误类型

### 4. Role 管理归入 xclaw-memory（~~独立 xclaw-role crate~~ — 已撤销）

> **2026-03-26 更新**：经 architect 评审，取消独立 xclaw-role crate，将 Role 定义与管理归入 xclaw-memory，多智能体编排归入 xclaw-agent。

Role 管理作为 `xclaw-memory` 的 `role/` 子模块：
- `role/config.rs`：`RoleConfig` AIOS 兼容的角色配置结构体
- `role/manager.rs`：`RoleManager` trait + `FsRoleManager` Role CRUD 生命周期管理
- `role/long_term.rs`：`LongTermMemory` trait + 实现
- `role/daily.rs`：`DailyMemory` trait + 实现

多智能体编排作为 `xclaw-agent` 的 `orchestrator/` 子模块：
- `orchestrator/traits.rs`：`RoleOrchestrator` trait
- `orchestrator/scheduler.rs`：调度实现（串行委派、并行执行、管道协作）
- 对应 AIOS 的 `submitAgent` / `awaitAgentExecution` 模式

**调整理由**：
- Role 的核心数据（配置、长期记忆、日常记忆）全部围绕文件系统持久化展开，与 xclaw-memory 天然内聚
- 多智能体编排本质上是 Agent 调度行为，需要持有/创建 Agent 实例，归入 xclaw-agent 避免循环依赖
- 减少一个 crate，降低 workspace 复杂度；xclaw-role 尚无代码实现，调整零成本
- xclaw-config 仅负责程序自身配置，不参与业务逻辑（不变）

### 5. Role 路由：三层 fallback + Peer 层级继承

> **2026-03-26 更新**：新增 Role 路由和 Peer 层级设计。

#### 5.1 三层 fallback 路由策略

Role 路由（`RoleRouter`）归入 `xclaw-agent/router/`，采用分层策略决定消息分派：

| Layer | 策略 | 成本 | 触发条件 |
|-------|------|------|---------|
| Layer 1 | 显式路由：命令前缀（`/role coder`）+ PeerChain 绑定查找 | 零 | 每条消息必检 |
| Layer 2 | LLM 意图分类：轻量模型（Haiku 级别）根据 Role descriptions 分类 | 低 | 仅 Layer 1 未匹配时 |
| Layer 3 | 默认 fallback：`default` Role 兜底 | 零 | Layer 2 confidence < threshold |

**为什么路由不全交给 LLM**：显式绑定（Layer 1）零成本且确定性最高，覆盖了大部分场景（用户通常在固定通道/频道使用固定 Role）。LLM 仅作为语义 fallback。

**为什么路由不完全排除 LLM**：Role 的 `description` 是自然语言，规则引擎无法可靠匹配语义意图，轻量 LLM 调用是性价比最高的 fallback。

#### 5.2 Peer 层级模型

通信实体（Peer）在不同平台上存在天然的层级关系。例如 Discord 或 Slack 中，Thread 是 Channel 的子实体，每个新 Thread 都产生一个全新的 PeerId。要求用户为每个新 Thread 都配置路由绑定是不现实的。

引入 `PeerChain`（归入 `xclaw-core/types.rs`）表达从最具体到最笼统的层级链：

```
Discord: [thread_id, channel_id, guild_id]
Slack:   [thread_ts, channel_id, workspace_id]
Telegram: [topic_id, group_id] 或 [dm_user_id]
```

Layer 1 的 `ExplicitRouter` 沿 `PeerChain` 向上遍历 `role_bindings.yaml`，第一个命中的绑定即为目标 Role。用户只需配置 `#ops → secretary`，该频道下所有 Thread 自动继承。

各 Channel adapter 通过实现 `resolve_peer_chain()` 将平台特有的层级映射为通用 `PeerChain`。

#### 5.3 编排通过 Tool 调用触发

多 Role 编排不由外部"meta-agent"上帝视角控制，而是当前 Role 的 LLM 通过 `delegate_to_role` tool 主动发起协作（内部调用 `RoleOrchestrator::submit()`）。这与 Agent Loop 的 Tool 调用模式一致，不引入额外的调度复杂度。

### 6. 工作区记忆文件（Workspace Memory Files）

> **2026-03-26 更新**：新增工作区记忆文件设计，移除 xclaw-tools 中的 memory_store/memory_recall 工具。

#### 6.1 移除 memory_store / memory_recall 工具

`memory_store` 和 `memory_recall` 从 xclaw-tools 内置工具表中移除（从未实现）。理由：

- Memory 是 Agent 层的基础设施关注点，不应作为 LLM 主动调用的 Tool 暴露
- Agent Loop 在构建 Prompt 时自动读取记忆文件并注入上下文，不需要 LLM "记得去记忆"
- 记忆文件的写入语义各不相同（覆盖、追加、结构化更新），不适合 `execute(ctx, params) -> ToolOutput` 的统一签名
- xclaw-tools 不应依赖 xclaw-memory，保持工具（无状态原子操作）与记忆（有状态上下文管理）的分离

#### 6.2 新增工作区记忆文件

在每个 Role 目录下新增 7 个可选的 Markdown 文件，LLM 可读可写：

| 文件 | 语义用途 |
|------|---------|
| `AGENTS.md` | 工作区协作护栏、注意事项与规范指导 |
| `SOUL.md` | AI 人设与语调（Persona & Tone） |
| `TOOLS.md` | 额外工具引导（非默认白名单的复合工具） |
| `IDENTITY.md` | AI 自身认同框架 |
| `USER.md` | 当前人类使用者的技术栈偏好 |
| `HEARTBEAT.md` | 心跳机制 / 长连接轮询应对动作参照 |
| `BOOTSTRAP.md` | 新工作区初始引导规范（仅限新工作区） |

**读取策略**：每次构建 Prompt 时直接从文件系统读取，无缓存、无热加载。

**写入策略**：LLM 通过 `WorkspaceMemoryLoader::save_file()` 写入，用户也可直接编辑文件。下次构建 Prompt 时自动获取最新内容。

**模块归属**：`xclaw-memory::workspace` 子模块（`types.rs` + `loader.rs`），提供 `WorkspaceMemoryLoader` trait 和 `FsWorkspaceLoader` 实现。

### 7. 依赖方向

```
xclaw-core  ←── xclaw-memory（Role 管理 + 记忆读写）
            ←── xclaw-agent（编排 + 组装 memory + provider）
```

## 影响

### 正面影响
- **人类可读可编辑**：Markdown 文件用户可直接浏览、搜索和修改
- **Git 友好**：纯文本格式天然支持版本控制和差异比较
- **角色隔离**：不同 Role 的记忆互不干扰，避免上下文污染
- **AIOS 互操作**：Role 配置可与 AIOS Agent Hub 生态互通
- **多智能体编排**：RoleOrchestrator 支持复杂的多角色协作场景
- **自动路由**：三层 fallback 策略平衡确定性和灵活性，用户无需为每个 Thread 手动配置
- **Peer 层级继承**：PeerChain 模型让 Channel → Thread 的绑定自动传递，跨平台通用
- **零额外依赖**：文件系统存储无需引入新的数据库或服务
- **渐进增强**：从文件系统开始，未来可无缝引入 SQLite FTS / 向量搜索
- **职责清晰**：xclaw-memory（角色管理 + 记忆读写）、xclaw-agent（路由 + 编排 + 运行时）、xclaw-config（程序配置）各司其职

### 负面影响
- **搜索能力有限**：纯文件系统不支持语义搜索（SQLite FTS 扩展点可缓解）
- **并发写入**：多进程同时追加同一日常记忆文件可能冲突（单用户场景影响可忽略）
- **AIOS 兼容有限**：xClaw 的 Role 是声明式配置，AIOS Agent 包含可执行 Python 代码，完全互操作需要适配层
- **模块复杂度**：xclaw-memory 子模块增多（通过 `role/` 子目录组织缓解）

### 备选方案
- **纯 SQLite**：搜索能力强，但用户无法直接浏览/编辑记忆
- **独立 xclaw-role crate**：~~初版设计~~ 已撤销。职责分离带来的收益不足以抵消额外 crate 的复杂度，且 Role 数据本质上是记忆持久化的一部分
- **Role 放在 xclaw-config**：config 应只管程序配置，不参与业务逻辑
- **Role 放在 xclaw-core**：core 应保持零业务依赖的纯类型层
- **不兼容 AIOS**：自定义配置格式更简单，但失去与 AIOS 生态互操作的可能性
- **路由完全由 LLM 决定**：每条消息都调 LLM 分类，成本高且延迟大；显式绑定覆盖大部分场景
- **路由完全由规则引擎决定**：无法可靠匹配 Role description 的语义意图，缺乏灵活性
- **扁平 PeerId（无层级）**：用户必须为每个新 Thread 单独配置绑定，不现实
- **编排由 meta-agent 上帝视角控制**：引入额外的调度层，增加复杂度；不如让每个 Role 通过 Tool 调用自主触发

## 状态
Proposed

## 日期
2026-03-25
