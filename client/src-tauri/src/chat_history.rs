use std::sync::atomic::{AtomicU64, Ordering};

static SESSION_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize)]
pub struct KayaSessionRecord {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AcpSessionRecord {
    pub id: String,
    pub kaya_session_id: String,
    pub remote_session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatMessageRecord {
    pub id: String,
    pub kaya_session_id: String,
    pub acp_session_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

fn init_kaya_sessions_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS kaya_sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0
        )",
    )
    .map_err(|e| format!("初始化会话表失败: {}", e))
}

fn init_acp_sessions_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS acp_sessions (
            id TEXT PRIMARY KEY,
            kaya_session_id TEXT NOT NULL,
            remote_session_id TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            is_active INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (kaya_session_id) REFERENCES kaya_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_acp_sessions_kaya_session_id
            ON acp_sessions(kaya_session_id)",
    )
    .map_err(|e| format!("初始化 ACP 会话表失败: {}", e))
}

fn init_chat_messages_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS chat_messages (
            id TEXT PRIMARY KEY,
            kaya_session_id TEXT NOT NULL,
            acp_session_id TEXT,
            role TEXT NOT NULL,
            content TEXT NOT NULL,
            created_at TEXT NOT NULL,
            FOREIGN KEY (kaya_session_id) REFERENCES kaya_sessions(id),
            FOREIGN KEY (acp_session_id) REFERENCES acp_sessions(id)
        );
        CREATE INDEX IF NOT EXISTS idx_chat_messages_kaya_session_id
            ON chat_messages(kaya_session_id)",
    )
    .map_err(|e| format!("初始化消息表失败: {}", e))
}

fn next_session_id() -> String {
    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
    let seq = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("kaya_{}_{}", ts, seq)
}

fn next_acp_session_id() -> String {
    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
    let seq = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("acp_{}_{}", ts, seq)
}

fn next_message_id() -> String {
    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
    let seq = SESSION_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("msg_{}_{}", ts, seq)
}

pub fn default_kaya_session_title(now: chrono::DateTime<chrono::Local>) -> String {
    format!("新会话 {}", now.format("%Y-%m-%d %H:%M"))
}

pub fn summarize_first_user_message_for_title(text: &str) -> String {
    text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub fn open_history_db_in_memory() -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open_in_memory()
        .map_err(|e| format!("打开内存数据库失败: {}", e))?;
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(|e| format!("启用外键约束失败: {}", e))?;
    init_kaya_sessions_table(&conn)?;
    init_acp_sessions_table(&conn)?;
    init_chat_messages_table(&conn)?;
    Ok(conn)
}

pub fn create_kaya_session(
    db: &rusqlite::Connection,
    title: &str,
) -> Result<KayaSessionRecord, String> {
    let id = next_session_id();
    let now = chrono::Utc::now().to_rfc3339();

    // Deactivate any previously active kaya session so only the new one is active.
    db.execute("UPDATE kaya_sessions SET is_active = 0 WHERE is_active = 1", [])
        .map_err(|e| format!("重置活跃会话失败: {}", e))?;

    db.execute(
        "INSERT INTO kaya_sessions (id, title, created_at, updated_at, is_active) VALUES (?1, ?2, ?3, ?4, 1)",
        rusqlite::params![id, title, now, now],
    )
    .map_err(|e| format!("创建会话失败: {}", e))?;

    Ok(KayaSessionRecord {
        id,
        title: title.to_string(),
        created_at: now.clone(),
        updated_at: now,
        is_active: true,
    })
}

pub fn load_kaya_sessions(
    db: &rusqlite::Connection,
) -> Result<Vec<KayaSessionRecord>, String> {
    let mut stmt = db
        .prepare(
            "SELECT id, title, created_at, updated_at, is_active \
             FROM kaya_sessions ORDER BY updated_at DESC",
        )
        .map_err(|e| format!("查询会话列表失败: {}", e))?;

    let records = stmt
        .query_map([], |row| {
            Ok(KayaSessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                is_active: row.get::<_, i32>(4)? != 0,
            })
        })
        .map_err(|e| format!("查询会话列表失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("查询会话列表失败: {}", e))?;

    Ok(records)
}

pub fn ensure_active_kaya_session(
    db: &rusqlite::Connection,
) -> Result<KayaSessionRecord, String> {
    let existing = db
        .query_row(
            "SELECT id, title, created_at, updated_at, is_active \
             FROM kaya_sessions WHERE is_active = 1 LIMIT 1",
            [],
            |row| {
                Ok(KayaSessionRecord {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                    is_active: row.get::<_, i32>(4)? != 0,
                })
            },
        )
        .ok();

    match existing {
        Some(session) => Ok(session),
        None => create_kaya_session(db, &default_kaya_session_title(chrono::Local::now())),
    }
}

pub fn update_kaya_session_title_from_first_user_message(
    db: &rusqlite::Connection,
    kaya_session_id: &str,
    first_user_message: &str,
) -> Result<KayaSessionRecord, String> {
    let mut stmt = db
        .prepare("SELECT id, title, created_at, updated_at, is_active FROM kaya_sessions WHERE id = ?1")
        .map_err(|e| format!("查询会话失败: {}", e))?;

    let session = stmt
        .query_row(rusqlite::params![kaya_session_id], |row| {
            Ok(KayaSessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                is_active: row.get::<_, i32>(4)? != 0,
            })
        })
        .map_err(|e| format!("查询会话失败: {}", e))?;

    if !session.title.starts_with("新会话 ") {
        return Ok(session);
    }

    let summarized = summarize_first_user_message_for_title(first_user_message);
    if summarized.is_empty() {
        return Ok(session);
    }

    let now = chrono::Utc::now().to_rfc3339();
    db.execute(
        "UPDATE kaya_sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![summarized, now, kaya_session_id],
    )
    .map_err(|e| format!("更新会话标题失败: {}", e))?;

    Ok(KayaSessionRecord {
        title: summarized,
        updated_at: now,
        ..session
    })
}

pub fn open_history_db(path: &std::path::Path) -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open(path)
        .map_err(|e| format!("打开历史数据库失败: {}", e))?;
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(|e| format!("启用外键约束失败: {}", e))?;
    init_kaya_sessions_table(&conn)?;
    init_acp_sessions_table(&conn)?;
    init_chat_messages_table(&conn)?;
    Ok(conn)
}

pub fn load_latest_kaya_session(
    db: &rusqlite::Connection,
) -> Result<Option<KayaSessionRecord>, String> {
    let mut stmt = db
        .prepare("SELECT id, title, created_at, updated_at, is_active FROM kaya_sessions ORDER BY updated_at DESC LIMIT 1")
        .map_err(|e| format!("查询会话失败: {}", e))?;

    let result = stmt
        .query_row([], |row| {
            Ok(KayaSessionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
                updated_at: row.get(3)?,
                is_active: row.get::<_, i32>(4)? != 0,
            })
        })
        .ok();

    Ok(result)
}

pub fn load_acp_sessions_for_kaya_session(
    db: &rusqlite::Connection,
    kaya_session_id: &str,
) -> Result<Vec<AcpSessionRecord>, String> {
    let mut stmt = db
        .prepare(
            "SELECT id, kaya_session_id, remote_session_id, created_at, updated_at, is_active \
             FROM acp_sessions \
             WHERE kaya_session_id = ?1 \
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| format!("查询 ACP 会话失败: {}", e))?;

    let records = stmt
        .query_map(rusqlite::params![kaya_session_id], |row| {
            Ok(AcpSessionRecord {
                id: row.get(0)?,
                kaya_session_id: row.get(1)?,
                remote_session_id: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                is_active: row.get::<_, i32>(5)? != 0,
            })
        })
        .map_err(|e| format!("查询 ACP 会话失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("查询 ACP 会话失败: {}", e))?;

    Ok(records)
}

pub fn create_or_switch_acp_session(
    db: &rusqlite::Connection,
    kaya_session_id: &str,
    remote_session_id: &str,
) -> Result<AcpSessionRecord, String> {
    // Deactivate any previously active ACP session for this kaya session
    // so only the target ACP session ends up active.
    db.execute(
        "UPDATE acp_sessions SET is_active = 0 WHERE kaya_session_id = ?1 AND is_active = 1",
        rusqlite::params![kaya_session_id],
    )
    .map_err(|e| format!("重置活跃 ACP 会话失败: {}", e))?;

    let existing = db.query_row(
        "SELECT id, kaya_session_id, remote_session_id, created_at, updated_at, is_active \
         FROM acp_sessions \
         WHERE kaya_session_id = ?1 AND remote_session_id = ?2",
        rusqlite::params![kaya_session_id, remote_session_id],
        |row| {
            Ok(AcpSessionRecord {
                id: row.get(0)?,
                kaya_session_id: row.get(1)?,
                remote_session_id: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
                is_active: row.get::<_, i32>(5)? != 0,
            })
        },
    );

    match existing {
        Ok(record) => {
            let now = chrono::Utc::now().to_rfc3339();
            db.execute(
                "UPDATE acp_sessions SET updated_at = ?1, is_active = 1 WHERE id = ?2",
                rusqlite::params![now, record.id],
            )
            .map_err(|e| format!("更新 ACP 会话失败: {}", e))?;
            Ok(AcpSessionRecord {
                updated_at: now,
                is_active: true,
                ..record
            })
        }
        Err(_) => {
            let id = next_acp_session_id();
            let now = chrono::Utc::now().to_rfc3339();
            db.execute(
                "INSERT INTO acp_sessions (id, kaya_session_id, remote_session_id, created_at, updated_at, is_active) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 1)",
                rusqlite::params![id, kaya_session_id, remote_session_id, now, now],
            )
            .map_err(|e| format!("创建 ACP 会话失败: {}", e))?;

            Ok(AcpSessionRecord {
                id,
                kaya_session_id: kaya_session_id.to_string(),
                remote_session_id: remote_session_id.to_string(),
                created_at: now.clone(),
                updated_at: now,
                is_active: true,
            })
        }
    }
}

pub fn append_chat_message(
    db: &rusqlite::Connection,
    kaya_session_id: &str,
    acp_session_id: Option<&str>,
    role: &str,
    content: &str,
) -> Result<ChatMessageRecord, String> {
    let id = next_message_id();
    let now = chrono::Utc::now().to_rfc3339();
    db.execute(
        "INSERT INTO chat_messages (id, kaya_session_id, acp_session_id, role, content, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![id, kaya_session_id, acp_session_id, role, content, now],
    )
    .map_err(|e| format!("追加消息失败: {}", e))?;

    Ok(ChatMessageRecord {
        id,
        kaya_session_id: kaya_session_id.to_string(),
        acp_session_id: acp_session_id.map(|s| s.to_string()),
        role: role.to_string(),
        content: content.to_string(),
        created_at: now,
    })
}

pub fn load_chat_messages(
    db: &rusqlite::Connection,
    kaya_session_id: &str,
) -> Result<Vec<ChatMessageRecord>, String> {
    let mut stmt = db
        .prepare(
            "SELECT id, kaya_session_id, acp_session_id, role, content, created_at \
             FROM chat_messages \
             WHERE kaya_session_id = ?1 \
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(|e| format!("查询消息失败: {}", e))?;

    let records = stmt
        .query_map(rusqlite::params![kaya_session_id], |row| {
            Ok(ChatMessageRecord {
                id: row.get(0)?,
                kaya_session_id: row.get(1)?,
                acp_session_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|e| format!("查询消息失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("查询消息失败: {}", e))?;

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_active_kaya_session_creates_one_when_missing() {
        let db = open_history_db_in_memory().unwrap();
        let session = ensure_active_kaya_session(&db).unwrap();
        let latest = load_latest_kaya_session(&db).unwrap().unwrap();
        assert_eq!(session.id, latest.id);
    }

    #[test]
    fn creates_and_restores_latest_kaya_session() {
        let db = open_history_db_in_memory().unwrap();
        let first = create_kaya_session(&db, "新会话 1").unwrap();
        let second = create_kaya_session(&db, "新会话 2").unwrap();

        // Verify only the latest session is active; any previous one was deactivated.
        let first_active: bool = db
            .query_row(
                "SELECT is_active FROM kaya_sessions WHERE id = ?1",
                rusqlite::params![first.id],
                |row| row.get::<_, i32>(0).map(|v| v != 0),
            )
            .unwrap();
        assert!(!first_active, "previous session must be deactivated");
        let second_active: bool = db
            .query_row(
                "SELECT is_active FROM kaya_sessions WHERE id = ?1",
                rusqlite::params![second.id],
                |row| row.get::<_, i32>(0).map(|v| v != 0),
            )
            .unwrap();
        assert!(second_active, "new session must be active");

        let latest = load_latest_kaya_session(&db).unwrap().unwrap();

        assert_eq!(latest.id, second.id);
        assert_ne!(first.id, second.id);
    }

    #[test]
    fn creates_new_acp_session_without_creating_new_kaya_session() {
        let db = open_history_db_in_memory().unwrap();
        let kaya = ensure_active_kaya_session(&db).unwrap();
        let first = create_or_switch_acp_session(&db, &kaya.id, "remote-a").unwrap();
        let second = create_or_switch_acp_session(&db, &kaya.id, "remote-b").unwrap();
        let sessions = load_acp_sessions_for_kaya_session(&db, &kaya.id).unwrap();

        assert_ne!(first.id, second.id);
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().all(|s| s.kaya_session_id == kaya.id));
    }

    #[test]
    fn updates_default_title_from_first_user_message() {
        let db = open_history_db_in_memory().unwrap();
        let session = create_kaya_session(&db, "新会话 2026-06-12 18:30").unwrap();

        let updated = update_kaya_session_title_from_first_user_message(
            &db,
            &session.id,
            "  帮我查一下 10.0.0.11 的客户端日志  ",
        )
        .unwrap();

        assert_eq!(updated.title, "帮我查一下 10.0.0.11 的客户端日志");
    }

    #[test]
    fn appends_messages_to_kaya_session_across_multiple_acp_sessions() {
        let db = open_history_db_in_memory().unwrap();
        let kaya = create_kaya_session(&db, "会话").unwrap();
        let acp_a = create_or_switch_acp_session(&db, &kaya.id, "remote-a").unwrap();
        assert!(acp_a.is_active, "first ACP session should be active");

        append_chat_message(&db, &kaya.id, Some(&acp_a.id), "user", "hello").unwrap();

        let acp_b = create_or_switch_acp_session(&db, &kaya.id, "remote-b").unwrap();
        assert!(acp_b.is_active, "second ACP session should be active");
        // acp_a must have been deactivated when acp_b was switched in
        let acp_a_still_active: bool = db
            .query_row(
                "SELECT is_active FROM acp_sessions WHERE id = ?1",
                rusqlite::params![acp_a.id],
                |row| row.get::<_, i32>(0).map(|v| v != 0),
            )
            .unwrap();
        assert!(!acp_a_still_active, "previous ACP session must be deactivated");

        append_chat_message(&db, &kaya.id, Some(&acp_b.id), "assistant", "world").unwrap();

        let messages = load_chat_messages(&db, &kaya.id).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "hello");
        assert_eq!(messages[1].content, "world");
        assert_eq!(messages[0].acp_session_id.as_deref(), Some(acp_a.id.as_str()));
        assert_eq!(messages[1].acp_session_id.as_deref(), Some(acp_b.id.as_str()));
    }
}
