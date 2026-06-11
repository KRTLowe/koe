# KOE 客户端消息上下文注解与工具可扩展架构设计

## 概述

解决两个问题：

1. **消息来源不可知** — Kaya 收到 ACP 聊天消息时不知道来自哪个客户端、通过什么通道发送，无法自动关联到 `call_client_tool` 的目标
2. **工具系统硬编码** — 客户端的 4 个工具定义在 `ws_client.rs::local_tools()` 中硬编码，扩展新工具需要修改核心文件，且没有权限控制

## 改动范围

| 组件 | 改动量 | 说明 |
|------|--------|------|
| `lib.rs` | ~3 行 | `send_acp_message` 拼接消息前缀 |
| `config.rs` | ~10 行 | `AppConfig` 增加 `tool_permissions` 字段 |
| `ws_client.rs` | ~15 行 | `local_tools()` 改为从 `ToolManager` 读取 |
| `tool_executor.rs` | ~20 行 | 入口保留，内部委托给 `ToolManager` |
| 新增 `tools/` 目录 | ~120 行 | 4 个工具文件 + `mod.rs`（Tool trait + ToolManager） |
| `CapabilitiesPage.vue` | ~80 行 | 每行增加开关 toggle 和重新注册逻辑 |
| `lib.rs` 新 command | ~20 行 | `set_tool_enabled` Tauri 命令 |

---

## 1. 消息上下文注解

### 当前流程

```
用户输入: "帮我截个图"
  → chatStore.sendMessage("帮我截个图")
  → invoke("send_acp_message", {text: "帮我截个图"})
  → acp_client session/prompt → Kaya

Kaya 收到的: "帮我截个图"
Kaya 不知道: 消息来自哪个客户端
```

### 改动后

```rust
// lib.rs
#[tauri::command]
fn send_acp_message(text: String, state: tauri::State<AppState>) -> Result<(), String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let config = config.as_ref().ok_or("配置未加载")?;
    
    // 自动标记来源
    let annotated = format!(
        "[file-transfer-hub | client: {}]\n{}",
        config.client_id, text
    );
    
    let msg = match annotated.trim() {
        "/session new" => "__new_session__".to_string(),
        "/cancel" | "/session cancel" => "__cancel__".to_string(),
        _ => annotated,
    };
    
    let tx = state.acp_tx.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = tx.as_ref() {
        tx.try_send(msg).map_err(|e| format!("发送失败: {}", e))
    } else {
        Err("ACP 客户端未启动".to_string())
    }
}
```

**效果：**

```
Kaya 收到:
[file-transfer-hub | client: pc-01]
帮我截个图
```

Kaya 可以识别 `client: pc-01` 来源，直接调用 `call_client_tool(client_id="pc-01", ...)`。

### 只改一个点

仅 `send_acp_message` 函数，不改 `acp_client.rs`。命令消息 (`/session new`, `/cancel`) 不加前缀。

---

## 2. 工具可扩展架构

### 目录结构

```
client/src-tauri/src/tools/
├── mod.rs              // Tool trait + ToolManager
├── screenshot.rs       // ScreenshotTool
├── clipboard.rs        // ClipboardTool
├── file_search.rs      // FileSearchTool
└── uia_tree.rs         // UiaTreeTool
```

### Tool trait

```rust
// tools/mod.rs

pub struct ToolContent {
    pub text: String,
}

pub struct ToolResult {
    pub content: Vec<ToolContent>,
    pub is_error: bool,
    pub upload_path: Option<String>,
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> serde_json::Value;
    fn is_enabled(&self) -> bool;
    fn set_enabled(&mut self, enabled: bool);
    async fn execute(&self, args: &serde_json::Value) -> ToolResult;
}
```

### 每个工具的结构

```rust
// tools/screenshot.rs
pub struct ScreenshotTool {
    enabled: bool,
}

impl ScreenshotTool {
    pub fn new(config: &AppConfig) -> Self {
        let enabled = config.tool_permissions
            .get("take_screenshot")
            .copied()
            .unwrap_or(true);
        Self { enabled }
    }
}

#[async_trait]
impl Tool for ScreenshotTool {
    fn name(&self) -> &'static str { "take_screenshot" }
    fn description(&self) -> &'static str { "Capture the Windows desktop screen" }
    fn input_schema(&self) -> Value { /* 同现有 */ }
    fn is_enabled(&self) -> bool { self.enabled }
    fn set_enabled(&mut self, e: bool) { self.enabled = e; }
    
    async fn execute(&self, args: &Value) -> ToolResult {
        // 直接从 tool_executor::execute_tool 搬过来
    }
}
```

### ToolManager

```rust
// tools/mod.rs
pub struct ToolManager {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolManager {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            tools: vec![
                Box::new(ScreenshotTool::new(config)),
                Box::new(ClipboardTool::new(config)),
                Box::new(FileSearchTool::new(config)),
                Box::new(UiaTreeTool::new(config)),
            ],
        }
    }

    /// 注册到服务端：只返回 enabled 的工具
    pub fn enabled_defs(&self) -> Vec<ToolDef> {
        self.tools.iter()
            .filter(|t| t.is_enabled())
            .map(|t| ToolDef {
                name: t.name(),
                description: t.description(),
                input_schema: t.input_schema(),
            })
            .collect()
    }

    /// 执行工具调用
    pub async fn execute(&self, name: &str, args: &Value) -> Option<ToolResult> {
        for tool in &self.tools {
            if tool.name() == name && tool.is_enabled() {
                return Some(tool.execute(args).await);
            }
        }
        None
    }
    
    /// 更新工具启用状态（前端触发后调用）
    pub fn set_enabled(&mut self, name: &str, enabled: bool) {
        for tool in &mut self.tools {
            if tool.name() == name {
                tool.set_enabled(enabled);
                return;
            }
        }
    }
}
```

### 现有代码适配

**ws_client.rs** — 认证成功后的工具注册：

```rust
// 改动前
let tools_msg = serde_json::json!({
    "type": "register_tools",
    "tools": local_tools().iter().map(...).collect(),
});

// 改动后 — ToolManager 实例从 lib.rs 传入
let tools_msg = serde_json::json!({
    "type": "register_tools",
    "tools": tool_manager.enabled_defs().iter().map(...).collect(),
});
```

**tool_executor.rs** — 原有 `execute_tool` 函数保留入口，内部分发委托给 ToolManager：

```rust
pub async fn execute_tool(name: &str, args: &Value) -> ToolResult {
    if let Some(app) = APP_HANDLE.get() {
        if let Some(state) = app.try_state::<AppState>() {
            if let Some(manager) = state.tool_manager.lock().unwrap().as_ref() {
                if let Some(result) = manager.execute(name, args).await {
                    return result;
                }
            }
        }
    }
    ToolResult {
        content: vec![ToolContent { text: format!("Tool not found or disabled: {}", name) }],
        is_error: true,
        upload_path: None,
    }
}
```

---

## 3. 前端工具权限控制

### AppConfig 扩展

```rust
// config.rs
pub struct AppConfig {
    pub server_url: String,
    pub client_id: String,
    pub passkey: String,
    pub storage_path: String,
    pub acp_url: Option<String>,
    pub acp_cwd: Option<String>,
    pub tool_permissions: HashMap<String, bool>,  // 新增
}
```

默认值：

```rust
fn default_tool_permissions() -> HashMap<String, bool> {
    let mut m = HashMap::new();
    m.insert("take_screenshot".into(), true);
    m.insert("get_clipboard".into(), true);
    m.insert("file_search".into(), true);
    m.insert("get_uia_tree".into(), true);
    m
}
```

### 前端 UI

在 `CapabilitiesPage.vue` 的工具表中，每行增加 `[● 开启 / ○ 关闭]` 切换开关：

```vue
<!-- 新增列头 -->
<th>权限</th>

<!-- 每行切换 -->
<td>
  <label class="toggle">
    <input type="checkbox" :checked="item.enabled"
           @change="toggleTool(item.name, $event.target.checked)" />
    <span class="toggle-label">{{ item.enabled ? '开启' : '关闭' }}</span>
  </label>
</td>
```

切换触发重新注册：

```typescript
async function toggleTool(name: string, enabled: boolean) {
  await invoke("set_tool_enabled", { name, enabled });
  // TODO: 触发重新注册
}
```

### Tauri command

```rust
#[tauri::command]
fn set_tool_enabled(name: String, enabled: bool, state: tauri::State<AppState>) -> Result<(), String> {
    // 1. 更新 AppState 中 ToolManager 的 enabled 状态
    if let Some(s) = app.try_state::<AppState>() {
        if let Ok(mut mgr) = s.tool_manager.lock() {
            if let Some(mgr) = mgr.as_mut() {
                mgr.set_enabled(&name, enabled);
            }
        }
    }
    // 2. 持久化到配置
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut config) = *config {
        config.tool_permissions.insert(name, enabled);
        // 保存到磁盘
    }
    // 3. 触发重新注册
    // 通过 upload_tx 发信号给 ws_client 重新发送 register_tools
    Ok(())
}
```

### 重新注册流程

```
toggle → set_tool_enabled → 更新内存 + 保存配置
  → ws_client 收到重注册信号
  → 发送 register_tools（仅 enabled 的工具）
  → 服务端 ToolRegistry 更新
```

注册信号通过 ws_client 现有的消息通道传递（复用 `upload_rx` 类似的机制或新增事件）。

---

## 4. 数据流总览

```
┌─ 用户发消息 ────────────────────────────────────────┐
│                                                      │
│  "帮我截个图"                                        │
│    → chatStore.sendMessage()                         │
│    → send_acp_message                                │
│    → 自动加前缀 "[file-transfer-hub | client: X]"    │
│    → acp_client → session/prompt → Kaya              │
│                                                      │
│  Kaya 看到 client: X，直接调 call_client_tool        │
│  → MCP → server → ws_handler → WS → client           │
│  → ToolManager.execute("take_screenshot")            │
│                                                      │
├─ 前端开关工具 ───────────────────────────────────────┤
│                                                      │
│  toggle tool → set_tool_enabled                      │
│    → 更新 ToolManager.enabled                        │
│    → 持久化到 AppConfig                              │
│    → 重发 register_tools（已过滤）                    │
│    → 服务端 ToolRegistry 更新                        │
└──────────────────────────────────────────────────────┘
```

---

## 5. 向后兼容

| 项目 | 兼容性 |
|------|--------|
| 现有配置 | `tool_permissions` 为新增字段，旧配置加载时默认为全开启 |
| 现有 `tool_executor::execute_tool` | 保留并重定向到 ToolManager，外部调用无感 |
| `local_tools()` | 移除后迁移到 `ToolManager::enabled_defs()` |
| 前端 | CapabilitiesPage 新增列，不影响其他页面 |

---

## 6. 测试策略

- 每个工具独立测试执行逻辑（现有 `tool_executor` 测试可迁移）
- ToolManager 单元测试：注册/过滤/执行
- 前端 toggle 需要 Tauri 环境，手动测试
