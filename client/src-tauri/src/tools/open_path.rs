use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct OpenPathTool {
    enabled: bool,
}

impl OpenPathTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("open_path")
                .copied()
                .unwrap_or(true),
        }
    }
}

impl Tool for OpenPathTool {
    fn name(&self) -> &'static str {
        "open_path"
    }

    fn description(&self) -> &'static str {
        "Open a file, folder, or URL with the default Windows program"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "File path, folder path, or URL to open"
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
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

        if path.is_empty() {
            return ToolResult::err("path is required".to_string());
        }

        log::info!("[OpenPath] opening: {}", path);

        match open::that(path) {
            Ok(()) => {
                log::info!("[OpenPath] done");
                ToolResult::ok(format!("已打开: {}", path))
            }
            Err(e) => ToolResult::err(format!("打开失败: {}", e)),
        }
    }
}
