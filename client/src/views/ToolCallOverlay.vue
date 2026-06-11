<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useRoute } from "vue-router";

const route = useRoute();
const toolName = ref((route.query.name as string) || "");
const status = ref<"running" | "done" | "error">(
  (route.query.status as "running" | "done" | "error") || "running"
);
const visible = ref(true);
let closeTimer: ReturnType<typeof setTimeout> | null = null;

onMounted(() => {
  document.body.style.margin = "0";
  document.body.style.padding = "0";
  document.body.style.overflow = "hidden";
  document.body.style.background = "transparent";

  if (status.value !== "running") {
    const delay = status.value === "done" ? 2000 : 3000;
    closeTimer = setTimeout(() => closeWindow(), delay);
  }
});

onUnmounted(() => {
  if (closeTimer) clearTimeout(closeTimer);
  document.body.style.margin = "";
  document.body.style.padding = "";
  document.body.style.overflow = "";
  document.body.style.background = "";
});

async function closeWindow() {
  try {
    await invoke("close_tool_call_overlay");
  } catch { /* ignore */ }
}
</script>

<template>
  <div class="overlay-root" :class="{ hidden: !visible }">
    <div class="card" :class="status">
      <span v-if="status === 'running'" class="spinner"></span>
      <span v-else-if="status === 'done'" class="icon done">✓</span>
      <span v-else class="icon error">⚠</span>
      <span class="label">
        <template v-if="status === 'running'">正在调用: {{ toolName }}</template>
        <template v-else-if="status === 'done'">{{ toolName }} 完成</template>
        <template v-else>{{ toolName }} 失败</template>
      </span>
    </div>
  </div>
</template>

<style scoped>
.overlay-root {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: flex-start;
  justify-content: flex-end;
  padding: 12px 16px;
  box-sizing: border-box;
  user-select: none;
  transition: opacity 0.3s;
}

.overlay-root.hidden {
  opacity: 0;
}

.card {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  background: rgba(15, 23, 42, 0.85);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border-radius: 10px;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.25);
}

.label {
  font-size: 13px;
  color: #f1f5f9;
  white-space: nowrap;
  line-height: 1.4;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid rgba(255, 255, 255, 0.2);
  border-top-color: #818cf8;
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
  flex-shrink: 0;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.icon {
  font-size: 14px;
  font-weight: bold;
  flex-shrink: 0;
}
.icon.done {
  color: #22c55e;
}
.icon.error {
  color: #f87171;
}
</style>
