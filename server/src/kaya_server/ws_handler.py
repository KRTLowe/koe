import asyncio
import json
import logging
import os
import hashlib
import uuid
from pathlib import Path
from typing import Optional

import websockets
from websockets.server import WebSocketServerProtocol, serve

from kaya_server.db import Database
from kaya_server.auth import verify_passkey
from kaya_server.connection_manager import ConnectionManager
from kaya_transfer_hub.tool_registry import ToolRegistry
from kaya_server.signal_registry import SignalRegistry, SignalDispatcher, Priority
from kaya_server.signal_handlers import register_all as register_signal_handlers

logger = logging.getLogger(__name__)

MAX_FILE_SIZE = 500 * 1024 * 1024
ACK_TIMEOUT = 30.0  # 等待客户端 file_ack 的超时秒数

# 上传文件保存根目录
UPLOAD_DIR = os.path.join(os.path.dirname(__file__), "..", "..", "..", "transfers")


class UploadState:
    """客户端→服务端文件上传状态。"""

    # 允许超出声明的字节数（防止因传输层分帧导致的轻微超额）
    SIZE_GRACE = 1024 * 1024  # 1MB

    def __init__(self, file_id: str, name: str, size: int):
        self.file_id = file_id
        self.name = name
        self.size = size
        self.data = bytearray()

    def append(self, chunk: bytes) -> bool:
        """追加二进制数据。超过上限（size + grace）返回 False。"""
        self.data.extend(chunk)
        return len(self.data) <= self.size + self.SIZE_GRACE

    @property
    def over_limit(self) -> bool:
        return len(self.data) > self.size + self.SIZE_GRACE


class WebSocketHandler:
    """处理客户端 WebSocket 连接、认证、心跳和文件推送。"""

    def __init__(
        self,
        db: Database,
        connection_manager: ConnectionManager,
        host: str = "0.0.0.0",
        port: int = 9765,
        tool_registry: ToolRegistry | None = None,
        signal_registry: SignalRegistry | None = None,
    ):
        self.db = db
        self.cm = connection_manager
        self.host = host
        self.port = port
        self._server: Optional[websockets.WebSocketServer] = None
        # file_id → asyncio.Future，用于等待客户端 file_ack
        self._pending_acks: dict[str, asyncio.Future] = {}
        self.tool_registry = tool_registry or ToolRegistry()
        self.signal_registry = signal_registry or SignalRegistry()
        self.signal_dispatcher: SignalDispatcher | None = None
        # client_id → UploadState  进行中的上传
        self._uploads: dict[str, UploadState] = {}

    async def start(self):
        # 注册默认信号处理器（serve 和 run_and_send 共用）
        register_signal_handlers(self.tool_registry)

        self._server = await websockets.serve(
            self._handle_client,
            self.host,
            self.port,
            ping_interval=20,
            ping_timeout=10,
            max_size=None,  # 应用层自行控制大小（MAX_FILE_SIZE=500MB），协议层不做限制
        )
        self.signal_dispatcher = SignalDispatcher(
            self.signal_registry, self.tool_registry,
        )
        self.signal_dispatcher.set_send_callback(self._send_signal_notification)
        await self.signal_dispatcher.start()
        logger.info(f"WebSocket server started on ws://{self.host}:{self.port}")

    async def stop(self):
        if self.signal_dispatcher:
            await self.signal_dispatcher.stop()
        if self._server:
            self._server.close()
            await self._server.wait_closed()

    async def _handle_client(self, websocket: WebSocketServerProtocol):
        """处理单个客户端连接。"""
        client_id = None
        try:
            async for message in websocket:
                # 二进制帧 = 上传文件内容
                if isinstance(message, bytes):
                    if client_id and client_id in self._uploads:
                        if not self._uploads[client_id].append(message):
                            # 超出上限，断开连接
                            logger.warning(
                                f"Upload from {client_id} exceeded size limit, disconnecting"
                            )
                            self._uploads.pop(client_id, None)
                            break
                    continue

                data = json.loads(message)
                msg_type = data.get("type")

                if msg_type == "auth":
                    client_id = data.get("client_id")
                    passkey = data.get("passkey")
                    if await self._authenticate(client_id, passkey):
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
                    # 如果有正在等待此 file_id ack 的发送任务，唤醒它
                    future = self._pending_acks.get(file_id)
                    if future and not future.done():
                        future.set_result({"status": status, "error": data.get("error")})

                elif msg_type == "register_tools":
                    tools = data.get("tools", [])
                    if not isinstance(tools, list):
                        await websocket.send(json.dumps({
                            "type": "register_tools_result",
                            "ok": False,
                            "error": "\"tools\" must be a list",
                        }))
                        continue
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

                elif msg_type == "file_upload_start":
                    if not client_id:
                        continue
                    # 拒绝重复的 upload_start
                    if client_id in self._uploads:
                        await websocket.send(json.dumps({
                            "type": "file_upload_result", "file_id": data.get("file_id", ""),
                            "ok": False, "error": "Upload already in progress",
                        }))
                        continue
                    file_id = data.get("file_id", "")
                    name = data.get("name", "unknown")
                    size = data.get("size", 0)
                    if size > MAX_FILE_SIZE:
                        await websocket.send(json.dumps({
                            "type": "file_upload_result", "file_id": file_id,
                            "ok": False, "error": f"File too large: {size} (max {MAX_FILE_SIZE})",
                        }))
                        continue
                    self._uploads[client_id] = UploadState(file_id, name, size)
                    await websocket.send(json.dumps({
                        "type": "file_upload_start_ack", "file_id": file_id, "ok": True,
                    }))

                elif msg_type == "file_upload_end":
                    if not client_id or client_id not in self._uploads:
                        continue
                    state = self._uploads.pop(client_id)
                    # 保存到 transfers/YYYY-MM/
                    from datetime import datetime
                    now = datetime.now()
                    date_dir = os.path.join(UPLOAD_DIR, f"{now.year:04d}-{now.month:02d}")
                    os.makedirs(date_dir, exist_ok=True)
                    path = os.path.join(date_dir, state.name)
                    # 同名加时间戳
                    if os.path.exists(path):
                        stem, ext = os.path.splitext(state.name)
                        path = os.path.join(date_dir, f"{stem}_{now.strftime('%Y%m%d%H%M%S')}{ext}")
                    try:
                        with open(path, "wb") as f:
                            f.write(state.data)
                        await websocket.send(json.dumps({
                            "type": "file_upload_result", "file_id": state.file_id,
                            "ok": True, "path": path, "name": state.name,
                            "size": len(state.data),
                        }))
                        logger.info(f"Upload saved: {path} from {client_id}")
                    except IOError as e:
                        await websocket.send(json.dumps({
                            "type": "file_upload_result", "file_id": state.file_id,
                            "ok": False, "error": f"Save failed: {e}",
                        }))

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
                        # 入队列，由 SignalDispatcher 决定何时推给 Kaya
                        sticky = data.get("sticky", False)
                        priority = Priority.from_str(data.get("priority", "normal"))
                        notify_once = data.get("notify_once", False)
                        self.signal_registry.push(
                            client_id, signal_name, signal_data,
                            sticky=sticky, priority=priority,
                            notify_once=notify_once,
                        )

                elif msg_type == "signal_clear":
                    signal_name = data.get("name")
                    if signal_name and client_id:
                        self.signal_registry.clear(client_id, signal_name)
                        logger.info(f"Signal cleared: {signal_name} from {client_id}")

        except websockets.exceptions.ConnectionClosed:
            logger.info(f"Client disconnected: {client_id}")
        except json.JSONDecodeError:
            logger.warning(f"Invalid JSON from {client_id}")
        finally:
            if client_id:
                self._uploads.pop(client_id, None)
                self.cm.unregister(client_id)
                self.signal_registry.clear_client(client_id)
                if self.tool_registry:
                    self.tool_registry.clear_client(client_id)

    async def _send_signal_notification(self, client_id: str, text: str):
        """被 SignalDispatcher 回调：向客户端发送 acp_inject 通知。"""
        ws = self.cm.get_connection(client_id)
        if ws is None:
            logger.warning("Signal notification dropped: client %s offline", client_id)
            return
        try:
            await ws.send(json.dumps({"type": "acp_inject", "text": text}))
        except Exception as e:
            logger.error("Failed to send signal notification to %s: %s", client_id, e)

    async def _authenticate(self, client_id: str, passkey: str) -> bool:
        if not client_id or not passkey:
            return False
        client = self.db.get_client(client_id)
        if client is None:
            return False
        return verify_passkey(passkey, client.passkey_hash)

    async def _wait_for_ack(self, file_id: str, timeout: float = ACK_TIMEOUT) -> dict:
        """等待客户端对指定 file_id 的确认。返回 ack 内容，超时返回错误。"""
        future = asyncio.get_event_loop().create_future()
        self._pending_acks[file_id] = future
        try:
            result = await asyncio.wait_for(future, timeout=timeout)
            return result
        except asyncio.TimeoutError:
            return {"status": "timeout", "error": "Client ack timeout"}
        finally:
            self._pending_acks.pop(file_id, None)

    async def send_tool_call(self, client_id: str, request_id: str, name: str, arguments: dict) -> bool:
        """向客户端发送工具调用请求。返回是否发送成功。"""
        ws = self.cm.get_connection(client_id)
        if ws is None:
            logger.warning("send_tool_call: client %s not connected", client_id)
            return False
        payload = json.dumps({
            "type": "call_tool",
            "request_id": request_id,
            "name": name,
            "arguments": arguments,
        })
        try:
            await ws.send(payload)
            logger.info("WS -> %s: call_tool request_id=%s name=%s", client_id, request_id, name)
            return True
        except Exception as e:
            logger.error("Failed to send tool call to %s: %s", client_id, e)
            return False

    async def send_file_to_client(self, client_id: str, file_path: str) -> dict:
        """被 MCP tool 调用，向客户端推送文件。"""
        ws = self.cm.get_connection(client_id)
        if ws is None:
            return {"ok": False, "error": f"Client {client_id} is offline"}

        path = Path(file_path)
        if not path.is_absolute():
            return {"ok": False, "error": f"Path must be absolute: {file_path}"}

        # 先打开 fd 再 stat，避免 TOCTOU 竞态
        try:
            fd = path.open("rb")
        except FileNotFoundError:
            return {"ok": False, "error": f"File not found: {file_path}"}
        except IOError as e:
            return {"ok": False, "error": f"Failed to open file: {e}"}

        try:
            st_size = os.fstat(fd.fileno()).st_size
            if st_size > MAX_FILE_SIZE:
                return {"ok": False, "error": f"File too large: {st_size} bytes (max {MAX_FILE_SIZE})"}

            file_data = fd.read()
        except IOError as e:
            return {"ok": False, "error": f"Failed to read file: {e}"}
        finally:
            fd.close()

        file_id = f"f_{uuid.uuid4().hex[:12]}"
        file_name = path.name
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

        # 等待客户端 ack（超时不算失败，记录日志即可——文件可能已到达但 ack 丢了）
        ack = await self._wait_for_ack(file_id)
        if ack.get("status") == "error":
            return {
                "ok": False,
                "error": f"Client rejected file: {ack.get('error', 'unknown reason')}",
                "file_id": file_id,
                "name": file_name,
                "size": file_size,
                "client_id": client_id,
            }

        return {
            "ok": True,
            "file_id": file_id,
            "name": file_name,
            "size": file_size,
            "client_id": client_id,
            "ack_status": ack.get("status"),
        }
