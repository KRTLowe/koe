import bcrypt


def hash_passkey(passkey: str) -> str:
    """返回 bcrypt 哈希字符串"""
    return bcrypt.hashpw(passkey.encode("utf-8"), bcrypt.gensalt()).decode("utf-8")


def verify_passkey(passkey: str, hashed: str) -> bool:
    """常量时间比较验证 passkey。bcrypt.checkpw 使用常量时间比较。"""
    return bcrypt.checkpw(passkey.encode("utf-8"), hashed.encode("utf-8"))
