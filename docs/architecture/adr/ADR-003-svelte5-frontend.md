# ADR-003: 选用 Svelte 5 作为前端框架

## 背景

xClaw 的前端需要同时服务于两个运行模式：
1. **桌面模式**：内嵌于 Tauri WebView，作为桌面应用界面
2. **Server 模式**：作为 Web UI 由 Gateway 提供，通过浏览器访问

前端需求包括：对话界面（含流式渲染）、记忆浏览器、配置管理、系统监控面板。
关键约束：Tauri WebView 中 bundle 体积直接影响启动速度和安装包大小。

## 决策

选用 **Svelte 5**（搭配 Vite 构建）作为 Web 前端框架。

Svelte 5 引入了 Runes 响应式系统，是一个编译时框架——组件在构建阶段被编译为高效的原生 DOM 操作代码，运行时不依赖虚拟 DOM。

## 影响

### 正面影响
- **极小 Bundle**：无运行时框架代码，典型应用 bundle < 50KB gzip，适合 Tauri 内嵌
- **高性能**：编译时生成精确的 DOM 更新，无 diffing 开销
- **简洁语法**：Svelte 5 Runes（`$state`, `$derived`, `$effect`）提供直观的响应式 API
- **内置能力**：过渡动画、CSS 作用域、Store 模式内置，减少第三方依赖
- **快速开发**：样板代码少，开发体验好

### 负面影响
- **生态规模**：组件库和社区比 React 小（但对于本项目的 UI 复杂度足够）
- **团队经验**：如果团队更熟悉 React，有一定学习成本
- **SSR 场景**：SvelteKit 的 SSR 在 Tauri 内嵌场景下不需要，需配置为 SPA 模式

### 备选方案
- **React 19 + Vite**：生态最大，但 bundle 体积较大（react + react-dom ~45KB gzip），运行时虚拟 DOM 有性能开销
- **SolidJS**：性能接近 Svelte，但生态更小，社区更不成熟
- **Vue 3**：中庸选择，但没有 Svelte 的编译时优势
- **Vanilla + Web Components**：最轻量，但开发效率低，不适合复杂 UI

## 状态
Proposed

## 日期
2026-03-22
