# kaya-beam ACP 聊天集成设计

## 概述

在 kaya-beam Windows Tauri 客户端中集成 ACP（Agent Client Protocol）聊天功能，实现通过快捷键唤起与 Kaya 的双向文字对话。

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│ Windows Tauri App (kaya-beam)                                │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐                         │
│  │ ws_client.rs │  │ acp_client.rs│ ← 新增                    │
│  │ (文件接收)    │  │ (ACP 聊天)   │                          │
│  │ → :9765      │  │ → :8765      │                          │
│  └──────┬───────┘  └──────┬───────┘                         │
│         │                 │                                   │
│         ▼                 ▼                                   │
│  ┌────────────────────────────────────┐                      │
│  │  lib.rs 事件循环 → Tauri emit()    │                      │
│  └──────────────┬─────────────────────┘                      │
│                 │                                             │
│  Vue 3 前端     │                                             │
│  ┌──────────────▼─────────────────────┐                      │
│  │ /status (文件接收状态)               │                      │
│  │ /chat   (ACP 聊天)  ← 新增           │                      │
│  │ stores: app, file, chat  ← 新增     │                      │
│  └────────────────────────────────────┘                      │
│                                                              │
│  全局热键: Ctrl+Alt+K → 切换聊天窗口                          │
│  托盘菜单: 新增「打开对话」项                                  │
└──────────────────────────────────────────────────────────────┘
           │                          │
           ▼                          ▼
┌──────────────────┐    ┌──────────────────────┐
│ Python 服务端      │    │ stdio-to-ws:8765     │
│ (文件传输, :9765)  │    │ → opencode acp       │
└──────────────────┘    │ (ACP 桥接，已运行)     │
                         └──────────────────────┘
```

### 两个 WebSocket 连接共存

- `ws_client.rs` → Python 服务端 `:9765`：文件推送接收（已有）
- `acp_client.rs` → ACP 桥接 `:8765`：聊天消息收发（新增）
- 二者相互独立，共享 Tauri 事件循环

## 组件详述

### 1. Rust 后端：`acp_client.rs`

新增文件，模式与 `ws_client.rs` 一致。

#### 事件枚举

```rust
pub enum AcpEvent {
    /// ACP 桥接已连接
    Connected,
    /// ACP 桥接已断开
    Disconnected,
    /// 收到完整消息（适用于非流式响应）
    Message { role: String, content: String },
    /// 流式响应块（打字机效果）
    StreamChunk { content: String, done: bool },
    /// 错误
    Error(String),
}
```

#### 主循环

```rust
pub async fn run_acp_client(
    server_url: String,       // ws://服务器IP:8765
    event_tx: mpsc::Sender<AcpEvent>,
    msg_rx: mpsc::Receiver<String>,  // 来自前端的用户输入
) {
    // 1. 连接 ACP 桥接 WebSocket
    //    → 收到 {"type":"connected","clientId":"..."}
    //    → 发送 AcpEvent::Connected
    
    // 2. 双工循环：
    //    tokio::select! {
    //        msg = msg_rx.recv() => {
    //            // 用户消息 → 发到 ACP WebSocket
    //            // 格式由 ACP 协议定义
    //        }
    //        resp = ws_read.next() => {
    //            // 解析 ACP 响应
    //            // → 流式块 → AcpEvent::StreamChunk
    //            // → 完整消息 → AcpEvent::Message
    //            // → 错误 → AcpEvent::Error
    //        }
    //        _ = heartbeat.tick() => {
    //            // 可选的 ACP 心跳
    //        }
    //    }
    
    // 3. 断开重连（指数退避, 1s→60s, 同 ws_client）
}
```

#### ACP 协议消息格式

参考标准 ACP（Agent Client Protocol）实现：

- **客户端 → 服务器**：
  ```json
  {
    "type": "message",
    "role": "user",
    "content": "用户输入文本"
  }
  ```
- **服务器 → 客户端**（流式）：
  ```json
  {
    "type": "chunk",
    "content": "响应片段",
    "done": false
  }
  ```
- **服务器 → 客户端**（结束）：
  ```json
  {
    "type": "chunk",
    "content": "",
    "done": true
  }
  ```

> 注：实际消息格式需在实现时对照 Agmente 或 ACP UI 源码确认，此处为占位设计。

### 2. Rust 后端：`lib.rs` 改动

#### AppState 扩展

```rust
struct AppState {
    config: Mutex<Option<AppConfig>>,
    connection_status: Mutex<String>,
    ws_started: Mutex<bool>,
    acp_started: Mutex<bool>,           // 新增
    acp_connected: Mutex<bool>,         // 新增
}
```

#### 新增 Tauri 命令

```rust
#[tauri::command]
fn start_acp(state: State<AppState>) -> Result<(), String> {
    // 启动 ACP 客户端（如果尚未启动）
}

#[tauri::command]
fn send_acp_message(text: String, state: State<AppState>) -> Result<(), String> {
    // 通过 msg_rx 通道发送到 acp_client 循环
}

#[tauri::command]
fn close_acp(state: State<AppState>) -> Result<(), String> {
    // 关闭 ACP 会话
}

#[tauri::command]
fn get_acp_status(state: State<AppState>) -> Result<String, String> {
    // 返回 ACP 连接状态
}
```

#### 全局热键

```rust
// 在 setup() 中添加
app.handle().plugin(
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, shortcut, event| {
            if event == ShortcutEvent::Pressed {
                let _ = app.emit("toggle-chat", ());
            }
        })
        .build()
)?;
// 注册快捷键
// shortcut::register("Ctrl+Alt+K")?;
```

### 3. Cargo.toml 新增依赖

```toml
tauri-plugin-global-shortcut = "2"
```

无需其他新依赖，`tokio-tungstenite`、`tokio`、`serde_json` 等已存在。

### 4. 前端：`ChatPage.vue`

新增聊天页面，置于 `/chat` 路由。

```vue
<template>
  <div class="chat">
    <div class="messages" ref="messagesRef">
      <ChatMessage
        v-for="msg in chatStore.messages"
        :key="msg.id"
        :role="msg.role"
        :content="msg.content"
      />
      <!-- 正在输入的指示器 -->
      <div v-if="chatStore.responding" class="typing">Kaya 正在输入…</div>
    </div>
    <ChatInput
      :disabled="!chatStore.connected"
      @send="chatStore.sendMessage"
    />
    <p v-if="chatStore.error" class="error">{{ chatStore.error }}</p>
  </div>
</template>
```

### 5. 前端：`stores/chat.ts`

```typescript
interface ChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  timestamp: number;
}

export const useChatStore = defineStore('chat', () => {
  const messages = ref<ChatMessage[]>([]);
  const connected = ref(false);
  const responding = ref(false);
  const error = ref<string | null>(null);

  async function sendMessage(text: string) {
    // 1. 追加用户消息到列表
    // 2. 调 Rust command: invoke('send_acp_message', { text })
    // 3. 设置 responding = true
    // 4. 接收流式响应，逐块追加到 assistant 消息
  }

  function appendChunk(chunk: string, done: boolean) {
    // 打字机效果：追加到最后一条 assistant 消息
  }

  return { messages, connected, responding, error, sendMessage, appendChunk };
}
```

### 6. 前端：组件

**`ChatMessage.vue`** — 单条消息气泡
- `role` 决定对齐方向（用户右、Kaya 左）
- `content` 支持 Markdown 渲染
- 展示时间戳

**`ChatInput.vue`** — 输入框
- 文本输入框 + 发送按钮
- Enter 发送，Shift+Enter 换行
- 发送中禁用

### 7. 系统托盘改动

`tray.rs` 新增菜单项：

```rust
let chat_item = MenuItemBuilder::with_id("chat", "打开对话").build(app)?;

// 在 MenuBuilder 中插入
MenuBuilder::new(app)
    .item(&show_item)
    .item(&chat_item)     // 新增
    .separator()
    .item(&recent_item)
    .separator()
    .item(&quit_item)
    .build()?;

// 事件处理
"chat" => {
    let _ = app.emit("toggle-chat", ());
}
```

### 8. 路由改动

`main.ts` 新增 `/chat` 路由：

```typescript
const router = createRouter({
  routes: [
    { path: "/", redirect: "/config" },
    { path: "/config", component: ConfigPage },
    { path: "/status", component: StatusPage },
    { path: "/chat", component: ChatPage },    // 新增
  ],
});
```

### 9. App.vue 热键监听

```typescript
// App.vue onMounted
import { listen } from "@tauri-apps/api/event";

onMounted(async () => {
  await appStore.load();
  
  // 监听热键切换聊天
  await listen("toggle-chat", () => {
    if (router.currentRoute.value.path === "/chat") {
      // 如果已经在聊天页，不重复导航
    } else {
      router.push("/chat");
    }
  });
  
  // 路由逻辑
  if (appStore.config) {
    router.push("/status");
  } else {
    router.push("/config");
  }
});
```

### 10. Tauri 安全能力

`capabilities/default.json` 新增权限：

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

## 未定义/待确认

1. **ACP 协议消息格式** — 需在实现时对照 Agmente（`rebornix/Agmente`）或 ACP UI（`formulahendry/acp-ui`）源码确认具体 JSON 字段
2. **acp_client.rs 的消息通道** — `msg_rx` 如何从 Tauri 命令接收消息的具体实现方式（mpsc 存 `AppState` 或全局 `OnceLock`）
3. **热键默认值** — 当前设计为 `Ctrl+Alt+K`，可在实现阶段确认是否与其他软件冲突

## 实施顺序

1. `Cargo.toml` + `capabilities/` — 添加依赖和权限
2. `acp_client.rs` — ACP WebSocket 客户端（先确认协议格式）
3. `lib.rs` — AppState 扩展、命令注册、热键
4. `tray.rs` — 新增菜单项
5. `stores/chat.ts` — 聊天状态管理
6. `ChatMessage.vue` + `ChatInput.vue` — 基础组件
7. `ChatPage.vue` — 聊天页面
8. `main.ts` — 路由注册
9. `App.vue` — 热键监听
10. 端到端测试
