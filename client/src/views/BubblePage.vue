<script setup lang="ts">
import { computed, ref, nextTick, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { renderMarkdown } from "../lib/markdown";

const THINK_MARKER = "__THINKING__";

const text = ref("");
const isThinking = ref(false);
const bubbleRef = ref<HTMLDivElement | null>(null);
const renderedHtml = computed(() => renderMarkdown(text.value));

function remeasure() {
  nextTick(() => {
    if (bubbleRef.value) {
      const h = bubbleRef.value.scrollHeight;
      const win = getCurrentWindow();
      invoke("resize_bubble", { label: win.label, height: h + 20 }).catch(() => {});
    }
  });
}

onMounted(async () => {
  document.body.style.margin = "0";
  document.body.style.padding = "0";
  document.body.style.overflow = "hidden";
  document.body.style.background = "transparent";

  const win = getCurrentWindow();
  try {
    const content = await invoke<string>("take_bubble_content", { label: win.label });
    if (content === THINK_MARKER) {
      isThinking.value = true;
      remeasure();
    } else {
      text.value = content;
      remeasure();
    }
  } catch { /* no content */ }
});
</script>

<template>
  <div class="bubble-root">
    <div class="bubble-body">
      <div v-if="isThinking" ref="bubbleRef" class="bubble-text think-bubble">
        <span class="think-spinner"></span>
        <span>卡雅思考中</span>
      </div>
      <div v-else ref="bubbleRef" class="bubble-text markdown-body" v-html="renderedHtml"></div>
      <div class="bubble-tail"></div>
    </div>
  </div>
</template>

<style scoped>
.bubble-root {
  width: 100vw;
  min-height: 100vh;
  display: flex;
  align-items: flex-start;
  justify-content: flex-start;
  padding: 6px 8px;
  box-sizing: border-box;
}

.bubble-body {
  display: flex;
  align-items: flex-start;
  gap: 0;
  width: 100%;
}

.bubble-text {
  background: rgba(0, 0, 0, 0.72);
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
  color: #f1f5f9;
  font-size: 13px;
  line-height: 1.5;
  padding: 8px 12px;
  border-radius: 10px;
  flex: 1;
  min-width: 0;
  word-break: break-word;
  white-space: pre-wrap;
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.markdown-body :deep(*) {
  box-sizing: border-box;
}

.markdown-body {
  white-space: normal;
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

.bubble-tail {
  width: 0;
  height: 0;
  border-top: 6px solid transparent;
  border-bottom: 6px solid transparent;
  border-left: 8px solid rgba(0, 0, 0, 0.72);
  margin-top: 10px;
  flex-shrink: 0;
}

.think-bubble {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: #f1f5f9;
}

.think-spinner {
  display: inline-block;
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.2);
  border-top-color: #93c5fd;
  border-radius: 50%;
  animation: think-spin 0.8s linear infinite;
}

@keyframes think-spin {
  to { transform: rotate(360deg); }
}
</style>
