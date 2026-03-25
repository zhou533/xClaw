# Architecture Changelog - 2026-03-25

## Summary
将 xclaw-core 拆分为 xclaw-core（基础定义层）+ xclaw-agent（智能体引擎）+ xclaw-memory（记忆持久化），并新增 xclaw-skill（技能系统）模块。

### Added
- **xclaw-agent** crate：从 xclaw-core 中分离出 Agent Loop、Session Manager、Tool/Skill Dispatch，成为系统核心驱动引擎
- **xclaw-memory** crate：从 xclaw-core 中分离出 Memory Store，独立负责分层存储（热/温/冷）与语义检索
- **xclaw-skill** crate：新增技能系统模块，包含 Skill 注册、发现、加载、执行，仅依赖 xclaw-core
- `/api/skills` REST 端点：Gateway 新增技能列表查询接口
- `Skill` trait 插件扩展点：在 §10 可扩展性规划中补充

### Changed
- **xclaw-core** 功能退化为基础定义层：仅保留 Trait definitions（AgentLoop, MemoryStore, Skill 等）、Shared types（Message, Role, SessionId, ToolCall 等）、Error types；移除对 xclaw-provider、xclaw-tools、xclaw-config 的依赖
- **依赖关系重构**：xclaw-core 成为零业务依赖的类型基石，被几乎所有 crate 依赖；xclaw-agent 作为引擎层依赖所有功能模块
- **CLI 入口**：直接依赖 xclaw-agent，无需经过 Gateway，实现轻量级直连
- **Tauri IPC**：从调用 xclaw-core 改为调用 xclaw-agent
- **§2 高层架构图**：重绘为三层结构（core 基石层 → 功能模块层 → agent 引擎层 → 运行模式层）
- **§4 核心组件设计**：从 4.1~4.6 调整为 4.1~4.8，每个组件明确标注依赖关系
- **§6 数据流图**：CLI 与 Gateway 并列接入 xclaw-agent，Tool/Skill 调用合并展示

### Removed
- xclaw-core 中的 `agent` 模块（迁移至 xclaw-agent）
- xclaw-core 中的 `memory` 模块（迁移至 xclaw-memory）
- xclaw-core 中的 `session` 模块（迁移至 xclaw-agent）
- xclaw-core 对 xclaw-provider、xclaw-tools、xclaw-config 的依赖

## Context
原 xclaw-core 承担了过多职责（Agent 循环、会话管理、记忆持久化、共享类型定义），导致：
1. **循环依赖风险**：core 依赖 provider/tools/config，而这些模块又可能需要 core 的类型定义
2. **编译粒度过粗**：修改 Agent 逻辑会触发依赖 core 的所有 crate 重编译
3. **职责不清**：基础类型定义与业务逻辑混杂在同一 crate

拆分后 xclaw-core 成为纯定义层（类似 std 的角色），各功能模块独立演进，依赖方向清晰单一。新增 xclaw-skill 模块为技能编排提供独立扩展点，与 Tool（原子操作）形成互补。

## Related ADR
无（本次为架构调整，未涉及新技术选型决策）
