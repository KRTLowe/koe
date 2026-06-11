import { defineStore } from "pinia";
import { ref } from "vue";
import type { TransferRecord } from "../lib/types";

export const useFileStore = defineStore("file", () => {
  const fileName = ref<string | null>(null);
  const fileSize = ref(0);
  const filePath = ref<string | null>(null);
  const visible = ref(false);
  const history = ref<TransferRecord[]>([]);

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
  }

  function dismiss() {
    visible.value = false;
  }

  return { fileName, fileSize, filePath, visible, history, show, dismiss, addSent };
});
