<script setup lang="ts">
import { ref } from "vue";
import { useFileStore } from "../stores/file";
import { invoke } from "@tauri-apps/api/core";
import type { TransferRecord } from "../lib/types";

const fileStore = useFileStore();
const fileInput = ref<HTMLInputElement | null>(null);
const uploading = ref(false);

function openFile(record: TransferRecord) {
  if (!record.path) return;
  invoke("open_file", { path: record.path }).catch((e) => {
    console.error("Failed to open file:", e);
  });
}

function triggerFilePick() {
  fileInput.value?.click();
}

async function onFileSelected(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;

  uploading.value = true;
  try {
    const buf = await file.arrayBuffer();
    const data = Array.from(new Uint8Array(buf));
    await invoke("upload_file_data", { name: file.name, data });
    fileStore.addSent(file.name, file.size);
  } catch (e) {
    console.error("Upload failed:", e);
  } finally {
    uploading.value = false;
    input.value = ""; // reset so same file can be re-selected
  }
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return bytes + " B";
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + " KB";
  return (bytes / (1024 * 1024)).toFixed(1) + " MB";
}

function formatTime(ts: number): string {
  const d = new Date(ts);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  if (isToday) {
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
  return (
    d.toLocaleDateString([], { month: "short", day: "numeric" }) +
    " " +
    d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
  );
}

function fileIcon(name: string): string {
  if (/\.(png|jpg|jpeg|gif|webp|bmp|svg)$/i.test(name)) return "🖼️";
  if (/\.(zip|tar|gz|7z|rar)$/i.test(name)) return "📦";
  if (/\.(doc|docx|pdf|txt)$/i.test(name)) return "📄";
  if (/\.(mp4|avi|mov|mkv)$/i.test(name)) return "🎬";
  return "📄";
}

const history = fileStore.history ?? [];
</script>

<template>
  <div class="file-transfer">
    <div class="page-header">
      <h2 class="page-title">文件传输</h2>
      <p class="page-subtitle">文件收发记录</p>
    </div>

    <div class="message-area">
      <div v-if="history.length === 0" class="empty-state">
        <div class="empty-icon">📂</div>
        <p>暂无文件传输记录</p>
        <p class="empty-hint">Kaya 发送的文件将显示在这里</p>
      </div>

      <div
        v-for="record in history"
        :key="record.id"
        class="bubble-row"
        :class="{ sent: record.direction === 'sent' }"
      >
        <div class="avatar" :class="{ sent: record.direction === 'sent' }">
          {{ record.direction === "sent" ? "我" : "K" }}
        </div>
        <div
          class="bubble"
          :class="{ sent: record.direction === 'sent', clickable: !!record.path }"
          @click="openFile(record)"
        >
          <div class="file-info">
            <span class="file-icon">{{ fileIcon(record.name) }}</span>
            <div class="file-detail">
              <div class="file-name">{{ record.name }}</div>
              <div class="file-meta">
                {{ formatSize(record.size) }}
                <span v-if="record.status === 'ok'" class="status-ok">✓ 已接收</span>
                <span v-else class="status-error">✗ 失败</span>
              </div>
            </div>
          </div>
          <div class="bubble-time">{{ formatTime(record.timestamp) }}</div>
        </div>
      </div>
    </div>

    <!-- Upload -->
    <div class="send-area">
      <div class="send-box">
        <input
          ref="fileInput"
          type="file"
          class="file-input-hidden"
          @change="onFileSelected"
        />
        <div class="send-input-placeholder" @click="triggerFilePick">
          📎 {{ uploading ? "上传中..." : "选择文件或点击此处上传" }}
        </div>
        <button class="send-btn" :disabled="uploading" @click="triggerFilePick">
          {{ uploading ? "上传中" : "选择文件" }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.file-transfer {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--color-bg);
}

.page-header {
  padding: 24px 32px 16px;
  border-bottom: 1px solid var(--color-border);
  background: var(--color-surface);
  flex-shrink: 0;
}

.page-title {
  font-size: 18px;
  font-weight: 600;
  color: var(--color-text);
}

.page-subtitle {
  font-size: 13px;
  color: var(--color-text-muted);
  margin-top: 2px;
}

.message-area {
  flex: 1;
  overflow-y: auto;
  padding: 16px 32px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-height: 0;
}

.empty-state {
  text-align: center;
  padding: 48px 20px;
  color: var(--color-text-muted);
}

.empty-icon {
  font-size: 48px;
  margin-bottom: 12px;
}

.empty-hint {
  font-size: 12px;
  color: var(--color-text-light);
  margin-top: 4px;
}

.bubble-row {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.bubble-row.sent {
  flex-direction: row-reverse;
}

.avatar {
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

.avatar.sent {
  background: var(--color-success);
}

.bubble {
  background: var(--color-surface);
  border-radius: 12px 12px 12px 4px;
  padding: 10px 14px;
  max-width: 65%;
  box-shadow: var(--shadow-bubble);
}

.bubble.sent {
  background: var(--color-primary);
  color: #fff;
  border-radius: 12px 12px 4px 12px;
}

.bubble.clickable {
  cursor: pointer;
  transition: filter 0.15s;
}

.bubble.clickable:hover {
  filter: brightness(0.97);
}

.bubble.sent.clickable:hover {
  filter: brightness(1.1);
}

.file-info {
  display: flex;
  align-items: center;
  gap: 10px;
}

.file-icon {
  font-size: 24px;
}

.file-name {
  font-size: 13px;
  font-weight: 500;
}

.bubble.sent .file-name {
  color: #fff;
}

.file-meta {
  font-size: 11px;
  color: var(--color-text-muted);
  margin-top: 2px;
  display: flex;
  align-items: center;
  gap: 6px;
}

.bubble.sent .file-meta {
  color: rgba(255, 255, 255, 0.7);
}

.status-ok {
  color: var(--color-success);
}

.bubble.sent .status-ok {
  color: rgba(255, 255, 255, 0.8);
}

.status-error {
  color: var(--color-error);
}

.bubble-time {
  font-size: 10px;
  color: var(--color-text-light);
  margin-top: 6px;
  text-align: right;
}

.bubble.sent .bubble-time {
  color: rgba(255, 255, 255, 0.5);
}

.send-area {
  padding: 12px 32px 16px;
  border-top: 1px solid var(--color-border);
  background: var(--color-surface);
  flex-shrink: 0;
}

.send-box {
  display: flex;
  gap: 8px;
  align-items: center;
}

.file-input-hidden {
  display: none;
}

.send-input-placeholder {
  flex: 1;
  padding: 10px 14px;
  border: 1.5px dashed var(--color-border);
  border-radius: var(--radius-md);
  font-size: 13px;
  color: var(--color-text-light);
  background: var(--color-bg);
  cursor: pointer;
  transition: border-color 0.15s;
}

.send-input-placeholder:hover {
  border-color: var(--color-primary);
}

.send-btn {
  background: var(--color-primary);
  color: #fff;
  border: none;
  border-radius: var(--radius-md);
  padding: 10px 20px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s;
}

.send-btn:hover:not(:disabled) {
  background: var(--color-primary-hover);
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
