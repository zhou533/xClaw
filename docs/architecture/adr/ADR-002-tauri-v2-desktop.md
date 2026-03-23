# ADR-002: 选用 Tauri v2 作为桌面 Shell

## 背景

xClaw 需要在 macOS 和 Windows 上提供原生桌面客户端体验，包括：
- 系统托盘常驻
- 原生窗口与通知
- 内嵌 Web UI（与 Server 模式共享前端）
- 与 Rust 核心层直接集成，避免 IPC 序列化开销

## 决策

选用 **Tauri v2** 作为桌面 Shell 框架。

Tauri v2 是一个 Rust 原生的桌面应用框架，使用操作系统原生 WebView 渲染前端界面：
- macOS: WKWebView
- Windows: WebView2 (Chromium-based)

核心优势在于后端完全由 Rust 编写，可以将 xclaw-core 作为库依赖直接调用，无需额外的进程间通信。

## 影响

### 正面影响
- **Rust 原生**：后端就是 Rust，xclaw-core 可作为 crate 直接链接，函数调用级别的集成
- **体积极小**：打包产物 macOS ~10MB、Windows ~5MB（不含 WebView2），远小于 Electron 的 ~150MB
- **内存占用低**：利用系统 WebView，不内嵌 Chromium
- **跨平台**：macOS + Windows 开箱即用
- **安全模型**：细粒度的 IPC 权限控制，前端不能随意调用系统 API
- **插件生态**：v2 引入插件系统，系统托盘、通知、文件对话框等均为官方插件

### 负面影响
- **WebView 一致性**：macOS WKWebView 与 Windows WebView2 存在渲染差异，需测试
- **Windows WebView2 依赖**：部分旧 Windows 系统需安装 WebView2 Runtime（安装包可内嵌 Bootstrapper）
- **调试体验**：不如 Electron 的 Chrome DevTools 完善（但 Tauri v2 已大幅改进）

### 备选方案
- **Electron**：成熟稳定，但体积和内存占用过大，且后端为 Node.js 无法直接复用 Rust 核心（需 FFI/NAPI）
- **Neutralinojs**：更轻量，但生态太小，稳定性不足
- **自研 native GUI (iced/egui)**：纯 Rust 渲染，但无法与 Server 模式共享前端代码，开发成本高
- **Flutter Desktop**：跨平台能力强，但与 Rust 集成需要 FFI，增加复杂度

## 状态
Proposed

## 日期
2026-03-22
