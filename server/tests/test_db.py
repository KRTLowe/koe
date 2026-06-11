import pytest
import tempfile
from pathlib import Path
from kaya_server.db import Database


def test_create_tables():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        db.register_client("pc-01", "My PC", "hash123")
        client = db.get_client("pc-01")
        assert client is not None
        assert client.client_id == "pc-01"
        assert client.description == "My PC"
        assert client.passkey_hash == "hash123"
        db.close()


def test_list_clients():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        db.register_client("pc-01", "PC 1", "h1")
        db.register_client("pc-02", "PC 2", "h2")
        clients = db.list_clients()
        assert len(clients) == 2
        db.close()


def test_remove_client():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        db.register_client("pc-01", "PC 1", "h1")
        db.remove_client("pc-01")
        assert db.get_client("pc-01") is None
        db.close()


def test_client_exists():
    with tempfile.TemporaryDirectory() as tmp:
        db_path = Path(tmp) / "test.db"
        db = Database(str(db_path))
        db.initialize()
        assert not db.client_exists("pc-01")
        db.register_client("pc-01", "PC 1", "h1")
        assert db.client_exists("pc-01")
        db.close()
