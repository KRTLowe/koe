# 气泡完整 Markdown 渲染设计

## 背景

当前悬浮消息气泡通过 `BubblePage.vue` 使用 `{{ text }}` 渲染纯文本。模型输出中的 Markdown 标记会原样显示，例如 `**重点**`、列表、代码块和表格都不会形成视觉层级。

项目已有 `marked@^18.0.5` 依赖，但目前没有任何 Markdown 渲染入口，也没有使用 `v-html`。Tauri 配置中 `csp` 为 `null`，因此一旦引入 HTML 渲染，必须显式处理 XSS 和布局边界。

## 目标

在悬浮消息气泡中支持完整 Markdown / GFM 渲染，让回复内容可以呈现标题、列表、引用、代码块、表格、链接和图片，而不是纯文本和标点。

## 非目标

- 不改 ACP / WebSocket / Rust 气泡生命周期。
- 不在 Rust 层预渲染 Markdown。
- 不同时改造主聊天页 `ChatMessage.vue`，除非后续单独扩展。
- 不允许原始 HTML 在气泡中作为真实 DOM 执行。

## 用户确认的范围

采用 **完整 Markdown / GFM 支持 + 完全禁用原始 HTML**：

- 支持：标题、粗体、斜体、列表、引用、行内代码、代码块、表格、链接、图片、分割线。
- 禁止：模型或用户文本中的原始 HTML 作为 HTML 执行。
- 示例：`<script>alert(1)</script>`、`<div style="...">`、`<img onerror=...>` 不应执行脚本或注入自定义 DOM 行为。

## 现有数据流

```text
ACP StreamChunk
  → acp_runtime.rs 写入 debounce_text / debounce_thinking
  → lib.rs 1s 循环在 1.5s 空闲后 flush
  → prefix diff + 按空行分段
  → bubble.rs create_message_bubble(rawText)
  → AppState.bubble_content[label] = rawText
  → Tauri 创建 /bubble 窗口
  → BubblePage.vue invoke("take_bubble_content")
  → {{ text }} 纯文本渲染
```

## 设计

### 渲染位置

Markdown 渲染放在 Vue 层的 `client/src/views/BubblePage.vue`。

Rust 继续只传递原始 Markdown 字符串，不改变 `bubble_content` 的类型与生命周期。

理由：

- Vue 层天然负责 DOM、样式和测量。
- `BubblePage.vue` 已经在内容加载后调用 `resize_bubble`，渲染成 HTML 后仍可通过 `scrollHeight` 重新测量。
- 保留 raw Markdown，便于日志、复制、调试和未来复用。

### 渲染链路

```text
raw markdown
  → marked.parse()
  → 禁用 / 转义 raw HTML token
  → DOMPurify.sanitize()
  → v-html
```

### 依赖

保留现有 `marked`，新增 sanitizer：

```bash
npm install isomorphic-dompurify
```

`isomorphic-dompurify` 在 Vite/Tauri WebView 环境中可用，也便于未来如果有 SSR/测试环境时复用。若实际打包兼容性出现问题，可回退到浏览器端 `dompurify`。

### HTML 禁用策略

采用双层防护：

1. Marked renderer/extension 层禁止 raw HTML token 输出真实 HTML。
2. DOMPurify 对最终 HTML 做二次清洗。

这样即使 Marked 配置变动或遇到边界输入，最终 `v-html` 也不会拿到危险 HTML。

实现原则：

- 原始 HTML 标签不作为真实 HTML 执行。
- Markdown 自身产生的安全标签保留，例如 `<p>`、`<strong>`、`<ul>`、`<pre>`、`<table>` 等。
- 链接需要禁止 `javascript:` 等危险协议。
- 图片允许，但必须受 CSS 尺寸限制。

### BubblePage 改造

当前：

```vue
<div ref="bubbleRef" class="bubble-text">{{ text }}</div>
```

目标：

```vue
<div ref="bubbleRef" class="bubble-text markdown-body" v-html="renderedHtml"></div>
```

`renderedHtml` 是基于 `text` 的 computed 值。`text` 改变后需要继续调用 `remeasure()`，确保 Markdown 展开后的高度被传回 Rust。

### 样式约束

气泡是小窗口，不是文档页面。完整 Markdown 需要压缩排版：

- `h1/h2/h3`：缩小字号和 margin。
- `p/ul/ol/blockquote/pre/table`：使用紧凑 margin。
- `pre`：横向滚动，避免撑宽窗口。
- `code`：使用轻量背景和圆角。
- `table`：`display: block; overflow-x: auto;` 或外层等效处理。
- `img`：`max-width: 100%; max-height` 限制，防止大图撑爆。
- `a`：清晰可见，必要时设置打开外部链接策略。

### 失败与降级

- 如果 Markdown 解析失败，气泡应回退到纯文本转义显示。
- 如果 sanitizer 依赖不可用，构建应失败，而不是静默渲染不安全 HTML。
- 如果某段 Markdown 产生非常高的内容，现有 `resize_bubble` 会扩高窗口；后续如需要可单独加最大高度和内部滚动。本次设计不默认改变气泡生命周期。

## 安全考虑

- `v-html` 只能接收 sanitizer 后的 HTML。
- 不使用 Marked 旧版 `sanitize` 选项；现代 Marked 已不应依赖该选项。
- Tauri 当前 `csp: null`，因此 sanitizer 是必要防线。
- 禁用 raw HTML 是产品边界，不只是技术实现细节。

## 测试计划

### 手动样例

用以下内容验证气泡：

1. 标题、粗体、列表。
2. 行内代码和多行代码块。
3. GFM 表格。
4. 引用块和分割线。
5. 图片 Markdown。
6. 恶意 HTML：

```md
<script>alert(1)</script>
<img src=x onerror=alert(1)>
<a href="javascript:alert(1)">bad</a>
```

期望：恶意脚本不执行，事件属性和危险链接被移除或转义。

### 构建验证

- `npm run build`
- `npm run tauri build` 或至少 `cargo check` + `vue-tsc --noEmit`

## 实施文件

- `client/package.json`：新增 sanitizer 依赖。
- `client/package-lock.json`：随 npm install 更新。
- `client/src/views/BubblePage.vue`：Markdown 渲染、HTML 清洗、Markdown 样式。

## 开放问题

当前无开放问题。用户已确认选择完整 Markdown，并要求完全禁用原始 HTML。
