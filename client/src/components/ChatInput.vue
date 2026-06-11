<script setup lang="ts">
import { ref } from "vue";

const props = defineProps<{
  disabled?: boolean;
}>();

const emit = defineEmits<{
  (e: "send", text: string): void;
}>();

const text = ref("");

function handleSend() {
  const msg = text.value.trim();
  if (!msg || props.disabled) return;
  emit("send", msg);
  text.value = "";
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    handleSend();
  }
}
</script>

<template>
  <div class="chat-input">
    <input
      v-model="text"
      class="input-field"
      :disabled="disabled"
      placeholder="给 Kaya 发消息..."
      @keydown="onKeydown"
    />
    <button
      class="send-button"
      :disabled="disabled || !text.trim()"
      @click="handleSend"
    >
      发送
    </button>
  </div>
</template>

<style scoped>
.chat-input {
  display: flex;
  gap: 8px;
  align-items: center;
}

.input-field {
  flex: 1;
  padding: 10px 14px;
  border: 1.5px solid var(--color-border);
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--color-text);
  background: var(--color-bg);
  transition: border-color 0.15s;
  outline: none;
}

.input-field:focus {
  border-color: var(--color-primary);
}

.input-field:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.send-button {
  background: var(--color-primary);
  color: #fff;
  border: none;
  border-radius: var(--radius-md);
  padding: 10px 20px;
  font-size: 13px;
  font-weight: 500;
  transition: background 0.15s;
  cursor: pointer;
}

.send-button:hover:not(:disabled) {
  background: var(--color-primary-hover);
}

.send-button:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
