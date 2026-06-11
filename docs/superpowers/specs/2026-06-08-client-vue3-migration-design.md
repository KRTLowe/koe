# kaya-beam 客户端 Vue 3 迁移设计

## 概述

将 kaya-beam 的 Tauri 桌面客户端前端从 Svelte 5 迁移到 Vue 3，参考 ACP UI 的技术选型和工程结构。

## 背景

当前客户端使用 Svelte 5 + Vite + TypeScript + Tauri 2。共 4 个前端文件（`main.ts`、`App.svelte`、`ConfigPage.svelte`、`StatusPage.svelte`），无路由库、无状态管理、无 CSS 框架。Rust 后端已稳定运行。

迁移目标是替换前端框架而不改动 Rust 后端。

## 技术选型

| 层面 | 当前 | 迁移后 | 参考来源 |
|------|------|--------|----------|
| UI 框架 | Svelte 5 | **Vue 3** | ACP UI |
| 构建 | Vite + svelte-vite-plugin | Vite + @vitejs/plugin-vue | ACP UI |
| 状态管理 | 无（Svelte 内置） | **Pinia** | ACP UI |
| 路由 | 无（条件渲染） | **Vue Router** | ACP UI |
| CSS | 无框架 | **纯 CSS 自写** | ACP UI |
| 类型检查 | TypeScript | TypeScript + vue-tsc | ACP UI |

## 目录结构

```
client/
├── src/
│   ├── assets/                    # 静态资源
│   ├── components/
│   │   └── StatusIndicator.vue    # 连接状态指示器
│   ├── lib/
│   │   └── tauri.ts               # Tauri invoke/listen 封装
│   ├── stores/
│   │   ├── app.ts                 # 应用全局状态
│   │   └── file.ts                # 文件接收状态
│   ├── views/
│   │   ├── ConfigPage.vue         # 配置页
│   │   └── StatusPage.vue         # 状态页
│   ├── App.vue                    # 根组件 + router-view
│   ├── main.ts                    # 入口：createApp + Pinia + mount
│   └── style.css                  # 全局样式
├── index.html                     # 不动
├── package.json                   # 替换依赖
├── vite.config.ts                 # 替换插件
├── tsconfig.json                  # 新增
├── tsconfig.node.json             # 新增
└── svelte.config.js               # 删除
```

## 路由设计

使用 Vue Router 5（`vue-router@^5`），`createRouter` + `createWebHistory`。

| 路径 | 组件 | 守卫 |
|------|------|------|
| `/` | 重定向 | 启动时 invoke("load_config") 判断去向 |
| `/config` | ConfigPage.vue | 未配置时的目标 |
| `/status` | StatusPage.vue | 已配置时的目标 |

启动流程：
1. 应用挂载 → `App.vue` `onMounted` 调用 `loadConfig()`
2. 未配置 → `router.push("/config")`
3. 已配置 → `router.push("/status")`

## Store 设计

### stores/app.ts

```typescript
interface AppConfig {
  serverUrl: string
  clientId: string
  passkey: string
}

// State
config: AppConfig | null
connected: boolean
lastHeartbeat: string
error: string | null
loading: boolean

// Actions
loadConfig()    // invoke("load_config") → 设置 config/navigate
saveConfig(cfg) // invoke("save_config") → 成功后 push /status
setError(msg)   // 设置错误信息
```

### stores/file.ts

```typescript
// State
fileName: string | null
fileSize: number
filePath: string | null
visible: boolean

// Actions
showFile(name, size, path)  // 显示文件卡片
dismiss()                    // 隐藏文件卡片
```

## Tauri 桥接层

`src/lib/tauri.ts` 封装所有 Tauri API 调用：

```typescript
// 命令
export async function loadConfig(): Promise<AppConfig | null>
export async function saveConfig(config: AppConfig): Promise<void>

// 事件监听（返回 unlisten 函数）
export async function onConnectionStatus(cb: (status, heartbeat?) => void): Promise<() => void>
export async function onFileReceived(cb: (name, size, path) => void): Promise<() => void>
```

## 数据流

```
Tauri invoke 命令:
  invoke("load_config")  ──→  app.loadConfig()  ──→  router push (config/status)
  invoke("save_config")  ──→  app.saveConfig()   ──→  router push /status

Tauri 事件:
  connection-status  ──→  lib/tauri.ts  ──→  app.connected / app.error 更新
  file-received      ──→  lib/tauri.ts  ──→  file store 更新
```

## 组件行为

### ConfigPage.vue
- 3 个输入框：serverUrl / clientId / passkey
- 表单校验：三个字段不能为空
- 保存按钮：调用 `saveConfig()`，保存中按钮 disabled
- 错误提示：红色文字行内显示

### StatusPage.vue
- 连接状态文字（"已连接"/"已断开"/"错误：..."）
- 上次心跳时间（有条件显示）
- 文件卡片：文件名、大小、保存路径（有条件显示）
- 自动接收 Tauri 事件更新状态

### StatusIndicator.vue（可选）
- 圆点指示器 + 状态文字
- 绿色 = 已连接，红色 = 断开/错误
- 可复用于标题栏

## Rust 后端接口（不变）

### 命令
- `load_config` → `Promise<AppConfig | null>`
- `save_config` → `void`（带服务端校验）

### 事件
- `connection-status` → `{ status: string, lastHeartbeat?: string }`
- `file-received` → `{ name: string, size: number, path: string }`

## 不需要做的事情

- ❌ 不改 Rust 后端一行代码
- ❌ 不改 Tauri 配置（tauri.conf.json、Cargo.toml）
- ❌ 不改 index.html
- ❌ 不加 CSS 框架或组件库
- ❌ 不加单元测试（当前 Svelte 版也没有前端测试）

## 迁移步骤（实现计划待 writing-plans 细化）

1. 更新 package.json：删 Svelte 依赖，加 Vue/Pinia/Router/vue-tsc
2. 更新 vite.config.ts：替换 vite 插件
3. 删除 svelte.config.js
4. 新增 tsconfig.json + tsconfig.node.json
5. 创建目录结构：assets/ components/ lib/ stores/ views/
6. 编写 lib/tauri.ts
7. 编写 stores/app.ts + stores/file.ts
8. 编写 views/ConfigPage.vue
9. 编写 views/StatusPage.vue
10. 编写 components/StatusIndicator.vue
11. 编写 App.vue + main.ts + style.css
12. 删除旧的 Svelte 文件（App.svelte, ConfigPage.svelte, StatusPage.svelte）
13. npm install 并验证构建
14. 验证 Tauri dev 启动正常
