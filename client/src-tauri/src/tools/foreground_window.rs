use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

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
    fn GetWindowTextW(hwnd: isize, buf: *mut u16, max: i32) -> i32;
    fn GetWindowTextLengthW(hwnd: isize) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut RECT) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
    fn CloseHandle(hObject: isize) -> i32;
    fn QueryFullProcessImageNameW(
        hProcess: isize,
        dwFlags: u32,
        lpExeName: *mut u16,
        lpdwSize: *mut u32,
    ) -> i32;
}

#[cfg(target_os = "windows")]
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

#[cfg(target_os = "windows")]
fn get_process_name(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 || handle == -1 as isize {
            return None;
        }
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ret = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ret == 0 || size == 0 {
            return None;
        }
        let name = String::from_utf16_lossy(&buf[..size as usize]);
        std::path::Path::new(&name)
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.to_string())
    }
}

pub struct ForegroundWindowTool {
    enabled: bool,
}

impl ForegroundWindowTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("get_foreground_window")
                .copied()
                .unwrap_or(true),
        }
    }
}

impl Tool for ForegroundWindowTool {
    fn name(&self) -> &'static str {
        "get_foreground_window"
    }

    fn description(&self) -> &'static str {
        "Get the currently focused (foreground) window: hwnd, title, process name, PID, position and size. \
         Pass the hwnd to type_text / key_press / mouse_click to activate that window before operating."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({ "type": "object", "properties": {} })
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, _args: &Value) -> ToolResult {
        #[cfg(not(target_os = "windows"))]
        {
            return ToolResult::err("get_foreground_window is only available on Windows".to_string());
        }

        #[cfg(target_os = "windows")]
        {
            unsafe {
                let hwnd = GetForegroundWindow();
                if hwnd == 0 {
                    return ToolResult::err("No foreground window found".to_string());
                }

                let len = GetWindowTextLengthW(hwnd);
                let title = if len > 0 {
                    let mut buf = vec![0u16; (len + 1) as usize];
                    GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
                    String::from_utf16_lossy(&buf[..len as usize])
                        .trim()
                        .to_string()
                } else {
                    String::new()
                };

                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                let process_name = get_process_name(pid).unwrap_or_default();

                let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
                GetWindowRect(hwnd, &mut rect);
                let w = rect.right - rect.left;
                let h = rect.bottom - rect.top;

                let result = serde_json::json!({
                    "hwnd": hwnd as usize,
                    "hwnd_hex": format!("0x{:x}", hwnd),
                    "title": title,
                    "process": process_name,
                    "pid": pid,
                    "x": rect.left,
                    "y": rect.top,
                    "width": w,
                    "height": h,
                });

                let summary = format!(
                    "前台窗口: \"{}\"  [{}]  hwnd=0x{:x}  pid={}  rect=({},{},{},{})  {}x{}",
                    title, process_name, hwnd as usize, pid,
                    rect.left, rect.top, rect.left + w, rect.top + h, w, h,
                );

                log::info!("[ForegroundWindow] {}", summary);

                match serde_json::to_string(&result) {
                    Ok(json) => ToolResult::ok(json),
                    Err(e) => ToolResult::err(format!("JSON 序列化失败: {}", e)),
                }
            }
        }
    }
}
