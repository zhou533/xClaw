# 2026-03-28: Session System 架构设计

## 变更类型

新增模块设计

## 概述

为 `xclaw-memory` 设计 Session 子系统，提供基于文件系统的会话索引和转录持久化能力。

## 变更内容

### 新增

- **`SessionKey`**（`xclaw-core::types`）：结构化会话标识符 `{role_id}:{scope}`，`role_id` 使用 `RoleId` 类型
- **`SessionEntry` / `SessionIndex` / `TranscriptRecord` / `SessionSummary`**（`xclaw-memory::session::types`）：会话数据结构
- **`SessionStore` trait**（`xclaw-memory::session::store`）：9 个方法（含 `delete_session`），`impl Future` 风格
- **`FsSessionStore`**（`xclaw-memory::session::fs_store`）：文件系统实现，JSON 索引 + JSONL 转录
- **`MemoryError` 5 个新变体**：`SessionNotFound`、`InvalidSessionKey`、`TranscriptParse`、`IndexCorrupted`、`JsonParse`

### 修改

- **`MemorySystem` facade**：泛型参数从 `<R, F, D>` 扩展为 `<R, F, D, S>`
- **`FsMemorySystem` 类型别名**：包含 `FsSessionStore`
- **workspace `Cargo.toml`**：新增 `uuid` 依赖

## 相关文档

- 架构设计：[session-system.md](../session-system.md)
- ADR：[ADR-006-session-system.md](../adr/ADR-006-session-system.md)
