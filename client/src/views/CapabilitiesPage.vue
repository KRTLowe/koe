<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { ref, onMounted } from "vue";

const sending = ref<string | null>(null);
const toolStates = ref<Record<string, boolean>>({
  take_screenshot: true,
  get_clipboard: true,
  write_clipboard: true,
  file_search: true,
  get_uia_tree: true,
  read_text_file: true,
  write_text_file: false,
  list_directory: true,
  get_file_info: true,
  grep_file: true,
  pull_file: true,
  type_text: false,
  key_press: false,
  mouse_click: false,
  open_path: true,
  system_info: true,
  list_windows: true,
  start_process: false,
  kill_process: false,
});

onMounted(async () => {
  try {
    const config: any = await invoke("load_config");
    if (config?.toolPermissions) {
      for (const [name, enabled] of Object.entries(config.toolPermissions)) {
        if (name in toolStates.value) {
          toolStates.value[name] = enabled as boolean;
        }
      }
    }
  } catch {
    // 配置未加载，使用默认值
  }
});

async function toggleTool(name: string, enabled: boolean) {
  console.log(`[Capabilities] toggleTool: ${name} -> ${enabled}`);
  toolStates.value[name] = enabled;
  try {
    await invoke("set_tool_enabled", { name, enabled });
    console.log(`[Capabilities] set_tool_enabled(${name}, ${enabled}) OK`);
  } catch (e) {
    console.error(`[Capabilities] set_tool_enabled failed:`, e);
    toolStates.value[name] = !enabled;
  }
}

async function sendSignal(name: string, sticky: boolean, priority: string) {
  console.log(`[Capabilities] sendSignal: ${name} sticky=${sticky} priority=${priority}`);
  sending.value = name;
  try {
    await invoke("send_signal", {
      name,
      sticky,
      priority,
      notifyOnce: false,
      data: {
        source: "manual",
        timestamp: new Date().toISOString(),
        hint: sticky ? "view" : "none",
      },
    });
    console.log(`[Capabilities] sendSignal(${name}) OK`);
  } catch (e) {
    console.error("[Capabilities] send_signal failed:", e);
  } finally {
    setTimeout(() => { sending.value = null; }, 1500);
  }
}

const sections = [
  {
    title: "系统工具",
    items: [
      {
        name: "take_screenshot",
        description: "截取 Windows 桌面屏幕，支持区域裁剪 (x/y/width/height)",
        params: "x, y, width, height（可选）",
        type: "有上传",
        enabled: true,
      },
      {
        name: "system_info",
        description: "获取系统信息：OS、CPU、内存、GPU、磁盘、网络",
        params: "无",
        type: "无上传",
        enabled: true,
      },
      {
        name: "list_windows",
        description: "枚举所有可见窗口，返回标题、位置、大小、PID",
        params: "filter, max_results",
        type: "无上传",
        enabled: true,
      },
      {
        name: "open_path",
        description: "用默认程序打开文件、文件夹或 URL",
        params: "path",
        type: "无上传",
        enabled: true,
      },
    ],
  },
  {
    title: "文件与剪贴板",
    items: [
      {
        name: "get_clipboard",
        description: "读取 Windows 剪贴板文本内容",
        params: "format (text)",
        type: "无上传",
        enabled: true,
      },
      {
        name: "write_clipboard",
        description: "将文本写入 Windows 剪贴板",
        params: "text",
        type: "无上传",
        enabled: true,
      },
      {
        name: "read_text_file",
        description: "读取文本文件，支持字节偏移和行号定位",
        params: "path, encoding, offset, limit",
        type: "无上传",
        enabled: true,
      },
      {
        name: "write_text_file",
        description: "写入或追加文本到文件（默认禁用）",
        params: "path, content, mode",
        type: "无上传",
        enabled: false,
      },
      {
        name: "file_search",
        description: "按文件名搜索目录，支持递归（默认关闭递归）",
        params: "root, pattern, recursive, max_results",
        type: "无上传",
        enabled: true,
      },
      {
        name: "list_directory",
        description: "列出目录内容，支持递归和名称过滤",
        params: "path, pattern, recursive, max_results",
        type: "无上传",
        enabled: true,
      },
      {
        name: "get_file_info",
        description: "获取文件或目录的元信息（大小、修改时间等）",
        params: "path",
        type: "无上传",
        enabled: true,
      },
      {
        name: "grep_file",
        description: "按模式搜索文件内容，返回匹配行及上下文",
        params: "path, pattern, use_regex, max_results",
        type: "无上传",
        enabled: true,
      },
      {
        name: "pull_file",
        description: "从客户端拉取文件上传到服务端",
        params: "path",
        type: "有上传",
        enabled: true,
      },
    ],
  },
  {
    title: "进程控制（默认禁用）",
    items: [
      {
        name: "run_command",
        description: "通过 cmd.exe 执行任意命令，超时 60s，输出上限 16KB",
        params: "command",
        type: "无上传",
        enabled: false,
      },
      {
        name: "start_process",
        description: "启动程序不等待，返回 PID。用 kill_process 停止",
        params: "path, args, cwd",
        type: "无上传",
        enabled: false,
      },
      {
        name: "kill_process",
        description: "通过 PID 或进程名终止进程（使用 taskkill /F）",
        params: "pid 或 name",
        type: "无上传",
        enabled: false,
      },
    ],
  },
  {
    title: "输入控制（默认禁用）",
    subtitle: "模拟键盘鼠标。使用 list_windows 获取窗口位置，take_screenshot 确认目标坐标",
    items: [
      {
        name: "type_text",
        description: "在当前焦点位置模拟键盘输入文字",
        params: "text",
        type: "无上传",
        enabled: false,
      },
      {
        name: "key_press",
        description: "模拟按键或组合键：enter, ctrl+c, alt+tab, f5 等",
        params: "keys (如 ctrl+shift+esc)",
        type: "无上传",
        enabled: false,
      },
      {
        name: "mouse_click",
        description: "移动鼠标到屏幕坐标并点击，支持左右键双击",
        params: "x, y, button, clicks",
        type: "无上传",
        enabled: false,
      },
    ],
  },
  {
    title: "无障碍",
    items: [
      {
        name: "get_uia_tree",
        description: "提取当前活动窗口的 UIA 无障碍控件树",
        params: "无",
        type: "无上传",
        enabled: true,
      },
    ],
  },
  {
    title: "外部信号",
    subtitle: "客户端主动发起的通知，由服务端调度后推给 Kaya",
    items: [
      {
        name: "visual_input_available",
        description: "外部触发的视觉输入就绪",
        trigger: "热键 / 定时器",
        sticky: true,
        priority: "high",
        hint: "Kaya 收到后可选择调用 take_screenshot",
      },
      {
        name: "clipboard_changed",
        description: "剪贴板内容变更",
        trigger: "自动检测",
        sticky: false,
        priority: "normal",
        hint: "一次性通知，不保留状态",
      },
      {
        name: "copilot_query",
        description: "Copilot 查询：携带问题 + 窗口 UIA 树发给 Kaya",
        trigger: "Ctrl+Alt+S / Ctrl+Alt+C",
        sticky: false,
        priority: "high",
        hint: "单次(S)一次性 / 连续(C)粘性",
      },
    ],
  },
  {
    title: "快捷键",
    items: [
      { name: "Ctrl + Alt + K", description: "打开/切换聊天窗口" },
      { name: "Ctrl + Alt + S", description: "Copilot 单次查询" },
      { name: "Ctrl + Alt + C", description: "Copilot 持续监测（Esc 停止）" },
    ],
  },
];
</script>

<template>
  <div class="page">
    <div class="page-header">
      <h1>能力与快捷键</h1>
      <p class="page-subtitle">客户端注册的工具、服务端处理的信号、系统快捷键</p>
    </div>

    <div class="content">
      <section v-for="(section, si) in sections" :key="si" class="section">
        <h2>{{ section.title }}</h2>
        <p v-if="section.subtitle" class="section-subtitle">{{ section.subtitle }}</p>

        <div v-if="section.title === '快捷键'" class="shortcut-list">
          <div v-for="(item, ii) in section.items" :key="ii" class="shortcut-row">
            <kbd>{{ item.name }}</kbd>
            <span class="shortcut-desc">{{ item.description }}</span>
          </div>
        </div>

        <table v-else class="cap-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>说明</th>
              <th v-if="'params' in section.items[0]">参数</th>
              <th v-if="'type' in section.items[0]">类型</th>
              <th v-if="'type' in section.items[0]">权限</th>
              <th v-if="'trigger' in section.items[0]">触发</th>
              <th v-if="'sticky' in section.items[0]">粘性</th>
              <th v-if="'priority' in section.items[0]">优先级</th>
              <th v-if="'trigger' in section.items[0]">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, ii) in section.items" :key="ii">
              <td><code>{{ item.name }}</code></td>
              <td>{{ item.description }}</td>
              <td v-if="'params' in item">{{ item.params }}</td>
              <td v-if="'type' in item">
                <span :class="['badge', item.type === '有上传' ? 'badge-upload' : 'badge-plain']">
                  {{ item.type }}
                </span>
              </td>
              <td v-if="'type' in item">
                <label class="toggle-switch">
                  <input type="checkbox"
                    :checked="toolStates[item.name] ?? true"
                    @change="toggleTool(item.name, ($event.target as HTMLInputElement).checked)" />
                  <span class="toggle-slider"></span>
                </label>
              </td>
              <td v-if="'trigger' in item">{{ item.trigger }}</td>
              <td v-if="'sticky' in item">
                <span :class="['badge', item.sticky ? 'badge-sticky' : 'badge-once']">
                  {{ item.sticky ? '粘性' : '一次性' }}
                </span>
              </td>
              <td v-if="'priority' in item">
                <span :class="['badge', 'badge-' + item.priority]">
                  {{ item.priority }}
                </span>
              </td>
              <td v-if="'trigger' in item">
                <button
                  class="signal-btn"
                  :disabled="sending === item.name"
                  @click="sendSignal(item.name, item.sticky, item.priority)"
                >
                  {{ sending === item.name ? '已发送' : '触发' }}
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </section>
    </div>
  </div>
</template>

<style scoped>
.page {
  padding: 24px 32px;
  height: 100%;
  overflow-y: auto;
}

.page-header {
  margin-bottom: 24px;
}

.page-header h1 {
  font-size: 20px;
  font-weight: 600;
  color: var(--color-text);
  margin: 0;
}

.page-subtitle {
  font-size: 13px;
  color: var(--color-text-muted);
  margin: 4px 0 0;
}

.section {
  margin-bottom: 32px;
}

.section h2 {
  font-size: 15px;
  font-weight: 600;
  color: var(--color-text);
  margin: 0 0 4px;
}

.section-subtitle {
  font-size: 12px;
  color: var(--color-text-muted);
  margin: 0 0 12px;
}

.cap-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.cap-table th {
  text-align: left;
  padding: 8px 12px;
  font-weight: 500;
  color: var(--color-text-muted);
  border-bottom: 1px solid var(--color-border);
  white-space: nowrap;
}

.cap-table td {
  padding: 10px 12px;
  border-bottom: 1px solid var(--color-border-light);
  color: var(--color-text);
}

.cap-table code {
  font-size: 12px;
  background: #f1f5f9;
  padding: 2px 6px;
  border-radius: 3px;
  color: var(--color-primary);
}

.badge {
  display: inline-block;
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 10px;
  font-weight: 500;
}

.badge-upload {
  background: #e0f2fe;
  color: #0369a1;
}

.badge-plain {
  background: #f1f5f9;
  color: #475569;
}

.badge-sticky {
  background: #fef3c7;
  color: #92400e;
}

.badge-once {
  background: #f1f5f9;
  color: #475569;
}

.badge-high {
  background: #fecaca;
  color: #991b1b;
}

.badge-normal {
  background: #f1f5f9;
  color: #475569;
}

.shortcut-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.shortcut-row {
  display: flex;
  align-items: center;
  gap: 16px;
}

.shortcut-row kbd {
  display: inline-block;
  font-size: 12px;
  padding: 4px 10px;
  background: var(--color-surface);
  border: 1px solid var(--color-border);
  border-radius: 4px;
  font-family: inherit;
  min-width: 100px;
  text-align: center;
  box-shadow: 0 1px 0 var(--color-border);
}

.shortcut-desc {
  font-size: 13px;
  color: var(--color-text);
}

.signal-btn {
  font-size: 11px;
  padding: 4px 12px;
  border: 1px solid var(--color-primary);
  border-radius: 4px;
  background: transparent;
  color: var(--color-primary);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.15s;
}

.signal-btn:hover:not(:disabled) {
  background: var(--color-primary);
  color: #fff;
}

.signal-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.toggle-switch {
  position: relative;
  display: inline-block;
  width: 36px;
  height: 20px;
}
.toggle-switch input {
  opacity: 0;
  width: 0;
  height: 0;
}
.toggle-slider {
  position: absolute;
  cursor: pointer;
  inset: 0;
  background: #ccc;
  border-radius: 20px;
  transition: 0.3s;
}
.toggle-slider::before {
  content: "";
  position: absolute;
  height: 16px;
  width: 16px;
  left: 2px;
  bottom: 2px;
  background: white;
  border-radius: 50%;
  transition: 0.3s;
}
.toggle-switch input:checked + .toggle-slider {
  background: var(--color-primary);
}
.toggle-switch input:checked + .toggle-slider::before {
  transform: translateX(16px);
}
</style>
