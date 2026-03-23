# xClaw 架构设计文档

> 版本：1.0 | 日期：2026-03-22 | 状态：Proposed

## 1. 概述

xClaw 是一个类 OpenClaw 的个人 AI 助手平台，采用 Rust 核心 + Svelte 5 前端的技术栈。支持 macOS/Windows 桌面客户端与云端 Server 部署两种运行模式，面向个人用户使用。

### 1.1 设计目标

| 目标 | 描述 |
|------|------|
| 跨平台 | 原生支持 macOS 和 Windows |
| 轻量高效 | 参照 ZeroClaw 的 Rust 单体二进制理念，追求低内存占用与快速启动 |
| 双模运行 | 桌面模式（Tauri）与服务器模式（Headless + Web UI）共用同一套核心逻辑 |
| 单用户 | 面向个人使用，无需多租户和复杂权限体系 |
| 可扩展 | 通道（Channel）、工具（Tool）、LLM 提供商（Provider）均可插拔 |

### 1.2 非功能性需求

- **启动时间**：< 500ms（桌面模式），< 100ms（CLI 模式）
- **内存占用**：空闲状态 < 30MB（不含 LLM 上下文）
- **并发处理**：支持同时处理多通道消息
- **安全性**：本地密钥加密存储，通道消息端到端不落盘

---

## 2. 高层架构

```
┌─────────────────────────────────────────────────────────┐
│                    运行模式层 (Runtime)                    │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐   │
│  │  Tauri 桌面   │  │  CLI 命令行   │  │  Server 模式  │   │
│  │  (macOS/Win) │  │              │  │  (云端部署)   │   │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘   │
│         │                 │                 │           │
│  ┌──────┴─────────────────┴─────────────────┴───────┐   │
│  │              Gateway 控制平面 (axum)               │   │
│  │         HTTP REST + WebSocket + 静态文件            │   │
│  └──────────────────────┬───────────────────────────┘   │
│                         │                               │
│  ┌──────────────────────┴───────────────────────────┐   │
│  │               xclaw-core 核心层                    │   │
│  │  ┌─────────┐ ┌─────────┐ ┌────────┐ ┌─────────┐ │   │
│  │  │ Agent   │ │ Session │ │ Memory │ │ Config  │ │   │
│  │  │ Loop    │ │ Manager │ │ Store  │ │ Manager │ │   │
│  │  └────┬────┘ └─────────┘ └────────┘ └─────────┘ │   │
│  │       │                                          │   │
│  │  ┌────┴────────────────────────────────────────┐ │   │
│  │  │           Tool Dispatch Engine              │ │   │
│  │  └─────────────────────────────────────────────┘ │   │
│  └──────────────────────────────────────────────────┘   │
│                                                         │
│  ┌─────────────────────┐  ┌──────────────────────────┐  │
│  │  Provider 抽象层     │  │  Channel 通道层           │  │
│  │  (Claude, OpenAI,   │  │  (Telegram, Slack,       │  │
│  │   Ollama, ...)      │  │   Discord, WebChat, ...) │  │
│  └─────────────────────┘  └──────────────────────────┘  │
└─────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────┐
│                  前端层 (Svelte 5 + Vite)                 │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────┐  │
│  │ 对话界面  │ │ 记忆浏览  │ │ 配置管理  │ │ 系统监控   │  │
│  └──────────┘ └──────────┘ └──────────┘ └───────────┘  │
│  Tauri 模式：内嵌于桌面窗口 / Server 模式：由 Gateway 提供  │
└─────────────────────────────────────────────────────────┘
```

---

## 3. 项目结构

```
xclaw/
├── Cargo.toml                    # Workspace 根配置
├── crates/
│   ├── xclaw-core/               # 核心运行时
│   │   ├── src/
│   │   │   ├── agent/            # Agent 循环、Prompt 构建
│   │   │   ├── session/          # 会话管理与隔离
│   │   │   ├── memory/           # 对话记忆与持久化
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   ├── xclaw-provider/           # LLM 提供商抽象
│   │   ├── src/
│   │   │   ├── traits.rs         # Provider trait 定义
│   │   │   ├── claude.rs         # Anthropic Claude
│   │   │   ├── openai.rs         # OpenAI 兼容
│   │   │   ├── ollama.rs         # 本地 Ollama
│   │   │   └── router.rs         # 模型路由与 Failover
│   │   └── Cargo.toml
│   ├── xclaw-gateway/            # HTTP/WS 控制平面
│   │   ├── src/
│   │   │   ├── http/             # REST API 端点
│   │   │   ├── ws/               # WebSocket 处理
│   │   │   ├── static_files.rs   # 前端静态资源服务
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   ├── xclaw-channel/            # 消息通道
│   │   ├── src/
│   │   │   ├── traits.rs         # Channel trait 定义
│   │   │   ├── telegram.rs
│   │   │   ├── slack.rs
│   │   │   ├── discord.rs
│   │   │   └── webchat.rs        # 内置 WebChat 通道
│   │   └── Cargo.toml
│   ├── xclaw-tools/              # 工具生态
│   │   ├── src/
│   │   │   ├── registry.rs       # 工具注册与分发
│   │   │   ├── shell.rs          # Shell 命令执行
│   │   │   ├── file_io.rs        # 文件读写
│   │   │   ├── web_fetch.rs      # HTTP 请求
│   │   │   └── browser.rs        # 浏览器控制
│   │   └── Cargo.toml
│   └── xclaw-config/             # 配置管理
│       ├── src/
│       │   ├── model.rs          # 配置数据结构
│       │   ├── loader.rs         # 多源配置加载
│       │   ├── secrets.rs        # 密钥安全存储
│       │   └── lib.rs
│       └── Cargo.toml
├── apps/
│   ├── cli/                      # CLI 入口
│   │   ├── src/main.rs           # clap 命令解析
│   │   └── Cargo.toml
│   ├── desktop/                  # Tauri v2 桌面应用
│   │   ├── src-tauri/
│   │   │   ├── src/
│   │   │   │   ├── main.rs       # Tauri 入口
│   │   │   │   ├── commands.rs   # Tauri IPC 命令
│   │   │   │   └── tray.rs       # 系统托盘
│   │   │   ├── Cargo.toml
│   │   │   └── tauri.conf.json
│   │   └── ...                   # 前端由 frontend/ 构建嵌入
│   └── server/                   # Server 模式入口
│       ├── src/main.rs           # Headless 启动
│       └── Cargo.toml
├── frontend/                     # Svelte 5 Web 前端（共享）
│   ├── src/
│   │   ├── lib/
│   │   │   ├── components/       # UI 组件
│   │   │   ├── stores/           # 状态管理
│   │   │   ├── api/              # Gateway API 客户端
│   │   │   └── types/            # TypeScript 类型
│   │   ├── routes/
│   │   │   ├── +page.svelte      # 对话主界面
│   │   │   ├── memory/           # 记忆浏览
│   │   │   ├── config/           # 配置管理
│   │   │   └── monitor/          # 系统监控
│   │   └── app.html
│   ├── package.json
│   ├── svelte.config.js
│   └── vite.config.ts
└── docs/
    └── architecture/
```

---

## 4. 核心组件设计

### 4.1 Agent Loop（智能体循环）

Agent Loop 是系统的核心引擎，负责接收用户消息、构建提示词、调用 LLM、分发工具调用。

```
消息输入 → Session 上下文加载 → Memory 注入 → Prompt 构建
    → LLM 调用 → 响应解析 → [工具调用 → 结果回注 → 再次 LLM] 循环
    → 最终响应 → 通道回复 → Memory 持久化
```

**关键设计**：
- 使用 Tokio 异步运行时驱动整个循环
- Tool 调用采用结构化 JSON Schema 描述，与 LLM 的 function calling 对接
- 支持流式响应（SSE/WebSocket），提升用户体验
- 循环上限保护：单次对话最多 N 轮工具调用，防止失控

### 4.2 Provider 抽象层

```rust
// 核心 trait 定义（伪代码）
trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    async fn chat_stream(&self, request: ChatRequest) -> Result<impl Stream<Item = ChatChunk>>;
    fn supported_features(&self) -> ProviderFeatures;
}
```

**路由策略**：
- 主模型 + 降级模型配置
- 自动 Failover：主模型失败后切换到备用
- 按任务类型路由（对话用 Sonnet，复杂推理用 Opus）

### 4.3 Gateway 控制平面

基于 axum 构建的 HTTP/WS 服务器，是所有外部交互的统一入口。

**REST API**：
| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/chat` | POST | 发送消息 |
| `/api/chat/stream` | GET (SSE) | 流式对话 |
| `/api/sessions` | GET/POST/DELETE | 会话管理 |
| `/api/memory` | GET/POST/DELETE | 记忆管理 |
| `/api/config` | GET/PUT | 配置读写 |
| `/api/channels` | GET/PUT | 通道状态 |
| `/api/tools` | GET | 工具列表 |
| `/api/health` | GET | 健康检查 |

**WebSocket**：
- `/ws/chat` — 实时对话（双向流式）
- `/ws/events` — 系统事件推送（状态变化、通道消息）

### 4.4 Channel 通道层

```rust
trait Channel: Send + Sync {
    async fn start(&self, sender: MessageSender) -> Result<()>;
    async fn send(&self, message: OutgoingMessage) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    fn channel_type(&self) -> ChannelType;
}
```

每个 Channel 实现独立运行在自己的 Tokio task 中，通过 `mpsc` channel 与 Agent Loop 通信。

### 4.5 Memory Store

采用分层存储策略：

| 层级 | 存储 | 用途 |
|------|------|------|
| 热数据 | 内存（`DashMap`） | 当前活跃会话上下文 |
| 温数据 | SQLite | 近期对话历史、配置 |
| 冷数据 | 文件系统（JSON） | 长期记忆、导出备份 |

**向量搜索**：可选集成 `qdrant`（嵌入式模式）或 SQLite FTS5 进行语义检索。

### 4.6 Config Manager

```
配置加载优先级（高 → 低）：
  1. 命令行参数
  2. 环境变量（XCLAW_*）
  3. 用户配置文件（~/.xclaw/config.toml）
  4. 项目配置文件（.xclaw/config.toml）
  5. 内置默认值
```

密钥使用操作系统原生密钥环存储：
- macOS: Keychain
- Windows: Windows Credential Manager

---

## 5. 运行模式

### 5.1 桌面模式（Tauri v2）

```
用户启动 xClaw.app / xClaw.exe
  → Tauri 初始化
  → 启动嵌入式 Gateway（绑定 127.0.0.1:随机端口）
  → 加载 Svelte 前端（Tauri webview）
  → 系统托盘常驻
  → Channel 连接（按配置自动启动）
```

**Tauri IPC 命令**（Rust ↔ 前端通信）：
- 文件对话框、系统通知、剪贴板、窗口控制
- 直接调用 xclaw-core 函数，跳过 HTTP（性能优化）

### 5.2 CLI 模式

```
xclaw chat "你好"              # 单次对话
xclaw chat --interactive       # 交互式对话
xclaw serve                    # 启动 Server 模式
xclaw config set provider.default claude
xclaw channel list
```

CLI 入口使用 `clap` 解析命令，直接调用 xclaw-core。

### 5.3 Server 模式（云端部署）

```
xclaw serve --bind 0.0.0.0:8080
  → 启动 Gateway（HTTP/WS）
  → 提供 Web UI（Svelte 构建产物）
  → Channel 连接
  → 无窗口、无 Tauri 依赖
```

部署选项：
- Docker 单容器部署
- 直接运行二进制文件
- systemd / launchd 服务

---

## 6. 数据流

### 6.1 对话消息流

```
[用户] → Channel/WebChat/CLI
         │
         ▼
    ┌─────────┐
    │ Gateway  │ ← 路由到对应 Session
    └────┬────┘
         ▼
    ┌─────────┐
    │ Agent   │ ← 加载 Session 上下文 + Memory
    │ Loop    │ ← 构建 Prompt
    └────┬────┘
         ▼
    ┌─────────┐
    │ Provider│ ← LLM API 调用（流式）
    └────┬────┘
         ▼
    ┌─────────┐
    │ Tool    │ ← 如果 LLM 请求工具调用
    │ Dispatch│ ← 执行工具 → 结果回注 Agent Loop
    └────┬────┘
         ▼
    ┌─────────┐
    │ Response│ ← 流式推送到前端/通道
    └────┬────┘
         ▼
    ┌─────────┐
    │ Memory  │ ← 持久化对话记录
    └─────────┘
```

### 6.2 配置热更新流

```
Web UI / config.toml 修改
  → Config Manager 检测变更
  → 广播 ConfigChanged 事件
  → 各组件响应更新（Channel 重连、Provider 切换等）
  → 无需重启进程
```

---

## 7. 技术选型总览

| 领域 | 技术 | 理由 | 详见 |
|------|------|------|------|
| 核心运行时 | Rust + Tokio | 高性能、低资源、跨平台编译 | 需求约束 |
| HTTP/WS 框架 | axum | Rust 生态最成熟的异步 Web 框架，Tower 中间件生态 | ADR-001 |
| 桌面 Shell | Tauri v2 | Rust 原生、体积小、macOS/Windows 原生支持 | ADR-002 |
| Web 前端 | Svelte 5 + Vite | 编译时框架、零运行时开销、bundle 小 | ADR-003 |
| 数据存储 | SQLite (rusqlite) | 零依赖嵌入式数据库、单用户场景最优 | ADR-004 |
| 序列化 | serde + toml/json | Rust 标准序列化方案 | — |
| CLI 解析 | clap v4 | Rust 生态标准 CLI 框架 | — |
| 日志 | tracing | 结构化日志 + 分布式追踪 | — |
| 密钥存储 | keyring-rs | 跨平台原生密钥环封装 | — |
| 构建 | Cargo workspace + pnpm | Rust workspace 统一管理，pnpm 管理前端依赖 | — |

---

## 8. 安全设计

### 8.1 密钥管理

- API Key 等敏感信息存储在 OS 原生密钥环中，不落配置文件
- 配置文件中的密钥字段支持 `env:XCLAW_OPENAI_KEY` 引用语法
- 运行时密钥仅在需要时加载到内存，不持久化到日志

### 8.2 工具沙箱

- Shell 工具执行：命令白名单 + 路径限制
- 文件 I/O：工作区隔离，禁止路径穿越
- Web Fetch：可配置域名白名单
- 自治等级（Autonomy Level）：ReadOnly / Supervised / Full

### 8.3 通信安全

- Server 模式建议通过反向代理（Nginx/Caddy）启用 TLS
- WebSocket 连接支持 token 认证
- 桌面模式 Gateway 仅绑定 127.0.0.1

---

## 9. 构建与部署

### 9.1 构建流水线

```
1. pnpm --prefix frontend build     # 构建 Svelte 前端
2. cargo build --release             # 构建 Rust 二进制
   - apps/cli     → xclaw            # CLI + Server 二进制
   - apps/desktop → xClaw.app/.exe   # Tauri 桌面应用（内嵌前端）
   - apps/server  → xclaw-server     # 纯 Server 二进制（无 Tauri 依赖）
```

### 9.2 交叉编译目标

| 平台 | Target | 产物 |
|------|--------|------|
| macOS (Apple Silicon) | `aarch64-apple-darwin` | xClaw.app, xclaw |
| macOS (Intel) | `x86_64-apple-darwin` | xClaw.app, xclaw |
| Windows | `x86_64-pc-windows-msvc` | xClaw.exe, xclaw.exe |
| Linux (Server) | `x86_64-unknown-linux-musl` | xclaw-server |

### 9.3 Docker 部署

```dockerfile
FROM rust:1-alpine AS builder
# 多阶段构建，最终镜像 < 30MB
FROM alpine:3
COPY --from=builder /app/xclaw-server /usr/local/bin/
COPY --from=builder /app/frontend/build /usr/share/xclaw/web
EXPOSE 8080
CMD ["xclaw-server", "--bind", "0.0.0.0:8080"]
```

---

## 10. 可扩展性规划

本项目面向个人使用，但架构预留了合理的扩展点：

| 阶段 | 规模 | 架构不变 |
|------|------|---------|
| 当前 | 单用户 | 嵌入式 SQLite + 单进程 |
| 未来可选 | 家庭/小团队 | 添加简单 token 认证，数据库不变 |
| 极端场景 | 多设备同步 | 替换 SQLite 为 PostgreSQL，添加同步层 |

**插件扩展点**：
- `Provider` trait：新增 LLM 后端
- `Channel` trait：新增消息通道
- `Tool` trait：新增工具能力
- 前端组件：Svelte 组件化架构天然支持扩展

---

## 11. 关键设计决策

详见 ADR 文档：

- **ADR-001**：选用 axum 作为 Gateway 框架
- **ADR-002**：选用 Tauri v2 作为桌面 Shell
- **ADR-003**：选用 Svelte 5 作为前端框架
- **ADR-004**：选用 SQLite 作为数据存储

---

## 12. 风险与缓解

| 风险 | 影响 | 缓解策略 |
|------|------|---------|
| Tauri v2 WebView 在 Windows 上兼容性问题 | 部分旧 Windows 系统 WebView2 缺失 | 安装包内嵌 WebView2 Bootstrapper |
| LLM API 调用延迟影响用户体验 | 对话响应慢 | 流式响应 + 本地 Ollama 降级 |
| SQLite 并发写入限制 | 多通道同时写入冲突 | WAL 模式 + 写入队列序列化 |
| 前端与桌面共享带来的耦合 | 维护复杂度增加 | 前端通过 API 客户端抽象通信层，Tauri IPC 仅作性能优化路径 |
