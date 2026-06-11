<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  role: "user" | "assistant" | "system";
  content: string;
}>();

// 从内容中解析 think 块
const parsed = computed(() => {
  const c = props.content;
  // 匹配 <think>...</think> 包裹
  const thinkMatch = c.match(/^<think>([\s\S]*?)<\/think>\n?([\s\S]*)$/);
  if (thinkMatch) {
    return { think: thinkMatch[1].trim(), display: thinkMatch[2] };
  }
  // 匹配 ```think ...``` 代码块
  const codeMatch = c.match(/^```think\n?([\s\S]*?)```\n?([\s\S]*)$/);
  if (codeMatch) {
    return { think: codeMatch[1].trim(), display: codeMatch[2] };
  }
  return { think: null, display: c };
});
</script>

<template>
  <div class="message-row" :class="{ user: role === 'user' }">
    <div class="msg-avatar" :class="{ user: role === 'user' }">
      {{ role === "user" ? "我" : "K" }}
    </div>
    <div class="msg-body" :class="{ user: role === 'user' }">
      <!-- Thinking bubble (collapsible) -->
      <details v-if="parsed.think" class="think-bubble">
        <summary class="think-summary">🧠 思考过程</summary>
        <div class="think-content">{{ parsed.think }}</div>
      </details>
      <!-- Main content -->
      <div class="msg-bubble" :class="{ user: role === 'user' }">
        <div class="msg-content">{{ parsed.display }}</div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.message-row {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.message-row.user {
  flex-direction: row-reverse;
}

.msg-avatar {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-weight: 500;
  flex-shrink: 0;
  background: var(--color-primary);
  color: #fff;
}

.msg-avatar.user {
  background: var(--color-text);
}

.msg-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-width: 75%;
}

.msg-body.user {
  align-items: flex-end;
}

/* Thinking bubble — 折叠式 */
.think-bubble {
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 8px;
  padding: 0;
  font-size: 12px;
  overflow: hidden;
}

.think-bubble[open] {
  padding-bottom: 8px;
}

.think-summary {
  padding: 6px 10px;
  cursor: pointer;
  font-weight: 500;
  color: var(--color-text-muted);
  user-select: none;
  list-style: none;
  display: flex;
  align-items: center;
  gap: 4px;
}

.think-summary::-webkit-details-marker {
  display: none;
}

.think-summary::before {
  content: "▶";
  font-size: 10px;
  transition: transform 0.15s;
}

.think-bubble[open] > .think-summary::before {
  content: "▼";
}

.think-content {
  padding: 0 10px 4px;
  color: var(--color-text-muted);
  line-height: 1.5;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 300px;
  overflow-y: auto;
}

.msg-bubble {
  background: var(--color-surface);
  border-radius: 12px 12px 12px 4px;
  padding: 10px 14px;
  box-shadow: var(--shadow-bubble);
}

.msg-bubble.user {
  background: var(--color-primary);
  color: #fff;
  border-radius: 12px 12px 4px 12px;
}

.msg-content {
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
}
</style>
