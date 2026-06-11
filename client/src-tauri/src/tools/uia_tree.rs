use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use crate::uia_tree;

pub struct UiaTreeTool {
    enabled: bool,
}

impl UiaTreeTool {
    pub fn new(config: &AppConfig) -> Self {
        let enabled = config
            .tool_permissions
            .get("get_uia_tree")
            .copied()
            .unwrap_or(true);
        Self { enabled }
    }
}

impl Tool for UiaTreeTool {
    fn name(&self) -> &'static str {
        "get_uia_tree"
    }

    fn description(&self) -> &'static str {
        "Get the UI Automation accessibility tree for the foreground window. \
         Returns control type, name, bounding rect, and state flags for each \
         element. Works with native Win32, WPF, Qt, and Electron apps that \
         implement UIA providers."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum tree depth (1-12, default: 6)",
                    "default": 6,
                    "minimum": 1,
                    "maximum": 12,
                },
                "max_items": {
                    "type": "integer",
                    "description": "Maximum elements to return (1-500, default: 120)",
                    "default": 120,
                    "minimum": 1,
                    "maximum": 500,
                },
            },
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
        log::info!("[UiaTreeTool] execute begin: enabled={}", self.enabled);

        let max_depth = args
            .get("max_depth")
            .and_then(|v| v.as_i64())
            .unwrap_or(6) as i32;
        let max_items = args
            .get("max_items")
            .and_then(|v| v.as_u64())
            .unwrap_or(120) as usize;

        let result = match uia_tree::get_uia_tree(max_depth, max_items) {
            Ok(tree) => {
                log::info!("[UiaTreeTool] tree extracted: {} chars", tree.len());
                ToolResult::ok(tree)
            }
            Err(e) => {
                log::info!("[UiaTreeTool] failed: {}", e);
                ToolResult::err(format!("UIA tree unavailable: {}", e))
            }
        };

        log::info!(
            "[UiaTreeTool] execute end: elapsed={:?}, is_error={}",
            start.elapsed(),
            result.is_error
        );
        result
    }
}
