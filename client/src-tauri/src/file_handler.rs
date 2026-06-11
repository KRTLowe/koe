use chrono::Datelike;
use std::path::PathBuf;

/// 文件接收状态机（用于组装二进制帧）
pub struct FileReceive {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub data: Vec<u8>,
}

impl FileReceive {
    pub fn new(file_id: String, name: String, size: u64) -> Self {
        Self {
            file_id,
            name,
            size,
            data: Vec::with_capacity(size as usize),
        }
    }

    pub fn append_data(&mut self, chunk: Vec<u8>) {
        self.data.extend(chunk);
    }
}

/// 用户主目录
fn home_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("USERPROFILE") {
        PathBuf::from(dir)
    } else if let Ok(dir) = std::env::var("HOME") {
        PathBuf::from(dir)
    } else {
        std::env::temp_dir()
    }
}

/// kaya-transfer 根目录（~/kaya-transfer）
pub fn base_dir() -> PathBuf {
    let dir = home_dir().join("kaya-transfer");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// 按月分层的保存目录（~/kaya-transfer/YYYY-MM）
pub fn date_dir() -> PathBuf {
    let now = chrono::Local::now();
    let dir = base_dir().join(format!("{:04}-{:02}", now.year(), now.month()));
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// 保存文件到 ~/kaya-transfer/YYYY/MM/DD/，返回完整路径
pub fn save_file(name: &str, data: &[u8]) -> Result<PathBuf, String> {
    let dir = date_dir();
    let path = dir.join(name);

    // 如果文件已存在，添加时间戳后缀
    let path = if path.exists() {
        let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_str().unwrap_or("")))
            .unwrap_or_default();
        dir.join(format!("{}_{}{}", stem, ts, ext))
    } else {
        path
    };

    std::fs::write(&path, data).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_receive_new() {
        let fr = FileReceive::new("f_abc".into(), "test.png".into(), 1024);
        assert_eq!(fr.file_id, "f_abc");
        assert_eq!(fr.name, "test.png");
        assert_eq!(fr.size, 1024);
        assert!(fr.data.is_empty());
        assert_eq!(fr.data.capacity(), 1024);
    }

    #[test]
    fn test_file_receive_append_data() {
        let mut fr = FileReceive::new("f_abc".into(), "test.bin".into(), 6);
        fr.append_data(vec![1, 2, 3]);
        assert_eq!(fr.data.len(), 3);
        fr.append_data(vec![4, 5, 6]);
        assert_eq!(fr.data.len(), 6);
        assert_eq!(fr.data, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_file_receive_empty() {
        let mut fr = FileReceive::new("f_empty".into(), "empty.bin".into(), 0);
        assert!(fr.data.is_empty());
        fr.append_data(vec![]);
        assert!(fr.data.is_empty());
    }

}
