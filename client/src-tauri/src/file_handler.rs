use chrono::Datelike;
use sha2::Digest;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 文件接收状态机（用于组装二进制帧）
pub struct FileReceive {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    bytes_received: u64,
    temp_path: PathBuf,
    final_path: PathBuf,
    file: std::fs::File,
    hasher: sha2::Sha256,
}

impl FileReceive {
    pub fn new(file_id: String, name: String, size: u64) -> Result<Self, String> {
        let dir = date_dir();
        let safe_name = safe_received_filename(&name);
        let final_path = unique_file_path(&dir, &safe_name);
        let temp_path = dir.join(format!(
            ".receive-{}-{}.part",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| e.to_string())?
                .as_nanos()
        ));
        let file = std::fs::File::create(&temp_path).map_err(|e| e.to_string())?;

        Ok(Self {
            file_id,
            name,
            size,
            bytes_received: 0,
            temp_path,
            final_path,
            file,
            hasher: sha2::Sha256::new(),
        })
    }

    pub fn append_data(&mut self, chunk: Vec<u8>) -> Result<(), String> {
        self.file.write_all(&chunk).map_err(|e| e.to_string())?;
        self.hasher.update(&chunk);
        self.bytes_received += chunk.len() as u64;
        Ok(())
    }

    pub fn finalize(mut self, checksum: &str) -> Result<PathBuf, String> {
        self.file.flush().map_err(|e| e.to_string())?;
        drop(self.file);
        if self.bytes_received != self.size {
            let _ = std::fs::remove_file(&self.temp_path);
            return Err(format!(
                "File size mismatch: received {} bytes, expected {} bytes",
                self.bytes_received, self.size,
            ));
        }
        let local_checksum = format!("sha256:{:x}", self.hasher.finalize());
        if !checksum.is_empty() && local_checksum != checksum {
            let _ = std::fs::remove_file(&self.temp_path);
            return Err("Checksum mismatch".to_string());
        }
        std::fs::rename(&self.temp_path, &self.final_path).map_err(|e| e.to_string())?;
        Ok(self.final_path)
    }

    pub fn abort(self) {
        let temp_path = self.temp_path.clone();
        drop(self.file);
        let _ = std::fs::remove_file(temp_path);
    }

    pub fn bytes_received(&self) -> u64 {
        self.bytes_received
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

fn safe_received_filename(name: &str) -> String {
    Path::new(&name.replace('\\', "/"))
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !matches!(*name, "" | "." | ".."))
        .unwrap_or("unknown")
        .to_string()
}

fn unique_file_path(dir: &Path, safe_name: &str) -> PathBuf {
    let path = dir.join(safe_name);
    if path.exists() {
        let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_str().unwrap_or("")))
            .unwrap_or_default();
        dir.join(format!("{}_{}{}", stem, ts, ext))
    } else {
        path
    }
}

/// 保存文件到 ~/kaya-transfer/YYYY/MM/DD/，返回完整路径
pub fn save_file(name: &str, data: &[u8]) -> Result<PathBuf, String> {
    let dir = date_dir();
    let safe_name = safe_received_filename(name);
    let path = unique_file_path(&dir, &safe_name);

    std::fs::write(&path, data).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated_home(prefix: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "{}-{}",
            prefix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        home
    }

    #[test]
    fn file_receive_streams_chunks_to_temp_file_and_finalizes() {
        let home = isolated_home("kaya-file-receive-stream");
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);
        let mut receive = FileReceive::new("f_1".into(), "stream.txt".into(), 8).unwrap();

        receive.append_data(b"abc".to_vec()).unwrap();
        receive.append_data(b"defgh".to_vec()).unwrap();
        let path = receive
            .finalize("sha256:9c56cc51b374c3ba189210d5b6d4bf57790d351c96c47c02190ecf1e430635ab")
            .unwrap();

        assert!(path.starts_with(home.join("kaya-transfer")));
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("stream.txt"));
        assert_eq!(std::fs::read(path).unwrap(), b"abcdefgh");
    }

    #[test]
    fn test_file_receive_new() {
        let home = isolated_home("kaya-file-receive-new");
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);

        let fr = FileReceive::new("f_abc".into(), "test.png".into(), 1024).unwrap();
        assert_eq!(fr.file_id, "f_abc");
        assert_eq!(fr.name, "test.png");
        assert_eq!(fr.size, 1024);
        assert_eq!(fr.bytes_received(), 0);
        fr.abort();
    }

    #[test]
    fn test_file_receive_append_data() {
        let home = isolated_home("kaya-file-receive-append");
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);

        let mut fr = FileReceive::new("f_abc".into(), "test.bin".into(), 6).unwrap();
        fr.append_data(vec![1, 2, 3]).unwrap();
        assert_eq!(fr.bytes_received(), 3);
        fr.append_data(vec![4, 5, 6]).unwrap();
        assert_eq!(fr.bytes_received(), 6);
        let path = fr.finalize("").unwrap();
        assert_eq!(std::fs::read(path).unwrap(), vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_file_receive_empty() {
        let home = isolated_home("kaya-file-receive-empty");
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);

        let mut fr = FileReceive::new("f_empty".into(), "empty.bin".into(), 0).unwrap();
        assert_eq!(fr.bytes_received(), 0);
        fr.append_data(vec![]).unwrap();
        assert_eq!(fr.bytes_received(), 0);
        let path = fr.finalize("").unwrap();
        assert_eq!(std::fs::read(path).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn save_file_strips_path_components_from_received_name() {
        let home = std::env::temp_dir().join(format!(
            "kaya-file-handler-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);

        let saved_path = save_file("../../evil.txt", b"data").unwrap();

        assert!(saved_path.starts_with(home.join("kaya-transfer")));
        assert_eq!(saved_path.file_name().and_then(|name| name.to_str()), Some("evil.txt"));
        assert!(!home.join("evil.txt").exists());
    }
}
