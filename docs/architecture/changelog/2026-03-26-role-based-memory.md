# Architecture Changelog - 2026-03-26 (Role-based Memory + xclaw-role)

## Summary
引入 Role-based 文件优先记忆体系，新增独立的 `xclaw-role` crate 负责角色管理与多智能体编排。确立"Trait 定义归属模块"架构原则。Role 配置对齐 AIOS Agent 配置规范。

### Added
- **§1.3 架构原则**：新增 Trait 定义归属模块、AIOS 兼容、文件优先三条原则
- **xclaw-role crate**：独立的角色管理与多智能体编排模块
  - `RoleConfig`：AIOS 兼容的角色配置结构体（name, description, tools, meta）
  - `RoleManager` trait：Role CRUD 生命周期管理
  - `RoleOrchestrator` trait：多 Role 编排（串行委派、并行执行、管道协作）
  - `FsRoleManager`：基于文件系统的 Role 管理实现
- **`role.yaml` 配置**：对齐 AIOS/Cerebrum Agent config.json 规范
- **`MEMORY.md` 长期记忆**：存储经过提炼的关键决策、用户偏好和持久性事实
- **`memory/YYYY-MM-DD.md` 日常记忆**：日常笔记和运行时上下文，Append-only
- **`LongTermMemory` trait**：长期记忆的 load/save 接口（在 xclaw-memory 中定义）
- **`DailyMemory` trait**：日常记忆的 append/load_day/list_days 接口（在 xclaw-memory 中定义）
- **`MemorySearcher` trait**（预留）：语义搜索接口，暂不实现
- **`FsMemoryStore`**：基于文件系统的记忆实现
- **`/api/roles` REST 端点**：Gateway 新增角色管理接口
- **`RoleManager` / `RoleOrchestrator` 插件扩展点**：在 §10 中补充

### Changed
- **§2 高层架构图**：新增 xclaw-role 节点，xclaw-core 描述改为 "Shared Types"
- **§3 项目结构**：新增 xclaw-role crate，xclaw-core 移除 traits.rs，xclaw-memory 移除 role.rs
- **§4.1 xclaw-core**：明确不再定义任何 Trait，仅提供共享类型和错误类型
- **§4.2 xclaw-agent**：依赖列表新增 xclaw-role
- **§4.4**：从 xclaw-memory 的子章节重构为独立的 xclaw-role 章节（含 AIOS 对照表和编排模式）
- **§4.5**：xclaw-memory 简化为纯记忆模块，移除 Role 管理职责
- **§4.6~§4.10**：章节重新编号（原 §4.5~§4.9）
- **§4.8 Gateway REST API**：新增 `/api/roles` 端点
- **§10 可扩展性规划**：插件扩展点改为 `RoleManager` / `RoleOrchestrator` 和 `LongTermMemory` / `DailyMemory`
- **ADR-005**：从纯记忆设计扩展为包含 AIOS 兼容、xclaw-role 分离、Trait 归属原则的综合决策

### Removed
- xclaw-core 中的 `traits.rs`（所有 Trait 迁移至各业务模块）
- xclaw-memory 中的 `role.rs`（Role 配置迁移至 xclaw-role）
- xclaw-memory 中的 `RoleManager` trait（迁移至 xclaw-role）
- `MemorySystem` 门面 trait（由 xclaw-role + xclaw-memory 分别提供各自的 trait 替代）

## Context
初版设计将 Role 管理和记忆系统耦合在 xclaw-memory 中。经评审发现 Role 是跨切面概念（影响 Agent、Memory、Gateway），且 xclaw-config 应仅负责程序配置不参与业务逻辑。因此将 Role 管理分离为独立的 xclaw-role crate。

同时确立"Trait 定义归属模块"架构原则——xclaw-core 退化为纯类型层，所有 Trait 在各自的业务模块 crate 中定义。

Role 配置对齐 AIOS/Cerebrum Agent 配置规范（name, description, tools, meta），保持与 AIOS 生态的互操作性。xclaw-role 同时承担多智能体编排职责，参考 AIOS Scheduler 的 submitAgent/awaitAgentExecution 模式。

## Related ADR
ADR-005
