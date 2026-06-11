<script setup lang="ts">
import { ref, nextTick, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

const text = ref("");
const bubbleRef = ref<HTMLDivElement | null>(null);

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

  // 取走完整内容（5s 去抖后已经累积完毕）
  const win = getCurrentWindow();
  try {
    text.value = await invoke<string>("take_bubble_content", { label: win.label });
    remeasure();
  } catch { /* no content */ }
});
</script>

<template>
  <div class="bubble-root">
    <div class="bubble-body">
      <div ref="bubbleRef" class="bubble-text">{{ text }}</div>
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

.bubble-tail {
  width: 0;
  height: 0;
  border-top: 6px solid transparent;
  border-bottom: 6px solid transparent;
  border-left: 8px solid rgba(0, 0, 0, 0.72);
  margin-top: 10px;
  flex-shrink: 0;
}
</style>
