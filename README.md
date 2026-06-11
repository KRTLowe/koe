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
┌─ Kaya (LLM via OpenCode) ─┐
│  MCP stdio                 │
└─────────┬──────────────────┘
          │
┌─────────┴──────────────────────────────┐
│  kaya-transfer-hub (Python 服务端)      │
│  ├─ MCP 工具注册 + 客户端工具代理       │
│  ├─ WebSocket 服务 (9765)               │
│  ├─ ACP 桥接 (8765) ← Python 原生实现   │
│  ├─ 信号调度 + 健康检查                 │
│  └─ 会话持久化（断连不丢失）            │
└──────┬───────────┬──────────────────────┘
       │ WS 9765   │ ACP 8765
       ▼           ▼
┌─────────────────────────────────────────┐
│  Tauri 2 Windows 客户端                 │
│  ├─ 悬浮角色 + 消息气泡                 │
│  ├─ 远程工具（截屏/OCR/UIA/输入）       │
│  ├─ 僵尸检测 + 非阻塞健康检查           │
│  └─ Copilot 查询浮层                    │
└─────────────────────────────────────────┘
```

---

## 快速开始

### 服务端

```bash
mkdir -p /opt/kaya_transfer_hub
cp -r server/src/kaya_transfer_hub /opt/kaya_transfer_hub/
cp -r server/src/kaya_server /opt/kaya_transfer_hub/
cp server/pyproject.toml /opt/kaya_transfer_hub/
cp server/run_and_send.py /opt/kaya_transfer_hub/kaya_server/

cd /opt/kaya_transfer_hub
sed -i 's/where = \["src"\]/where = ["."]/' pyproject.toml
pip install -e .

python3 kaya_server/run_and_send.py 9765 8765
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
```

---

## 功能

| 功能 | 说明 |
|------|------|
| 📁 文件传输 | LLM ↔ Windows 桌面双向推送 |
| 💬 ACP 聊天 | 悬浮聊天窗口与 Kaya 实时对话 |
| 🖼️ 悬浮角色 | 透明置顶角色立绘 + 消息气泡 |
| 🖥️ 远程工具 | 截屏、OCR、UIA 无障碍树、输入控制 |
| 🩺 健康检查 | 服务端 /proc 诊断 + 客户端非阻塞僵尸检测 |
| 🔄 会话保持 | Python 原生 ACP 桥，断连不杀进程，重连自动恢复 |

---

## 项目结构

```
kaya-transfer-hub/
├── server/
│   ├── run_and_send.py              # 服务端入口
│   ├── pyproject.toml
│   ├── src/
│   │   ├── kaya_transfer_hub/       # MCP 工具
│   │   │   ├── server.py            # 工具注册
│   │   │   ├── tool_registry.py     # 客户端工具代理
│   │   │   └── cli.py               # kaya-transfer-hub CLI
│   │   └── kaya_server/             # 运行时服务
│   │       ├── ws_handler.py        # WebSocket 服务
│   │       ├── signal_registry.py   # 信号调度
│   │       ├── signal_handlers.py   # copilot / 健康检查
│   │       ├── acp_bridge.py        # Python ACP 桥接
│   │       └── connection_manager.py
│   └── tests/
├── client/
│   ├── src/                         # Vue 3 前端
│   └── src-tauri/                   # Rust 后端
│       └── src/
│           ├── lib.rs               # 气泡循环 + 事件处理
│           ├── acp_client.rs        # ACP 客户端
│           ├── ws_client.rs         # WebSocket 客户端
│           ├── uia_tree.rs          # UIA 无障碍树
│           └── tools/               # 远程工具
├── docs/
├── build.bat
└── README.md
```

## CLI

```bash
kaya-transfer-hub register-client --id <id> --desc "<描述>" --passkey "<密钥>"
kaya-transfer-hub list-clients
kaya-transfer-hub remove-client --id <id>
kaya-transfer-hub serve --ws-port <port>
```

## License

MIT
