# KOE 文件读写工具实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在客户端新增 `read_text_file`、`write_text_file`、`list_directory`、`get_file_info` 四个工具，补齐 LLM 读写文件的能力缺口。

**架构：** 遵循已建立的 Tool trait 模式，每个工具一个独立文件注册到 ToolManager。新增 `validate_file_path` 安全守卫模块。权限走现有 `tool_permissions` 体系。

**技术栈：** Rust（tokio::fs、walkdir 已有）、Vue（CapabilitiesPage 自动渲染）

---

## 文件结构

### 新增文件

| 文件 | 职责 |
|------|------|
| `client/src-tauri/src/tools/read_file.rs` | `ReadFileTool` — 读文本文件，支持偏移/截断/编码 |
| `client/src-tauri/src/tools/write_file.rs` | `WriteFileTool` — 写/追加文本文件 |
| `client/src-tauri/src/tools/list_dir.rs` | `ListDirTool` — 列出目录内容 |
| `client/src-tauri/src/tools/file_info.rs` | `FileInfoTool` — 文件/目录元信息 |
| `client/src-tauri/src/tools/path_guard.rs` | 路径校验：规范化、白名单、黑名单 |

### 修改文件

| 文件 | 改动 |
|------|------|
| `client/src-tauri/src/tools/mod.rs` | 注册 4 个新工具 + `mod` 声明 |
| `client/src-tauri/src/config.rs` | `tool_permissions` 默认值加 4 个新条目，新增路径白名单字段 |

---

## 任务

### 任务 1：path_guard — 路径安全校验模块

**文件：**
- 创建：`client/src-tauri/src/tools/path_guard.rs`
- 修改：`client/src-tauri/src/tools/mod.rs`（加 `mod path_guard;`）

**职责：** 统一处理路径规范化、白名单匹配、扩展名黑名单过滤。所有文件工具共享此模块。

```rust
// tools/path_guard.rs
use std::path::{Path, PathBuf};

pub struct PathGuard {
    pub allowed_reads: Vec<PathBuf>,
    pub allowed_writes: Vec<PathBuf>,
    pub denied_exts: Vec<String>,
}

impl PathGuard {
    pub fn new(config: &crate::config::AppConfig) -> Self {
        let expand = |s: &str| -> PathBuf {
            if s.starts_with("~/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                PathBuf::from(s.replacen("~/", &format!("{}/", home), 1))
            } else {
                PathBuf::from(s)
            }
        };
        Self {
            allowed_reads: config.allowed_read_paths.iter().map(|s| expand(s)).collect(),
            allowed_writes: config.allowed_write_paths.iter().map(|s| expand(s)).collect(),
            denied_exts: config.denied_extensions.clone(),
        }
    }

    /// 校验路径可读。返回 canonicalized path 或错误。
    pub fn check_read(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        if !p.is_absolute() {
            return Err("路径必须是绝对路径".to_string());
        }
        let canonical = p.canonicalize().map_err(|_| "路径不存在或无权限访问".to_string())?;
        let ext = canonical.extension().map(|e| e.to_string_lossy().to_lowercase());
        if let Some(ref e) = ext {
            if self.denied_exts.contains(e) {
                return Err(format!("不允许读取 .{} 类型的文件", e));
            }
        }
        if !self.allowed_reads.iter().any(|d| canonical.starts_with(d)) {
            return Err("路径不在允许的读取范围内".to_string());
        }
        Ok(canonical)
    }

    /// 校验路径可写。返回 canonicalized parent 或错误。
    pub fn check_write(&self, path: &str) -> Result<PathBuf, String> {
        let p = Path::new(path);
        if !p.is_absolute() {
            return Err("路径必须是绝对路径".to_string());
        }
        // 父目录必须存在且在白名单内
        let parent = p.parent().ok_or("路径无效")?;
        let canonical_parent = parent.canonicalize().map_err(|_| "父目录不存在".to_string())?;
        let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase());
        if let Some(ref e) = ext {
            if self.denied_exts.contains(e) {
                return Err(format!("不允许写入 .{} 类型的文件", e));
            }
        }
        if !self.allowed_writes.iter().any(|d| canonical_parent.starts_with(d)) {
            return Err("路径不在允许的写入范围内".to_string());
        }
        Ok(canonical_parent.join(p.file_name().unwrap()))
    }
}
```

**验证：** `cargo check` 通过

**Commit 信息：** `feat(tools): add PathGuard for file path validation`

---

### 任务 2：config.rs — 新增配置字段

**文件：** `client/src-tauri/src/config.rs`

- [ ] 在 `AppConfig` 新增三个字段：

```rust
#[serde(default = "default_allowed_read_paths")]
pub allowed_read_paths: Vec<String>,
#[serde(default = "default_allowed_write_paths")]
pub allowed_write_paths: Vec<String>,
#[serde(default = "default_denied_extensions")]
pub denied_extensions: Vec<String>,
```

- [ ] 添加默认函数：

```rust
fn default_allowed_read_paths() -> Vec<String> {
    vec!["~/kaya-transfer".into(), "~/Desktop".into(), "~/Documents".into()]
}
fn default_allowed_write_paths() -> Vec<String> {
    vec!["~/kaya-transfer".into(), "~/Desktop".into()]
}
fn default_denied_extensions() -> Vec<String> {
    vec!["exe".into(), "dll".into(), "sys".into(), "bin".into()]
}
```

- [ ] 更新 `default_tool_permissions` 增加新工具条目：

```rust
("read_text_file".into(), true),
("write_text_file".into(), false),
("list_directory".into(), true),
("get_file_info".into(), true),
```

**验证：** `cargo check` 通过

**Commit：** `feat(config): add allowed_paths and new tool permissions`

---

### 任务 3：ReadFileTool

**文件：** `client/src-tauri/src/tools/read_file.rs`

```rust
use async_trait::async_trait;
use serde_json::Value;
use std::io::Read;

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
    fn description(&self) -> &'static str { "Read a text file with optional offset/limit for large files" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file path" },
                "encoding": { "type": "string", "enum": ["utf-8", "utf-16", "gbk"], "description": "File encoding (default: utf-8)" },
                "offset": { "type": "integer", "description": "Byte offset to start reading from (default: 0)" },
                "limit": { "type": "integer", "description": "Max bytes to read (default: 65536, max: 1048576)" }
            },
            "required": ["path"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }

    fn execute(&self, args: &Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let encoding = args.get("encoding").and_then(|v| v.as_str()).unwrap_or("utf-8");
        let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(65536).min(1048576) as usize;

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

        if offset > 0 {
            use std::io::Seek;
            if let Err(e) = file.seek(std::io::SeekFrom::Start(offset as u64)) {
                return ToolResult::err(format!("定位偏移失败: {}", e));
            }
        }

        // detect encoding and decode
        let read_size = limit.min(file_size.saturating_sub(offset));
        let mut raw = vec![0u8; read_size];
        if read_size > 0 {
            if let Err(e) = file.read_exact(&mut raw) {
                return ToolResult::err(format!("读取文件失败: {}", e));
            }
        }

        let content = match encoding {
            "utf-16" => String::from_utf16(
                &raw.chunks(2).map(|c| u16::from_le_bytes([c[0], c.get(1).copied().unwrap_or(0)])).collect::<Vec<_>>()
            ).unwrap_or_else(|_| "(encoding error)".to_string()),
            "gbk" => {
                // gbk decoding via encoding_rs or fallback
                // use a simple fallback: treat as latin1
                raw.iter().map(|&b| b as char).collect()
            }
            _ => String::from_utf8(raw).unwrap_or_else(|_| "(encoding error)".to_string()),
        };

        let truncated = offset + read_size < file_size;
        let result_text = if truncated {
            format!("{} (文件共 {} bytes，已截取前 {} bytes，共返回 {} bytes。可用 offset 参数翻页)", content, file_size, limit, read_size)
        } else {
            content
        };

        ToolResult::ok(result_text)
    }
}
```

**注意：** GBK 解码暂用 fallback，后续可加 `encoding_rs` crate。

**Compatibility note:** All tools in this feature are synchronous (no async needed), following the same pattern as the existing tools after the refactor.

**验证：** `cargo check` 通过

**Commit：** `feat(tools): add ReadFileTool with offset/limit and encoding`

---

### 任务 4：WriteFileTool

**文件：** `client/src-tauri/src/tools/write_file.rs`

```rust
use serde_json::Value;

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct WriteFileTool {
    enabled: bool,
    guard: PathGuard,
}

impl WriteFileTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config.tool_permissions.get("write_text_file").copied().unwrap_or(false),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for WriteFileTool {
    fn name(&self) -> &'static str { "write_text_file" }
    fn description(&self) -> &'static str { "Write or append text content to a file. Default mode is append for safety." }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file path" },
                "content": { "type": "string", "description": "Text content to write" },
                "mode": { "type": "string", "enum": ["write", "append"], "description": "write=overwrite, append=add to end (default: append)" },
                "create_dirs": { "type": "boolean", "description": "Auto-create parent directories (default: false)" }
            },
            "required": ["path", "content"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }

    fn execute(&self, args: &Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("append");
        let create_dirs = args.get("create_dirs").and_then(|v| v.as_bool()).unwrap_or(false);

        if content.len() > 10 * 1024 * 1024 {
            return ToolResult::err("文件太大，单次写入上限为 10MB".to_string());
        }

        let target = match self.guard.check_write(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        if create_dirs {
            if let Some(parent) = target.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
        }

        let result = match mode {
            "write" => std::fs::write(&target, content),
            _ => {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .create(true).append(true).open(&target);
                match file {
                    Ok(ref mut f) => f.write_all(content.as_bytes()),
                    Err(e) => return ToolResult::err(format!("无法打开文件追加: {}", e)),
                }
            }
        };

        match result {
            Ok(_) => ToolResult::ok(format!("已{}到 {} ({} bytes)",
                if mode == "write" { "写入" } else { "追加" },
                target.display(), content.len())),
            Err(e) => ToolResult::err(format!("写入失败: {}", e)),
        }
    }
}
```

**验证：** `cargo check` 通过

**Commit：** `feat(tools): add WriteFileTool with append/write modes`

---

### 任务 5：ListDirTool

**文件：** `client/src-tauri/src/tools/list_dir.rs`

```rust
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
                "pattern": { "type": "string", "description": "Glob pattern filter (e.g. *.rs)" },
                "recursive": { "type": "boolean", "description": "List recursively (default: false)" },
                "max_results": { "type": "integer", "description": "Max entries (default: 100)" }
            },
            "required": ["path"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }

    fn execute(&self, args: &Value) -> ToolResult {
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
    }
}
```

**验证：** `cargo check` 通过

**Commit：** `feat(tools): add ListDirTool`

---

### 任务 6：FileInfoTool

**文件：** `client/src-tauri/src/tools/file_info.rs`

```rust
use serde_json::Value;
use chrono::{DateTime, Local};

use super::{Tool, ToolResult};
use crate::config::AppConfig;
use super::path_guard::PathGuard;

pub struct FileInfoTool {
    enabled: bool,
    guard: PathGuard,
}

impl FileInfoTool {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            enabled: config.tool_permissions.get("get_file_info").copied().unwrap_or(true),
            guard: PathGuard::new(config),
        }
    }
}

impl Tool for FileInfoTool {
    fn name(&self) -> &'static str { "get_file_info" }
    fn description(&self) -> &'static str { "Get metadata for a file or directory" }
    fn input_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Absolute file or directory path" }
            },
            "required": ["path"]
        })
    }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }

    fn execute(&self, args: &Value) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

        let canonical = match self.guard.check_read(path) {
            Ok(p) => p,
            Err(e) => return ToolResult::err(e),
        };

        let meta = match std::fs::metadata(&canonical) {
            Ok(m) => m,
            Err(e) => return ToolResult::err(format!("无法访问: {}", e)),
        };

        let is_dir = meta.is_dir();
        let size = meta.len();
        let modified = meta.modified().ok()
            .map(|t| {
                let dt: DateTime<Local> = t.into();
                dt.format("%Y-%m-%d %H:%M:%S").to_string()
            })
            .unwrap_or_else(|| "(unknown)".to_string());
        let ext = canonical.extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "(none)".to_string());

        let kind = if is_dir { "📁 目录" } else { "📄 文件" };
        let text = format!(
            "类型: {}\n路径: {}\n大小: {} bytes\n修改时间: {}\n扩展名: {}",
            kind, canonical.display(), size, modified, ext
        );

        ToolResult::ok(text)
    }
}
```

**验证：** `cargo check` 通过

**Commit：** `feat(tools): add FileInfoTool`

---

### 任务 7：ToolManager 注册 + 编译验证

**文件：** `client/src-tauri/src/tools/mod.rs`

- [ ] 在文件末尾 `mod` 声明区添加：

```rust
mod path_guard;
mod read_file;
mod write_file;
mod list_dir;
mod file_info;
```

- [ ] 在 `ToolManager::new()` 的 `tools` vec 末尾添加：

```rust
Box::new(read_file::ReadFileTool::new(config)),
Box::new(write_file::WriteFileTool::new(config)),
Box::new(list_dir::ListDirTool::new(config)),
Box::new(file_info::FileInfoTool::new(config)),
```

- [ ] 运行 `cargo check`，确认零错误

**Commit：** `feat(tools): register new tools in ToolManager`

---

## 提交顺序

| # | Commit | 文件 |
|---|--------|------|
| 1 | `feat(tools): add PathGuard for file path validation` | tools/path_guard.rs + mod.rs |
| 2 | `feat(config): add allowed_paths and new tool permissions` | config.rs |
| 3 | `feat(tools): add ReadFileTool with offset/limit and encoding` | tools/read_file.rs |
| 4 | `feat(tools): add WriteFileTool with append/write modes` | tools/write_file.rs |
| 5 | `feat(tools): add ListDirTool` | tools/list_dir.rs |
| 6 | `feat(tools): add FileInfoTool` | tools/file_info.rs |
| 7 | `feat(tools): register new tools in ToolManager` | tools/mod.rs |

## 验证清单

- [ ] `cargo check` 零错误
- [ ] `npx vite build` 零错误
- [ ] `read_text_file` 能读取文本文件并返回内容
- [ ] `read_text_file` 超出 1MB 截断并返回 truncated flag
- [ ] `read_text_file` 拒绝读取不在白名单中的路径
- [ ] `read_text_file` 拒绝读取 exe/dll/sys 文件
- [ ] `write_text_file` 默认不启用（permission=false）
- [ ] `write_text_file` append 模式追加到文件末尾
- [ ] `write_text_file` write 模式覆盖文件
- [ ] `list_directory` 列出目录内容
- [ ] `get_file_info` 返回文件元信息
