// ── 公开 API ─────────────────────────────────────

/// 采集前台窗口的 UI Automation 无障碍树。
///
/// `max_depth` — 最大递归深度（1-12，超出自动裁剪）
/// `max_items` — 最大输出元素数（1-500，超出自动截断）
#[cfg(target_os = "windows")]
pub fn get_uia_tree(max_depth: i32, max_items: usize) -> Result<String, String> {
    self::uia_impl::get_foreground_uia_tree(max_depth, max_items)
}

#[cfg(not(target_os = "windows"))]
pub fn get_uia_tree(_max_depth: i32, _max_items: usize) -> Result<String, String> {
    Err("UIA tree is only available on Windows".to_string())
}

// ── Windows COM 实现 ─────────────────────────────

#[cfg(target_os = "windows")]
mod uia_impl {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
        UIA_AppBarControlTypeId, UIA_ButtonControlTypeId, UIA_CalendarControlTypeId,
        UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_CustomControlTypeId,
        UIA_DataGridControlTypeId, UIA_DataItemControlTypeId, UIA_DocumentControlTypeId,
        UIA_EditControlTypeId, UIA_GroupControlTypeId, UIA_HeaderControlTypeId,
        UIA_HeaderItemControlTypeId, UIA_HyperlinkControlTypeId, UIA_ImageControlTypeId,
        UIA_ListControlTypeId, UIA_ListItemControlTypeId, UIA_MenuBarControlTypeId,
        UIA_MenuControlTypeId, UIA_MenuItemControlTypeId, UIA_PaneControlTypeId,
        UIA_ProgressBarControlTypeId, UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId,
        UIA_SemanticZoomControlTypeId, UIA_SeparatorControlTypeId, UIA_SliderControlTypeId,
        UIA_SpinnerControlTypeId, UIA_SplitButtonControlTypeId, UIA_StatusBarControlTypeId,
        UIA_TabControlTypeId, UIA_TabItemControlTypeId, UIA_TableControlTypeId,
        UIA_TextControlTypeId, UIA_ThumbControlTypeId, UIA_TitleBarControlTypeId,
        UIA_ToolBarControlTypeId, UIA_ToolTipControlTypeId, UIA_TreeControlTypeId,
        UIA_TreeItemControlTypeId, UIA_WindowControlTypeId,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    pub fn get_foreground_uia_tree(max_depth: i32, max_items: usize) -> Result<String, String> {
        // Clamp parameters
        let max_depth = max_depth.clamp(1, 12);
        let max_items = max_items.clamp(1, 500);

        unsafe {
            // COM init — ignore S_FALSE (already initialized on this thread)
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            // Create UIA automation object
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| format!("UIA unavailable: CoCreateInstance failed ({})", e))?;

            // Get foreground window handle
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return Ok("No foreground window".to_string());
            }

            // Get the UIA element for the foreground window
            let root = automation
                .ElementFromHandle(hwnd)
                .map_err(|e| format!("UIA ElementFromHandle failed: {}", e))?;

            // Use ControlViewWalker (user-perceivable controls only, not raw elements)
            let walker = automation
                .ControlViewWalker()
                .map_err(|e| format!("UIA ControlViewWalker failed: {}", e))?;

            // DFS tree walk
            let mut lines: Vec<String> = Vec::new();
            let mut item_count: usize = 0;

            walk_uia_subtree(
                &walker,
                &root,
                0,
                max_depth,
                max_items,
                &mut item_count,
                &mut lines,
            );

            let mut result = lines.join("\n");

            // Truncation notice
            if item_count >= max_items {
                result.push_str(&format!(
                    "\n\n(truncated: {}/? elements shown, depth limit {})",
                    max_items, max_depth,
                ));
            }

            log::info!(
                "[UIA] tree done: depth_limit={}, items={}, output_bytes={}",
                max_depth,
                item_count,
                result.len(),
            );

            Ok(result)
        }
    }

    /// Recursive DFS: formats current node, then walks children.
    fn walk_uia_subtree(
        walker: &IUIAutomationTreeWalker,
        element: &IUIAutomationElement,
        depth: i32,
        max_depth: i32,
        max_items: usize,
        item_count: &mut usize,
        lines: &mut Vec<String>,
    ) {
        if depth > max_depth || *item_count >= max_items {
            return;
        }

        // Format this element
        *item_count += 1;
        lines.push(format_element(element, depth));

        if depth >= max_depth || *item_count >= max_items {
            return;
        }

        // Get first child — Err means no children (or COM error — treat as leaf)
        let first_child = match unsafe { walker.GetFirstChildElement(element) } {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut current = first_child;
        loop {
            if *item_count >= max_items {
                break;
            }
            walk_uia_subtree(
                walker,
                &current,
                depth + 1,
                max_depth,
                max_items,
                item_count,
                lines,
            );

            match unsafe { walker.GetNextSiblingElement(&current) } {
                Ok(next) => current = next,
                Err(_) => break, // no more siblings
            }
        }
    }

    /// Format a single UIA element as one line:
    ///   indent ControlType "Name" [x,y,wxh] enabled|disabled [focusable] [offscreen] [password]
    fn format_element(element: &IUIAutomationElement, depth: i32) -> String {
        let indent = "  ".repeat(depth as usize);

        let ct_name = unsafe {
            element
                .CurrentControlType()
                .ok()
                .map(|ct| control_type_name(ct.0))
                .unwrap_or_else(|| "?".to_string())
        };

        let name = unsafe {
            element
                .CurrentName()
                .ok()
                .and_then(|b| {
                    let s = b.to_string();
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                })
                .unwrap_or_default()
        };

        let rect_str = unsafe {
            element
                .CurrentBoundingRectangle()
                .ok()
                .map(|r| {
                    let w = r.right - r.left;
                    let h = r.bottom - r.top;
                    format!("[{},{},{}x{}]", r.left, r.top, w, h)
                })
                .unwrap_or_else(|| "[?]".to_string())
        };

        // ── flags ──────────────────────────────────
        let mut flags: Vec<&'static str> = Vec::new();

        let enabled = unsafe {
            element
                .CurrentIsEnabled()
                .ok()
                .map(|b| b.as_bool())
                .unwrap_or(true)
        };
        flags.push(if enabled { "enabled" } else { "disabled" });

        if let Ok(v) = unsafe { element.CurrentIsKeyboardFocusable() } {
            if v.as_bool() {
                flags.push("focusable");
            }
        }
        if let Ok(v) = unsafe { element.CurrentIsOffscreen() } {
            if v.as_bool() {
                flags.push("offscreen");
            }
        }
        if let Ok(v) = unsafe { element.CurrentIsPassword() } {
            if v.as_bool() {
                flags.push("password");
            }
        }

        if name.is_empty() {
            format!("{} {} {} {}", indent, ct_name, rect_str, flags.join(" "))
        } else {
            format!(
                "{} {} \"{}\" {} {}",
                indent,
                ct_name,
                name,
                rect_str,
                flags.join(" ")
            )
        }
    }

    /// Map a UIA control type ID (i32) to a human-readable name.
    fn control_type_name(ct: i32) -> String {
        // sorted by ID for binary-search readability (not performance-critical)
        if ct == UIA_ButtonControlTypeId.0 {
            return "Button".into();
        }
        if ct == UIA_CalendarControlTypeId.0 {
            return "Calendar".into();
        }
        if ct == UIA_CheckBoxControlTypeId.0 {
            return "CheckBox".into();
        }
        if ct == UIA_ComboBoxControlTypeId.0 {
            return "ComboBox".into();
        }
        if ct == UIA_EditControlTypeId.0 {
            return "Edit".into();
        }
        if ct == UIA_HyperlinkControlTypeId.0 {
            return "Hyperlink".into();
        }
        if ct == UIA_ImageControlTypeId.0 {
            return "Image".into();
        }
        if ct == UIA_ListItemControlTypeId.0 {
            return "ListItem".into();
        }
        if ct == UIA_ListControlTypeId.0 {
            return "List".into();
        }
        if ct == UIA_MenuControlTypeId.0 {
            return "Menu".into();
        }
        if ct == UIA_MenuBarControlTypeId.0 {
            return "MenuBar".into();
        }
        if ct == UIA_MenuItemControlTypeId.0 {
            return "MenuItem".into();
        }
        if ct == UIA_ProgressBarControlTypeId.0 {
            return "ProgressBar".into();
        }
        if ct == UIA_RadioButtonControlTypeId.0 {
            return "RadioButton".into();
        }
        if ct == UIA_ScrollBarControlTypeId.0 {
            return "ScrollBar".into();
        }
        if ct == UIA_SliderControlTypeId.0 {
            return "Slider".into();
        }
        if ct == UIA_SpinnerControlTypeId.0 {
            return "Spinner".into();
        }
        if ct == UIA_StatusBarControlTypeId.0 {
            return "StatusBar".into();
        }
        if ct == UIA_TabControlTypeId.0 {
            return "Tab".into();
        }
        if ct == UIA_TabItemControlTypeId.0 {
            return "TabItem".into();
        }
        if ct == UIA_TextControlTypeId.0 {
            return "Text".into();
        }
        if ct == UIA_ToolBarControlTypeId.0 {
            return "ToolBar".into();
        }
        if ct == UIA_ToolTipControlTypeId.0 {
            return "ToolTip".into();
        }
        if ct == UIA_TreeControlTypeId.0 {
            return "Tree".into();
        }
        if ct == UIA_TreeItemControlTypeId.0 {
            return "TreeItem".into();
        }
        if ct == UIA_CustomControlTypeId.0 {
            return "Custom".into();
        }
        if ct == UIA_GroupControlTypeId.0 {
            return "Group".into();
        }
        if ct == UIA_ThumbControlTypeId.0 {
            return "Thumb".into();
        }
        if ct == UIA_DataGridControlTypeId.0 {
            return "DataGrid".into();
        }
        if ct == UIA_DataItemControlTypeId.0 {
            return "DataItem".into();
        }
        if ct == UIA_DocumentControlTypeId.0 {
            return "Document".into();
        }
        if ct == UIA_SplitButtonControlTypeId.0 {
            return "SplitButton".into();
        }
        if ct == UIA_WindowControlTypeId.0 {
            return "Window".into();
        }
        if ct == UIA_PaneControlTypeId.0 {
            return "Pane".into();
        }
        if ct == UIA_HeaderControlTypeId.0 {
            return "Header".into();
        }
        if ct == UIA_HeaderItemControlTypeId.0 {
            return "HeaderItem".into();
        }
        if ct == UIA_TableControlTypeId.0 {
            return "Table".into();
        }
        if ct == UIA_TitleBarControlTypeId.0 {
            return "TitleBar".into();
        }
        if ct == UIA_SeparatorControlTypeId.0 {
            return "Separator".into();
        }
        if ct == UIA_SemanticZoomControlTypeId.0 {
            return "SemanticZoom".into();
        }
        if ct == UIA_AppBarControlTypeId.0 {
            return "AppBar".into();
        }
        format!("ControlType({})", ct)
    }
}
