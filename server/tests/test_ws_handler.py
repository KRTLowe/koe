import asyncio
import json
from pathlib import Path
from unittest.mock import AsyncMock, patch

import pytest

pytestmark = pytest.mark.asyncio

from kaya_server.auth import hash_passkey
from kaya_server.connection_manager import ConnectionManager
from kaya_server.db import Database
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
def handler(db, cm):
    return WebSocketHandler(db, cm, host="127.0.0.1", port=0)


class TestAuthenticate:
    async def test_success(self, handler, db):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        assert await handler._authenticate("pc-01", "secret") is True

    async def test_wrong_passkey(self, handler, db):
        h = hash_passkey("correct")
        db.register_client("pc-01", "Test PC", h)
        assert await handler._authenticate("pc-01", "wrong") is False

    async def test_nonexistent_client(self, handler):
        assert await handler._authenticate("ghost", "any") is False

    async def test_empty_credentials(self, handler):
        assert await handler._authenticate("", "") is False
        assert await handler._authenticate("pc-01", "") is False
        assert await handler._authenticate("", "pass") is False


class TestSendFile:
    async def test_client_offline(self, handler):
        result = await handler.send_file_to_client("pc-01", "/tmp/test.txt")
        assert result["ok"] is False
        assert "offline" in result["error"]

    async def test_relative_path_rejected(self, handler, db, cm):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        result = await handler.send_file_to_client("pc-01", "relative/path.txt")
        assert result["ok"] is False
        assert "absolute" in result["error"]

    async def test_file_not_found(self, handler, db, cm):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        result = await handler.send_file_to_client("pc-01", "/nonexistent/file.txt")
        assert result["ok"] is False
        assert "not found" in result["error"]

    async def test_file_too_large(self, handler, db, cm, tmp_path):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        big_file = tmp_path / "big.bin"
        big_file.write_bytes(b"x" * (500 * 1024 * 1024 + 1))

        result = await handler.send_file_to_client("pc-01", str(big_file))
        assert result["ok"] is False
        assert "too large" in result["error"]

    async def test_send_success(self, handler, db, cm, tmp_path):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        test_file = tmp_path / "hello.txt"
        test_file.write_text("Hello, world!")

        async def mock_ack(file_id, timeout=30):
            return {"status": "ok"}

        with patch.object(handler, "_wait_for_ack", mock_ack):
            result = await handler.send_file_to_client("pc-01", str(test_file))

        assert result["ok"] is True
        assert result["name"] == "hello.txt"
        assert result["size"] == 13
        assert result["client_id"] == "pc-01"
        assert result["ack_status"] == "ok"

        # 验证发送了三帧
        assert mock_ws.send.await_count >= 3
        # 第一帧：file_meta（文本）
        meta_call = mock_ws.send.await_args_list[0]
        meta = json.loads(meta_call[0][0])
        assert meta["type"] == "file_meta"
        assert meta["name"] == "hello.txt"
        # 第二帧：二进制数据
        bin_call = mock_ws.send.await_args_list[1]
        assert isinstance(bin_call[0][0], bytes)
        # 第三帧：file_end
        end_call = mock_ws.send.await_args_list[2]
        end = json.loads(end_call[0][0])
        assert end["type"] == "file_end"
        assert end["checksum"].startswith("sha256:")

    async def test_send_rejected_by_client(self, handler, db, cm, tmp_path):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        test_file = tmp_path / "rejected.txt"
        test_file.write_text("data")

        async def mock_ack(file_id, timeout=30):
            return {"status": "error", "error": "checksum mismatch"}

        with patch.object(handler, "_wait_for_ack", mock_ack):
            result = await handler.send_file_to_client("pc-01", str(test_file))

        assert result["ok"] is False
        assert "rejected" in result["error"]

    async def test_ack_timeout(self, handler, db, cm, tmp_path):
        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)

        test_file = tmp_path / "timeout.txt"
        test_file.write_text("data")

        async def mock_ack(file_id, timeout=30):
            return {"status": "timeout", "error": "Client ack timeout"}

        with patch.object(handler, "_wait_for_ack", mock_ack):
            result = await handler.send_file_to_client("pc-01", str(test_file))

        # 超时不判定为失败——文件可能已经到达
        assert result["ok"] is True
        assert result["ack_status"] == "timeout"


class TestAckFuture:
    """测试 _wait_for_ack 与 _handle_client 的 ack 唤醒交互。"""

    async def test_ack_resolves_future(self, handler):
        file_id = "f_test_001"

        async def resolve_ack():
            future = handler._pending_acks[file_id]
            future.set_result({"status": "ok", "error": None})

        # 创建 Task 分别运行 ack 等待和 ack 解析
        wait_task = asyncio.create_task(handler._wait_for_ack(file_id, timeout=5))
        # 让 wait_task 先开始注册 future
        await asyncio.sleep(0.01)
        resolve_task = asyncio.create_task(resolve_ack())

        result = await wait_task
        resolve_task.cancel()

        assert result["status"] == "ok"
        assert file_id not in handler._pending_acks
