<script setup lang="ts">
import { ref, nextTick, watch, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const emit = defineEmits<{
  (e: "close"): void;
}>();

const props = defineProps<{
  visible: boolean;
  mode: "single" | "continuous";
}>();

const question = ref("");
const status = ref<"input" | "sending" | "sent" | "monitoring" | "error">("input");
const statusMessage = ref("");
const inputRef = ref<HTMLInputElement | null>(null);

watch(
  () => props.visible,
  async (v: boolean) => {
    if (v) {
      question.value = "";
      status.value = "input";
      statusMessage.value = "";
      await nextTick();
      inputRef.value?.focus();
    }
  }
);

async function handleSubmit() {
  const q = question.value.trim();
  if (!q) return;

  status.value = props.mode === "continuous" ? "monitoring" : "sending";
  statusMessage.value = q;

  try {
    await invoke("execute_copilot", {
      question: q,
      mode: props.mode,
    });
    if (props.mode === "single") {
      status.value = "sent";
    }
    // Close handled by Rust-side copilot-close event after signal processes
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
    cancelCopilot();
  }
}

async function cancelCopilot() {
  try {
    await invoke("cancel_copilot");
  } catch { /* ignore */ }
  emit("close");
}

let unlistenCopilotStatus: (() => void) | undefined;
let unlistenCopilotClose: (() => void) | undefined;

onMounted(async () => {
  unlistenCopilotStatus = await listen<{ status: string }>(
    "copilot-status",
    (e: { payload: { status: string } }) => {
      if (e.payload.status === "sent" && props.mode === "single") {
        status.value = "sent";
      }
    }
  );
  unlistenCopilotClose = await listen("copilot-close", () => {
    emit("close");
  });
});

onUnmounted(() => {
  unlistenCopilotStatus?.();
  unlistenCopilotClose?.();
});
</script>

<template>
  <Teleport to="body">
    <Transition name="overlay">
      <div v-if="visible" class="copilot-backdrop" @click.self="cancelCopilot">
        <div class="copilot-container" :class="mode">
          <div class="copilot-header">
            <span class="copilot-mode-badge" :class="mode">
              {{ mode === "continuous" ? "🔴 持续" : "⚡ Copilot" }}
            </span>
            <button class="copilot-close" @click="cancelCopilot">✕</button>
          </div>

          <div v-if="status === 'input'" class="copilot-input-area">
            <input
              ref="inputRef"
              v-model="question"
              class="copilot-input"
              placeholder="问 Kaya 关于当前窗口的问题..."
              @keydown="handleKeydown"
            />
            <div class="copilot-hint">Enter 发送 · Esc 取消</div>
          </div>

          <div v-else class="copilot-status-area">
            <div class="copilot-question">{{ statusMessage }}</div>
            <div class="copilot-progress">
              <template v-if="status === 'sending'">
                <span class="spinner"></span>
                正在采集窗口信息并发送...
              </template>
              <template v-else-if="status === 'sent'">
                <span class="checkmark">✓</span>
                已发送给 Kaya，请在聊天窗口查看回复
              </template>
              <template v-else-if="status === 'monitoring'">
                <span class="pulse-dot"></span>
                持续监测中... Kaya 每 30 秒自动重新分析
                <button class="copilot-stop-btn" @click="cancelCopilot">停止</button>
              </template>
              <template v-else-if="status === 'error'">
                <span class="error-icon">✕</span>
                发送失败: {{ statusMessage }}
              </template>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.copilot-backdrop {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;
  background: rgba(0, 0, 0, 0.3);
  display: flex;
  justify-content: center;
  padding-top: 80px;
  z-index: 9999;
}

.copilot-container {
  width: 560px;
  max-width: 90vw;
  background: var(--color-surface, #ffffff);
  border-radius: 12px;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
  overflow: hidden;
  height: fit-content;
}

.copilot-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  border-bottom: 1px solid var(--color-border-light, #e2e8f0);
}

.copilot-mode-badge {
  font-size: 11px;
  font-weight: 600;
  padding: 3px 10px;
  border-radius: 10px;
}

.copilot-mode-badge.single {
  background: #e0f2fe;
  color: #0369a1;
}

.copilot-mode-badge.continuous {
  background: #fef3c7;
  color: #92400e;
}

.copilot-close {
  background: none;
  border: none;
  font-size: 14px;
  cursor: pointer;
  color: var(--color-text-muted, #94a3b8);
  padding: 4px 8px;
  border-radius: 4px;
}

.copilot-close:hover {
  background: var(--color-bg, #f8fafc);
}

.copilot-input-area {
  padding: 16px;
}

.copilot-input {
  width: 100%;
  padding: 12px 16px;
  font-size: 15px;
  border: 2px solid var(--color-primary, #6366f1);
  border-radius: 8px;
  outline: none;
  color: var(--color-text, #1e293b);
  background: var(--color-bg, #f8fafc);
  box-sizing: border-box;
}

.copilot-input:focus {
  border-color: var(--color-primary-hover, #4f46e5);
}

.copilot-hint {
  font-size: 11px;
  color: var(--color-text-muted, #94a3b8);
  margin-top: 6px;
  text-align: right;
}

.copilot-status-area {
  padding: 24px 16px;
  text-align: center;
}

.copilot-question {
  font-size: 14px;
  font-weight: 500;
  color: var(--color-text, #1e293b);
  margin-bottom: 12px;
  word-break: break-word;
}

.copilot-progress {
  font-size: 13px;
  color: var(--color-text-muted, #94a3b8);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
}

.copilot-stop-btn {
  margin-left: 12px;
  font-size: 11px;
  padding: 4px 12px;
  border: 1px solid #ef4444;
  border-radius: 4px;
  background: transparent;
  color: #ef4444;
  cursor: pointer;
}

.copilot-stop-btn:hover {
  background: #fef2f2;
}

.spinner {
  width: 14px;
  height: 14px;
  border: 2px solid var(--color-border, #e2e8f0);
  border-top-color: var(--color-primary, #6366f1);
  border-radius: 50%;
  animation: spin 0.6s linear infinite;
  display: inline-block;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.checkmark {
  color: #22c55e;
  font-weight: bold;
  font-size: 16px;
}

.error-icon {
  color: #ef4444;
  font-weight: bold;
}

.pulse-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #ef4444;
  animation: pulse 1.5s ease-in-out infinite;
  display: inline-block;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

.overlay-enter-active,
.overlay-leave-active {
  transition: opacity 0.2s ease;
}

.overlay-enter-from,
.overlay-leave-to {
  opacity: 0;
}
</style>
