use std::time::Instant;

use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindowBuilder,
};

use crate::AppState;

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
    let gap = 8.0;
    let col_gap = 16.0;
    let bw = 338.0;
    let min_top = 20.0;
    let (float_x, anchor_y) = anchor_xy(app);
    let base_x = float_x - bw - gap + 80.0;

    let positions: Vec<(String, f64, f64)> = {
        let state = app.state::<AppState>();
        let bubbles = state.active_bubbles.lock().unwrap();
        if bubbles.is_empty() {
            return;
        }

        let mut result = Vec::with_capacity(bubbles.len());
        let mut col = 0;
        let mut col_y = anchor_y;

        for b in bubbles.iter().rev() {
            col_y -= b.height;
            result.push((label_clone(b), base_x - col as f64 * (bw + col_gap), col_y));
            col_y -= gap;

            if col_y < min_top + gap {
                col += 1;
                col_y = anchor_y;
            }
        }
        result.reverse();
        result
    };

    for (label, x, y) in &positions {
        if let Some(win) = app.get_webview_window(label) {
            let _ = win.set_position(PhysicalPosition::new(*x as i32, *y as i32));
        }
    }
}

fn label_clone(bubble: &BubbleInfo) -> String {
    bubble.label.clone()
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

pub(crate) fn create_message_bubble(app: &AppHandle, content: &str) -> String {
    let bubble_width = 338.0;
    let state = app.state::<AppState>();

    let seq = {
        let mut seq = state.bubble_seq.lock().unwrap();
        *seq += 1;
        *seq
    };
    let label = format!("bubble-{}", seq);
    state
        .bubble_content
        .lock()
        .unwrap()
        .insert(label.clone(), content.to_string());

    state.active_bubbles.lock().unwrap().push(BubbleInfo {
        label: label.clone(),
        height: 40.0,
    });
    *state.last_msg_time.lock().unwrap() = Some(Instant::now());

    let _ = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("bubble".into()))
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .inner_size(bubble_width, 40.0)
        .position(0.0, 0.0)
        .visible(false)
        .build();
    if let Some(win) = app.get_webview_window(&label) {
        let _ = win.show();
    }

    reposition_all(app);

    label
}

pub(crate) fn close_bubble_by_label(app: &AppHandle, label: &str) {
    let state = app.state::<AppState>();
    state.bubble_content.lock().unwrap().remove(label);
    state
        .active_bubbles
        .lock()
        .unwrap()
        .retain(|bubble| bubble.label != label);
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.close();
    }
}
