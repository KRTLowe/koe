# kaya-beam 远程 MCP 框架设计

## 概述

扩展 kaya-beam 的 WebSocket 协议和 MCP 接口，实现三个递进能力：

1. **远程工具注册与调用** — Windows 客户端将本地能力（截屏、剪贴板、文件搜索）注册到服务端，Kaya 通过 MCP 工具远程调用
2. **ACP 生命周期管理** — ACP 连接在程序启动时建立，服务端追踪 session ID，前端感知就绪状态
3. **信号系统** — 客户端触发预定义事件，服务端自动通过 ACP 注入上下文消息

## 架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                          SERVER (run_and_send.py)                    │
│                                                                     │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────────────┐  │
│  │ MCPServer       │  │ ToolRegistry   │  │ ACPBridge            │  │
│  │ (server.py)     │  │ (NEW)          │  │ (NEW)                │  │
│  │                 │  │                │  │                      │  │
│  │ list_clients    │  │ _tools         │  │ WS→:8765 persistent  │  │
│  │ send_file       │  │ _pending       │  │ init handshake       │  │
│  │ list_client_tools│ │ _sessions      │  │ inject_message()     │  │
│  │ call_client_tool│  │ _signals       │  │ auto-reconnect       │  │
│  └────────┬────────┘  └───────┬────────┘  └──────────┬───────────┘  │
│           │                   │                      │              │
│           └────────┬──────────┴──────────────────────┘              │
│                    │                                                │
│             ┌──────▼──────────┐                                     │
│             │ WS Handler      │◄── WS :9765 ────┐                  │
│             │ + tool msgs     │                  │                  │
│             │ + signal handle │                  │                  │
│             └─────────────────┘                  │                  │
└──────────────────────────────────────────────────┼──────────────────┘
                                                    │
                         ┌──────────────────────────┴──────────────┐
                         │  Tauri Windows Client                   │
                         │                                         │
                         │  WsClient + tool reg/call/signal        │
                         │  ToolExecutor (take_screenshot, etc.)   │
                         │  SignalEmitter (auto-fire on events)    │
                         │  AcpClient + session_id tracking        │
                         │                                         │
                         │  WS :8765 ──► opencode-bridge           │
                         └─────────────────────────────────────────┘
```

## 1. WebSocket 协议扩展

### 新增消息类型

所有新增类型与现有 `auth`、`heartbeat`、`file_meta` 等共存，通过 `type` 字段区分。

#### `register_tools`

客户端认证成功后立即发送，声明本地可用工具。

```
Client → Server:
{
  "type": "register_tools",
  "tools": [
    {
      "name": "take_screenshot",
      "description": "Capture the Windows desktop screen",
      "inputSchema": {
        "type": "object",
        "properties": {
          "region": {
            "type": "string",
            "enum": ["full", "active_window", "monitor_1"]
          }
        }
      }
    }
  ]
}

Server → Client:
{"type": "register_tools_result", "ok": true, "registered": 3}
```

#### `call_tool` / `tool_result`

服务端发起工具调用请求，客户端异步返回结果。

```
Server → Client:
{
  "type": "call_tool",
  "request_id": "req_a1b2c3",
  "name": "take_screenshot",
  "arguments": {"region": "full"}
}

Client → Server (success):
{
  "type": "tool_result",
  "request_id": "req_a1b2c3",
  "content": [{"type": "text", "text": "Screenshot saved to C:\\..."}]
}

Client → Server (error):
{
  "type": "tool_result",
  "request_id": "req_a1b2c3",
  "content": [],
  "is_error": true
}
```

#### `signal` / `signal_ack`

客户端触发预定义事件，服务端确认收到。

```
Client → Server:
{
  "type": "signal",
  "name": "visual_input_available",
  "data": {"source": "screenshot", "timestamp": "2026-06-08T14:30:00Z"}
}

Server → Client:
{"type": "signal_ack", "name": "visual_input_available", "ok": true}
```

#### `session_update`

客户端告知服务端当前 ACP 会话 ID。

```
Client → Server:
{"type": "session_update", "session_id": "ses_abc123def"}
```

## 2. 服务端新增模块

### 2.1 ToolRegistry

路径：`server/src/file_transfer_hub/tool_registry.py`

```python
class ToolRegistry:
    """
    管理客户端注册的工具和待处理的调用请求。
    """

    # client_id → list[ToolDef]
    _tools: dict[str, list[dict]]
    # request_id → asyncio.Future[dict]   (pending invocations)
    _pending: dict[str, asyncio.Future]
    # client_id → str                     (ACP session IDs)
    _sessions: dict[str, str]
    # signal_name → handler function
    _signal_handlers: dict[str, callable]

    def register_tools(client_id: str, tools: list[dict]) -> int
        """注册/覆盖客户端工具列表。返回注册数量。"""

    def get_tools(client_id: str) -> list[dict]
        """获取某客户端的工具列表。"""

    def create_invoke(client_id: str, name: str, args: dict, timeout: float = 30.0) -> asyncio.Future
        """创建一个工具调用，返回 Future。超时自动异常。"""

    def resolve_invoke(request_id: str, result: dict) -> None
        """客户端返回结果时，resolve 对应的 Future。"""

    def set_session(client_id: str, session_id: str) -> None
        """记录客户端的 ACP session ID。"""

    def handle_signal(client_id: str, name: str, data: dict) -> str | None
        """处理客户端信号。返回构造的 ACP 消息文本（如有）。"""
```

### 2.2 ACPBridge

路径：`server/src/file_transfer_hub/acp_bridge.py`

```python
class ACPBridge:
    """
    服务端侧持久 WebSocket 连接到 opencode-bridge (:8765)，
    用于信号触发时自动注入 ACP 系统消息。
    """

    async def start()
        """连接 ws://127.0.0.1:8765，走 ACP initialize 握手。自动重连。"""

    async def stop()
        """关闭连接。"""

    async def inject_message(session_id: str, text: str) -> bool
        """向 ACP session 注入系统消息。返回是否发送成功。"""
```

ACPBridge 发送的 ACP 消息格式：

```json
{
  "jsonrpc": "2.0",
  "method": "session/prompt",
  "params": {
    "sessionId": "ses_abc",
    "prompt": [{"type": "text", "text": "用户系统消息..."}]
  }
}
```

### 2.3 MCPServer 扩展

`server.py` 新增 2 个 MCP 工具：

| 工具 | 参数 | 返回值 |
|------|------|--------|
| `list_client_tools` | `client_id: string` | Markdown 表格或 JSON |
| `call_client_tool` | `client_id, tool_name, arguments` | 执行结果文本 |

### 2.4 WebSocketHandler 扩展

`ws_handler.py` 新增消息处理分支：

- `register_tools` → `ToolRegistry.register_tools()`
- `tool_result` → `ToolRegistry.resolve_invoke()`
- `signal` → `ToolRegistry.handle_signal()` → 可选调用 `ACPBridge.inject_message()`
- `session_update` → `ToolRegistry.set_session()`

新增方法：

```python
async def send_tool_call(client_id: str, request_id: str, name: str, args: dict):
    """向客户端发送工具调用请求。"""
```

## 3. 客户端新增模块

### 3.1 ToolExecutor

路径：`client/src-tauri/src/tool_executor.rs`

```rust
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: serde_json::Value,
}

pub struct ToolResult {
    pub content: Vec<serde_json::Value>,
    pub is_error: bool,
}

pub fn get_local_tools() -> Vec<ToolDef>;
/// 根据工具名分派到具体实现
pub async fn execute_tool(name: &str, args: &serde_json::Value) -> ToolResult;
```

内置工具：

| 工具名 | 实现方式 |
|--------|----------|
| `take_screenshot` | Rust image crate + screenshots crate / Tauri API |
| `get_clipboard` | arboard crate |
| `file_search` | walkdir crate + glob 匹配 |

### 3.2 SignalEmitter

路径：`client/src-tauri/src/signal_emitter.rs`

```rust
pub enum Signal {
    VisualInputAvailable,
    ClipboardChanged,
}

/// 发送信号到服务端（通过 ws_client 的 channel）
pub async fn emit_signal(ws_tx: &mpsc::Sender<WsMessage>, signal: Signal);
```

SignalEmitter 自动触发时机：

- `take_screenshot` 执行成功后 → `VisualInputAvailable`
- 剪贴板变化（可选轮询） → `ClipboardChanged`

### 3.3 WsClient 扩展

`ws_client.rs` 增加：

- 认证成功后发送 `register_tools`
- 处理 `call_tool` 消息 → `ToolExecutor.execute_tool()` → 发送 `tool_result`
- 处理 `signal_ack` 消息（日志记录）
- 收到 `AcpEvent::SessionReady` 时发送 `session_update`

### 3.4 lib.rs 扩展

`AppState` 新增：

```rust
struct AppState {
    // ... existing fields ...
    session_id: Mutex<Option<String>>,
}
```

事件处理：`AcpEvent::SessionReady { session_id }` → 存入 `AppState.session_id` + 通过 `WsClient` 发送 `session_update`。

### 3.5 前端 chat.ts 扩展

```typescript
export const useChatStore = defineStore("chat", () => {
    // ... existing ...
    const sessionId = ref<string | null>(null);
    const acpReady = computed(() => connected.value && sessionReady.value);
    
    // listen("acp-session") → set sessionId
});
```

## 4. 信号 → ACP 注入流程

```
1. 客户端 ToolExecutor.take_screenshot() 完成
2. SignalEmitter.emit("visual_input_available") → WS: {"type":"signal",...}
3. 服务端 ToolRegistry.handle_signal("pc-01", "visual_input_available", data)
   → 查 _sessions["pc-01"] → "ses_abc"
   → 查信号处理器 → 构造消息文本
4. ACPBridge.inject_message("ses_abc", "A screenshot was taken...")
   → WS→:8765 session/prompt
5. opencode acp 处理 → Kaya 模型收到消息
6. Kaya 调用 call_client_tool("pc-01", "take_screenshot", {...})
7. 服务端转发 WS call_tool → 客户端执行 → 返回结果
```

## 5. 错误处理

| 场景 | 处理方式 |
|------|----------|
| 工具调用时客户端离线 | `is_online()` 检查返回 MCP 错误 |
| 工具执行超时（30s） | Future 超时异常，返回 MCP 错误 |
| 客户端返回 `is_error: true` | 透传错误信息给 MCP |
| ACPBridge 未连接时收到信号 | 记录 warn 日志，跳过注入 |
| 客户端重连后工具定义变化 | 重新注册，覆盖旧定义 |

## 6. 实施计划

### Phase 1 — 协议 + ToolRegistry（当前）

- [ ] 创建 `tool_registry.py`
- [ ] `ws_handler.py` 扩展：handle `register_tools`, `tool_result` + 新增 `send_tool_call()`
- [ ] `server.py` 扩展：新增 `list_client_tools`, `call_client_tool` MCP 工具
- [ ] `ws_client.rs` 扩展：发送 `register_tools`, 处理 `call_tool`, 返回 `tool_result`
- [ ] Rust: `ToolDef` + `ToolResult` 结构体
- [ ] 端到端测试：注册 → 调用 → 返回

### Phase 2 — 工具实现

- [ ] `take_screenshot` 实现
- [ ] `get_clipboard` 实现
- [ ] `file_search` 实现
- [ ] `tool_executor.rs` 调度器

### Phase 3 — 信号 + ACPBridge

- [ ] 创建 `acp_bridge.py`
- [ ] `ws_handler.py` 扩展：`signal` / `signal_ack`
- [ ] `signal_emitter.rs`
- [ ] `handle_signal()` → `inject_message()` 链路
- [ ] Rust: `session_update` 发送 + `AppState.session_id`

### Phase 4 — 错误处理 + 集成

- [ ] 超时、离线错误处理
- [ ] 重连清理
- [ ] 前端 `sessionId` / `acpReady`
