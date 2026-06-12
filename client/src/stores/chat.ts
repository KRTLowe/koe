import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { KayaSessionRecord } from "../lib/types";
import { ensureActiveKayaSession, loadKayaSessions, createKayaSession, loadChatMessages, sendChatMessage } from "../lib/tauri";

function chatLog(label: string, ...args: any[]) {
  console.log(`[ChatStore] ${label}`, ...args);
}

export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  timestamp: number;
}

export interface ConcurrencyState {
  activeReplySessionId: string | null;
}

export function canStartReplyInSession(
  sessionId: string,
  state: ConcurrencyState,
): boolean {
  return state.activeReplySessionId === null || state.activeReplySessionId === sessionId;
}

export const useChatStore = defineStore("chat", () => {
  const messages = ref<ChatMessage[]>([]);
  const connected = ref(false);
  const sessionReady = ref(false);
  const sessionId = ref<string | null>(null);
  const responding = ref(false);
  const error = ref<string | null>(null);
  const acpReady = computed(() => connected.value && sessionReady.value);

  const kayaSessions = ref<KayaSessionRecord[]>([]);
  const currentKayaSessionId = ref<string | null>(null);
  const currentAcpSessionId = ref<string | null>(null);
  const activeReplySessionId = ref<string | null>(null);

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

    await listen<{ sessionId: string }>("acp-session", async (e) => {
      chatLog("acp-session event:", e.payload.sessionId);
      sessionReady.value = true;
      sessionId.value = e.payload.sessionId;
      currentAcpSessionId.value = e.payload.sessionId;
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
      activeReplySessionId.value = null;
    });

    chatLog("init: ensuring active kaya session...");
    try {
      const session = await ensureActiveKayaSession();
      currentKayaSessionId.value = session.id;
      kayaSessions.value = await loadKayaSessions();
      const chatMessages = await loadChatMessages(session.id);
      messages.value = chatMessages.map((m) => ({
        id: m.id,
        role: m.role as "user" | "assistant" | "system",
        content: m.content,
        timestamp: new Date(m.created_at).getTime(),
      }));
    } catch (e) {
      chatLog("init: sessions load failed:", e);
    }

    chatLog("init: invoking start_acp...");
    try {
      await invoke("start_acp");
      chatLog("init: start_acp succeeded");
    } catch (e) {
      chatLog("init: start_acp failed (may already be started):", e);
    }
  }

  async function newSession() {
    chatLog("newSession: creating new kaya session...");
    try {
      const session = await createKayaSession();
      kayaSessions.value.unshift(session);
      currentKayaSessionId.value = session.id;
      messages.value = [];
      error.value = null;
    } catch (e) {
      chatLog("newSession: failed:", e);
      error.value = String(e);
    }
  }

  async function switchSession(id: string) {
    if (id === currentKayaSessionId.value) return;
    chatLog("switchSession:", id);
    currentKayaSessionId.value = id;
    messages.value = [];
    error.value = null;
    try {
      const chatMessages = await loadChatMessages(id);
      messages.value = chatMessages.map((m) => ({
        id: m.id,
        role: m.role as "user" | "assistant" | "system",
        content: m.content,
        timestamp: new Date(m.created_at).getTime(),
      }));
    } catch (e) {
      chatLog("switchSession: load messages failed:", e);
    }
  }

  async function sendMessage(text: string) {
    if (!text.trim()) return;

    const sessionIdForReply = currentKayaSessionId.value;
    if (!sessionIdForReply) {
      error.value = "请先选择或新建一个会话";
      return;
    }

    if (!canStartReplyInSession(sessionIdForReply, {
      activeReplySessionId: activeReplySessionId.value,
    })) {
      error.value = "已有会话正在回复，请等待完成后重试";
      return;
    }

    chatLog("sendMessage: text=", text.substring(0, 80));
    messages.value.push({
      id: `msg_${++msgCounter}`,
      role: "user",
      content: text,
      timestamp: Date.now(),
    });
    responding.value = true;
    activeReplySessionId.value = sessionIdForReply;
    error.value = null;
    setTimeout(() => {
      if (responding.value) {
        chatLog("sendMessage: 120s timeout, forcing responding=false");
        responding.value = false;
        activeReplySessionId.value = null;
      }
    }, 120000);

    try {
      const updatedSession = await sendChatMessage(text, sessionIdForReply);
      kayaSessions.value = kayaSessions.value.map((session) => (
        session.id === updatedSession.id ? updatedSession : session
      ));
      chatLog("sendMessage: unified IPC call succeeded");
    } catch (e: any) {
      chatLog("sendMessage: IPC failed:", String(e));
      error.value = String(e);
      responding.value = false;
      activeReplySessionId.value = null;
    }
  }

  function clearConversation() {
    messages.value = [];
  }

  return {
    messages, connected, sessionReady, sessionId, acpReady, responding, error,
    kayaSessions, currentKayaSessionId, currentAcpSessionId, activeReplySessionId,
    init, newSession, switchSession, sendMessage, clearConversation,
  };
});
