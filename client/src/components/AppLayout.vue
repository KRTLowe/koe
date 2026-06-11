<script setup lang="ts">
import Sidebar from "./Sidebar.vue";
</script>

<template>
  <div class="app-layout">
    <Sidebar />
    <main class="content-area">
      <router-view />
    </main>
  </div>
</template>

<style scoped>
.app-layout {
  display: flex;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
}

.content-area {
  flex: 1;
  overflow-y: auto;
  background: var(--color-bg);
  min-width: 0;
}
</style>
