# File Transfer Hub — 设计规格

## 概述

自建文件传输通道，通过 MCP 工具让大模型（Kaya）查询客户端在线状态并推送文件。服务端独立部署，Windows 端使用 Tauri 桌面客户端接收文件。

## 技术栈

| 组件 | 技术 |
|------|------|
| 服务端 | Python 3 + `mcp` SDK + `websockets` + `aiohttp` + SQLite |
| Windows 客户端 | Tauri（Rust + Svelte/HTML 前端） |
| 通信 | WebSocket（二进制 frame 传文件） |
| 密钥存储 | bcrypt 哈希 |

## 架构

```
                     MCP stdio
  Kaya (OpenCode) ◄──────────────►┐
                                   │
                          ┌────────▼────────┐
                          │  Python 服务端    │
                          │  (MCP + WS)     │── SQLite (客户端注册表)
                          └────────┬────────┘
                                   │ WebSocket tcp:9765
                                   │ (passkey 认证)
                          ┌────────▼────────┐
                          │  Windows 客户端   │
                          │  (Tauri)         │
                          │  - 托盘后台       │
                          │  - 弹窗通知       │
                          │  - 文件→%TEMP%   │
                          └─────────────────┘
```

## 项目结构

```
file-transfer-hub/
├── server/
│   ├── pyproject.toml
│   └── src/file_transfer_hub/
│       ├── __init__.py
│       ├── main.py              # 入口：MCP + WebSocket
│       ├── server.py            # MCP 工具注册
│       ├── ws_handler.py        # WebSocket 客户端管理
│       ├── connection_manager.py # 在线客户端连接池
│       ├── db.py                # SQLite 操作
│       ├── auth.py              # passkey 校验
│       └── cli.py               # register-client 命令行
├── client/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── main.rs          # Tauri 入口
│   │   │   ├── ws_client.rs     # WebSocket 连接
│   │   │   ├── file_handler.rs  # 文件接收 & 写入
│   │   │   ├── tray.rs          # 系统托盘
│   │   │   ├── notify.rs        # 弹窗通知
│   │   │   └── config.rs        # 本地配置
│   │   ├── Cargo.toml
│   │   └── tauri.conf.json
│   ├── src/                     # 前端 UI
│   │   ├── App.svelte
│   │   └── main.ts
│   ├── package.json
│   └── vite.config.ts
└── docs/
    └── protocol.md
```

## 组件详述

### 1. 服务端（Python）

**连接管理器（ConnectionManager）：**
MCP 和 WebSocket 之间的桥梁。维护 `client_id → WebSocket` 映射。
MCP 工具的 `send_file` 通过它找到目标客户端连接并推送数据。

**MCP 工具：**

| 工具 | 描述 | 参数 |
|------|------|------|
| `list_clients` | 列出所有注册客户端及在线状态 | 无 |
| `send_file` | 发送文件到指定客户端 | `client_id`, `file_path` |
| `register_client` | 预注册客户端 | `client_id`, `description`, `passkey` |

**文件发送流程：**
1. `send_file` 被调用 → 读取本地文件
2. 查 ConnectionManager 获取目标 WebSocket
3. 发送文本帧 `{"type":"file_meta","file_id":"...","name":"...","size":N}`
4. 发送二进制帧（原始文件字节）
5. 发送文本帧 `{"type":"file_end","file_id":"...","checksum":"sha256:..."}`
6. 等待 `file_ack` → 返回结果给 Kaya

**启动方式：**
- `file-transfer-hub serve`（同时启动 MCP stdio + WS tcp:9765）
- `file-transfer-hub register-client --id pc-01 --desc "xxx" --passkey "xxx"`
- `file-transfer-hub list-clients`

### 2. Windows 客户端（Tauri）

**Rust 后端模块：**

| 模块 | 职责 |
|------|------|
| `ws_client.rs` | WebSocket 连接、心跳 30s、自动重连、消息分发 |
| `file_handler.rs` | 二进制帧组装、checksum 校验、写入 `%TEMP%/file-transfer-hub/` |
| `tray.rs` | 系统托盘（在线状态指示、最近文件、退出） |
| `notify.rs` | 收到文件时弹出 Tauri 小窗口，显示文件名/大小/时间 |
| `config.rs` | 本地 JSON 配置（server_url, client_id, passkey） |

**交互行为：**
- 启动 → 最小化到系统托盘
- 收到文件 → 弹出窗口展示文件详情
- 关闭窗口 → 回到托盘后台
- 双击托盘图标 → 打开主窗口

**主窗口布局：**
- 状态栏：连接状态
- 最近传输列表（文件名、大小、时间）
- 已注册客户端列表（扩展预留：多客户端转发）
- 设置入口

**托盘菜单：**
```
📁 File Transfer Hub
──────────────
  显示窗口
  最近文件
──────────────
  ● pc-01 在线
  ○ svr-01 离线
──────────────
  退出
```

### 3. 通信协议

**WebSocket 消息格式：**

```
客户端 → 服务端（文本帧）：

// 认证
{ "type": "auth", "client_id": "pc-01", "passkey": "xxx" }

// 心跳（每 30s）
{ "type": "heartbeat" }

// 文件确认
{ "type": "file_ack", "file_id": "f_001", "status": "ok" }


服务端 → 客户端（文本帧 + 二进制帧）：

// 认证结果
{ "type": "auth_result", "ok": true, "error": "" }

// 文件推送（三帧组合）
[文本帧]   { "type": "file_meta", "file_id":"f_001", "name":"screenshot.png", "size":1048576 }
[二进制帧] <原始文件字节流>
[文本帧]   { "type": "file_end", "file_id":"f_001", "checksum":"sha256:abc..." }

// 心跳响应
{ "type": "pong" }
```

### 4. 注册流程（预注册模式）

1. 服务端 CLI 预注册：`register-client --id pc-01 --desc "Kricto's PC" --passkey "xxx"`
   - passkey 存 bcrypt 哈希
   - 入库：`clients(id, description, passkey_hash, created_at)`
2. Windows 客户端首次启动 → 配置界面
   - 用户填入 `server_url`、`client_id`、`passkey`
   - 保存到本地 `config.json`
3. 客户端连接 WebSocket → 发送 auth
   - 服务端查 SQLite → 常量时间比较 passkey
   - 验证通过 → 加入 ConnectionManager 连接池
4. 后续自动重连，无需再次配置

**在线状态判定：**
- WebSocket 连接建立 → 在线
- 心跳超时 90s 未收到 → 离线
- WebSocket 断开 → 离线

## 边界与约束

- 文件传输不经服务端持久化，流式转发
- passkey 不存明文，bcrypt 哈希
- MCP 和 WebSocket 共享一个 asyncio 事件循环
- 单进程部署，一个 systemd 服务
- 大文件支持：二进制 WebSocket frame，不 base64

## 安全

- passkey 服务端存 bcrypt 哈希
- 认证使用常量时间字符串比较
- WebSocket 连接未认证时不接受其他消息
- 预注册模式防止未授权客户端接入

## 扩展预留

- 客户端窗口预留「已注册客户端列表」面板 → 未来支持多客户端转发
- 预留文件历史记录面板
- 连接池数据结构支持 `client_id → ws` 映射，方便扩展为多客户端互转
