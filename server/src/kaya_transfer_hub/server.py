import logging

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import TextContent
import mcp.types as types

from kaya_server.db import Database
from kaya_server.auth import hash_passkey
from kaya_server.connection_manager import ConnectionManager
from kaya_server.ws_handler import WebSocketHandler
from kaya_transfer_hub.tool_defs import all_tools as shared_tools

logger = logging.getLogger(__name__)


class MCPServer:
    """MCP 工具注册 + 服务生命周期管理。"""

    def __init__(self, db: Database, cm: ConnectionManager, ws_handler: WebSocketHandler):
        self.db = db
        self.cm = cm
        self.ws_handler = ws_handler
        self.server = Server("kaya-transfer-hub")
        self._register_tools()

    def _register_tools(self):
        @self.server.list_tools()
        async def list_tools() -> list[types.Tool]:
            return shared_tools()

        @self.server.call_tool()
        async def call_tool(name: str, arguments: dict) -> list[types.TextContent]:
            if name == "list_clients":
                return await self._handle_list_clients()
            elif name == "send_file":
                return await self._handle_send_file(arguments)
            elif name == "remove_client":
                return await self._handle_remove_client(arguments)
            elif name == "register_client":
                return await self._handle_register_client(arguments)
            elif name == "list_client_tools":
                return await self._handle_list_client_tools(arguments)
            elif name == "list_all_client_tools":
                return await self._handle_list_all_client_tools()
            elif name == "call_client_tool":
                return await self._handle_call_client_tool(arguments)
            elif name == "get_signal_status":
                return await self._handle_get_signal_status(arguments)
            else:
                raise ValueError(f"Unknown tool: {name}")

    def _format_clients_table(self) -> str:
        clients = self.db.list_clients()
        online_set = set(self.cm.get_online_clients())
        if not clients:
            return "（暂无已注册客户端）"
        lines = ["| 客户端 ID | 描述 | 状态 |", "|---|---|---|"]
        for c in clients:
            status = "● 在线" if c.client_id in online_set else "○ 离线"
            lines.append(f"| `{c.client_id}` | {c.description} | {status} |")
        return "\n".join(lines)

    async def _handle_list_clients(self) -> list[TextContent]:
        return [TextContent(type="text", text=self._format_clients_table())]

    async def _handle_send_file(self, args: dict) -> list[TextContent]:
        client_id = args["client_id"]
        file_path = args["file_path"]

        if not self.cm.is_online(client_id):
            return [TextContent(
                type="text",
                text=f"客户端 `{client_id}` 当前离线，无法发送文件。请先确认客户端在线。",
            )]

        result = await self.ws_handler.send_file_to_client(client_id, file_path)

        if result.get("ok"):
            return [TextContent(
                type="text",
                text=f"✅ 文件已发送：`{result['name']}` ({result['size']} bytes) → `{client_id}`",
            )]
        else:
            return [TextContent(
                type="text",
                text=f"❌ 发送失败：{result.get('error', '未知错误')}",
            )]

    async def _handle_remove_client(self, args: dict) -> list[TextContent]:
        client_id = args["client_id"]
        if not self.db.client_exists(client_id):
            return [TextContent(
                type="text",
                text=f"客户端 `{client_id}` 不存在。",
            )]
        self.db.remove_client(client_id)
        return [TextContent(
            type="text",
            text=f"✅ 客户端 `{client_id}` 已删除。",
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

    async def _handle_list_all_client_tools(self) -> list[TextContent]:
        online_ids = self.cm.get_online_clients()
        if not online_ids:
            return [TextContent(type="text", text="暂无在线客户端。")]
        parts = []
        for cid in online_ids:
            parts.append(f"### `{cid}`\n{self._format_tools_table(cid)}")
        return [TextContent(type="text", text="\n\n".join(parts))]

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
            self.ws_handler.tool_registry.resolve_invoke(request_id, {
                "ok": False, "error": "Failed to send tool call to client",
            })
            return [TextContent(
                type="text",
                text=f"❌ 向客户端 `{client_id}` 发送工具调用失败。",
            )]

        result = await self.ws_handler.tool_registry.wait_invoke(request_id)

        if result.get("ok") is False:
            return [TextContent(
                type="text",
                text=f"❌ 工具调用失败：{result.get('error', '未知错误')}",
            )]

        content = result.get("content", [])
        text_parts = [c.get("text", "") for c in content if c.get("type") == "text"]
        return [TextContent(type="text", text="\n".join(text_parts) or "✅ 工具执行完成（无文本输出）")]

    async def _handle_get_signal_status(self, args: dict) -> list[TextContent]:
        client_id = args["client_id"]
        signal_name = args["signal_name"]
        active_signals = self.ws_handler.signal_registry.get_active(client_id)
        active = any(s.name == signal_name for s in active_signals)
        return [TextContent(type="text", text=f"signal `{signal_name}` for client `{client_id}`: {'active' if active else 'inactive'}")]

    async def run_stdio(self):
        async with stdio_server() as (read_stream, write_stream):
            await self.server.run(
                read_stream, write_stream,
                self.server.create_initialization_options()
            )
