use std::sync::atomic::{AtomicU64, Ordering};

static FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct FileTransferRecord {
    pub id: String,
    pub file_name: String,
    pub file_size: i64,
    pub direction: String,
    pub status: String,
    pub file_path: Option<String>,
    pub kaya_session_id: Option<String>,
    pub acp_session_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct NewFileTransferRecord {
    pub file_name: String,
    pub file_size: i64,
    pub direction: String,
    pub status: String,
    pub file_path: Option<String>,
    pub kaya_session_id: Option<String>,
    pub acp_session_id: Option<String>,
}

fn init_file_transfer_history_table(conn: &rusqlite::Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS file_transfer_history (
            id TEXT PRIMARY KEY,
            file_name TEXT NOT NULL,
            file_size INTEGER NOT NULL,
            direction TEXT NOT NULL,
            status TEXT NOT NULL,
            file_path TEXT,
            kaya_session_id TEXT,
            acp_session_id TEXT,
            created_at TEXT NOT NULL
        )",
    )
    .map_err(|e| format!("初始化文件传输记录表失败: {}", e))
}

fn next_file_transfer_id() -> String {
    let ts = chrono::Utc::now().format("%Y%m%d%H%M%S%3f").to_string();
    let seq = FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ft_{}_{}", ts, seq)
}

pub fn open_file_history_db(path: &std::path::Path) -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open(path)
        .map_err(|e| format!("打开文件历史数据库失败: {}", e))?;
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(|e| format!("启用外键约束失败: {}", e))?;
    init_file_transfer_history_table(&conn)?;
    Ok(conn)
}

pub fn open_file_history_db_in_memory() -> Result<rusqlite::Connection, String> {
    let conn = rusqlite::Connection::open_in_memory()
        .map_err(|e| format!("打开内存数据库失败: {}", e))?;
    conn.execute_batch("PRAGMA foreign_keys = ON")
        .map_err(|e| format!("启用外键约束失败: {}", e))?;
    init_file_transfer_history_table(&conn)?;
    Ok(conn)
}

pub fn append_file_transfer_record(
    db: &rusqlite::Connection,
    record: NewFileTransferRecord,
) -> Result<FileTransferRecord, String> {
    let id = next_file_transfer_id();
    let now = chrono::Utc::now().to_rfc3339();

    db.execute(
        "INSERT INTO file_transfer_history (id, file_name, file_size, direction, status, file_path, kaya_session_id, acp_session_id, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            id,
            record.file_name,
            record.file_size,
            record.direction,
            record.status,
            record.file_path,
            record.kaya_session_id,
            record.acp_session_id,
            now,
        ],
    )
    .map_err(|e| format!("追加文件传输记录失败: {}", e))?;

    Ok(FileTransferRecord {
        id,
        file_name: record.file_name,
        file_size: record.file_size,
        direction: record.direction,
        status: record.status,
        file_path: record.file_path,
        kaya_session_id: record.kaya_session_id,
        acp_session_id: record.acp_session_id,
        created_at: now,
    })
}

pub fn load_file_transfer_history(
    db: &rusqlite::Connection,
) -> Result<Vec<FileTransferRecord>, String> {
    let mut stmt = db
        .prepare(
            "SELECT id, file_name, file_size, direction, status, file_path, kaya_session_id, acp_session_id, created_at \
             FROM file_transfer_history \
             ORDER BY created_at DESC, id DESC",
        )
        .map_err(|e| format!("查询文件传输历史失败: {}", e))?;

    let records = stmt
        .query_map([], |row| {
            Ok(FileTransferRecord {
                id: row.get(0)?,
                file_name: row.get(1)?,
                file_size: row.get(2)?,
                direction: row.get(3)?,
                status: row.get(4)?,
                file_path: row.get(5)?,
                kaya_session_id: row.get(6)?,
                acp_session_id: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| format!("查询文件传输历史失败: {}", e))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("查询文件传输历史失败: {}", e))?;

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_file_transfer_history_in_descending_time_order() {
        let db = open_file_history_db_in_memory().unwrap();

        let r1 = append_file_transfer_record(
            &db,
            NewFileTransferRecord {
                file_name: "alpha.txt".into(),
                file_size: 100,
                direction: "sent".into(),
                status: "ok".into(),
                file_path: None,
                kaya_session_id: None,
                acp_session_id: None,
            },
        )
        .unwrap();

        // Ensure distinct timestamps for deterministic ordering
        std::thread::sleep(std::time::Duration::from_millis(5));

        let r2 = append_file_transfer_record(
            &db,
            NewFileTransferRecord {
                file_name: "beta.txt".into(),
                file_size: 200,
                direction: "received".into(),
                status: "ok".into(),
                file_path: None,
                kaya_session_id: None,
                acp_session_id: None,
            },
        )
        .unwrap();

        std::thread::sleep(std::time::Duration::from_millis(5));

        let r3 = append_file_transfer_record(
            &db,
            NewFileTransferRecord {
                file_name: "gamma.txt".into(),
                file_size: 300,
                direction: "sent".into(),
                status: "error".into(),
                file_path: None,
                kaya_session_id: None,
                acp_session_id: None,
            },
        )
        .unwrap();

        let history = load_file_transfer_history(&db).unwrap();

        assert_eq!(history.len(), 3);
        // Must be ordered by created_at DESC, id DESC
        assert_eq!(history[0].id, r3.id, "most recent first");
        assert_eq!(history[1].id, r2.id, "middle record second");
        assert_eq!(history[2].id, r1.id, "oldest last");
        // Verify all fields match
        assert_eq!(history[0].file_name, "gamma.txt");
        assert_eq!(history[1].file_name, "beta.txt");
        assert_eq!(history[2].file_name, "alpha.txt");
        assert_eq!(history[0].direction, "sent");
        assert_eq!(history[1].direction, "received");
        assert_eq!(history[2].direction, "sent");
        assert_eq!(history[0].status, "error");
        assert_eq!(history[1].status, "ok");
        assert_eq!(history[2].status, "ok");
        for record in &history {
            assert_eq!(record.file_path, None);
            assert_eq!(record.kaya_session_id, None);
            assert_eq!(record.acp_session_id, None);
        }
    }

    #[test]
    fn appends_and_loads_single_file_transfer_record() {
        let db = open_file_history_db_in_memory().unwrap();

        let record = append_file_transfer_record(
            &db,
            NewFileTransferRecord {
                file_name: "photo.png".into(),
                file_size: 42_000,
                direction: "received".into(),
                status: "ok".into(),
                file_path: Some("C:\\Downloads\\photo.png".into()),
                kaya_session_id: Some("kaya_20260612_0".into()),
                acp_session_id: Some("acp_20260612_0".into()),
            },
        )
        .unwrap();

        assert!(record.id.starts_with("ft_"));
        assert_eq!(record.file_name, "photo.png");
        assert_eq!(record.file_size, 42_000);
        assert_eq!(record.direction, "received");
        assert_eq!(record.status, "ok");
        assert_eq!(record.file_path.as_deref(), Some("C:\\Downloads\\photo.png"));
        assert_eq!(record.kaya_session_id.as_deref(), Some("kaya_20260612_0"));
        assert_eq!(record.acp_session_id.as_deref(), Some("acp_20260612_0"));
        assert!(!record.created_at.is_empty());

        let history = load_file_transfer_history(&db).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, record.id);
        assert_eq!(history[0].file_path, record.file_path);
        assert_eq!(history[0].kaya_session_id, record.kaya_session_id);
        assert_eq!(history[0].acp_session_id, record.acp_session_id);
    }
}
