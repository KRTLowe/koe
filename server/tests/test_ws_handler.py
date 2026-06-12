import asyncio
import contextlib
import json
import os
from pathlib import Path
from unittest.mock import AsyncMock, patch

import pytest

pytestmark = pytest.mark.asyncio

from kaya_server.auth import hash_passkey
from kaya_server.connection_manager import ConnectionManager
from kaya_server.db import Database
from kaya_server.ws_handler import WebSocketHandler


class FakeWebSocket:
    def __init__(self, messages, *, stay_open=False):
        self._messages = iter(messages)
        self._stay_open = stay_open
        self.sent = []

    def __aiter__(self):
        return self

    async def __anext__(self):
        try:
            return next(self._messages)
        except StopIteration:
            if self._stay_open:
                await asyncio.Future()
            raise StopAsyncIteration

    async def send(self, message):
        self.sent.append(message)


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

    async def test_second_auth_message_does_not_unregister_authenticated_client(
        self, handler, db, cm,
    ):
        passkey_hash = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", passkey_hash)
        websocket = FakeWebSocket(
            [
                json.dumps({"type": "auth", "client_id": "pc-01", "passkey": "secret"}),
                json.dumps({"type": "auth", "client_id": "pc-01", "passkey": "wrong"}),
                json.dumps({"type": "heartbeat"}),
            ],
            stay_open=True,
        )

        task = asyncio.create_task(handler._handle_client(websocket))
        await asyncio.sleep(0.01)

        assert cm.is_online("pc-01") is True
        auth_results = [
            json.loads(message)
            for message in websocket.sent
            if json.loads(message).get("type") == "auth_result"
        ]
        assert auth_results == [{"type": "auth_result", "ok": True}]
        task.cancel()
        with contextlib.suppress(asyncio.CancelledError):
            await task


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

    async def test_send_file_streams_binary_frames_in_chunks(
        self, handler, db, cm, tmp_path,
    ):
        import kaya_server.ws_handler as ws_handler

        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)
        test_file = tmp_path / "chunked.bin"
        test_file.write_bytes(b"a" * (ws_handler.FILE_CHUNK_SIZE + 3))

        async def mock_ack(file_id, timeout=30):
            return {"status": "ok"}

        with patch.object(handler, "_wait_for_ack", mock_ack):
            result = await handler.send_file_to_client("pc-01", str(test_file))

        binary_frames = [
            call.args[0]
            for call in mock_ws.send.await_args_list
            if isinstance(call.args[0], bytes)
        ]
        assert result["ok"] is True
        assert [len(frame) for frame in binary_frames] == [ws_handler.FILE_CHUNK_SIZE, 3]

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


class TestUploadStateStreaming:
    def test_upload_state_streams_chunks_to_temp_file(self, tmp_path):
        import kaya_server.ws_handler as ws_handler

        final_path = tmp_path / "final.txt"
        temp_path = tmp_path / "final.txt.part"
        state = ws_handler.UploadState(
            file_id="up_1",
            name="final.txt",
            size=8,
            temp_path=temp_path,
            final_path=final_path,
        )

        assert state.append(b"abc") is True
        assert state.append(b"defgh") is True
        saved_path = state.finalize()

        assert saved_path == final_path
        assert final_path.read_bytes() == b"abcdefgh"
        assert not temp_path.exists()
        assert state.bytes_received == 8


class TestFileUploadSecurity:
    async def test_upload_streams_to_temp_file_and_returns_final_path(
        self, handler, db, cm, tmp_path,
    ):
        import kaya_server.ws_handler as ws_handler

        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        websocket = FakeWebSocket(
            [
                json.dumps({"type": "auth", "client_id": "pc-01", "passkey": "secret"}),
                json.dumps({
                    "type": "file_upload_start",
                    "file_id": "up_1",
                    "name": "stream.txt",
                    "size": 8,
                }),
                b"abc",
                b"defgh",
                json.dumps({"type": "file_upload_end", "file_id": "up_1"}),
            ]
        )

        with patch.object(ws_handler, "UPLOAD_DIR", str(tmp_path)):
            await handler._handle_client(websocket)

        result = [
            json.loads(msg)
            for msg in websocket.sent
            if json.loads(msg).get("type") == "file_upload_result"
        ][-1]
        saved_path = Path(result["path"])

        assert result["ok"] is True
        assert result["size"] == 8
        assert saved_path.read_bytes() == b"abcdefgh"
        assert not list(tmp_path.rglob("*.part"))

    async def test_upload_size_mismatch_aborts_temp_file(
        self, handler, db, cm, tmp_path,
    ):
        import kaya_server.ws_handler as ws_handler

        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        websocket = FakeWebSocket(
            [
                json.dumps({"type": "auth", "client_id": "pc-01", "passkey": "secret"}),
                json.dumps({
                    "type": "file_upload_start",
                    "file_id": "up_1",
                    "name": "short.txt",
                    "size": 8,
                }),
                b"abc",
                json.dumps({"type": "file_upload_end", "file_id": "up_1"}),
            ]
        )

        with patch.object(ws_handler, "UPLOAD_DIR", str(tmp_path)):
            await handler._handle_client(websocket)

        result = [
            json.loads(msg)
            for msg in websocket.sent
            if json.loads(msg).get("type") == "file_upload_result"
        ][-1]
        assert result["ok"] is False
        assert "size mismatch" in result["error"].lower()
        assert not list(tmp_path.rglob("*.part"))
        assert not list(tmp_path.rglob("short.txt"))

    async def test_upload_filename_is_sanitized_before_saving(
        self, handler, db, cm, tmp_path,
    ):
        import kaya_server.ws_handler as ws_handler

        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        websocket = FakeWebSocket(
            [
                json.dumps({"type": "auth", "client_id": "pc-01", "passkey": "secret"}),
                json.dumps({
                    "type": "file_upload_start",
                    "file_id": "up_1",
                    "name": "../../evil.txt",
                    "size": 4,
                }),
                b"data",
                json.dumps({"type": "file_upload_end", "file_id": "up_1"}),
            ]
        )

        with patch.object(ws_handler, "UPLOAD_DIR", str(tmp_path)):
            await handler._handle_client(websocket)

        saved_files = list(tmp_path.rglob("evil.txt"))
        assert len(saved_files) == 1
        assert saved_files[0].is_relative_to(tmp_path)
        assert not (tmp_path.parent / "evil.txt").exists()


async def test_command_socket_permissions_are_owner_only(tmp_path, monkeypatch):
    import importlib
    import sys

    monkeypatch.setattr(sys, "argv", ["run_and_send.py"])
    run_and_send = importlib.import_module("run_and_send")
    socket_path = tmp_path / "cmd.sock"
    socket_path.write_text("")

    run_and_send.set_command_socket_permissions(str(socket_path))

    assert oct(os.stat(socket_path).st_mode & 0o777) == "0o600"
