<script setup lang="ts">
import { onMounted, ref, nextTick, watch } from "vue";
import { useChatStore } from "../stores/chat";
import ChatMessage from "../components/ChatMessage.vue";
import ChatInput from "../components/ChatInput.vue";

const chatStore = useChatStore();
const messagesRef = ref<HTMLElement | null>(null);

onMounted(async () => {
  await chatStore.init();
});

watch(
  () => chatStore.messages.length,
  async () => {
    await nextTick();
    if (messagesRef.value) {
      messagesRef.value.scrollTop = messagesRef.value.scrollHeight;
    }
  }
);
</script>

<template>
  <div class="chat-page">
    <!-- Header -->
    <div class="chat-header">
      <span class="status-indicator" :class="{ ready: chatStore.acpReady, connected: chatStore.connected && !chatStore.acpReady }"></span>
      <span class="chat-partner">Kaya</span>
      <span class="chat-status">
        {{ chatStore.acpReady ? "在线" : chatStore.connected ? "等待会话…" : "已断开" }}
      </span>
    </div>

    <!-- Messages area: fixed height via flex, scrollable -->
    <div ref="messagesRef" class="messages-area">
      <ChatMessage
        v-for="msg in chatStore.messages"
        :key="msg.id"
        :role="msg.role"
        :content="msg.content"
      />
      <div v-if="chatStore.responding" class="typing-indicator">
        <span class="typing-dot"></span>
        <span class="typing-dot"></span>
        <span class="typing-dot"></span>
      </div>
    </div>

    <!-- Input -->
    <div class="chat-input-area">
      <ChatInput
        :disabled="!chatStore.acpReady"
        @send="(text: string) => chatStore.sendMessage(text)"
      />
    </div>
  </div>
</template>

<style scoped>
.chat-page {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--color-bg);
}

.chat-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px 24px;
  border-bottom: 1px solid var(--color-border);
  background: var(--color-surface);
  flex-shrink: 0;
}

.status-indicator {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-text-light);
  transition: background 0.3s;
}

.status-indicator.connected {
  background: var(--color-warning, #f59e0b);
}

.status-indicator.ready {
  background: var(--color-success);
}

.chat-partner {
  font-size: 14px;
  font-weight: 600;
  color: var(--color-text);
}

.chat-status {
  font-size: 11px;
  color: var(--color-text-muted);
}

.messages-area {
  flex: 1;
  overflow-y: auto;
  padding: 16px 24px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0; /* crucial for flex overflow */
}

.typing-indicator {
  display: flex;
  gap: 4px;
  padding: 12px 16px;
  background: var(--color-surface);
  border-radius: 12px 12px 12px 4px;
  align-self: flex-start;
  box-shadow: var(--shadow-bubble);
}

.typing-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--color-text-light);
  animation: typing 1.4s infinite;
}

.typing-dot:nth-child(2) {
  animation-delay: 0.2s;
}

.typing-dot:nth-child(3) {
  animation-delay: 0.4s;
}

@keyframes typing {
  0%, 60%, 100% { opacity: 0.3; }
  30% { opacity: 1; }
}

.chat-input-area {
  flex-shrink: 0;
  padding: 12px 24px 16px;
  border-top: 1px solid var(--color-border);
  background: var(--color-surface);
}
</style>
