use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct ReadFileTool {
    enabled: bool,
    guard: PathGuard,
}

impl ReadFileTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config.tool_permissions.get("read_text_file").copied().unwrap_or(true),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for ReadFileTool {
    fn name(&self) -> &'static str { "read_text_file" }
    fn description(&self) -> &'static str {
        "Read a text file. Byte mode (default) or line mode (with start_line)"
    }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file path" },
                "encoding": { "type": "string", "enum": ["utf-8", "utf-16", "gbk"], "description": "File encoding (default: utf-8)" },
                "offset": { "type": "integer", "description": "Byte mode: byte offset. Line mode: extra bytes into start_line (default: 0)" },
                "limit": { "type": "integer", "description": "Byte mode: max bytes (default 65536, max 1048576). Line mode: max lines (default 50, max 1000)" },
                "start_line": { "type": "integer", "description": "Line number to start from (1-based). When set, switches to line mode" }
            },
            "required": ["path"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }

    fn execute(&self, args: &Value) -> ToolResult {
        let start = std::time::Instant::now();
        log::info!("[ReadFileTool] execute begin: enabled={}", self.enabled);

        let result = (|| -> ToolResult {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let encoding = args.get("encoding").and_then(|v| v.as_str()).unwrap_or("utf-8");
            let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let raw_limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            let limit_specified = args.get("limit").is_some();
            let start_line = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(0) as usize;

            let canonical = match self.guard.check_read(path) {
                Ok(p) => p,
                Err(e) => return ToolResult::err(e),
            };

            let metadata = match std::fs::metadata(&canonical) {
                Ok(m) => m,
                Err(e) => return ToolResult::err(format!("无法读取文件元信息: {}", e)),
            };
            let file_size = metadata.len() as usize;

            let mut file = match std::fs::File::open(&canonical) {
                Ok(f) => f,
                Err(e) => return ToolResult::err(format!("无法打开文件: {}", e)),
            };

            // ── Line mode ──
            if start_line > 0 {
                // Scan to start_line byte position
                let line_offset = {
                    let reader = BufReader::new(&file);
                    let mut byte_pos = 0u64;
                    let mut lines_skipped = 0u64;
                    for line_result in reader.split(b'\n').take(start_line - 1) {
                        match line_result {
                            Ok(buf) => byte_pos += buf.len() as u64 + 1,
                            Err(_) => return ToolResult::err("读取文件行失败".to_string()),
                        }
                        lines_skipped += 1;
                    }
                    if lines_skipped < (start_line - 1) as u64 {
                        return ToolResult::err(
                            format!("文件只有 {} 行，不足第 {} 行", lines_skipped + 1, start_line)
                        );
                    }
                    byte_pos as usize
                };

                let max_lines = if limit_specified {
                    raw_limit.min(1000).max(1)
                } else {
                    50
                };

                let seek_pos = line_offset + offset;
                if seek_pos > 0 {
                    if let Err(e) = file.seek(SeekFrom::Start(seek_pos as u64)) {
                        return ToolResult::err(format!("定位失败: {}", e));
                    }
                }

                let reader = BufReader::new(&file);
                let mut content_lines: Vec<String> = Vec::new();
                for line_result in reader.split(b'\n') {
                    if content_lines.len() >= max_lines {
                        break;
                    }
                    match line_result {
                        Ok(raw) => {
                            let decoded = match encoding {
                                "utf-16" => {
                                    let chars: Vec<u16> = raw.chunks(2)
                                        .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                                        .collect();
                                    String::from_utf16(&chars).unwrap_or_else(|_| "(encoding error)".to_string())
                                }
                                "gbk" => {
                                    let (cow, _, _) = encoding_rs::GBK.decode(&raw);
                                    cow.into_owned()
                                }
                                _ => String::from_utf8(raw).unwrap_or_else(|_| "(encoding error)".to_string()),
                            };
                            content_lines.push(decoded);
                        }
                        Err(_) => break,
                    }
                }

                let line_count = content_lines.len();
                let content = content_lines.join("\n");
                let result_text = if line_count < max_lines {
                    format!("（第 {}–{} 行，共 {} 行）\n{}",
                        start_line, start_line + line_count - 1, line_count, content)
                } else {
                    format!("（第 {}–{} 行，共 {} 行，可能还有更多。增大 limit 继续翻页）\n{}",
                        start_line, start_line + line_count - 1, line_count, content)
                };
                return ToolResult::ok(result_text);
            }

            // ── Byte mode ──
            let byte_limit = if limit_specified {
                raw_limit.min(1048576).max(1)
            } else {
                65536
            };

            if offset > 0 {
                if let Err(e) = file.seek(SeekFrom::Start(offset as u64)) {
                    return ToolResult::err(format!("定位偏移失败: {}", e));
                }
            }

            let read_size = byte_limit.min(file_size.saturating_sub(offset));
            let mut raw = vec![0u8; read_size];
            if read_size > 0 {
                if let Err(e) = file.read_exact(&mut raw) {
                    return ToolResult::err(format!("读取文件失败: {}", e));
                }
            }

            let content = match encoding {
                "utf-16" => {
                    String::from_utf16(
                        &raw.chunks(2)
                            .map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)]))
                            .collect::<Vec<_>>()
                    ).unwrap_or_else(|_| "(encoding error)".to_string())
                }
                "gbk" => {
                    let (cow, _, _) = encoding_rs::GBK.decode(&raw);
                    cow.into_owned()
                }
                _ => String::from_utf8(raw).unwrap_or_else(|_| "(encoding error)".to_string()),
            };

            let truncated = offset + read_size < file_size;
            let result_text = if truncated {
                format!("{} (文件共 {} bytes，已截取 {} bytes，共返回 {} bytes。可用 offset 参数翻页)",
                    content, file_size, byte_limit, read_size)
            } else {
                content
            };

            ToolResult::ok(result_text)
        })();

        log::info!("[ReadFileTool] execute end: elapsed={:?}, is_error={}", start.elapsed(), result.is_error);
        result
    }
}
