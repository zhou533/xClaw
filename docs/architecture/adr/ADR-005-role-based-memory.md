# ADR-005: 采用 Role-based 文件优先记忆体系 + AIOS 兼容角色定义

## 背景

xClaw 作为个人 AI 助手，需要跨会话保留用户偏好、关键决策和日常交互上下文。现有 `MemoryStore` trait 仅服务于 Session 级别的对话历史（热/温数据），缺乏以下能力：

1. **角色隔离**：用户可能以不同身份（秘书、编程助手、私人助理）使用 AI，各角色的记忆应互不干扰
2. **长期记忆**：经过提炼的关键信息需要持久化，跨会话可用
3. **日常记忆**：运行时上下文和流水账需要按日归档，保证时间线完整性
4. **人类可读**：用户应能直接浏览和编辑记忆文件，而非被锁定在数据库中
5. **多智能体编排**：多个 Role 需要能够协作完成复杂任务
6. **生态互操作**：Role 定义应与 AIOS 等 AI Agent 操作系统生态兼容

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
- `RoleManager` / `RoleOrchestrator` → `xclaw-role`
- `LongTermMemory` / `DailyMemory` / `MemoryStore` → `xclaw-memory`
- `Tool` → `xclaw-tools`
- `xclaw-core` 仅存放跨模块共享的基础类型（`RoleId`、`SessionId` 等）和错误类型

### 4. 独立 xclaw-role crate

Role 管理从 xclaw-memory 中分离为独立的 `xclaw-role` crate：
- `RoleConfig`：AIOS 兼容的角色配置结构体
- `RoleManager`：Role CRUD 生命周期管理
- `RoleOrchestrator`：多 Role 编排（串行委派、并行执行、管道协作）
- 对应 AIOS 的 `submitAgent` / `awaitAgentExecution` 模式

**分离理由**：Role 是跨切面概念，影响 Agent（system_prompt）、Memory（记忆隔离）、Gateway（CRUD API），不应耦合在任何单一模块中。xclaw-config 仅负责程序自身配置，不参与业务逻辑。

### 5. 依赖方向

```
xclaw-core  ←── xclaw-role（Role 管理 + 编排）
            ←── xclaw-memory（记忆读写，接收 RoleId）
            ←── xclaw-agent（组装 role + memory）
```

## 影响

### 正面影响
- **人类可读可编辑**：Markdown 文件用户可直接浏览、搜索和修改
- **Git 友好**：纯文本格式天然支持版本控制和差异比较
- **角色隔离**：不同 Role 的记忆互不干扰，避免上下文污染
- **AIOS 互操作**：Role 配置可与 AIOS Agent Hub 生态互通
- **多智能体编排**：RoleOrchestrator 支持复杂的多角色协作场景
- **零额外依赖**：文件系统存储无需引入新的数据库或服务
- **渐进增强**：从文件系统开始，未来可无缝引入 SQLite FTS / 向量搜索
- **职责清晰**：xclaw-role（角色生命周期）、xclaw-memory（记忆读写）、xclaw-config（程序配置）各司其职

### 负面影响
- **搜索能力有限**：纯文件系统不支持语义搜索（SQLite FTS 扩展点可缓解）
- **并发写入**：多进程同时追加同一日常记忆文件可能冲突（单用户场景影响可忽略）
- **AIOS 兼容有限**：xClaw 的 Role 是声明式配置，AIOS Agent 包含可执行 Python 代码，完全互操作需要适配层
- **新增 crate**：xclaw-role 增加了 workspace 复杂度（但职责分离带来的收益大于成本）

### 备选方案
- **纯 SQLite**：搜索能力强，但用户无法直接浏览/编辑记忆
- **Role 放在 xclaw-memory**：职责耦合，Agent/Gateway 被迫依赖 memory 模块获取 Role 信息
- **Role 放在 xclaw-config**：config 应只管程序配置，不参与业务逻辑
- **Role 放在 xclaw-core**：core 应保持零业务依赖的纯类型层
- **不兼容 AIOS**：自定义配置格式更简单，但失去与 AIOS 生态互操作的可能性

## 状态
Proposed

## 日期
2026-03-25
