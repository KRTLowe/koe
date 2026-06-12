use crate::file_handler;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// 收到文件后的处理逻辑：保存文件 + 通知前端窗口弹出。
/// 返回 `Some(path)` 保存成功，`None` 保存失败。
pub fn on_file_received(app: &AppHandle, name: &str, size: u64, data: &[u8]) -> Option<PathBuf> {
    let save_path = match file_handler::save_file(name, data) {
        Ok(p) => p,
        Err(e) => {
            log::error!("Failed to save file '{}': {}", name, e);
            let _ = app.emit(
                "file-error",
                serde_json::json!({ "name": name, "error": e }),
            );
            return None;
        }
    };

    // 发送事件给前端（弹出窗口显示文件信息）
    let _ = app.emit(
        "file-received",
        serde_json::json!({
            "name": name,
            "size": size,
            "path": save_path.to_string_lossy(),
        }),
    );

    // 如果有主窗口，将其显示到前台
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }

    log::info!(
        "File received: {} ({} bytes) -> {}",
        name,
        size,
        save_path.display()
    );
    Some(save_path)
}

pub fn file_received_payload(name: &str, size: u64, path: &Path) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "size": size,
        "path": path.to_string_lossy(),
    })
}

pub fn on_file_saved(app: &AppHandle, name: &str, size: u64, path: &PathBuf) {
    let _ = app.emit("file-received", file_received_payload(name, size, path));
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    log::info!("File received: {} ({}) -> {}", name, size, path.display());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_received_payload_contains_saved_path() {
        let payload = file_received_payload("a.txt", 3, std::path::Path::new("/tmp/a.txt"));

        assert_eq!(payload["name"], "a.txt");
        assert_eq!(payload["size"], 3);
        assert_eq!(payload["path"], "/tmp/a.txt");
    }
}
