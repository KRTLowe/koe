use std::time::Instant;

use raw_window_handle::HasWindowHandle;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder};

use crate::AppState;

const BUBBLE_WIDTH: f64 = 338.0;
const BUBBLE_GAP: f64 = 8.0;
const BUBBLE_COLUMN_GAP: f64 = 16.0;
const MIN_SCREEN_TOP: f64 = 20.0;

#[derive(Clone)]
pub(crate) struct BubbleInfo {
    pub(crate) label: String,
    pub(crate) height: f64,
}

#[tauri::command]
pub(crate) fn take_bubble_content(
    label: String,
    state: tauri::State<AppState>,
) -> Result<String, String> {
    state
        .bubble_content
        .lock()
        .map_err(|e| e.to_string())?
        .remove(&label)
        .ok_or("no content".to_string())
}

fn anchor_xy(app: &AppHandle) -> (f64, f64) {
    if let Some(float) = app.get_webview_window("kaya-float") {
        let pos = float.inner_position().ok();
        let size = float.inner_size().ok();
        let s = size.unwrap_or(PhysicalSize::new(320, 320));
        let logical_h = s.height as f64 / float.scale_factor().unwrap_or(1.0);
        (
            pos.map(|p| p.x as f64).unwrap_or(0.0),
            pos.map(|p| p.y as f64).unwrap_or(0.0) + logical_h * 0.33,
        )
    } else {
        (0.0, 100.0)
    }
}

fn reposition_all(app: &AppHandle) {
    let (float_x, anchor_y) = anchor_xy(app);
    let base_x = float_x - BUBBLE_WIDTH - BUBBLE_GAP + 80.0;

    let positions: Vec<(String, f64, f64)> = {
        let state = app.state::<AppState>();
        let bubbles = state.active_bubbles.lock().unwrap();
        layout_positions(&bubbles, base_x, anchor_y)
    };

    if positions.is_empty() {
        return;
    }

    for (label, x, y) in &positions {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_position(PhysicalPosition::new(*x as i32, *y as i32));
        }
    }
}

fn layout_positions(bubbles: &[BubbleInfo], base_x: f64, anchor_y: f64) -> Vec<(String, f64, f64)> {
    let mut positions = Vec::with_capacity(bubbles.len());
    let mut col = 0;
    let mut col_y = anchor_y;

    for bubble in bubbles.iter().rev() {
        col_y -= bubble.height;
        positions.push((
            bubble.label.clone(),
            base_x - col as f64 * (BUBBLE_WIDTH + BUBBLE_COLUMN_GAP),
            col_y,
        ));
        col_y -= BUBBLE_GAP;

        if col_y < MIN_SCREEN_TOP + BUBBLE_GAP {
            col += 1;
            col_y = anchor_y;
        }
    }

    positions.reverse();
    positions
}

#[tauri::command]
pub(crate) fn resize_bubble(
    app: tauri::AppHandle,
    label: String,
    height: f64,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    {
        let mut bubbles = state.active_bubbles.lock().map_err(|e| e.to_string())?;
        if let Some(bubble) = bubbles.iter_mut().find(|bubble| bubble.label == label) {
            bubble.height = height;
        }
    }

    if let Some(window) = app.get_webview_window(&label) {
        window
            .set_size(PhysicalSize::new(338.0, height))
            .map_err(|e| e.to_string())?;
    }

    reposition_all(&app);
    Ok(())
}

const MAX_BUBBLES: usize = 10;

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

pub(crate) fn create_message_bubble(app: &AppHandle, content: &str) -> String {
    let state = app.state::<AppState>();

    let seq = {
        let mut seq = state.bubble_seq.lock().unwrap();
        *seq += 1;
        *seq
    };
    let label = format!("bubble-{}", seq);
    log::debug!(
        "[bubble] create requested: label={} len={} preview={}",
        label,
        content.len(),
        safe_preview(content, 120),
    );
    state
        .bubble_content
        .lock()
        .unwrap()
        .insert(label.clone(), content.to_string());

    // 限流：超出 MAX_BUBBLES 时移除最旧的气泡
    {
        let mut bubbles = state.active_bubbles.lock().unwrap();
        bubbles.push(BubbleInfo {
            label: label.clone(),
            height: 40.0,
        });
        while bubbles.len() > MAX_BUBBLES {
            if let Some(old) = bubbles.first().cloned() {
                drop(bubbles);
                close_bubble_by_label(app, &old.label);
                bubbles = state.active_bubbles.lock().unwrap();
            } else {
                break;
            }
        }
    }
    *state.last_msg_time.lock().unwrap() = Some(Instant::now());

    if let Ok(window) = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("bubble".into()))
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .inner_size(BUBBLE_WIDTH, 40.0)
        .position(0.0, 0.0)
        .visible(false)
        .build()
    {
        #[cfg(target_os = "windows")]
        show_quietly(&window);
        #[cfg(not(target_os = "windows"))]
        let _ = window.show();
    }
    log::info!("[bubble] created: label={}", label);

    reposition_all(app);

    label
}

pub(crate) fn close_bubble_by_label(app: &AppHandle, label: &str) {
    log::info!("[bubble] close requested: label={}", label);
    let state = app.state::<AppState>();
    state.bubble_content.lock().unwrap().remove(label);
    state
        .active_bubbles
        .lock()
        .unwrap()
        .retain(|bubble| bubble.label != label);
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.close();
        log::info!("[bubble] closed window: label={}", label);
    } else {
        log::info!("[bubble] close skipped, window not found: label={}", label);
    }
}

fn safe_preview(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::{layout_positions, BubbleInfo};

    #[test]
    fn layout_positions_stacks_newest_near_anchor_and_wraps_columns() {
        let bubbles = vec![
            BubbleInfo {
                label: "old".to_string(),
                height: 40.0,
            },
            BubbleInfo {
                label: "older-middle".to_string(),
                height: 40.0,
            },
            BubbleInfo {
                label: "middle".to_string(),
                height: 40.0,
            },
            BubbleInfo {
                label: "new".to_string(),
                height: 40.0,
            },
        ];

        let positions = layout_positions(&bubbles, 100.0, 130.0);

        assert_eq!(
            positions,
            vec![
                ("old".to_string(), -254.0, 90.0),
                ("older-middle".to_string(), 100.0, -6.0),
                ("middle".to_string(), 100.0, 42.0),
                ("new".to_string(), 100.0, 90.0),
            ],
        );
    }
}
