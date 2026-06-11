import pytest
from kaya_server.auth import hash_passkey, verify_passkey


def test_hash_and_verify():
    passkey = "my-secret-key-123"
    hashed = hash_passkey(passkey)
    assert hashed != passkey  # 不存明文
    assert verify_passkey(passkey, hashed) is True


def test_wrong_passkey():
    hashed = hash_passkey("correct-key")
    assert verify_passkey("wrong-key", hashed) is False


def test_different_lengths():
    h1 = hash_passkey("short")
    h2 = hash_passkey("a-very-long-passkey-12345")
    assert verify_passkey("short", h1) is True
    assert verify_passkey("short", h2) is False
