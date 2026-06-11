# kaya-beam 客户端 Vue 3 迁移 实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development 或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 将 kaya-beam 的 Tauri 桌面客户端前端从 Svelte 5 迁移到 Vue 3，采用 ACP UI 的技术选型（Vue 3 + Pinia + Vue Router + 纯 CSS）。

**架构：** 原地替换 client/ 目录下的前端文件，Rust 后端不变。引入 Vue 3、Pinia（状态管理）、Vue Router（页面路由），用 lib/tauri.ts 封装 Tauri API 调用。

**技术栈：** Vue 3、Pinia、Vue Router 5、Vite 6、TypeScript、Tauri 2

---

## 文件变更清单

### 创建
- `client/tsconfig.json` — TypeScript 配置（含 Vue 编译器选项）
- `client/tsconfig.node.json` — Node 端 Vite 配置
- `client/src/lib/tauri.ts` — Tauri invoke/listen 封装层
- `client/src/stores/app.ts` — 应用全局状态（配置、连接状态）
- `client/src/stores/file.ts` — 文件接收状态
- `client/src/views/ConfigPage.vue` — 配置页
- `client/src/views/StatusPage.vue` — 状态页
- `client/src/components/StatusIndicator.vue` — 连接状态指示器
- `client/src/App.vue` — 根组件（router-view + 初始化逻辑）
- `client/src/main.ts` — 入口（createApp + Pinia + mount）
- `client/src/style.css` — 全局样式

### 修改
- `client/package.json` — 替换 Svelte 依赖为 Vue/Pinia/Router/vue-tsc
- `client/vite.config.ts` — 替换 vite 插件

### 删除
- `client/svelte.config.js`
- `client/src/App.svelte`
- `client/src/ConfigPage.svelte`
- `client/src/StatusPage.svelte`

### 不动
- `client/index.html`
- `client/src/main.ts`（路径不变，但内容重写）
- `client/src-tauri/`（全部 Rust 代码）

---

### 任务 1：替换项目依赖和构建配置

**文件：**
- 修改：`client/package.json`
- 修改：`client/vite.config.ts`
- 删除：`client/svelte.config.js`
- 创建：`client/tsconfig.json`
- 创建：`client/tsconfig.node.json`

- [ ] **步骤 1：更新 package.json**

将 Svelte 依赖替换为 Vue 技术栈，保持 `@tauri-apps/api` + `@tauri-apps/cli` 不变。

```json
{
  "name": "file-transfer-hub-client",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.0.0",
    "@vitejs/plugin-vue": "^5.2.1",
    "typescript": "~5.6.2",
    "vite": "^6.0.0",
    "vue-tsc": "^2.1.10"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.0.0",
    "pinia": "^3.0.4",
    "vue": "^3.5.13",
    "vue-router": "^5.0.1"
  }
}
```

- [ ] **步骤 2：重写 vite.config.ts**

```typescript
import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
});
```

- [ ] **步骤 3：删除 svelte.config.js**

运行：`rm client/svelte.config.js`

- [ ] **步骤 4：创建 tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "module": "ESNext",
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "preserve",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src/**/*.ts", "src/**/*.d.ts", "src/**/*.tsx", "src/**/*.vue"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

- [ ] **步骤 5：创建 tsconfig.node.json**

```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true
  },
  "include": ["vite.config.ts"]
}
```

- [ ] **步骤 6：创建目录结构**

运行：
```bash
mkdir -p client/src/assets client/src/components client/src/lib client/src/stores client/src/views
```

- [ ] **步骤 7：Commit**

```bash
git add client/package.json client/vite.config.ts client/tsconfig.json client/tsconfig.node.json
git rm client/svelte.config.js
git commit -m "chore: 替换构建配置为 Vue 3 技术栈"
```

---

### 任务 2：实现 Tauri 桥接层

**文件：**
- 创建：`client/src/lib/tauri.ts`

- [ ] **步骤 1：编写 lib/tauri.ts**

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface AppConfig {
  serverUrl: string;
  clientId: string;
  passkey: string;
}

/** 加载本地配置 */
export async function loadConfig(): Promise<AppConfig | null> {
  try {
    return await invoke<AppConfig | null>("load_config");
  } catch {
    return null;
  }
}

/** 保存配置 */
export async function saveConfig(config: AppConfig): Promise<void> {
  await invoke("save_config", { config });
}

/** 监听连接状态事件 */
export async function onConnectionStatus(
  cb: (status: string, lastHeartbeat?: string) => void,
): Promise<() => void> {
  return listen<{ status: string; lastHeartbeat?: string }>("connection-status", (e) => {
    cb(e.payload.status, e.payload.lastHeartbeat);
  });
}

/** 监听文件接收事件 */
export async function onFileReceived(
  cb: (name: string, size: number, path: string) => void,
): Promise<() => void> {
  return listen<{ name: string; size: number; path: string }>("file-received", (e) => {
    cb(e.payload.name, e.payload.size, e.payload.path);
  });
}
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/lib/tauri.ts
git commit -m "feat: 添加 Tauri API 桥接层"
```

---

### 任务 3：实现 Pinia Store

**文件：**
- 创建：`client/src/stores/app.ts`
- 创建：`client/src/stores/file.ts`

- [ ] **步骤 1：编写 stores/app.ts**

```typescript
import { defineStore } from "pinia";
import { ref } from "vue";
import type { AppConfig } from "../lib/tauri";
import { loadConfig as loadCfg, saveConfig as saveCfg } from "../lib/tauri";

export const useAppStore = defineStore("app", () => {
  const config = ref<AppConfig | null>(null);
  const connected = ref(false);
  const lastHeartbeat = ref("");
  const error = ref<string | null>(null);
  const loading = ref(true);

  async function load() {
    loading.value = true;
    try {
      config.value = await loadCfg();
    } catch {
      config.value = null;
    } finally {
      loading.value = false;
    }
  }

  async function save(cfg: AppConfig) {
    await saveCfg(cfg);
    config.value = cfg;
  }

  function setConnected(status: string, heartbeat?: string) {
    if (status === "已连接") {
      connected.value = true;
      error.value = null;
    } else {
      connected.value = false;
      if (status.startsWith("错误")) {
        error.value = status;
      }
    }
    if (heartbeat) lastHeartbeat.value = heartbeat;
  }

  function setError(msg: string) {
    error.value = msg;
    connected.value = false;
  }

  return { config, connected, lastHeartbeat, error, loading, load, save, setConnected, setError };
});
```

- [ ] **步骤 2：编写 stores/file.ts**

```typescript
import { defineStore } from "pinia";
import { ref } from "vue";

export const useFileStore = defineStore("file", () => {
  const fileName = ref<string | null>(null);
  const fileSize = ref(0);
  const filePath = ref<string | null>(null);
  const visible = ref(false);

  function show(name: string, size: number, path: string) {
    fileName.value = name;
    fileSize.value = size;
    filePath.value = path;
    visible.value = true;
  }

  function dismiss() {
    visible.value = false;
  }

  return { fileName, fileSize, filePath, visible, show, dismiss };
});
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/stores/app.ts client/src/stores/file.ts
git commit -m "feat: 实现 Pinia Store（app + file）"
```

---

### 任务 4：实现全局样式

**文件：**
- 创建：`client/src/style.css`

- [ ] **步骤 1：编写 style.css**

```css
:root {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial,
    sans-serif;
  font-size: 14px;
  line-height: 1.5;
  color: #0f0f0f;
  background-color: #f6f6f6;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  margin: 0;
  min-height: 100vh;
}

#app {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 20px;
}

input {
  border: 1px solid #ccc;
  border-radius: 6px;
  padding: 10px 12px;
  font-size: 14px;
  width: 100%;
  outline: none;
  transition: border-color 0.2s;
}

input:focus {
  border-color: #396cd8;
}

button {
  border: none;
  border-radius: 6px;
  padding: 10px 24px;
  font-size: 14px;
  background-color: #396cd8;
  color: #fff;
  cursor: pointer;
  transition: background-color 0.2s;
}

button:hover:not(:disabled) {
  background-color: #2b5bbf;
}

button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

h1 {
  font-size: 1.5rem;
  margin-bottom: 8px;
}

p {
  margin-bottom: 6px;
}
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/style.css
git commit -m "feat: 添加全局样式"
```

---

### 任务 5：实现 ConfigPage.vue

**文件：**
- 创建：`client/src/views/ConfigPage.vue`

- [ ] **步骤 1：编写 ConfigPage.vue**

```vue
<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "../stores/app";

const router = useRouter();
const appStore = useAppStore();

const serverUrl = ref("ws://");
const clientId = ref("");
const passkey = ref("");
const saving = ref(false);

async function save() {
  saving.value = true;
  try {
    await appStore.save({ serverUrl: serverUrl.value, clientId: clientId.value, passkey: passkey.value });
    router.push("/status");
  } catch (e: any) {
    appStore.setError(String(e));
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <div class="config">
    <h1>File Transfer Hub</h1>
    <p>首次使用，请配置服务器连接信息</p>
    <input v-model="serverUrl" placeholder="WebSocket 地址 (ws://...)" />
    <input v-model="clientId" placeholder="客户端 ID" />
    <input v-model="passkey" type="password" placeholder="Passkey" />
    <p v-if="appStore.error" class="error">{{ appStore.error }}</p>
    <button :disabled="saving" @click="save">
      {{ saving ? "保存中..." : "保存并连接" }}
    </button>
  </div>
</template>

<style scoped>
.config {
  width: 100%;
  max-width: 360px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.error {
  color: #d32f2f;
  font-size: 0.9rem;
}
</style>
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/views/ConfigPage.vue
git commit -m "feat: 实现 ConfigPage 配置页"
```

---

### 任务 6：实现 StatusPage.vue

**文件：**
- 创建：`client/src/views/StatusPage.vue`

- [ ] **步骤 1：编写 StatusPage.vue**

```vue
<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { useAppStore } from "../stores/app";
import { useFileStore } from "../stores/file";
import { onConnectionStatus, onFileReceived } from "../lib/tauri";
import StatusIndicator from "../components/StatusIndicator.vue";

const appStore = useAppStore();
const fileStore = useFileStore();

let unlistenConnection: (() => void) | undefined;
let unlistenFile: (() => void) | undefined;

onMounted(async () => {
  unlistenConnection = await onConnectionStatus((status, heartbeat) => {
    appStore.setConnected(status, heartbeat);
  });
  unlistenFile = await onFileReceived((name, size, path) => {
    fileStore.show(name, size, path);
  });
});

onUnmounted(() => {
  unlistenConnection?.();
  unlistenFile?.();
});

function formatSize(bytes: number): string {
  return (bytes / 1024 / 1024).toFixed(2) + " MB";
}
</script>

<template>
  <div class="status">
    <h1>File Transfer Hub</h1>
    <StatusIndicator />
    <p v-if="appStore.error" class="error">{{ appStore.error }}</p>
    <p v-if="appStore.lastHeartbeat">上次心跳：{{ appStore.lastHeartbeat }}</p>
    <div v-if="fileStore.visible" class="file-card">
      <p>📄 {{ fileStore.fileName }} ({{ formatSize(fileStore.fileSize) }})</p>
      <p>保存到：{{ fileStore.filePath }}</p>
      <button @click="fileStore.dismiss()">关闭</button>
    </div>
  </div>
</template>

<style scoped>
.status {
  width: 100%;
  max-width: 480px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.error {
  color: #d32f2f;
}

.file-card {
  border: 1px solid #ccc;
  border-radius: 8px;
  padding: 16px;
  background: #fff;
  margin-top: 8px;
}

.file-card p {
  word-break: break-all;
}
</style>
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/views/StatusPage.vue
git commit -m "feat: 实现 StatusPage 状态页"
```

---

### 任务 7：实现 StatusIndicator.vue

**文件：**
- 创建：`client/src/components/StatusIndicator.vue`

- [ ] **步骤 1：编写 StatusIndicator.vue**

```vue
<script setup lang="ts">
import { computed } from "vue";
import { useAppStore } from "../stores/app";

const appStore = useAppStore();

const indicatorClass = computed(() => {
  if (appStore.error) return "indicator error";
  if (appStore.connected) return "indicator connected";
  return "indicator disconnected";
});

const statusText = computed(() => {
  if (appStore.error) return "连接错误";
  if (appStore.connected) return "已连接";
  return "未连接";
});
</script>

<template>
  <div class="status-bar">
    <span :class="indicatorClass" />
    <span>{{ statusText }}</span>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.9rem;
}

.indicator {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  display: inline-block;
}

.connected {
  background-color: #4caf50;
}

.disconnected {
  background-color: #9e9e9e;
}

.error {
  background-color: #d32f2f;
}
</style>
```

- [ ] **步骤 2：Commit**

```bash
git add client/src/components/StatusIndicator.vue
git commit -m "feat: 实现 StatusIndicator 连接状态指示器"
```

---

### 任务 8：实现根组件 App.vue 和入口 main.ts

**文件：**
- 创建：`client/src/App.vue`
- 创建：`client/src/main.ts`

- [ ] **步骤 1：编写 App.vue**

```vue
<script setup lang="ts">
import { onMounted } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "./stores/app";

const router = useRouter();
const appStore = useAppStore();

onMounted(async () => {
  await appStore.load();
  if (appStore.config) {
    router.push("/status");
  } else {
    router.push("/config");
  }
});
</script>

<template>
  <div v-if="appStore.loading" class="loading">
    <p>加载中...</p>
  </div>
  <router-view v-else />
</template>

<style scoped>
.loading {
  text-align: center;
  color: #666;
}
</style>
```

- [ ] **步骤 2：编写 main.ts**

```typescript
import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHistory } from "vue-router";
import App from "./App.vue";
import ConfigPage from "./views/ConfigPage.vue";
import StatusPage from "./views/StatusPage.vue";
import "./style.css";

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/config" },
    { path: "/config", component: ConfigPage },
    { path: "/status", component: StatusPage },
  ],
});

const app = createApp(App);
const pinia = createPinia();

app.use(pinia);
app.use(router);
app.mount("#app");
```

- [ ] **步骤 3：Commit**

```bash
git add client/src/App.vue client/src/main.ts
git commit -m "feat: 实现根组件和入口，集成 Pinia + Vue Router"
```

---

### 任务 9：清理旧 Svelte 文件

**文件：**
- 删除：`client/src/App.svelte`
- 删除：`client/src/ConfigPage.svelte`
- 删除：`client/src/StatusPage.svelte`

- [ ] **步骤 1：删除旧文件**

```bash
git rm client/src/App.svelte client/src/ConfigPage.svelte client/src/StatusPage.svelte
```

- [ ] **步骤 2：Commit**

```bash
git commit -m "chore: 清理旧 Svelte 文件"
```

---

### 任务 10：安装依赖并验证构建

- [ ] **步骤 1：安装 npm 依赖**

运行：
```bash
cd client && npm install
```

预期：无错误，`node_modules/` 中包含 vue、pinia、vue-router、vue-tsc 等

- [ ] **步骤 2：运行 TypeScript 类型检查**

运行：
```bash
cd client && npx vue-tsc --noEmit
```

预期：无错误输出

- [ ] **步骤 3：运行 Vite 构建**

运行：
```bash
cd client && npm run build
```

预期：dist/ 目录生成，包含 index.html + js/css 资源

- [ ] **步骤 4：提交最终状态**

```bash
git add -A
git commit -m "chore: 安装 Vue 3 依赖并验证构建"
```
