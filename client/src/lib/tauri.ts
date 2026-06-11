import type { AppConfig } from "./types";

let wsClient: any = null;

function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function loadConfig(): Promise<AppConfig | null> {
  if (!isTauri()) {
    const raw = localStorage.getItem("kaya-beam-config");
    return raw ? JSON.parse(raw) : null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<AppConfig | null>("load_config");
  } catch {
    return null;
  }
}

export async function saveConfig(config: AppConfig): Promise<void> {
  if (!isTauri()) {
    localStorage.setItem("kaya-beam-config", JSON.stringify(config));
    return;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("save_config", { config });
}

/** 主动查询连接状态（用于启动时补漏） */
export async function getConnectionStatus(): Promise<string> {
  if (!isTauri()) {
    return "已连接";
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return await invoke<string>("get_connection_status");
}

export async function onConnectionStatus(
  cb: (status: string, lastHeartbeat?: string) => void,
): Promise<() => void> {
  if (!isTauri()) {
    const { WsClient } = await import("./ws-client");
    const cfg = loadCfg();
    if (!cfg) return () => {};
    cb("连接中...");
    wsClient = new WsClient(cfg, {
      onConnected: () => cb("已连接"),
      onDisconnected: () => cb("已断开"),
      onError: (msg) => cb("错误: " + msg),
      onHeartbeat: (time) => cb("已连接", time),
      onFileReceived: () => {},
    });
    wsClient.connect();
    return () => {
      wsClient?.destroy();
      wsClient = null;
    };
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<{ status: string; lastHeartbeat?: string }>("connection-status", (e) => {
    cb(e.payload.status, e.payload.lastHeartbeat);
  });
}

export async function onFileReceived(
  cb: (name: string, size: number, path: string) => void,
): Promise<() => void> {
  if (!isTauri()) {
    if (wsClient) {
      wsClient.cb.onFileReceived = cb;
    }
    return () => {};
  }
  const { listen } = await import("@tauri-apps/api/event");
  return listen<{ name: string; size: number; path: string }>("file-received", (e) => {
    cb(e.payload.name, e.payload.size, e.payload.path);
  });
}

function loadCfg(): AppConfig | null {
  const raw = localStorage.getItem("kaya-beam-config");
  return raw ? JSON.parse(raw) : null;
}
