# 架构设计：SessionId 轮换策略

> 版本：1.0 | 日期：2026-03-30 | 状态：Proposed

## 背景

当前 `FsSessionStore::get_or_create` 只要同一个 `SessionKey` 存在旧 session 就永远复用，导致 transcript 无限增长。需要 sessionId 的更新策略，使会话在合理的时间边界上自动轮换。

### 现状

- `SessionId`（`xclaw-core::types`）：不透明字符串包装，UUID v4 生成
- `SessionKey`（`xclaw-core::types`）：复合键 `{role_id}:{scope}`
- `SessionEntry`（`xclaw-memory::session::types`）：包含 `session_id`, `session_key`, `transcript_path`, `created_at`, `updated_at`
- `FsSessionStore::get_or_create`：查找已有 → 找不到则创建，**没有任何过期/轮换判断**

## 需求

### 轮换策略

1. **新 sessionKey → 新 sessionId**（已有行为，不变）
2. **每日重置 (Daily Mode)**：默认在凌晨 `reset_at_hour`（默认 4 点 UTC）划线。`updated_at` 在今日此时间点之前则视为过期
3. **空闲超时 (Idle Mode)**：可选。前后两次消息间隔超过 `idle_minutes` 则过期
4. **用户主动重置 (Explicit Reset)**：调用 `reset_session` 强制创建新 session
5. Daily 与 Idle 是 **OR** 关系，任一触发即轮换

### 约束

- 轮换逻辑**内聚于 Memory 模块**，不对外暴露实现细节
- `SessionStore` trait 的 `get_or_create` 签名不变，调用方零改动
- 旧 session 保留不删除（历史可追溯）

## 设计

### 核心数据结构

#### `SessionPolicy`

```rust
// crates/xclaw-memory/src/session/policy.rs

pub const DEFAULT_RESET_AT_HOUR: u8 = 4;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionPolicy {
    /// 每日重置的小时（UTC），默认 4
    pub reset_at_hour: u8,
    /// 空闲超时（分钟），None 时不启用
    pub idle_minutes: Option<u64>,
}

impl Default for SessionPolicy {
    fn default() -> Self {
        Self {
            reset_at_hour: DEFAULT_RESET_AT_HOUR,
            idle_minutes: None,
        }
    }
}
```

归属 `xclaw-memory::session`，不放入 `xclaw-config`（业务逻辑）或 `xclaw-core`（领域概念归属）。

### 过期判断

```rust
// crates/xclaw-memory/src/session/expiry.rs

/// 纯函数，时间作为参数注入（可测试性）
pub(crate) fn is_expired(
    updated_at: &str,
    now_epoch_secs: u64,
    policy: &SessionPolicy,
) -> bool
```

**每日重置算法**：

1. 根据 `now_epoch_secs` 计算今日 UTC 日期
2. 计算今日 reset 时间点 = 今日 00:00 UTC + `reset_at_hour` 小时
3. 如果 `now >= reset_point` 且 `updated_at < reset_point` → 过期
4. 如果 `now < reset_point`（还没到今天的 reset 点），用昨天的 reset 点比较

**空闲超时算法**：

1. `idle_threshold = now - idle_minutes * 60`
2. 如果 `updated_at < idle_threshold` → 过期

两种策略 OR 关系。

### 时间工具提取

从 `fs_store.rs` 提取到 `session/time_util.rs`（`pub(crate)`）：

```rust
pub(crate) fn epoch_to_ymd_hms(secs: u64) -> (u32, u32, u32, u32, u32, u32)
pub(crate) fn ymd_hms_to_epoch(y: u32, m: u32, d: u32, h: u32, min: u32, sec: u32) -> u64
pub(crate) fn parse_iso8601_to_epoch(s: &str) -> Result<u64, MemoryError>
pub(crate) fn now_utc() -> String
pub(crate) fn now_epoch_secs() -> u64
```

### SessionStore trait 变更

`get_or_create` 签名不变。仅新增：

```rust
fn reset_session(
    &self,
    key: &SessionKey,
) -> impl Future<Output = Result<SessionEntry, MemoryError>> + Send;
```

### FsSessionStore 变更

```rust
pub struct FsSessionStore {
    base_dir: PathBuf,
    policy: SessionPolicy,  // 新增
}

impl FsSessionStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self { /* policy: default */ }
    pub fn with_policy(base_dir: impl Into<PathBuf>, policy: SessionPolicy) -> Self { .. }
}
```

`get_or_create` 新逻辑：

```
找到已有 entry → is_expired? → 过期则创建新 session，否则复用
未找到 → 创建新 session
```

**关键变更**：同一 `SessionKey` 可对应多条 `SessionEntry`。`get_by_key` 改为返回最新创建的匹配。

### MemorySystem facade

新增 `fs_with_session_policy` 构造方法，允许传入自定义策略。

## 公开 API 边界

| 符号 | 可见性 | 说明 |
|---|---|---|
| `SessionPolicy`, `DEFAULT_RESET_AT_HOUR` | `pub` | 允许外部构造策略 |
| `FsSessionStore::with_policy` | `pub` | 自定义策略 |
| `SessionStore::reset_session` | `pub` (trait) | 用户主动重置 |
| `is_expired`, `time_util::*` | `pub(crate)` | 内部实现 |

## 取舍

### Daily + Idle 跨边界问题

**Q**: 如果 session 跨 4 点但在 idle time 内，需要新 sessionId 吗？

**A**: **需要。** 保持 OR 关系不变。

理由：
1. **Daily 服务于数据归档边界**——DailyMemory 按天组织，session 跨天会导致 daily summary 归属模糊
2. **凌晨 4 点活跃聊天是极端边界**——reset hour 选在低活跃时段，概率极低
3. **新 sessionId ≠ 丢失上下文**——session 轮换是内部机制。通过 memory 衔接（前 session 的 summary 作为新 session 的 context），用户无感知

后续在 agent 层实现 "context carry-over"：新 session 首次请求自动注入前 session 摘要。

### 其他取舍

| 决策 | 选择 | 理由 |
|------|------|------|
| 轮换逻辑放在实现内部 vs. 新增 trait 方法 | 实现内部 | 调用方零改动，语义自然延伸 |
| 旧 session 保留 vs. 自动删除 | 保留 | 历史可追溯，归档清理可后续独立实现 |
| 自行实现时间函数 vs. 引入 chrono | 自行实现 | 零新依赖，仅处理 UTC 固定格式 |

## 模块结构

```
crates/xclaw-memory/src/session/
├── mod.rs          ← 新增 pub mod policy; pub(crate) mod time_util; pub(crate) mod expiry;
├── policy.rs       ← 新文件
├── time_util.rs    ← 新文件
├── expiry.rs       ← 新文件
├── store.rs        ← 新增 reset_session
├── fs_store.rs     ← 持有 policy、过期检查、提取时间函数
├── fs_store_tests.rs
└── types.rs        ← 不变
```

## 文件变更清单

### 新增

| 文件 | 说明 |
|---|---|
| `session/policy.rs` | `SessionPolicy` + 常量 + Default |
| `session/time_util.rs` | 时间工具函数 |
| `session/expiry.rs` | `is_expired` 纯函数 |

### 修改

| 文件 | 变更 |
|---|---|
| `session/mod.rs` | 新增模块声明和 re-export |
| `session/store.rs` | 新增 `reset_session` |
| `session/fs_store.rs` | `policy` 字段、`with_policy`、过期检查、提取时间函数 |
| `session/fs_store_tests.rs` | 轮换测试用例 |
| `lib.rs` | re-export `SessionPolicy` |
| `facade.rs` | `fs_with_session_policy` 方法 |

### 不变

| 文件 | 原因 |
|---|---|
| `xclaw-core/src/types.rs` | SessionId/SessionKey 无需变更 |
| `xclaw-config` | 非业务逻辑 |
| `session/types.rs` | `updated_at` 已足够 |

## 测试策略

### expiry.rs 单元测试

1. daily: updated_at 在今日 reset_at_hour 之前 → 过期
2. daily: updated_at 在今日 reset_at_hour 之后 → 未过期
3. daily: 跨午夜边界（now 在 reset_at_hour 之前）
4. idle: 超过 idle_minutes → 过期
5. idle: 未超过 → 未过期
6. 组合: daily 未过期 + idle 过期 → 过期
7. 组合: daily 过期 + idle 未过期 → 过期
8. 边界: reset_at_hour = 0, 23

### fs_store 集成测试

1. `get_or_create` 过期时创建新 session
2. `get_or_create` 未过期时复用旧 session
3. `reset_session` 强制创建新 session
4. 多次轮换后 `list_sessions` 返回所有历史
5. `get_by_key` 返回最新 session

## 未来扩展

- `prune_sessions(role_id, older_than_days)` 清理历史 session
- Role config YAML 中 `session_policy` 字段为不同 role 设不同策略
- `max_sessions_per_key: Option<usize>` 超限自动清理最旧
