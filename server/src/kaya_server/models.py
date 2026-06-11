from dataclasses import dataclass
from datetime import datetime


@dataclass
class Client:
    client_id: str
    description: str
    passkey_hash: str
    created_at: datetime
    updated_at: datetime


__all__ = ["Client"]
