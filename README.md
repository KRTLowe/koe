<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="client/public/kaya-float.png">
    <img src="client/public/kaya-float.png" width="160" alt="Kaya Logo">
  </picture>
  <h1 align="center">kaya-transfer-hub</h1>
  <p align="center">
    <em>把 LLM 带到你的桌面 —— 文件传输、实时聊天、远程工具、悬浮助手</em>
  </p>

<p align="center">
  <img src="https://img.shields.io/badge/python-≥3.11-blue?logo=python" alt="Python">
  <img src="https://img.shields.io/badge/rust-≥1.80-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/vue-3-4FC08D?logo=vue.js" alt="Vue 3">
  <img src="https://img.shields.io/badge/tauri-2-FFC131?logo=tauri" alt="Tauri 2">
  <img src="https://img.shields.io/badge/license-MIT-green" alt="License">
</p>

---

## 架构

```
┌─ Kaya (LLM via OpenCode) ──────────────────┐
│  MCP stdio → kaya-transfer-hub              │
└─────────────────┬───────────────────────────┘
                  │
┌─────────────────┴──────────────────────────────┐
│      kaya-transfer-hub (Python 服务端)          │
│                                                  │
│  ┌─ kaya_transfer_hub ──────────────────┐       │
│  │  MCP 工具注册 (8 tools)               │       │
│  │   serve 命令 / mcp 子命令 (Unix socket)│       │
│  │  CLI：客户端管理 + 工具代理             │       │
│  └──────────────────────────────────────┘       │
│                                                  │
│  ┌─ kaya_server (运行时) ─────────────────┐      │
│  │  WS 9765 → 认证/心跳/文件传输/工具调用   │      │
│  │  ACP 8765 → Python 原生桥 (常驻 opencode)│      │
│  │  信号调度 → 优先级队列 + 粘性/一次性信号  │      │
│  │  SQLite → 客户端注册表持久化             │      │
│  │  bcrypt → 客户端认证                    │      │
│  └──────────────────────────────────────┘       │
└──────────────────┬───────────┬──────────────────┘
                   │ WS 9765   │ ACP 8765
                   ▼           ▼
┌─────────────────────────────────────────────────┐
│  Tauri 2 Windows 客户端 (Rust + Vue 3)           │
│                                                   │
│  ┌─ 核心通信 ──────────────────────────────┐     │
│  │  WS Client: 认证/重连/工具调用/文件收发/信号 │     │
│  │  ACP Client: JSON-RPC/僵尸检测/流式chunk   │     │
│  └─────────────────────────────────────────┘     │
│                                                   │
│  ┌─ 远程工具 (21 个) ─────────────────────┐     │
│  │  截屏/OCR/UIA树/文件读写/剪贴板/输入    │     │
│  │  grep/进程管理/窗口列表/系统信息        │     │
│  └─────────────────────────────────────────┘     │
│                                                   │
│  ┌─ 界面 ────────────────────────────────┐       │
│  │  透明悬浮角色窗口 + 消息气泡           │       │
│  │  ACP 聊天 / 快速聊天 / 设置 / 文件传输  │       │
│  │  Copilot 查询浮层 (单次 / 持续监测)    │       │
│  │  工具调用状态浮层                      │       │
│  └─────────────────────────────────────────┘     │
└─────────────────────────────────────────────────┘
```

---

## 快速开始

### 前置条件

| 组件 | Linux (服务端) | Windows (客户端构建) |
|------|---------------|-------------------|
| Python | ≥ 3.11 | — |
| Rust | — | ≥ 1.80 (rustup.rs) |
| Node.js | — | ≥ 18 |
| WebView2 | — | Windows 11 自带 / Win10 手动装 |

### 服务端

```bash
# 安装
cd server
pip install -e .

# 启动（WS 9765 + ACP 8765 + Unix socket）
kaya-transfer-hub serve --ws-port 9765
# 或直接用入口脚本
python3 run_and_send.py 9765 8765
```

### MCP 配置（opencode.json）

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

### Windows 客户端

```bash
cd client
npm install
npm run tauri build
# 或在 Windows 上直接运行 build.bat（自动装依赖）
```

---

## 功能

| 功能 | 说明 |
|------|------|
| 📁 文件传输 | LLM ↔ Windows 桌面双向推送 + SHA256 校验 |
| 💬 ACP 聊天 | 通过悬浮窗口与 Kaya 实时对话（流式输出） |
| 🖼️ 悬浮角色 | 透明置顶角色立绘 + 消息气泡（1.5s 去抖 / 30s 超时清理） |
| 🖥️ 远程工具 | 截屏、OCR、UIA 无障碍树、文件读写、剪贴板、输入控制 |
| 🔍 Copilot 查询 | 单次查询 / 持续监测循环，UIA + OCR + Vision 联动 |
| 🩺 健康检查 | 服务端 /proc 诊断 + 客户端非阻塞僵尸检测 + 自动恢复 |
| 🔄 会话保持 | Python 原生 ACP 桥，断连不杀进程，重连自动恢复 |
| ⚡ 快捷键 | `Ctrl+Alt+K` 快速聊天 / `Ctrl+Alt+S` 单次查询 / `Ctrl+Alt+C` 持续监测 |
| 🔔 信号系统 | 粘性/一次性信号 + 优先级队列 + 最小通知间隔控制 |
| 🛡️ 路径安全 | 可配置允许读写路径白名单 + 拒绝扩展名黑名单 + 工具权限开关 |

---

## 远程工具列表（21 个）

| 工具 | 说明 |
|------|------|
| `take_screenshot` | 截取屏幕指定区域 |
| `ocr_region` | Windows OCR 识别图片文字 |
| `uia_tree` | 获取 UIA 无障碍结构树 |
| `get_clipboard` / `set_clipboard` | 读写系统剪贴板 |
| `read_file` / `write_file` | 安全路径下的文件读写 |
| `list_directory` / `file_info` | 目录列表 / 文件元信息 |
| `grep_file` | 文件内容搜索 |
| `pull_file` | 拉取文件到服务端 |
| `run_command` | 执行命令并获取输出 |
| `type_text` / `key_press` / `mouse_click` | 输入模拟 |
| `list_windows` / `get_foreground_window` | 窗口管理 |
| `start_process` / `kill_process` | 进程管理 |
| `system_info` | 系统信息 |
| `open_path` | 在资源管理器中打开路径 |

---

## 项目结构

```
kaya-transfer-hub/
├── server/                          # Python 服务端
│   ├── run_and_send.py              # 服务端入口（WS + ACP + Unix socket）
│   ├── pyproject.toml
│   ├── src/
│   │   ├── kaya_transfer_hub/       # MCP 工具面
│   │   │   ├── server.py            # MCP 工具注册 + 生命周期
│   │   │   ├── mcp_agent.py         # Unix socket RPC 代理
│   │   │   ├── tool_defs.py         # 8 个共享工具定义
│   │   │   ├── tool_registry.py     # 客户端工具注册表 + 信号处理器
│   │   │   ├── cli.py               # Click CLI
│   │   │   └── __main__.py          # 入口
│   │   └── kaya_server/             # 运行时服务
│   │       ├── ws_handler.py        # WebSocket 服务 (9765)
│   │       ├── acp_bridge.py        # Python ACP 桥接 (8765)
│   │       ├── signal_registry.py   # 信号注册 + 优先级调度
│   │       ├── signal_handlers.py   # copilot / 健康检查 处理器
│   │       ├── connection_manager.py# 在线客户端连接池
│   │       ├── auth.py              # bcrypt 认证
│   │       ├── db.py                # SQLite 客户端注册表
│   │       ├── models.py            # 数据模型
│   │       └── constants.py         # 常量
│   └── tests/                       # 9 个测试文件
│
├── client/                          # Tauri 2 Windows 客户端
│   ├── src/                         # Vue 3 前端
│   │   ├── views/                   # 10 个页面
│   │   │   ├── FloatPage.vue        # 透明悬浮角色
│   │   │   ├── ChatPage.vue         # ACP 聊天
│   │   │   ├── QuickChat.vue        # 快速聊天
│   │   │   ├── BubblePage.vue       # 消息气泡
│   │   │   ├── CopilotOverlayWindow.vue  # Copilot 浮层
│   │   │   ├── HomePage.vue         # 首页
│   │   │   ├── SettingsPage.vue     # 设置
│   │   │   ├── FileTransferPage.vue # 文件传输
│   │   │   ├── CapabilitiesPage.vue # 工具管理
│   │   │   └── ToolCallOverlay.vue  # 工具调用状态
│   │   ├── components/              # 6 个组件
│   │   ├── stores/                  # Pinia 状态 (app/chat/file)
│   │   └── lib/                     # Tauri IPC / 类型
│   │
│   └── src-tauri/                   # Rust 后端
│       ├── src/
│       │   ├── lib.rs               # Tauri 入口 + AppState + 气泡循环
│       │   ├── main.rs              # 程序入口
│       │   ├── ws_client.rs         # WebSocket 客户端
│       │   ├── ws_runtime.rs        # WS 运行时启动
│       │   ├── acp_client.rs        # ACP 客户端 (JSON-RPC)
│       │   ├── acp_runtime.rs       # ACP 运行时启动
│       │   ├── protocol.rs          # 服务端消息解析 (含测试)
│       │   ├── tools/               # 21 个远程工具
│       │   │   ├── mod.rs
│       │   │   ├── screenshot.rs    # 截屏
│       │   │   ├── ocr.rs           # OCR
│       │   │   ├── uia_tree.rs      # UIA 无障碍
│       │   │   ├── input.rs         # 输入模拟
│       │   │   ├── clipboard.rs     # 剪贴板
│       │   │   ├── file_search.rs   # 文件搜索
│       │   │   ├── read_file.rs     # 读文件
│       │   │   ├── write_file.rs    # 写文件
│       │   │   ├── ...              # 其余工具
│       │   │   └── path_guard.rs    # 路径安全检查
│       │   ├── bubble.rs            # 气泡布局管理
│       │   ├── overlay.rs           # 浮层管理
│       │   ├── copilot.rs           # Copilot 查询引擎
│       │   ├── signal_emitter.rs    # 自动信号发射
│       │   ├── tool_executor.rs     # 工具执行分发
│       │   ├── file_handler.rs      # 文件接收
│       │   ├── config.rs            # 配置管理
│       │   ├── tray.rs              # 系统托盘
│       │   ├── notify.rs            # 系统通知
│       │   └── uia_tree.rs          # UIA 树辅助
│       └── tauri.conf.json
│
├── build.bat                        # Windows 一键构建脚本
├── docs/
│   ├── protocol.md                  # WebSocket 通信协议
│   └── superpowers/                 # 设计文档 / 计划
├── README.md
└── README.en.md
```

---

## CLI

```bash
# 启动服务
kaya-transfer-hub serve --ws-port 9765
kaya-transfer-hub mcp              # 仅 MCP stdio（委托 Unix socket）

# 客户端管理
kaya-transfer-hub register-client --id <id> --desc "<描述>" --passkey "<密钥>"
kaya-transfer-hub list-clients
kaya-transfer-hub remove-client --id <id>
```

---

## 通信协议

详见 [docs/protocol.md](docs/protocol.md)。

- **WebSocket (9765)**: 认证 / 心跳 / 文件三帧传输 / 工具调用 / 信号
- **ACP (8765)**: JSON-RPC over WebSocket → opencode acp 子进程
- **Unix socket (RPC)**: MCP 子命令通过 `/tmp/ft-hub-cmd.sock` 委托操作

---

## License

MIT
