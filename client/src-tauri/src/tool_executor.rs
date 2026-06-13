use serde_json::Value;
use tauri::Manager;

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: Vec<Value>,
    pub is_error: bool,
    pub upload_path: Option<String>,
}

impl ToolResult {
    pub fn ok(text: String) -> Self {
        Self {
            content: vec![serde_json::json!({"type": "text", "text": text})],
            is_error: false,
            upload_path: None,
        }
    }

    pub fn err(text: String) -> Self {
        Self {
            content: vec![serde_json::json!({"type": "text", "text": text})],
            is_error: true,
            upload_path: None,
        }
    }

    pub fn ok_with_upload(text: String, path: String) -> Self {
        Self {
            content: vec![serde_json::json!({"type": "text", "text": text})],
            is_error: false,
            upload_path: Some(path),
        }
    }
}

pub fn execute_tool(name: &str, args: &Value) -> ToolResult {
    log::info!("[ToolExecutor] execute_tool: name={}", name);
    log::debug!("[ToolExecutor] args: {}", args);

    // 用 catch_unwind 防止工具内部 panic 导致 Mutex 中毒
    let result = std::panic::catch_unwind(|| {
        if let Some(app) = crate::APP_HANDLE.get() {
            if let Some(state) = app.try_state::<crate::AppState>() {
                if let Ok(mgr) = state.tool_manager.lock() {
                    if let Some(ref mgr) = *mgr {
                        if let Some(result) = mgr.execute(name, args) {
                            log::info!(
                                "[ToolExecutor] tool executed: name={} is_error={} upload={:?}",
                                name,
                                result.is_error,
                                result.upload_path
                            );
                            return Some(ToolResult {
                                content: result
                                    .content
                                    .into_iter()
                                    .map(|c| serde_json::json!({"type": "text", "text": c.text}))
                                    .collect(),
                                is_error: result.is_error,
                                upload_path: result.upload_path,
                            });
                        } else {
                            log::info!("[ToolExecutor] tool not found in ToolManager: {}", name);
                        }
                    } else {
                        log::info!("[ToolExecutor] ToolManager not initialized");
                    }
                } else {
                    log::info!("[ToolExecutor] mutex poisoned");
                }
            } else {
                log::info!("[ToolExecutor] AppState not available");
            }
        } else {
            log::info!("[ToolExecutor] APP_HANDLE not set");
        }
        None
    });

    match result {
        Ok(Some(r)) => r,
        Ok(None) => ToolResult::err(format!("Unknown tool: {}", name)),
        Err(panic_info) => {
            let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                s.to_string()
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                s.clone()
            } else {
                "unknown panic".to_string()
            };
            log::error!("[ToolExecutor] tool panicked: {} (error={})", name, msg);
            ToolResult::err(format!("工具执行异常: {}", msg))
        }
    }
}
