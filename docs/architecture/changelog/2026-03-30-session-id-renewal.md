# 2026-03-30: SessionId 轮换策略

## 变更类型

模块增强

## 概述

为 `xclaw-memory` 的 session 子系统增加 SessionId 轮换机制，避免同一 SessionKey 下的 session 无限存续。

## 变更内容

### 新增

- **`SessionPolicy`**（`xclaw-memory::session::policy`）：轮换策略配置，包含 `reset_at_hour`（每日重置时间，默认 4 点 UTC）和 `idle_minutes`（空闲超时，可选）
- **`is_expired`**（`xclaw-memory::session::expiry`）：纯函数过期判断，时间参数注入，`pub(crate)`
- **`time_util` 模块**（`xclaw-memory::session::time_util`）：从 `fs_store.rs` 提取 `epoch_to_ymd_hms`/`now_utc`，新增 `ymd_hms_to_epoch`/`parse_iso8601_to_epoch`/`now_epoch_secs`，`pub(crate)`
- **`SessionStore::reset_session`**：新增 trait 方法，用户主动重置会话
- **`FsSessionStore::with_policy`**：自定义策略构造方法
- **`FsMemorySystem::fs_with_session_policy`**：facade 层自定义策略入口

### 修改

- **`FsSessionStore`**：新增 `policy` 字段；`get_or_create` 内部加入过期检查；`get_by_key` 改为返回最新匹配
- **`lib.rs`**：re-export `SessionPolicy`、`DEFAULT_RESET_AT_HOUR`

### 设计决策

- **Daily + Idle 为 OR 关系**：任一触发即轮换。Daily 服务于数据归档边界（DailyMemory 按天组织），跨 reset_at_hour 的活跃聊天属极端边界，新 sessionId 不等于丢失上下文（通过 memory 衔接保证连续性）
- **轮换逻辑在实现内部**：`SessionStore` trait 的 `get_or_create` 签名不变，调用方零改动
- **旧 session 保留不删除**：历史可追溯，归档清理可后续独立实现
- **自行实现时间函数**：零新依赖，仅处理 UTC 固定格式

## 相关文档

- 设计文档：[session-id-renewal.md](../session-id-renewal.md)
- 基础架构：[session-system.md](../session-system.md)
- ADR：[ADR-006-session-system.md](../adr/ADR-006-session-system.md)
