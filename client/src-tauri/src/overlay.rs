use raw_window_handle::HasWindowHandle;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::{ws_client, AppState};

#[cfg(target_os = "windows")]
mod win32 {
    extern "system" {
        pub fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
        pub fn GetWindowLongPtrW(hWnd: isize, nIndex: i32) -> isize;
        pub fn SetWindowLongPtrW(hWnd: isize, nIndex: i32, dwNewLong: isize) -> isize;
    }
    pub const SW_SHOWNOACTIVATE: i32 = 4;
    pub const GWL_EXSTYLE: i32 = -20;
    pub const WS_EX_NOACTIVATE: isize = 0x08000000;
}

#[cfg(target_os = "windows")]
fn show_quietly(window: &impl HasWindowHandle) {
    if let Ok(handle) = window.window_handle() {
        let raw = handle.as_raw();
        if let raw_window_handle::RawWindowHandle::Win32(win32_handle) = raw {
            let hwnd = win32_handle.hwnd.get() as isize;
            unsafe {
                let ex = win32::GetWindowLongPtrW(hwnd, win32::GWL_EXSTYLE);
                win32::SetWindowLongPtrW(hwnd, win32::GWL_EXSTYLE, ex | win32::WS_EX_NOACTIVATE);
                win32::ShowWindow(hwnd, win32::SW_SHOWNOACTIVATE);
            }
        }
    }
}

pub(crate) fn toggle_copilot_window(app: &AppHandle, mode: &str) {
    if let Some(window) = app.get_webview_window("copilot-overlay") {
        if let Some(state) = app.try_state::<AppState>() {
            if let Ok(tx) = state.signal_tx.lock() {
                if let Some(tx) = tx.as_ref() {
                    let clear_req = ws_client::SignalRequest {
                        name: "__copilot_clear__".to_string(),
                        sticky: false,
                        priority: "critical".to_string(),
                        notify_once: false,
                        data: serde_json::json!({"clear_signal": "copilot_query"}),
                    };
                    let _ = tx.try_send(clear_req);
                }
            }
            if let Ok(tx) = state.acp_tx.lock() {
                if let Some(tx) = tx.as_ref() {
                    let _ = tx.try_send("__cancel__".to_string());
                }
            }
        }
        let _ = window.close();
        return;
    }

    let (x, y) = if let Ok(Some(monitor)) = app.primary_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let logical_width = size.width as f64 / scale;
        let center_x = (logical_width - 560.0) / 2.0;
        (center_x, 200.0)
    } else {
        (400.0, 200.0)
    };

    if let Ok(window) = WebviewWindowBuilder::new(
        app,
        "copilot-overlay",
        WebviewUrl::App(format!("copilot?mode={}", mode).into()),
    )
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .inner_size(560.0, 110.0)
    .position(x, y)
    .visible(false)
    .build()
    {
        #[cfg(target_os = "windows")]
        show_quietly(&window);
        #[cfg(not(target_os = "windows"))]
        let _ = window.show();
    }
}

#[tauri::command]
pub(crate) fn copilot_enter_monitor(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("copilot-overlay")
        .ok_or("Copilot window not found")?;

    let monitor = app
        .primary_monitor()
        .ok()
        .flatten()
        .ok_or("No monitor found")?;

    let screen_size = monitor.size();
    let window_width = 320i32;
    let padding = 20i32;
    let x = screen_size.width as i32 - window_width - padding;

    window
        .set_position(PhysicalPosition::new(x, 24))
        .map_err(|e| e.to_string())?;
    window
        .set_size(PhysicalSize::new(window_width, 56))
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub(crate) fn copilot_close(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("copilot-overlay") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn show_tool_call_overlay(app: &AppHandle, status: &str, name: &str) {
    if let Some(window) = app.get_webview_window("tool-call-overlay") {
        let _ = window.eval(&format!(
            "window.location.href = 'tool-call?status={}&name={}'",
            status, name,
        ));
        return;
    }
    let (width, height, x, y) = if let Ok(Some(monitor)) = app.primary_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let logical_width = size.width as f64 / scale;
        (280.0, 52.0, logical_width - 280.0 - 16.0, 12.0)
    } else {
        (280.0, 52.0, 400.0, 12.0)
    };

    if let Ok(window) = WebviewWindowBuilder::new(
        app,
        "tool-call-overlay",
        WebviewUrl::App(format!("tool-call?status={}&name={}", status, name).into()),
    )
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .shadow(false)
    .inner_size(width, height)
    .position(x, y)
    .visible(false)
    .build()
    {
        #[cfg(target_os = "windows")]
        show_quietly(&window);
        #[cfg(not(target_os = "windows"))]
        let _ = window.show();
    }
}

#[tauri::command]
pub(crate) fn close_tool_call_overlay(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("tool-call-overlay") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn toggle_quick_chat(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("quick-chat") {
        let _ = window.close();
        return;
    }

    let (x, y) = if let Ok(Some(monitor)) = app.primary_monitor() {
        let logical_width = monitor.size().width as f64 / monitor.scale_factor();
        let logical_height = monitor.size().height as f64 / monitor.scale_factor();
        (
            (logical_width - 800.0) / 2.0,
            (logical_height - 200.0) / 3.0,
        )
    } else {
        (200.0, 200.0)
    };

    if let Ok(window) =
        WebviewWindowBuilder::new(app, "quick-chat", WebviewUrl::App("quick-chat".into()))
            .title("kaya-is-listen-to-you")
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .resizable(false)
            .shadow(false)
            .inner_size(800.0, 200.0)
            .position(x, y)
            .build()
    {
        let _ = window.set_focus();
    }
}

#[tauri::command]
pub(crate) fn quick_chat_close(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("quick-chat") {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}
