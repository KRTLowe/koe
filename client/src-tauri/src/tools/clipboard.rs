use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct ClipboardTool {
    enabled: bool,
}

impl ClipboardTool {
    pub fn new(config: &AppConfig) -> Self {
        let enabled = config
            .tool_permissions
            .get("get_clipboard")
            .copied()
            .unwrap_or(true);
        Self { enabled }
    }
}

impl Tool for ClipboardTool {
    fn name(&self) -> &'static str {
        "get_clipboard"
    }

    fn description(&self) -> &'static str {
        "Read the Windows clipboard content"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "format": {
                    "type": "string",
                    "enum": ["text"],
                    "description": "Clipboard format"
                }
            }
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
        log::info!("[ClipboardTool] execute begin: enabled={}", self.enabled);

        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("text");
        log::info!("[ClipboardTool] format={}", format);

        let result = match format {
            "text" => {
                match arboard::Clipboard::new() {
                    Ok(mut clipboard) => {
                        log::info!("[ClipboardTool] clipboard opened");
                        match clipboard.get_text() {
                            Ok(text) => {
                                if text.is_empty() {
                                    log::info!("[ClipboardTool] clipboard empty");
                                    ToolResult::ok("Clipboard is empty".to_string())
                                } else {
                                    log::info!("[ClipboardTool] clipboard text: {} chars", text.len());
                                    ToolResult::ok(text)
                                }
                            }
                            Err(e) => {
                                log::info!("[ClipboardTool] get_text failed: {}", e);
                                ToolResult::err(format!("Failed to read clipboard text: {}", e))
                            },
                        }
                    }
                    Err(e) => {
                        log::info!("[ClipboardTool] Clipboard::new failed: {}", e);
                        ToolResult::err(format!("Clipboard access failed: {}", e))
                    },
                }
            }
            "image" => {
                log::info!("[ClipboardTool] image format requested, unsupported");
                ToolResult::err(
                    "Clipboard image format is not yet supported, use format=\"text\" instead".to_string(),
                )
            }
            _ => {
                log::info!("[ClipboardTool] unknown format: {}", format);
                ToolResult::err(format!("Unknown clipboard format: {}", format))
            },
        };

        log::info!("[ClipboardTool] execute end: elapsed={:?}, is_error={}",
            start.elapsed(), result.is_error);
        result
    }
}
