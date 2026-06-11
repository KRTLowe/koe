import time
from dataclasses import dataclass
from typing import Dict, Optional, List, Any


@dataclass
class Connection:
    client_id: str
    websocket: Any
    connected_at: float
    last_heartbeat: float


class ConnectionManager:
    """在线客户端连接池，MCP 和 WebSocket 共享的桥梁。"""

    def __init__(self, heartbeat_timeout: float = 90.0):
        self._connections: Dict[str, Connection] = {}
        self._heartbeat_timeout = heartbeat_timeout

    def register(self, client_id: str, websocket: Any) -> None:
        now = time.time()
        self._connections[client_id] = Connection(
            client_id=client_id,
            websocket=websocket,
            connected_at=now,
            last_heartbeat=now,
        )

    def unregister(self, client_id: str) -> None:
        self._connections.pop(client_id, None)

    def get_connection(self, client_id: str) -> Optional[Any]:
        conn = self._connections.get(client_id)
        if conn is None:
            return None
        if time.time() - conn.last_heartbeat > self._heartbeat_timeout:
            self.unregister(client_id)
            return None
        return conn.websocket

    def is_online(self, client_id: str) -> bool:
        conn = self._connections.get(client_id)
        if conn is None:
            return False
        if time.time() - conn.last_heartbeat > self._heartbeat_timeout:
            self.unregister(client_id)
            return False
        return True

    def update_heartbeat(self, client_id: str) -> None:
        conn = self._connections.get(client_id)
        if conn:
            conn.last_heartbeat = time.time()

    def get_online_clients(self) -> List[str]:
        """返回当前在线客户端 ID 列表（附带过期连接清理）。"""
        now = time.time()
        expired = [
            cid
            for cid, conn in self._connections.items()
            if now - conn.last_heartbeat > self._heartbeat_timeout
        ]
        for cid in expired:
            self.unregister(cid)
        return list(self._connections.keys())
