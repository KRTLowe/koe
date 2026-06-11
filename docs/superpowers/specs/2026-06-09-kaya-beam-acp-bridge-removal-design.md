# kaya-beam ACPBridge 移除与信号注入改造设计

## 概述

移除服务端 `ACPBridge` 及它维护的独立 WS 连接（→ `opencode-bridge:8765`），改为通过客户端现有的 file-transfer-hub WS 连接回传注入消息，由客户端用自己的 ACP 聊天连接注入。

## 背景

现有 ACPBridge 存在两个问题：

1. **进程泄漏** — ACPBridge 持有一条独立的 WS 连接到 `opencode-bridge`，导致 `stdio-to-ws` 为它 spawn 一个独立的 `opencode acp` 进程。该进程在 ACPBridge 生命周期内始终存活，即使没有客户端在聊天。

2. **注入实际上无效** — ACPBridge 的连接和客户端的 ACP 连接分别对应不同的 `opencode acp` 进程（`stdio-to-ws` 为每个 WS 连接 spawn 独立进程）。ACPBridge 拿客户端的 `sessionId` 去 `inject_message`，但该 session 在客户端对应的进程中，ACPBridge 的进程不认识它，消息静默丢失。

## 架构变化

### 改造前

```
signal → WS:9765 → 服务端 ws_handler
  → ACPBridge WS:8765（独立连接）
    → opencode acp 进程 #B（不认识客户端的 sessionId）
    → 消息丢失 ✗
```

### 改造后

```
signal → WS:9765 → 服务端 ws_handler
  → 回发 {type: "acp_inject"} → WS:9765
    → 客户端 ws_client → event_tx → lib.rs
      → acp_tx → acp_client ACP WS:8765（客户端的连接）
        → opencode acp 进程 #A（认识 sessionId）
        → Kaya 收到 ✓
```

## 组件变动

### 删除

| 文件 | 说明 |
|------|------|
| `server/src/file_transfer_hub/acp_bridge.py` | 整文件删除 |
| `server/src/file_transfer_hub/constants.py` | 删 `ACP_BRIDGE_URL`（如只剩 `SOCKET_PATH` 可保留） |

### 服务端修改

`server/src/file_transfer_hub/ws_handler.py`：
- `__init__` 删 `acp_bridge` 参数
- 删 `self.acp_bridge` 字段
- `signal` 分支：`acp_bridge.inject_message()` → `websocket.send({"type":"acp_inject","text": msg})`
- `finally` 块：删 `acp_bridge.cancel_session()` 调用
- 删 `from file_transfer_hub.acp_bridge import ACPBridge`

`server/run_and_send.py`：
- 删 `from file_transfer_hub.acp_bridge import ACPBridge`
- 删 `acp_bridge = ACPBridge()`
- 删 `await acp_bridge.start()`
- 删 `WebSocketHandler(... acp_bridge=acp_bridge)` 参数
- 删 `await acp_bridge.stop()`

`server/src/file_transfer_hub/tool_registry.py`：
- `_sessions` / `set_session` / `get_session` / `clear_client` 中的 session 清理 → 不再需要，可删
- `session_update` 消息类型 → 不再需要，可删

### 客户端修改

`client/src-tauri/src/ws_client.rs`：
- `WsEvent` 枚举新增 `AcpInject { text: String }` 变体
- WS 消息循环新增 `Some("acp_inject")` 分支

`client/src-tauri/src/lib.rs`：
- 事件循环新增 `WsEvent::AcpInject { text }` → 通过 `acp_tx` 发送到 ACP 客户端

### 不改的

- `acp_client.rs` — 注入消息通过现有的 msg_rx 通道发送，走现有的 `session/prompt` 逻辑
- `ChatMessage.vue` / `chat.ts` — 无需变化，注入后的响应流走现有流程
- `signal_emitter.rs` — 信号发射逻辑不变
- `tool_executor.rs` — 工具执行逻辑不变

## 数据流详述

### 正常注入流程

```
1. Kaya 调用 call_client_tool("take_screenshot")
   → MCP → run_and_send 发 call_tool → WS:9765 → 客户端

2. 客户端执行截图
   → tool_result + signal(visual_input_available) → WS:9765 → 服务端

3. 服务端 ws_handler.signal 分支
   → handle_signal() 生成 "[系统] 客户端 win-kricto 报告新的视觉输入可用..."
   → 回发 {"type":"acp_inject","text":"[系统]..."} → WS:9765 → 客户端

4. 客户端 ws_client 收到 acp_inject
   → event_tx.send(WsEvent::AcpInject { text })

5. lib.rs 事件循环
   → app.state 取 acp_tx
   → acp_tx.send(text)

6. acp_client 收到消息（与用户发消息走同一通道）
   → session/prompt → ACP WS:8765 → opencode-bridge
   → Kaya 收到系统消息

7. Kaya 响应
   → 流式 session/update → WS:8765 → acp_client → MessageResponse → 前端气泡
```

### 断开清理

`--persist` 已移除，客户端 ACP WS 断开时 `stdio-to-ws` 自动 `child.kill()`，无需服务端 `session/cancel`。

## 错误处理

- **客户端离线时收到 signal**：服务端 `websocket.send(acp_inject)` 抛出 `ConnectionClosed`，走现有断开处理逻辑，忽略即可
- **acp_tx 通道满**：`try_send` 失败，日志记录即可（注入消息非关键路径）
- **acp_client 未就绪（无 session）**：`session/prompt` 不会发送，acp_client 已有 `会话未就绪` 处理

## 测试要点

1. 服务端启动后不再有 ACPBridge 连接 → `acp_processes` 不因服务端增加
2. 客户端截图后，Kaya 收到系统通知并回复（功能验证）
3. 客户端断开后，对应的 `opencode acp` 进程自动终止（`--persist` 移除保障）
