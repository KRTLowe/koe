use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct WriteClipboardTool {
    enabled: bool,
}

impl WriteClipboardTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("write_clipboard")
                .copied()
                .unwrap_or(false),
        }
    }
}

impl Tool for WriteClipboardTool {
    fn name(&self) -> &'static str {
        "write_clipboard"
    }

    fn description(&self) -> &'static str {
        "Write text to the Windows clipboard"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to copy to the clipboard"
                }
            },
            "required": ["text"]
        })
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");

        if text.is_empty() {
            return ToolResult::err("text is required".to_string());
        }

        log::info!("[WriteClipboard] writing {} chars", text.len());

        match arboard::Clipboard::new() {
            Ok(mut clipboard) => match clipboard.set_text(text) {
                Ok(()) => {
                    log::info!("[WriteClipboard] done");
                    ToolResult::ok(format!("已复制到剪贴板 ({} 字符)", text.len()))
                }
                Err(e) => ToolResult::err(format!("Failed to set clipboard: {}", e)),
            },
            Err(e) => ToolResult::err(format!("Clipboard access failed: {}", e)),
        }
    }
}
