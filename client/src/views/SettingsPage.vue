<script setup lang="ts">
import { ref } from "vue";
import { useAppStore } from "../stores/app";
import type { AppConfig } from "../lib/types";

const appStore = useAppStore();

const form = ref<AppConfig>({
  serverUrl: appStore.config?.serverUrl || "",
  clientId: appStore.config?.clientId || "",
  passkey: appStore.config?.passkey || "",
  acpUrl: appStore.config?.acpUrl || "",
  storagePath: appStore.config?.storagePath || "~/kaya-transfer/",
  acpCwd: appStore.config?.acpCwd || "",
  floatImage: appStore.config?.floatImage || "kaya-float",
  allowedReadPaths: appStore.config?.allowedReadPaths || ["~/kaya-transfer", "~/Desktop", "~/Documents"],
  allowedWritePaths: appStore.config?.allowedWritePaths || ["~/kaya-transfer", "~/Desktop"],
  deniedExtensions: appStore.config?.deniedExtensions || [".exe", ".dll", ".sys", ".bin"],
});

const saving = ref(false);
const saved = ref(false);
const saveError = ref<string | null>(null);

async function handleSave() {
  saving.value = true;
  saved.value = false;
  saveError.value = null;
  try {
    if (!form.value.acpUrl && form.value.serverUrl) {
      const host = form.value.serverUrl.replace("ws://", "").split(":")[0];
      form.value.acpUrl = `ws://${host}:8765`;
    }
    await appStore.save(form.value);
    saved.value = true;
    setTimeout(() => { saved.value = false; }, 3000);
  } catch (e) {
    saveError.value = String(e);
  } finally {
    saving.value = false;
  }
}

function handleReset() {
  form.value = {
    serverUrl: "",
    clientId: "",
    passkey: "",
    acpUrl: "",
    storagePath: "~/kaya-transfer/",
    acpCwd: "",
    allowedReadPaths: ["~/kaya-transfer", "~/Desktop", "~/Documents"],
    allowedWritePaths: ["~/kaya-transfer", "~/Desktop"],
    deniedExtensions: [".exe", ".dll", ".sys", ".bin"],
  };
}

// ── 路径白名单编辑 ──
function addPath(list: string[]) {
  list.push("");
}
function removePath(list: string[], index: number) {
  list.splice(index, 1);
}
</script>

<template>
  <div class="settings">
    <h2 class="page-title">设置</h2>
    <p class="page-subtitle">配置服务端连接、存储与安全策略</p>

    <!-- 服务端连接 -->
    <div class="form-section">
      <div class="section-title">🔗 服务端连接</div>
      <div class="form-group">
        <label class="form-label">WebSocket 地址</label>
        <input v-model="form.serverUrl" class="form-input" placeholder="ws://192.168.1.100:9765" />
      </div>
      <div class="form-group">
        <label class="form-label">ACP 桥接地址</label>
        <input v-model="form.acpUrl" class="form-input" placeholder="留空自动推导" />
      </div>
      <div class="form-row">
        <div class="form-group">
          <label class="form-label">客户端 ID</label>
          <input v-model="form.clientId" class="form-input" placeholder="pc-01" />
        </div>
        <div class="form-group">
          <label class="form-label">Passkey</label>
          <input v-model="form.passkey" type="password" class="form-input" placeholder="输入密钥" />
        </div>
      </div>
      <div class="form-group">
        <label class="form-label">ACP 工作目录（服务端路径）</label>
        <input v-model="form.acpCwd" class="form-input" placeholder="/kaya/tmp_workplace/kaya-beam" />
        <div class="form-hint">opencode-bridge 创建 session 时使用的服务端工作目录</div>
      </div>
    </div>

    <!-- 存储 -->
    <div class="form-section">
      <div class="section-title">📂 存储设置</div>
      <div class="form-group">
        <label class="form-label">默认存储路径</label>
        <div class="input-with-button">
          <input v-model="form.storagePath" class="form-input" placeholder="~/kaya-transfer/" />
        </div>
        <div class="form-hint">文件将保存到此目录下的 YYYY-MM/ 子文件夹。使用 ~ 表示用户主目录。</div>
      </div>
    </div>

    <!-- 路径白名单 -->
    <div class="form-section">
      <div class="section-title">🛡️ 路径安全</div>
      <p class="section-desc">控制 Kaya 能读写的目录和文件类型。修改后需重新编译客户端生效。</p>

      <div class="form-group">
        <label class="form-label">允许读取的路径</label>
        <div class="path-list">
          <div v-for="(_p, i) in form.allowedReadPaths" :key="'r'+i" class="path-row">
            <input v-model="form.allowedReadPaths![i]" class="form-input path-input" placeholder="~/Documents" />
            <button class="btn-icon" @click="removePath(form.allowedReadPaths!, i)" title="移除">✕</button>
          </div>
        </div>
        <button class="btn-add" @click="addPath(form.allowedReadPaths!)">+ 添加路径</button>
        <div class="form-hint">Kaya 只能在这些目录及其子目录中读取文件。</div>
      </div>

      <div class="form-group">
        <label class="form-label">允许写入的路径</label>
        <div class="path-list">
          <div v-for="(_p, i) in form.allowedWritePaths" :key="'w'+i" class="path-row">
            <input v-model="form.allowedWritePaths![i]" class="form-input path-input" placeholder="~/Desktop" />
            <button class="btn-icon" @click="removePath(form.allowedWritePaths!, i)" title="移除">✕</button>
          </div>
        </div>
        <button class="btn-add" @click="addPath(form.allowedWritePaths!)">+ 添加路径</button>
        <div class="form-hint">Kaya 只能在这些目录及其子目录中写入或创建文件。</div>
      </div>

      <div class="form-group">
        <label class="form-label">禁止的文件扩展名</label>
        <div class="path-list">
          <div v-for="(_e, i) in form.deniedExtensions" :key="'d'+i" class="path-row">
            <input v-model="form.deniedExtensions![i]" class="form-input path-input" placeholder=".exe" />
            <button class="btn-icon" @click="removePath(form.deniedExtensions!, i)" title="移除">✕</button>
          </div>
        </div>
        <button class="btn-add" @click="addPath(form.deniedExtensions!)">+ 添加类型</button>
        <div class="form-hint">即使路径在白名单内，这些扩展名的文件也不能被读取或写入。</div>
      </div>
    </div>

    <!-- 悬浮图 -->
    <div class="form-section">
      <div class="section-title">🖼️ 悬浮图设置</div>
      <div class="radio-group">
        <label class="radio-card" :class="{ active: form.floatImage === 'kaya-float' }">
          <input type="radio" v-model="form.floatImage" value="kaya-float" />
          <div class="radio-preview"><img src="/kaya-float.png" class="radio-thumb" /></div>
          <div class="radio-label">方形（320×320）</div>
        </label>
        <label class="radio-card" :class="{ active: form.floatImage === 'kaya-full' }">
          <input type="radio" v-model="form.floatImage" value="kaya-full" />
          <div class="radio-preview"><img src="/kaya-full.png" class="radio-thumb" /></div>
          <div class="radio-label">竖版（自适应高度）</div>
        </label>
      </div>
      <div class="form-hint">切换后需重新打开悬浮窗生效</div>
    </div>

    <div v-if="saveError" class="save-error">{{ saveError }}</div>

    <div class="form-actions">
      <button class="btn-secondary" @click="handleReset">重置</button>
      <button class="btn-primary" :disabled="saving" @click="handleSave">
        {{ saving ? "保存中..." : "保存配置" }}
      </button>
      <span v-if="saved" class="save-success">✓ 已保存</span>
    </div>
  </div>
</template>

<style scoped>
.settings { padding: 24px 32px; max-width: 640px; }
.page-title { font-size: 18px; font-weight: 600; color: var(--color-text); margin-bottom: 4px; }
.page-subtitle { font-size: 13px; color: var(--color-text-muted); margin-bottom: 24px; }

.form-section {
  background: var(--color-surface);
  border-radius: var(--radius-lg);
  padding: 20px 24px;
  margin-bottom: 16px;
  box-shadow: var(--shadow-card);
}
.section-title {
  font-size: 13px; font-weight: 600; color: var(--color-text);
  margin-bottom: 4px; padding-bottom: 12px;
  border-bottom: 1px solid var(--color-border-light);
}
.section-desc {
  font-size: 12px; color: var(--color-text-muted);
  margin-bottom: 14px; margin-top: -8px; line-height: 1.5;
}

.form-group { margin-bottom: 14px; }
.form-group:last-child { margin-bottom: 0; }
.form-label { font-size: 12px; font-weight: 500; color: var(--color-text-secondary); display: block; margin-bottom: 4px; }
.form-input {
  width: 100%; padding: 10px 12px; border: 1.5px solid var(--color-border);
  border-radius: var(--radius-sm); font-size: 13px; color: var(--color-text);
  background: var(--color-bg); box-sizing: border-box; transition: border-color 0.15s;
}
.form-input:focus { border-color: var(--color-primary); }
.form-row { display: flex; gap: 12px; }
.form-row .form-group { flex: 1; }
.input-with-button { display: flex; gap: 8px; }
.input-with-button .form-input { flex: 1; }
.form-hint { font-size: 11px; color: var(--color-text-light); margin-top: 4px; }

/* path list */
.path-list { display: flex; flex-direction: column; gap: 6px; }
.path-row { display: flex; gap: 6px; align-items: center; }
.path-input { flex: 1; }
.btn-icon {
  width: 28px; height: 28px; border: none; background: transparent;
  color: var(--color-text-light); font-size: 14px; cursor: pointer;
  border-radius: 4px; display: flex; align-items: center; justify-content: center;
}
.btn-icon:hover { background: rgba(239, 68, 68, 0.1); color: #ef4444; }
.btn-add {
  font-size: 12px; padding: 6px 14px; border: 1.5px dashed var(--color-border);
  border-radius: 6px; background: transparent; color: var(--color-text-muted);
  cursor: pointer; margin-top: 6px; width: 100%; transition: border-color 0.15s, color 0.15s;
}
.btn-add:hover { border-color: var(--color-primary); color: var(--color-primary); }

.form-actions { display: flex; gap: 8px; align-items: center; justify-content: flex-end; }
.btn-primary {
  background: var(--color-primary); color: #fff; border: none;
  border-radius: var(--radius-md); padding: 10px 24px; font-size: 13px;
  font-weight: 500; transition: background 0.15s; cursor: pointer;
}
.btn-primary:hover:not(:disabled) { background: var(--color-primary-hover); }
.btn-primary:disabled { opacity: 0.5; cursor: default; }
.btn-secondary {
  background: var(--color-surface); border: 1.5px solid var(--color-border);
  border-radius: var(--radius-md); padding: 10px 24px; font-size: 13px;
  color: var(--color-text-secondary); transition: background 0.15s; cursor: pointer;
}
.btn-secondary:hover { background: var(--color-sidebar-hover); }
.save-success { font-size: 13px; color: var(--color-success); font-weight: 500; }
.save-error { font-size: 13px; color: var(--color-error); margin-bottom: 12px; text-align: right; }

.radio-group { display: flex; gap: 12px; }
.radio-card {
  flex: 1; display: flex; flex-direction: column; align-items: center; gap: 8px;
  padding: 12px; border: 2px solid var(--color-border); border-radius: var(--radius-md);
  cursor: pointer; transition: border-color 0.15s, background 0.15s; background: var(--color-bg);
}
.radio-card:hover { border-color: var(--color-primary); }
.radio-card.active { border-color: var(--color-primary); background: rgba(99, 102, 241, 0.06); }
.radio-card input[type="radio"] { display: none; }
.radio-preview {
  width: 80px; height: 80px; display: flex; align-items: center; justify-content: center;
  overflow: hidden; border-radius: 6px; background: #f1f5f9;
}
.radio-thumb { max-width: 100%; max-height: 100%; object-fit: contain; }
.radio-label { font-size: 12px; font-weight: 500; color: var(--color-text-secondary); }
</style>
