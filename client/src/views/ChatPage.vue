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
  <div class="chat-layout">
    <!-- Left sidebar: session list -->
    <aside class="session-sidebar">
      <button class="new-session-btn" @click="chatStore.newSession()">+ 新建会话</button>
      <div class="session-list">
        <div
          v-for="session in chatStore.kayaSessions"
          :key="session.id"
          class="session-item"
          :class="{ active: session.id === chatStore.currentKayaSessionId }"
          @click="chatStore.switchSession(session.id)"
        >
          <span class="session-title">{{ session.title }}</span>
        </div>
      </div>
    </aside>

    <!-- Right main area -->
    <div class="chat-main">
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
  </div>
</template>

<style scoped>
.chat-layout {
  height: 100%;
  display: flex;
  flex-direction: row;
  background: var(--color-bg);
}

/* ---- Left sidebar ---- */
.session-sidebar {
  width: 200px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--color-surface);
  border-right: 1px solid var(--color-border);
}

.new-session-btn {
  margin: 12px;
  padding: 8px 12px;
  border: 1px dashed var(--color-border);
  border-radius: 8px;
  background: transparent;
  color: var(--color-text);
  font-size: 13px;
  cursor: pointer;
  transition: background 0.15s;
}

.new-session-btn:hover {
  background: var(--color-bg);
}

.session-list {
  flex: 1;
  overflow-y: auto;
  padding: 0 8px 8px;
}

.session-item {
  padding: 10px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
  margin-bottom: 2px;
}

.session-item:hover {
  background: var(--color-bg);
}

.session-item.active {
  background: var(--color-primary, #6366f1);
  color: #fff;
}

.session-title {
  font-size: 13px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  display: block;
}

/* ---- Right main area ---- */
.chat-main {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
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
  min-height: 0;
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
