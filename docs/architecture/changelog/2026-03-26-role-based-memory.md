# Architecture Changelog - 2026-03-26 (Role-based Memory)

## Summary
引入 Role-based 文件优先记忆体系。确立"Trait 定义归属模块"架构原则。Role 配置对齐 AIOS Agent 配置规范。Role 定义与管理归入 `xclaw-memory`（`role/` 子模块），多智能体编排归入 `xclaw-agent`（`orchestrator/` 子模块）。新增三层 fallback Role 路由（`router/` 子模块）和 Peer 层级继承模型（`PeerChain`），编排通过 `delegate_to_role` tool 触发。新增工作区记忆文件（Workspace Memory Files）设计，移除 memory_store/memory_recall 工具。

### Added
- **§1.3 架构原则**：新增 Trait 定义归属模块、AIOS 兼容、文件优先三条原则
- **xclaw-memory `role/` 子模块**：角色管理（原计划为独立 xclaw-role crate，经评审取消）
  - `role/config.rs`：`RoleConfig` AIOS 兼容的角色配置结构体（name, description, tools, meta）
  - `role/manager.rs`：`RoleManager` trait + `FsRoleManager` Role CRUD 生命周期管理
  - `role/long_term.rs`：`LongTermMemory` trait + 实现
  - `role/daily.rs`：`DailyMemory` trait + 实现
- **xclaw-agent `orchestrator/` 子模块**：多智能体编排
  - `orchestrator/traits.rs`：`RoleOrchestrator` trait（串行委派、并行执行、管道协作）
  - `orchestrator/scheduler.rs`：调度实现
- **`role.yaml` 配置**：对齐 AIOS/Cerebrum Agent config.json 规范
- **`MEMORY.md` 长期记忆**：存储经过提炼的关键决策、用户偏好和持久性事实
- **`memory/YYYY-MM-DD.md` 日常记忆**：日常笔记和运行时上下文，Append-only
- **`MemorySearcher` trait**（预留）：语义搜索接口，暂不实现
- **`FsMemoryStore`**：基于文件系统的记忆实现
- **`/api/roles` REST 端点**：Gateway 新增角色管理接口
- **xclaw-agent `router/` 子模块**：Role 路由（三层 fallback 策略）
  - `router/traits.rs`：`RoleRouter` trait + `RouteInput` / `RouteDecision` 类型
  - `router/explicit.rs`：Layer 1 显式路由（命令前缀 + PeerChain 绑定向上遍历）
  - `router/llm_classifier.rs`：Layer 2 LLM 意图分类（轻量模型）
  - `router/chain.rs`：`ChainRouter` 按优先级串联 Layer 1→2→3
- **`PeerId` + `PeerChain`**（xclaw-core/types.rs）：通信实体层级模型，表达 Thread → Channel → Guild/Workspace 的继承关系
- **`Channel.resolve_peer_chain()`**：各平台 adapter 实现，将平台特有层级映射为通用 PeerChain
- **`role_bindings.yaml`**：Peer → Role 路由绑定配置文件（`~/.xclaw/role_bindings.yaml`）
- **`delegate_to_role` tool**（xclaw-tools/delegate.rs）：跨 Role 委派工具，LLM 通过 Tool 调用触发 RoleOrchestrator
- **xclaw-memory `workspace/` 子模块**：工作区记忆文件
  - `workspace/types.rs`：`WorkspaceFileKind` 枚举（Agents/Soul/Tools/Identity/User/Heartbeat/Bootstrap）、`WorkspaceSnapshot`
  - `workspace/loader.rs`：`WorkspaceMemoryLoader` trait + `FsWorkspaceLoader`（读写工作区记忆文件）
- **7 个工作区记忆文件**（每个 Role 目录下，LLM 可读可写）：`AGENTS.md`、`SOUL.md`、`TOOLS.md`、`IDENTITY.md`、`USER.md`、`HEARTBEAT.md`、`BOOTSTRAP.md`

### Changed
- **§2 高层架构图**：移除 xclaw-role 节点；xclaw-memory 描述改为 "Role + Memory + Storage"；xclaw-agent 描述改为 "Agent Runtime + Orchestration"
- **§3 项目结构**：xclaw-memory 新增 `role/` 子模块；xclaw-agent 新增 `orchestrator/` 子模块；移除 xclaw-role crate
- **§4.1 xclaw-core**：明确不再定义任何 Trait，仅提供共享类型和错误类型
- **§4.1 xclaw-core**：新增 `PeerId`、`PeerChain` 类型定义和 Peer 层级模型说明
- **§4.2 xclaw-agent**：新增 `RoleRouter` trait、三层 fallback 路由流程图、`RoleOrchestrator` trait 和编排模式描述；编排新增"通过 Tool 调用触发"说明；依赖列表移除 xclaw-role
- **§3 项目结构**：xclaw-agent 新增 `router/` 子模块；xclaw-tools 新增 `delegate.rs`
- **§4.4**：从独立的 xclaw-role 章节重构为 xclaw-memory 的统一章节（含 Role 管理 + 记忆系统 + AIOS 对照表）
- **§3 项目结构**：xclaw-memory 新增 `workspace/` 子模块（types.rs + loader.rs）
- **§4.4.3 文件系统布局**：Role 目录下新增 7 个工作区记忆文件
- **§4.4.6 记忆类型表**：新增"工作区记忆"行
- **§4.4.11**（新增）：工作区记忆文件设计（WorkspaceFileKind、WorkspaceMemoryLoader trait、读写策略）
- **§4.4.12 数据流图**（原 §4.4.11）：新增 WorkspaceMemory 节点和注入路径；章节重新编号
- **§4.6 xclaw-tools**：内置工具表移除 `memory_store` 和 `memory_recall`；移除 `memory.rs` 文件
- **§4.8 Channel**：trait 新增 `resolve_peer_chain()` 方法
- **§6.1 对话消息流**：重构为包含路由和编排阶段的完整数据流
- **§10 可扩展性**：新增 `RoleRouter` trait 扩展点
- **§4.5~§4.9**：章节重新编号（原 §4.6~§4.10）
- **ADR-005**：新增§5 Role 路由（三层 fallback + Peer 层级 + Tool 触发编排）决策；新增备选方案

### Removed
- xclaw-core 中的 `traits.rs`（所有 Trait 迁移至各业务模块）
- ~~xclaw-role crate~~（从未实现，设计阶段取消，职责拆分到 xclaw-memory + xclaw-agent）
- `MemorySystem` 门面 trait（由 xclaw-memory 统一提供各记忆 trait 替代）
- **`memory_store` / `memory_recall` 工具**（从 xclaw-tools 内置工具表移除，从未实现。Memory 是 Agent 层基础设施，不应作为 LLM 主动调用的 Tool）
- **`memory.rs`**（从 xclaw-tools 项目结构移除）

## Context
初版设计（同日早期）将 Role 管理规划为独立的 xclaw-role crate，理由是"Role 是跨切面概念"。经 architect 评审发现：

1. Role 的核心数据（配置、长期记忆、日常记忆）全部围绕文件系统持久化展开，与 xclaw-memory 天然内聚
2. 多智能体编排（submit/await/stop）本质上是 Agent 调度行为，归入 xclaw-agent 更自然，且避免循环依赖
3. xclaw-role 尚无代码实现，调整零成本
4. 减少一个 crate，降低 workspace 复杂度

因此取消 xclaw-role，将 Role 管理归入 xclaw-memory（`role/` 子模块），编排归入 xclaw-agent（`orchestrator/` 子模块）。

同日后续评审发现架构缺少 Role 路由环节——用户消息进来后由谁决定交给哪个 Role？经 architect 分析：

1. 路由和编排是正交的两个阶段（入口分派 vs 执行中协作），需独立设计
2. 路由采用三层 fallback：显式指定（零成本）→ LLM 意图分类（轻量）→ default 兜底
3. 显式路由需支持 Peer 层级继承（PeerChain），解决 Discord/Slack Thread 自动继承 Channel 绑定的需求
4. 编排通过 `delegate_to_role` tool 由 Role 的 LLM 自主触发，不引入 meta-agent 上帝视角

"Trait 定义归属模块"架构原则和 AIOS 兼容性设计不变。

同日后续评审新增工作区记忆文件（Workspace Memory Files）设计：

1. `memory_store` / `memory_recall` 工具从未实现，且 Memory 应由 Agent 自动注入而非 LLM 主动调用，从工具表移除
2. 新增 7 个可选 Markdown 文件（AGENTS/SOUL/TOOLS/IDENTITY/USER/HEARTBEAT/BOOTSTRAP），LLM 可读可写
3. 读取策略采用"构建 Prompt 时直接读文件"，无缓存、无热加载，保证简单性和实时性
4. Prompt 注入顺序后续设计，本次不定义
5. 新增 `xclaw-memory::workspace` 子模块（`WorkspaceFileKind` 枚举 + `WorkspaceMemoryLoader` trait）

## Related ADR
ADR-005
