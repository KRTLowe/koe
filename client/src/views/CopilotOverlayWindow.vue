<script setup lang="ts">
import { ref, nextTick, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useRoute } from "vue-router";

const route = useRoute();
const mode = ref((route.query.mode as string) || "single");
const question = ref("");
const status = ref<"input" | "countdown" | "sending" | "sent" | "monitoring" | "error">("input");
const statusMessage = ref("");
const countdownValue = ref(3);
const inputRef = ref<HTMLInputElement | null>(null);
let countdownTimer: ReturnType<typeof setInterval> | null = null;
let unlistenReset: (() => void) | undefined;

// 透明悬浮窗需要 body 透明
onMounted(async () => {
  document.body.style.margin = "0";
  document.body.style.padding = "0";
  document.body.style.overflow = "hidden";
  document.body.style.background = "transparent";

  nextTick(() => inputRef.value?.focus());

  // 监听重置事件（窗口重新显示时）
  unlistenReset = await listen<{ mode: string }>("copilot-reset", (event) => {
    mode.value = event.payload.mode || "single";
    question.value = "";
    status.value = "input";
    statusMessage.value = "";
    nextTick(() => inputRef.value?.focus());
  });
});

onUnmounted(() => {
  document.body.style.margin = "";
  document.body.style.padding = "";
  document.body.style.overflow = "";
  document.body.style.background = "";
  unlistenReset?.();
});

async function handleSubmit() {
  const q = question.value.trim();
  if (!q) return;

  statusMessage.value = q;
  status.value = "countdown";
  countdownValue.value = 3;

  countdownTimer = setInterval(() => {
    countdownValue.value--;
    if (countdownValue.value <= 0) {
      if (countdownTimer) clearInterval(countdownTimer);
      countdownTimer = null;
      doExecute(q);
    }
  }, 1000);
}

async function doExecute(q: string) {
  if (mode.value === "continuous") {
    status.value = "monitoring";
  } else {
    status.value = "sending";
  }

  try {
    await invoke("execute_copilot", { question: q, mode: mode.value });
    await invoke("copilot_enter_monitor");

    if (mode.value === "single") {
      status.value = "sent";
      setTimeout(() => dismissWindow(), 2000);
    }
  } catch (e: any) {
    status.value = "error";
    statusMessage.value = String(e);
  }
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    handleSubmit();
  }
  if (e.key === "Escape") {
    closeWindow();
  }
}

async function closeWindow() {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
  try {
    await invoke("send_acp_message", { text: "/cancel" });
  } catch { /* ignore */ }
  try {
    await invoke("cancel_copilot");
  } catch { /* ignore */ }
  try {
    await invoke("copilot_close");
  } catch { /* ignore */ }
}

async function dismissWindow() {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
  try {
    await invoke("copilot_close");
  } catch { /* ignore */ }
}
</script>

<template>
  <div class="overlay-root">
    <!-- 输入态 -->
    <div v-if="status === 'input'" class="card">
      <div class="card-header">
        <span :class="['mode-badge', mode]">
          kaya-is-watching-you
        </span>
        <button class="close-btn" @click="closeWindow">✕</button>
      </div>
      <div class="card-body">
        <input
          ref="inputRef"
          v-model="question"
          class="query-input"
          placeholder="问 Kaya 关于当前窗口的问题..."
          @keydown="handleKeydown"
        />
        <div class="hint">Enter 发送 · Esc 取消</div>
      </div>
    </div>

    <!-- 倒计时 3-2-1 -->
    <div v-else-if="status === 'countdown'" class="card countdown-card">
      <div class="countdown-number">{{ countdownValue }}</div>
      <div class="countdown-label">即将采集当前窗口信息...</div>
    </div>

    <!-- 发送中 -->
    <div v-else-if="status === 'sending'" class="card monitor-card">
      <span class="spinner"></span>
      <span class="monitor-text">正在采集窗口信息并发送...</span>
    </div>

    <!-- 已发送（单次） -->
    <div v-else-if="status === 'sent'" class="card monitor-card">
      <span class="checkmark">✓</span>
      <span class="monitor-text">已发送</span>
    </div>

    <!-- 监测中（持续） -->
    <div v-else-if="status === 'monitoring'" class="card monitor-card">
      <span class="pulse-dot"></span>
      <span class="monitor-text">监测中</span>
      <button class="stop-btn" @click="closeWindow">停止</button>
    </div>

    <!-- 错误 -->
    <div v-else-if="status === 'error'" class="card monitor-card">
      <span class="error-icon">✕</span>
      <span class="monitor-text error-text">{{ statusMessage }}</span>
    </div>
  </div>
</template>

<style scoped>
.overlay-root {
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 12px;
  user-select: none;
}

.card {
  background: rgba(255, 255, 255, 0.92);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.5);
  border-radius: 12px;
  box-shadow:
    0 8px 32px rgba(0, 0, 0, 0.18),
    inset 0 1px 0 rgba(255, 255, 255, 0.6);
  overflow: hidden;
  width: 100%;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 6px 10px;
  border-bottom: 1px solid rgba(0, 0, 0, 0.06);
}

.mode-badge {
  font-size: 10px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 8px;
}
.mode-badge.single {
  background: rgba(99, 102, 241, 0.12);
  color: #4f46e5;
}
.mode-badge.continuous {
  background: rgba(251, 191, 36, 0.15);
  color: #92400e;
}

.close-btn {
  background: none;
  border: none;
  font-size: 13px;
  cursor: pointer;
  color: #94a3b8;
  padding: 2px 6px;
  border-radius: 4px;
  line-height: 1;
}
.close-btn:hover {
  background: rgba(0, 0, 0, 0.06);
}

.card-body {
  padding: 10px 14px 14px;
}

.query-input {
  width: 100%;
  padding: 12px 16px;
  font-size: 15px;
  border: 1.5px solid var(--color-primary, #6366f1);
  border-radius: 8px;
  outline: none;
  color: var(--color-text, #1e293b);
  background: rgba(248, 250, 252, 0.8);
  box-sizing: border-box;
}
.query-input:focus {
  border-color: var(--color-primary-hover, #4f46e5);
}

.hint {
  font-size: 10px;
  color: #94a3b8;
  margin-top: 4px;
  text-align: right;
}

/* ── 倒计时（3-2-1） ── */

.countdown-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 20px 14px;
  height: 100%;
}

.countdown-number {
  font-size: 36px;
  font-weight: 700;
  color: var(--color-primary, #6366f1);
  line-height: 1;
}

.countdown-label {
  font-size: 11px;
  color: #94a3b8;
}

/* ── 监测态（缩到右上角后的紧凑布局） ── */

.monitor-card {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 10px 14px;
  height: 100%;
}

.monitor-text {
  font-size: 12px;
  color: #475569;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.spinner {
  width: 12px;
  height: 12px;
  border: 2px solid #e2e8f0;
  border-top-color: var(--color-primary, #6366f1);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
  flex-shrink: 0;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.checkmark {
  color: #22c55e;
  font-weight: bold;
  font-size: 14px;
  flex-shrink: 0;
}

.error-icon {
  color: #ef4444;
  font-weight: bold;
  flex-shrink: 0;
}

.error-text {
  color: #ef4444;
  font-size: 11px;
}

.pulse-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #ef4444;
  animation: pulse 1.5s ease-in-out infinite;
  flex-shrink: 0;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.stop-btn {
  font-size: 10px;
  padding: 2px 10px;
  border: 1px solid #ef4444;
  border-radius: 4px;
  background: transparent;
  color: #ef4444;
  cursor: pointer;
  flex-shrink: 0;
  margin-left: auto;
}
.stop-btn:hover {
  background: #fef2f2;
}
</style>
