# ACPBridge 移除与客户端注入改造 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 移除服务端 ACPBridge 及其独立 WS 连接，改为通过客户端 file-transfer-hub WS 回传注入消息，由客户端用自己的 ACP 连接注入。

**架构：** signal → WS:9765 → 服务端 handle_signal() 生成消息文本 → 回发 `acp_inject` → WS:9765 → 客户端 ws_client → event_tx → lib.rs → acp_tx → acp_client → session/prompt → Kaya

**技术栈：** Python (asyncio/websockets), Rust (tokio/tokio-tungstenite), Tauri, Vue 3

---

### 任务 1：服务端 ws_handler — 移除 ACPBridge + 改为回发 acp_inject

**文件：**
- 修改：`server/src/file_transfer_hub/ws_handler.py`

- [ ] **步骤 1：修改 __init__ 和 signal 处理**

移除 `acp_bridge` 参数和字段，signal 分支改为回发 `acp_inject` 消息，finally 块移除 `cancel_session`。

```python
# ws_handler.py 改动

# __init__ 删 acp_bridge 参数
def __init__(
    self,
    db: Database,
    connection_manager: ConnectionManager,
    host: str = "0.0.0.0",
    port: int = 9765,
    tool_registry: ToolRegistry | None = None,
):
    self.db = db
    self.cm = connection_manager
    self.host = host
    self.port = port
    self._server: Optional[websockets.WebSocketServer] = None
    self._pending_acks: dict[str, asyncio.Future] = {}
    self.tool_registry = tool_registry or ToolRegistry()
    self._uploads: dict[str, UploadState] = {}

# signal 分支改为回发
elif msg_type == "signal":
    signal_name = data.get("name")
    signal_data = data.get("data", {})
    if signal_name and client_id:
        logger.info(f"Signal '{signal_name}' from {client_id}: {signal_data}")
        await websocket.send(json.dumps({
            "type": "signal_ack",
            "name": signal_name,
            "ok": True,
        }))
        msg = self.tool_registry.handle_signal(client_id, signal_name, signal_data)
        if msg:
            await websocket.send(json.dumps({
                "type": "acp_inject",
                "text": msg,
            }))

# finally 块删 acp_bridge 相关
finally:
    if client_id:
        self._uploads.pop(client_id, None)
        self.cm.unregister(client_id)
        if self.tool_registry:
            self.tool_registry.clear_client(client_id)
```

- [ ] **步骤 2：验证改动**

```bash
cd /kaya/tmp_workplace/kaya-beam/server
python -c "from file_transfer_hub.ws_handler import WebSocketHandler; print('OK')"
```

预期：无 ImportError（不再引用 `acp_bridge`）

- [ ] **步骤 3：Commit**

```bash
cd /kaya/tmp_workplace/kaya-beam/.worktrees/feat-remote-mcp
git add server/src/file_transfer_hub/ws_handler.py
git commit -m "refactor: remove ACPBridge from ws_handler, replace with acp_inject reply"
```

---

### 任务 2：服务端 run_and_send — 移除 ACPBridge 初始化

**文件：**
- 修改：`server/run_and_send.py`

- [ ] **步骤 1：移除 ACPBridge 相关代码**

```python
# 删 import
# - from file_transfer_hub.acp_bridge import ACPBridge

# 删 main() 中的初始化
# - acp_bridge = ACPBridge()
# - await acp_bridge.start()
# - ws = WebSocketHandler(db, cm, host="0.0.0.0", port=WS_PORT, tool_registry=tool_registry)  # 删 acp_bridge=

# 删 finally 中的
# - await acp_bridge.stop()

# 删全局变量
# - acp_bridge 在 global 声明中移除
```

- [ ] **步骤 2：验证改动**

```bash
python -c "import run_and_send; print('OK')"  # 从 server/ 目录
```

预期：无 ImportError，无语法错误

- [ ] **步骤 3：Commit**

```bash
git add server/run_and_send.py
git commit -m "refactor: remove ACPBridge initialization from run_and_send"
```

---

### 任务 3：删除 acp_bridge.py

**文件：**
- 删除：`server/src/file_transfer_hub/acp_bridge.py`

- [ ] **步骤 1：删除文件并清理 import 链**

```bash
git rm server/src/file_transfer_hub/acp_bridge.py
```

确认没有其他文件 import 它：

```bash
grep -r "acp_bridge" server/ --include="*.py"
```

预期：无匹配（ws_handler.py 和 run_and_send.py 已修正）

- [ ] **步骤 2：Commit**

```bash
git commit -m "refactor: delete acp_bridge.py"
```

---

### 任务 4：服务端 tool_registry — 移除 session 相关代码

**文件：**
- 修改：`server/src/file_transfer_hub/tool_registry.py`

- [ ] **步骤 1：移除 session 追踪**

```python
# 删
# - self._sessions: dict[str, str] = {}
# - def set_session(...)
# - def get_session(...)
# - clear_client 中的 self._sessions.pop(client_id, None)
```

- [ ] **步骤 2：同步清理 ws_handler 中 session_update 消息处理**

ws_handler.py 中的 `session_update` 分支（第158-162行）——删掉，因为服务端不再追踪 sessionId：

```python
# 删这个分支
elif msg_type == "session_update":
    session_id = data.get("session_id")
    if session_id and client_id:
        self.tool_registry.set_session(client_id, session_id)
```

- [ ] **步骤 3：验证**

```bash
python -c "
from file_transfer_hub.tool_registry import ToolRegistry
tr = ToolRegistry()
tr.register_tools('test', [{'name':'t','description':'d','inputSchema':{}}])
assert tr.get_tools('test') == [{'name':'t','description':'d','inputSchema':{}}]
print('session methods removed OK')
"
```

预期：`set_session` / `get_session` 不再存在

- [ ] **步骤 4：Commit**

```bash
git add server/src/file_transfer_hub/tool_registry.py server/src/file_transfer_hub/ws_handler.py
git commit -m "refactor: remove session tracking from tool_registry and ws_handler"
```

---

### 任务 5：客户端 ws_client — 新增 AcpInject 事件和处理分支

**文件：**
- 修改：`client/src-tauri/src/ws_client.rs`

- [ ] **步骤 1：WsEvent 新增 AcpInject 变体**

```rust
// 在 WsEvent 枚举中新增
pub enum WsEvent {
    Connected,
    Disconnected,
    FileReceived {
        name: String,
        size: u64,
        data: Vec<u8>,
    },
    AcpInject {
        text: String,
    },
    Error(String),
}
```

- [ ] **步骤 2：WS 消息循环新增 acp_inject 分支**

在 `msg = read.next()` 的 match 块中，在 `Some("call_tool")` 分支后面增加：

```rust
Some("acp_inject") => {
    if let Some(text) = val["text"].as_str() {
        let _ = event_tx.send(WsEvent::AcpInject {
            text: text.to_string(),
        }).await;
    }
}
```

- [ ] **步骤 3：验证编译**

```bash
cd client/src-tauri && cargo check 2>&1 | head -20
```

预期：编译通过，无错误

- [ ] **步骤 4：Commit**

```bash
git add client/src-tauri/src/ws_client.rs
git commit -m "feat(client): add AcpInject event for ACP signal injection"
```

---

### 任务 6：客户端 lib — 转发 AcpInject 到 ACP 连接

**文件：**
- 修改：`client/src-tauri/src/lib.rs`

- [ ] **步骤 1：在事件循环中处理 AcpInject**

在 `ws_client::WsEvent::FileReceived` 分支的 `else if` 后面追加：

```rust
ws_client::WsEvent::AcpInject { text } => {
    if let Some(s) = handle.try_state::<AppState>() {
        if let Ok(tx) = s.acp_tx.lock() {
            if let Some(tx) = tx.as_ref() {
                if tx.try_send(text.clone()).is_err() {
                    log::error!("Failed to forward ACP inject message");
                }
            }
        }
    }
}
```

- [ ] **步骤 2：验证编译**

```bash
cd client/src-tauri && cargo check 2>&1 | head -20
```

预期：编译通过，无错误

- [ ] **步骤 3：Commit**

```bash
git add client/src-tauri/src/lib.rs
git commit -m "feat(client): forward AcpInject events to ACP connection"
```

---

### 任务 7：同步服务端文件并重启

**文件：**
- 同步：`server/` 下的改动到 main project

- [ ] **步骤 1：同步到 main project 并重启**

```bash
# 同步改动的服务端文件
cp server/src/file_transfer_hub/ws_handler.py /kaya/tmp_workplace/kaya-beam/server/src/file_transfer_hub/ws_handler.py
cp server/run_and_send.py /kaya/tmp_workplace/kaya-beam/server/run_and_send.py
cp server/src/file_transfer_hub/tool_registry.py /kaya/tmp_workplace/kaya-beam/server/src/file_transfer_hub/tool_registry.py
rm /kaya/tmp_workplace/kaya-beam/server/src/file_transfer_hub/acp_bridge.py

# 重启服务
sudo systemctl restart kaya-beam.service
sleep 3
sudo systemctl status kaya-beam.service --no-pager -l | head -10
```

- [ ] **步骤 2：验证无 ACPBridge 连接**

```bash
journalctl -u kaya-beam.service --no-pager -n 15 | grep -E "ACPBridge|Bridge"
```

预期：无 ACPBridge 相关日志（不再尝试连接 bridge）

- [ ] **步骤 3：提交到 main project（可选）**

如果 worktree 和 main project 目录不同，commit 在 worktree 中即可。

---

### 任务 8：发送客户端文件到 Windows

**文件：**
- `client/src-tauri/src/ws_client.rs`
- `client/src-tauri/src/lib.rs`

- [ ] **步骤 1：上传到 Windows**

```bash
# 通过 ssh-mcp-server 发送改动的客户端文件
```

- [ ] **步骤 2：在 Windows 上编译验证**

在 Windows 客户端运行：
```bash
cd client && npm run tauri build
```

预期：编译成功

---

### 任务 9：集成测试

- [ ] **步骤 1：验证服务端正常启动**

```bash
journalctl -u kaya-beam.service --no-pager -n 10
```

预期：服务启动，WebSocket 监听 :9765，无 ACPBridge 相关日志

- [ ] **步骤 2：验证客户端连接后信号注入**

1. 启动 Windows 客户端，连接成功
2. 在 ACP 聊天中让 Kaya 调用 `take_screenshot`
3. 预期：截图自动上传，Kaya 收到系统通知并回复
4. 验证：服务端日志显示 `acp_inject` 消息被发送

- [ ] **步骤 3：验证 ACP 进程无泄漏**

```bash
ps aux | grep 'opencode acp' | grep -v grep
```

预期：仅有一个 `opencode acp` 进程（客户端 ACP 连接产生的），无服务端产生的残留进程
