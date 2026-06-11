import { createApp } from "vue";
import { createPinia } from "pinia";
import { createRouter, createWebHistory } from "vue-router";
import App from "./App.vue";
import HomePage from "./views/HomePage.vue";
import FileTransferPage from "./views/FileTransferPage.vue";
import ChatPage from "./views/ChatPage.vue";
import SettingsPage from "./views/SettingsPage.vue";
import CapabilitiesPage from "./views/CapabilitiesPage.vue";
import CopilotOverlayWindow from "./views/CopilotOverlayWindow.vue";
import FloatPage from "./views/FloatPage.vue";
import BubblePage from "./views/BubblePage.vue";
import QuickChat from "./views/QuickChat.vue";
import ToolCallOverlay from "./views/ToolCallOverlay.vue";
import "./style.css";

const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/float" },
    { path: "/float", component: FloatPage },
    { path: "/home", component: HomePage },
    { path: "/files", component: FileTransferPage },
    { path: "/chat", component: ChatPage },
    { path: "/settings", component: SettingsPage },
    { path: "/capabilities", component: CapabilitiesPage },
    { path: "/copilot", component: CopilotOverlayWindow },
    { path: "/bubble", component: BubblePage },
    { path: "/quick-chat", component: QuickChat },
    { path: "/tool-call", component: ToolCallOverlay },
  ],
});

const app = createApp(App);
const pinia = createPinia();

app.use(pinia);
app.use(router);
app.mount("#app");
