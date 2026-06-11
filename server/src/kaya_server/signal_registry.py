"""信号注册与调度系统。

管理粘性/一次性信号的队列、优先级排序、通知间隔控制。
"""
import asyncio
import enum
import logging
import time
from dataclasses import dataclass, field
from typing import Callable, Coroutine

logger = logging.getLogger(__name__)


class Priority(enum.IntEnum):
    """优先级（值越小优先级越高）。"""
    CRITICAL = 0
    HIGH = 1
    NORMAL = 2
    LOW = 3

    @classmethod
    def from_str(cls, s: str) -> "Priority":
        mapping = {
            "critical": cls.CRITICAL,
            "high": cls.HIGH,
            "normal": cls.NORMAL,
            "low": cls.LOW,
        }
        return mapping.get(s.lower(), cls.NORMAL)


@dataclass
class SignalEntry:
    """队列中的信号条目。"""
    client_id: str
    name: str
    data: dict
    priority: Priority = Priority.NORMAL
    sticky: bool = False
    notify_once: bool = False
    created_at: float = field(default_factory=time.monotonic)
    last_notified: float | None = None


# 队列内部条目：(priority, seq, entry)，asyncio.PriorityQueue 按元组排序
_QueueItem = tuple[int, int, SignalEntry]


class SignalRegistry:
    """信号注册器。

    管理两类信号：
    - 一次性信号：入队列→通知→丢弃
    - 粘性信号：入队列→通知→保留直到收到 clear
    """

    def __init__(self):
        # 粘性信号: client_id → { signal_name → SignalEntry }
        self._sticky: dict[str, dict[str, SignalEntry]] = {}
        # 调度队列
        self._queue: asyncio.PriorityQueue[_QueueItem] = asyncio.PriorityQueue()
        self._seq = 0

    # ── 公开 API ──

    def push(
        self,
        client_id: str,
        name: str,
        data: dict,
        *,
        sticky: bool = False,
        priority: Priority = Priority.NORMAL,
        notify_once: bool = False,
    ) -> SignalEntry:
        """将信号入队。

        - 粘性信号：替换同名旧条目，保持最后活跃状态
        - 一次性信号：每次独立入队
        - notify_once：粘性信号只通知一次，之后不再重新入队（但仍保留在注册表中供查询）
        """
        entry = SignalEntry(
            client_id=client_id,
            name=name,
            data=data,
            priority=priority if isinstance(priority, Priority) else Priority.from_str(str(priority)),
            sticky=sticky,
            notify_once=notify_once,
        )
        if sticky:
            if client_id not in self._sticky:
                self._sticky[client_id] = {}
            self._sticky[client_id][name] = entry

        self._put(entry)
        return entry

    def clear(self, client_id: str, name: str) -> bool:
        """清除指定粘性信号。返回是否存在目标信号。"""
        if client_id not in self._sticky:
            return False
        found = self._sticky[client_id].pop(name, None)
        if not self._sticky[client_id]:
            del self._sticky[client_id]
        return found is not None

    def get_active(self, client_id: str) -> list[SignalEntry]:
        """获取某客户端所有活跃粘性信号。"""
        if client_id in self._sticky:
            return list(self._sticky[client_id].values())
        return []

    def clear_client(self, client_id: str):
        """客户端断开时清理所有信号。"""
        self._sticky.pop(client_id, None)

    def has_pending(self, client_id: str) -> bool:
        """客户端是否有待处理的信号。"""
        return client_id in self._sticky and bool(self._sticky[client_id])

    # ── 内部 ──

    def _put(self, entry: SignalEntry):
        self._seq += 1
        self._queue.put_nowait((entry.priority.value, self._seq, entry))


class SignalDispatcher:
    """信号调度循环。

    按优先级从队列取信号，通过回调发送 acp_inject。
    受最小通知间隔控制，避免刷屏。
    粘性信号按 TTL 重新入队，直到被 clear。
    """

    def __init__(
        self,
        registry: SignalRegistry,
        tool_registry,
        *,
        min_interval: float = 5.0,
        sticky_ttl: float = 30.0,
    ):
        self.registry = registry
        self.tool_registry = tool_registry
        self.min_interval = min_interval
        self.sticky_ttl = sticky_ttl

        self._task: asyncio.Task | None = None
        self._last_notified: float = 0
        # send_notification(client_id, text) -> Coroutine
        self._send_cb: Callable[[str, str], Coroutine] | None = None

    def set_send_callback(self, cb: Callable[[str, str], Coroutine]):
        """设置 acp_inject 发送回调。由 WebSocketHandler 提供。"""
        self._send_cb = cb

    async def start(self):
        self._task = asyncio.create_task(self._dispatch_loop())
        logger.info("SignalDispatcher started (interval=%ss, sticky_ttl=%ss)", self.min_interval, self.sticky_ttl)

    async def stop(self):
        if self._task:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass
            self._task = None

    # ── 调度循环 ──

    async def _dispatch_loop(self):
        while True:
            now = time.monotonic()
            since_last = now - self._last_notified
            wait = max(0.0, self.min_interval - since_last)

            # 用超时等队列，同时也允许定时检查粘性信号是否需要重新通知
            try:
                _, _, entry = await asyncio.wait_for(
                    self.registry._queue.get(),
                    timeout=max(wait, 0.5),
                )
            except asyncio.TimeoutError:
                # 队列空 → 检查粘性信号是否需要重新入队
                self._requeue_stale_sticky()
                continue

            # ── 处理取出的条目 ──

            # 如果粘性信号已被清除，跳过
            if entry.sticky and not self._is_active(entry):
                continue

            # 检查间隔 —— CRITICAL 可跳过
            elapsed = time.monotonic() - self._last_notified
            if entry.priority != Priority.CRITICAL and elapsed < self.min_interval:
                # 间隔未到 — 等待剩余时间后重新检查
                await asyncio.sleep(self.min_interval - elapsed)
                # 等待期间可能已被清除
                if entry.sticky and not self._is_active(entry):
                    continue

            # 用 handler 产消息
            msg = self.tool_registry.handle_signal(
                entry.client_id, entry.name, entry.data,
            )
            if msg is None:
                # handler 返回 None = 不需要通知
                continue

            # 发送通知
            if self._send_cb:
                try:
                    await self._send_cb(entry.client_id, msg)
                except Exception as e:
                    logger.error("SignalDispatcher send failed: %s", e)

            self._last_notified = time.monotonic()
            entry.last_notified = self._last_notified

            # 粘性信号不立即重新入队 —— 由 _requeue_stale_sticky 在 TTL 到期后处理

    # ── 辅助 ──

    def _is_active(self, entry: SignalEntry) -> bool:
        """检查粘性信号是否仍在 registry 中。"""
        client_signals = self.registry._sticky.get(entry.client_id)
        if client_signals is None:
            return False
        stored = client_signals.get(entry.name)
        return stored is not None and stored is entry

    def _requeue_stale_sticky(self):
        """将超过 TTL 未重新通知的粘性信号再次入队。notify_once 信号跳过。"""
        now = time.monotonic()
        for client_id, signals in list(self.registry._sticky.items()):
            for name, entry in list(signals.items()):
                if entry.notify_once and entry.last_notified is not None:
                    continue  # 已通知过，不再重复推送
                if entry.last_notified is None:
                    # 从未通知过 → 入队
                    self.registry._put(entry)
                    entry.last_notified = now
                elif now - entry.last_notified > self.sticky_ttl:
                    # 超过 TTL → 重新入队
                    self.registry._put(entry)
                    entry.last_notified = now
