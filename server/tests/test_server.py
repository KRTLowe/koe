import pytest
from unittest.mock import AsyncMock, MagicMock, patch

pytestmark = pytest.mark.asyncio

from kaya_server.auth import hash_passkey
from kaya_server.connection_manager import ConnectionManager
from kaya_server.db import Database
from kaya_transfer_hub.server import MCPServer
from kaya_server.ws_handler import WebSocketHandler


@pytest.fixture
def db():
    _db = Database(":memory:")
    _db.initialize()
    yield _db
    _db.close()


@pytest.fixture
def cm():
    return ConnectionManager()


@pytest.fixture
def ws_handler(db, cm):
    return WebSocketHandler(db, cm, host="127.0.0.1", port=0)


@pytest.fixture
def server(db, cm, ws_handler):
    return MCPServer(db, cm, ws_handler)


class TestListClients:
    async def test_empty(self, server):
        result = await server._handle_list_clients()
        text = result[0].text
        assert "暂无" in text

    async def test_with_clients(self, server, db):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Main PC", h)
        db.register_client("pc-02", "Laptop", h)

        result = await server._handle_list_clients()
        text = result[0].text
        assert "pc-01" in text
        assert "pc-02" in text
        assert "Main PC" in text
        assert "Laptop" in text
        # 都离线
        assert "○ 离线" in text

    async def test_with_online_client(self, server, db, cm):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Main PC", h)
        cm.register("pc-01", MagicMock())

        result = await server._handle_list_clients()
        text = result[0].text
        assert "● 在线" in text


class TestRegisterClient:
    async def test_register_new(self, server):
        result = await server._handle_register_client(
            {"client_id": "pc-01", "description": "New PC", "passkey": "mykey"}
        )
        text = result[0].text
        assert "注册成功" in text
        assert "pc-01" in text

    async def test_register_duplicate(self, server, db):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Original", h)

        result = await server._handle_register_client(
            {"client_id": "pc-01", "description": "Duplicate", "passkey": "other"}
        )
        text = result[0].text
        assert "已存在" in text


class TestRemoveClient:
    async def test_remove_exists(self, server, db):
        h = hash_passkey("secret")
        db.register_client("pc-01", "To Delete", h)

        result = await server._handle_remove_client({"client_id": "pc-01"})
        text = result[0].text
        assert "已删除" in text
        assert db.get_client("pc-01") is None

    async def test_remove_not_exists(self, server):
        result = await server._handle_remove_client({"client_id": "ghost"})
        text = result[0].text
        assert "不存在" in text


class TestSendFile:
    async def test_offline_client(self, server):
        result = await server._handle_send_file(
            {"client_id": "pc-01", "file_path": "/tmp/test.txt"}
        )
        text = result[0].text
        assert "当前离线" in text

    async def test_online_client_success(self, server, db, cm, tmp_path):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Online PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        test_file = tmp_path / "report.pdf"
        test_file.write_text("test content")

        # Mock send_file_to_client to avoid actual WebSocket I/O
        server.ws_handler.send_file_to_client = AsyncMock(return_value={
            "ok": True,
            "file_id": "f_test",
            "name": "report.pdf",
            "size": 12,
            "client_id": "pc-01",
            "ack_status": "ok",
        })

        result = await server._handle_send_file(
            {"client_id": "pc-01", "file_path": str(test_file)}
        )
        text = result[0].text
        assert "已发送" in text
        assert "report.pdf" in text

    async def test_online_client_send_failure(self, server, db, cm, tmp_path):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Online PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        server.ws_handler.send_file_to_client = AsyncMock(return_value={
            "ok": False,
            "error": "Client rejected file: checksum mismatch",
        })

        result = await server._handle_send_file(
            {"client_id": "pc-01", "file_path": str(tmp_path / "bad.txt")}
        )
        text = result[0].text
        assert "发送失败" in text
