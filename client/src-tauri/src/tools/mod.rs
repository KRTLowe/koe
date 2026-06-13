use serde_json::Value;

use crate::config::AppConfig;
use crate::ws_client::ToolDef;

pub struct ToolContent {
    pub text: String,
}

pub struct ToolResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
    pub upload_path: Option<String>,
}

impl ToolResult {
    pub fn ok(text: String) -> Self {
        Self {
            content: vec![ToolContent { text }],
            is_error: false,
            upload_path: None,
        }
    }

    pub fn err(text: String) -> Self {
        Self {
            content: vec![ToolContent { text }],
            is_error: true,
            upload_path: None,
        }
    }

    pub fn ok_with_upload(text: String, path: String) -> Self {
        Self {
            content: vec![ToolContent { text }],
            is_error: false,
            upload_path: Some(path),
        }
    }
}

pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> Value;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
    fn execute(&self, args: &Value) -> ToolResult;
}

// ── ToolManager ───────────────────────────────────

pub struct ToolManager {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolManager {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            tools: vec![
                Box::new(screenshot::ScreenshotTool::new(config)),
                Box::new(clipboard::ClipboardTool::new(config)),
                Box::new(file_search::FileSearchTool::new(config)),
                Box::new(uia_tree::UiaTreeTool::new(config)),
                Box::new(read_file::ReadFileTool::new(config)),
                Box::new(write_file::WriteFileTool::new(config)),
                Box::new(list_dir::ListDirTool::new(config)),
                Box::new(file_info::FileInfoTool::new(config)),
                Box::new(grep_file::GrepFileTool::new(config)),
                Box::new(pull_file::PullFileTool::new(config)),
                Box::new(run_command::RunCommandTool::new(config)),
                Box::new(write_clipboard::WriteClipboardTool::new(config)),
                Box::new(open_path::OpenPathTool::new(config)),
                Box::new(system_info::SystemInfoTool::new(config)),
                Box::new(list_windows::ListWindowsTool::new(config)),
                Box::new(start_process::StartProcessTool::new(config)),
                Box::new(kill_process::KillProcessTool::new(config)),
                Box::new(input::TypeTextTool::new(config)),
                Box::new(input::KeyPressTool::new(config)),
                Box::new(input::MouseClickTool::new(config)),
                Box::new(ocr::OcrTool::new(config)),
                Box::new(foreground_window::ForegroundWindowTool::new(config)),
            ],
        }
    }

    /// 返回所有已启用工具的 ToolDef（用于注册到服务端）
    pub fn enabled_defs(&self) -> Vec<ToolDef> {
        let names: Vec<&str> = self
            .tools
            .iter()
            .filter(|t| t.is_enabled())
            .map(|t| t.name())
            .collect();
        log::info!(
            "[ToolManager] enabled_defs: {} tools enabled: {:?}",
            names.len(),
            names
        );
        self.tools
            .iter()
            .filter(|t| t.is_enabled())
            .map(|t| ToolDef {
                name: t.name(),
                description: t.description(),
                input_schema: t.input_schema(),
            })
            .collect()
    }

    /// 按名称执行工具
    pub fn execute(&self, name: &str, args: &Value) -> Option<ToolResult> {
        log::info!(
            "[ToolManager] execute: name={}, args={}, total_tools={}",
            name,
            args,
            self.tools.len()
        );
        for tool in &self.tools {
            log::info!(
                "[ToolManager]   check tool={}, enabled={}",
                tool.name(),
                tool.is_enabled()
            );
            if tool.name() == name && tool.is_enabled() {
                let result = tool.execute(args);
                log::info!(
                    "[ToolManager]   -> executed {}, is_error={}, has_upload={}",
                    name,
                    result.is_error,
                    result.upload_path.is_some()
                );
                return Some(result);
            }
        }
        log::info!("[ToolManager]   -> tool not found or disabled: {}", name);
        None
    }

    /// 更新工具的启用状态
    pub fn set_enabled(&mut self, name: &str, enabled: bool) {
        log::info!(
            "[ToolManager] set_enabled: name={}, enabled={}",
            name,
            enabled
        );
        for tool in &mut self.tools {
            if tool.name() == name {
                let old = tool.is_enabled();
                tool.set_enabled(enabled);
                log::info!("[ToolManager]   -> {}: {} -> {}", name, old, enabled);
                return;
            }
        }
        log::info!("[ToolManager]   -> tool not found: {}", name);
    }
}

// ── 子模块 ────────────────────────────────────────

mod clipboard;
mod file_info;
mod file_search;
mod foreground_window;
mod grep_file;
mod input;
mod kill_process;
mod list_dir;
mod list_windows;
mod ocr;
mod open_path;
mod path_guard;
mod pull_file;
mod read_file;
mod run_command;
mod screenshot;
mod start_process;
mod system_info;
mod uia_tree;
mod write_clipboard;
mod write_file;
