<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="client/public/kaya-float.png">
    <img src="client/public/kaya-float.png" width="160" alt="KOE Logo">
  </picture>
  <h1 align="center">KOE — Kaya-On-Everywhere</h1>
  <p align="center">
    <em>把 LLM 带到你的桌面。文件传输、实时聊天、远程工具 —— 一个悬浮助手全部搞定。</em>
    <br>
    <a href="#功能特性"><strong>功能特性</strong></a> ·
    <a href="#快速开始"><strong>快速开始</strong></a> ·
    <a href="#架构"><strong>架构</strong></a> ·
    <a href="#技术栈"><strong>技术栈</strong></a>
    <br>
    <a href="README.en.md"><strong>English Version</strong></a>
  </p>

<p align="center">
  <img src="https://img.shields.io/badge/python-≥3.11-blue?logo=python" alt="Python">
  <img src="https://img.shields.io/badge/rust-≥1.80-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/vue-3-4FC08D?logo=vue.js" alt="Vue 3">
  <img src="https://img.shields.io/badge/tauri-2-FFC131?logo=tauri" alt="Tauri 2">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License">
</p>
</p>

---

## 概述

KOE 通过自托管桥接，将 LLM（Kaya）连接到你的 Windows 桌面。它实现了：

- **📁 文件推送** — 通过 MCP 工具从 LLM 直接发送文件到 Windows 机器
- **💬 实时聊天** — 通过 ACP（Agent Chat Protocol）与 LLM 在原生窗口中对话
- **🖥️ 远程工具** — 截屏、剪贴板读写、文件搜索 —— 将你的桌面暴露给 LLM
- **🖼️ 悬浮助手** — 透明、置顶的角色悬浮窗，配合可交互的消息气泡

系统由 **Python MCP 服务端**（运行在 Linux 或任何主机上）和 **Tauri 2 Windows 客户端**（运行在用户 PC 上）组成。

---

## 功能特性

- **📁 文件传输中枢** — 通过 WebSocket 从 LLM 推送文件到 Windows，支持二进制帧传输、SHA256 校验和、自动重试
- **💬 ACP 聊天** — 全双工 JSON-RPC 2.0 聊天，支持流式响应、Markdown 渲染、对话历史
- **🖼️ 悬浮角色** — 屏幕右下角透明置顶窗口，显示 Kaya 角色立绘
- **💭 消息气泡** — 自动堆叠、多列换行的通知气泡，5 秒去抖
- **⚡ 全局热键** — `Ctrl+Alt+K` 打开聊天、`Ctrl+Alt+S` 截屏、`Ctrl+Alt+C` 快捷命令
- **🖥️ 远程桌面工具** — 截屏（`screenshots`）、剪贴板读写（`arboard`）、文件搜索（`walkdir`）
- **🔐 客户端认证** — 预注册客户端 ID，bcrypt 哈希存储 passkey
- **🎨 主题化 UI** — CSS 变量设计系统，浅色主题，#6366F1 品牌色

---

## 架构

```
                                            ┌── OpenCode ACP 环境 ──┐
                                            │                        │
                                            │  opencode acp          │
                                            │  ▲  stdin/stdout       │
                                            │  │                     │
                                            │  └─┴───────────────────┤
                                            │  stdio-to-ws (:8765)   │
                                            │  @rebornix/stdio-to-ws │
                                            └──────────┬─────────────┘
                                                       │ ACP JSON-RPC WS
                                                       │
┌──────────────────────┐    MCP stdio      ┌───────────┴──────────────────┐
│   LLM (Kaya)         │ ◄────────────────►│  Python 服务端              │
│   通过 OpenCode       │                   │  (MCP + WebSocket + 信号)  │
└──────────────────────┘                   │  + SQLite 注册表            │
                                             └──────┬──────────┬─────────┘
                                                     │          │
                                        ┌────────────┘          └────────────┐
                                        │ WebSocket (passkey)   │ ACP WS    │
                                        │ port 9765              │ port 8765 │
                                        ▼                       ▼           ▼
                              ┌──────────────────────────────────────────────────────┐
                              │              Tauri Windows 客户端                   │
                              │                                                      │
                              │  ┌─ WS 9765 ──────────────────────────────────┐     │
                              │  │  文件传输 · 信号 · acp_inject · 远程工具    │     │
                              │  ├─ WS 8765 ──────────────────────────────────┤     │
                              │  │  ACP 聊天（直连 stdio-to-ws）              │     │
                              │  ├─ 主窗口 (960×640) ─────────────────────────┤     │
                              │  │ 侧栏 200px | 🏠 首页  💬 聊天  ⚙️ 设置    │     │
                              │  │               📁 文件  🖼️ 能力            │     │
                              │  └──────────────────────────────────────────────┘     │
                              │                                                      │
                              │  ┌─ 悬浮窗口（透明、置顶）────────────────────┐    │
                              │  │  🖼️ kaya-float   屏幕角落角色立绘          │    │
                              │  │  💬 bubble-*     消息气泡（自动堆叠）        │    │
                              │  │  ⚡ copilot      Copilot 查询浮层            │    │
                              │  └──────────────────────────────────────────────┘    │
                              │                                                      │
                              │  系统托盘 · 全局热键 · 桌面通知                     │
                              └──────────────────────────────────────────────────────┘
```

### 通信层

| 层级 | 协议 | 端口 | 用途 |
|------|------|------|------|
| 工具调用 | MCP stdio (JSON-RPC) | — | LLM 调用 `push_file`、`take_screenshot` 等 |
| 文件/信号 | WebSocket（二进制 + JSON） | **9765** | 三帧文件传输 · 信号 · 远程工具调用 |
| ACP 聊天 | ACP JSON-RPC 2.0  over WebSocket | **8765** | 用户与 LLM 之间的流式对话 |
| ACP 桥接 | stdio ↔ WebSocket | **8765** | `@rebornix/stdio-to-ws` 将 `opencode acp` 的 stdio 包装为 WebSocket |

---

## ACP 聊天桥接：@rebornix/stdio-to-ws

ACP（Agent Chat Protocol）聊天功能依赖一个额外的桥接组件：**[@rebornix/stdio-to-ws](https://github.com/rebornix/stdio-to-ws)**。

### 作用

`stdio-to-ws` 是一个 npm 包，它将一个 stdio 进程的 stdin/stdout 包装为 WebSocket 服务端。在这里，它把 `opencode acp` 的标准输入输出转换为 WebSocket，使得 Windows 客户端可以通过 WebSocket 与 LLM 进行 ACP 对话。

```
opencode acp (stdio)
     ▲     ▼
     │  stdin/stdout
     │
stdio-to-ws (:8765)
     ▲
     │  WebSocket (ACP JSON-RPC 2.0)
     │
Windows 客户端 (acp_client.rs)
```

### 安装

```bash
npm install -g @rebornix/stdio-to-ws
```

### 配置为 systemd 服务（opencode-bridge）

```ini
[Unit]
Description=ACP stdio-to-ws bridge for KOE
After=network.target

[Service]
Type=simple
ExecStart=npx @rebornix/stdio-to-ws --port 8766 -- /root/.opencode/bin/opencode acp
WorkingDirectory=/path/to/koe/server
Restart=always
RestartSec=3
User=your-user

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable opencode-bridge
sudo systemctl start opencode-bridge
```

> **注意：** `stdio-to-ws` 为每个 WebSocket 客户端连接 spawn 一个独立的 `opencode acp` 子进程。每个客户端的 ACP session 只在它对应的子进程中有效，因此信号注入必须通过客户端自身的 ACP 连接进行（即 `acp_inject` 走 9765 → 客户端 → 客户端自己的 ACP 连接 8765），而不能由服务端单独开连接注入。

### 连接流程

```
客户端 acp_client.rs                    stdio-to-ws (:8765)              opencode acp
      │                                      │                              │
      ├── WebSocket connect ────────────────►│                              │
      │                                      ├── spawn ───────────────────►│
      │                                      │                              │
      ├── {"jsonrpc":"2.0","id":1,           │                              │
      │    "method":"initialize",...}        │                              │
      │         ────────────────────────────►│──── stdin ──────────────────►│
      │                                      │                              │
      │◄── {"jsonrpc":"2.0","id":1,          │                              │
      │     "result":{"protocolVersion":1,...}│◄─── stdout ─────────────────│
      │         ◄────────────────────────────│                              │
      │                                      │                              │
      ├── session/new ──────────────────────►│──── stdin ──────────────────►│
      │◄── sessionId "ses_xxx" ◄────────────│◄─── stdout ─────────────────│
      │                                      │                              │
      ├── session/prompt ───────────────────►│──── stdin ──────────────────►│
      │◄── session/update (流式) ◄──────────│◄─── stdout ─────────────────│
      │◄── ResponseDone ◄───────────────────│◄─── stdout ─────────────────│
```

客户端发送的每条 JSON-RPC 消息末尾附加 `\n`（换行分隔），`stdio-to-ws` 也按换行切割 stdio 输出并转发为 WebSocket 文本帧。

---

## 快速开始

### 1. 部署服务端（推荐 Linux）

```bash
# 安装
cd server
pip install -e .

# 注册客户端
kaya-transfer-hub register-client --id my-pc --desc "我的 Windows 电脑" --passkey "your-secret"

# 启动服务
kaya-transfer-hub serve --ws-port 9765
```

### 2. 配置 MCP（opencode.json）

```json
{
  "mcpServers": {
    "kaya-transfer-hub": {
      "command": "kaya-transfer-hub",
      "args": ["serve", "--ws-port", "9765"]
    }
  }
}
```

### 3. 运行 Windows 客户端

**方式 A — 下载 Release**（待提供）
**方式 B — 本地编译：**

```bash
cd client
npm install
npm run tauri build
```

首次启动会自动跳转到**设置页**，配置以下内容：

| 字段 | 示例 | 说明 |
|------|------|------|
| WebSocket 地址 | `ws://192.168.1.100:9765` | 服务端地址 |
| 客户端 ID | `my-pc` | 注册时填写的 ID |
| Passkey | `your-secret` | 注册时设置的密钥 |
| 存储路径 | `~/kaya-transfer/` | 收到文件的保存位置 |

### 4. 开始使用

- **推送文件** — 让 Kaya 调用 `push_file` MCP 工具，文件会直接出现在你的存储目录中
- **聊天** — 按 `Ctrl+Alt+K` 或点击托盘菜单"打开对话"，与 Kaya 实时交流
- **截屏** — 请求 Kaya 截取当前屏幕，客户端自动捕获并返回给 LLM
- **搜索文件** — 让 Kaya 在 Windows 上帮你找文件

---

## CLI 命令

```bash
# 注册新客户端
kaya-transfer-hub register-client --id <id> --desc "<描述>" --passkey "<密钥>"

# 列出已注册客户端
kaya-transfer-hub list-clients

# 删除客户端
kaya-transfer-hub remove-client --id <id>

# 启动服务
kaya-transfer-hub serve --ws-port <端口>
```

---

## 技术栈

### 服务端（Linux）

| 组件 | 技术 |
|------|------|
| 运行环境 | Python ≥ 3.11 |
| 协议 | MCP SDK + WebSockets |
| 认证 | bcrypt |
| CLI | Click |

### 客户端（Windows）

| 层级 | 技术 |
|------|------|
| 桌面框架 | Tauri 2 (Rust) |
| 前端 | Vue 3 + Pinia + Vue Router + Vite |
| WebSocket | tokio-tungstenite |
| 截屏 | screenshots crate |
| 剪贴板 | arboard crate |
| 文件搜索 | walkdir crate |

### 通信

| 通道 | 协议 |
|------|------|
| 工具调用 | MCP stdio (JSON-RPC 2.0) |
| 文件传输 | WebSocket（二进制帧） |
| 聊天 | ACP JSON-RPC 2.0（流式） |
| 信号 | WebSocket (JSON) |

---

## 项目结构

```
koe/
├── server/                          # Python MCP 服务端
│   ├── src/kaya_transfer_hub/
│   │   ├── server.py                # MCP 工具注册
│   │   ├── ws_handler.py            # WebSocket 处理（认证、心跳、文件传输、信号）
│   │   ├── connection_manager.py    # 在线客户端连接池
│   │   ├── db.py                    # SQLite 客户端注册表
│   │   ├── models.py                # 客户端数据模型
│   │   ├── auth.py                  # Passkey bcrypt 哈希/验证
│   │   ├── cli.py                   # Click CLI
│   │   ├── tool_registry.py         # 客户端工具注册表
│   │   ├── signal_registry.py       # 信号调度（优先级、粘性、TTL）
│   │   └── constants.py             # 常量
│   ├── tests/
│   │   ├── test_tool_registry.py
│   │   └── test_signal_registry.py
│   ├── pyproject.toml
│   └── run_and_send.py              # 综合入口：服务端 + 文件推送 + ACP 看门狗
│
├── client/                          # Tauri 2 Windows 客户端
│   ├── public/
│   │   ├── kaya-float.png           # 角色立绘（方形）
│   │   └── kaya-full.png            # 角色立绘（全身）
│   ├── src/                         # Vue 3 前端
│   │   ├── views/                   # 页面
│   │   │   ├── HomePage.vue         # 连接状态 + 最近传输
│   │   │   ├── FileTransferPage.vue # 文件传输历史（气泡样式）
│   │   │   ├── ChatPage.vue         # ACP 聊天窗口
│   │   │   ├── SettingsPage.vue     # 服务端/存储配置
│   │   │   ├── CapabilitiesPage.vue # 工具、信号、快捷键
│   │   │   ├── FloatPage.vue        # 角色悬浮窗
│   │   │   ├── BubblePage.vue       # 消息气泡窗口
│   │   │   └── CopilotOverlayWindow.vue
│   │   ├── components/              # 可复用组件
│   │   ├── stores/                  # Pinia 状态管理
│   │   └── lib/                     # 工具库（类型、IPC、WebSocket 客户端）
│   ├── src-tauri/                   # Rust 后端
│   │   ├── src/
│   │   │   ├── lib.rs               # 应用初始化、命令、ACP、气泡管理
│   │   │   ├── config.rs            # 本地配置读写
│   │   │   ├── ws_client.rs         # WebSocket 文件传输客户端
│   │   │   ├── acp_client.rs        # ACP JSON-RPC 2.0 客户端
│   │   │   ├── file_handler.rs      # 文件保存
│   │   │   ├── tray.rs              # 系统托盘
│   │   │   ├── notify.rs            # 桌面通知
│   │   │   ├── copilot.rs           # Copilot 引擎
│   │   │   ├── signal_emitter.rs    # 信号发射器
│   │   │   ├── tool_executor.rs     # 本地工具执行（截屏、剪贴板、文件搜索）
│   │   │   └── uia_tree.rs          # UIA 无障碍树（桩）
│   │   ├── Cargo.toml
│   │   └── tauri.conf.json
│   ├── package.json
│   └── vite.config.ts
│
├── docs/
│   ├── protocol.md                  # WebSocket 文件传输协议规范
│   └── superpowers/                 # 设计文档和实现计划
│
├── README.md                        # 本文件（简体中文）
├── README.en.md                     # 英文版
└── build.bat                        # Windows 构建脚本
```

---

## 通信协议

文件传输采用 **三帧 WebSocket 序列**：

1. **元数据** (JSON) — `{"type": "file_meta", "file_id": "...", "name": "...", "size": N}`
2. **二进制内容** — 原始文件字节流
3. **结束帧** (JSON) — `{"type": "file_end", "file_id": "...", "checksum": "sha256:..."}`

客户端认证使用预注册 ID + bcrypt 哈希 passkey。详见 [docs/protocol.md](docs/protocol.md)。

---

## 安全说明

- Passkey 以 **bcrypt 哈希** 存储，不存明文
- 认证使用**常量时间比较**，防止时序攻击
- 客户端必须**预注册**，拒绝未授权连接
- 文件完整性通过 **SHA256 校验和** 验证
- ⚠️ 当前使用 **ws:// 明文传输**，生产环境请配置 wss://

---

## systemd 自启

### KOE 服务端

```ini
[Unit]
Description=KOE File Transfer Hub
After=network.target

[Service]
Type=simple
ExecStart=/usr/bin/python3 /path/to/koe/server/run_and_send.py 9765
WorkingDirectory=/path/to/koe/server
Restart=always
RestartSec=3
User=your-user

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable koe
sudo systemctl start koe
```

### ACP 桥接（opencode-bridge）

```ini
[Unit]
Description=ACP stdio-to-ws bridge for KOE
After=network.target

[Service]
Type=simple
ExecStart=npx @rebornix/stdio-to-ws --port 8766 -- /root/.opencode/bin/opencode acp
WorkingDirectory=/path/to/koe/server
Restart=always
RestartSec=3
User=your-user

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable opencode-bridge
sudo systemctl start opencode-bridge
```

---

## 路线图

- [x] MCP 文件传输（从 LLM 推送文件到客户端）
- [x] ACP 聊天（实时双向对话）
- [x] 远程工具（截屏、剪贴板、文件搜索）
- [x] 悬浮角色立绘
- [x] 多列消息气泡
- [ ] 端到端加密（wss://）
- [ ] 客户端到服务端文件推送（双向传输）
- [ ] macOS 客户端
- [ ] 自定义客户端工具插件系统

---

## 贡献

欢迎贡献！本项目正在积极开发中。欢迎提交 Issue 和 Pull Request。

---

## 许可

MIT
