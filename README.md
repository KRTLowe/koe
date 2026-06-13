<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="client/public/kaya-float.png">
    <img src="client/public/kaya-float.png" width="160" alt="Kaya Logo">
  </picture>
  <h1 align="center">Kaya-On-Everywhere</h1>
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
</p>

Kaya-On-Everywhere 是 Kaya 的桌面扩展层——通过 OpenCode MCP 协议连接，为 LLM 提供文件传输、实时聊天和远程桌面工具能力。

## 快速开始

### 前置条件

| 组件 | 服务端 (Linux) | 客户端构建 (Windows) |
|------|---------------|---------------------|
| Python | ≥ 3.11 | — |
| Rust | — | ≥ 1.80 |
| Node.js | — | ≥ 18 |
| WebView2 | — | Windows 11 自带 |

### 服务端

```bash
cd server
pip install -e .

# 完整启动（推荐）
python3 run_and_send.py 9765 8765

# 仅 MCP stdio（功能受限）
kaya-transfer-hub serve --ws-port 9765
```

### 注册客户端

```bash
kaya-transfer-hub register-client --id pc-01 --desc "Windows PC" --passkey "<你的密钥>"
kaya-transfer-hub list-clients
```

### opencode.json 配置

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

### Windows 客户端构建

```bash
cd client
npm install
npm run tauri build
```

Windows 上一键构建：双击 `build.bat`（自动设置 VS 环境变量）。

## 功能

| 功能 | 说明 |
|------|------|
| 文件传输 | LLM ↔ Windows 双向推送 + SHA256 校验 |
| ACP 聊天 | 通过悬浮窗口与 Kaya 实时对话（流式输出） |
| 悬浮角色 | 透明置顶角色立绘 + 消息气泡 |
| 远程工具 | 截屏、OCR、UIA 树、文件读写、剪贴板、输入控制 |
| Copilot 查询 | 单次查询 / 持续监测循环，UIA + OCR + Vision 联动 |
| 会话保持 | Python 原生 ACP 桥，断连不杀进程，重连自动恢复 |
| 快捷键 | `Ctrl+Alt+K` 快速聊天 / `Ctrl+Alt+S` 单次查询 / `Ctrl+Alt+C` 持续监测 |
| 信号系统 | 粘性/一次性信号 + 优先级队列 + 最小通知间隔 |
| 路径安全 | 可配置允许读写路径白名单 + 拒绝扩展名 |

## 远程工具（21 个）

| 工具 | 说明 |
|------|------|
| `take_screenshot` | 截取屏幕指定区域 |
| `ocr_region` | Windows OCR 文字识别 |
| `uia_tree` | UIA 无障碍结构树 |
| `get_clipboard` / `set_clipboard` | 读写剪贴板 |
| `read_file` / `write_file` | 路径安全下的文件读写 |
| `list_directory` / `file_info` | 目录 / 文件元信息 |
| `grep_file` | 文件内容搜索 |
| `pull_file` | 拉取文件到服务端 |
| `run_command` | 执行命令并获取输出 |
| `type_text` / `key_press` / `mouse_click` | 输入模拟 |
| `list_windows` / `get_foreground_window` | 窗口管理 |
| `start_process` / `kill_process` | 进程管理 |
| `system_info` | 系统信息 |
| `open_path` | 打开路径 |

## CLI

```bash
kaya-transfer-hub serve --ws-port 9765
kaya-transfer-hub mcp                          # 委托 Unix socket
kaya-transfer-hub register-client --id <id> --desc "<描述>" --passkey "<密钥>"
kaya-transfer-hub list-clients
kaya-transfer-hub remove-client --id <id>
```

## 通信协议

详见 [docs/protocol.md](docs/protocol.md)。

## 项目结构

```
server/                          # Python 服务端
├── run_and_send.py              # 入口（WS + ACP + Unix socket）
├── pyproject.toml
├── src/
│   ├── kaya_transfer_hub/       # MCP 工具层（8 工具 + CLI）
│   └── kaya_server/             # 运行时（WS/ACP/信号/认证/DB）
└── tests/

client/                          # Tauri 2 Windows 客户端
├── src/                         # Vue 3 前端（10 页面）
└── src-tauri/src/               # Rust 后端（18 模块 + 21 工具）

docs/
├── protocol.md                  # WebSocket 协议文档
└── superpowers/                 # 设计文档
```

## License

MIT
