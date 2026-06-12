import { defineStore } from "pinia";
import { ref } from "vue";
import type { TransferRecord } from "../lib/types";
import { appendFileTransferRecord, loadFileTransferHistory } from "../lib/tauri";

export function normalizeFileHistory(records: TransferRecord[]): TransferRecord[] {
  return [...records].sort((a, b) => b.timestamp - a.timestamp);
}

export const useFileStore = defineStore("file", () => {
  const fileName = ref<string | null>(null);
  const fileSize = ref(0);
  const filePath = ref<string | null>(null);
  const visible = ref(false);
  const history = ref<TransferRecord[]>([]);
  const loaded = ref(false);

  async function init() {
    if (loaded.value) return;
    loaded.value = true;
    try {
      const records = await loadFileTransferHistory();
      history.value = records.reverse();
    } catch (e) {
      console.error("Failed to load file history:", e);
    }
  }

  function show(name: string, size: number, path: string) {
    fileName.value = name;
    fileSize.value = size;
    filePath.value = path;
    visible.value = true;

    history.value.push({
      id: `f_${Date.now()}`,
      name,
      size,
      path,
      direction: "received",
      timestamp: Date.now(),
      status: "ok",
    });
  }

  function addSent(name: string, size: number) {
    history.value.push({
      id: `f_${Date.now()}`,
      name,
      size,
      path: undefined,
      direction: "sent",
      timestamp: Date.now(),
      status: "ok",
    });
    appendFileTransferRecord(name, size, "sent", "ok").catch((e) =>
      console.error("Failed to persist sent file:", e),
    );
  }

  function dismiss() {
    visible.value = false;
  }

  return { fileName, fileSize, filePath, visible, history, loaded, init, show, dismiss, addSent };
});
