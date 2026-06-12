import type { AppConfig, FileTransferRecord, KayaSessionRecord, ChatMessageRecord, TransferRecord } from "./types";

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

export async function loadKayaSessions(): Promise<KayaSessionRecord[]> {
  if (!isTauri()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<KayaSessionRecord[]>("load_kaya_sessions");
}

export async function loadLatestKayaSession(): Promise<KayaSessionRecord | null> {
  if (!isTauri()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<KayaSessionRecord | null>("load_latest_kaya_session");
}

export async function createKayaSession(): Promise<KayaSessionRecord> {
  if (!isTauri()) {
    const now = new Date().toISOString();
    return { id: `local_${Date.now()}`, title: "新会话", created_at: now, updated_at: now, is_active: true };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<KayaSessionRecord>("create_kaya_session");
}

export async function ensureActiveKayaSession(): Promise<KayaSessionRecord> {
  if (!isTauri()) {
    const now = new Date().toISOString();
    return { id: `local_${Date.now()}`, title: "新会话", created_at: now, updated_at: now, is_active: true };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<KayaSessionRecord>("ensure_active_kaya_session");
}

export async function loadChatMessages(kayaSessionId: string): Promise<ChatMessageRecord[]> {
  if (!isTauri()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<ChatMessageRecord[]>("load_chat_messages", { kayaSessionId });
}

export async function createOrSwitchAcpSession(kayaSessionId: string, remoteSessionId: string): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("create_or_switch_acp_session", { kayaSessionId, remoteSessionId });
}

export async function sendChatMessage(
  text: string,
  kayaSessionId: string | null,
): Promise<KayaSessionRecord> {
  if (!isTauri()) {
    const now = new Date().toISOString();
    return {
      id: kayaSessionId ?? `local_${Date.now()}`,
      title: text.trim(),
      created_at: now,
      updated_at: now,
      is_active: true,
    };
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<KayaSessionRecord>("send_chat_message", {
    text,
    kayaSessionId,
  });
}

export async function loadFileTransferHistory(): Promise<TransferRecord[]> {
  if (!isTauri()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  const records = await invoke<FileTransferRecord[]>("load_file_transfer_history");
  return records.map((r) => ({
    id: r.id,
    name: r.file_name,
    size: r.file_size,
    direction: r.direction as "received" | "sent",
    timestamp: new Date(r.created_at).getTime(),
    status: r.status as "ok" | "error",
    path: r.file_path ?? undefined,
  }));
}

export async function appendFileTransferRecord(
  fileName: string,
  fileSize: number,
  direction: string,
  status: string,
  filePath?: string,
): Promise<void> {
  if (!isTauri()) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("append_file_transfer_record", {
    fileName,
    fileSize,
    direction,
    status,
    filePath: filePath ?? null,
  });
}
