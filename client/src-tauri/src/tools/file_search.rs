use serde_json::Value;
use walkdir::WalkDir;

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct FileSearchTool {
    enabled: bool,
    guard: PathGuard,
}

impl FileSearchTool {
    pub fn new(config: &AppConfig) -> Self {
        let enabled = config
            .tool_permissions
            .get("file_search")
            .copied()
            .unwrap_or(true);
        Self { enabled, guard: PathGuard::new(config) }
    }
}

impl Tool for FileSearchTool {
    fn name(&self) -> &'static str {
        "file_search"
    }

    fn description(&self) -> &'static str {
        "Search for files by name pattern in a directory. When recursive, only searches within allowed read paths."
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "root": {
                    "type": "string",
                    "description": "Root directory to search from. Must be an absolute path within the allowed read paths."
                },
                "pattern": {
                    "type": "string",
                    "description": "File name pattern (case-insensitive substring match)"
                },
                "recursive": {
                    "type": "boolean",
                    "description": "Search recursively (default: false). When enabled, max depth is 8 and up to 100k entries are scanned."
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results (default: 50)"
                }
            },
            "required": ["root", "pattern"]
        })
    }

    fn is_enabled(&self) -> bool {
        self.enabled
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = std::time::Instant::now();
        log::info!("[FileSearchTool] execute begin: enabled={}", self.enabled);

        let root = args
            .get("root")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("*");
        let recursive = args
            .get("recursive")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(50) as usize;
        log::info!("[FileSearchTool] args: root={}, pattern={}, recursive={}, max_results={}",
            root, pattern, recursive, max_results);

        // 安全守卫：禁止搜索根目录
        let root_trimmed = root.trim();
        if root_trimmed == "/"
            || root_trimmed == "\\"
            || root_trimmed.eq_ignore_ascii_case("C:\\")
            || root_trimmed.eq_ignore_ascii_case("C:/")
        {
            log::info!("[FileSearchTool] rejected: root is filesystem root");
            return ToolResult::err(
                "Searching the entire filesystem root is not allowed. Please specify a subdirectory."
                    .to_string(),
            );
        }

        // PathGuard 校验
        let canonical = match self.guard.check_read(root) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        let max_depth = if recursive { 8 } else { 1 };
        let max_entries = if recursive { 100_000usize } else { 10_000 };

        let mut results: Vec<String> = Vec::new();
        let pattern_lower = pattern.to_lowercase();
        let mut dirs_visited = 0u64;

        for entry in WalkDir::new(&canonical)
            .max_depth(max_depth)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            dirs_visited += 1;
            if dirs_visited as usize > max_entries {
                log::info!("[FileSearchTool] exceeded max_entries={}, aborting", max_entries);
                return ToolResult::err(format!(
                    "搜索被中断：已扫描 {} 个条目（上限 {}）。请缩小搜索范围或关闭递归搜索。",
                    dirs_visited, max_entries,
                ));
            }
            if results.len() >= max_results {
                log::info!("[FileSearchTool] hit max_results={}, stopping early", max_results);
                break;
            }
            let name = entry.file_name().to_string_lossy().to_lowercase();
            if name.contains(&pattern_lower) {
                results.push(entry.path().display().to_string());
            }
        }

        log::info!("[FileSearchTool] execute end: elapsed={:?}, dirs_visited={}, results={}",
            start.elapsed(), dirs_visited, results.len());
        if results.is_empty() {
            ToolResult::ok(format!(
                "No files matching '{}' found in {}",
                pattern, root
            ))
        } else {
            ToolResult::ok(format!(
                "Found {} files matching '{}':\n{}",
                results.len(),
                pattern,
                results.join("\n")
            ))
        }
    }
}
