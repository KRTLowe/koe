# KOE 文件读写工具设计

## 概述

为 LLM 补齐读写能力缺口。当前客户端工具仅支持只读操作（截屏、剪贴板、文件搜索、UIA 树），新增三个工具实现双向文件访问。

## 新增工具

| 工具 | 功能 | 默认权限 |
|------|------|---------|
| `read_text_file` | 读文本文件（带截断翻页） | 开启 |
| `write_text_file` | 写/追加文本文件 | 关闭 |
| `list_directory` | 列出目录内容 | 开启 |
| `get_file_info` | 文件/目录元信息 | 开启 |

## 工具定义

### read_text_file

```
参数:
  path: string       — 绝对路径（必需）
  encoding?: string  — 编码（默认 utf-8，支持 utf-16/gbk）
  offset?: integer   — 起始字节偏移（默认 0）
  limit?: integer    — 最大读取字节数（默认 65536，上限 1MB）

返回:
  content: string    — 文件文本内容
  size: integer      — 文件总大小
  truncated: bool    — 是否因超过 limit 截断
  encoding: string   — 实际使用的编码
```

### write_text_file

```
参数:
  path: string       — 绝对路径（必需）
  content: string    — 写入内容（必需）
  mode?: string      — "write"（覆盖）/ "append"（追加，默认）
  create_dirs?: bool — 自动创建父目录（默认 false）

返回:
  path: string       — 实际写入路径
  size: integer      — 写入字节数
```

### list_directory

```
参数:
  path: string            — 目录绝对路径（必需）
  pattern?: string        — 文件名过滤（glob，如 "*.rs"）
  recursive?: boolean     — 是否递归（默认 false）
  max_results?: integer   — 最大条目数（默认 100）

返回:
  entries: [{ name, path, is_dir, size, modified }]
  total: integer
  truncated: bool
```

### get_file_info

```
参数:
  path: string       — 文件/目录绝对路径（必需）

返回:
  exists: bool
  is_dir: bool
  is_file: bool
  size: integer
  modified: string   — ISO 8601 时间
  extension: string
```

## 安全守卫

三层检查，在 `validate_file_path()` 中统一处理：

```
LLM 请求 → validate_file_path()
            ├─ 1. 路径规范化（canonicalize，防 ../ 逃逸）
            ├─ 2. 白名单前缀匹配
            └─ 3. 扩展名黑名单过滤
         → 执行工具 → 返回结果
```

### 路径白名单

```rust
// AppConfig 新增字段
pub allowed_read_paths: Vec<String>,      // 读/列表允许的目录
pub allowed_write_paths: Vec<String>,     // 写允许的目录（默认空 = 禁止写）
pub denied_extensions: Vec<String>,       // 禁止读写的扩展名
```

默认值：

```rust
fn default_allowed_read_paths() -> Vec<String> {
    vec!["~/kaya-transfer".into(), "~/Desktop".into(), "~/Documents".into()]
}

fn default_denied_extensions() -> Vec<String> {
    vec!["exe".into(), "dll".into(), "sys".into(), "bin".into()]
}
```

### 写入保护

`write_text_file` 默认 `tool_permissions["write_text_file"] = false`，需用户在前端手动开启。

### 大小限制

| 限制 | 值 | 行为 |
|------|-----|------|
| 单次读取上限 | 1 MB | 超过时截断，返回 `truncated: true`，LLM 可调 offset+limit 翻页 |
| 单次写入上限 | 10 MB | 超过时拒绝并提示 |

## 与现有架构的集成

### 文件结构

```
client/src-tauri/src/tools/
├── mod.rs              ← ToolManager 注册 read_file / write_file / list_dir / file_info
├── read_file.rs         ← 新增
├── write_file.rs        ← 新增
├── list_dir.rs          ← 新增
├── file_info.rs         ← 新增
├── screenshot.rs        (已有)
├── clipboard.rs         (已有)
├── file_search.rs       (已有)
└── uia_tree.rs          (已有)
```

### ToolManager 注册

```rust
// tools/mod.rs
impl ToolManager {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            tools: vec![
                Box::new(ScreenshotTool::new(config)),
                Box::new(ClipboardTool::new(config)),
                Box::new(FileSearchTool::new(config)),
                Box::new(UiaTreeTool::new(config)),
                Box::new(ReadFileTool::new(config)),
                Box::new(WriteFileTool::new(config)),
                Box::new(ListDirTool::new(config)),
                Box::new(FileInfoTool::new(config)),
            ],
        }
    }
}
```

### 权限控制

```rust
// config.rs AppConfig 新增字段
#[serde(default = "default_tool_permissions")]
pub tool_permissions: HashMap<String, bool>,

fn default_tool_permissions() -> HashMap<String, bool> {
    let mut m = HashMap::new();
    m.insert("take_screenshot".into(), true);
    m.insert("get_clipboard".into(), true);
    m.insert("file_search".into(), true);
    m.insert("get_uia_tree".into(), true);
    m.insert("read_text_file".into(), true);
    m.insert("write_text_file".into(), false);  // 写默认关闭
    m.insert("list_directory".into(), true);
    m.insert("get_file_info".into(), true);
    m
}
```

CapabilitiesPage 自动显示新增工具的开关（无需改代码——Vue 端从注册的工具列表动态渲染）。

## 示例对话

### 读取文件

```
用户: "帮我看看桌面上那个 config.json"
Kaya 调 read_text_file(path="C:\Users\krict\Desktop\config.json")
  → 返回内容 { server: "ws://10.0.0.2:9765", ... }
Kaya: "这是你的 KOE 配置，服务器指向 10.0.0.2:9765"
```

### 翻页读大文件

```
Kaya 调 read_text_file(path="D:\logs\app.log", limit=65536)
  → 返回前 64KB，truncated=true，总大小 2MB
Kaya: "日志文件较大（2MB），已显示前 64KB。要我继续读下一页吗？"
Kaya 调 read_text_file(path="D:\logs\app.log", offset=65536, limit=65536)
  → 返回 64KB~128KB 段
```

### 写入文件（需先开启权限）

```
用户: "把分析结果保存到桌面 report.md"
Kaya 调 write_text_file(path="C:\Users\krict\Desktop\report.md",
                        content="# 分析报告\n\n...", mode="write")
  → 写入成功
Kaya: "已保存到桌面 report.md"
```

### 禁止操作

```
Kaya 调 read_text_file(path="C:\Windows\System32\config\SAM")
  → 错误: "路径不在允许范围内"
Kaya: "系统文件无法读取，已拒绝该操作。"
```

## 数据流

```
Kaya → MCP call_client_tool(read_text_file, {path: "..."})
  → run_and_send.py handle_cmd
  → ws_handler.send_tool_call() → WS → 客户端
  → ws_client call_tool handler
  → tool_executor::execute_tool("read_text_file")
  → ToolManager → ReadFileTool::execute({path})
  → validate_file_path()
  → std::fs::read_to_string()
  → ToolResult { content: "..." }
  → tool_result → WS → 服务端 → MCP → Kaya
```

## 向后兼容

| 项目 | 兼容性 |
|------|--------|
| 旧配置 | `tool_permissions` 新增字段默认全开启（除 write），旧配置反序列化时自动填充 |
| `allowed_read_paths` | 新增字段，默认 `["~/kaya-transfer", "~/Desktop", "~/Documents"]` |
| 已有工具 | 不修改 |
| CapabilitiesPage | 自动显示新工具开关（无模板改动） |
