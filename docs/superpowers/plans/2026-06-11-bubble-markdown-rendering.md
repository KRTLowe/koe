# 气泡完整 Markdown 渲染实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 让悬浮消息气泡支持完整 Markdown / GFM 渲染，同时完全禁用原始 HTML。

**架构：** Rust 气泡管线继续传递 raw Markdown 字符串，不改变 `bubble_content` 状态模型。Vue 层新增一个专用 Markdown 渲染 helper，`BubblePage.vue` 用 `v-html` 渲染 helper 输出的已清洗 HTML，并用紧凑 CSS 约束表格、代码块、图片等元素。

**技术栈：** Vue 3 `<script setup>`、Tauri IPC、`marked@18`、`isomorphic-dompurify`、现有 Node 静态测试脚本风格。

---

## 文件结构

- 创建：`client/src/lib/markdown.ts`
  - 负责把 raw Markdown 转成安全 HTML。
  - 内部配置 Marked renderer，阻止 raw HTML token 输出真实 HTML。
  - 内部调用 DOMPurify 清洗最终 HTML。
- 修改：`client/src/views/BubblePage.vue`
  - 从纯文本插值改为 `v-html="renderedHtml"`。
  - 引入 `renderMarkdown()`。
  - 添加 `.markdown-body` 紧凑样式。
  - 保留 `remeasure()` 逻辑。
- 修改：`client/package.json`
  - 新增 `isomorphic-dompurify` dependency。
- 修改：`client/package-lock.json`
  - 由 `npm install isomorphic-dompurify` 自动更新。
- 创建：`client/tests/bubble-markdown-rendering.test.mjs`
  - 按项目现有测试风格，用 Node 读取源码并验证关键安全/渲染约束存在。
  - 额外直接用 `marked` + `isomorphic-dompurify` 验证危险 HTML 会被清理。

---

### 任务 1：添加 Markdown sanitizer 依赖

**文件：**
- 修改：`client/package.json`
- 修改：`client/package-lock.json`

- [ ] **步骤 1：安装依赖**

运行：

```bash
npm install isomorphic-dompurify
```

工作目录：`client/`

预期：

- `client/package.json` 的 `dependencies` 中出现 `isomorphic-dompurify`。
- `client/package-lock.json` 更新。
- 命令退出码为 0。

- [ ] **步骤 2：检查依赖写入**

运行：

```bash
node -e "const p=require('./package.json'); if(!p.dependencies['isomorphic-dompurify']) process.exit(1); console.log(p.dependencies['isomorphic-dompurify'])"
```

工作目录：`client/`

预期：输出版本范围，例如 `^2.x.x`，退出码为 0。

- [ ] **步骤 3：版本控制检查点**

仅在用户明确要求 commit 时执行：

```bash
git add client/package.json client/package-lock.json
git commit -m "chore: add markdown sanitizer dependency"
```

---

### 任务 2：创建 Markdown 渲染 helper

**文件：**
- 创建：`client/src/lib/markdown.ts`
- 测试：`client/tests/bubble-markdown-rendering.test.mjs`

- [ ] **步骤 1：编写失败的静态测试**

创建 `client/tests/bubble-markdown-rendering.test.mjs`，初始内容：

```js
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { marked } from "marked";
import DOMPurify from "isomorphic-dompurify";

const __dirname = dirname(fileURLToPath(import.meta.url));
const markdownPath = resolve(__dirname, "../src/lib/markdown.ts");
const bubblePath = resolve(__dirname, "../src/views/BubblePage.vue");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

const markdownSource = readFileSync(markdownPath, "utf8");
const bubbleSource = readFileSync(bubblePath, "utf8");

assert(
  markdownSource.includes("from \"marked\"") || markdownSource.includes("from 'marked'"),
  "markdown helper must import marked",
);

assert(
  markdownSource.includes("isomorphic-dompurify"),
  "markdown helper must import isomorphic-dompurify",
);

assert(
  /html\s*\(/.test(markdownSource),
  "markdown helper must override the Marked html renderer/token output",
);

assert(
  markdownSource.includes("DOMPurify.sanitize"),
  "markdown helper must sanitize rendered HTML",
);

assert(
  bubbleSource.includes("v-html=\"renderedHtml\""),
  "BubblePage must render sanitized markdown HTML with v-html",
);

const unsafe = marked.parse('<img src=x onerror="alert(1)">\n\n<script>alert(2)</script>');
const sanitized = DOMPurify.sanitize(String(unsafe));

assert(!sanitized.includes("onerror"), "DOMPurify must remove event attributes");
assert(!sanitized.includes("<script"), "DOMPurify must remove script tags");

console.log("Bubble markdown rendering source constraints are present.");
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
node tests/bubble-markdown-rendering.test.mjs
```

工作目录：`client/`

预期：FAIL，原因是 `src/lib/markdown.ts` 不存在，或 `BubblePage.vue` 尚未包含 `v-html="renderedHtml"`。

- [ ] **步骤 3：创建 helper 最小实现**

创建 `client/src/lib/markdown.ts`：

```ts
import { Renderer, marked } from "marked";
import DOMPurify from "isomorphic-dompurify";

const renderer = new Renderer();

function escapeHtml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

renderer.html = ({ text }) => escapeHtml(text);

const purifyConfig = {
  ALLOW_DATA_ATTR: false,
  ALLOWED_URI_REGEXP:
    /^(?:(?:https?|mailto|tel):|[^a-z]|[a-z+.-]+(?:[^a-z+.-:]|$))/i,
};

export function renderMarkdown(markdown: string): string {
  try {
    const html = marked.parse(markdown, {
      async: false,
      gfm: true,
      breaks: false,
      renderer,
    });

    return DOMPurify.sanitize(String(html), purifyConfig);
  } catch (error) {
    console.error("[markdown] render failed", error);
    return `<p>${escapeHtml(markdown)}</p>`;
  }
}
```

- [ ] **步骤 4：运行测试确认 helper 相关断言仍因 BubblePage 未改而失败**

运行：

```bash
node tests/bubble-markdown-rendering.test.mjs
```

工作目录：`client/`

预期：FAIL，报错应推进到 `BubblePage must render sanitized markdown HTML with v-html`。

- [ ] **步骤 5：类型检查 helper**

运行：

```bash
npm run build
```

工作目录：`client/`

预期：当前可能仍 FAIL，因为 `BubblePage.vue` 未使用 helper；如果出现 `Renderer` API 类型不匹配，应按 `node_modules/marked/lib/marked.d.ts` 中 `html({ text }: Tokens.HTML | Tokens.Tag)` 的签名修正。

---

### 任务 3：改造 BubblePage 为 Markdown HTML 渲染

**文件：**
- 修改：`client/src/views/BubblePage.vue`
- 测试：`client/tests/bubble-markdown-rendering.test.mjs`

- [ ] **步骤 1：修改 script 引入 computed 和 helper**

将 `BubblePage.vue` 顶部：

```ts
import { ref, nextTick, onMounted } from "vue";
```

改为：

```ts
import { computed, ref, nextTick, onMounted } from "vue";
import { renderMarkdown } from "../lib/markdown";
```

在 `const bubbleRef = ...` 后添加：

```ts
const renderedHtml = computed(() => renderMarkdown(text.value));
```

- [ ] **步骤 2：修改 template 使用 v-html**

将：

```vue
<div ref="bubbleRef" class="bubble-text">{{ text }}</div>
```

改为：

```vue
<div ref="bubbleRef" class="bubble-text markdown-body" v-html="renderedHtml"></div>
```

- [ ] **步骤 3：运行测试验证核心绑定通过**

运行：

```bash
node tests/bubble-markdown-rendering.test.mjs
```

工作目录：`client/`

预期：PASS，输出 `Bubble markdown rendering source constraints are present.`

- [ ] **步骤 4：运行 TypeScript/Vite 构建**

运行：

```bash
npm run build
```

工作目录：`client/`

预期：PASS，`vue-tsc --noEmit && vite build` 退出码为 0。

---

### 任务 4：添加气泡内 Markdown 样式

**文件：**
- 修改：`client/src/views/BubblePage.vue`
- 测试：`client/tests/bubble-markdown-rendering.test.mjs`

- [ ] **步骤 1：扩展静态测试覆盖样式约束**

在 `client/tests/bubble-markdown-rendering.test.mjs` 的 `bubbleSource` 断言后追加：

```js
assert(
  bubbleSource.includes(".markdown-body :deep(pre)"),
  "BubblePage must style markdown code blocks",
);

assert(
  bubbleSource.includes("overflow-x: auto"),
  "BubblePage markdown code/table content must be horizontally scrollable",
);

assert(
  bubbleSource.includes(".markdown-body :deep(table)"),
  "BubblePage must style markdown tables",
);

assert(
  bubbleSource.includes(".markdown-body :deep(img)"),
  "BubblePage must constrain markdown images",
);
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
node tests/bubble-markdown-rendering.test.mjs
```

工作目录：`client/`

预期：FAIL，报错为缺少 Markdown 样式相关断言。

- [ ] **步骤 3：添加紧凑 Markdown CSS**

在 `BubblePage.vue` `<style scoped>` 中 `.bubble-text` 后添加：

```css
.markdown-body :deep(*) {
  box-sizing: border-box;
}

.markdown-body :deep(p),
.markdown-body :deep(ul),
.markdown-body :deep(ol),
.markdown-body :deep(blockquote),
.markdown-body :deep(pre),
.markdown-body :deep(table) {
  margin: 0 0 8px;
}

.markdown-body :deep(p:last-child),
.markdown-body :deep(ul:last-child),
.markdown-body :deep(ol:last-child),
.markdown-body :deep(blockquote:last-child),
.markdown-body :deep(pre:last-child),
.markdown-body :deep(table:last-child) {
  margin-bottom: 0;
}

.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3) {
  margin: 0 0 8px;
  line-height: 1.25;
  color: #fff;
}

.markdown-body :deep(h1) {
  font-size: 17px;
}

.markdown-body :deep(h2) {
  font-size: 16px;
}

.markdown-body :deep(h3) {
  font-size: 15px;
}

.markdown-body :deep(ul),
.markdown-body :deep(ol) {
  padding-left: 18px;
}

.markdown-body :deep(blockquote) {
  padding-left: 10px;
  border-left: 3px solid rgba(147, 197, 253, 0.85);
  color: #cbd5e1;
}

.markdown-body :deep(code) {
  padding: 1px 5px;
  border-radius: 5px;
  background: rgba(255, 255, 255, 0.12);
  color: #dbeafe;
  font-family: ui-monospace, SFMono-Regular, Consolas, "Liberation Mono", monospace;
  font-size: 12px;
}

.markdown-body :deep(pre) {
  max-width: 100%;
  overflow-x: auto;
  padding: 8px;
  border-radius: 8px;
  background: rgba(255, 255, 255, 0.1);
}

.markdown-body :deep(pre code) {
  padding: 0;
  background: transparent;
  color: inherit;
  white-space: pre;
}

.markdown-body :deep(table) {
  display: block;
  max-width: 100%;
  overflow-x: auto;
  border-collapse: collapse;
  font-size: 12px;
}

.markdown-body :deep(th),
.markdown-body :deep(td) {
  padding: 4px 6px;
  border: 1px solid rgba(148, 163, 184, 0.7);
}

.markdown-body :deep(th) {
  background: rgba(255, 255, 255, 0.08);
}

.markdown-body :deep(img) {
  max-width: 100%;
  max-height: 220px;
  border-radius: 8px;
}

.markdown-body :deep(a) {
  color: #93c5fd;
  text-decoration: underline;
  text-underline-offset: 2px;
}
```

- [ ] **步骤 4：运行样式测试确认通过**

运行：

```bash
node tests/bubble-markdown-rendering.test.mjs
```

工作目录：`client/`

预期：PASS。

- [ ] **步骤 5：运行构建确认 Vue scoped deep 语法通过**

运行：

```bash
npm run build
```

工作目录：`client/`

预期：PASS。

---

### 任务 5：端到端手动验证与回归检查

**文件：**
- 不新增文件。
- 使用已修改的 `client/src/views/BubblePage.vue` 和 `client/src/lib/markdown.ts`。

- [ ] **步骤 1：运行全部当前前端静态测试**

运行：

```bash
node tests/copilot-overlay-window.test.mjs
node tests/bubble-markdown-rendering.test.mjs
```

工作目录：`client/`

预期：两个脚本均 PASS。

- [ ] **步骤 2：运行生产前端构建**

运行：

```bash
npm run build
```

工作目录：`client/`

预期：PASS。

- [ ] **步骤 3：运行 Rust 编译检查**

运行：

```bash
cargo check
```

工作目录：`client/src-tauri/`

预期：PASS。允许已有 unused warnings；不允许新增 error。

- [ ] **步骤 4：手动气泡样例验证**

启动客户端后发送包含以下内容的回复或临时触发气泡：

```md
# GPU 结论

**可以跑**，但建议量化。

| 模式 | 建议 |
| --- | --- |
| Q4 | 稳 |
| Q5 | 可试 |

```bash
nvidia-smi
ollama ps
```

> 注意显存占用。

<script>alert(1)</script>
<img src=x onerror=alert(1)>
[bad](javascript:alert(1))
```

预期：

- 标题、粗体、表格、代码块、引用块正常显示。
- 代码块和表格横向滚动，不撑破气泡。
- `<script>` 不执行。
- `onerror` 不存在。
- `javascript:` 链接不能执行。

- [ ] **步骤 5：版本控制检查点**

仅在用户明确要求 commit 时执行：

```bash
git add client/package.json client/package-lock.json client/src/lib/markdown.ts client/src/views/BubblePage.vue client/tests/bubble-markdown-rendering.test.mjs
git commit -m "feat: render markdown in message bubbles"
```

---

## 规格覆盖自检

- 完整 Markdown / GFM：任务 2、3、4 覆盖。
- 完全禁用原始 HTML：任务 2 helper 和测试覆盖。
- 不改 Rust 管线：文件结构与任务均未修改 Rust 行为。
- 布局约束：任务 4 覆盖代码块、表格、图片、标题等样式。
- 验证：任务 5 覆盖静态测试、构建、Rust check 和手动恶意 HTML 样例。

## 占位符自检

本计划已检查占位内容风险。每个代码步骤都给出具体文件、代码片段、命令和预期结果。
