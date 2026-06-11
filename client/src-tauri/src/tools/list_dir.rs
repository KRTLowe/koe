use serde_json::Value;
use walkdir::WalkDir;
use std::time::UNIX_EPOCH;

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct ListDirTool {
    enabled: bool,
    guard: PathGuard,
}

impl ListDirTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config.tool_permissions.get("list_directory").copied().unwrap_or(true),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for ListDirTool {
    fn name(&self) -> &'static str { "list_directory" }
    fn description(&self) -> &'static str { "List files and directories with size and modification time" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute directory path" },
                "pattern": { "type": "string", "description": "Case-insensitive substring filter (e.g. .rs matches all .rs files)" },
                "recursive": { "type": "boolean", "description": "List recursively (default: false)" },
                "max_results": { "type": "integer", "description": "Max entries (default: 100)" }
            },
            "required": ["path"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = std::time::Instant::now();
        log::info!("[ListDirTool] execute begin: enabled={}", self.enabled);

        let result = (|| -> ToolResult {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("*");
            let recursive = args.get("recursive").and_then(|v| v.as_bool()).unwrap_or(false);
            let max_results = args.get("max_results").and_then(|v| v.as_u64()).unwrap_or(100) as usize;

            let canonical = match self.guard.check_read(path) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(e),
            };

            if !canonical.is_dir() {
                return ToolResult::err("路径不是一个目录".to_string());
            }

            let mut entries: Vec<String> = Vec::new();
            let pattern_lower = pattern.to_lowercase();

            let walker = if recursive {
                WalkDir::new(&canonical).max_depth(10).into_iter()
            } else {
                WalkDir::new(&canonical).max_depth(1).into_iter()
            };

            for entry in walker.filter_map(|e| e.ok()).skip(1) {
                if entries.len() >= max_results { break; }
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if !name.contains(&pattern_lower.replace('*', "")) { continue; }

                let meta = entry.metadata().ok();
                let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = meta.and_then(|m| m.modified().ok())
                    .map(|t| t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0))
                    .unwrap_or(0);
                let kind = if entry.file_type().is_dir() { "📁" } else { "📄" };
                entries.push(format!("{} {}  {} bytes  modified={}", kind, entry.path().display(), size, modified));
            }

            let total = entries.len();
            let truncated = total >= max_results;
            let text = format!("共 {} 条目{}\n{}",
                total,
                if truncated { "（已截断）" } else { "" },
                entries.join("\n"));

            ToolResult::ok(text)
        })();

        log::info!("[ListDirTool] execute end: elapsed={:?}, is_error={}", start.elapsed(), result.is_error);
        result
    }
}
