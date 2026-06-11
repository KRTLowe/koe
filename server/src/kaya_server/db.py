import sqlite3
from pathlib import Path
from datetime import datetime, timezone
from typing import Optional, List
from kaya_server.models import Client


class Database:
    def __init__(self, db_path: str | None = None):
        if db_path is None:
            db_path = str(Path.home() / ".kaya-transfer-hub" / "hub.db")
            Path(db_path).parent.mkdir(parents=True, exist_ok=True)
        self.db_path = db_path
        self.conn: Optional[sqlite3.Connection] = None

    def initialize(self):
        self.conn = sqlite3.connect(self.db_path)
        self.conn.row_factory = sqlite3.Row
        self.conn.execute("""
            CREATE TABLE IF NOT EXISTS clients (
                client_id TEXT PRIMARY KEY,
                description TEXT NOT NULL,
                passkey_hash TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )
        """)
        self.conn.commit()

    def register_client(self, client_id: str, description: str, passkey_hash: str) -> Client:
        now = datetime.now(timezone.utc).isoformat()
        self.conn.execute(
            "INSERT INTO clients (client_id, description, passkey_hash, created_at, updated_at) "
            "VALUES (?, ?, ?, ?, ?)",
            (client_id, description, passkey_hash, now, now),
        )
        self.conn.commit()
        return self.get_client(client_id)

    def get_client(self, client_id: str) -> Optional[Client]:
        row = self.conn.execute(
            "SELECT * FROM clients WHERE client_id = ?", (client_id,)
        ).fetchone()
        if row is None:
            return None
        return Client(
            client_id=row["client_id"],
            description=row["description"],
            passkey_hash=row["passkey_hash"],
            created_at=datetime.fromisoformat(row["created_at"]),
            updated_at=datetime.fromisoformat(row["updated_at"]),
        )

    def list_clients(self) -> List[Client]:
        rows = self.conn.execute("SELECT * FROM clients ORDER BY created_at").fetchall()
        return [
            Client(
                client_id=r["client_id"],
                description=r["description"],
                passkey_hash=r["passkey_hash"],
                created_at=datetime.fromisoformat(r["created_at"]),
                updated_at=datetime.fromisoformat(r["updated_at"]),
            )
            for r in rows
        ]

    def remove_client(self, client_id: str) -> bool:
        cur = self.conn.execute("DELETE FROM clients WHERE client_id = ?", (client_id,))
        self.conn.commit()
        return cur.rowcount > 0

    def client_exists(self, client_id: str) -> bool:
        row = self.conn.execute(
            "SELECT 1 FROM clients WHERE client_id = ?", (client_id,)
        ).fetchone()
        return row is not None

    def close(self):
        if self.conn:
            self.conn.close()
