<script setup lang="ts">
import { computed } from "vue";
import { useAppStore } from "../stores/app";

const appStore = useAppStore();

const indicatorClass = computed(() => {
  if (appStore.error) return "indicator error";
  if (appStore.connected) return "indicator connected";
  if (appStore.connecting) return "indicator connecting";
  return "indicator disconnected";
});

const statusText = computed(() => {
  if (appStore.error) return "连接错误";
  if (appStore.connected) return "已连接";
  if (appStore.connecting) return "正在连接…";
  return "未连接";
});
</script>

<template>
  <div class="status-bar">
    <span :class="indicatorClass" />
    <span>{{ statusText }}</span>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.9rem;
}

.indicator {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  display: inline-block;
}

.connected {
  background-color: #4caf50;
}

.connecting {
  background-color: #ffc107;
  animation: pulse 1.2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.4; }
}

.disconnected {
  background-color: #9e9e9e;
}

.error {
  background-color: #d32f2f;
}
</style>
