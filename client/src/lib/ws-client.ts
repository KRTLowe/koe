import type { AppConfig } from "./types";

type WsEventCallback = {
  onConnected: () => void;
  onDisconnected: () => void;
  onError: (msg: string) => void;
  onHeartbeat: (time: string) => void;
  onFileReceived: (name: string, size: number, path: string) => void;
};

type FileReceiveState = {
  fileId: string;
  name: string;
  size: number;
  chunks: Blob[];
};

const MIME_MAP: Record<string, string> = {
  txt: "text/plain", html: "text/html", htm: "text/html",
  csv: "text/csv", xml: "text/xml", json: "application/json",
  md: "text/markdown", pdf: "application/pdf",
  png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg",
  gif: "image/gif", webp: "image/webp", svg: "image/svg+xml",
  mp4: "video/mp4", mp3: "audio/mpeg", zip: "application/zip",
};

function guessMime(name: string): string {
  const ext = name.split(".").pop()?.toLowerCase() || "";
  return MIME_MAP[ext] || "application/octet-stream";
}

export class WsClient {
  private ws: WebSocket | null = null;
  private config: AppConfig;
  cb: WsEventCallback;
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private retryDelay = 1;
  private destroyed = false;
  private fileState: FileReceiveState | null = null;

  constructor(config: AppConfig, cb: WsEventCallback) {
    this.config = config;
    this.cb = cb;
  }

  connect() {
    if (this.destroyed) return;
    try {
      this.ws = new WebSocket(this.config.serverUrl);
    } catch (e: any) {
      this.cb.onError("连接失败: " + e.message);
      this.scheduleReconnect();
      return;
    }

    this.ws.onopen = () => {
      this.retryDelay = 1;
      this.send({ type: "auth", client_id: this.config.clientId, passkey: this.config.passkey });
    };

    this.ws.onmessage = (event) => {
      if (typeof event.data === "string") {
        try {
          const msg = JSON.parse(event.data);
          switch (msg.type) {
            case "auth_result":
              if (msg.ok) {
                this.cb.onConnected();
                this.startHeartbeat();
              } else {
                this.cb.onError("认证失败: " + (msg.error || "unknown"));
              }
              break;
            case "pong":
              this.cb.onHeartbeat(new Date().toLocaleTimeString());
              break;
            case "file_meta":
              this.fileState = {
                fileId: msg.file_id || "",
                name: msg.name || "unknown",
                size: msg.size || 0,
                chunks: [],
              };
              break;
            case "file_end":
              if (this.fileState) {
                const blob = new Blob(this.fileState.chunks, { type: guessMime(this.fileState.name) });
                const url = URL.createObjectURL(blob);
                const a = document.createElement("a");
                a.href = url;
                a.download = this.fileState.name;
                document.body.appendChild(a);
                a.click();
                setTimeout(() => {
                  document.body.removeChild(a);
                  URL.revokeObjectURL(url);
                }, 1000);

                this.cb.onFileReceived(
                  this.fileState.name,
                  this.fileState.size,
                  `已下载: ${this.fileState.name}`,
                );
                this.fileState = null;
              }
              break;
          }
        } catch {
          // ignore
        }
      } else if (event.data instanceof Blob) {
        if (this.fileState) {
          this.fileState.chunks.push(event.data);
        }
      }
    };

    this.ws.onclose = () => {
      this.stopHeartbeat();
      this.cb.onDisconnected();
      this.scheduleReconnect();
    };

    this.ws.onerror = () => {
      this.cb.onError("WebSocket 连接错误");
    };
  }

  private send(data: any) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
    }
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      this.send({ type: "heartbeat" });
    }, 30000);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private scheduleReconnect() {
    if (this.destroyed) return;
    this.reconnectTimer = setTimeout(() => {
      this.retryDelay = Math.min(this.retryDelay * 2, 60);
      this.connect();
    }, this.retryDelay * 1000);
  }

  destroy() {
    this.destroyed = true;
    this.stopHeartbeat();
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    if (this.ws) {
      this.ws.onclose = null;
      this.ws.close();
      this.ws = null;
    }
  }
}
