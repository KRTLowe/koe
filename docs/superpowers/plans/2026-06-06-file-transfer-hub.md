# File Transfer Hub 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 构建一个 MCP 工具 + Windows 桌面客户端的文件传输系统，让大模型（Kaya）能查询客户端在线状态并向指定客户端发送文件。

**架构：** Python 3 服务端同时运行 MCP stdio 接口（供 Kaya 调用）和 WebSocket 服务（供 Windows 客户端连接），共享一个在线客户端连接池。Tauri（Rust + Svelte）Windows 客户端运行于系统托盘，接收文件时弹出窗口通知。

**技术栈：** Python 3 + `mcp` SDK + `websockets` + `aiohttp` + SQLite + `bcrypt`；Tauri 2 + Rust + Svelte + Vite

---

## 文件结构

```
file-transfer-hub/
├── server/
│   ├── pyproject.toml
│   ├── src/file_transfer_hub/
│   │   ├── __init__.py
│   │   ├── __main__.py           # python -m file_transfer_hub 入口
│   │   ├── main.py               # serve 命令：启动 MCP + WebSocket
│   │   ├── server.py             # MCP 工具注册 (list_clients, send_file, register_client)
│   │   ├── ws_handler.py         # WebSocket 客户端连接处理
│   │   ├── connection_manager.py # 在线客户端连接池 {client_id → WebSocket}
│   │   ├── db.py                 # SQLite 客户端注册表操作
│   │   ├── auth.py               # passkey bcrypt 哈希 + 常量时间比较
│   │   ├── cli.py                # register-client / list-clients / remove-client 命令
│   │   └── models.py             # 数据类定义 (Client)
│   └── tests/
│       ├── __init__.py
│       ├── test_db.py
│       ├── test_auth.py
│       └── test_connection_manager.py
│
├── client/
│   ├── src-tauri/
│   │   ├── src/
│   │   │   ├── main.rs           # Tauri 入口，setup
│   │   │   ├── ws_client.rs      # WebSocket 连接、心跳、重连
│   │   │   ├── file_handler.rs   # 二进制帧接收、组装、写入
│   │   │   ├── tray.rs           # 系统托盘
│   │   │   ├── notify.rs         # 文件接收弹窗
│   │   │   └── config.rs         # 本地配置读写
│   │   ├── Cargo.toml
│   │   └── tauri.conf.json
│   ├── src/
│   │   ├── App.svelte
│   │   ├── main.ts
│   │   ├── ConfigPage.svelte
│   │   └── StatusPage.svelte
│   ├── package.json
│   ├── svelte.config.js
│   └── vite.config.ts
│
├── docs/
│   └── protocol.md
└── README.md
```

---

## 任务 1：服务端项目脚手架

**文件：**
- 创建：`server/pyproject.toml`
- 创建：`server/src/file_transfer_hub/__init__.py`
- 创建：`server/src/file_transfer_hub/__main__.py`
- 创建：`server/src/file_transfer_hub/models.py`
- 创建：`server/tests/__init__.py`

- [ ] **步骤 1：创建 pyproject.toml**

```toml
[project]
name = "file-transfer-hub"
version = "0.1.0"
description = "MCP file transfer server for LLM-to-client file pushing"
requires-python = ">=3.11"
dependencies = [
    "mcp>=1.0.0",
    "websockets>=12.0",
    "aiohttp>=3.9",
    "bcrypt>=4.0",
    "click>=8.0",
]

[project.scripts]
file-transfer-hub = "file_transfer_hub.__main__:main"

[build-system]
requires = ["setuptools>=68"]
build-backend = "setuptools.build_meta"

[tool.setuptools.packages.find]
where = ["src"]
```

- [ ] **步骤 2：创建 \_\_init\_\_.py 和 \_\_main\_\_.py**

```python
# __init__.py
"""File Transfer Hub — MCP tool for LLM-to-client file transfer."""

# __main__.py
from file_transfer_hub.cli import cli

if __name__ == "__main__":
    cli()
```

- [ ] **步骤 3：创建 models.py**

```python
from dataclasses import dataclass
from datetime import datetime
from typing import Optional


@dataclass
class Client:
    client_id: str
    description: str
    passkey_hash: str
    created_at: datetime
    updated_at: datetime
```

- [ ] **步骤 4：验证脚手架**

运行：`cd server && pip install -e . && file-transfer-hub --help`
预期：显示 file-transfer-hub 命令帮助信息

- [ ] **步骤 5：Commit**

```bash
git add server/
git commit -m "feat(file-transfer-hub): add server project scaffold"
```

---

## 任务 2：数据库层

**文件：**
- 创建：`server/src/file_transfer_hub/db.py`
- 创建：`server/tests/test_db.py`

- [ ] **步骤 1：编写失败的测试**

```python
# tests/test_db.py
import pytest
import tempfile
from pathlib import Path
from file_transfer_hub.db import Database


def test_create_tables():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        # 表应该存在，能正常写入
        db.register_client("pc-01", "My PC", "hash123")
        client = db.get_client("pc-01")
        assert client is not None
        assert client.client_id == "pc-01"
        assert client.description == "My PC"
        assert client.passkey_hash == "hash123"
        db.close()


def test_list_clients():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        db.register_client("pc-01", "PC 1", "h1")
        db.register_client("pc-02", "PC 2", "h2")
        clients = db.list_clients()
        assert len(clients) == 2
        db.close()


def test_remove_client():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        db.register_client("pc-01", "PC 1", "h1")
        db.remove_client("pc-01")
        assert db.get_client("pc-01") is None
        db.close()


def test_client_exists():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        assert not db.client_exists("pc-01")
        db.register_client("pc-01", "PC 1", "h1")
        assert db.client_exists("pc-01")
        db.close()
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd server && python -m pytest tests/test_db.py -v`
预期：ImportError / ModuleNotFoundError — Database 未定义

- [ ] **步骤 3：编写 db.py**

```python
import sqlite3
from pathlib import Path
from datetime import datetime
from typing import Optional, List
from file_transfer_hub.models import Client


class Database:
    def __init__(self, db_path: str = None):
        if db_path is None:
            db_path = str(Path.home() / ".file-transfer-hub" / "hub.db")
            Path(db_path).parent.mkdir(parents=True, exist_ok=True)
        self.db_path = db_path
        self.conn: Optional[sqlite3.Connection] = None

    def initialize(self):
        self.conn = sqlite3.connect(self.db_path)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("""
            CREATE TABLE IF NOT EXISTS clients (
                client_id TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                passkey_hash TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        """)
        self.conn.commit()

    def register_client(self, client_id: str, description: str, passkey_hash: str) -> Client:
        now = datetime.utcnow().isoformat()
        self.conn.execute(
            "INSERT OR REPLACE INTO clients (client_id, description, passkey_hash, created_at, updated_at) "
            "VALUES (?, ?, ?, COALESCE((SELECT created_at FROM clients WHERE client_id=?), ?), ?)",
            (client_id, description, passkey_hash, client_id, now, now),
        )
        self.conn.commit()
        return self.get_client(client_id)

    def get_client(self, client_id: str) -> Optional[Client]:
        row = self.conn.execute(
            "SELECT * FROM clients WHERE client_id = ?", (client_id,)
        ).fetchone()
        if row is None:
            return None
        return Client(
            client_id=row["client_id"],
            description=row["description"],
            passkey_hash=row["passkey_hash"],
            created_at=datetime.fromisoformat(row["created_at"]),
            updated_at=datetime.fromisoformat(row["updated_at"]),
        )

    def list_clients(self) -> List[Client]:
        rows = self.conn.execute("SELECT * FROM clients ORDER BY created_at").fetchall()
        return [
            Client(
                client_id=r["client_id"],
                description=r["description"],
                passkey_hash=r["passkey_hash"],
                created_at=datetime.fromisoformat(r["created_at"]),
                updated_at=datetime.fromisoformat(r["updated_at"]),
            )
            for r in rows
        ]

    def remove_client(self, client_id: str) -> bool:
        cur = self.conn.execute("DELETE FROM clients WHERE client_id = ?", (client_id,))
        self.conn.commit()
        return cur.rowcount > 0

    def client_exists(self, client_id: str) -> bool:
        row = self.conn.execute(
            "SELECT 1 FROM clients WHERE client_id = ?", (client_id,)
        ).fetchone()
        return row is not None

    def close(self):
        if self.conn:
            self.conn.close()
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd server && python -m pytest tests/test_db.py -v`
预期：4 passed

- [ ] **步骤 5：Commit**

```bash
git add server/src/file_transfer_hub/db.py server/tests/test_db.py
git commit -m "feat(file-transfer-hub): add SQLite database layer"
```

---

## 任务 3：认证模块

**文件：**
- 创建：`server/src/file_transfer_hub/auth.py`
- 创建：`server/tests/test_auth.py`

- [ ] **步骤 1：编写失败的测试**

```python
# tests/test_auth.py
import pytest
from file_transfer_hub.auth import hash_passkey, verify_passkey


def test_hash_and_verify():
    passkey = "my-secret-key-123"
    hashed = hash_passkey(passkey)
    assert hashed != passkey  # 不存明文
    assert verify_passkey(passkey, hashed) is True


def test_wrong_passkey():
    hashed = hash_passkey("correct-key")
    assert verify_passkey("wrong-key", hashed) is False


def test_constant_time_comparison():
    """确认不会因长度不同而提前返回（不测实现，测行为）"""
    h1 = hash_passkey("short")
    h2 = hash_passkey("a-very-long-passkey-12345")
    assert verify_passkey("short", h1) is True
    assert verify_passkey("short", h2) is False
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd server && python -m pytest tests/test_auth.py -v`
预期：ImportError — auth 未定义

- [ ] **步骤 3：编写 auth.py**

```python
import bcrypt
import hmac


def hash_passkey(passkey: str) -> str:
    """返回 bcrypt 哈希字符串"""
    return bcrypt.hashpw(passkey.encode("utf-8"), bcrypt.gensalt()).decode("utf-8")


def verify_passkey(passkey: str, hashed: str) -> bool:
    """常量时间比较验证 passkey"""
    return bcrypt.checkpw(passkey.encode("utf-8"), hashed.encode("utf-8"))
```

bcrypt.checkpw 内部已使用常量时间比较，不需要额外实现 hmac.compare_digest。

- [ ] **步骤 4：运行测试验证通过**

运行：`cd server && python -m pytest tests/test_auth.py -v`
预期：3 passed

- [ ] **步骤 5：Commit**

```bash
git add server/src/file_transfer_hub/auth.py server/tests/test_auth.py
git commit -m "feat(file-transfer-hub): add passkey auth with bcrypt"
```

---

## 任务 4：连接管理器

**文件：**
- 创建：`server/src/file_transfer_hub/connection_manager.py`
- 创建：`server/tests/test_connection_manager.py`

- [ ] **步骤 1：编写失败的测试**

```python
# tests/test_connection_manager.py
import pytest
from file_transfer_hub.connection_manager import ConnectionManager


@pytest.mark.asyncio
async def test_register_and_get():
    mgr = ConnectionManager()
    fake_ws = "fake-websocket-object"
    mgr.register("pc-01", fake_ws)
    assert mgr.is_online("pc-01")
    assert mgr.get_connection("pc-01") == fake_ws


@pytest.mark.asyncio
async def test_unregister():
    mgr = ConnectionManager()
    mgr.register("pc-01", "ws")
    mgr.unregister("pc-01")
    assert not mgr.is_online("pc-01")


@pytest.mark.asyncio
async def test_list_online():
    mgr = ConnectionManager()
    mgr.register("pc-01", "ws1")
    mgr.register("pc-02", "ws2")
    online = mgr.list_online()
    assert set(online) == {"pc-01", "pc-02"}
    mgr.unregister("pc-01")
    online = mgr.list_online()
    assert set(online) == {"pc-02"}
```

- [ ] **步骤 2：运行测试验证失败**

运行：`cd server && python -m pytest tests/test_connection_manager.py -v`
预期：ImportError

- [ ] **步骤 3：编写 connection_manager.py**

```python
import asyncio
import time
from dataclasses import dataclass
from typing import Dict, Optional, List, Any


@dataclass
class Connection:
    client_id: str
    websocket: Any
    connected_at: float
    last_heartbeat: float


class ConnectionManager:
    """在线客户端连接池，MCP 和 WebSocket 共享的桥梁。"""

    def __init__(self, heartbeat_timeout: float = 90.0):
        self._connections: Dict[str, Connection] = {}
        self._heartbeat_timeout = heartbeat_timeout

    def register(self, client_id: str, websocket: Any) -> None:
        now = time.time()
        self._connections[client_id] = Connection(
            client_id=client_id,
            websocket=websocket,
            connected_at=now,
            last_heartbeat=now,
        )

    def unregister(self, client_id: str) -> None:
        self._connections.pop(client_id, None)

    def get_connection(self, client_id: str) -> Optional[Any]:
        conn = self._connections.get(client_id)
        return conn.websocket if conn else None

    def is_online(self, client_id: str) -> bool:
        conn = self._connections.get(client_id)
        if conn is None:
            return False
        if time.time() - conn.last_heartbeat > self._heartbeat_timeout:
            self.unregister(client_id)
            return False
        return True

    def update_heartbeat(self, client_id: str) -> None:
        conn = self._connections.get(client_id)
        if conn:
            conn.last_heartbeat = time.time()

    def list_online(self) -> List[str]:
        now = time.time()
        expired = []
        for cid, conn in self._connections.items():
            if now - conn.last_heartbeat > self._heartbeat_timeout:
                expired.append(cid)
        for cid in expired:
            self.unregister(cid)
        return list(self._connections.keys())
```

- [ ] **步骤 4：运行测试验证通过**

运行：`cd server && python -m pytest tests/test_connection_manager.py -v`
预期：3 passed

- [ ] **步骤 5：Commit**

```bash
git add server/src/file_transfer_hub/connection_manager.py server/tests/test_connection_manager.py
git commit -m "feat(file-transfer-hub): add connection manager for online clients"
```

---

## 任务 5：WebSocket 处理器

**文件：**
- 创建：`server/src/file_transfer_hub/ws_handler.py`

- [ ] **步骤 1：编写 ws_handler.py**

```python
import asyncio
import json
import logging
import hashlib
import uuid
from typing import Optional

import websockets
from websockets.server import WebSocketServerProtocol, serve

from file_transfer_hub.db import Database
from file_transfer_hub.auth import verify_passkey
from file_transfer_hub.connection_manager import ConnectionManager

logger = logging.getLogger(__name__)


class WebSocketHandler:
    """处理客户端 WebSocket 连接、认证、心跳和文件推送。"""

    def __init__(
        self,
        db: Database,
        connection_manager: ConnectionManager,
        host: str = "0.0.0.0",
        port: int = 9765,
    ):
        self.db = db
        self.cm = connection_manager
        self.host = host
        self.port = port
        self._server: Optional[serve] = None

    async def start(self):
        self._server = await websockets.serve(
            self._handle_client,
            self.host,
            self.port,
            ping_interval=None,  # 用自己的 heartbeat
            ping_timeout=None,
        )
        logger.info(f"WebSocket server started on ws://{self.host}:{self.port}")

    async def stop(self):
        if self._server:
            self._server.close()
            await self._server.wait_closed()

    async def _handle_client(self, websocket: WebSocketServerProtocol):
        """处理单个客户端连接。"""
        client_id = None
        try:
            async for message in websocket:
                data = json.loads(message)
                msg_type = data.get("type")

                if msg_type == "auth":
                    client_id = data.get("client_id")
                    passkey = data.get("passkey")
                    if await self._authenticate(client_id, passkey, websocket):
                        self.cm.register(client_id, websocket)
                        await websocket.send(json.dumps({"type": "auth_result", "ok": True}))
                        logger.info(f"Client authenticated: {client_id}")
                    else:
                        await websocket.send(json.dumps(
                            {"type": "auth_result", "ok": False, "error": "auth failed"}
                        ))
                        break

                elif msg_type == "heartbeat":
                    if client_id:
                        self.cm.update_heartbeat(client_id)
                        await websocket.send(json.dumps({"type": "pong"}))

                elif msg_type == "file_ack":
                    file_id = data.get("file_id")
                    status = data.get("status")
                    logger.info(f"File {file_id} acknowledged by {client_id}: {status}")

        except websockets.exceptions.ConnectionClosed:
            logger.info(f"Client disconnected: {client_id}")
        finally:
            if client_id:
                self.cm.unregister(client_id)

    async def _authenticate(
        self, client_id: str, passkey: str, websocket: WebSocketServerProtocol
    ) -> bool:
        if not client_id or not passkey:
            return False
        client = self.db.get_client(client_id)
        if client is None:
            return False
        return verify_passkey(passkey, client.passkey_hash)

    async def send_file_to_client(
        self, client_id: str, file_path: str
    ) -> dict:
        """被 MCP tool 调用，向客户端推送文件。"""
        ws = self.cm.get_connection(client_id)
        if ws is None:
            return {"ok": False, "error": f"Client {client_id} is offline"}

        try:
            with open(file_path, "rb") as f:
                file_data = f.read()
        except FileNotFoundError:
            return {"ok": False, "error": f"File not found: {file_path}"}
        except IOError as e:
            return {"ok": False, "error": f"Failed to read file: {e}"}

        file_id = f"f_{uuid.uuid4().hex[:12]}"
        file_name = file_path.split("/")[-1]
        file_size = len(file_data)
        checksum = hashlib.sha256(file_data).hexdigest()

        # 三帧传输：元数据 → 二进制 → 结束
        meta = json.dumps({
            "type": "file_meta",
            "file_id": file_id,
            "name": file_name,
            "size": file_size,
        })
        end = json.dumps({
            "type": "file_end",
            "file_id": file_id,
            "checksum": f"sha256:{checksum}",
        })

        await ws.send(meta)
        await ws.send(file_data)  # 二进制帧
        await ws.send(end)

        return {
            "ok": True,
            "file_id": file_id,
            "name": file_name,
            "size": file_size,
            "client_id": client_id,
        }
```

- [ ] **步骤 2：Commit**

```bash
git add server/src/file_transfer_hub/ws_handler.py
git commit -m "feat(file-transfer-hub): add WebSocket handler with auth and file push"
```

---

## 任务 6：MCP 工具注册

**文件：**
- 创建：`server/src/file_transfer_hub/server.py`

- [ ] **步骤 1：编写 server.py**

```python
import logging
from typing import Optional

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import Tool, TextContent
import mcp.types as types
import pydantic

from file_transfer_hub.db import Database
from file_transfer_hub.auth import hash_passkey
from file_transfer_hub.connection_manager import ConnectionManager
from file_transfer_hub.ws_handler import WebSocketHandler

logger = logging.getLogger(__name__)


class MCPServer:
    """MCP 工具注册 + 服务生命周期管理。"""

    def __init__(self, db: Database, cm: ConnectionManager, ws_handler: WebSocketHandler):
        self.db = db
        self.cm = cm
        self.ws_handler = ws_handler
        self.server = Server("file-transfer-hub")

        self._register_tools()

    def _register_tools(self):
        @self.server.list_tools()
        async def list_tools() -> list[types.Tool]:
            return [
                Tool(
                    name="list_clients",
                    description="列出所有已注册的客户端及其在线状态。返回 client_id, description, online 字段。",
                    inputSchema={
                        "type": "object",
                        "properties": {},
                    },
                ),
                Tool(
                    name="send_file",
                    description="向指定在线客户端发送文件。需要客户端在线且文件路径在服务端可读。",
                    inputSchema={
                        "type": "object",
                        "properties": {
                            "client_id": {
                                "type": "string",
                                "description": "客户端 ID（如 pc-01）",
                            },
                            "file_path": {
                                "type": "string",
                                "description": "服务端上文件的绝对路径",
                            },
                        },
                        "required": ["client_id", "file_path"],
                    },
                ),
                Tool(
                    name="register_client",
                    description="预注册一个新客户端。返回 client_id 和确认信息。",
                    inputSchema={
                        "type": "object",
                        "properties": {
                            "client_id": {
                                "type": "string",
                                "description": "唯一客户端 ID（如 pc-01）",
                            },
                            "description": {
                                "type": "string",
                                "description": "客户端描述（如 Kricto's Windows PC）",
                            },
                            "passkey": {
                                "type": "string",
                                "description": "客户端认证密钥",
                            },
                        },
                        "required": ["client_id", "description", "passkey"],
                    },
                ),
            ]

        @self.server.call_tool()
        async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
            if name == "list_clients":
 return await self._handle_list_clients()
            elif name == "send_file":
                return await self._handle_send_file(arguments)
            elif name == "register_client":
                return await self._handle_register_client(arguments)
            else:
                raise ValueError(f"Unknown tool: {name}")

    async def _handle_list_clients(self) -> list[TextContent]:
        clients = self.db.list_clients()
        online_set = set(self.cm.list_online())
        lines = []
        for c in clients:
            status = "● 在线" if c.client_id in online_set else "○ 离线"
            lines.append(f"`{c.client_id}` | {c.description} | {status}")
        header = "| 客户端 ID | 描述 | 状态 |\n|---|---|---|"
        table = "\n".join([header] + lines) if lines else "（暂无已注册客户端）"
        return [TextContent(type="text", text=table)]

    async def _handle_send_file(self, args: dict) -> list[TextContent]:
        client_id = args["client_id"]
        file_path = args["file_path"]

        if not self.cm.is_online(client_id):
            return [TextContent(
                type="text",
                text=f"客户端 `{client_id}` 当前离线，无法发送文件。请先确认客户端在线。",
            )]

        result = await self.ws_handler.send_file_to_client(client_id, file_path)

        if result["ok"]:
            return [TextContent(
                type="text",
                text=f"✅ 文件已发送：`{result['name']}` ({result['size']} bytes) → `{client_id}`",
            )]
        else:
            return [TextContent(
                type="text",
                text=f"❌ 发送失败：{result['error']}",
            )]

    async def _handle_register_client(self, args: dict) -> list[TextContent]:
        client_id = args["client_id"]
        description = args["description"]
        passkey = args["passkey"]

        if self.db.client_exists(client_id):
            return [TextContent(
                type="text",
                text=f"客户端 `{client_id}` 已存在。如需更新，请先删除再注册。",
            )]

        passkey_hash = hash_passkey(passkey)
        self.db.register_client(client_id, description, passkey_hash)
        return [TextContent(
            type="text",
            text=f"✅ 客户端 `{client_id}` 注册成功（`{description}`）\n请将此 passkey 配置到客户端上。",
        )]

    async def run_stdio(self):
        async with stdio_server() as (read_stream, write_stream):
            await self.server.run(read_stream, write_stream, self.server.create_initialization_options())
```

注意：MCP SDK 的 Python 接口可能略有不同。如果 `list_tools` 和 `call_tool` 使用装饰器报错，改用 `server.tool()` 装饰器形式：

```python
@self.server.tool()
async def list_clients() -> list[types.TextContent]:
    ...

@self.server.tool()
async def send_file(client_id: str, file_path: str) -> list[types.TextContent]:
    ...

@self.server.tool()
async def register_client(client_id: str, description: str, passkey: str) -> list[types.TextContent]:
    ...
```

具体取决于 `mcp` Python SDK 版本。实现时以 SDK 文档为准。

- [ ] **步骤 2：Commit**

```bash
git add server/src/file_transfer_hub/server.py
git commit -m "feat(file-transfer-hub): add MCP tool registration"
```

---

## 任务 7：CLI 入口

**文件：**
- 创建：`server/src/file_transfer_hub/cli.py`

- [ ] **步骤 1：编写 cli.py**

```python
import asyncio
import logging
import sys

import click

from file_transfer_hub.db import Database
from file_transfer_hub.auth import hash_passkey
from file_transfer_hub.connection_manager import ConnectionManager
from file_transfer_hub.ws_handler import WebSocketHandler
from file_transfer_hub.server import MCPServer

logger = logging.getLogger(__name__)


@click.group()
@click.option("--db-path", default=None, help="SQLite 数据库路径")
@click.option("--verbose", is_flag=True, help="详细日志")
@click.pass_context
def cli(ctx, db_path, verbose):
    """File Transfer Hub — MCP tool for LLM-to-client file pushing."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
    )
    ctx.ensure_object(dict)
    ctx.obj["db_path"] = db_path


@cli.command()
@click.option("--ws-port", default=9765, help="WebSocket 服务端口")
@click.option("--ws-host", default="0.0.0.0", help="WebSocket 监听地址")
@click.pass_context
def serve(ctx, ws_port, ws_host):
    """启动服务：MCP stdio + WebSocket"""
    db_path = ctx.obj["db_path"]

    db = Database(db_path)
    db.initialize()
    cm = ConnectionManager()
    ws_handler = WebSocketHandler(db, cm, host=ws_host, port=ws_port)
    mcp_server = MCPServer(db, cm, ws_handler)

    async def _run():
        await ws_handler.start()
        logger.info(f"WebSocket server running on ws://{ws_host}:{ws_port}")
        logger.info("MCP server running on stdio")
        await mcp_server.run_stdio()

    try:
        asyncio.run(_run())
    except KeyboardInterrupt:
        logger.info("Shutting down...")


@cli.command()
@click.argument("client_id")
@click.argument("description")
@click.argument("passkey")
@click.pass_context
def register_client(ctx, client_id, description, passkey):
    """预注册一个客户端"""
    db = Database(ctx.obj["db_path"])
    db.initialize()
    if db.client_exists(client_id):
        click.echo(f"❌ 客户端 {client_id} 已存在")
        sys.exit(1)
    passkey_hash = hash_passkey(passkey)
    db.register_client(client_id, description, passkey_hash)
    click.echo(f"✅ 客户端 {client_id} ({description}) 注册成功")


@cli.command()
@click.pass_context
def list_clients(ctx):
    """列出所有已注册客户端"""
    db = Database(ctx.obj["db_path"])
    db.initialize()
    clients = db.list_clients()
    if not clients:
        click.echo("（暂无已注册客户端）")
        return
    for c in clients:
        click.echo(f"{c.client_id:20s} {c.description}")


@cli.command()
@click.argument("client_id")
@click.pass_context
def remove_client(ctx, client_id):
    """删除已注册客户端"""
    db = Database(ctx.obj["db_path"])
    db.initialize()
    if db.remove_client(client_id):
        click.echo(f"✅ 客户端 {client_id} 已删除")
    else:
        click.echo(f"❌ 客户端 {client_id} 不存在")
```

- [ ] **步骤 2：验证 CLI 命令**

运行：`cd server && file-transfer-hub --help`
预期：显示 serve, register-client, list-clients, remove-client 子命令

- [ ] **步骤 3：Commit**

```bash
git add server/src/file_transfer_hub/cli.py
git commit -m "feat(file-transfer-hub): add CLI entry with serve/register/list/remove"
```

---

## 任务 8：main.py 入口（集成）

**文件：**
- 创建：`server/src/file_transfer_hub/main.py`

- [ ] **步骤 1：编写 main.py**

```python
"""serve 命令的 main 函数，分离后保持 cli.py 整洁。"""

from file_transfer_hub.cli import cli

if __name__ == "__main__":
    cli()
```

- [ ] **步骤 2：确保 \_\_main\_\_.py 正确**

验证 `__main__.py` 已调用 `cli()`。更新 pyproject.toml 中 `[project.scripts]` 确保 `file-transfer-hub` 指向 `file_transfer_hub.__main__:main`。

- [ ] **步骤 3：Commit**

```bash
git add server/src/file_transfer_hub/main.py
git commit -m "feat(file-transfer-hub): add main entry point"
```

---

## 任务 9：Tauri 客户端项目脚手架

**文件：**
- 创建：`client/package.json`
- 创建：`client/vite.config.ts`
- 创建：`client/svelte.config.js`
- 创建：`client/src-tauri/Cargo.toml`
- 创建：`client/src-tauri/tauri.conf.json`
- 创建：`client/src-tauri/src/main.rs`
- 创建：`client/src/main.ts`
- 创建：`client/src/App.svelte`
- 创建：`client/index.html`

**前置条件：** 需要 Rust、Node.js、pnpm/npm 环境。Windows 上需要 WebView2（Win10+ 内置）。

- [ ] **步骤 1：创建 package.json**

```json
{
  "name": "file-transfer-hub-client",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "devDependencies": {
    "@sveltejs/vite-plugin-svelte": "^4.0.0",
    "@tauri-apps/cli": "^2.0.0",
    "svelte": "^5.0.0",
    "typescript": "^5.5.0",
    "vite": "^6.0.0"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0"
  }
}
```

- [ ] **步骤 2：创建 vite.config.ts**

```typescript
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_"],
});
```

- [ ] **步骤 3：创建 svelte.config.js**

```javascript
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

export default {
  preprocess: vitePreprocess(),
};
```

- [ ] **步骤 4：创建 index.html**

```html
<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>File Transfer Hub</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
```

- [ ] **步骤 5：创建 Cargo.toml**

```toml
[package]
name = "file-transfer-hub-client"
version = "0.1.0"
edition = "2021"

[lib]
name = "file_transfer_hub_client_lib"
crate-type = ["lib", "cdylib", "staticlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
tokio = { version = "1", features = ["full"] }
url = "2"
chrono = "0.4"
sha2 = "0.10"
base64 = "0.22"
log = "0.4"
env_logger = "0.11"
```

- [ ] **步骤 6：创建 tauri.conf.json**

```json
{
  "$schema": "https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-config-schema/schema.json",
  "productName": "File Transfer Hub",
  "version": "0.1.0",
  "identifier": "com.kricto.file-transfer-hub",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "windows": [],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/icon.ico"
    ]
  }
}
```

注意：windows 数组留空，应用默认不创建窗口。窗口由 Rust 端在收到文件时创建。

- [ ] **步骤 7：创建 src/main.ts**

```typescript
import App from "./App.svelte";

const app = new App({
  target: document.getElementById("app")!,
});

export default app;
```

- [ ] **步骤 8：创建 src/App.svelte**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import ConfigPage from "./ConfigPage.svelte";
  import StatusPage from "./StatusPage.svelte";

  let configured = $state(false);

  onMount(async () => {
    // 检查是否已配置
    const { invoke } = await import("@tauri-apps/api/core");
    try {
      const cfg = await invoke("load_config");
      configured = !!cfg;
    } catch {
      configured = false;
    }
  });
</script>

<main>
  {#if configured}
    <StatusPage />
  {:else}
    <ConfigPage on:configured={() => (configured = true)} />
  {/if}
</main>
```

- [ ] **步骤 9：创建 ConfigPage.svelte 和 StatusPage.svelte（占位）**

```svelte
<!-- src/ConfigPage.svelte -->
<script lang="ts">
  let serverUrl = $state("ws://");
  let clientId = $state("");
  let passkey = $state("");
  let saving = $state(false);
  let error = $state("");

  async function save() {
    saving = true;
    error = "";
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("save_config", {
        config: { serverUrl, clientId, passkey },
      });
    } catch (e: any) {
      error = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="config">
  <h1>File Transfer Hub</h1>
  <p>首次使用，请配置服务器连接信息</p>
  <input bind:value={serverUrl} placeholder="WebSocket 地址 (ws://...)" />
  <input bind:value={clientId} placeholder="客户端 ID" />
  <input bind:value={passkey} type="password" placeholder="Passkey" />
  {#if error}<p class="error">{error}</p>{/if}
  <button onclick={save} disabled={saving}>
    {saving ? "保存中..." : "保存并连接"}
  </button>
</div>
```

```svelte
<!-- src/StatusPage.svelte -->
<script lang="ts">
  import { onMount } from "svelte";

  let status = $state("连接中...");
  let lastHeartbeat = $state("");

  onMount(async () => {
    const { listen } = await import("@tauri-apps/api/event");
    await listen("connection-status", (e) => {
      status = e.payload.status;
      lastHeartbeat = e.payload.lastHeartbeat || "";
    });
  });
</script>

<div class="status">
  <h1>File Transfer Hub</h1>
  <p>状态：{status}</p>
  {#if lastHeartbeat}<p>上次心跳：{lastHeartbeat}</p>{/if}
</div>
```

- [ ] **步骤 10：创建 src-tauri/src/main.rs（初始）**

```rust
// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **步骤 11：验证脚手架编译**

运行：`cd client && npm install && cargo build`
预期：编译成功，生成初始二进制

- [ ] **步骤 12：Commit**

```bash
git add client/
git commit -m "feat(file-transfer-hub): add Tauri client scaffold"
```

---

## 任务 10：Tauri 客户端 — 配置模块

**文件：**
- 创建：`client/src-tauri/src/config.rs`
- 修改：`client/src-tauri/src/main.rs`

- [ ] **步骤 1：编写 config.rs**

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub server_url: String,
    pub client_id: String,
    pub passkey: String,
}

impl AppConfig {
    pub fn is_valid(&self) -> bool {
        !self.server_url.is_empty()
            && !self.client_id.is_empty()
            && !self.passkey.is_empty()
    }
}

fn config_path(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_config_dir()
        .expect("Failed to get config dir");
    fs::create_dir_all(&dir).ok();
    dir.join("config.json")
}

pub fn load_config(app: &AppHandle) -> Option<AppConfig> {
    let path = config_path(app);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app);
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}
```

- [ ] **步骤 2：更新 main.rs 注册命令**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod ws_client;
mod file_handler;
mod tray;
mod notify;

use config::{AppConfig, load_config, save_config};
use std::sync::Mutex;
use tauri::Manager;

struct AppState {
    config: Mutex<Option<AppConfig>>,
}

#[tauri::command]
fn load_config_cmd(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Option<AppConfig>, String> {
    let cfg = load_config(&app);
    *state.config.lock().map_err(|e| e.to_string())? = cfg.clone();
    Ok(cfg)
}

#[tauri::command]
fn save_config_cmd(
    config: AppConfig,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if !config.is_valid() {
        return Err("配置不完整".to_string());
    }
    save_config(&app, &config)?;
    *state.config.lock().map_err(|e| e.to_string())? = Some(config);
    Ok(())
}

fn main() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            config: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![load_config_cmd, save_config_cmd])
        .setup(|app| {
            // 加载配置
            let cfg = load_config(&app.handle());
            let state = app.state::<AppState>();
            *state.config.lock().unwrap() = cfg;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **步骤 3：Commit**

```bash
git add client/src-tauri/src/config.rs client/src-tauri/src/main.rs
git commit -m "feat(file-transfer-hub): add config module for Tauri client"
```

---

## 任务 11：Tauri 客户端 — WebSocket 客户端模块

**文件：**
- 创建：`client/src-tauri/src/ws_client.rs`
- 修改：`client/src-tauri/src/main.rs`

- [ ] **步骤 1：编写 ws_client.rs**

```rust
use crate::config::AppConfig;
use crate::file_handler;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

pub enum WsEvent {
    Connected,
    Disconnected,
    FileReceived { name: String, size: u64, data: Vec<u8> },
    Error(String),
}

pub async fn run_client(
    config: AppConfig,
    tx: mpsc::Sender<WsEvent>,
    mut rx: mpsc::Receiver<()>, // 接收停止信号
) {
    let ws_url = match Url::parse(&config.server_url) {
        Ok(u) => u,
        Err(e) => {
            let _ = tx.send(WsEvent::Error(format!("Invalid URL: {}", e))).await;
            return;
        }
    };

    loop {
        // 连接
        let connect_result = connect_async(&ws_url).await;
        let (ws_stream, _) = match connect_result {
            Ok(r) => r,
            Err(e) => {
                let _ = tx.send(WsEvent::Error(format!("Connection failed: {}", e))).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        let _ = tx.send(WsEvent::Connected).await;

        let (mut write, mut read) = ws_stream.split();

        // 发送 auth
        let auth_msg = serde_json::json!({
            "type": "auth",
            "client_id": config.client_id,
            "passkey": config.passkey,
        });
        if write.send(Message::Text(auth_msg.to_string())).await.is_err() {
            continue;
        }

        // 心跳任务
        let (heartbeat_tx, mut heartbeat_rx) = mpsc::channel::<()>(1);
        let mut heartbeat_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                if heartbeat_tx.try_send(()).is_err() {
                    break;
                }
            }
        });

        let mut file_receive_state: Option<file_handler::FileReceive> = None;

        loop {
            tokio::select! {
                _ = heartbeat_rx.recv() => {
                    if write.send(Message::Text(r#"{"type":"heartbeat"}"#.into())).await.is_err() {
                        break;
                    }
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // 处理文本帧
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                                match val["type"].as_str() {
                                    Some("auth_result") => {
                                        if val["ok"].as_bool().unwrap_or(false) {
                                            log::info!("Authenticated successfully");
                                        } else {
                                            let err = val["error"].as_str().unwrap_or("unknown");
                                            let _ = tx.send(WsEvent::Error(format!("Auth failed: {}", err))).await;
                                            break;
                                        }
                                    }
                                    Some("pong") => {
                                        // 心跳响应，无需处理
                                    }
                                    Some("file_meta") => {
                                        // 开始接收文件
                                        let file_id = val["file_id"].as_str().unwrap_or("").to_string();
                                        let name = val["name"].as_str().unwrap_or("unknown").to_string();
                                        let size = val["size"].as_u64().unwrap_or(0);
                                        file_receive_state = Some(file_handler::FileReceive::new(file_id, name, size));
                                    }
                                    Some("file_end") => {
                                        // 文件传输结束
                                        if let Some(state) = file_receive_state.take() {
                                            let checksum = val["checksum"].as_str().unwrap_or("");
                                            let file_data = state.data;
                                            let file_name = state.name;
                                            let file_size = state.data.len() as u64;

                                            // 验证 checksum
                                            let hash = sha2::Sha256::digest(&file_data);
                                            let hash_hex = format!("sha256:{:x}", hash);
                                            if !checksum.is_empty() && hash_hex != checksum {
                                                let _ = tx.send(WsEvent::Error("Checksum mismatch".to_string())).await;
                                            } else {
                                                // 保存到 temp
                                                let _ = tx.send(WsEvent::FileReceived {
                                                    name: file_name,
                                                    size: file_size,
                                                    data: file_data,
                                                }).await;
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            // 二进制帧 = 文件内容
                            if let Some(state) = &mut file_receive_state {
                                state.append_data(data);
                            }
                        }
                        Some(Ok(Message::Close(_))) => break,
                        Some(Err(e)) => {
                            let _ = tx.send(WsEvent::Error(format!("WebSocket error: {}", e))).await;
                            break;
                        }
                        None => break,
                        _ => {}
                    }
                }
                _ = rx.recv() => {
                    // 收到停止信号
                    let _ = write.close().await;
                    heartbeat_handle.abort();
                    return;
                }
            }
        }

        heartbeat_handle.abort();
        let _ = tx.send(WsEvent::Disconnected).await;

        // 重连前等待
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
```

- [ ] **步骤 2：更新 main.rs 集成 WebSocket 客户端**

扩展 main.rs 的 setup，启动 WebSocket 客户端任务，通过事件通道与前端通信。

```rust
// 在 main.rs 的 setup 闭包中添加：
use tokio::sync::mpsc;
use tauri::Emitter;

// ...

.setup(|app| {
    let handle = app.handle().clone();
    let cfg = load_config(&handle);
    let state = app.state::<AppState>();
    *state.config.lock().unwrap() = cfg.clone();

    // 如果有配置，启动 WebSocket 客户端
    if let Some(config) = cfg {
        let (ws_tx, mut ws_rx) = mpsc::channel::<WsEvent>(100);
        let (stop_tx, stop_rx) = mpsc::channel::<()>(1);

        // 保存 stop_tx 以供后续使用
        // 启动后台任务
        tokio::spawn(async move {
            ws_client::run_client(config, ws_tx, stop_rx).await;
        });

        // 监听事件并转发到前端
        let handle_clone = handle.clone();
        tokio::spawn(async move {
            while let Some(event) = ws_rx.recv().await {
                match event {
                    WsEvent::Connected => {
                        let _ = handle_clone.emit("connection-status", serde_json::json!({"status": "已连接"}));
                    }
                    WsEvent::Disconnected => {
                        let _ = handle_clone.emit("connection-status", serde_json::json!({"status": "已断开"}));
                    }
                    WsEvent::FileReceived { name, size, data } => {
                        // 保存文件并通知
                        let save_path = notify::on_file_received(&handle_clone, &name, size, &data);
                        // 发送 ack
                        // ...
                    }
                    WsEvent::Error(e) => {
                        log::error!("WS Error: {}", e);
                        let _ = handle_clone.emit("connection-status", serde_json::json!({"status": format!("错误: {}", e)}));
                    }
                }
            }
        });
    }

    Ok(())
})
```

- [ ] **步骤 3：Commit**

```bash
git add client/src-tauri/src/ws_client.rs client/src-tauri/src/main.rs
git commit -m "feat(file-transfer-hub): add WebSocket client with auth and file receiving"
```

---

## 任务 12：Tauri 客户端 — 文件处理器

**文件：**
- 创建：`client/src-tauri/src/file_handler.rs`

- [ ] **步骤 1：编写 file_handler.rs**

```rust
use std::path::PathBuf;

/// 文件接收状态机
pub struct FileReceive {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub data: Vec<u8>,
}

impl FileReceive {
    pub fn new(file_id: String, name: String, size: u64) -> Self {
        Self {
            file_id,
            name,
            size,
            data: Vec::with_capacity(size as usize),
        }
    }

    pub fn append_data(&mut self, chunk: Vec<u8>) {
        self.data.extend(chunk);
    }
}

/// 获取临时保存目录
pub fn temp_dir() -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push("file-transfer-hub");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// 保存文件到临时目录，返回完整路径
pub fn save_file(name: &str, data: &[u8]) -> Result<PathBuf, String> {
    let dir = temp_dir();
    let path = dir.join(name);

    // 如果文件已存在，添加时间戳后缀
    let path = if path.exists() {
        let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
        let stem = path.file_stem().unwrap_or_default().to_str().unwrap_or("file");
        let ext = path.extension()
            .map(|e| format!(".{}", e.to_str().unwrap_or("")))
            .unwrap_or_default();
        dir.join(format!("{}_{}{}", stem, ts, ext))
    } else {
        path
    };

    std::fs::write(&path, data).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(path)
}
```

- [ ] **步骤 2：Commit**

```bash
git add client/src-tauri/src/file_handler.rs
git commit -m "feat(file-transfer-hub): add file handler for Tauri client"
```

---

## 任务 13：Tauri 客户端 — 托盘模块

**文件：**
- 创建：`client/src-tauri/src/tray.rs`
- 修改：`client/src-tauri/src/main.rs`

- [ ] **步骤 1：编写 tray.rs**

```rust
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    menu::{MenuBuilder, MenuItemBuilder},
    AppHandle, Emitter,
};

pub fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItemBuilder::with_id("show", "显示窗口").build(app)?;
    let recent_item = MenuItemBuilder::with_id("recent", "最近文件").build(app)?;
    let quit_item = MenuItemBuilder::with_id("quit", "退出").build(app)?;

    let menu = MenuBuilder::new(app)
        .item(&show_item)
        .item(&recent_item)
        .separator()
        .item(&quit_item)
        .build()?;

    let tray = TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    // 显示主窗口
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "recent" => {
                    // 打开最近文件目录
                    // 文件管理器打开 temp dir
                    let dir = std::env::temp_dir().join("file-transfer-hub");
                    let _ = open::that(dir);
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}
```

注意：需要在 Cargo.toml 添加 `open` crate 用于打开文件管理器。

```toml
open = "5"
```

- [ ] **步骤 2：更新 main.rs 集成托盘**

```rust
// 在 setup 闭包中调用
.setup(|app| {
    tray::setup_tray(app.handle())?;
    // ... 其他设置
})
```

- [ ] **步骤 3：Commit**

```bash
git add client/src-tauri/src/tray.rs client/src-tauri/Cargo.toml client/src-tauri/src/main.rs
git commit -m "feat(file-transfer-hub): add system tray for Tauri client"
```

---

## 任务 14：Tauri 客户端 — 弹窗通知模块

**文件：**
- 创建：`client/src-tauri/src/notify.rs`
- 修改：`client/src-tauri/src/main.rs`

- [ ] **步骤 1：编写 notify.rs**

```rust
use crate::file_handler;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

/// 收到文件后的处理逻辑：保存文件 + 创建通知窗口
pub fn on_file_received(
    app: &AppHandle,
    name: &str,
    size: u64,
    data: &[u8],
) -> PathBuf {
    // 保存文件
    let save_path = file_handler::save_file(name, data)
        .unwrap_or_else(|_| PathBuf::from(name));

    // 发送事件给前端（前端窗口会弹出）
    let _ = app.emit("file-received", serde_json::json!({
        "name": name,
        "size": size,
        "path": save_path.to_string_lossy(),
    }));

    // 如果有主窗口，将其显示到前台
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit("file-received", serde_json::json!({
            "name": name,
            "size": size,
            "path": save_path.to_string_lossy(),
        }));
    }

    log::info!("File saved: {} ({}) -> {}", name, size, save_path.display());
    save_path
}
```

- [ ] **步骤 2：更新 StatusPage.svelte 监听 file-received 事件**

```svelte
<!-- 在 StatusPage.svelte 中添加 -->
let lastFile = $state<{name: string; size: number; path: string} | null>(null);

onMount(async () => {
  const { listen } = await import("@tauri-apps/api/event");
  await listen("file-received", (e: any) => {
    lastFile = e.payload;
    // 显示通知信息
  });
});
```

- [ ] **步骤 3：Commit**

```bash
git add client/src-tauri/src/notify.rs
git commit -m "feat(file-transfer-hub): add file notification module"
```

---

## 任务 15：协议文档

**文件：**
- 创建：`docs/protocol.md`

- [ ] **步骤 1：编写 protocol.md**

````markdown
# File Transfer Hub 通信协议

## 传输层

WebSocket（ws://），默认端口 9765。

## 消息格式

所有控制消息为 JSON 文本帧，文件内容为二进制帧。

### 认证

客户端 → 服务端：
```json
{"type": "auth", "client_id": "pc-01", "passkey": "xxx"}
```

服务端 → 客户端：
```json
{"type": "auth_result", "ok": true}
{"type": "auth_result", "ok": false, "error": "auth failed"}
```

### 心跳

客户端 → 服务端（每 30 秒）：
```json
{"type": "heartbeat"}
```

服务端 → 客户端：
```json
{"type": "pong"}
```

### 文件推送

服务端 → 客户端（三帧序列）：

1️⃣ 元数据（文本帧）：
```json
{"type": "file_meta", "file_id": "f_abc123", "name": "screenshot.png", "size": 1048576}
```

2️⃣ 文件内容（二进制帧）
3️⃣ 结束（文本帧）：
```json
{"type": "file_end", "file_id": "f_abc123", "checksum": "sha256:abc123..."}
```

客户端 → 服务端：
```json
{"type": "file_ack", "file_id": "f_abc123", "status": "ok"}
```

## 错误处理

- 未认证的消息被忽略
- 认证失败 → 服务端断开连接
- 心跳超时（90 秒无消息）→ 标记离线
- 文件校验和 mismatch → 客户端丢弃文件并记录日志
````

- [ ] **步骤 2：Commit**

```bash
git add docs/protocol.md
git commit -m "docs(file-transfer-hub): add protocol documentation"
```

---

## 任务 16：README

**文件：**
- 创建：`README.md`

- [ ] **步骤 1：编写 README.md**

```markdown
# File Transfer Hub

通过 MCP 工具让大模型直接推送文件到 Windows 桌面客户端。

## 架构

- **服务端**（Python 3）：MCP stdio 接口 + WebSocket 服务，单进程
- **客户端**（Tauri）：Windows 系统托盘应用，收到文件后弹窗通知

## 快速开始

### 服务端

```bash
pip install file-transfer-hub
file-transfer-hub register-client --id pc-01 --desc "My PC" --passkey "your-secret"
file-transfer-hub serve --ws-port 9765
```

### MCP 配置（opencode.json）

```json
{
  "mcpServers": {
    "file-transfer-hub": {
      "command": "file-transfer-hub",
      "args": ["serve"],
      "env": {
        "FT_HUB_WS_PORT": "9765",
        "FT_HUB_BIND": "0.0.0.0"
      }
    }
  }
}
```

### Windows 客户端

1. 从 Release 下载安装包
2. 首次启动配置 server_url、client_id、passkey
3. 客户端自动连接并运行在系统托盘

## systemd 配置

```ini
[Unit]
Description=File Transfer Hub
After=network.target

[Service]
ExecStart=/usr/local/bin/file-transfer-hub serve
Restart=always

[Install]
WantedBy=multi-user.target
```
```

- [ ] **步骤 2：Commit**

```bash
git add README.md
git commit -m "docs(file-transfer-hub): add README"
```

---

## 实现顺序建议

1. **服务端先完成**（任务 1-8）— 可独立测试，MCP Inspector 可验证工具
2. **客户端后实现**（任务 9-14）— 依赖服务端协议
3. **文档最后**（任务 15-16）

建议使用子代理驱动模式，每个任务一个独立子代理，按顺序执行。
