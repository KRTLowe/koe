use serde_json::Value;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

const DEFAULT_TIMEOUT_MS: u64 = 30_000;
const POLL_INTERVAL: Duration = Duration::from_millis(50);
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
        "Execute a short-lived platform shell command and wait for completion. Captures stdout/stderr and returns JSON {stdout, stderr, exit_code}. Use this when you need command output or exit status. Supports timeout in milliseconds and optional cwd. For long-running programs, GUI apps, services, or commands that should keep running, use start_process instead."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Command line to execute through the platform shell (Windows: cmd /C, Linux/macOS: sh -c)."
                },
                "timeout": {
                    "type": "number",
                    "description": "Timeout in milliseconds. Defaults to 30000. On timeout, the process is killed and exit_code is -1."
                },
                "cwd": {
                    "type": "string",
                    "description": "Working directory for the command. Optional."
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
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");

        if command.trim().is_empty() {
            return ToolResult::err("command is required".to_string());
        }

        let timeout = match parse_timeout(args) {
            Ok(timeout) => timeout,
            Err(message) => return ToolResult::err(message),
        };

        let cwd = match args.get("cwd") {
            Some(Value::String(path)) => Some(path.as_str()),
            Some(_) => return ToolResult::err("cwd must be a string".to_string()),
            None => None,
        };

        log::info!(
            "[RunCommand] executing: {}, timeout={:?}, cwd={:?}",
            command,
            timeout,
            cwd
        );

        let mut cmd_builder = shell_command(command);
        #[cfg(target_os = "windows")]
        {
            cmd_builder.creation_flags(CREATE_NO_WINDOW);
        }

        cmd_builder
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null());

        if let Some(dir) = cwd {
            cmd_builder.current_dir(dir);
        }

        let mut child = match cmd_builder.spawn() {
            Ok(c) => c,
            Err(e) => {
                log::info!("[RunCommand] spawn failed: {}", e);
                return ToolResult::err(format!("Failed to spawn command: {}", e));
            }
        };

        let stdout_reader = child.stdout.take().map(read_output);
        let stderr_reader = child.stderr.take().map(read_output);
        let deadline = Instant::now() + timeout;
        let exit_code = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status.code().unwrap_or(-1),
                Ok(None) => {
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        log::info!("[RunCommand] timed out after {:?}", timeout);
                        break -1;
                    }
                    thread::sleep(POLL_INTERVAL);
                }
                Err(e) => {
                    let _ = child.kill();
                    log::info!("[RunCommand] try_wait error: {}", e);
                    return ToolResult::err(format!("Command error: {}", e));
                }
            }
        };

        let stdout = match join_output(stdout_reader) {
            Ok(stdout) => stdout,
            Err(message) => return ToolResult::err(message),
        };
        let stderr = match join_output(stderr_reader) {
            Ok(stderr) => stderr,
            Err(message) => return ToolResult::err(message),
        };

        let result = match serde_json::to_string(&serde_json::json!({
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": exit_code,
        })) {
            Ok(result) => result,
            Err(e) => {
                log::info!("[RunCommand] JSON serialization failed: {}", e);
                return ToolResult::err(format!("Failed to serialize command result: {}", e));
            }
        };

        log::info!(
            "[RunCommand] done: elapsed={:?}, exit={}",
            start.elapsed(),
            exit_code,
        );

        ToolResult::ok(result)
    }
}

fn parse_timeout(args: &Value) -> Result<Duration, String> {
    match args.get("timeout") {
        Some(Value::Number(number)) => {
            let millis = number
                .as_u64()
                .ok_or_else(|| "timeout must be a positive integer in milliseconds".to_string())?;
            if millis == 0 {
                return Err("timeout must be greater than 0".to_string());
            }
            Ok(Duration::from_millis(millis))
        }
        Some(_) => Err("timeout must be a positive integer in milliseconds".to_string()),
        None => Ok(Duration::from_millis(DEFAULT_TIMEOUT_MS)),
    }
}

fn shell_command(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", command]);
        cmd
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", command]);
        cmd
    }
}

fn read_output<R>(mut pipe: R) -> JoinHandle<Result<String, String>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes)
            .map_err(|e| format!("Failed to read command output: {}", e))?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    })
}

fn join_output(reader: Option<JoinHandle<Result<String, String>>>) -> Result<String, String> {
    match reader {
        Some(handle) => match handle.join() {
            Ok(result) => result,
            Err(_) => Err("Failed to read command output: reader thread panicked".to_string()),
        },
        None => Ok(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::{RunCommandTool, Tool};
    use serde_json::Value;
    use std::env;

    fn tool() -> RunCommandTool {
        RunCommandTool { enabled: true }
    }

    fn result_json(text: &str) -> Value {
        serde_json::from_str(text).expect("run_command should return JSON text")
    }

    #[cfg(target_os = "windows")]
    fn success_command() -> &'static str {
        "echo kaya-run-command"
    }

    #[cfg(not(target_os = "windows"))]
    fn success_command() -> &'static str {
        "printf kaya-run-command"
    }

    #[cfg(target_os = "windows")]
    fn nonzero_command() -> &'static str {
        "exit /B 7"
    }

    #[cfg(not(target_os = "windows"))]
    fn nonzero_command() -> &'static str {
        "exit 7"
    }

    #[cfg(target_os = "windows")]
    fn print_cwd_command() -> &'static str {
        "cd"
    }

    #[cfg(not(target_os = "windows"))]
    fn print_cwd_command() -> &'static str {
        "pwd"
    }

    #[cfg(target_os = "windows")]
    fn slow_command() -> &'static str {
        "ping -n 3 127.0.0.1 > nul"
    }

    #[cfg(not(target_os = "windows"))]
    fn slow_command() -> &'static str {
        "sleep 2"
    }

    #[cfg(target_os = "windows")]
    fn large_output_command() -> &'static str {
        "powershell -NoProfile -Command \"Write-Output ('x' * 20000)\""
    }

    #[cfg(not(target_os = "windows"))]
    fn large_output_command() -> &'static str {
        "python3 -c 'print(\"x\" * 20000)'"
    }

    #[test]
    fn schema_accepts_timeout_and_cwd() {
        let schema = tool().input_schema();

        assert!(schema["properties"].get("command").is_some());
        assert!(schema["properties"].get("timeout").is_some());
        assert!(schema["properties"].get("cwd").is_some());
    }

    #[test]
    fn run_command_returns_json_for_success() {
        let result = tool().execute(&serde_json::json!({
            "command": success_command(),
            "timeout": 30000
        }));

        assert!(!result.is_error);
        let body = result_json(&result.content[0].text);
        assert_eq!(body["stdout"].as_str().unwrap().trim(), "kaya-run-command");
        assert_eq!(body["stderr"], "");
        assert_eq!(body["exit_code"], 0);
    }

    #[test]
    fn run_command_preserves_nonzero_exit_code_without_tool_error() {
        let result = tool().execute(&serde_json::json!({
            "command": nonzero_command(),
            "timeout": 30000
        }));

        assert!(!result.is_error);
        let body = result_json(&result.content[0].text);
        assert_eq!(body["exit_code"], 7);
    }

    #[test]
    fn run_command_uses_cwd() {
        let cwd = env::temp_dir();
        let result = tool().execute(&serde_json::json!({
            "command": print_cwd_command(),
            "timeout": 30000,
            "cwd": cwd.to_string_lossy()
        }));

        assert!(!result.is_error);
        let body = result_json(&result.content[0].text);
        assert_eq!(
            body["stdout"].as_str().unwrap().trim(),
            cwd.to_string_lossy()
        );
        assert_eq!(body["exit_code"], 0);
    }

    #[test]
    fn run_command_returns_timeout_json() {
        let result = tool().execute(&serde_json::json!({
            "command": slow_command(),
            "timeout": 50
        }));

        assert!(!result.is_error);
        let body = result_json(&result.content[0].text);
        assert_eq!(body["exit_code"], -1);
    }

    #[test]
    fn run_command_does_not_truncate_large_stdout() {
        let result = tool().execute(&serde_json::json!({
            "command": large_output_command(),
            "timeout": 30000
        }));

        assert!(!result.is_error);
        let body = result_json(&result.content[0].text);
        assert_eq!(body["stdout"].as_str().unwrap().trim().len(), 20_000);
        assert_eq!(body["exit_code"], 0);
    }
}
