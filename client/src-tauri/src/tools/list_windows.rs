use serde_json::Value;
use std::sync::Mutex;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

pub struct ListWindowsTool {
    enabled: bool,
}

impl ListWindowsTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("list_windows")
                .copied()
                .unwrap_or(true),
        }
    }
}

// ── Win32 FFI ───────────────────────────────────────

#[cfg(target_os = "windows")]
#[repr(C)]
struct RECT {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
struct WindowEntry {
    title: String,
    hwnd: isize,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    pid: u32,
}

#[cfg(target_os = "windows")]
extern "system" {
    fn EnumWindows(cb: unsafe extern "system" fn(isize, isize) -> i32, lp: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: isize) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut RECT) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
}

#[cfg(target_os = "windows")]
static WINDOW_LIST: Mutex<Vec<WindowEntry>> = Mutex::new(Vec::new());

#[cfg(target_os = "windows")]
unsafe extern "system" fn enum_proc(hwnd: isize, _lp: isize) -> i32 {
    if IsWindowVisible(hwnd) == 0 {
        return 1; // continue
    }

    let len = GetWindowTextLengthW(hwnd);
    if len == 0 {
        return 1;
    }

    let mut buf: Vec<u16> = vec![0; (len + 1) as usize];
    GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
    let title = String::from_utf16_lossy(&buf[..len as usize]).trim().to_string();

    if title.is_empty() {
        return 1;
    }

    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    GetWindowRect(hwnd, &mut rect);

    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);

    if let Ok(mut list) = WINDOW_LIST.lock() {
        list.push(WindowEntry {
            title,
            hwnd,
            x: rect.left,
            y: rect.top,
            w: rect.right - rect.left,
            h: rect.bottom - rect.top,
            pid,
        });
    }

    1 // continue enumeration
}

#[cfg(target_os = "windows")]
fn enumerate_windows() -> Vec<WindowEntry> {
    {
        let mut list = WINDOW_LIST.lock().unwrap();
        list.clear();
    }
    unsafe {
        EnumWindows(enum_proc, 0);
    }
    let mut list = WINDOW_LIST.lock().unwrap();
    std::mem::take(&mut *list)
}

// ── Tool impl ───────────────────────────────────────

impl Tool for ListWindowsTool {
    fn name(&self) -> &'static str {
        "list_windows"
    }

    fn description(&self) -> &'static str {
        "List all visible windows with title, position, size, and PID. Use with take_screenshot x/y/width/height to capture a specific window."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "filter": {
                    "type": "string",
                    "description": "Case-insensitive substring filter for window title (optional)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 30)"
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
        #[cfg(not(target_os = "windows"))]
        {
            let _ = args;
            return ToolResult::err("list_windows is only available on Windows".to_string());
        }

        #[cfg(target_os = "windows")]
        {
            let start = std::time::Instant::now();
            log::info!("[ListWindows] enumerating...");

            let filter = args.get("filter").and_then(|v| v.as_str()).unwrap_or("");
            let max_results = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(30) as usize;
            let filter_lower = filter.to_lowercase();

            let windows = enumerate_windows();

            let mut lines = Vec::new();
            let mut count = 0;

            for w in &windows {
                if count >= max_results {
                    break;
                }
                if !filter_lower.is_empty()
                    && !w.title.to_lowercase().contains(&filter_lower)
                {
                    continue;
                }
                lines.push(format!(
                    "hwnd={:#x}  pid={}  rect=({},{},{},{})\n  \"{}\"",
                    w.hwnd as usize, w.pid, w.x, w.y, w.w, w.h, w.title,
                ));
                count += 1;
            }

            log::info!("[ListWindows] done: elapsed={:?}, total={}, matched={}",
                start.elapsed(), windows.len(), count);

            if lines.is_empty() {
                if filter_lower.is_empty() {
                    ToolResult::ok("No visible windows found".to_string())
                } else {
                    ToolResult::ok(format!(
                        "No windows matching '{}' ({} total visible windows)",
                        filter, windows.len(),
                    ))
                }
            } else {
                let summary = format!(
                    "{} windows ({} total visible)\n\n{}",
                    count, windows.len(),
                    lines.join("\n"),
                );
                ToolResult::ok(summary)
            }
        }
    }
}
