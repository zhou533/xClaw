# Architecture Changelog - 2026-03-22

## Summary
初始架构设计：定义 xClaw 个人 AI 助手平台的整体技术架构，涵盖核心运行时、桌面客户端、Web 前端与云端部署方案。

### Added
- 高层架构设计：Rust 核心层 + axum Gateway + Svelte 5 前端 + Tauri v2 桌面 Shell
- Cargo workspace 项目结构：6 个核心 crate + 3 个应用入口
- 三种运行模式定义：桌面模式、CLI 模式、Server 模式
- Agent Loop 智能体循环设计
- Provider 抽象层（LLM 提供商可插拔）
- Channel 通道层（消息平台可插拔）
- Tool Dispatch 工具分发引擎
- Memory Store 分层存储策略（内存 → SQLite → 文件系统）
- Config Manager 多源配置加载与密钥管理
- Gateway REST API 与 WebSocket 端点定义
- 安全设计：密钥环存储、工具沙箱、自治等级
- 构建与部署方案：交叉编译目标、Docker 部署
- ADR-001 至 ADR-004 架构决策记录

## Context
项目启动阶段，需要建立技术基线。参照 OpenClaw（Node.js 实现）和 ZeroClaw（Rust 实现）的设计理念，结合 xClaw 的具体需求（macOS/Windows 桌面 + 云端部署 + 单用户）进行架构设计。

## Related ADR
ADR-001, ADR-002, ADR-003, ADR-004
