# 客户端本地历史存储实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 为 kaya-beam 客户端增加基于 SQLite 的本地聊天历史与文件传输历史基础版持久化，并在聊天页左侧展示卡雅会话列表。

**架构：** 在 Tauri Rust 端新增 `chat_history.rs` 和 `file_history.rs` 两个目的型 SQLite 模块，由 `lib.rs` 暴露 Tauri command 给前端使用。前端 `chat`/`file` store 改为从 IPC 加载与写入本地历史；聊天页改为“左侧会话列表 + 右侧消息区域”布局，并明确基础版只允许单活跃流式会话，防止串台。

**技术栈：** Rust `rusqlite` / Tauri 2 command / Vue 3 / Pinia / pytest / cargo test / cargo check。

---

## 文件结构

- 修改：`client/src-tauri/Cargo.toml`
  - 增加 SQLite 依赖（`rusqlite`），保持现有依赖风格。
- 创建：`client/src-tauri/src/chat_history.rs`
  - 负责 `kaya_sessions`、`acp_sessions`、`chat_messages` 的建表、查询、写入、恢复逻辑。
- 创建：`client/src-tauri/src/file_history.rs`
  - 负责 `file_transfer_history` 的建表、查询、写入逻辑。
- 修改：`client/src-tauri/src/lib.rs`
  - 注册历史相关 command，初始化数据库访问入口，把现有聊天/文件事件接入持久层。
- 修改：`client/src-tauri/src/acp_runtime.rs`
  - 如果当前有 `session_id` 更新入口，补与卡雅会话/ACP 会话绑定的调用；若逻辑集中在 `lib.rs`/事件侧，则只在对应文件修改。
- 修改：`client/src/lib/types.ts`
  - 增加卡雅会话、ACP 会话、聊天消息、本地文件历史的前端类型。
- 修改：`client/src/lib/tauri.ts`
  - 增加加载会话、加载消息、新建会话、加载文件历史等 IPC 封装。
- 修改：`client/src/stores/chat.ts`
  - 从单一内存消息流改为“会话列表 + 当前卡雅会话 + 当前 ACP 会话 + 持久化消息加载/追加 + 单活跃回复约束”。
- 修改：`client/src/stores/file.ts`
  - 从纯内存历史改为可初始化加载 SQLite 历史，并在发送/接收时同步持久化。
- 修改：`client/src/views/ChatPage.vue`
  - 改为左侧边栏会话列表、右侧消息区布局；增加“新建聊天”“切换会话”交互和“已有会话正在回复”提示。
- 修改：`client/src/views/HomePage.vue`
  - 最近传输记录继续展示，但来源改为持久化后的 `fileStore.history`。
- 修改：`client/src/views/FileTransferPage.vue`
  - 上传后写入持久化文件历史；页面初始化显示从数据库恢复的历史。
- 修改：`docs/protocol.md`
  - 如果需要，补一行说明客户端本地历史不影响协议字段，仅作为本地恢复能力。

---

### 任务 1：引入 SQLite 依赖并建立聊天历史数据模块

**文件：**
- 修改：`client/src-tauri/Cargo.toml`
- 创建：`client/src-tauri/src/chat_history.rs`

- [ ] **步骤 1：编写失败测试，固定聊天历史最小 API**

在 `client/src-tauri/src/chat_history.rs` 中先写 tests 模块，定义最小 API 期待：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_restores_latest_kaya_session() {
        let db = open_history_db_in_memory().unwrap();
        let first = create_kaya_session(&db, "新会话 1").unwrap();
        let second = create_kaya_session(&db, "新会话 2").unwrap();

        let latest = load_latest_kaya_session(&db).unwrap().unwrap();

        assert_eq!(latest.id, second.id);
        assert_ne!(first.id, second.id);
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cd /kaya/tmp_workplace/kaya-beam/client/src-tauri
cargo test chat_history::tests::creates_and_restores_latest_kaya_session
```

预期：FAIL，报错 `chat_history` 模块或 `open_history_db_in_memory` / `create_kaya_session` / `load_latest_kaya_session` 未定义。

- [ ] **步骤 3：增加 SQLite 依赖并实现最小聊天历史模块**

在 `Cargo.toml` 的 `[dependencies]` 增加：

```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

在 `chat_history.rs` 里实现最小结构：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct KayaSessionRecord {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
}

pub fn open_history_db_in_memory() -> Result<rusqlite::Connection, String>;
pub fn create_kaya_session(db: &rusqlite::Connection, title: &str) -> Result<KayaSessionRecord, String>;
pub fn load_latest_kaya_session(db: &rusqlite::Connection) -> Result<Option<KayaSessionRecord>, String>;
```

并在建表 SQL 中先建立 `kaya_sessions`：

```sql
CREATE TABLE IF NOT EXISTS kaya_sessions (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  is_active INTEGER NOT NULL DEFAULT 0
)
```

- [ ] **步骤 4：运行测试验证通过**

运行：

```bash
cargo test chat_history::tests::creates_and_restores_latest_kaya_session
```

预期：PASS。

---

### 任务 2：扩展聊天历史为卡雅会话 / ACP 会话 / 消息三表

**文件：**
- 修改：`client/src-tauri/src/chat_history.rs`

- [ ] **步骤 1：编写失败测试，验证单卡雅会话下可绑定多个 ACP 会话并按消息归属查询**

在 `chat_history.rs` tests 模块追加：

```rust
#[test]
fn appends_messages_to_kaya_session_across_multiple_acp_sessions() {
    let db = open_history_db_in_memory().unwrap();
    let kaya = create_kaya_session(&db, "会话").unwrap();
    let acp_a = create_or_switch_acp_session(&db, &kaya.id, "remote-a").unwrap();
    append_chat_message(&db, &kaya.id, Some(&acp_a.id), "user", "hello").unwrap();
    let acp_b = create_or_switch_acp_session(&db, &kaya.id, "remote-b").unwrap();
    append_chat_message(&db, &kaya.id, Some(&acp_b.id), "assistant", "world").unwrap();

    let messages = load_chat_messages(&db, &kaya.id).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "hello");
    assert_eq!(messages[1].content, "world");
    assert_eq!(messages[0].acp_session_id.as_deref(), Some(acp_a.id.as_str()));
    assert_eq!(messages[1].acp_session_id.as_deref(), Some(acp_b.id.as_str()));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cargo test chat_history::tests::appends_messages_to_kaya_session_across_multiple_acp_sessions
```

预期：FAIL，报错 `create_or_switch_acp_session` / `append_chat_message` / `load_chat_messages` 未定义。

- [ ] **步骤 3：实现三表模型和查询顺序**

在 `chat_history.rs` 中补充：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct AcpSessionRecord {
    pub id: String,
    pub kaya_session_id: String,
    pub remote_session_id: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_active: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChatMessageRecord {
    pub id: String,
    pub kaya_session_id: String,
    pub acp_session_id: Option<String>,
    pub role: String,
    pub content: String,
    pub created_at: String,
}
```

并建立：

```sql
CREATE TABLE IF NOT EXISTS acp_sessions (...)
CREATE TABLE IF NOT EXISTS chat_messages (...)
```

要求 `load_chat_messages` 使用：

```sql
ORDER BY created_at ASC, id ASC
```

- [ ] **步骤 4：运行聊天历史测试集**

运行：

```bash
cargo test chat_history::tests -- --test-threads=1
```

预期：PASS。

---

### 任务 3：建立文件传输历史数据模块

**文件：**
- 创建：`client/src-tauri/src/file_history.rs`

- [ ] **步骤 1：编写失败测试，验证 sent/received 记录按时间倒序返回**

在 `file_history.rs` 中先写：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_file_history_in_descending_time_order() {
        let db = open_file_history_db_in_memory().unwrap();
        append_file_transfer_record(&db, NewFileTransferRecord {
            direction: "received".into(),
            file_name: "a.txt".into(),
            file_path: Some("C:/a.txt".into()),
            file_size: 1,
            status: "ok".into(),
            kaya_session_id: None,
            acp_session_id: None,
        }).unwrap();
        append_file_transfer_record(&db, NewFileTransferRecord {
            direction: "sent".into(),
            file_name: "b.txt".into(),
            file_path: None,
            file_size: 2,
            status: "ok".into(),
            kaya_session_id: None,
            acp_session_id: None,
        }).unwrap();

        let history = load_file_transfer_history(&db).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].file_name, "b.txt");
        assert_eq!(history[1].file_name, "a.txt");
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cargo test file_history::tests::loads_file_history_in_descending_time_order
```

预期：FAIL，报错模块或相关函数未定义。

- [ ] **步骤 3：实现文件历史记录模块**

实现：

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct FileTransferRecord {
    pub id: String,
    pub direction: String,
    pub file_name: String,
    pub file_path: Option<String>,
    pub file_size: i64,
    pub status: String,
    pub created_at: String,
    pub kaya_session_id: Option<String>,
    pub acp_session_id: Option<String>,
}

pub struct NewFileTransferRecord { ... }
```

并建立：

```sql
CREATE TABLE IF NOT EXISTS file_transfer_history (...)
```

查询顺序：

```sql
ORDER BY created_at DESC, id DESC
```

- [ ] **步骤 4：运行文件历史测试**

运行：

```bash
cargo test file_history::tests -- --test-threads=1
```

预期：PASS。

---

### 任务 4：在 Tauri 端接入历史数据库与 IPC 命令

**文件：**
- 修改：`client/src-tauri/src/lib.rs`
- 测试：`client/src-tauri/src/lib.rs` 内部 tests（如果已有）或为 `chat_history.rs` / `file_history.rs` 添加更贴近 command 的测试

- [ ] **步骤 1：编写失败测试，验证首次启动无会话时会自动创建卡雅会话**

优先在 `chat_history.rs` 追加一个更贴近恢复逻辑的测试：

```rust
#[test]
fn ensure_active_kaya_session_creates_one_when_missing() {
    let db = open_history_db_in_memory().unwrap();
    let session = ensure_active_kaya_session(&db).unwrap();
    let latest = load_latest_kaya_session(&db).unwrap().unwrap();
    assert_eq!(session.id, latest.id);
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cargo test chat_history::tests::ensure_active_kaya_session_creates_one_when_missing
```

预期：FAIL，`ensure_active_kaya_session` 未定义。

- [ ] **步骤 3：在 Rust 端接入实际数据库文件和 command**

在 `lib.rs` 新增模块声明：

```rust
mod chat_history;
mod file_history;
```

并增加数据库文件路径 helper，例如：

```rust
fn history_db_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("history.db"))
}
```

增加 command（命名可微调，但前后必须一致）：

```rust
#[tauri::command]
fn load_kaya_sessions(app: tauri::AppHandle) -> Result<Vec<chat_history::KayaSessionRecord>, String>;

#[tauri::command]
fn load_latest_kaya_session(app: tauri::AppHandle) -> Result<Option<chat_history::KayaSessionRecord>, String>;

#[tauri::command]
fn create_kaya_session(app: tauri::AppHandle) -> Result<chat_history::KayaSessionRecord, String>;

#[tauri::command]
fn load_chat_messages(app: tauri::AppHandle, kaya_session_id: String) -> Result<Vec<chat_history::ChatMessageRecord>, String>;

#[tauri::command]
fn load_file_transfer_history(app: tauri::AppHandle) -> Result<Vec<file_history::FileTransferRecord>, String>;
```

并把它们注册到 `invoke_handler`。

- [ ] **步骤 4：运行 Rust 检查**

运行：

```bash
cargo check
```

预期：PASS。

---

### 任务 5：让聊天 store 具备会话列表、恢复、单活跃流约束

**文件：**
- 修改：`client/src/lib/types.ts`
- 修改：`client/src/lib/tauri.ts`
- 修改：`client/src/stores/chat.ts`

- [ ] **步骤 1：编写失败测试，固定“切换会话不改变已发起请求归属”的基础状态结构**

如果项目当前没有 Vitest，就先在 `chat.ts` 中提取纯函数并写最小测试文件 `client/src/stores/chat.spec.ts`；如果不想引入新测试框架，则先在 `chat.ts` 提取一个纯状态 helper，并通过 TypeScript 编译 + 现有手工验证推进。推荐先添加 Vitest 测试文件：

```ts
import { describe, expect, it } from "vitest";
import { canStartReplyInSession } from "./chat";

describe("chat concurrency", () => {
  it("blocks starting a new reply in another session while one session is responding", () => {
    expect(canStartReplyInSession("b", { activeReplySessionId: "a" })).toBe(false);
  });
});
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cd /kaya/tmp_workplace/kaya-beam/client
npx vitest run src/stores/chat.spec.ts
```

预期：FAIL，`canStartReplyInSession` 未定义；如果项目尚无 `vitest`，则先安装开发依赖并在本任务中一并引入测试基础设施。

- [ ] **步骤 3：扩展类型与 IPC 封装，并重写 chat store**

在 `types.ts` 增加：

```ts
export interface KayaSessionRecord { ... }
export interface ChatMessageRecord { ... }
```

在 `tauri.ts` 增加封装：

```ts
export async function loadKayaSessions(): Promise<KayaSessionRecord[]> { ... }
export async function loadLatestKayaSession(): Promise<KayaSessionRecord | null> { ... }
export async function createKayaSession(): Promise<KayaSessionRecord> { ... }
export async function loadChatMessages(kayaSessionId: string): Promise<ChatMessageRecord[]> { ... }
```

在 `chat.ts` 中把 store 结构改为至少包含：

```ts
const kayaSessions = ref<KayaSessionRecord[]>([]);
const currentKayaSessionId = ref<string | null>(null);
const currentAcpSessionId = ref<string | null>(null);
const activeReplySessionId = ref<string | null>(null);
```

并实现：

- `init()`：加载最近卡雅会话和消息
- `newSession()`：创建新会话并切换
- `switchSession(id)`：切换当前会话并加载消息
- `sendMessage(text)`：若当前 `activeReplySessionId` 存在且不等于当前会话，则阻止发送并设置错误提示

- [ ] **步骤 4：运行前端 store 测试和类型检查**

运行：

```bash
cd /kaya/tmp_workplace/kaya-beam/client
npx vitest run src/stores/chat.spec.ts
npx tsc --noEmit
```

预期：PASS。

---

### 任务 6：聊天页改为左侧会话列表 + 右侧消息区

**文件：**
- 修改：`client/src/views/ChatPage.vue`

- [ ] **步骤 1：编写失败测试或最小可验证断言，固定聊天页结构**

如果项目已有 Vue 测试基础设施，则增加组件测试；若没有，则以模板中可 grep 的结构作为最小验证目标，并在本任务里做运行时截图/人工验证。推荐先写结构性测试（若可行）：

```ts
it("renders a session sidebar and chat message area", () => {
  // mount ChatPage and assert sidebar exists
});
```

若当前无组件测试基础设施，则本任务验证改为 `npm run build` + 运行页面人工检查。

- [ ] **步骤 2：运行验证确认当前结构不满足**

运行：

```bash
cd /kaya/tmp_workplace/kaya-beam/client
npx tsc --noEmit
```

预期：当前页面还没有左侧会话列表，因此即便类型检查通过，UI 结构仍需改造。

- [ ] **步骤 3：改造 `ChatPage.vue` 布局和交互**

把现有单列布局改成两栏：

```vue
<div class="chat-layout">
  <aside class="session-sidebar">...</aside>
  <section class="chat-main">...</section>
</div>
```

侧边栏至少包含：

- “新建聊天”按钮
- `v-for="session in chatStore.kayaSessions"`
- 当前会话高亮

主区域仍保留：

- 状态头部
- 消息区
- 输入框

- [ ] **步骤 4：运行前端类型检查 / 构建**

运行：

```bash
npx tsc --noEmit
npm run build
```

预期：PASS。

---

### 任务 7：把文件历史从内存态切到 SQLite

**文件：**
- 修改：`client/src/lib/types.ts`
- 修改：`client/src/lib/tauri.ts`
- 修改：`client/src/stores/file.ts`
- 修改：`client/src/views/FileTransferPage.vue`
- 修改：`client/src/views/HomePage.vue`
- 修改：`client/src-tauri/src/lib.rs`

- [ ] **步骤 1：编写失败测试，固定文件历史加载顺序**

为 `file.ts` 新增最小测试 `client/src/stores/file.spec.ts`：

```ts
import { describe, expect, it } from "vitest";
import { normalizeFileHistory } from "./file";

describe("file history", () => {
  it("keeps newest record first for list views", () => {
    const history = normalizeFileHistory([
      { id: "1", name: "a", size: 1, direction: "received", timestamp: 1, status: "ok" },
      { id: "2", name: "b", size: 1, direction: "sent", timestamp: 2, status: "ok" },
    ]);
    expect(history[0].id).toBe("2");
  });
});
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
npx vitest run src/stores/file.spec.ts
```

预期：FAIL，`normalizeFileHistory` 未定义。

- [ ] **步骤 3：接入文件历史持久化**

在 `tauri.ts` 增加：

```ts
export async function loadFileTransferHistory(): Promise<TransferRecord[]> { ... }
```

在 `file.ts` 中增加：

- `init()`：从 SQLite 加载历史
- `show()`：收到文件后除了更新当前提示，也通过 Tauri command 记录历史，随后刷新/追加内存列表
- `addSent()`：发送文件后也记录历史

在 `FileTransferPage.vue` 中把：

```ts
const history = fileStore.history ?? [];
```

改成直接从 store 响应式使用，并在页面挂载时调用 `fileStore.init()`。

在 `HomePage.vue` 中仍显示最近历史，但不再依赖“只在当前运行期 push 过的内存记录”。

- [ ] **步骤 4：运行文件 store 测试与前端构建**

运行：

```bash
cd /kaya/tmp_workplace/kaya-beam/client
npx vitest run src/stores/file.spec.ts
npm run build
```

预期：PASS。

---

### 任务 8：把 ACP 会话切换和消息归属接入持久层

**文件：**
- 修改：`client/src-tauri/src/lib.rs`
- 修改：`client/src/stores/chat.ts`

- [ ] **步骤 1：编写失败测试，验证新的 ACP session 归属仍留在当前卡雅会话**

优先在 `chat_history.rs` 中增加：

```rust
#[test]
fn creates_new_acp_session_without_creating_new_kaya_session() {
    let db = open_history_db_in_memory().unwrap();
    let kaya = ensure_active_kaya_session(&db).unwrap();
    let first = create_or_switch_acp_session(&db, &kaya.id, "remote-a").unwrap();
    let second = create_or_switch_acp_session(&db, &kaya.id, "remote-b").unwrap();
    let sessions = load_acp_sessions_for_kaya_session(&db, &kaya.id).unwrap();

    assert_ne!(first.id, second.id);
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().all(|s| s.kaya_session_id == kaya.id));
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cargo test chat_history::tests::creates_new_acp_session_without_creating_new_kaya_session
```

预期：FAIL，`load_acp_sessions_for_kaya_session` 未定义或逻辑不完整。

- [ ] **步骤 3：在运行时把 ACP session 事件接入当前卡雅会话**

在 `lib.rs` / 相关 ACP 事件处理路径中：

- 当收到新的 `acp-session` 事件时，为当前活跃卡雅会话创建/绑定新的 ACP 会话记录
- 把 `currentAcpSessionId` 同步给前端 store

在 `chat.ts` 中：

- `acp-session` 事件不再只更新一个裸 `sessionId`
- 需要把它理解为“当前卡雅会话下的技术会话切换”

- [ ] **步骤 4：运行聊天历史测试 + Rust 检查**

运行：

```bash
cargo test chat_history::tests -- --test-threads=1
cargo check
```

预期：PASS。

---

### 任务 9：最终验证与文档同步

**文件：**
- 修改：`docs/protocol.md`（仅当需要补“本地历史不影响协议结构”说明时）

- [ ] **步骤 1：如有必要，更新协议文档中的本地历史说明**

若 `docs/protocol.md` 需要补充，增加一句：

```markdown
客户端本地聊天历史与文件历史属于本地恢复能力，不改变 WebSocket 协议字段与帧顺序。
```

- [ ] **步骤 2：运行最终验证**

运行：

```bash
cd /kaya/tmp_workplace/kaya-beam/server
python3 -m pytest

cd /kaya/tmp_workplace/kaya-beam/client/src-tauri
cargo test chat_history::tests -- --test-threads=1
cargo test file_history::tests -- --test-threads=1
cargo test file_handler::tests -- --test-threads=1
cargo test notify::tests
cargo test ws_client::upload_tests
cargo check

cd /kaya/tmp_workplace/kaya-beam/client
npx vitest run src/stores/chat.spec.ts src/stores/file.spec.ts
npx tsc --noEmit
npm run build
```

预期：

- 服务端：`68 passed` 或更多（允许现有 deprecation warnings）
- Rust：新增历史模块测试 PASS，`cargo check` 无 error
- 前端：store 测试 PASS，类型检查和构建通过

---

## 自检

**规格覆盖度：**
- SQLite 底座与目的型模块：任务 1、2、3、4 覆盖。
- 卡雅会话 / ACP 会话 / 消息模型：任务 1、2、4、8 覆盖。
- 聊天页左侧边栏会话列表：任务 5、6 覆盖。
- 文件传输历史持久化：任务 3、7 覆盖。
- 启动恢复最近卡雅会话与最近 ACP 会话：任务 4、5、8 覆盖。
- 单活跃流式会话约束：任务 5 覆盖。

**占位符扫描：**
- 未使用“待定”“TODO”“后续实现”“添加适当错误处理”等占位表述。
- 每个任务都包含路径、失败测试、运行命令、实现方向和验证命令。

**类型一致性：**
- `KayaSessionRecord` / `AcpSessionRecord` / `ChatMessageRecord` / `FileTransferRecord` 在 Rust 与 TypeScript 层必须统一字段命名。
- 聊天消息排序固定为 `created_at ASC, id ASC`；文件历史排序固定为 `created_at DESC, id DESC`；会话列表按 `updated_at DESC` 组织。
- `currentKayaSessionId` 是前端主会话状态；`currentAcpSessionId` 仅是技术映射，不能反过来主导 UI 会话切换。
