<script setup lang="ts">
import { onMounted, onUnmounted, ref } from "vue";
import { useRouter } from "vue-router";
import { useAppStore } from "./stores/app";
import { useFileStore } from "./stores/file";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { onConnectionStatus, onFileReceived, getConnectionStatus } from "./lib/tauri";
import AppLayout from "./components/AppLayout.vue";

const router = useRouter();
const appStore = useAppStore();
const fileStore = useFileStore();

const currentWindow = getCurrentWindow();
const isMainWindow = currentWindow.label === "main";
const isFloatWindow = currentWindow.label === "kaya-float";

const pollActive = ref(true);
let unlistenConnection: (() => void) | undefined;
let unlistenFile: (() => void) | undefined;
let pollTimer: ReturnType<typeof setInterval> | null = null;

onMounted(async () => {
  // 悬浮窗和 Copilot 窗口跳过初始化
  if (!isMainWindow) return;

  console.log("App.vue mounted, starting init...");
  pollTimer = setInterval(async () => {
    if (!pollActive.value) return;
    try {
      const s = await getConnectionStatus();
      if (s !== "未连接") {
        appStore.setConnected(s);
      }
    } catch {
      // IPC 调用失败不中断
    }
  }, 1000);

  try {
    unlistenConnection = await onConnectionStatus((status, heartbeat) => {
      appStore.setConnected(status, heartbeat);
      pollActive.value = false;
    });
  } catch (e) {
    console.error("Failed to set up connection listener", e);
  }

  try {
    unlistenFile = await onFileReceived((name, size, path) => {
      fileStore.show(name, size, path);
    });
  } catch {
    // 文件监听失败不影响主要功能
  }

  await listen("toggle-chat", () => {
    router.push("/chat");
  });

  // 加载配置并跳转到首页（用 replace 避免 /float 留在历史里）
  await appStore.load();
  await router.replace(appStore.config ? "/home" : "/settings");
});

onUnmounted(() => {
  if (!isMainWindow) return;
  pollActive.value = false;
  if (pollTimer) clearInterval(pollTimer);
  unlistenConnection?.();
  unlistenFile?.();
});
</script>

<template>
  <AppLayout v-if="isMainWindow" />
  <router-view v-else-if="isFloatWindow" />
  <router-view v-else />
</template>
