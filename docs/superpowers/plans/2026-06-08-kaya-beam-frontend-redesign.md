# kaya-beam 前端重设计实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 对 kaya-beam Tauri 客户端进行前端 UI 重设计——新增侧栏导航布局、重构所有页面、修复聊天框高度问题、应用现代浅色主题。

**架构：** 新增 AppLayout.vue（侧栏+内容区骨架）包裹 router-view，所有页面改为内容区局部组件，CSS 变量统一主题，无第三方 UI 库依赖。

**技术栈：** Vue 3 + Pinia + Vue Router + CSS Variables + Tauri 2

---

## 文件结构

### 新建文件

| 文件 | 职责 |
|------|------|
| `client/src/components/AppLayout.vue` | 应用骨架：flex 容器，侧栏固定 200px + 内容区 flex:1 |
| `client/src/components/Sidebar.vue` | 侧栏导航：4 个菜单项，router-link 高亮，Logo + 版本 |
| `client/src/views/HomePage.vue` | 首页：连接状态卡片 + 存储路径卡片 + 最近传输列表 |
| `client/src/views/FileTransferPage.vue` | 文件传输页：气泡样式收发历史 + 底部发送框占位 |
| `client/src/views/SettingsPage.vue` | 设置页：分块表单（连接配置 + 存储路径） |

### 修改文件

| 文件 | 职责 |
|------|------|
| `client/src/style.css` | 替换为 CSS 变量设计系统 |
| `client/src/main.ts` | 更新路由表 |
| `client/src/App.vue` | 包裹 AppLayout，移除旧路由逻辑 |
| `client/src/components/ChatMessage.vue` | 气泡样式刷新 |
| `client/src/components/ChatInput.vue` | 输入框样式刷新 |
| `client/src/views/ChatPage.vue` | 消息区固定高度 + 重构布局 |
| `client/src/stores/app.ts` | 新增 acp_url, storage_path 字段 |
| `client/src/stores/file.ts` | 新增文件历史列表 |
| `client/src/lib/types.ts` | AppConfig 类型更新 |
| `client/src-tauri/src/config.rs` | Rust AppConfig 新增字段 |
| `client/src-tauri/src/lib.rs` | ACP URL 优先使用 acp_url 配置 |

### 删除文件

| 文件 | 替代 |
|------|------|
| `client/src/views/ConfigPage.vue` | → SettingsPage.vue |
| `client/src/views/StatusPage.vue` | → HomePage.vue |

---

### 任务 1：Rust AppConfig 结构体扩展

**文件：**
- 修改：`client/src-tauri/src/config.rs`

- [ ] **步骤 1：修改 AppConfig 结构体**

在 `config.rs` 的 `AppConfig` 结构体中新增 `acp_url` 和 `storage_path` 字段：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server_url: String,
    pub client_id: String,
    pub passkey: String,
    #[serde(default)]
    pub acp_url: Option<String>,
    #[serde(default)]
    pub storage_path: Option<String>,
}
```

用 `#[serde(default)]` 保证旧配置向后兼容。

- [ ] **步骤 2：扩展 is_valid() 方法**

```rust
impl AppConfig {
    pub fn is_valid(&self) -> bool {
        !self.server_url.is_empty() && !self.client_id.is_empty() && !self.passkey.is_empty()
    }
}
```

`is_valid()` 不变——acp_url 和 storage_path 是可选的。

- [ ] **步骤 3：Build & verify**

运行：`cargo build`（在 client/src-tauri 目录）
预期：编译通过

- [ ] **步骤 4：Commit**

```bash
git add client/src-tauri/src/config.rs
git commit -m "feat: add acp_url and storage_path to AppConfig"
```

---

### 任务 2：Rust lib.rs 适配新字段

**文件：**
- 修改：`client/src-tauri/src/lib.rs`

- [ ] **步骤 1：修改 acp_url_from_config 函数**

改为优先使用 config 中保存的 acp_url：

```rust
/// 从配置获取 ACP 桥接地址，优先使用独立配置
fn acp_url_from_config(config: &AppConfig) -> String {
    if let Some(ref acp_url) = config.acp_url {
        if !acp_url.is_empty() {
            return acp_url.clone();
        }
    }
    // 回退：从 server_url 推导
    if let Some(rest) = config.server_url.strip_prefix("ws://") {
        if let Some(host) = rest.split(':').next() {
            return format!("ws://{}:8765", host);
        }
    }
    "ws://127.0.0.1:8765".to_string()
}
```

- [ ] **步骤 2：更新 start_acp_client 调用**

```rust
fn start_acp_client(app: &AppHandle, config: &AppConfig) {
    // ... 前面的 setup ...
    let acp_url = acp_url_from_config(config);

    tauri::async_runtime::spawn(async move {
        acp_client::run_acp_client(acp_url, event_tx, msg_rx).await;
    });
    // ... 后面的 event 处理 ...
}
```

- [ ] **步骤 3：Build & verify**

运行：`cargo build`
预期：编译通过

- [ ] **步骤 4：Commit**

```bash
git add client/src-tauri/src/lib.rs
git commit -m "feat: respect acp_url config in ACP client startup"
```

---

### 任务 3：前端 TypeScript 类型更新

**文件：**
- 修改：`client/src/lib/types.ts`

- [ ] **步骤 1：更新 AppConfig 类型**

```typescript
export interface AppConfig {
  server_url: string;
  client_id: string;
  passkey: string;
  acp_url?: string | null;
  storage_path?: string | null;
}

export interface TransferRecord {
  id: string;
  name: string;
  size: number;
  direction: "received" | "sent";
  timestamp: number;
  status: "ok" | "error";
}
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/lib/types.ts
git commit -m "feat: update AppConfig and add TransferRecord types"
```

---

### 任务 4：CSS 设计系统

**文件：**
- 重写：`client/src/style.css`

- [ ] **步骤 1：替换为 CSS 变量设计系统**

```css
:root {
  --color-primary: #6366F1;
  --color-primary-hover: #5558E6;
  --color-bg: #F5F5FA;
  --color-surface: #FFFFFF;
  --color-border: #E8E8EE;
  --color-border-light: #F0F0F5;
  --color-text: #1a1a2e;
  --color-text-secondary: #555;
  --color-text-muted: #999;
  --color-text-light: #BBB;
  --color-success: #22C55E;
  --color-error: #EF4444;
  --color-sidebar-hover: #F0F0F5;

  --radius-sm: 8px;
  --radius-md: 10px;
  --radius-lg: 12px;

  --shadow-card: 0 1px 3px rgba(0,0,0,0.04);
  --shadow-bubble: 0 1px 2px rgba(0,0,0,0.06);

  --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --font-mono: "SF Mono", "Fira Code", "Fira Mono", "Roboto Mono", monospace;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

html, body, #app {
  height: 100%;
  width: 100%;
  overflow: hidden;
}

body {
  font-family: var(--font-sans);
  font-size: 14px;
  color: var(--color-text);
  background: var(--color-bg);
  -webkit-font-smoothing: antialiased;
}

input {
  font-family: var(--font-sans);
  outline: none;
}

input:focus {
  border-color: var(--color-primary) !important;
}

button {
  font-family: var(--font-sans);
  cursor: pointer;
}

::-webkit-scrollbar {
  width: 6px;
}

::-webkit-scrollbar-track {
  background: transparent;
}

::-webkit-scrollbar-thumb {
  background: #D0D0DD;
  border-radius: 3px;
}

::-webkit-scrollbar-thumb:hover {
  background: #B0B0C0;
}
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/style.css
git commit -m "feat: add CSS variable design system"
```

---

### 任务 5：AppLayout + Sidebar 布局骨架

**文件：**
- 创建：`client/src/components/AppLayout.vue`
- 创建：`client/src/components/Sidebar.vue`

- [ ] **步骤 1：创建 Sidebar.vue**

```vue
<script setup lang="ts">
import { useRoute } from "vue-router";

const route = useRoute();

const navItems = [
  { path: "/home", label: "Home", icon: "🏠" },
  { path: "/files", label: "文件传输", icon: "📁" },
  { path: "/chat", label: "聊天", icon: "💬" },
  { path: "/settings", label: "设置", icon: "⚙️" },
];

function isActive(path: string): boolean {
  return route.path.startsWith(path);
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-header">
      <div class="sidebar-logo">kaya-beam</div>
      <div class="sidebar-subtitle">File Transfer Hub</div>
    </div>

    <nav class="sidebar-nav">
      <router-link
        v-for="item in navItems"
        :key="item.path"
        :to="item.path"
        class="nav-item"
        :class="{ active: isActive(item.path) }"
      >
        <span class="nav-icon">{{ item.icon }}</span>
        <span class="nav-label">{{ item.label }}</span>
      </router-link>
    </nav>

    <div class="sidebar-footer">v0.1.0</div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 200px;
  min-width: 200px;
  height: 100%;
  background: var(--color-surface);
  border-right: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  padding: 16px 0;
  user-select: none;
}

.sidebar-header {
  padding: 0 20px 16px;
  border-bottom: 1px solid var(--color-border-light);
  margin-bottom: 8px;
}

.sidebar-logo {
  font-weight: 600;
  font-size: 14px;
  color: var(--color-text);
}

.sidebar-subtitle {
  font-size: 11px;
  color: var(--color-text-muted);
  margin-top: 1px;
}

.sidebar-nav {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 0 8px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--color-text-secondary);
  text-decoration: none;
  transition: background 0.15s, color 0.15s;
}

.nav-item:hover {
  background: var(--color-sidebar-hover);
  color: var(--color-text);
}

.nav-item.active,
.nav-item.router-link-active {
  background: var(--color-primary);
  color: #fff;
  font-weight: 500;
}

.nav-icon {
  font-size: 16px;
  width: 20px;
  text-align: center;
}

.nav-label {
  line-height: 1;
}

.sidebar-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--color-border-light);
  font-size: 11px;
  color: var(--color-text-light);
}
</style>
```

- [ ] **步骤 2：创建 AppLayout.vue**

```vue
<script setup lang="ts">
import Sidebar from "./Sidebar.vue";
</script>

<template>
  <div class="app-layout">
    <Sidebar />
    <main class="content-area">
      <router-view />
    </main>
  </div>
</template>

<style scoped>
.app-layout {
  display: flex;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
}

.content-area {
  flex: 1;
  overflow-y: auto;
  background: var(--color-bg);
  min-width: 0;
}
</style>
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/components/AppLayout.vue client/src/components/Sidebar.vue
git commit -m "feat: add AppLayout skeleton with Sidebar navigation"
```

---

### 任务 6：路由配置更新

**文件：**
- 修改：`client/src/main.ts`

- [ ] **步骤 1：更新路由表**

```typescript
import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHistory } from "vue-router";
import App from "./App.vue";
import HomePage from "./views/HomePage.vue";
import FileTransferPage from "./views/FileTransferPage.vue";
import ChatPage from "./views/ChatPage.vue";
import SettingsPage from "./views/SettingsPage.vue";
import "./style.css";

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/home" },
    { path: "/home", component: HomePage },
    { path: "/files", component: FileTransferPage },
    { path: "/chat", component: ChatPage },
    { path: "/settings", component: SettingsPage },
  ],
});

const app = createApp(App);
const pinia = createPinia();

app.use(pinia);
app.use(router);
app.mount("#app");
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/main.ts
git commit -m "feat: update routes for new page structure"
```

---

### 任务 7：App.vue 接入 AppLayout

**文件：**
- 修改：`client/src/App.vue`

- [ ] **步骤 1：重写 App.vue**

```vue
<script setup lang="ts">
import { onMounted } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "./stores/app";
import { listen } from "@tauri-apps/api/event";
import AppLayout from "./components/AppLayout.vue";

const router = useRouter();
const appStore = useAppStore();

onMounted(async () => {
  await listen("toggle-chat", () => {
    if (router.currentRoute.value.path !== "/chat") {
      router.push("/chat");
    }
  });

  await appStore.load();
  if (appStore.config) {
    router.push("/home");
  } else {
    router.push("/settings");
  }
});
</script>

<template>
  <AppLayout>
    <template #default>
      <router-view />
    </template>
  </AppLayout>
</template>
```

注意：AppLayout 里 `<router-view />` 直接放在 `<main>` 里，所以不需要 slot。

简化版本：

```vue
<script setup lang="ts">
import { onMounted } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "./stores/app";
import { listen } from "@tauri-apps/api/event";
import AppLayout from "./components/AppLayout.vue";

const router = useRouter();
const appStore = useAppStore();

onMounted(async () => {
  await listen("toggle-chat", () => {
    if (router.currentRoute.value.path !== "/chat") {
      router.push("/chat");
    }
  });

  await appStore.load();
  if (appStore.config) {
    router.push("/home");
  } else {
    router.push("/settings");
  }
});
</script>

<template>
  <AppLayout />
</template>
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/App.vue
git commit -m "feat: integrate AppLayout into App.vue"
```

---

### 任务 8：HomePage（替换 StatusPage）

**文件：**
- 创建：`client/src/views/HomePage.vue`
- 删除：`client/src/views/StatusPage.vue`

- [ ] **步骤 1：创建 HomePage.vue**

```vue
<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { useAppStore } from "../stores/app";
import { useFileStore } from "../stores/file";
import { listen } from "@tauri-apps/api/event";

const appStore = useAppStore();
const fileStore = useFileStore();

onMounted(() => {
  if (!appStore.config) {
    appStore.load();
  }
});
</script>

<template>
  <div class="home">
    <h2 class="page-title">Home</h2>
    <p class="page-subtitle">服务连接与传输概览</p>

    <div class="status-cards">
      <div class="card">
        <div class="card-label">连接状态</div>
        <div class="card-value">
          <span
            class="status-dot"
            :class="{ connected: appStore.connected }"
          ></span>
          {{ appStore.connected ? "已连接" : "已断开" }}
        </div>
        <div v-if="appStore.error" class="card-error">{{ appStore.error }}</div>
      </div>
      <div class="card">
        <div class="card-label">存储路径</div>
        <div class="card-value mono">{{ appStore.config?.storage_path || "~/kaya-transfer/" }}</div>
      </div>
    </div>

    <!-- Recent transfers -->
    <div class="section-card">
      <div class="section-header">最近传输</div>
      <div v-if="fileStore.history.length === 0" class="empty-state">
        暂无传输记录
      </div>
      <div
        v-for="record in fileStore.history.slice(-10).reverse()"
        :key="record.id"
        class="transfer-row"
      >
        <span class="transfer-name">{{ record.name }}</span>
        <span class="transfer-size">{{ (record.size / 1024 / 1024).toFixed(1) }} MB</span>
        <span class="transfer-time">{{ new Date(record.timestamp).toLocaleTimeString() }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.home {
  padding: 24px 32px;
  max-width: 800px;
}

.page-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
  margin-bottom: 4px;
}

.page-subtitle {
  font-size: 13px;
  color: var(--color-text-muted);
  margin-bottom: 24px;
}

.status-cards {
  display: flex;
  gap: 16px;
  margin-bottom: 24px;
}

.card {
  flex: 1;
  background: var(--color-surface);
  border-radius: var(--radius-lg);
  padding: 16px 20px;
  box-shadow: var(--shadow-card);
}

.card-label {
  font-size: 12px;
  color: var(--color-text-muted);
  margin-bottom: 6px;
}

.card-value {
  font-size: 14px;
  font-weight: 500;
  color: var(--color-text);
  display: flex;
  align-items: center;
  gap: 6px;
}

.card-value.mono {
  font-family: var(--font-mono);
  font-size: 13px;
}

.card-error {
  font-size: 12px;
  color: var(--color-error);
  margin-top: 4px;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-text-light);
  display: inline-block;
}

.status-dot.connected {
  background: var(--color-success);
}

.section-card {
  background: var(--color-surface);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  overflow: hidden;
}

.section-header {
  padding: 16px 20px;
  border-bottom: 1px solid var(--color-border-light);
  font-size: 13px;
  font-weight: 500;
  color: var(--color-text);
}

.empty-state {
  padding: 32px 20px;
  text-align: center;
  color: var(--color-text-light);
  font-size: 13px;
}

.transfer-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  border-bottom: 1px solid #F8F8FB;
  font-size: 13px;
}

.transfer-row:last-child {
  border-bottom: none;
}

.transfer-name {
  color: var(--color-text-secondary);
  flex: 1;
}

.transfer-size {
  color: var(--color-text-muted);
  width: 80px;
  text-align: right;
}

.transfer-time {
  color: var(--color-text-light);
  width: 70px;
  text-align: right;
}
</style>
```

- [ ] **步骤 2：删除 StatusPage.vue**

```bash
git rm client/src/views/StatusPage.vue
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/views/HomePage.vue
git commit -m "feat: add HomePage with connection status and transfer history"
```

---

### 任务 9：FileTransferPage（气泡样式文件记录）

**文件：**
- 创建：`client/src/views/FileTransferPage.vue`

- [ ] **步骤 1：创建 FileTransferPage.vue**

```vue
<script setup lang="ts">
import { useFileStore } from "../stores/file";

const fileStore = useFileStore();

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  if (isToday) {
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
  return d.toLocaleDateString([], { month: "short", day: "numeric" }) + " " +
    d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}
</script>

<template>
  <div class="file-transfer">
    <div class="page-header">
      <h2 class="page-title">📁 文件传输</h2>
      <p class="page-subtitle">文件收发记录</p>
    </div>

    <div class="message-area">
      <div v-if="fileStore.history.length === 0" class="empty-state">
        <div class="empty-icon">📂</div>
        <p>暂无文件传输记录</p>
        <p class="empty-hint">Kaya 发送的文件将显示在这里</p>
      </div>

      <div
        v-for="record in fileStore.history"
        :key="record.id"
        class="bubble-row"
        :class="{ sent: record.direction === 'sent' }"
      >
        <div class="avatar" :class="{ sent: record.direction === 'sent' }">
          {{ record.direction === "sent" ? "我" : "K" }}
        </div>
        <div class="bubble" :class="{ sent: record.direction === 'sent' }">
          <div class="file-info">
            <span class="file-icon">{{ record.name.match(/\.(png|jpg|jpeg|gif|webp|bmp)$/i) ? "🖼️" : "📄" }}</span>
            <div class="file-detail">
              <div class="file-name">{{ record.name }}</div>
              <div class="file-meta">
                {{ formatSize(record.size) }}
                <span v-if="record.status === 'ok'" class="status-ok">✓ 已接收</span>
                <span v-else class="status-error">✗ 失败</span>
              </div>
            </div>
          </div>
          <div class="bubble-time">{{ formatTime(record.timestamp) }}</div>
        </div>
      </div>
    </div>

    <!-- Send placeholder -->
    <div class="send-area">
      <div class="send-box">
        <div class="send-input-placeholder">📎 选择文件或拖拽到此处...</div>
        <button class="send-btn" disabled>发送</button>
      </div>
      <div class="send-hint">发送文件功能即将上线</div>
    </div>
  </div>
</template>

<style scoped>
.file-transfer {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--color-bg);
}

.page-header {
  padding: 24px 32px 16px;
  border-bottom: 1px solid var(--color-border);
  background: var(--color-surface);
}

.page-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
}

.page-subtitle {
  font-size: 13px;
  color: var(--color-text-muted);
  margin-top: 2px;
}

.message-area {
  flex: 1;
  overflow-y: auto;
  padding: 16px 32px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.empty-state {
  text-align: center;
  padding: 48px 20px;
  color: var(--color-text-muted);
}

.empty-icon {
  font-size: 48px;
  margin-bottom: 12px;
}

.empty-hint {
  font-size: 12px;
  color: var(--color-text-light);
  margin-top: 4px;
}

.bubble-row {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.bubble-row.sent {
  flex-direction: row-reverse;
}

.avatar {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-weight: 500;
  flex-shrink: 0;
  background: var(--color-primary);
  color: #fff;
}

.avatar.sent {
  background: var(--color-success);
}

.bubble {
  background: var(--color-surface);
  border-radius: 12px 12px 12px 4px;
  padding: 10px 14px;
  max-width: 65%;
  box-shadow: var(--shadow-bubble);
}

.bubble.sent {
  background: var(--color-primary);
  color: #fff;
  border-radius: 12px 12px 4px 12px;
}

.file-info {
  display: flex;
  align-items: center;
  gap: 10px;
}

.file-icon {
  font-size: 24px;
}

.file-name {
  font-size: 13px;
  font-weight: 500;
}

.bubble.sent .file-name {
  color: #fff;
}

.file-meta {
  font-size: 11px;
  color: var(--color-text-muted);
  margin-top: 2px;
  display: flex;
  align-items: center;
  gap: 6px;
}

.bubble.sent .file-meta {
  color: rgba(255,255,255,0.7);
}

.status-ok {
  color: var(--color-success);
}

.bubble.sent .status-ok {
  color: rgba(255,255,255,0.8);
}

.status-error {
  color: var(--color-error);
}

.bubble-time {
  font-size: 10px;
  color: var(--color-text-light);
  margin-top: 6px;
  text-align: right;
}

.bubble.sent .bubble-time {
  color: rgba(255,255,255,0.5);
}

.send-area {
  padding: 12px 32px 16px;
  border-top: 1px solid var(--color-border);
  background: var(--color-surface);
}

.send-box {
  display: flex;
  gap: 8px;
  align-items: center;
}

.send-input-placeholder {
  flex: 1;
  padding: 10px 14px;
  border: 1.5px dashed var(--color-border);
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--color-text-light);
  background: var(--color-bg);
}

.send-btn {
  background: var(--color-primary);
  color: #fff;
  border: none;
  border-radius: var(--radius-md);
  padding: 10px 20px;
  font-size: 13px;
  font-weight: 500;
  opacity: 0.5;
  cursor: default;
}

.send-hint {
  font-size: 10px;
  color: var(--color-text-light);
  margin-top: 6px;
  text-align: right;
}
</style>
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/views/FileTransferPage.vue
git commit -m "feat: add FileTransferPage with chat-bubble style file history"
```

---

### 任务 10：SettingsPage（替换 ConfigPage）

**文件：**
- 创建：`client/src/views/SettingsPage.vue`
- 删除：`client/src/views/ConfigPage.vue`

- [ ] **步骤 1：创建 SettingsPage.vue**

```vue
<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "../stores/app";
import type { AppConfig } from "../lib/types";

const router = useRouter();
const appStore = useAppStore();

const form = ref<AppConfig>({
  server_url: appStore.config?.server_url || "",
  client_id: appStore.config?.client_id || "",
  passkey: appStore.config?.passkey || "",
  acp_url: appStore.config?.acp_url || "",
  storage_path: appStore.config?.storage_path || "~/kaya-transfer/",
});

const saving = ref(false);
const saved = ref(false);

async function handleSave() {
  saving.value = true;
  saved.value = false;
  try {
    // 如果 acp_url 没填，自动推导
    if (!form.value.acp_url && form.value.server_url) {
      const host = form.value.server_url.replace("ws://", "").split(":")[0];
      form.value.acp_url = `ws://${host}:8765`;
    }
    await appStore.save(form.value);
    saved.value = true;
    setTimeout(() => { saved.value = false; }, 3000);
  } catch (e) {
    console.error("Save failed", e);
  } finally {
    saving.value = false;
  }
}

function handleReset() {
  form.value = {
    server_url: "",
    client_id: "",
    passkey: "",
    acp_url: "",
    storage_path: "~/kaya-transfer/",
  };
}
</script>

<template>
  <div class="settings">
    <h2 class="page-title">设置</h2>
    <p class="page-subtitle">配置服务端连接与存储选项</p>

    <div class="form-section">
      <div class="section-title">🔗 服务端连接</div>
      <div class="form-group">
        <label class="form-label">WebSocket 地址</label>
        <input v-model="form.server_url" class="form-input" placeholder="ws://192.168.1.100:9765" />
      </div>
      <div class="form-group">
        <label class="form-label">ACP 桥接地址</label>
        <input v-model="form.acp_url" class="form-input" placeholder="留空自动推导" />
      </div>
      <div class="form-row">
        <div class="form-group">
          <label class="form-label">客户端 ID</label>
          <input v-model="form.client_id" class="form-input" placeholder="pc-01" />
        </div>
        <div class="form-group">
          <label class="form-label">Passkey</label>
          <input v-model="form.passkey" type="password" class="form-input" placeholder="输入密钥" />
        </div>
      </div>
    </div>

    <div class="form-section">
      <div class="section-title">📂 存储设置</div>
      <div class="form-group">
        <label class="form-label">默认存储路径</label>
        <div class="input-with-button">
          <input v-model="form.storage_path" class="form-input" placeholder="~/kaya-transfer/" />
          <button class="btn-secondary" disabled>浏览</button>
        </div>
        <div class="form-hint">文件将保存到此目录下的 YYYY-MM/ 子文件夹</div>
      </div>
    </div>

    <div class="form-actions">
      <button class="btn-secondary" @click="handleReset">重置</button>
      <button class="btn-primary" :disabled="saving" @click="handleSave">
        {{ saving ? "保存中..." : "保存配置" }}
      </button>
      <span v-if="saved" class="save-success">✓ 已保存</span>
    </div>
  </div>
</template>

<style scoped>
.settings {
  padding: 24px 32px;
  max-width: 640px;
}

.page-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
  margin-bottom: 4px;
}

.page-subtitle {
  font-size: 13px;
  color: var(--color-text-muted);
  margin-bottom: 24px;
}

.form-section {
  background: var(--color-surface);
  border-radius: var(--radius-lg);
  padding: 20px 24px;
  margin-bottom: 16px;
  box-shadow: var(--shadow-card);
}

.section-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--color-text);
  margin-bottom: 16px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--color-border-light);
}

.form-group {
  margin-bottom: 14px;
}

.form-group:last-child {
  margin-bottom: 0;
}

.form-label {
  font-size: 12px;
  font-weight: 500;
  color: var(--color-text-secondary);
  display: block;
  margin-bottom: 4px;
}

.form-input {
  width: 100%;
  padding: 10px 12px;
  border: 1.5px solid var(--color-border);
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--color-text);
  background: var(--color-bg);
  box-sizing: border-box;
  transition: border-color 0.15s;
}

.form-input:focus {
  border-color: var(--color-primary);
}

.form-row {
  display: flex;
  gap: 12px;
}

.form-row .form-group {
  flex: 1;
}

.input-with-button {
  display: flex;
  gap: 8px;
}

.input-with-button .form-input {
  flex: 1;
}

.form-hint {
  font-size: 11px;
  color: var(--color-text-light);
  margin-top: 4px;
}

.form-actions {
  display: flex;
  gap: 8px;
  align-items: center;
  justify-content: flex-end;
}

.btn-primary {
  background: var(--color-primary);
  color: #fff;
  border: none;
  border-radius: var(--radius-md);
  padding: 10px 24px;
  font-size: 13px;
  font-weight: 500;
  transition: background 0.15s;
}

.btn-primary:hover:not(:disabled) {
  background: var(--color-primary-hover);
}

.btn-primary:disabled {
  opacity: 0.5;
  cursor: default;
}

.btn-secondary {
  background: var(--color-surface);
  border: 1.5px solid var(--color-border);
  border-radius: var(--radius-md);
  padding: 10px 24px;
  font-size: 13px;
  color: var(--color-text-secondary);
  transition: background 0.15s;
}

.btn-secondary:hover {
  background: var(--color-sidebar-hover);
}

.save-success {
  font-size: 13px;
  color: var(--color-success);
  font-weight: 500;
}
</style>
```

- [ ] **步骤 2：删除 ConfigPage.vue**

```bash
git rm client/src/views/ConfigPage.vue
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/views/SettingsPage.vue
git commit -m "feat: add SettingsPage with server config and storage path"
```

---

### 任务 11：ChatPage 固定高度 + 布局重构

**文件：**
- 重写：`client/src/views/ChatPage.vue`

- [ ] **步骤 1：重写 ChatPage.vue**

```vue
<script setup lang="ts">
import { ref, nextTick, watch } from "vue";
import { useChatStore } from "../stores/chat";
import ChatMessage from "../components/ChatMessage.vue";
import ChatInput from "../components/ChatInput.vue";

const chatStore = useChatStore();
const messagesRef = ref<HTMLElement | null>(null);

watch(
  () => chatStore.messages.length,
  async () => {
    await nextTick();
    if (messagesRef.value) {
      messagesRef.value.scrollTop = messagesRef.value.scrollHeight;
    }
  }
);
</script>

<template>
  <div class="chat-page">
    <!-- Header -->
    <div class="chat-header">
      <span class="status-indicator" :class="{ connected: chatStore.connected }"></span>
      <span class="chat-partner">Kaya</span>
      <span class="chat-status">{{ chatStore.connected ? "在线" : "已断开" }}</span>
    </div>

    <!-- Messages area: fixed height via flex, scrollable -->
    <div ref="messagesRef" class="messages-area">
      <ChatMessage
        v-for="msg in chatStore.messages"
        :key="msg.id"
        :role="msg.role"
        :content="msg.content"
      />
      <div v-if="chatStore.responding" class="typing-indicator">
        <span class="typing-dot"></span>
        <span class="typing-dot"></span>
        <span class="typing-dot"></span>
      </div>
    </div>

    <!-- Input -->
    <div class="chat-input-area">
      <ChatInput
        :disabled="!chatStore.connected"
        @send="chatStore.sendMessage"
      />
    </div>
  </div>
</template>

<style scoped>
.chat-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--color-bg);
}

.chat-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px 24px;
  border-bottom: 1px solid var(--color-border);
  background: var(--color-surface);
  flex-shrink: 0;
}

.status-indicator {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-text-light);
}

.status-indicator.connected {
  background: var(--color-success);
}

.chat-partner {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text);
}

.chat-status {
  font-size: 11px;
  color: var(--color-text-muted);
}

.messages-area {
  flex: 1;
  overflow-y: auto;
  padding: 16px 24px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0; /* crucial for flex overflow */
}

.typing-indicator {
  display: flex;
  gap: 4px;
  padding: 12px 16px;
  background: var(--color-surface);
  border-radius: 12px 12px 12px 4px;
  align-self: flex-start;
  box-shadow: var(--shadow-bubble);
}

.typing-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-text-light);
  animation: typing 1.4s infinite;
}

.typing-dot:nth-child(2) {
  animation-delay: 0.2s;
}

.typing-dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes typing {
  0%, 60%, 100% { opacity: 0.3; }
  30% { opacity: 1; }
}

.chat-input-area {
  flex-shrink: 0;
  padding: 12px 24px 16px;
  border-top: 1px solid var(--color-border);
  background: var(--color-surface);
}
</style>
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/views/ChatPage.vue
git commit -m "feat: fix ChatPage height - scrollable messages, fixed input"
```

---

### 任务 12：ChatMessage 和 ChatInput 样式刷新

**文件：**
- 修改：`client/src/components/ChatMessage.vue`
- 修改：`client/src/components/ChatInput.vue`

- [ ] **步骤 1：ChatMessage.vue 样式刷新**

```vue
<script setup lang="ts">
defineProps<{
  role: "user" | "assistant";
  content: string;
}>();
</script>

<template>
  <div class="message-row" :class="{ user: role === 'user' }">
    <div class="msg-avatar" :class="{ user: role === 'user' }">
      {{ role === "user" ? "我" : "K" }}
    </div>
    <div class="msg-bubble" :class="{ user: role === 'user' }">
      <div class="msg-content">{{ content }}</div>
    </div>
  </div>
</template>

<style scoped>
.message-row {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.message-row.user {
  flex-direction: row-reverse;
}

.msg-avatar {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-weight: 500;
  flex-shrink: 0;
  background: var(--color-primary);
  color: #fff;
}

.msg-avatar.user {
  background: var(--color-text);
}

.msg-bubble {
  background: var(--color-surface);
  border-radius: 12px 12px 12px 4px;
  padding: 10px 14px;
  max-width: 75%;
  box-shadow: var(--shadow-bubble);
}

.msg-bubble.user {
  background: var(--color-primary);
  color: #fff;
  border-radius: 12px 12px 4px 12px;
}

.msg-content {
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
```

- [ ] **步骤 2：ChatInput.vue 样式刷新**

```vue
<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{
  disabled?: boolean;
}>();

const emit = defineEmits<{
  send: [text: string];
}>();

const text = ref("");

function handleSend() {
  const msg = text.value.trim();
  if (!msg || props.disabled) return;
  emit("send", msg);
  text.value = "";
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    handleSend();
  }
}
</script>

<template>
  <div class="chat-input">
    <input
      v-model="text"
      class="input-field"
      :disabled="disabled"
      placeholder="给 Kaya 发消息..."
      @keydown="onKeydown"
    />
    <button
      class="send-button"
      :disabled="disabled || !text.trim()"
      @click="handleSend"
    >
      发送
    </button>
  </div>
</template>

<style scoped>
.chat-input {
  display: flex;
  gap: 8px;
  align-items: center;
}

.input-field {
  flex: 1;
  padding: 10px 14px;
  border: 1.5px solid var(--color-border);
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--color-text);
  background: var(--color-bg);
  transition: border-color 0.15s;
}

.input-field:focus {
  border-color: var(--color-primary);
}

.input-field:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.send-button {
  background: var(--color-primary);
  color: #fff;
  border: none;
  border-radius: var(--radius-md);
  padding: 10px 20px;
  font-size: 13px;
  font-weight: 500;
  transition: background 0.15s;
}

.send-button:hover:not(:disabled) {
  background: var(--color-primary-hover);
}

.send-button:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/components/ChatMessage.vue client/src/components/ChatInput.vue
git commit -m "style: refresh ChatMessage and ChatInput with design system"
```

---

### 任务 13：Store 适配（app.ts + file.ts）

**文件：**
- 修改：`client/src/stores/app.ts`
- 修改：`client/src/stores/file.ts`

- [ ] **步骤 1：app.ts 适配新字段**

主要改动：`config` 的类型已通过 `types.ts` 更新，`load()` 和 `save()` 不需要改，直接从 Rust 端获取/提交完整配置对象即可。但 `storage_path` 默认值需要处理：

```typescript
// 在 load() 方法后补一个默认值逻辑
async function load() {
  loading.value = true;
  try {
    const cfg = await loadCfg();
    if (cfg) {
      // 填充默认值
      if (!cfg.storage_path) cfg.storage_path = "~/kaya-transfer/";
    }
    config.value = cfg;
  } catch {
    config.value = null;
  } finally {
    loading.value = false;
  }
}
```

其他 keep——`setConnected`、`setError` 等保持不变。

- [ ] **步骤 2：file.ts 扩展为历史记录列表**

```typescript
import { defineStore } from "pinia";
import { ref } from "vue";
import type { TransferRecord } from "../lib/types";

export const useFileStore = defineStore("file", () => {
  const fileName = ref<string | null>(null);
  const fileSize = ref(0);
  const filePath = ref<string | null>(null);
  const visible = ref(false);
  const history = ref<TransferRecord[]>([]);

  function show(name: string, size: number, path: string) {
    fileName.value = name;
    fileSize.value = size;
    filePath.value = path;
    visible.value = true;

    // 追加到历史记录
    history.value.push({
      id: `f_${Date.now()}`,
      name,
      size,
      direction: "received",
      timestamp: Date.now(),
      status: "ok",
    });
  }

  function dismiss() {
    visible.value = false;
  }

  return { fileName, fileSize, filePath, visible, history, show, dismiss };
});
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/stores/app.ts client/src/stores/file.ts
git commit -m "feat: adapt stores for new config fields and file transfer history"
```

---

### 任务 14：验证编译

**文件：**
- 检查：全项目

- [ ] **步骤 1：检查 TypeScript 编译**

```bash
cd client && npx vue-tsc --noEmit
```

预期：编译通过，无类型错误

- [ ] **步骤 2：检查 Vite build**

```bash
cd client && npx vite build
```

预期：构建成功

- [ ] **步骤 3：检查 Rust 编译**

```bash
cd client/src-tauri && cargo check
```

预期：编译通过

- [ ] **步骤 4：最终 commit**

```bash
git add -A
git commit -m "chore: cleanup - remove old ConfigPage and StatusPage"
```

---

## 验证后的清理

- [ ] 检查 `ConfigPage.vue` 和 `StatusPage.vue` 已从文件系统中删除
- [ ] 检查 `client/src/main.ts` 不再引用已删除的页面
- [ ] 检查 `client/src/style.css` 已替换为新的设计系统
- [ ] 检查所有路由正确（`/` → `/home`, `/files`, `/chat`, `/settings`）
- [ ] 检查 `Ctrl+Alt+K` 热键仍能切换到聊天页
