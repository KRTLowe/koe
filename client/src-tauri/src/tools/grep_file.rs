use serde_json::Value;
use walkdir::WalkDir;
use std::io::{BufRead, BufReader};
use std::fs::File;

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct GrepFileTool {
    enabled: bool,
    guard: PathGuard,
}

impl GrepFileTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config
                .tool_permissions
                .get("grep_file")
                .copied()
                .unwrap_or(true),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for GrepFileTool {
    fn name(&self) -> &'static str {
        "grep_file"
    }

    fn description(&self) -> &'static str {
        "Search file contents by pattern, return matching lines with line numbers"
    }

    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Absolute file or directory path to search"
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (substring or regex)"
                },
                "use_regex": {
                    "type": "boolean",
                    "description": "Treat pattern as regex (default: false)"
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Case-sensitive matching (default: false)"
                },
                "include": {
                    "type": "string",
                    "description": "File extension filter, e.g. .rs or *.rs (default: all text files)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Max matching lines to return (default: 100, max: 1000)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Lines of context before/after each match (default: 0)"
                }
            },
            "required": ["path", "pattern"]
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
        log::info!("[GrepFileTool] execute begin: enabled={}", self.enabled);

        let result = (|| -> ToolResult {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let use_regex = args
                .get("use_regex")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let case_sensitive = args
                .get("case_sensitive")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let include = args
                .get("include")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let max_results = args
                .get("max_results")
                .and_then(|v| v.as_u64())
                .unwrap_or(100)
                .min(1000) as usize;
            let context_lines = args
                .get("context_lines")
                .and_then(|v| v.as_u64())
                .unwrap_or(0) as usize;

            if pattern.is_empty() {
                return ToolResult::err("搜索模式不能为空".to_string());
            }

            let canonical = match self.guard.check_read(path) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(e),
            };

            // Compile regex upfront if needed
            let regex_pattern = if use_regex {
                match regex::Regex::new(pattern) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        return ToolResult::err(format!("正则表达式无效: {}", e));
                    }
                }
            } else {
                None
            };

            // Collect files to search
            let files: Vec<_> = if canonical.is_dir() {
                WalkDir::new(&canonical)
                    .max_depth(10)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                    .map(|e| e.path().to_path_buf())
                    .collect()
            } else {
                vec![canonical.clone()]
            };

            // Filter by include pattern
            let include_lower = include.to_lowercase();
            let ext_filter = include_lower.trim_start_matches("*.");
            let files: Vec<_> = if include.is_empty() {
                files
            } else {
                files
                    .into_iter()
                    .filter(|f| {
                        let name = f.to_string_lossy().to_lowercase();
                        name.ends_with(ext_filter)
                    })
                    .collect()
            };

            if files.is_empty() {
                return ToolResult::ok(
                    "没有找到匹配的文件（路径下无可搜索文件）".to_string(),
                );
            }

            let mut output_lines: Vec<String> = Vec::new();
            let mut files_matched = 0u32;
            let mut total_matches = 0u32;

            for file_path in &files {
                if total_matches >= max_results as u32 {
                    break;
                }

                // Quick binary check + file size gate
                let meta = match std::fs::metadata(file_path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if meta.len() > 10 * 1024 * 1024 || meta.len() == 0 {
                    continue;
                }
                if file_is_binary(file_path) {
                    continue;
                }

                let file_display = file_path.to_string_lossy();

                // Read all lines into memory
                let file = match File::open(file_path) {
                    Ok(f) => f,
                    Err(_) => continue,
                };
                let reader = BufReader::with_capacity(32 * 1024, file);

                let mut all_lines: Vec<String> = Vec::new();
                let mut match_lines: Vec<usize> = Vec::new();

                for (i, line_result) in reader.lines().enumerate() {
                    let line = match line_result {
                        Ok(l) => l,
                        Err(_) => continue,
                    };
                    let line_num = i + 1;
                    let matched = line_matches(
                        &line,
                        pattern,
                        &regex_pattern,
                        case_sensitive,
                    );
                    all_lines.push(line);
                    if matched {
                        match_lines.push(line_num);
                    }
                }

                if match_lines.is_empty() {
                    continue;
                }

                files_matched += 1;

                if context_lines > 0 {
                    // Group adjacent matches into clusters
                    let clusters = cluster_matches(&match_lines, context_lines);
                    let match_set: std::collections::HashSet<usize> =
                        match_lines.iter().copied().collect();

                    for (cluster_start, cluster_end) in &clusters {
                        if total_matches >= max_results as u32 {
                            break;
                        }

                        if !output_lines.is_empty() {
                            output_lines.push("--".to_string());
                        }

                        let print_start = cluster_start.saturating_sub(context_lines).max(1);
                        let print_end = (*cluster_end + context_lines).min(all_lines.len());

                        for ln in print_start..=print_end {
                            let line_text = &all_lines[ln - 1];
                            let sep = if match_set.contains(&ln) {
                                ":"
                            } else {
                                "-"
                            };
                            output_lines.push(format!(
                                "{}{}{}",
                                file_display, sep, line_text
                            ));
                            if match_set.contains(&ln) {
                                total_matches += 1;
                            }
                        }
                    }
                } else {
                    // Simple file:line: content output
                    let match_set: std::collections::HashSet<usize> =
                        match_lines.iter().copied().collect();
                    for ln in 1..=all_lines.len() {
                        if total_matches >= max_results as u32 {
                            break;
                        }
                        if match_set.contains(&ln) {
                            output_lines.push(format!(
                                "{}:{}: {}",
                                file_display, ln, all_lines[ln - 1]
                            ));
                            total_matches += 1;
                        }
                    }
                }
            }

            if output_lines.is_empty() {
                let msg = if use_regex {
                    format!("没有匹配正则 '{}' 的结果", pattern)
                } else {
                    format!("没有匹配 '{}' 的结果", pattern)
                };
                ToolResult::ok(msg)
            } else {
                let truncated = total_matches >= max_results as u32;
                let summary = format!(
                    "\n-- {} 个文件匹配，{} 行匹配{}",
                    files_matched,
                    total_matches,
                    if truncated {
                        format!("（已截断，上限 {} 行）", max_results)
                    } else {
                        String::new()
                    }
                );
                output_lines.push(summary);
                ToolResult::ok(output_lines.join("\n"))
            }
        })();

        log::info!(
            "[GrepFileTool] execute end: elapsed={:?}, is_error={}",
            start.elapsed(),
            result.is_error
        );
        result
    }
}

/// Check if a line matches the given pattern.
fn line_matches(
    line: &str,
    pattern: &str,
    regex_pattern: &Option<regex::Regex>,
    case_sensitive: bool,
) -> bool {
    if let Some(ref re) = regex_pattern {
        if case_sensitive {
            re.is_match(line)
        } else {
            re.is_match(&line.to_lowercase())
        }
    } else {
        let haystack = if case_sensitive {
            line.to_string()
        } else {
            line.to_lowercase()
        };
        let needle = if case_sensitive {
            pattern.to_string()
        } else {
            pattern.to_lowercase()
        };
        haystack.contains(&needle)
    }
}

/// Group sorted match line numbers into clusters where each cluster
/// contains matches within `gap` lines of each other.
fn cluster_matches(matches: &[usize], gap: usize) -> Vec<(usize, usize)> {
    if matches.is_empty() {
        return vec![];
    }
    let mut clusters: Vec<(usize, usize)> = Vec::new();
    let mut start = matches[0];
    let mut end = matches[0];
    for &m in &matches[1..] {
        if m <= end + gap * 2 + 1 {
            end = m;
        } else {
            clusters.push((start, end));
            start = m;
            end = m;
        }
    }
    clusters.push((start, end));
    clusters
}

/// Quick binary-file detection by reading first 8KB for null bytes.
fn file_is_binary(path: &std::path::Path) -> bool {
    let mut buf = [0u8; 8192];
    if let Ok(mut f) = File::open(path) {
        use std::io::Read;
        let n = f.read(&mut buf).unwrap_or(0);
        buf[..n].contains(&0u8)
    } else {
        false
    }
}
