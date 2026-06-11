use crate::tool_executor;
use crate::ws_client;
use tauri::{AppHandle, Emitter};

/// Copilot 工作模式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CopilotMode {
    Single,
    Continuous,
}

impl CopilotMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CopilotMode::Single => "single",
            CopilotMode::Continuous => "continuous",
        }
    }
}

// ── 窗口坐标 FFI ────────────────────────────────────

#[cfg(target_os = "windows")]
#[repr(C)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
extern "system" {
    fn GetForegroundWindow() -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut RECT) -> i32;
}

#[cfg(target_os = "windows")]
fn get_foreground_window_rect() -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            return None;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return None;
        }
        Some((rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top))
    }
}

#[cfg(not(target_os = "windows"))]
fn get_foreground_window_rect() -> Option<(i32, i32, i32, i32)> {
    None
}

// ── 信号 ────────────────────────────────────────────

pub async fn send_copilot_signal(
    signal_tx: &tokio::sync::mpsc::Sender<ws_client::SignalRequest>,
    question: String,
    uia_tree: String,
    mode: CopilotMode,
    window_rect: Option<(i32, i32, i32, i32)>,
) {
    let mut data = serde_json::json!({
        "question": question,
        "uia_tree": uia_tree,
        "mode": mode.as_str(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    if let Some((x, y, w, h)) = window_rect {
        data["window_rect"] = serde_json::json!({
            "x": x, "y": y, "width": w, "height": h,
        });
    }

    let req = ws_client::SignalRequest {
        name: "copilot_query".to_string(),
        sticky: mode == CopilotMode::Continuous,
        priority: "high".to_string(),
        notify_once: mode == CopilotMode::Continuous,
        data,
    };

    if let Err(e) = signal_tx.try_send(req) {
        log::warn!("Copilot signal dropped (channel full): {}", e);
    }
}

pub async fn get_uia_tree() -> String {
    let args = serde_json::json!({});
    let result = tool_executor::execute_tool("get_uia_tree", &args);

    if result.is_error {
        let err_msg = result
            .content
            .first()
            .and_then(|c| c.get("text"))
            .and_then(|t| t.as_str())
            .unwrap_or("UIA tree unavailable");
        return format!("[UIA 采集失败] {}", err_msg);
    }

    result
        .content
        .first()
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("(empty tree)")
        .to_string()
}

pub async fn execute_copilot(
    app: &AppHandle,
    signal_tx: &tokio::sync::mpsc::Sender<ws_client::SignalRequest>,
    question: String,
    mode: CopilotMode,
) {
    log::info!(
        "Copilot execute: mode={} question={}",
        mode.as_str(),
        question
    );

    let uia_tree = get_uia_tree().await;
    log::info!("Copilot UIA tree: {} chars", uia_tree.len());

    let window_rect = get_foreground_window_rect();
    if let Some((x, y, w, h)) = window_rect {
        log::info!("Copilot window rect: x={} y={} w={} h={}", x, y, w, h);
    }

    send_copilot_signal(signal_tx, question.clone(), uia_tree, mode, window_rect).await;
    log::info!("Copilot signal sent");

    let _ = app.emit(
        "copilot-status",
        serde_json::json!({
            "status": "sent",
            "question": question,
            "mode": mode.as_str(),
        }),
    );

    if mode == CopilotMode::Single {
        let app_handle = app.clone();
        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            let _ = app_handle.emit("copilot-close", serde_json::json!({}));
        });
    }
}

pub async fn cancel_copilot(
    signal_tx: &tokio::sync::mpsc::Sender<ws_client::SignalRequest>,
) {
    let clear_req = ws_client::SignalRequest {
        name: "__copilot_clear__".to_string(),
        sticky: false,
        priority: "critical".to_string(),
        notify_once: false,
        data: serde_json::json!({"clear_signal": "copilot_query"}),
    };
    if let Err(e) = signal_tx.try_send(clear_req) {
        log::warn!("Copilot cancel signal dropped (channel full): {}", e);
    }
    log::info!("Copilot cancelled");
}
