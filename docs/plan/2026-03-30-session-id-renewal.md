# 实施计划：SessionId 轮换策略

> 日期：2026-03-30 | 状态：Approved | 架构文档：[session-id-renewal.md](../architecture/session-id-renewal.md)

## 概览

为 `xclaw-memory` 的 session 子系统新增自动轮换能力，包含每日重置（Daily）、空闲超时（Idle）和用户主动重置三种策略。轮换逻辑内聚于 Memory 模块，`SessionStore::get_or_create` 签名不变，调用方零改动。

## 需求

- 每日重置：默认凌晨 4 点 UTC 划线，`updated_at` 在 reset 时间点之前则视为过期
- 空闲超时：可选，前后两次消息间隔超过 `idle_minutes` 则过期
- 用户主动重置：调用 `reset_session` 强制创建新 session
- Daily 与 Idle 为 OR 关系，任一触发即轮换
- 旧 session 保留不删除
- 不引入 chrono，自行实现时间函数
- `get_or_create` 签名不变

## 架构变更

### 新增文件

| 文件 | 说明 |
|---|---|
| `crates/xclaw-memory/src/session/policy.rs` | `SessionPolicy` 结构体 + 常量 + Default |
| `crates/xclaw-memory/src/session/time_util.rs` | 从 `fs_store.rs` 提取的时间工具函数 + 新增函数 |
| `crates/xclaw-memory/src/session/expiry.rs` | `is_expired` 纯函数 |

### 修改文件

| 文件 | 变更 |
|---|---|
| `session/mod.rs` | 新增模块声明和 re-export |
| `session/store.rs` | 新增 `reset_session` trait 方法 |
| `session/fs_store.rs` | `policy` 字段、`with_policy`、过期检查、提取时间函数到 `time_util` |
| `session/fs_store_tests.rs` | 新增轮换测试用例 |
| `lib.rs` | re-export `SessionPolicy` |
| `facade.rs` | `fs_with_session_policy` 构造方法 |

## 实施步骤

### 阶段 1：时间工具提取（纯重构，无功能变更）

1. **创建 `time_util.rs` 并提取时间函数**
   - 从 `fs_store.rs` 提取 `epoch_to_ymd_hms` 和 `now_utc` 为 `pub(crate)` 独立函数
   - 新增：`ymd_hms_to_epoch`（逆运算）、`parse_iso8601_to_epoch`、`now_epoch_secs`
   - 依赖：无

2. **修改 `fs_store.rs` 使用 `time_util`**
   - 删除私有时间函数，改为调用 `super::time_util::*`
   - 依赖：步骤 1

3. **在 `mod.rs` 中声明 `time_util` 模块**
   - 添加 `pub(crate) mod time_util;`
   - 依赖：步骤 1

4. **验证：`cargo test -p xclaw-memory` 全部通过**
   - 依赖：步骤 1-3

### 阶段 2：SessionPolicy + is_expired 纯函数（TDD）

5. **创建 `policy.rs`**
   - `SessionPolicy { reset_at_hour: u8, idle_minutes: Option<u64> }`
   - `DEFAULT_RESET_AT_HOUR = 4`、`Default` 实现
   - 依赖：无

6. **编写 `expiry.rs` 测试（RED）**
   - 覆盖场景：
     - daily: updated_at 在今日 reset_at_hour 之前 → 过期
     - daily: updated_at 在今日 reset_at_hour 之后 → 未过期
     - daily: 跨午夜边界（now 在 reset_at_hour 之前，用昨日 reset 点比较）
     - idle: 超过 idle_minutes → 过期
     - idle: 未超过 → 未过期
     - 组合: daily 未过期 + idle 过期 → 过期（OR）
     - 组合: daily 过期 + idle 未过期 → 过期（OR）
     - 边界: reset_at_hour = 0 和 23
     - 边界: idle_minutes = None 时只看 daily
     - 边界: updated_at 恰好等于 reset 时间点 → 未过期
   - 依赖：步骤 5

7. **实现 `is_expired` 纯函数（GREEN）**
   - `pub(crate) fn is_expired(updated_at: &str, now_epoch_secs: u64, policy: &SessionPolicy) -> bool`
   - 使用 `time_util::parse_iso8601_to_epoch` 解析 `updated_at`
   - 依赖：步骤 1、5

8. **在 `mod.rs` 中声明新模块**
   - `pub mod policy;` 和 `pub(crate) mod expiry;`
   - 依赖：步骤 5-7

9. **验证：`cargo test -p xclaw-memory is_expired` 全部通过**

### 阶段 3：FsSessionStore 集成轮换逻辑（TDD）

10. **编写 `fs_store_tests.rs` 轮换测试（RED）**
    - `get_or_create_renews_expired_daily_session`
    - `get_or_create_reuses_unexpired_session`
    - `get_or_create_renews_idle_expired_session`
    - `reset_session_creates_new_session`
    - `reset_session_preserves_old_session`
    - `get_by_key_returns_latest_session`
    - 依赖：步骤 5
    - 注意：测试中直接操作 index 文件注入过期的 `updated_at`

11. **`FsSessionStore` 新增 `policy` 字段 + `with_policy`**
    - `new` 使用 `SessionPolicy::default()`
    - `pub fn with_policy(base_dir, policy) -> Self`
    - 依赖：步骤 5

12. **修改 `get_or_create` 加入过期检查（GREEN）**
    - 找到已有 entry → `is_expired?` → 过期则创建新 session
    - `get_by_key` 改为按 `created_at` 降序取最新匹配
    - 依赖：步骤 7、11

13. **`SessionStore` trait 新增 `reset_session`**
    - `fn reset_session(&self, key: &SessionKey) -> impl Future<Output = Result<SessionEntry, MemoryError>> + Send;`
    - 依赖：无

14. **`FsSessionStore` 实现 `reset_session`**
    - 无条件创建新 session，提取 `create_new_session` 私有方法复用
    - 依赖：步骤 12-13

15. **验证：`cargo test -p xclaw-memory` 全部通过**

### 阶段 4：公开 API 与 Facade 集成

16. **`lib.rs` re-export `SessionPolicy`**
    - 依赖：步骤 5

17. **`facade.rs` 新增 `fs_with_session_policy`**
    - 内部使用 `FsSessionStore::with_policy`
    - 依赖：步骤 11

18. **Facade 集成测试**
    - `fs_with_session_policy_uses_custom_policy`
    - 依赖：步骤 17

19. **验证：全部测试 + clippy**
    - `cargo test -p xclaw-memory && cargo clippy -p xclaw-memory -- -D warnings`

### 阶段 5：time_util 单元测试补充

20. **为 `time_util.rs` 编写全面单元测试**
    - `epoch_to_ymd_hms` 已知日期验证
    - `ymd_hms_to_epoch` 与 `epoch_to_ymd_hms` 往返一致性
    - `parse_iso8601_to_epoch` 正常格式、错误格式、边界值
    - 闰年（2024-02-29）和非闰年边界

## 风险与缓解

| 风险 | 级别 | 缓解 |
|------|------|------|
| 跨午夜算法错误（now 在 reset_at_hour 之前时需用昨天的 reset 点） | 中 | `is_expired` 纯函数 + 参数化测试覆盖午夜前后各种时间组合 |
| `ymd_hms_to_epoch` 逆运算实现错误 | 中 | 往返一致性测试 + 已知日期锚点（epoch 0 = 1970-01-01） |
| 同一 key 多 entry 查找逻辑变更影响现有行为 | 低 | 现有 24 个测试保障 + 新增测试增量验证 |
| `parse_iso8601_to_epoch` 遇到非标准格式 | 低 | `updated_at` 始终由 `now_utc()` 生成，格式固定；解析失败返回 `MemoryError` |

## 成功标准

- [ ] `is_expired` 纯函数覆盖 daily、idle、组合、边界共 10+ 个测试用例
- [ ] `time_util` 函数覆盖往返一致性和边界测试
- [ ] `get_or_create` 过期时创建新 session，未过期时复用旧 session
- [ ] `reset_session` 强制创建新 session，旧 session 保留在 index 中
- [ ] `get_by_key` 返回同一 key 下最新创建的 session
- [ ] `SessionStore::get_or_create` 签名不变，现有 24 个测试全部通过
- [ ] `cargo clippy -p xclaw-memory -- -D warnings` 无警告
- [ ] 新增代码测试覆盖率 80%+
