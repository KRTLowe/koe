# kaya-beam 前端重设计规格

## 概述

对 kaya-beam Windows 桌面客户端的前端进行全面 UI 重设计，解决三个核心问题：
1. 样式粗糙，缺乏现代感
2. 聊天页面高度随内容增长，没有固定约束
3. 缺少导航骨架，页面切换靠路由硬跳

## 布局架构

### 整体骨架

```
┌──────────────────────────────────────┐
│  侧栏 (200px)  │    内容区 (flex:1)   │
│  ┌───────────┐ │  ┌────────────────┐  │
│  │ kaya-beam │ │  │ 页面标题        │  │
│  │ (Logo)    │ │  │                │  │
│  ├───────────┤ │  │  内容区块       │  │
│  │ 🏠 Home   │ │  │  （溢出滚动）   │  │
│  │ 📁 文件传输 │ │  │                │  │
│  │ 💬 聊天    │ │  │                │  │
│  │ ⚙️ 设置    │ │  │                │  │
│  ├───────────┤ │  └────────────────┘  │
│  │ v0.1.0    │ │                      │
│  └───────────┘ │                      │
└──────────────────────────────────────┘
```

- 侧栏固定 200px 宽，图标+文字
- 内容区 `flex:1; overflow-y:auto`
- 选中态使用品牌色 #6366F1 高亮

### 布局组件

新增 `AppLayout.vue` 作为所有页面的包装组件：

```vue
<template>
  <div class="app-layout">
    <Sidebar />
    <main class="content">
      <router-view />
    </main>
  </div>
</template>
```

`router-view` 直接放在 `AppLayout` 内部，每个页面只需要写自己的内容区内容，不需要重复写布局骨架。

## 导航设计

侧栏固定在左侧，始终 4 个菜单项：

| 图标 | 名称 | 路由 | 页面组件 |
|------|------|------|---------|
| 🏠 | Home | `/home` | HomePage.vue |
| 📁 | 文件传输 | `/files` | FileTransferPage.vue |
| 💬 | 聊天 | `/chat` | ChatPage.vue |
| ⚙️ | 设置 | `/settings` | SettingsPage.vue |

侧栏包含：
- 顶部：应用名称 `kaya-beam` + 副标题 `File Transfer Hub`
- 中间：4 个导航项，当前路由高亮
- 底部：版本号 `v0.1.0`

## 页面设计

### 1. Home 页 (`/home`)

精简 Dashboard，只展示核心信息：

- **连接状态卡片**：绿点 + "已连接"/"已断开" + 状态文字
- **存储路径卡片**：显示当前保存目录（默认为 `~/kaya-transfer/`）
- **最近传输列表**：表格样式，显示文件名、大小、时间。最多 10 条。

### 2. 文件传输页 (`/files`)

类聊天气泡样式展示文件收发记录：

- **收到文件**：左对齐，Kaya 头像（#6366F1 圆形），白色气泡，显示文件图标+文件名+大小+"✓ 已接收"+时间
- **发送文件**（预留）：右对齐，"我"头像（#22C55E 圆形），品牌色气泡，显示文件图标+文件名+大小+"已发送 ✓"+时间
- **底部发送框**：虚线边框占位区 + "选择文件或拖拽到此处..." + 发送按钮（灰色禁用态），标注"发送文件功能即将上线"

传输记录来自 `stores/file.ts`（适配现有 Pinia store）。

### 3. 聊天页 (`/chat`)

**核心改动：消息区域固定高度，内容溢出滚动**

- **顶部栏**：绿点 + "Kaya" + 在线状态
- **消息区**：`flex:1; overflow-y:auto; min-height:0`，高度由父容器撑满，不随内容增长
  - Kaya 消息：左对齐，白色气泡
  - 用户消息：右对齐，品牌色气泡
- **输入框**：固定在底部，不影响消息区
- 消息组件 `ChatMessage.vue` 和 `ChatInput.vue` 复用现有实现，只改样式

### 4. 设置页 (`/settings`)

分区块表单布局：

- **🔗 服务端连接**：
  - WebSocket 地址（input，默认 placeholder）
  - ACP 桥接地址（自动推导，也可手动）
  - 客户端 ID（input）
  - Passkey（input type=password）
- **📂 存储设置**：
  - 默认存储路径（input + "浏览"按钮占位）
  - 说明文字："文件将保存到此目录下的 YYYY-MM/ 子文件夹"
- **操作按钮**：重置 / 保存配置

## CSS 设计系统

### 配色

| Token | 值 | 用途 |
|-------|-----|------|
| `--color-primary` | `#6366F1` | 品牌色，选中态、按钮、Kaya 头像 |
| `--color-bg` | `#F5F5FA` | 内容区背景 |
| `--color-surface` | `#FFFFFF` | 卡片/侧栏背景 |
| `--color-border` | `#E8E8EE` | 分割线、边框 |
| `--color-text` | `#1a1a2e` | 主文字 |
| `--color-text-secondary` | `#555` | 二级文字 |
| `--color-text-muted` | `#999` | 辅助文字 |
| `--color-success` | `#22C55E` | 在线状态、已接收 |
| `--color-sidebar-hover` | `#F0F0F5` | 侧栏悬停态 |

### 圆角

- 卡片：12px
- 按钮/输入框：8-10px
- 消息气泡：12px

### 阴影

- 卡片：`0 1px 3px rgba(0,0,0,0.04)`

### 字体

- 系统默认：`-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`
- 代码块：`monospace`

## 路由变更

当前路由：
```
/config → /status → /chat
```

新路由：
```
/  → redirect → /home
/home        → HomePage.vue
/files       → FileTransferPage.vue
/chat        → ChatPage.vue (重构)
/settings    → SettingsPage.vue
```

ConfigPage 路由移除，其功能合并到 SettingsPage。StatusPage 内容合并到 HomePage。

## 现有代码改动清单

### 新增文件

| 文件 | 说明 |
|------|------|
| `src/components/AppLayout.vue` | 应用骨架（侧栏 + 内容区） |
| `src/components/Sidebar.vue` | 侧栏导航组件 |
| `src/views/HomePage.vue` | Home 页（替代 StatusPage） |
| `src/views/FileTransferPage.vue` | 文件传输页 |
| `src/views/SettingsPage.vue` | 设置页（替代 ConfigPage） |

### 修改文件

| 文件 | 改动 |
|------|------|
| `src/App.vue` | 包裹 `<AppLayout>`，移除旧路由逻辑 |
| `src/main.ts` | 更新路由表：4 条新路由 |
| `src/views/ChatPage.vue` | 重构：消息区固定高度，调整样式 |
| `src/components/ChatMessage.vue` | 气泡样式刷新 |
| `src/components/ChatInput.vue` | 输入框样式刷新 |
| `src/components/StatusIndicator.vue` | 保留或合并到 sidebar |
| `src/stores/app.ts` | 适配新配置字段（add acp_url, storage_path） |
| `src/style.css` | 替换为设计系统 CSS 变量 |

### 删除文件

| 文件 | 替代 |
|------|------|
| `src/views/ConfigPage.vue` | → SettingsPage.vue |
| `src/views/StatusPage.vue` | → HomePage.vue |

## 现有 Pinia store 适配

### `stores/app.ts`

当前 `AppConfig` 类型（来自 Rust `config.rs`）：

```typescript
interface AppConfig {
  server_url: string;
  client_id: string;
  passkey: string;
}
```

新增字段（与 Rust 侧 `config.rs` 同步）：

```typescript
interface AppConfig {
  server_url: string;
  client_id: string;
  passkey: string;
  acp_url?: string;        // 新增，可由 server_url 推导
  storage_path?: string;   // 新增，默认 ~/kaya-transfer/
}
```

### `stores/file.ts`

现有 Pinia store 用来存储文件接收通知。文件传输页直接消费其状态。

### `stores/chat.ts`

现有实现不做结构性改动，只改样式。

## Rust 后端适配

### `config.rs`

`AppConfig` 结构体新增两个可选字段：

```rust
pub struct AppConfig {
    pub server_url: String,
    pub client_id: String,
    pub passkey: String,
    pub acp_url: Option<String>,       // 新增，覆盖自动推导
    pub storage_path: Option<String>,  // 新增，默认 ~/kaya-transfer/
}
```

### `lib.rs`

- `acp_url_from_config()` 改为优先使用 `config.acp_url`，回退到自动推导
- `save_config` 命令保留 ACP 地址和存储路径
- 首次保存配置后，存储路径传递给 `file_handler`

这些改动量极小（< 20 行），与前端重设计同步实施。

## 未改动范围

- Python 服务端不涉及
- 文件传输协议不涉及
- ACP 协议不涉及
