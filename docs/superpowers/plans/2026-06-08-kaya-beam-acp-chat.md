# kaya-beam ACP 聊天集成 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在 kaya-beam Windows Tauri 客户端中集成 ACP (Agent Client Protocol) 聊天，支持全局快捷键唤起、与 Kaya 双向文字对话。

**架构：** 在现有 Tauri 应用中新增 `acp_client.rs`（Rust ACP WebSocket 客户端），通过 JSON-RPC 2.0 协议连接到已有的 `stdio-to-ws:8765` → `opencode acp` 桥接。前端新增 Vue 3 聊天页面（ChatPage.vue）和聊天 store。全局热键 `Ctrl+Alt+K` 通过 `tauri-plugin-global-shortcut` 实现。

**技术栈：** Rust (tokio-tungstenite), Tauri 2, tauri-plugin-global-shortcut, Vue 3 + Pinia + TypeScript

---

## 文件清单

### 新建文件
- `client/src-tauri/src/acp_client.rs` — ACP WebSocket 客户端（JSON-RPC 2.0）
- `client/src/views/ChatPage.vue` — 聊天页面
- `client/src/stores/chat.ts` — 聊天状态管理
- `client/src/components/ChatMessage.vue` — 消息气泡组件
- `client/src/components/ChatInput.vue` — 输入框组件

### 修改文件
- `client/src-tauri/Cargo.toml` — 添加 `tauri-plugin-global-shortcut`
- `client/src-tauri/capabilities/default.json` — 添加 global-shortcut 权限
- `client/src-tauri/src/lib.rs` — AppState 扩展、Tauri 命令注册、热键、ACP 事件循环
- `client/src-tauri/src/tray.rs` — 添加"打开对话"菜单项
- `client/src/main.ts` — 添加 `/chat` 路由
- `client/src/App.vue` — 监听 `toggle-chat` 事件
- `client/src/lib/tauri.ts` — 添加 ACP 相关 Tauri bridge 函数

---

## ACP 协议参考

ACP 是基于 JSON-RPC 2.0 的协议，通过 WebSocket（子协议 `acp.v1`）传输，消息以 `\n` 分隔。

**初始化流程：**
```
客户端 → 服务器: {"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
服务器 → 客户端: {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"..."}}
```

**新建会话：**
```
客户端 → 服务器: {"jsonrpc":"2.0","id":2,"method":"session/new","params":{"title":"kaya-beam chat"}}
服务器 → 客户端: {"jsonrpc":"2.0","id":2,"result":{"sessionId":"ses_xxx"}}
```

**发送消息（流式响应）：**
```
客户端 → 服务器: {"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"sessionId":"ses_xxx","messages":[{"role":"user","content":"你好"}]}}
服务器 → 客户端: {"jsonrpc":"2.0","id":3,"result":{"messages":[{"role":"assistant","content":"你好！"}]}}
```

**取消：**
```
客户端 → 服务器: {"jsonrpc":"2.0","method":"session/cancel","params":{"sessionId":"ses_xxx"}}
```

**心跳（客户端每 25 秒）：**
```
客户端 → 服务器: {"jsonrpc":"2.0","method":"$/ping"}
```

**stdio-to-ws 桥接连接时：**
```
服务器 → 客户端: {"type":"connected","clientId":"<uuid>"}
```

---

### 任务 1：Rust 依赖和权限配置

**文件：**
- 修改：`client/src-tauri/Cargo.toml`
- 修改：`client/src-tauri/capabilities/default.json`
- 修改：`client/src-tauri/src/lib.rs`（模块声明）

- [ ] **步骤 1：添加 tauri-plugin-global-shortcut 依赖**

编辑 `Cargo.toml`，在 `[dependencies]` 中添加：

```toml
tauri-plugin-global-shortcut = "2"
```

- [ ] **步骤 2：添加权限声明**

编辑 `capabilities/default.json`：

```json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "core:window:default",
    "core:window:allow-show",
    "core:window:allow-set-focus",
    "core:event:default",
    "global-shortcut:default",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister"
  ]
}
```

- [ ] **步骤 3：在 lib.rs 声明 acp_client 模块**

```rust
// 在现有 mod 声明后添加
mod acp_client;
```

- [ ] **步骤 4：Commit**

```bash
git add client/src-tauri/Cargo.toml client/src-tauri/capabilities/default.json client/src-tauri/src/lib.rs
git commit -m "chore: add global-shortcut dependency and acp_client module"
```

---

### 任务 2：实现 `acp_client.rs`

**文件：**
- 创建：`client/src-tauri/src/acp_client.rs`

`acp_client.rs` 实现 ACP 的 JSON-RPC 2.0 客户端，模式与 `ws_client.rs` 一致。

- [ ] **步骤 1：创建 acp_client.rs 骨架**

```rust
use crate::config::AppConfig;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

/// ACP 事件，用于通知 Tauri 前端
#[derive(Debug, Clone)]
pub enum AcpEvent {
    Connected,
    Disconnected,
    /// 收到消息响应（非流式）
    MessageResponse { content: String },
    /// 会话就绪
    SessionReady { session_id: String },
    Error(String),
}

/// JSON-RPC 请求 ID 生成器
type RequestId = u64;

const RECONNECT_BASE_DELAY: u64 = 1;
const RECONNECT_MAX_DELAY: u64 = 60;
```

- [ ] **步骤 2：实现 run_acp_client 主循环**

```rust
pub async fn run_acp_client(
    server_url: String,
    event_tx: mpsc::Sender<AcpEvent>,
    msg_rx: mpsc::Receiver<String>,
) {
    if let Err(e) = Url::parse(&server_url) {
        let _ = event_tx.send(AcpEvent::Error(format!("Invalid URL: {}", e))).await;
        return;
    }

    let mut retry_delay = RECONNECT_BASE_DELAY;
    let mut next_id: RequestId = 1;
    let mut session_id: Option<String> = None;

    loop {
        let connect_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            connect_async(&server_url),
        ).await;

        let (ws_stream, _) = match connect_result {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                let _ = event_tx.send(AcpEvent::Error(format!("Connection failed: {}", e))).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
                continue;
            }
            Err(_) => {
                let _ = event_tx.send(AcpEvent::Error("连接超时（5 秒）".to_string())).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
                continue;
            }
        };

        retry_delay = RECONNECT_BASE_DELAY;
        let _ = event_tx.send(AcpEvent::Connected).await;

        let (mut write, mut read) = ws_stream.split();

        // JSON-RPC 初始化
        let init_id = next_id;
        next_id += 1;
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {}
        });
        if write.send(Message::Text(init_req.to_string())).await.is_err() {
            let _ = event_tx.send(AcpEvent::Disconnected).await;
            continue;
        }

        // 心跳定时器（每 25 秒）
        let mut heartbeat = tokio::time::interval(tokio::time::Duration::from_secs(25));
        heartbeat.reset();

        // 合并 msg_rx 和 read 的事件循环
        let mut msg_rx = msg_rx;
        let mut msg_rx_open = true;

        loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    let ping = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "$/ping"
                    });
                    let _ = write.send(Message::Text(ping.to_string())).await;
                }
                user_msg = msg_rx.recv(), if msg_rx_open => {
                    match user_msg {
                        Some(text) => {
                            if text == "__cancel__" {
                                // 取消当前请求
                                if let Some(ref sid) = session_id {
                                    let cancel = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "method": "session/cancel",
                                        "params": {"sessionId": sid}
                                    });
                                    let _ = write.send(Message::Text(cancel.to_string())).await;
                                }
                            } else if text == "__new_session__" {
                                // 新建会话
                                let new_id = next_id;
                                next_id += 1;
                                let req = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": new_id,
                                    "method": "session/new",
                                    "params": {"title": "kaya-beam chat"}
                                });
                                let _ = write.send(Message::Text(req.to_string())).await;
                            } else {
                                // 发送用户消息
                                if let Some(ref sid) = session_id {
                                    let prompt_id = next_id;
                                    next_id += 1;
                                    let req = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "id": prompt_id,
                                        "method": "session/prompt",
                                        "params": {
                                            "sessionId": sid,
                                            "messages": [{"role": "user", "content": text}]
                                        }
                                    });
                                    let _ = write.send(Message::Text(req.to_string())).await;
                                } else {
                                    let _ = event_tx.send(AcpEvent::Error("会话未就绪，请稍候".to_string())).await;
                                }
                            }
                        }
                        None => {
                            msg_rx_open = false;
                        }
                    }
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // stdio-to-ws 可能发送换行分隔的 JSON
                            for line in text.split('\n') {
                                let line = line.trim();
                                if line.is_empty() { continue; }
                                if let Ok(val) = serde_json::from_str::<Value>(&line) {
                                    // 处理 stdio-to-ws 的连接消息
                                    if val.get("type").and_then(|t| t.as_str()) == Some("connected") {
                                        continue;
                                    }
                                    // 处理 JSON-RPC 响应
                                    if let Some(id) = val.get("id").and_then(|i| i.as_u64()) {
                                        if id == init_id {
                                            // initialize 响应 → 创建会话
                                            let new_id = next_id;
                                            next_id += 1;
                                            let req = serde_json::json!({
                                                "jsonrpc": "2.0",
                                                "id": new_id,
                                                "method": "session/new",
                                                "params": {"title": "kaya-beam chat"}
                                            });
                                            let _ = write.send(Message::Text(req.to_string())).await;
                                        } else if let Some(result) = val.get("result") {
                                            if let Some(sid) = result.get("sessionId").and_then(|s| s.as_str()) {
                                                session_id = Some(sid.to_string());
                                                let _ = event_tx.send(AcpEvent::SessionReady {
                                                    session_id: sid.to_string(),
                                                }).await;
                                            }
                                            // prompt 响应
                                            if let Some(msgs) = result.get("messages").and_then(|m| m.as_array()) {
                                                for msg_val in msgs {
                                                    if let Some(content) = msg_val.get("content").and_then(|c| c.as_str()) {
                                                        if let Some(role) = msg_val.get("role").and_then(|r| r.as_str()) {
                                                            if role == "assistant" {
                                                                let _ = event_tx.send(AcpEvent::MessageResponse {
                                                                    content: content.to_string(),
                                                                }).await;
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        } else if let Some(error) = val.get("error") {
                                            let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown error");
                                            let _ = event_tx.send(AcpEvent::Error(format!("ACP error: {}", msg))).await;
                                        }
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Close(_))) => break,
                        Some(Err(e)) => {
                            let _ = event_tx.send(AcpEvent::Error(format!("WebSocket error: {}", e))).await;
                            break;
                        }
                        None => break,
                        _ => {}
                    }
                }
            }
        }

        let _ = event_tx.send(AcpEvent::Disconnected).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
        retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
    }
}
```

- [ ] **步骤 3：Commit**

```bash
git add client/src-tauri/src/acp_client.rs
git commit -m "feat: add ACP JSON-RPC 2.0 WebSocket client"
```

---

### 任务 3：扩展 `lib.rs` — AppState、命令、热键、ACP 事件循环

**文件：**
- 修改：`client/src-tauri/src/lib.rs`

- [ ] **步骤 1：扩展 AppState**

```rust
struct AppState {
    config: Mutex<Option<AppConfig>>,
    connection_status: Mutex<String>,
    ws_started: Mutex<bool>,
    acp_started: Mutex<bool>,
    acp_tx: Mutex<Option<tokio::sync::mpsc::Sender<String>>>,  // 向 acp_client 发消息的通道
}
```

在 `run()` 的 `.manage()` 中初始化：

```rust
.manage(AppState {
    config: Mutex::new(None),
    connection_status: Mutex::new("未连接".to_string()),
    ws_started: Mutex::new(false),
    acp_started: Mutex::new(false),
    acp_tx: Mutex::new(None),
})
```

- [ ] **步骤 2：添加 ACP 启动函数和事件循环**

```rust
fn start_acp_client(app: &AppHandle, config: &AppConfig) {
    use tokio::sync::mpsc;

    let (event_tx, mut event_rx) = mpsc::channel::<acp_client::AcpEvent>(100);
    let (msg_tx, msg_rx) = mpsc::channel::<String>(100);

    // 保存 msg_tx 到 AppState
    if let Some(s) = app.try_state::<AppState>() {
        if let Ok(mut tx) = s.acp_tx.lock() {
            *tx = Some(msg_tx);
        }
    }

    // ACP 桥接地址：基于文件传输服务器的地址推导
    let acp_url = acp_url_from_config(&config.server_url);

    tauri::async_runtime::spawn(async move {
        acp_client::run_acp_client(acp_url, event_tx, msg_rx).await;
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match &event {
                acp_client::AcpEvent::Connected => {
                    let _ = handle.emit("acp-status", serde_json::json!({"status": "已连接"}));
                }
                acp_client::AcpEvent::Disconnected => {
                    let _ = handle.emit("acp-status", serde_json::json!({"status": "已断开"}));
                }
                acp_client::AcpEvent::MessageResponse { content } => {
                    let _ = handle.emit("acp-message", serde_json::json!({"content": content}));
                }
                acp_client::AcpEvent::SessionReady { session_id } => {
                    let _ = handle.emit("acp-session", serde_json::json!({"sessionId": session_id}));
                }
                acp_client::AcpEvent::Error(e) => {
                    let _ = handle.emit("acp-status", serde_json::json!({"status": format!("错误: {}", e)}));
                }
            }
        }
    });
}

/// 从文件传输服务器 URL 推导 ACP 桥接地址
fn acp_url_from_config(server_url: &str) -> String {
    // ws://10.0.0.240:9765 → ws://10.0.0.240:8765
    if let Some(rest) = server_url.strip_prefix("ws://") {
        if let Some(host) = rest.split(':').next() {
            return format!("ws://{}:8765", host);
        }
    }
    "ws://127.0.0.1:8765".to_string()
}
```

- [ ] **步骤 3：注册 Tauri 命令**

```rust
#[tauri::command]
fn send_acp_message(text: String, state: tauri::State<AppState>) -> Result<(), String> {
    let tx = state.acp_tx.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = tx.as_ref() {
        tx.try_send(text).map_err(|e| format!("发送失败: {}", e))
    } else {
        Err("ACP 客户端未启动".to_string())
    }
}

#[tauri::command]
fn start_acp(state: tauri::State<AppState>) -> Result<(), String> {
    let mut started = state.acp_started.lock().map_err(|e| e.to_string())?;
    if *started {
        return Ok(());
    }
    *started = true;
    Ok(())
}
```

在 `invoke_handler` 中注册：

```rust
.invoke_handler(tauri::generate_handler![
    load_config, save_config, get_connection_status,
    send_acp_message, start_acp,
])
```

- [ ] **步骤 4：注册全局热键**

在 `setup()` 中添加：

```rust
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, Code, Modifiers};

// 注册全局热键
app.handle().plugin(
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event == ShortcutEvent::Pressed {
                let _ = app.emit("toggle-chat", ());
            }
        })
        .build(),
)?;

// 注册 Ctrl+Alt+K
app.global_shortcut().register(
    Shortcut::new(Some(Modifiers::ALT | Modifiers::CONTROL), Code::KeyK),
)?;
```

- [ ] **步骤 5：在 setup() 中判断启动 ACP 客户端**

在 `setup()` 中的 `if let Some(config) = cfg { ... }` 块末尾添加：

```rust
// ACP 客户端跟随文件 WS 一起启动
start_acp_client(&handle, &config);
```

- [ ] **步骤 6：Commit**

```bash
git add client/src-tauri/src/lib.rs
git commit -m "feat: add ACP commands, hotkey, and event loop"
```

---

### 任务 4：系统托盘添加「打开对话」

**文件：**
- 修改：`client/src-tauri/src/tray.rs`

- [ ] **步骤 1：添加「打开对话」菜单项**

```rust
let chat_item = MenuItemBuilder::with_id("chat", "打开对话").build(app)?;
```

在 MenuBuilder 中插入：

```rust
MenuBuilder::new(app)
    .item(&show_item)
    .item(&chat_item)     // 新增
    .separator()
    .item(&recent_item)
    .separator()
    .item(&quit_item)
    .build()?;
```

在 `on_menu_event` 中添加：

```rust
"chat" => {
    let _ = app.emit("toggle-chat", ());
}
```

- [ ] **步骤 2：Commit**

```bash
git add client/src-tauri/src/tray.rs
git commit -m "feat: add 'open chat' tray menu item"
```

---

### 任务 5：前端聊天 Store

**文件：**
- 创建：`client/src/stores/chat.ts`
- 修改：`client/src/lib/tauri.ts`（添加 ACP bridge 函数）

- [ ] **步骤 1：创建 chat Store**

```typescript
import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

export const useChatStore = defineStore("chat", () => {
  const messages = ref<ChatMessage[]>([]);
  const connected = ref(false);
  const sessionReady = ref(false);
  const responding = ref(false);
  const error = ref<string | null>(null);
  let msgCounter = 0;

  async function init() {
    // 监听 ACP 事件
    await listen<{ status: string }>("acp-status", (e) => {
      if (e.payload.status === "已连接") {
        connected.value = true;
        error.value = null;
      } else if (e.payload.status.startsWith("错误")) {
        connected.value = false;
        error.value = e.payload.status;
      } else {
        connected.value = false;
      }
    });

    await listen<{ sessionId: string }>("acp-session", () => {
      sessionReady.value = true;
    });

    await listen<{ content: string }>("acp-message", (e) => {
      const last = messages.value[messages.value.length - 1];
      if (last && last.role === "assistant" && !responding.value) {
        // 完整响应
        last.content = e.payload.content;
      } else {
        messages.value.push({
          id: `msg_${++msgCounter}`,
          role: "assistant",
          content: e.payload.content,
          timestamp: Date.now(),
        });
        responding.value = false;
      }
    });

    // 启动 ACP 客户端
    try {
      await invoke("start_acp");
    } catch {
      // 可能已经启动了
    }
  }

  async function sendMessage(text: string) {
    if (!text.trim()) return;
    messages.value.push({
      id: `msg_${++msgCounter}`,
      role: "user",
      content: text,
      timestamp: Date.now(),
    });
    responding.value = true;
    error.value = null;

    try {
      await invoke("send_acp_message", { text });
    } catch (e: any) {
      error.value = String(e);
      responding.value = false;
    }
  }

  function clearConversation() {
    messages.value = [];
  }

  return {
    messages, connected, sessionReady, responding, error,
    init, sendMessage, clearConversation,
  };
});
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/stores/chat.ts
git commit -m "feat: add chat store with ACP event listeners"
```

---

### 任务 6：前端聊天组件（ChatMessage + ChatInput）

**文件：**
- 创建：`client/src/components/ChatMessage.vue`
- 创建：`client/src/components/ChatInput.vue`

- [ ] **步骤 1：创建 ChatMessage.vue**

```vue
<script setup lang="ts">
defineProps<{
  role: "user" | "assistant" | "system";
  content: string;
}>();
</script>

<template>
  <div class="message" :class="role">
    <div class="bubble">
      <div class="role-label">{{ role === "user" ? "你" : "Kaya" }}</div>
      <div class="content">{{ content }}</div>
    </div>
  </div>
</template>

<style scoped>
.message {
  display: flex;
  margin-bottom: 12px;
}
.message.user {
  justify-content: flex-end;
}
.message.assistant {
  justify-content: flex-start;
}
.bubble {
  max-width: 80%;
  padding: 10px 14px;
  border-radius: 12px;
  background: #f0f0f0;
}
.message.user .bubble {
  background: #396cd8;
  color: #fff;
}
.role-label {
  font-size: 0.75rem;
  font-weight: 600;
  margin-bottom: 4px;
  opacity: 0.7;
}
.content {
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
```

- [ ] **步骤 2：创建 ChatInput.vue**

```vue
<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{
  disabled: boolean;
}>();

const emit = defineEmits<{
  send: [text: string];
}>();

const input = ref("");

function onSend() {
  const text = input.value.trim();
  if (!text || props.disabled) return;
  emit("send", text);
  input.value = "";
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    onSend();
  }
}
</script>

<template>
  <div class="input-area">
    <textarea
      v-model="input"
      :disabled="disabled"
      placeholder="输入消息…"
      rows="2"
      @keydown="onKeydown"
    />
    <button :disabled="disabled || !input.trim()" @click="onSend">
      发送
    </button>
  </div>
</template>

<style scoped>
.input-area {
  display: flex;
  gap: 8px;
  align-items: flex-end;
}
textarea {
  flex: 1;
  border: 1px solid #ccc;
  border-radius: 8px;
  padding: 10px;
  font-size: 14px;
  resize: none;
  outline: none;
  font-family: inherit;
}
textarea:focus {
  border-color: #396cd8;
}
button {
  padding: 10px 20px;
  background: #396cd8;
  color: #fff;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  font-size: 14px;
  white-space: nowrap;
}
button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/components/ChatMessage.vue client/src/components/ChatInput.vue
git commit -m "feat: add ChatMessage and ChatInput components"
```

---

### 任务 7：前端聊天页面 ChatPage.vue

**文件：**
- 创建：`client/src/views/ChatPage.vue`

- [ ] **步骤 1：创建 ChatPage.vue**

```vue
<script setup lang="ts">
import { onMounted, ref, nextTick } from "vue";
import { useChatStore } from "../stores/chat";
import ChatMessage from "../components/ChatMessage.vue";
import ChatInput from "../components/ChatInput.vue";

const chatStore = useChatStore();
const messagesRef = ref<HTMLElement | null>(null);

onMounted(async () => {
  await chatStore.init();
  scrollToBottom();
});

function scrollToBottom() {
  nextTick(() => {
    if (messagesRef.value) {
      messagesRef.value.scrollTop = messagesRef.value.scrollHeight;
    }
  });
}

// 每收到新消息时滚动到底部
chatStore.$subscribe(() => {
  scrollToBottom();
});

function onSend(text: string) {
  chatStore.sendMessage(text);
}
</script>

<template>
  <div class="chat-page">
    <div class="header">
      <h2>Kaya 对话</h2>
      <span v-if="chatStore.connected" class="badge online">已连接</span>
      <span v-else class="badge offline">未连接</span>
    </div>

    <div class="messages" ref="messagesRef">
      <ChatMessage
        v-for="msg in chatStore.messages"
        :key="msg.id"
        :role="msg.role"
        :content="msg.content"
      />
      <div v-if="chatStore.responding" class="typing-indicator">
        Kaya 正在输入…
      </div>
      <div v-if="chatStore.messages.length === 0" class="empty-state">
        开始与 Kaya 对话
      </div>
    </div>

    <p v-if="chatStore.error" class="error">{{ chatStore.error }}</p>

    <ChatInput
      :disabled="!chatStore.connected"
      @send="onSend"
    />
  </div>
</template>

<style scoped>
.chat-page {
  width: 100%;
  max-width: 600px;
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 8px;
  border-bottom: 1px solid #eee;
}
.header h2 {
  margin: 0;
  font-size: 1.1rem;
}
.badge {
  font-size: 0.75rem;
  padding: 2px 8px;
  border-radius: 10px;
}
.badge.online {
  background: #e8f5e9;
  color: #2e7d32;
}
.badge.offline {
  background: #f5f5f5;
  color: #888;
}
.messages {
  flex: 1;
  overflow-y: auto;
  padding: 8px 0;
}
.empty-state {
  text-align: center;
  color: #aaa;
  padding: 40px 0;
}
.typing-indicator {
  color: #888;
  font-size: 0.85rem;
  padding: 8px 0;
}
.error {
  color: #d32f2f;
  font-size: 0.85rem;
}
</style>
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/views/ChatPage.vue
git commit -m "feat: add chat page with ACP messaging"
```

---

### 任务 8：路由注册 + App.vue 热键监听

**文件：**
- 修改：`client/src/main.ts`
- 修改：`client/src/App.vue`

- [ ] **步骤 1：注册 /chat 路由**

编辑 `main.ts`，在 routes 数组中添加：

```typescript
import ChatPage from "./views/ChatPage.vue";

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/config" },
    { path: "/config", component: ConfigPage },
    { path: "/status", component: StatusPage },
    { path: "/chat", component: ChatPage },
  ],
});
```

- [ ] **步骤 2：App.vue 监听 toggle-chat 事件**

在 `onMounted` 中添加：

```typescript
import { listen } from "@tauri-apps/api/event";
import { useRouter } from "vue-router";

const router = useRouter();

onMounted(async () => {
  await appStore.load();

  // 监听热键切换聊天
  await listen("toggle-chat", () => {
    if (router.currentRoute.value.path === "/chat") {
      // 已在聊天页，聚焦窗口
    } else {
      router.push("/chat");
    }
  });

  if (appStore.config) {
    router.push("/status");
  } else {
    router.push("/config");
  }
});
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/main.ts client/src/App.vue
git commit -m "feat: add /chat route and toggle-chat hotkey listener"
```

---

### 任务 9：端到端验证

- [ ] **步骤 1：编译前端 + Rust**

```bash
cd client
npm run build
npm run tauri build
```

- [ ] **步骤 2：测试热键**

在 Windows 上按 `Ctrl+Alt+K`，确认窗口切换到聊天页面。

- [ ] **步骤 3：测试消息收发**

在聊天输入框中输入消息，确认：
- ACP 桥接连接成功（显示「已连接」）
- 消息发送后 Kaya 回复
- 消息按时间正序展示

- [ ] **步骤 4：测试托盘菜单**

右键系统托盘 → 点击「打开对话」→ 切换到聊天页面。

- [ ] **步骤 5：Commit**

```bash
git add -A
git commit -m "feat: ACP chat integration complete"
```
