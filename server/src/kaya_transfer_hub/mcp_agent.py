"""Standalone MCP stdio server — 通过 Unix socket 委托客户端操作给 run_and_send.py。

用法（由 OpenCode/ACP 拉起）:
    kaya-transfer-hub mcp

不绑定任何端口，仅通过 stdio 运行 MCP 协议。
所有实际客户端操作委托给 run_and_send.py 的 Unix socket (/tmp/ft-hub-cmd.sock)。
"""
import asyncio
import json
import logging
import os

from mcp.server import Server
from mcp.server.stdio import stdio_server
from mcp.types import TextContent
import mcp.types as types

from kaya_transfer_hub.tool_defs import all_tools as shared_tools
from kaya_server.constants import SOCKET_PATH

logger = logging.getLogger(__name__)

RPC_TIMEOUT = 35.0


class McpAgent:
    """通过 Unix socket 与 run_and_send.py 通信的 MCP stdio 服务器。"""

    def __init__(self):
        self.server = Server("kaya-transfer-hub")
        self._register_tools()

    # ── Unix socket RPC ─────────────────────────────

    async def _rpc(self, command: dict) -> dict:
        """向 run_and_send.py 的 Unix socket 发送命令并等待响应。"""
        reader, writer = await asyncio.wait_for(
            asyncio.open_unix_connection(SOCKET_PATH),
            timeout=5.0,
        )
        try:
            writer.write((json.dumps(command) + "\n").encode())
            await writer.drain()
            line = await asyncio.wait_for(reader.readline(), timeout=RPC_TIMEOUT)
            if not line:
                return {"ok": False, "error": "Empty response from server"}
            return json.loads(line.decode())
        except asyncio.TimeoutError:
            return {"ok": False, "error": "RPC timed out"}
        except FileNotFoundError:
            return {"ok": False, "error": f"Server socket not found ({SOCKET_PATH}). Is run_and_send.py running?"}
        except ConnectionRefusedError:
            return {"ok": False, "error": "Server socket connection refused. Is run_and_send.py running?"}
        finally:
            writer.close()

    # ── MCP 工具注册 ────────────────────────────────

    def _register_tools(self):
        @self.server.list_tools()
        async def list_tools() -> list[types.Tool]:
            return shared_tools()

        @self.server.call_tool()
        async def call_tool(name: str, arguments: dict) -> list[TextContent]:
            action_map = {
                "list_clients": "list_clients",
                "send_file": "send_file",
                "register_client": "register_client",
                "remove_client": "remove_client",
                "list_client_tools": "list_client_tools",
                "list_all_client_tools": "list_all_client_tools",
                "call_client_tool": "call_tool",
                "get_signal_status": "get_signal_status",
            }
            action = action_map.get(name)
            if action is None:
                raise ValueError(f"Unknown tool: {name}")

            cmd = {"action": action, **arguments}
            result = await self._rpc(cmd)

            if not result.get("ok"):
                error = result.get("error", "unknown error")
                return [TextContent(type="text", text=f"❌ {error}")]

            if name == "list_clients":
                clients = result.get("clients", [])
                if not clients:
                    return [TextContent(type="text", text="（暂无已注册客户端）")]
                lines = ["| 客户端 ID | 描述 | 状态 |", "|---|---|---|"]
                for c in clients:
                    status = "● 在线" if c.get("online") else "○ 离线"
                    lines.append(f"| `{c['client_id']}` | {c.get('description', '')} | {status} |")
                return [TextContent(type="text", text="\n".join(lines))]

            if name == "list_client_tools":
                tools = result.get("tools", [])
                if not tools:
                    return [TextContent(type="text", text="该客户端暂无已注册工具。")]
                lines = ["| 工具名 | 描述 | 参数 |", "|---|---|---|"]
                for t in tools:
                    params = ", ".join(t.get("inputSchema", {}).get("properties", {}).keys())
                    lines.append(f"| `{t['name']}` | {t.get('description', '')} | {params or '-'} |")
                return [TextContent(type="text", text="\n".join(lines))]

            if name == "call_client_tool":
                content = result.get("content", [])
                text_parts = [c.get("text", "") for c in content if c.get("type") == "text"]
                text = "\n".join(text_parts) or "✅ 工具执行完成（无文本输出）"
                if result.get("is_error"):
                    text = f"❌ 工具执行失败：{text}"
                return [TextContent(type="text", text=text)]

            if name == "list_all_client_tools":
                clients = result.get("clients", {})
                if not clients:
                    return [TextContent(type="text", text="暂无在线客户端注册工具。")]
                parts = []
                for cid, tools in clients.items():
                    if not tools:
                        continue
                    lines = [f"### `{cid}`", "| 工具名 | 描述 | 参数 |", "|---|---|---|"]
                    for t in tools:
                        params = ", ".join(t.get("inputSchema", {}).get("properties", {}).keys())
                        lines.append(f"| `{t['name']}` | {t.get('description', '')} | {params or '-'} |")
                    parts.append("\n".join(lines))
                return [TextContent(type="text", text="\n\n".join(parts))]

            if name == "get_signal_status":
                active = result.get("active", False)
                return [TextContent(type="text",
                    text=f"signal `{arguments.get('signal_name', '?')}` for client `{arguments.get('client_id', '?')}`: {'active' if active else 'inactive'}")]

            # send_file / register_client / remove_client
            return [TextContent(type="text", text=json.dumps(result, ensure_ascii=False))]

    async def run_stdio(self):
        async with stdio_server() as (read_stream, write_stream):
            await self.server.run(
                read_stream, write_stream,
                self.server.create_initialization_options(),
            )
