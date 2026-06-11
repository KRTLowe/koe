"""SignalRegistry + SignalDispatcher 单元测试。"""
import asyncio
import pytest
from kaya_server.signal_registry import (
    SignalRegistry, SignalDispatcher, Priority,
)


class TestSignalRegistry:
    def test_push_ephemeral(self):
        reg = SignalRegistry()
        entry = reg.push("pc-01", "clipboard_changed", {"timestamp": "now"}, sticky=False)
        assert not entry.sticky
        assert reg._queue.qsize() == 1

    def test_push_sticky(self):
        reg = SignalRegistry()
        entry = reg.push("pc-01", "visual_input_available", {"source": "screenshot"}, sticky=True)
        assert entry.sticky
        assert "pc-01" in reg._sticky
        assert "visual_input_available" in reg._sticky["pc-01"]

    def test_sticky_replace(self):
        reg = SignalRegistry()
        e1 = reg.push("pc-01", "visual", {"version": 1}, sticky=True)
        e2 = reg.push("pc-01", "visual", {"version": 2}, sticky=True)
        # 同名的粘性信号，后者替换前者
        assert reg._sticky["pc-01"]["visual"] is e2
        assert reg._sticky["pc-01"]["visual"] is not e1

    def test_clear_exists(self):
        reg = SignalRegistry()
        reg.push("pc-01", "visual", {}, sticky=True)
        assert reg.clear("pc-01", "visual") is True
        assert "pc-01" not in reg._sticky

    def test_clear_nonexistent(self):
        reg = SignalRegistry()
        assert reg.clear("pc-01", "nonexistent") is False

    def test_get_active(self):
        reg = SignalRegistry()
        reg.push("pc-01", "visual", {}, sticky=True)
        reg.push("pc-01", "clipboard", {}, sticky=True)
        active = reg.get_active("pc-01")
        assert len(active) == 2

    def test_get_active_empty(self):
        reg = SignalRegistry()
        assert reg.get_active("pc-01") == []
        assert reg.get_active("nonexistent") == []

    def test_clear_client(self):
        reg = SignalRegistry()
        reg.push("pc-01", "visual", {}, sticky=True)
        reg.push("pc-02", "visual", {}, sticky=True)
        reg.clear_client("pc-01")
        assert "pc-01" not in reg._sticky
        assert "pc-02" in reg._sticky

    def test_has_pending(self):
        reg = SignalRegistry()
        assert not reg.has_pending("pc-01")
        reg.push("pc-01", "visual", {}, sticky=True)
        assert reg.has_pending("pc-01")
        reg.clear("pc-01", "visual")
        assert not reg.has_pending("pc-01")

    def test_priority_ordering(self):
        """高优先级先出队。"""
        reg = SignalRegistry()
        reg.push("pc-01", "low", {}, sticky=False, priority=Priority.LOW)
        reg.push("pc-01", "critical", {}, sticky=False, priority=Priority.CRITICAL)
        reg.push("pc-01", "high", {}, sticky=False, priority=Priority.HIGH)

        # 取出所有条目
        items = []
        while not reg._queue.empty():
            items.append(reg._queue.get_nowait())

        # 按优先级排序：CRITICAL(0) < HIGH(1) < LOW(3)
        names = [i[2].name for i in items]
        assert names == ["critical", "high", "low"], f"Got {names}"

    def test_priority_parse(self):
        assert Priority.from_str("high") == Priority.HIGH
        assert Priority.from_str("CRITICAL") == Priority.CRITICAL
        assert Priority.from_str("unknown") == Priority.NORMAL


class TestSignalDispatcher:
    @pytest.mark.asyncio
    async def test_dispatcher_sends_notification(self):
        """调度器应该调用 send_callback 发送 acp_inject。"""
        reg = SignalRegistry()
        tool_registry = _MockToolRegistry("系统消息")
        dispatcher = SignalDispatcher(reg, tool_registry, min_interval=0.1)

        sent = []

        async def fake_send(client_id, text):
            sent.append((client_id, text))

        dispatcher.set_send_callback(fake_send)
        await dispatcher.start()

        # 入队一个一次性信号
        reg.push("pc-01", "test", {"key": "val"}, sticky=False)
        await asyncio.sleep(0.3)

        await dispatcher.stop()
        assert len(sent) >= 1
        assert sent[0][0] == "pc-01"
        assert "系统消息" in sent[0][1]

    @pytest.mark.asyncio
    async def test_dispatcher_respects_interval(self):
        """调度器不应该在间隔期内发送第二条消息。"""
        reg = SignalRegistry()
        tool_registry = _MockToolRegistry("msg")
        dispatcher = SignalDispatcher(reg, tool_registry, min_interval=0.5)

        sent_times = []

        async def fake_send(client_id, text):
            sent_times.append(asyncio.get_event_loop().time())

        dispatcher.set_send_callback(fake_send)
        await dispatcher.start()

        reg.push("pc-01", "a", {}, sticky=False)
        reg.push("pc-01", "b", {}, sticky=False)
        await asyncio.sleep(0.15)

        await dispatcher.stop()

        # 第一条应该发了，第二条受间隔控制
        assert len(sent_times) == 1

    @pytest.mark.asyncio
    async def test_critical_skips_interval(self):
        """CRITICAL 优先级跳过间隔限制。"""
        reg = SignalRegistry()
        tool_registry = _MockToolRegistry("msg")
        dispatcher = SignalDispatcher(reg, tool_registry, min_interval=9999)  # 很长的间隔

        sent_count = 0

        async def fake_send(client_id, text):
            nonlocal sent_count
            sent_count += 1

        dispatcher.set_send_callback(fake_send)
        await dispatcher.start()

        # 先发一个 NORMAL，被间隔挡住
        reg.push("pc-01", "normal", {}, sticky=False)
        await asyncio.sleep(0.1)

        # 再发一个 CRITICAL，应该跳过间隔
        reg.push("pc-01", "critical", {}, sticky=False, priority=Priority.CRITICAL)
        await asyncio.sleep(0.2)

        await dispatcher.stop()
        assert sent_count == 2

    @pytest.mark.asyncio
    async def test_sticky_signal_requeued(self):
        """粘性信号处理后应该重新入队。"""
        reg = SignalRegistry()
        tool_registry = _MockToolRegistry("msg")
        dispatcher = SignalDispatcher(reg, tool_registry, min_interval=0.1, sticky_ttl=0.3)

        call_count = 0

        async def fake_send(client_id, text):
            nonlocal call_count
            call_count += 1

        dispatcher.set_send_callback(fake_send)
        await dispatcher.start()

        reg.push("pc-01", "sticky_test", {}, sticky=True)

        # 等足够时间让 TTL 触发重新入队
        await asyncio.sleep(1.0)

        await dispatcher.stop()
        # 应该至少通知了 2 次（初始 + TTL 后重新通知）
        assert call_count >= 2, f"Expected >=2, got {call_count}"

    @pytest.mark.asyncio
    async def test_clear_sticky_stops_requeue(self):
        """清除粘性信号后不应再重新入队。"""
        reg = SignalRegistry()
        tool_registry = _MockToolRegistry("msg")
        dispatcher = SignalDispatcher(reg, tool_registry, min_interval=0.1, sticky_ttl=0.2)

        call_count = 0

        async def fake_send(client_id, text):
            nonlocal call_count
            call_count += 1

        dispatcher.set_send_callback(fake_send)
        await dispatcher.start()

        reg.push("pc-01", "sticky_test", {}, sticky=True)
        await asyncio.sleep(0.15)

        # 清除信号
        reg.clear("pc-01", "sticky_test")
        await asyncio.sleep(0.5)

        await dispatcher.stop()
        # 清除后不应再通知
        assert call_count == 1, f"Expected 1, got {call_count}"

    @pytest.mark.asyncio
    async def test_handler_returns_none_skips_notification(self):
        """handler 返回 None 时不发送通知。"""
        reg = SignalRegistry()
        tool_registry = _MockToolRegistry(None)  # handler 返回 None
        dispatcher = SignalDispatcher(reg, tool_registry, min_interval=0.1)

        sent = []

        async def fake_send(client_id, text):
            sent.append(text)

        dispatcher.set_send_callback(fake_send)
        await dispatcher.start()

        reg.push("pc-01", "silent", {}, sticky=False)
        await asyncio.sleep(0.3)

        await dispatcher.stop()
        assert len(sent) == 0


class _MockToolRegistry:
    """模拟 ToolRegistry，返回固定消息。"""

    def __init__(self, message: str | None):
        self._message = message

    def handle_signal(self, client_id, name, data):
        return self._message


def test_priority_enum_values():
    """CRITICAL 必须是最小值（最高优先级）。"""
    assert Priority.CRITICAL < Priority.HIGH < Priority.NORMAL < Priority.LOW
