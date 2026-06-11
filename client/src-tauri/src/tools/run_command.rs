use serde_json::Value;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_OUTPUT: usize = 16 * 1024; // 16KB
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub struct RunCommandTool {
    enabled: bool,
}

impl RunCommandTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("run_command")
                .copied()
                .unwrap_or(false),
        }
    }
}

impl Tool for RunCommandTool {
    fn name(&self) -> &'static str {
        "run_command"
    }

    fn description(&self) -> &'static str {
        "Execute a command via cmd.exe. Returns stdout and stderr. Timeout 60s, output capped at 16KB. \
         Use && to chain commands. For PowerShell, prefix with: powershell -Command \"...\""
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command line to execute via cmd /C. Combine with && for multi-step."
                }
            },
            "required": ["command"]
        })
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = Instant::now();
        let command = args
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if command.is_empty() {
            return ToolResult::err("command is required".to_string());
        }

        log::info!("[RunCommand] executing: {}", command);

        let mut cmd_builder = &mut Command::new("cmd");
        #[cfg(target_os = "windows")]
        {
            cmd_builder = cmd_builder.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = match cmd_builder
            .args(["/C", command])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                log::info!("[RunCommand] spawn failed: {}", e);
                return ToolResult::err(format!("Failed to spawn command: {}", e));
            }
        };

        // 手动超时控制: 每 50ms poll 一次，总超时 60s
        let deadline = Instant::now() + TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => break,
                Ok(None) => {
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        log::info!("[RunCommand] timed out after {:?}", TIMEOUT);
                        return ToolResult::err(format!(
                            "Command timed out after {}s",
                            TIMEOUT.as_secs()
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    log::info!("[RunCommand] try_wait error: {}", e);
                    return ToolResult::err(format!("Command error: {}", e));
                }
            }
        }

        let output = match child.wait_with_output() {
            Ok(o) => o,
            Err(e) => {
                log::info!("[RunCommand] wait_with_output failed: {}", e);
                return ToolResult::err(format!("Failed to read output: {}", e));
            }
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let mut parts = Vec::new();

        if !stdout.trim().is_empty() {
            let s = truncate(&stdout, MAX_OUTPUT, "stdout");
            parts.push(format!("stdout:\n{}", s));
        }
        if !stderr.trim().is_empty() {
            let s = truncate(&stderr, MAX_OUTPUT / 2, "stderr");
            parts.push(format!("stderr:\n{}", s));
        }

        log::info!(
            "[RunCommand] done: elapsed={:?}, exit={}, stdout={}, stderr={}",
            start.elapsed(),
            exit_code,
            stdout.len(),
            stderr.len(),
        );

        if parts.is_empty() {
            ToolResult::ok(format!("exit code: {}", exit_code))
        } else {
            let mut result = parts.join("\n\n");
            if exit_code != 0 {
                result.push_str(&format!("\n\nexit code: {}", exit_code));
            }
            // 非零退出码不算 error，LLM 需要自己根据 exit code 判断
            ToolResult::ok(result)
        }
    }
}

fn truncate(s: &str, max_bytes: usize, label: &str) -> String {
    if s.len() <= max_bytes {
        s.to_string()
    } else {
        // 在字符边界截断
        let end = s
            .char_indices()
            .take_while(|(i, _)| *i < max_bytes)
            .last()
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(max_bytes.min(s.len()));
        format!(
            "{}...\n[{label} truncated at {max_bytes} bytes]",
            &s[..end]
        )
    }
}
