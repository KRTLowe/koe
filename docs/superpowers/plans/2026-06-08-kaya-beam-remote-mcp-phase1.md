# Phase 1: Remote MCP Framework — 协议 + ToolRegistry 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 实现远程 MCP 框架 Phase 1：客户端工具注册 + 服务端调用转发 + 端到端链路。

**架构：** 服务端新增 ToolRegistry 管理客户端工具定义和待处理调用，WebSocketHandler 扩展 4 个新消息类型，MCPServer 新增 2 个 MCP 工具。客户端 ws_client.rs 扩展工具注册和调用处理。

**技术栈：** Python 3 + asyncio + Rust + Tauri + MCP SDK

---

## 文件结构

### 创建文件

| 文件 | 职责 |
|------|------|
| `server/src/file_transfer_hub/tool_registry.py` | 工具注册表：存储/查询工具定义、管理待处理调用 Future、记录 session ID、信号处理 |
| `server/tests/test_tool_registry.py` | ToolRegistry 单元测试 |

### 修改文件

| 文件 | 职责 |
|------|------|
| `server/src/file_transfer_hub/ws_handler.py` | 处理 `register_tools`, `tool_result`, `session_update`；新增 `send_tool_call()` |
| `server/src/file_transfer_hub/server.py` | 新增 `list_client_tools`, `call_client_tool` 两个 MCP 工具 |
| `server/run_and_send.py` | 初始化 ToolRegistry，注入到 ws_handler |
| `client/src-tauri/src/ws_client.rs` | 认证后发送 `register_tools`，处理 `call_tool`，返回 `tool_result` |

---

### 任务 1：ToolRegistry 模块

**文件：**
- 创建：`server/src/file_transfer_hub/tool_registry.py`
- 创建：`server/tests/test_tool_registry.py`

- [ ] **步骤 1：创建 tool_registry.py**

```python
"""客户端工具注册表。管理工具定义、待处理调用、ACP session ID 和信号处理器。"""
import asyncio
import logging
import uuid
from typing import Callable

logger = logging.getLogger(__name__)

INVOKE_TIMEOUT = 30.0


class ToolRegistry:
    """管理客户端注册的工具和待处理的调用请求。"""

    def __init__(self):
        # client_id → list[dict]  工具定义列表
        self._tools: dict[str, list[dict]] = {}
        # request_id → asyncio.Future  待处理的工具调用
        self._pending: dict[str, asyncio.Future] = {}
        # client_id → str  ACP session ID
        self._sessions: dict[str, str] = {}
        # signal_name → handler(client_id, data) → str | None
        self._signal_handlers: dict[str, Callable] = {}

    def register_tools(self, client_id: str, tools: list[dict]) -> int:
        """注册/覆盖客户端的工具列表。返回注册数量。"""
        self._tools[client_id] = list(tools)
        logger.info(f"Registered {len(tools)} tools for client {client_id}")
        return len(tools)

    def get_tools(self, client_id: str) -> list[dict]:
        """获取某客户端的工具列表。"""
        return list(self._tools.get(client_id, []))

    def clear_client(self, client_id: str):
        """客户端断开时清理。"""
        self._tools.pop(client_id, None)
        self._sessions.pop(client_id, None)

    def has_tools(self, client_id: str) -> bool:
        return client_id in self._tools and bool(self._tools[client_id])

    # ── 工具调用 ──

    def create_invoke(self, client_id: str, tool_name: str, arguments: dict) -> str:
        """创建一个待处理的工具调用，返回 request_id。"""
        request_id = f"req_{uuid.uuid4().hex[:12]}"
        loop = asyncio.get_event_loop()
        future = loop.create_future()
        self._pending[request_id] = future
        return request_id

    def get_invoke_future(self, request_id: str) -> asyncio.Future | None:
        return self._pending.get(request_id)

    async def wait_invoke(self, request_id: str, timeout: float = INVOKE_TIMEOUT) -> dict:
        """等待工具调用完成，返回结果。超时引发 asyncio.TimeoutError。"""
        future = self._pending.get(request_id)
        if future is None:
            return {"ok": False, "error": f"Unknown request_id: {request_id}"}
        try:
            result = await asyncio.wait_for(future, timeout=timeout)
            return result
        except asyncio.TimeoutError:
            self._pending.pop(request_id, None)
            return {"ok": False, "error": f"Tool invocation timed out after {timeout}s"}
        finally:
            self._pending.pop(request_id, None)

    def resolve_invoke(self, request_id: str, result: dict) -> bool:
        """客户端返回结果时，resolve 对应的 Future。"""
        future = self._pending.get(request_id)
        if future is None or future.done():
            return False
        future.set_result(result)
        return True

    # ── ACP session ──

    def set_session(self, client_id: str, session_id: str):
        self._sessions[client_id] = session_id

    def get_session(self, client_id: str) -> str | None:
        return self._sessions.get(client_id)

    # ── 信号 ──

    def register_signal_handler(self, signal_name: str, handler: Callable):
        self._signal_handlers[signal_name] = handler

    def handle_signal(self, client_id: str, signal_name: str, data: dict) -> str | None:
        """处理客户端信号。返回构造的 ACP 消息文本，或 None。"""
        handler = self._signal_handlers.get(signal_name)
        if handler is None:
            logger.warning(f"No handler for signal '{signal_name}' from {client_id}")
            return None
        return handler(client_id, data)
```

- [ ] **步骤 2：创建测试文件**

```python
"""ToolRegistry 单元测试。"""
import asyncio
import pytest
from file_transfer_hub.tool_registry import ToolRegistry


@pytest.fixture
def registry():
    return ToolRegistry()


SAMPLE_TOOLS = [
    {
        "name": "take_screenshot",
        "description": "Capture the screen",
        "inputSchema": {"type": "object", "properties": {"region": {"type": "string"}}},
    },
    {
        "name": "get_clipboard",
        "description": "Read clipboard",
        "inputSchema": {"type": "object", "properties": {}},
    },
]


class TestToolRegistry:
    def test_register_tools(self, registry):
        count = registry.register_tools("pc-01", SAMPLE_TOOLS)
        assert count == 2
        assert registry.has_tools("pc-01")

    def test_get_tools(self, registry):
        registry.register_tools("pc-01", SAMPLE_TOOLS)
        tools = registry.get_tools("pc-01")
        assert len(tools) == 2
        assert tools[0]["name"] == "take_screenshot"

    def test_get_tools_empty(self, registry):
        assert registry.get_tools("nonexistent") == []

    def test_clear_client(self, registry):
        registry.register_tools("pc-01", SAMPLE_TOOLS)
        registry.clear_client("pc-01")
        assert not registry.has_tools("pc-01")

    @pytest.mark.asyncio
    async def test_invoke_flow(self, registry):
        registry.register_tools("pc-01", SAMPLE_TOOLS)
        request_id = registry.create_invoke("pc-01", "take_screenshot", {})
        assert request_id.startswith("req_")

        # Simulate client returning result
        result = {"ok": True, "content": [{"type": "text", "text": "done"}]}
        registry.resolve_invoke(request_id, result)

        # Wait for result
        got = await registry.wait_invoke(request_id)
        assert got["ok"] is True

    @pytest.mark.asyncio
    async def test_invoke_timeout(self, registry):
        registry.register_tools("pc-01", SAMPLE_TOOLS)
        request_id = registry.create_invoke("pc-01", "take_screenshot", {})
        got = await registry.wait_invoke(request_id, timeout=0.1)
        assert got["ok"] is False
        assert "timeout" in got["error"]

    def test_session(self, registry):
        registry.set_session("pc-01", "ses_abc")
        assert registry.get_session("pc-01") == "ses_abc"

    def test_signal_handler(self, registry):
        calls = []

        def handler(cid, data):
            calls.append((cid, data))
            return "system message"

        registry.register_signal_handler("test_signal", handler)
        result = registry.handle_signal("pc-01", "test_signal", {"key": "val"})
        assert result == "system message"
        assert len(calls) == 1
```

- [ ] **步骤 3：运行测试**

```bash
cd server && pip install -e . && python -m pytest tests/test_tool_registry.py -v
```

预期：6/6 passed

- [ ] **步骤 4：Commit**

```bash
git add server/src/file_transfer_hub/tool_registry.py server/tests/test_tool_registry.py
git commit -m "feat: add ToolRegistry for remote client tool management"
```

---

### 任务 2：WebSocketHandler 扩展

**文件：**
- 修改：`server/src/file_transfer_hub/ws_handler.py`

- [ ] **步骤 1：添加新消息处理**

在 `ws_handler.py` 的 `_handle_client()` 中，`msg_type` 分支新增：

```python
elif msg_type == "register_tools":
    tools = data.get("tools", [])
    if client_id:
        count = self.tool_registry.register_tools(client_id, tools)
        await websocket.send(json.dumps({
            "type": "register_tools_result",
            "ok": True,
            "registered": count,
        }))

elif msg_type == "tool_result":
    request_id = data.get("request_id")
    if request_id and client_id:
        self.tool_registry.resolve_invoke(request_id, data)
        logger.info(f"Tool result received: {request_id} from {client_id}")

elif msg_type == "session_update":
    session_id = data.get("session_id")
    if session_id and client_id:
        self.tool_registry.set_session(client_id, session_id)
        logger.info(f"Session updated for {client_id}: {session_id}")

elif msg_type == "signal":
    signal_name = data.get("name")
    signal_data = data.get("data", {})
    if signal_name and client_id:
        logger.info(f"Signal '{signal_name}' from {client_id}: {signal_data}")
        # 确认收到
        await websocket.send(json.dumps({
            "type": "signal_ack",
            "name": signal_name,
            "ok": True,
        }))
        # 处理信号（后续 Phase 3 接入 ACPBridge）
        msg = self.tool_registry.handle_signal(client_id, signal_name, signal_data)
        if msg:
            session_id = self.tool_registry.get_session(client_id)
            if session_id and self.acp_bridge:
                await self.acp_bridge.inject_message(session_id, msg)
```

- [ ] **步骤 2：添加 `send_tool_call()` 方法**

```python
async def send_tool_call(self, client_id: str, request_id: str, name: str, arguments: dict) -> bool:
    """向客户端发送工具调用请求。返回是否发送成功。"""
    ws = self.cm.get_connection(client_id)
    if ws is None:
        return False
    payload = json.dumps({
        "type": "call_tool",
        "request_id": request_id,
        "name": name,
        "arguments": arguments,
    })
    try:
        await ws.send(payload)
        return True
    except Exception as e:
        logger.error(f"Failed to send tool call to {client_id}: {e}")
        return False
```

- [ ] **步骤 3：修改 `__init__` 接收 ToolRegistry 和可选的 ACPBridge**

```python
def __init__(
    self,
    db: Database,
    connection_manager: ConnectionManager,
    host: str = "0.0.0.0",
    port: int = 9765,
    tool_registry: ToolRegistry | None = None,
    acp_bridge=None,  # Phase 3
):
    # ... existing ...
    self.tool_registry = tool_registry or ToolRegistry()
    self.acp_bridge = acp_bridge
```

在文件顶部添加 import：

```python
from file_transfer_hub.tool_registry import ToolRegistry
```

- [ ] **步骤 4：客户端断开时清理**

在 `_handle_client()` 的 `finally` 块中，添加：

```python
finally:
    if client_id:
        self.cm.unregister(client_id)
        if self.tool_registry:
            self.tool_registry.clear_client(client_id)
```

- [ ] **步骤 5：运行现有测试确认无破坏**

```bash
cd server && python -m pytest tests/ -v 2>&1 | tail -10
```

预期：全部通过

- [ ] **步骤 6：Commit**

```bash
git add server/src/file_transfer_hub/ws_handler.py
git commit -m "feat: extend ws_handler for tool registration, invocation, signals, session tracking"
```

---

### 任务 3：MCPServer 扩展

**文件：**
- 修改：`server/src/file_transfer_hub/server.py`

- [ ] **步骤 1：添加新 MCP 工具**

在 `_register_tools()` 方法的 `list_tools` 中添加两个新工具：

```python
types.Tool(
    name="list_client_tools",
    description="列出指定客户端注册的本地工具。返回工具名称和参数描述。",
    inputSchema={
        "type": "object",
        "properties": {
            "client_id": {
                "type": "string",
                "description": "客户端 ID（如 pc-01）",
            },
        },
        "required": ["client_id"],
    },
),
types.Tool(
    name="call_client_tool",
    description="调用客户端上的本地工具（如截屏、剪贴板等）。工具需提前注册。",
    inputSchema={
        "type": "object",
        "properties": {
            "client_id": {
                "type": "string",
                "description": "客户端 ID",
            },
            "tool_name": {
                "type": "string",
                "description": "工具名称",
            },
            "arguments": {
                "type": "object",
                "description": "工具参数（JSON 对象）",
            },
        },
        "required": ["client_id", "tool_name", "arguments"],
    },
),
```

- [ ] **步骤 2：实现 handler**

在 `call_tool()` 中新增分支：

```python
elif name == "list_client_tools":
    return await self._handle_list_client_tools(arguments)
elif name == "call_client_tool":
    return await self._handle_call_client_tool(arguments)
```

新增方法：

```python
def _format_tools_table(self, client_id: str) -> str:
    tools = self.ws_handler.tool_registry.get_tools(client_id)
    if not tools:
        return f"客户端 `{client_id}` 暂无已注册工具。"
    lines = ["| 工具名 | 描述 | 参数 |", "|---|---|---|"]
    for t in tools:
        params = ", ".join(t.get("inputSchema", {}).get("properties", {}).keys())
        lines.append(f"| `{t['name']}` | {t.get('description', '')} | {params or '-'} |")
    return "\n".join(lines)

async def _handle_list_client_tools(self, args: dict) -> list[TextContent]:
    client_id = args["client_id"]
    return [TextContent(type="text", text=self._format_tools_table(client_id))]

async def _handle_call_client_tool(self, args: dict) -> list[TextContent]:
    client_id = args["client_id"]
    tool_name = args["tool_name"]
    arguments = args.get("arguments", {})

    if not self.cm.is_online(client_id):
        return [TextContent(
            type="text",
            text=f"❌ 客户端 `{client_id}` 当前离线，无法调用工具。",
        )]

    if not self.ws_handler.tool_registry.has_tools(client_id):
        return [TextContent(
            type="text",
            text=f"❌ 客户端 `{client_id}` 未注册任何工具。",
        )]

    request_id = self.ws_handler.tool_registry.create_invoke(client_id, tool_name, arguments)
    sent = await self.ws_handler.send_tool_call(client_id, request_id, tool_name, arguments)

    if not sent:
        return [TextContent(
            type="text",
            text=f"❌ 向客户端 `{client_id}` 发送工具调用失败。",
        )]

    # 等待结果（带超时）
    result = await self.ws_handler.tool_registry.wait_invoke(request_id)

    if result.get("ok") is False:
        return [TextContent(
            type="text",
            text=f"❌ 工具调用失败：{result.get('error', '未知错误')}",
        )]

    content = result.get("content", [])
    text_parts = [c.get("text", "") for c in content if c.get("type") == "text"]
    return [TextContent(type="text", text="\n".join(text_parts) or "✅ 工具执行完成（无文本输出）")]
```

- [ ] **步骤 3：Commit**

```bash
git add server/src/file_transfer_hub/server.py
git commit -m "feat: add list_client_tools and call_client_tool MCP tools"
```

---

### 任务 4：run_and_send.py 接入 ToolRegistry

**文件：**
- 修改：`server/run_and_send.py`

- [ ] **步骤 1：初始化 ToolRegistry 并注入**

在 `main()` 中，创建 WebSocketHandler 时传入 tool_registry：

```python
from file_transfer_hub.tool_registry import ToolRegistry

# 创建共享的 ToolRegistry（后续 Phase 3 注入 ACPBridge）
tool_registry = ToolRegistry()

db = Database(None)
db.initialize()
cm = ConnectionManager()
ws = WebSocketHandler(
    db, cm,
    host="0.0.0.0", port=WS_PORT,
    tool_registry=tool_registry,
)
```

确保 `ws_handler.py` 的 `__init__` 已经接受 `tool_registry` 参数（任务 2 已改）。

- [ ] **步骤 2：验证重启**

```bash
systemctl restart kaya-beam && sleep 2 && systemctl is-active kaya-beam
```

预期：active

- [ ] **步骤 3：Commit**

```bash
git add server/run_and_send.py
git commit -m "feat: wire ToolRegistry into run_and_send.py"
```

---

### 任务 5：客户端 ws_client.rs 扩展

**文件：**
- 修改：`client/src-tauri/src/ws_client.rs`

- [ ] **步骤 1：添加 ToolDef 和工具注册**

在 `ws_client.rs` 顶部定义结构体：

```rust
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

pub fn local_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "take_screenshot",
            description: "Capture the Windows desktop screen",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "region": {
                        "type": "string",
                        "enum": ["full", "active_window"],
                        "description": "Screen region to capture"
                    }
                }
            }),
        },
        ToolDef {
            name: "get_clipboard",
            description: "Read the Windows clipboard content",
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "format": {
                        "type": "string",
                        "enum": ["text", "image"],
                        "description": "Clipboard format"
                    }
                }
            }),
        },
    ]
}
```

- [ ] **步骤 2：认证成功后发送 register_tools**

在 `run_client()` 中，处理 `auth_result` 之后（约在 Websocket 连接建立→认证成功后），发送工具注册消息：

```rust
// 在 auth 成功的处理逻辑中，追加：
let tools_msg = serde_json::json!({
    "type": "register_tools",
    "tools": local_tools().iter().map(|t| serde_json::json!({
        "name": t.name,
        "description": t.description,
        "inputSchema": t.input_schema,
    })).collect::<Vec<_>>(),
});
let _ = write.send(Message::Text(format!("{}\n", tools_msg.to_string()).into())).await;
```

- [ ] **步骤 3：处理 `call_tool` 消息**

在 WS 消息接收循环中，新增 `call_tool` 消息处理：

```rust
// 在消息匹配分支中增加：
if msg_type == "call_tool" {
    let request_id = data.get("request_id").and_then(|v| v.as_str()).unwrap_or("");
    let tool_name = data.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let args = data.get("arguments").cloned().unwrap_or(serde_json::json!({}));

    // 暂返回占位结果（Phase 2 接入真实 ToolExecutor）
    let result = serde_json::json!({
        "type": "tool_result",
        "request_id": request_id,
        "content": [{"type": "text", "text": format!("Tool '{}' called with args: {}", tool_name, args)}],
    });
    let _ = write.send(Message::Text(format!("{}\n", result.to_string()).into())).await;
}
```

- [ ] **步骤 4：会话更新发送**

当 ACP 客户端通过 event_tx 通知 session ready 时，把 session_id 通过 WS 通道发给服务端：

```rust
// 在 event 处理循环中，收到 AcpEvent::SessionReady 时：
if let Ok(sid) = session_id_str {
    let update = serde_json::json!({
        "type": "session_update",
        "session_id": sid,
    });
    let _ = ws_tx.send(Message::Text(format!("{}\n", update.to_string()).into())).await;
}
```

（注：需要 ws_client 的 event 通道和 ws_tx 在同一作用域。更简单的做法是 Phase 2 精细调整，当前先保留为 TODO 注释）

- [ ] **步骤 5：验证编译**

```bash
cd client/src-tauri && cargo check 2>&1 | tail -5
```

预期：编译通过（可能有 dead_code 警告）

- [ ] **步骤 6：Commit**

```bash
git add client/src-tauri/src/ws_client.rs
git commit -m "feat: client tool registration and call_tool handling in ws_client"
```

---

### 任务 6：集成测试

**文件：**
- 运行：全链路验证

- [ ] **步骤 1：编译验证**

```bash
cd client && npx vue-tsc --noEmit && echo "vue-tsc OK"
cd client/src-tauri && cargo check && echo "cargo OK"
cd server && python -m pytest tests/ -v && echo "pytest OK"
```

- [ ] **步骤 2：重启服务并确认**

```bash
systemctl restart kaya-beam && sleep 2 && systemctl status kaya-beam --no-pager | head -5
```

- [ ] **步骤 3：最终 commit**

```bash
git add -A
git commit -m "chore: Phase 1 integration - tool registry, ws handler, MCP tools, client registration"
```
