use serde_json::Value;
use std::process::Command;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct KillProcessTool {
    enabled: bool,
}

impl KillProcessTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("kill_process")
                .copied()
                .unwrap_or(false),
        }
    }
}

impl Tool for KillProcessTool {
    fn name(&self) -> &'static str {
        "kill_process"
    }

    fn description(&self) -> &'static str {
        "Kill a process by PID or by executable name. Uses taskkill /F."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pid": {
                    "type": "integer",
                    "description": "Process ID to kill"
                },
                "name": {
                    "type": "string",
                    "description": "Executable name to kill (e.g. notepad.exe)"
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
        let pid = args.get("pid").and_then(|v| v.as_u64());
        let name = args.get("name").and_then(|v| v.as_str());

        if pid.is_none() && name.is_none() {
            return ToolResult::err("pid or name is required".to_string());
        }

        let mut cmd = Command::new("taskkill");
        cmd.arg("/F");

        if let Some(p) = pid {
            log::info!("[KillProcess] killing PID {}", p);
            cmd.args(["/PID", &p.to_string()]);
        } else if let Some(n) = name {
            log::info!("[KillProcess] killing {}", n);
            cmd.args(["/IM", n]);
        }

        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let combined = format!("{}{}", stdout.trim(), stderr.trim());

                if output.status.success() {
                    log::info!("[KillProcess] done: {}", combined);
                    ToolResult::ok(if combined.is_empty() {
                        "进程已终止".to_string()
                    } else {
                        combined
                    })
                } else {
                    log::info!("[KillProcess] failed: {}", combined);
                    ToolResult::err(combined)
                }
            }
            Err(e) => ToolResult::err(format!("taskkill 执行失败: {}", e)),
        }
    }
}
