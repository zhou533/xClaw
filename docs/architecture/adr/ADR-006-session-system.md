# ADR-006: Session System 架构

> 状态：Accepted | 日期：2026-03-28

## 上下文

xClaw 的 `xclaw-memory` crate 提供了 Role 管理、工作区记忆和日常记忆，但缺少会话（Session）管理能力。现有的 `SessionId` 仅是不透明的字符串包装类型，没有持久化、索引和转录记录机制。Agent 层需要结构化的会话上下文来构建 Prompt 和进行 Memory Extraction。

## 决策

在 `xclaw-memory` 中新增 `session` 子模块，提供基于文件系统的会话索引和转录持久化。

### 关键设计决策

#### 1. 双标识符体系

- **SessionKey**（`{role_id}:{scope}`）：外部语义标识符，由 channel adapter 定义，用于 `get_or_create`
- **SessionId**（UUID v4）：内部存储标识符，用于文件名和后续操作
- 二者通过 `SessionEntry` 关联

#### 2. 文件格式

- **会话索引**：`sessions/sessions.json`（JSON），含 `version` 字段支持迁移
- **转录记录**：`sessions/{session_id}.jsonl`（JSONL），追加 O(1)
- 选择 JSON/JSONL 而非 SQLite，遵循项目文件优先原则

#### 3. SessionKey 类型安全

`SessionKey.role_id` 使用 `RoleId` 类型而非裸 `String`，复用已有的 snake_case 验证逻辑，避免重复验证。

#### 4. Trait 风格

`SessionStore` 使用 `impl Future`（非 dyn-safe），与项目中 `RoleManager`、`DailyMemory`、`MemoryStore` 等 trait 保持一致。

#### 5. TranscriptRecord 扩展性

包含 `metadata: Option<serde_json::Value>` 字段，为 Agent 层附加 token 计数、模型名称等信息预留扩展点。JSONL 格式一旦生产使用，逐字段迁移成本高，一个 metadata 字段提供灵活扩展且不影响默认序列化。

#### 6. 索引写入原子性

使用 `tempfile::NamedTempFile` + `persist()` 防止中断导致索引损坏，与 `FsMemoryFileLoader::save_file` 模式一致。

#### 7. 并发模型

Session 层不持有内部锁。**调用方（Agent 层）必须保证同一 SessionKey 的操作串行执行**。这是单用户场景下的务实选择，避免引入不必要的锁复杂度。

## 备选方案

| 决策点 | 选择 | 备选 | 理由 |
|--------|------|------|------|
| 索引存储 | JSON 文件 | SQLite | 文件优先原则；会话数有限；人类可读 |
| 转录存储 | JSONL | SQLite / JSON 数组 | 追加 O(1)；流式友好 |
| SessionKey.role_id | `RoleId` 类型 | 裸 `String` | 复用验证，类型安全 |
| Trait 风格 | `impl Future` | `#[async_trait]` | 与项目一致 |
| 扩展策略 | `Option<T>` + metadata | `serde(flatten) HashMap` | 类型安全、可审查 |

## 影响

- `xclaw-core::types` 新增 `SessionKey` 类型
- `xclaw-memory` 新增 `session` 子模块（types、store、fs_store）
- `MemoryError` 新增 5 个变体
- `MemorySystem` 泛型参数从 `<R, F, D>` 扩展为 `<R, F, D, S>`
- workspace 新增 `uuid` 依赖
- 现有 API 需更新 `FsMemorySystem` 类型别名和构造函数
