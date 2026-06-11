<script setup lang="ts">
import { ref, nextTick, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

const input = ref("");
const inputRef = ref<HTMLInputElement | null>(null);

onMounted(() => {
  document.body.style.margin = "0";
  document.body.style.padding = "0";
  document.body.style.overflow = "hidden";
  document.body.style.background = "transparent";
  nextTick(() => inputRef.value?.focus());
});

async function sendMessage() {
  const text = input.value.trim();
  if (!text) return;
  try {
    await invoke("send_acp_message", { text });
  } catch { /* ignore */ }
  closeWindow();
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    sendMessage();
  }
  if (e.key === "Escape") {
    closeWindow();
  }
}

async function closeWindow() {
  try {
    await invoke("quick_chat_close");
  } catch { /* ignore */ }
}
</script>

<template>
  <div class="qc-root">
    <div class="qc-card">
      <div class="qc-title">kaya-is-listen-to-you</div>
      <input
        ref="inputRef"
        v-model="input"
        class="qc-input"
        placeholder="给 Kaya 发消息..."
        @keydown="handleKeydown"
      />
      <div class="qc-hint">Enter 发送 · Esc 取消</div>
    </div>
  </div>
</template>

<style scoped>
.qc-root {
  width: 100vw;
  height: 100vh;
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 20px;
  box-sizing: border-box;
  user-select: none;
}

.qc-card {
  background: rgba(255, 255, 255, 0.92);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.5);
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
  overflow: hidden;
  width: 100%;
  height: fit-content;
}

.qc-title {
  font-size: 10px;
  font-weight: 500;
  color: var(--color-text-muted, #94a3b8);
  padding: 4px 12px 0;
  text-align: center;
  letter-spacing: 0.3px;
  line-height: 1.3;
}

.qc-input {
  width: 100%;
  padding: 8px 14px;
  font-size: 13px;
  border: 1.5px solid var(--color-primary, #6366f1);
  border-radius: 0;
  outline: none;
  color: var(--color-text, #1e293b);
  background: transparent;
  box-sizing: border-box;
}
.qc-input:focus {
  border-color: var(--color-primary-hover, #4f46e5);
}

.qc-hint {
  font-size: 9px;
  color: #94a3b8;
  padding: 2px 12px 6px;
  text-align: right;
}
</style>
