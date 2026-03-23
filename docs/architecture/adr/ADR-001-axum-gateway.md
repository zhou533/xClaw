# ADR-001: 选用 axum 作为 Gateway 框架

## 背景

xClaw 需要一个 HTTP/WebSocket 控制平面来承载 REST API、WebSocket 实时通信和静态文件服务。该框架需要满足：
- 纯 Rust 实现，与核心层无缝集成
- 高性能异步处理
- 成熟的中间件生态（日志、认证、CORS）
- WebSocket 原生支持

## 决策

选用 **axum** 作为 Gateway 层的 HTTP/WS 框架。

axum 由 Tokio 团队维护，基于 Tower 中间件栈和 hyper HTTP 库构建。它提供：
- 类型安全的路由提取器（Extractor）
- 与 Tokio 生态无缝集成
- Tower 中间件复用（tower-http 提供压缩、CORS、tracing 等）
- 原生 WebSocket 升级支持
- 社区活跃，文档充分

## 影响

### 正面影响
- 与 Tokio 运行时零摩擦集成，xclaw-core 也基于 Tokio
- Tower 中间件可跨 Gateway 和内部服务复用
- 编译时路由检查，减少运行时错误
- 生态成熟，第三方集成丰富（axum-extra, tower-sessions 等）

### 负面影响
- 学习曲线略高于 Actix-web（Extractor 模式需要理解）
- 宏使用较少，路由定义略显冗长

### 备选方案
- **Actix-web**：成熟稳定，但 Actor 模型与我们的架构不完全匹配，且与 Tokio 生态存在部分不兼容
- **Warp**：基于 Filter 的组合式路由，灵活但复杂项目中可读性差
- **Rocket**：API 优雅，但异步支持较晚，生态相对较小

## 状态
Proposed

## 日期
2026-03-22
