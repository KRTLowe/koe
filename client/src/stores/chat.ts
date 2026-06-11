import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function chatLog(label: string, ...args: any[]) {
  console.log(`[ChatStore] ${label}`, ...args);
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

export const useChatStore = defineStore("chat", () => {
  const messages = ref<ChatMessage[]>([]);
  const connected = ref(false);
  const sessionReady = ref(false);
  const sessionId = ref<string | null>(null);
  const responding = ref(false);
  const error = ref<string | null>(null);
  const acpReady = computed(() => connected.value && sessionReady.value);
  let msgCounter = 0;
  let currentAssistantId: string | null = null;

  async function init() {
    chatLog("init: registering event listeners...");

    await listen<{ status: string }>("acp-status", (e) => {
      chatLog("acp-status event:", e.payload.status);
      if (e.payload.status === "已连接") {
        connected.value = true;
        error.value = null;
      } else if (e.payload.status.startsWith("错误")) {
        connected.value = false;
        error.value = e.payload.status;
        responding.value = false;
        currentAssistantId = null;
      } else {
        connected.value = false;
        sessionReady.value = false;
        currentAssistantId = null;
      }
    });

    await listen<{ sessionId: string }>("acp-session", (e) => {
      chatLog("acp-session event:", e.payload.sessionId);
      sessionReady.value = true;
      sessionId.value = e.payload.sessionId;
    });

    await listen<{ content: string }>("acp-message", (e) => {
      chatLog("acp-message event: content length=", e.payload.content.length);
      if (currentAssistantId) {
        const msg = messages.value.find(m => m.id === currentAssistantId);
        if (msg) {
          msg.content = e.payload.content;
          return;
        }
      }
      const id = `msg_${++msgCounter}`;
      currentAssistantId = id;
      messages.value.push({
        id,
        role: "assistant",
        content: e.payload.content,
        timestamp: Date.now(),
      });
    });

    await listen("acp-done", () => {
      chatLog("acp-done event");
      responding.value = false;
      currentAssistantId = null;
    });

    chatLog("init: invoking start_acp...");
    try {
      await invoke("start_acp");
      chatLog("init: start_acp succeeded");
    } catch (e) {
      chatLog("init: start_acp failed (may already be started):", e);
    }
  }

  async function sendMessage(text: string) {
    if (!text.trim()) return;
    chatLog("sendMessage: text=", text.substring(0, 80));
    messages.value.push({
      id: `msg_${++msgCounter}`,
      role: "user",
      content: text,
      timestamp: Date.now(),
    });
    responding.value = true;
    error.value = null;
    setTimeout(() => {
      if (responding.value) {
        chatLog("sendMessage: 120s timeout, forcing responding=false");
        responding.value = false;
      }
    }, 120000);

    try {
      await invoke("send_acp_message", { text });
      chatLog("sendMessage: IPC call succeeded");
    } catch (e: any) {
      chatLog("sendMessage: IPC failed:", String(e));
      error.value = String(e);
      responding.value = false;
    }
  }

  function clearConversation() {
    messages.value = [];
  }

  return {
    messages, connected, sessionReady, sessionId, acpReady, responding, error,
    init, sendMessage, clearConversation,
  };
});
