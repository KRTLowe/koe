import os


# Unix socket — MCP agent (mcp 命令) 与 run_and_send.py 之间的 RPC 通道
SOCKET_PATH: str = os.environ.get(
    "FT_HUB_SOCKET_PATH",
    "/tmp/ft-hub-cmd.sock",
)


