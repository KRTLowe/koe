use serde_json::Value;

use super::path_guard::PathGuard;
use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct WriteFileTool {
    enabled: bool,
    guard: PathGuard,
}

impl WriteFileTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("write_text_file")
                .copied()
                .unwrap_or(false),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for WriteFileTool {
    fn name(&self) -> &'static str {
        "write_text_file"
    }
    fn description(&self) -> &'static str {
        "Write or append text content to a file. Default mode is append for safety."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file path" },
                "content": { "type": "string", "description": "Text content to write" },
                "mode": { "type": "string", "enum": ["write", "append"], "description": "write=overwrite, append=add to end (default: append)" },
                "create_dirs": { "type": "boolean", "description": "Auto-create parent directories (default: false)" }
            },
            "required": ["path", "content"]
        })
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, e: bool) {
        self.enabled = e;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = std::time::Instant::now();
        log::info!("[WriteFileTool] execute begin: enabled={}", self.enabled);

        let result = (|| -> ToolResult {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let mode = args
                .get("mode")
                .and_then(|v| v.as_str())
                .unwrap_or("append");
            let create_dirs = args
                .get("create_dirs")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if content.len() > 10 * 1024 * 1024 {
                return ToolResult::err("文件太大，单次写入上限为 10MB".to_string());
            }

            let target = match self.guard.check_write(path) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(e),
            };

            if create_dirs {
                if let Some(parent) = target.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
            }

            let result = match mode {
                "write" => std::fs::write(&target, content),
                _ => {
                    use std::io::Write;
                    let mut file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&target);
                    match file {
                        Ok(ref mut f) => f.write_all(content.as_bytes()),
                        Err(e) => return ToolResult::err(format!("无法打开文件追加: {}", e)),
                    }
                }
            };

            match result {
                Ok(_) => ToolResult::ok(format!(
                    "已{}到 {} ({} bytes)",
                    if mode == "write" { "写入" } else { "追加" },
                    target.display(),
                    content.len()
                )),
                Err(e) => ToolResult::err(format!("写入失败: {}", e)),
            }
        })();

        log::info!(
            "[WriteFileTool] execute end: elapsed={:?}, is_error={}",
            start.elapsed(),
            result.is_error
        );
        result
    }
}
