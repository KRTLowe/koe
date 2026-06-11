<script setup lang="ts">
import { useRoute } from "vue-router";

const route = useRoute();

const navItems = [
  { path: "/home", label: "Home", icon: "🏠" },
  { path: "/files", label: "文件传输", icon: "📁" },
  { path: "/chat", label: "聊天", icon: "💬" },
  { path: "/capabilities", label: "能力", icon: "🧰" },
  { path: "/settings", label: "设置", icon: "⚙️" },
];

function isActive(path: string): boolean {
  return route.path.startsWith(path);
}
</script>

<template>
  <aside class="sidebar">
    <div class="sidebar-header">
      <div class="sidebar-logo">kaya-beam</div>
      <div class="sidebar-subtitle">File Transfer Hub</div>
    </div>

    <nav class="sidebar-nav">
      <router-link
        v-for="item in navItems"
        :key="item.path"
        :to="item.path"
        class="nav-item"
        :class="{ active: isActive(item.path) }"
      >
        <span class="nav-icon">{{ item.icon }}</span>
        <span class="nav-label">{{ item.label }}</span>
      </router-link>
    </nav>

    <div class="sidebar-footer">v0.1.0</div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 200px;
  min-width: 200px;
  height: 100%;
  background: var(--color-surface);
  border-right: 1px solid var(--color-border);
  display: flex;
  flex-direction: column;
  padding: 16px 0;
  user-select: none;
}

.sidebar-header {
  padding: 0 20px 16px;
  border-bottom: 1px solid var(--color-border-light);
  margin-bottom: 8px;
}

.sidebar-logo {
  font-weight: 600;
  font-size: 14px;
  color: var(--color-text);
}

.sidebar-subtitle {
  font-size: 11px;
  color: var(--color-text-muted);
  margin-top: 1px;
}

.sidebar-nav {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 0 8px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  color: var(--color-text-secondary);
  text-decoration: none;
  transition: background 0.15s, color 0.15s;
}

.nav-item:hover {
  background: var(--color-sidebar-hover);
  color: var(--color-text);
}

.nav-item.active,
.nav-item.router-link-active {
  background: var(--color-primary);
  color: #fff;
  font-weight: 500;
}

.nav-icon {
  font-size: 16px;
  width: 20px;
  text-align: center;
}

.nav-label {
  line-height: 1;
}

.sidebar-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--color-border-light);
  font-size: 11px;
  color: var(--color-text-light);
}
</style>
