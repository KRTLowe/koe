"""ToolRegistry 单元测试。"""
import asyncio
import pytest
from kaya_transfer_hub.tool_registry import ToolRegistry


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
        assert "timed out" in got["error"]

    def test_signal_handler(self, registry):
        calls = []

        def handler(cid, data):
            calls.append((cid, data))
            return "system message"

        registry.register_signal_handler("test_signal", handler)
        result = registry.handle_signal("pc-01", "test_signal", {"key": "val"})
        assert result == "system message"
        assert len(calls) == 1
