import { defineStore } from "pinia";
import { ref } from "vue";
import type { AppConfig } from "../lib/types";
import { loadConfig as loadCfg, saveConfig as saveCfg } from "../lib/tauri";

export const useAppStore = defineStore("app", () => {
  const config = ref<AppConfig | null>(null);
  const connected = ref(false);
  const connecting = ref(false);
  const lastHeartbeat = ref("");
  const error = ref<string | null>(null);
  const loading = ref(true);

  async function load() {
    loading.value = true;
    try {
      const cfg = await loadCfg();
      if (cfg) {
        if (!cfg.storagePath) cfg.storagePath = "~/kaya-transfer/";
      }
      config.value = cfg;
    } catch {
      config.value = null;
    } finally {
      loading.value = false;
    }
  }

  async function save(cfg: AppConfig) {
    await saveCfg(cfg);
    config.value = cfg;
  }

  function setConnected(status: string, heartbeat?: string) {
    if (status === "已连接") {
      connected.value = true;
      connecting.value = false;
      error.value = null;
    } else if (status.startsWith("错误")) {
      connected.value = false;
      connecting.value = false;
      error.value = status;
    } else if (status === "连接中...") {
      connecting.value = true;
    } else {
      connected.value = false;
      connecting.value = false;
      // "已断开" 之类的状态不覆盖 error，保留之前的错误信息
    }
    if (heartbeat) lastHeartbeat.value = heartbeat;
  }

  function setError(msg: string) {
    error.value = msg;
    connected.value = false;
  }

  return {
    config, connected, connecting, lastHeartbeat, error, loading,
    load, save, setConnected, setError,
  };
});
