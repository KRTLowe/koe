use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct PullFileTool {
    enabled: bool,
    guard: PathGuard,
}

impl PullFileTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("pull_file")
                .copied()
                .unwrap_or(true),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for PullFileTool {
    fn name(&self) -> &'static str {
        "pull_file"
    }

    fn description(&self) -> &'static str {
        "Pull a file from the client machine to the server"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute file path on the client"
                }
            },
            "required": ["path"]
        })
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = std::time::Instant::now();
        log::info!("[PullFileTool] execute begin: enabled={}", self.enabled);

        let result = (|| -> ToolResult {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return ToolResult::err("文件路径不能为空".to_string());
            }

            let canonical = match self.guard.check_read(path) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(e),
            };

            // Check file exists and is not a directory
            let meta = match std::fs::metadata(&canonical) {
                Ok(m) => m,
                Err(e) => return ToolResult::err(format!("无法访问文件: {}", e)),
            };
            if meta.is_dir() {
                return ToolResult::err("不支持拉取目录，请使用单个文件".to_string());
            }
            if meta.len() > 10 * 1024 * 1024 {
                return ToolResult::err("文件超过 10MB，暂不支持拉取大文件".to_string());
            }

            let path_str = canonical.to_string_lossy().to_string();
            let file_name = canonical
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());
            let size = meta.len();

            log::info!(
                "[PullFileTool] pulling: {} ({} bytes)",
                path_str, size
            );

            ToolResult::ok_with_upload(
                format!("正在从客户端拉取文件: {} ({} bytes)", file_name, size),
                path_str,
            )
        })();

        log::info!(
            "[PullFileTool] execute end: elapsed={:?}, is_error={}",
            start.elapsed(),
            result.is_error
        );
        result
    }
}
