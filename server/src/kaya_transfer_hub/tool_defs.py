"""共享 MCP 工具定义。

MCPServer（serve 命令）和 McpAgent（mcp 命令）共用这一套定义，
确保工具 schema 不会 drift。
"""
from mcp.types import Tool


def all_tools() -> list[Tool]:
    return [
        Tool(
            name="list_clients",
            description="列出所有已注册的客户端及其在线状态。返回 client_id、description、online 字段。",
            inputSchema={"type": "object", "properties": {}},
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
        # ⚠️ 风险说明：register_client 和 remove_client 通过 MCP 暴露意味着 LLM 可以
        # 管理客户端注册表。在个人自托管场景下可接受，如果未来放开 MCP 访问权限，
        # 建议将这两个工具标记为 admin-only 或从 MCP 工具列表中移除。
        Tool(
            name="remove_client",
            description="删除一个已注册的客户端。需要指定 client_id。",
            inputSchema={
                "type": "object",
                "properties": {
                    "client_id": {
                        "type": "string",
                        "description": "要删除的客户端 ID",
                    },
                },
                "required": ["client_id"],
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
        Tool(
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
        Tool(
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
        Tool(
            name="list_all_client_tools",
            description="列出所有在线客户端及其注册的本地工具。返回每个客户端的工具列表。",
            inputSchema={"type": "object", "properties": {}},
        ),
        Tool(
            name="get_signal_status",
            description="查询指定客户端的信号状态。用于 Kaya 在持续监测循环中判断是否继续。返回 active 字段表示信号是否仍在活跃。",
            inputSchema={
                "type": "object",
                "properties": {
                    "client_id": {
                        "type": "string",
                        "description": "客户端 ID",
                    },
                    "signal_name": {
                        "type": "string",
                        "description": "信号名称（如 copilot_query）",
                    },
                },
                "required": ["client_id", "signal_name"],
            },
        ),
    ]
