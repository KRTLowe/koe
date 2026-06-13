use std::time::Duration;
use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;

// ── Win32 FFI ───────────────────────────────────────

#[cfg(target_os = "windows")]
mod win32 {
    pub const VK_RETURN: u8 = 0x0D;
    pub const VK_TAB: u8 = 0x09;
    pub const VK_ESCAPE: u8 = 0x1B;
    pub const VK_BACK: u8 = 0x08;
    pub const VK_SPACE: u8 = 0x20;
    pub const VK_DELETE: u8 = 0x2E;
    pub const VK_INSERT: u8 = 0x2D;
    pub const VK_HOME: u8 = 0x24;
    pub const VK_END: u8 = 0x23;
    pub const VK_PRIOR: u8 = 0x21;
    pub const VK_NEXT: u8 = 0x22;
    pub const VK_UP: u8 = 0x26;
    pub const VK_DOWN: u8 = 0x28;
    pub const VK_LEFT: u8 = 0x25;
    pub const VK_RIGHT: u8 = 0x27;
    pub const VK_CONTROL: u8 = 0x11;
    pub const VK_MENU: u8 = 0x12;
    pub const VK_SHIFT: u8 = 0x10;
    pub const VK_LWIN: u8 = 0x5B;

    pub const KEYEVENTF_KEYUP: u32 = 0x0002;
    pub const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
    pub const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
    pub const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
    pub const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
    pub const MOUSEEVENTF_MIDDLEDOWN: u32 = 0x0020;
    pub const MOUSEEVENTF_MIDDLEUP: u32 = 0x0040;

    extern "system" {
        pub fn keybd_event(bVk: u8, bScan: u8, dwFlags: u32, dwExtraInfo: usize);
        pub fn SetCursorPos(x: i32, y: i32) -> i32;
        pub fn mouse_event(dwFlags: u32, dx: i32, dy: i32, dwData: u32, dwExtraInfo: usize);
        pub fn VkKeyScanW(ch: u16) -> i16;
        pub fn SetForegroundWindow(hWnd: isize) -> i32;
        pub fn SetFocus(hWnd: isize) -> isize;
        pub fn GetForegroundWindow() -> isize;
        pub fn GetWindowThreadProcessId(hWnd: isize, lpdwProcessId: *mut u32) -> u32;
        pub fn AttachThreadInput(idAttach: u32, idAttachTo: u32, fAttach: i32) -> i32;
        pub fn IsIconic(hWnd: isize) -> i32;
        pub fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
    }
}

// ── Helpers ─────────────────────────────────────────

#[cfg(target_os = "windows")]
fn press_vk(vk: u8, hold: bool) {
    let flags = if hold { 0 } else { win32::KEYEVENTF_KEYUP };
    unsafe {
        win32::keybd_event(vk, 0, flags, 0);
    }
}

#[cfg(target_os = "windows")]
fn type_char(ch: char) -> bool {
    let scan = unsafe { win32::VkKeyScanW(ch as u16) };
    if scan == -1 {
        return false; // 字符不支持
    }
    let vk = (scan & 0xFF) as u8;
    let shift = ((scan >> 8) & 0xFF) as u8;

    if shift & 1 != 0 {
        press_vk(win32::VK_SHIFT, true);
    }
    press_vk(vk, true);
    std::thread::sleep(std::time::Duration::from_millis(5));
    press_vk(vk, false);
    if shift & 1 != 0 {
        press_vk(win32::VK_SHIFT, false);
    }
    std::thread::sleep(std::time::Duration::from_millis(5));
    true
}

/// 解析按键组合，返回 [(vk, is_hold)] 序列。
/// 格式: "ctrl+c", "alt+tab", "enter", "f5"
#[cfg(target_os = "windows")]
fn parse_key_combo(combo: &str) -> Result<Vec<(u8, bool)>, String> {
    let combo_lower = combo.to_lowercase();
    let parts: Vec<&str> = combo_lower.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return Err("empty key combo".to_string());
    }

    let mut keys: Vec<(u8, bool)> = Vec::new();
    let main = parts.last().unwrap();

    // 先记录所有修饰键
    for p in &parts[..parts.len() - 1] {
        match *p {
            "ctrl" | "control" => keys.push((win32::VK_CONTROL, true)),
            "alt" => keys.push((win32::VK_MENU, true)),
            "shift" => keys.push((win32::VK_SHIFT, true)),
            "win" | "windows" => keys.push((win32::VK_LWIN, true)),
            _ => return Err(format!("unknown modifier: {}", p)),
        }
    }

    // 解析主键
    let vk = match *main {
        "enter" | "return" => win32::VK_RETURN,
        "tab" => win32::VK_TAB,
        "escape" | "esc" => win32::VK_ESCAPE,
        "backspace" | "back" => win32::VK_BACK,
        "space" => win32::VK_SPACE,
        "delete" | "del" => win32::VK_DELETE,
        "insert" | "ins" => win32::VK_INSERT,
        "home" => win32::VK_HOME,
        "end" => win32::VK_END,
        "pageup" | "pgup" => win32::VK_PRIOR,
        "pagedown" | "pgdn" => win32::VK_NEXT,
        "up" => win32::VK_UP,
        "down" => win32::VK_DOWN,
        "left" => win32::VK_LEFT,
        "right" => win32::VK_RIGHT,
        s if s.len() == 1 => {
            let ch = s.chars().next().unwrap();
            if ch.is_ascii_alphabetic() {
                ch.to_ascii_uppercase() as u8
            } else if ch.is_ascii_digit() {
                ch as u8
            } else {
                return Err(format!("unsupported key: {}", s));
            }
        }
        s if s.starts_with('f') => {
            if let Ok(n) = s[1..].parse::<u8>() {
                if (1..=12).contains(&n) {
                    0x6F + n
                } else {
                    return Err(format!("F-key out of range: {}", s));
                }
            } else {
                return Err(format!("invalid F-key: {}", s));
            }
        }
        _ => return Err(format!("unknown key: {}", main)),
    };

    // 主键按下 + 释放
    keys.push((vk, true));
    keys.push((vk, false));

    // 修饰键释放（逆序），先收集再追加
    let mut releases: Vec<(u8, bool)> = Vec::new();
    for &(vk, _) in keys.iter().rev() {
        if vk == win32::VK_CONTROL
            || vk == win32::VK_MENU
            || vk == win32::VK_SHIFT
            || vk == win32::VK_LWIN
        {
            releases.push((vk, false));
        }
    }
    keys.extend(releases);

    Ok(keys)
}

#[cfg(target_os = "windows")]
fn exec_key_combo(combo: &str) -> Result<(), String> {
    let sequence = parse_key_combo(combo)?;
    for (vk, hold) in &sequence {
        press_vk(*vk, *hold);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    std::thread::sleep(std::time::Duration::from_millis(20));
    Ok(())
}

/// 将指定窗口带到前台（绕过 UIPI 限制）。
/// Alt 键模拟 → SetForegroundWindow → SetFocus
#[cfg(target_os = "windows")]
fn bring_window_to_foreground(hwnd: isize) {
    unsafe {
        if win32::GetForegroundWindow() == hwnd {
            return;
        }
        log::info!("[Focus] bringing window hwnd=0x{:x} to foreground", hwnd);
        // 最小化窗口先还原
        if win32::IsIconic(hwnd) != 0 {
            win32::ShowWindow(hwnd, 9); // SW_RESTORE = 9
            std::thread::sleep(Duration::from_millis(100));
        }
        // Alt 键模拟绕过 UIPI，使 SetForegroundWindow 生效
        win32::keybd_event(win32::VK_MENU, 0, 0, 0);
        std::thread::sleep(Duration::from_millis(10));
        win32::keybd_event(win32::VK_MENU, 0, win32::KEYEVENTF_KEYUP, 0);
        win32::SetForegroundWindow(hwnd);
        std::thread::sleep(Duration::from_millis(100));
        // 等确认焦点到位
        for _ in 0..5 {
            if win32::GetForegroundWindow() == hwnd {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

// ── Tool: type_text ─────────────────────────────────

pub struct TypeTextTool {
    enabled: bool,
}

impl TypeTextTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("type_text")
                .copied()
                .unwrap_or(false),
        }
    }
}

impl Tool for TypeTextTool {
    fn name(&self) -> &'static str {
        "type_text"
    }
    fn description(&self) -> &'static str {
        "Type text at the current keyboard focus. Supports all characters (Chinese, emoji etc. via clipboard paste). \
         Make sure the target window has focus before calling this. \
         Use end_with_enter=true to press Enter after typing."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "text": { "type": "string", "description": "Text to type" },
                "end_with_enter": { "type": "boolean", "description": "Press Enter after typing (default: false)" },
                "hwnd": { "type": "integer", "description": "Window handle to activate before typing (from get_foreground_window)" }
            },
            "required": ["text"]
        })
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, e: bool) {
        self.enabled = e;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = args;
            return ToolResult::err("type_text is only available on Windows".to_string());
        }
        #[cfg(target_os = "windows")]
        {
            let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
            if text.is_empty() {
                return ToolResult::err("text is required".to_string());
            }
            let end_with_enter = args
                .get("end_with_enter")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if let Some(hwnd) = args.get("hwnd").and_then(|v| v.as_i64()) {
                log::info!("[TypeText] activating window hwnd=0x{:x}", hwnd);
                bring_window_to_foreground(hwnd as isize);
                std::thread::sleep(Duration::from_millis(50));
                log::info!("[TypeText] window activated, starting typing");
            }

            log::info!("[TypeText] typing {} chars, end_with_enter={}", text.len(), end_with_enter);

            let all_ascii = text.chars().all(|c| {
                c.is_ascii()
                    && (c.is_alphanumeric()
                        || c.is_ascii_punctuation()
                        || c == ' '
                        || c == '\n'
                        || c == '\r'
                        || c == '\t')
            });

            if all_ascii {
                let mut skipped = 0;
                for ch in text.chars() {
                    if !type_char(ch) {
                        skipped += 1;
                    }
                }
                if end_with_enter {
                    exec_key_combo("enter").ok();
                }
                if skipped > 0 {
                    ToolResult::ok(format!(
                        "已输入 {} 字符（{} 个不支持跳过）",
                        text.len() - skipped,
                        skipped
                    ))
                } else {
                    ToolResult::ok(format!("已输入 {} 字符", text.len()))
                }
            } else {
                match arboard::Clipboard::new() {
                    Ok(mut clipboard) => {
                        if let Err(e) = clipboard.set_text(text) {
                            return ToolResult::err(format!("写入剪贴板失败: {}", e));
                        }
                    }
                    Err(e) => return ToolResult::err(format!("剪贴板访问失败: {}", e)),
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
                match exec_key_combo("ctrl+v") {
                    Ok(()) => {
                        if end_with_enter {
                            std::thread::sleep(std::time::Duration::from_millis(30));
                            exec_key_combo("enter").ok();
                        }
                        ToolResult::ok(format!("已输入 {} 字符", text.len()))
                    }
                    Err(e) => ToolResult::err(format!("粘贴失败: {}", e)),
                }
            }
        }
    }
}

// ── Tool: key_press ─────────────────────────────────

pub struct KeyPressTool {
    enabled: bool,
}

impl KeyPressTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("key_press")
                .copied()
                .unwrap_or(false),
        }
    }
}

impl Tool for KeyPressTool {
    fn name(&self) -> &'static str {
        "key_press"
    }
    fn description(&self) -> &'static str {
        "Press a key or key combination. Examples: enter, tab, escape, ctrl+c, alt+tab, f5, ctrl+shift+esc"
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "keys": { "type": "string", "description": "Key or combo: enter, ctrl+c, alt+tab, f5, ctrl+shift+esc" },
                "hwnd": { "type": "integer", "description": "Window handle to activate before pressing (from get_foreground_window)" }
            },
            "required": ["keys"]
        })
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, e: bool) {
        self.enabled = e;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = args;
            return ToolResult::err("key_press is only available on Windows".to_string());
        }
        #[cfg(target_os = "windows")]
        {
            let keys = args.get("keys").and_then(|v| v.as_str()).unwrap_or("");
            if keys.is_empty() {
                return ToolResult::err("keys is required".to_string());
            }
            if let Some(hwnd) = args.get("hwnd").and_then(|v| v.as_i64()) {
                log::info!("[KeyPress] activating window hwnd=0x{:x}", hwnd);
                bring_window_to_foreground(hwnd as isize);
                std::thread::sleep(Duration::from_millis(50));
            }
            log::info!("[KeyPress] {}", keys);
            match exec_key_combo(keys) {
                Ok(()) => ToolResult::ok(format!("已按下: {}", keys)),
                Err(e) => ToolResult::err(e),
            }
        }
    }
}

// ── Tool: mouse_click ───────────────────────────────

pub struct MouseClickTool {
    enabled: bool,
}

impl MouseClickTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("mouse_click")
                .copied()
                .unwrap_or(false),
        }
    }
}

impl Tool for MouseClickTool {
    fn name(&self) -> &'static str {
        "mouse_click"
    }
    fn description(&self) -> &'static str {
        "Move mouse to screen coordinates and click. Use list_windows + take_screenshot to find coordinates. \
         Coordinates are absolute screen pixels."
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "x": { "type": "integer", "description": "Screen X coordinate" },
                "y": { "type": "integer", "description": "Screen Y coordinate" },
                "button": { "type": "string", "enum": ["left", "right", "middle"], "description": "Mouse button (default: left)" },
                "clicks": { "type": "integer", "description": "1 for single-click, 2 for double-click (default: 1)" },
                "hwnd": { "type": "integer", "description": "Window handle to activate before clicking (from get_foreground_window)" }
            },
            "required": ["x", "y"]
        })
    }
    fn is_enabled(&self) -> bool {
        self.enabled
    }
    fn set_enabled(&mut self, e: bool) {
        self.enabled = e;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        #[cfg(not(target_os = "windows"))]
        {
            let _ = args;
            return ToolResult::err("mouse_click is only available on Windows".to_string());
        }
        #[cfg(target_os = "windows")]
        {
            let x = args.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let y = args.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let button = args
                .get("button")
                .and_then(|v| v.as_str())
                .unwrap_or("left");
            let clicks = args.get("clicks").and_then(|v| v.as_u64()).unwrap_or(1);

            if let Some(hwnd) = args.get("hwnd").and_then(|v| v.as_i64()) {
                log::info!("[MouseClick] activating window hwnd=0x{:x}", hwnd);
                bring_window_to_foreground(hwnd as isize);
                std::thread::sleep(Duration::from_millis(50));
            }

            log::info!("[MouseClick] x={} y={} button={} clicks={}", x, y, button, clicks);

            unsafe {
                win32::SetCursorPos(x, y);
            }
            std::thread::sleep(std::time::Duration::from_millis(20));

            let (down, up) = match button {
                "right" => (win32::MOUSEEVENTF_RIGHTDOWN, win32::MOUSEEVENTF_RIGHTUP),
                "middle" => (win32::MOUSEEVENTF_MIDDLEDOWN, win32::MOUSEEVENTF_MIDDLEUP),
                _ => (win32::MOUSEEVENTF_LEFTDOWN, win32::MOUSEEVENTF_LEFTUP),
            };

            for _ in 0..clicks {
                unsafe {
                    win32::mouse_event(down, 0, 0, 0, 0);
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
                unsafe {
                    win32::mouse_event(up, 0, 0, 0, 0);
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }

            ToolResult::ok(format!("已点击: ({}, {}) {} x{}", x, y, button, clicks))
        }
    }
}

// ── Tool: activate_window ───────────────────────────

pub struct ActivateWindowTool {
    enabled: bool,
}

impl ActivateWindowTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("activate_window")
                .copied()
                .unwrap_or(true),
        }
    }
}

impl Tool for ActivateWindowTool {
    fn name(&self) -> &'static str {
        "activate_window"
    }

    fn description(&self) -> &'static str {
        "Bring a window to the foreground by HWND. Works for minimized, hidden, or inactive windows — \
         restores and activates it so it becomes the focused window. \
         Use after get_foreground_window to focus a specific window before other operations."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "hwnd": { "type": "integer", "description": "Window handle to activate (from get_foreground_window)" }
            },
            "required": ["hwnd"]
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
            return ToolResult::err("activate_window is only available on Windows".to_string());
        }

        #[cfg(target_os = "windows")]
        {
            let hwnd = args.get("hwnd").and_then(|v| v.as_i64()).unwrap_or(0);
            if hwnd == 0 {
                return ToolResult::err("hwnd is required".to_string());
            }
            log::info!("[ActivateWindow] activating hwnd=0x{:x}", hwnd);
            bring_window_to_foreground(hwnd as isize);
            ToolResult::ok(format!("窗口已激活 (hwnd=0x{:x})", hwnd))
        }
    }
}
