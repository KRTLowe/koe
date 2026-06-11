"""ACP 桥接：Python 原生实现。保持 opencode acp 子进程常驻。"""
import asyncio
import json
import logging
import os
import uuid

logger = logging.getLogger("acp_bridge")
OPENCODE_BIN = os.environ.get("OPENCODE_BIN", "/root/.opencode/bin/opencode")
_reconnect_buffers: dict[str, list[str]] = {}


async def start_acp_bridge(port: int = 8765) -> None:
    cmd = f"{OPENCODE_BIN} acp"
    logger.info("ACP bridge: spawning %s", cmd)

    proc = await asyncio.create_subprocess_exec(
        *cmd.split(),
        stdin=asyncio.subprocess.PIPE,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    logger.info("ACP bridge: opencode pid=%d", proc.pid)

    async def _log_stderr():
        while True:
            line = await proc.stderr.readline()
            if not line: break
            logger.debug("acp: %s", line.decode(errors="replace").rstrip())
    asyncio.create_task(_log_stderr())

    async def _restart_on_exit():
        code = await proc.wait()
        logger.warning("ACP bridge: opencode exited (%d), restarting...", code)
        await asyncio.sleep(2)
        asyncio.create_task(start_acp_bridge(port))
    asyncio.create_task(_restart_on_exit())

    stdin_lock = asyncio.Lock()

    async def handle_client(ws):
        cid = uuid.uuid4().hex[:8]
        logger.info("ACP client connected: %s", cid)
        await ws.send(json.dumps({"type": "connected", "clientId": cid}))

        buf = _reconnect_buffers.setdefault(cid, [])
        for msg in buf:
            try: await ws.send(msg.rstrip())
            except Exception: break
        buf.clear()

        async def _fwd():
            try:
                while True:
                    line = await proc.stdout.readline()
                    if not line: break
                    text = line.decode(errors="replace").rstrip()
                    if not text: continue
                    for b in _reconnect_buffers.values():
                        b.append(text)
                        if len(b) > 200: b.pop(0)
                    try: await ws.send(text)
                    except Exception: break
            except Exception: pass

        fwd = asyncio.create_task(_fwd())
        try:
            async for msg in ws:
                if isinstance(msg, str):
                    async with stdin_lock:
                        proc.stdin.write((msg + "\n").encode())
                        await proc.stdin.drain()
        except Exception: pass
        finally:
            fwd.cancel()
            logger.info("ACP client disconnected: %s", cid)

    import websockets
    server = await websockets.serve(
        handle_client, "0.0.0.0", port,
        ping_interval=20, ping_timeout=10,
        max_size=10 * 1024 * 1024,
    )
    logger.info("ACP bridge listening on ws://0.0.0.0:%d", port)
    await server.wait_closed()
