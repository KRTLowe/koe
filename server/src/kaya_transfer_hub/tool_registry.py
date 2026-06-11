"""客户端工具注册表。管理工具定义、待处理调用和信号处理器。"""
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
        # request_id → client_id  用于清理 _client_invokes
        self._request_owner: dict[str, str] = {}
        # client_id → set[request_id]  用于断开时清理
        self._client_invokes: dict[str, set[str]] = {}
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
        """客户端断开时清理。取消所有待处理调用。"""
        self._tools.pop(client_id, None)
        # 取消该客户端的所有待处理调用
        for req_id in list(self._client_invokes.pop(client_id, set())):
            self._request_owner.pop(req_id, None)
            future = self._pending.pop(req_id, None)
            if future and not future.done():
                future.set_result({"ok": False, "error": "Client disconnected"})

    def has_tools(self, client_id: str) -> bool:
        return client_id in self._tools and bool(self._tools[client_id])

    # ── 工具调用 ──

    def create_invoke(self, client_id: str, tool_name: str, arguments: dict) -> str:
        """创建一个待处理的工具调用，返回 request_id。"""
        request_id = f"req_{uuid.uuid4().hex[:12]}"
        loop = asyncio.get_event_loop()
        future = loop.create_future()
        self._pending[request_id] = future
        self._request_owner[request_id] = client_id
        self._client_invokes.setdefault(client_id, set()).add(request_id)
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
            # 清理 _client_invokes 和 _request_owner
            client_id = self._request_owner.pop(request_id, None)
            if client_id and client_id in self._client_invokes:
                self._client_invokes[client_id].discard(request_id)

    def resolve_invoke(self, request_id: str, result: dict) -> bool:
        """客户端返回结果时，resolve 对应的 Future。"""
        future = self._pending.get(request_id)
        if future is None or future.done():
            return False
        future.set_result(result)
        return True

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
