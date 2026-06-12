#!/usr/bin/env python3
"""WS 服务端 + Unix socket 命令接口。
文件传输 WebSocket 服务，附带孤儿 ACP 进程清理。"""
import asyncio
import json
import sys
import os
import signal
import logging

sys.path.insert(0, os.path.dirname(__file__))
os.chdir(os.path.dirname(__file__))

from kaya_server.db import Database
from kaya_server.connection_manager import ConnectionManager
from kaya_server.ws_handler import WebSocketHandler
from kaya_transfer_hub.tool_registry import ToolRegistry
from kaya_server.signal_registry import SignalRegistry
from kaya_server.signal_handlers import register_all as register_signal_handlers
from kaya_server.constants import SOCKET_PATH
from kaya_server.acp_bridge import start_acp_bridge

WS_PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 9765
ACP_PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8765

logger = logging.getLogger("run_and_send")


def set_command_socket_permissions(socket_path: str) -> None:
    os.chmod(socket_path, 0o600)


# ── 孤儿 ACP 进程清理（手动） ─────────────────────────
# cleanup_orphans 保留作手动应急用。可通过 Unix socket 发送 cleanup_acp 命令触发。

def find_all_acp_pids() -> list[int]:
    """找到所有 opencode acp 进程 PID。"""
    try:
        import subprocess
        result = subprocess.run(
            ["pgrep", "-f", r"/root/\.opencode/bin/opencode acp"],
            capture_output=True, text=True, timeout=5,
        )
        if result.returncode == 0 and result.stdout.strip():
            return [int(p) for p in result.stdout.strip().split()]
    except Exception:
        pass
    return []


def cleanup_orphans():
    """手动清理孤儿 opencode acp 进程。"""
    from datetime import datetime, timezone
    pids = find_all_acp_pids()
    if not pids:
        return

    now = datetime.now(timezone.utc)
    with_times = []
    for pid in pids:
        try:
            start_ts = os.path.getctime(f"/proc/{pid}")
            start_time = datetime.fromtimestamp(start_ts, tz=timezone.utc)
            age_hours = (now - start_time).total_seconds() / 3600
            with_times.append((start_time, pid, age_hours))
        except (FileNotFoundError, ProcessLookupError, OSError):
            continue

    if len(with_times) <= 2:
        return

    with_times.sort(key=lambda x: x[0], reverse=True)
    keep = 2
    for start_time, pid, age_hours in with_times[keep:]:
        if age_hours < 0.1:
            continue
        logger.warning(f"Killing orphan ACP process (age={age_hours:.1f}h): {pid}")
        try:
            os.kill(pid, signal.SIGKILL)
        except ProcessLookupError:
            pass


# ── 主程序 ──────────────────────────────────────────

# 模块级变量，handle_cmd 闭包访问
signal_registry = None

async def handle_cmd(reader, writer):
    """Unix socket 命令处理。"""
    try:
        line = await reader.readline()
        if not line:
            writer.close()
            return
        cmd = json.loads(line.decode())
        action = cmd.get("action", cmd.get("type", ""))

        if action == "send_file":
            result = await ws.send_file_to_client(cmd["client_id"], cmd["file_path"])
        elif action == "cleanup_acp":
            cleanup_orphans()
            result = {"ok": True, "message": "ACP orphan cleanup triggered"}
        elif action == "stats":
            acp_pids = find_all_acp_pids()
            online = list(cm.get_online_clients()) if cm else []
            result = {
                "ok": True,
                "ws_clients": online,
                "acp_processes": len(acp_pids),
            }
        elif action == "list_clients":
            clients = db.list_clients()
            online = set(cm.get_online_clients()) if cm else set()
            result = {
                "ok": True,
                "clients": [
                    {
                        "client_id": c.client_id,
                        "description": c.description,
                        "online": c.client_id in online,
                    }
                    for c in clients
                ],
            }
        elif action == "list_client_tools":
            client_id = cmd.get("client_id", "")
            tools = tool_registry.get_tools(client_id) if tool_registry else []
            result = {"ok": True, "tools": tools}
        elif action == "list_all_client_tools":
            online_ids = list(cm.get_online_clients()) if cm else []
            all_tools = {}
            for cid in online_ids:
                tools = tool_registry.get_tools(cid) if tool_registry else []
                if tools:
                    all_tools[cid] = tools
            result = {"ok": True, "clients": all_tools}
        elif action == "call_tool":
            client_id = cmd.get("client_id", "")
            tool_name = cmd.get("tool_name", "")
            arguments = cmd.get("arguments", {})
            logger.info("MCP call_tool: client=%s tool=%s args=%s", client_id, tool_name, arguments)
            if not cm.is_online(client_id):
                result = {"ok": False, "error": f"Client {client_id} is offline"}
                logger.warning("call_tool failed: client %s offline", client_id)
            elif not tool_registry.has_tools(client_id):
                result = {"ok": False, "error": f"Client {client_id} has no registered tools"}
                logger.warning("call_tool failed: client %s has no tools", client_id)
            else:
                request_id = tool_registry.create_invoke(client_id, tool_name, arguments)
                logger.info("call_tool request_id=%s sent to %s", request_id, client_id)
                sent = await ws.send_tool_call(client_id, request_id, tool_name, arguments)
                if not sent:
                    tool_registry.resolve_invoke(request_id, {
                        "ok": False, "error": "Failed to send tool call",
                    })
                    result = {"ok": False, "error": "Failed to send tool call to client"}
                    logger.error("call_tool failed: send_tool_call to %s returned False", client_id)
                else:
                    invoke_result = await tool_registry.wait_invoke(request_id)
                    result = {"ok": True, "content": invoke_result.get("content", []), "is_error": invoke_result.get("is_error", False)}
                    logger.info("call_tool result: client=%s tool=%s ok=%s", client_id, tool_name, not result.get("is_error", False))
        elif action == "register_client":
            from kaya_server.auth import hash_passkey
            cid = cmd["client_id"]
            if db.client_exists(cid):
                result = {"ok": False, "error": f"Client {cid} already exists"}
            else:
                desc = cmd.get("description", "")
                passkey_hash = hash_passkey(cmd["passkey"])
                db.register_client(cid, desc, passkey_hash)
                result = {"ok": True, "client_id": cid}
        elif action == "remove_client":
            cid = cmd["client_id"]
            if db.remove_client(cid):
                result = {"ok": True, "client_id": cid}
            else:
                result = {"ok": False, "error": f"Client {cid} not found"}
        elif action == "get_signal_status":
            client_id = cmd.get("client_id", "")
            signal_name = cmd.get("signal_name", "")
            active_signals = signal_registry.get_active(client_id) if signal_registry else []
            active = any(s.name == signal_name for s in active_signals)
            result = {"ok": True, "active": active}
        else:
            result = {"ok": False, "error": f"Unknown action: {action}"}

        writer.write((json.dumps(result) + "\n").encode())
        await writer.drain()
    except Exception as e:
        writer.write((json.dumps({"ok": False, "error": str(e)}) + "\n").encode())
        await writer.drain()
    finally:
        writer.close()


async def main():
    global ws, cm, db, tool_registry

    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
        handlers=[
            logging.StreamHandler(sys.stderr),
            logging.FileHandler("/tmp/kaya-beam-server.log", mode="a"),
        ],
    )
    logging.getLogger("websockets").setLevel(logging.INFO)
    logging.getLogger("kaya_transfer_hub").setLevel(logging.INFO)

    db = Database(None)
    db.initialize()
    cm = ConnectionManager()
    tool_registry = ToolRegistry()
    global signal_registry
    signal_registry = SignalRegistry()

    register_signal_handlers(tool_registry)

    ws = WebSocketHandler(db, cm, host="0.0.0.0", port=WS_PORT, tool_registry=tool_registry, signal_registry=signal_registry)
    await ws.start()
    print(f"WS server on ws://0.0.0.0:{WS_PORT}", flush=True)

    # 启动 ACP 桥接（替代 @rebornix/stdio-to-ws）
    asyncio.create_task(start_acp_bridge(port=ACP_PORT))
    print(f"ACP bridge on ws://0.0.0.0:{ACP_PORT}", flush=True)

    if os.path.exists(SOCKET_PATH):
        os.unlink(SOCKET_PATH)
    server = await asyncio.start_unix_server(handle_cmd, path=SOCKET_PATH)
    set_command_socket_permissions(SOCKET_PATH)
    print(f"Cmd socket on {SOCKET_PATH}", flush=True)

    try:
        await asyncio.Future()
    except asyncio.CancelledError:
        pass
    finally:
        await ws.stop()
        db.close()


if __name__ == "__main__":
    asyncio.run(main())
