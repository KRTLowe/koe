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

assert(
  /\.markdown-body\s*\{[\s\S]*?white-space:\s*normal/.test(bubbleSource),
  "BubblePage markdown container must reset white-space to normal for rendered HTML",
);

const unsafe = marked.parse('<img src=x onerror="alert(1)">\n\n<script>alert(2)</script>');
const sanitized = DOMPurify.sanitize(String(unsafe));

assert(!sanitized.includes("onerror"), "DOMPurify must remove event attributes");
assert(!sanitized.includes("<script"), "DOMPurify must remove script tags");

console.log("Bubble markdown rendering source constraints are present.");
