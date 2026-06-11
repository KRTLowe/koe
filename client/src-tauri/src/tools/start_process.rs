use serde_json::Value;
use std::process::{Command, Stdio};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct StartProcessTool {
    enabled: bool,
}

impl StartProcessTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("start_process")
                .copied()
                .unwrap_or(false),
        }
    }
}

impl Tool for StartProcessTool {
    fn name(&self) -> &'static str {
        "start_process"
    }

    fn description(&self) -> &'static str {
        "Launch a program without waiting. Returns PID. Use kill_process to stop it."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the executable (e.g. notepad.exe or C:\\path\\to\\app.exe)"
                },
                "args": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Command-line arguments (optional)"
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory (optional)"
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
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if path.is_empty() {
            return ToolResult::err("path is required".to_string());
        }

        let cli_args: Vec<String> = args
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let cwd = args.get("cwd").and_then(|v| v.as_str());

        log::info!(
            "[StartProcess] path={}, args={:?}, cwd={:?}",
            path, cli_args, cwd
        );

        let mut cmd = Command::new(path);
        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd.args(&cli_args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null());

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                log::info!("[StartProcess] started: pid={}", pid);
                ToolResult::ok(format!("已启动: {} (PID: {})", path, pid))
            }
            Err(e) => ToolResult::err(format!("启动失败: {}", e)),
        }
    }
}
