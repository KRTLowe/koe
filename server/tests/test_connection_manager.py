import time
import pytest
from kaya_server.connection_manager import ConnectionManager


def test_register_and_get():
    mgr = ConnectionManager()
    fake_ws = "fake-websocket-object"
    mgr.register("pc-01", fake_ws)
    assert mgr.is_online("pc-01")
    assert mgr.get_connection("pc-01") == fake_ws


def test_unregister():
    mgr = ConnectionManager()
    mgr.register("pc-01", "ws")
    mgr.unregister("pc-01")
    assert not mgr.is_online("pc-01")


def test_get_online_clients():
    mgr = ConnectionManager()
    mgr.register("pc-01", "ws1")
    mgr.register("pc-02", "ws2")
    online = mgr.get_online_clients()
    assert set(online) == {"pc-01", "pc-02"}
    mgr.unregister("pc-01")
    online = mgr.get_online_clients()
    assert set(online) == {"pc-02"}


def test_heartbeat_timeout():
    mgr = ConnectionManager(heartbeat_timeout=0.1)  # 100ms timeout
    mgr.register("pc-01", "ws")
    assert mgr.is_online("pc-01")
    time.sleep(0.15)
    assert not mgr.is_online("pc-01")


def test_update_heartbeat():
    mgr = ConnectionManager(heartbeat_timeout=0.1)
    mgr.register("pc-01", "ws")
    time.sleep(0.05)
    mgr.update_heartbeat("pc-01")  # 重置计时器
    time.sleep(0.05)
    assert mgr.is_online("pc-01")  # 因为刷新了心跳
    time.sleep(0.1)
    assert not mgr.is_online("pc-01")  # 超时
