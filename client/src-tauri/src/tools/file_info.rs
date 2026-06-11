use serde_json::Value;
use chrono::{DateTime, Local};

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct FileInfoTool {
    enabled: bool,
    guard: PathGuard,
}

impl FileInfoTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config.tool_permissions.get("get_file_info").copied().unwrap_or(true),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for FileInfoTool {
    fn name(&self) -> &'static str { "get_file_info" }
    fn description(&self) -> &'static str { "Get metadata for a file or directory" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file or directory path" }
            },
            "required": ["path"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = std::time::Instant::now();
        log::info!("[FileInfoTool] execute begin: enabled={}", self.enabled);

        let result = (|| -> ToolResult {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

            let canonical = match self.guard.check_read(path) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(e),
            };

            let meta = match std::fs::metadata(&canonical) {
                Ok(m) => m,
                Err(e) => return ToolResult::err(format!("无法访问: {}", e)),
            };

            let is_dir = meta.is_dir();
            let size = meta.len();
            let modified = meta.modified().ok()
                .map(|t| {
                    let dt: DateTime<Local> = t.into();
                    dt.format("%Y-%m-%d %H:%M:%S").to_string()
                })
                .unwrap_or_else(|| "(unknown)".to_string());
            let ext = canonical.extension()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_else(|| "(none)".to_string());

            let kind = if is_dir { "📁 目录" } else { "📄 文件" };
            let text = format!(
                "类型: {}\n路径: {}\n大小: {} bytes\n修改时间: {}\n扩展名: {}",
                kind, canonical.display(), size, modified, ext
            );

            ToolResult::ok(text)
        })();

        log::info!("[FileInfoTool] execute end: elapsed={:?}, is_error={}", start.elapsed(), result.is_error);
        result
    }
}
