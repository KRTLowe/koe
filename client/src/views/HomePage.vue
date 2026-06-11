<script setup lang="ts">
import { onMounted } from "vue";
import { useAppStore } from "../stores/app";
import { useFileStore } from "../stores/file";

const appStore = useAppStore();
const fileStore = useFileStore();

onMounted(() => {
  if (!appStore.config) {
    appStore.load();
  }
});

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}
</script>

<template>
  <div class="home">
    <h2 class="page-title">Home</h2>
    <p class="page-subtitle">服务连接与传输概览</p>

    <div class="status-cards">
      <div class="card">
        <div class="card-label">连接状态</div>
        <div class="card-value">
          <span
            class="status-dot"
            :class="{ connected: appStore.connected }"
          ></span>
          {{ appStore.connected ? "已连接" : "已断开" }}
        </div>
        <div v-if="appStore.error" class="card-error">{{ appStore.error }}</div>
      </div>
      <div class="card">
        <div class="card-label">存储路径</div>
        <div class="card-value mono">{{ appStore.config?.storagePath || "~/kaya-transfer/" }}</div>
      </div>
    </div>

    <div class="section-card">
      <div class="section-header">最近传输</div>
      <div v-if="fileStore.history.length === 0" class="empty-state">
        暂无传输记录
      </div>
      <div
        v-for="record in [...fileStore.history].reverse().slice(0, 10)"
        :key="record.id"
        class="transfer-row"
      >
        <span class="transfer-name">{{ record.name }}</span>
        <span class="transfer-size">{{ formatSize(record.size) }}</span>
        <span class="transfer-time">{{ new Date(record.timestamp).toLocaleTimeString() }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.home {
  padding: 24px 32px;
  max-width: 800px;
}

.page-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
  margin-bottom: 4px;
}

.page-subtitle {
  font-size: 13px;
  color: var(--color-text-muted);
  margin-bottom: 24px;
}

.status-cards {
  display: flex;
  gap: 16px;
  margin-bottom: 24px;
}

.card {
  flex: 1;
  background: var(--color-surface);
  border-radius: var(--radius-lg);
  padding: 16px 20px;
  box-shadow: var(--shadow-card);
}

.card-label {
  font-size: 12px;
  color: var(--color-text-muted);
  margin-bottom: 6px;
}

.card-value {
  font-size: 14px;
  font-weight: 500;
  color: var(--color-text);
  display: flex;
  align-items: center;
  gap: 6px;
}

.card-value.mono {
  font-family: var(--font-mono);
  font-size: 13px;
}

.card-error {
  font-size: 12px;
  color: var(--color-error);
  margin-top: 4px;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--color-text-light);
  display: inline-block;
}

.status-dot.connected {
  background: var(--color-success);
}

.section-card {
  background: var(--color-surface);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-card);
  overflow: hidden;
}

.section-header {
  padding: 16px 20px;
  border-bottom: 1px solid var(--color-border-light);
  font-size: 13px;
  font-weight: 500;
  color: var(--color-text);
}

.empty-state {
  padding: 32px 20px;
  text-align: center;
  color: var(--color-text-light);
  font-size: 13px;
}

.transfer-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px 20px;
  border-bottom: 1px solid #F8F8FB;
  font-size: 13px;
}

.transfer-row:last-child {
  border-bottom: none;
}

.transfer-name {
  color: var(--color-text-secondary);
  flex: 1;
}

.transfer-size {
  color: var(--color-text-muted);
  width: 80px;
  text-align: right;
}

.transfer-time {
  color: var(--color-text-light);
  width: 70px;
  text-align: right;
}
</style>
