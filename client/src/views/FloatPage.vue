<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";

const IMAGE_MAP: Record<string, { file: string; w: number; h: number }> = {
  "kaya-float": { file: "/kaya-float.png", w: 320, h: 320 },
  "kaya-full": { file: "/kaya-full.png", w: 937, h: 1678 },
};

const MAX_W = 320;
const MAX_H = 500;

const imageSrc = ref("/kaya-float.png");
const rootW = ref(320);
const rootH = ref(320);

onMounted(async () => {
  document.body.style.margin = "0";
  document.body.style.padding = "0";
  document.body.style.overflow = "hidden";
  document.body.style.background = "transparent";

  // 读取配置，确定用哪张图
  let imgKey = "kaya-float";
  try {
    const cfg = await invoke<Record<string, any>>("load_config");
    imgKey = cfg?.floatImage || "kaya-float";
  } catch { /* 默认用 kaya-float */ }

  const info = IMAGE_MAP[imgKey] || IMAGE_MAP["kaya-float"];
  imageSrc.value = info.file;

  // 计算显示尺寸（保持比例）
  const scale = Math.min(MAX_W / info.w, MAX_H / info.h, 1);
  rootW.value = Math.round(info.w * scale);
  rootH.value = Math.round(info.h * scale);

  // 调整窗口大小
  try {
    await invoke("resize_float_window", { width: rootW.value, height: rootH.value });
  } catch { /* 窗口可能尚未就绪 */ }
});

onUnmounted(() => {
  document.body.style.margin = "";
  document.body.style.padding = "";
  document.body.style.overflow = "";
  document.body.style.background = "";
});
</script>

<template>
  <div class="float-root" :style="{ width: rootW + 'px', height: rootH + 'px' }">
    <img
      :src="imageSrc"
      class="float-image"
      :style="{ width: rootW + 'px', height: rootH + 'px' }"
      draggable="false"
    />
  </div>
</template>

<style scoped>
.float-root {
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
}

.float-image {
  object-fit: contain;
  opacity: 0.55;
  pointer-events: none;
  user-select: none;
  -webkit-user-drag: none;
}
</style>
