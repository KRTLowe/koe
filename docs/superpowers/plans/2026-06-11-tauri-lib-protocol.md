# Tauri lib 拆分与协议类型化实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 落地高优先级改进 2 和 3：减轻 `client/src-tauri/src/lib.rs` 职责，并为 Rust WebSocket 客户端引入 typed protocol message 解析。

**架构：** 第一阶段只做低风险切片：新增 `protocol.rs` 解析服务端发给客户端的 JSON 文本帧；新增 `bubble.rs` 承接气泡窗口状态与布局函数。`ws_client.rs` 使用 typed enum 替代裸 `val["type"]` 分发；`lib.rs` 保留 Tauri app 装配与命令注册。

**技术栈：** Rust 2021、Tauri 2、Serde、Tokio、tokio-tungstenite。

---

### 任务 1：协议消息类型化

**文件：**
- 创建：`client/src-tauri/src/protocol.rs`
- 修改：`client/src-tauri/src/lib.rs`
- 修改：`client/src-tauri/src/ws_client.rs`

- [ ] **步骤 1：编写失败的协议解析测试**

在 `protocol.rs` 中先添加 `#[cfg(test)]` 测试，期望 `ClientboundMessage::parse_text()` 能解析 `auth_result`、`file_meta`、`call_tool` 和未知消息。

- [ ] **步骤 2：运行测试验证失败**

运行：`cargo test protocol --lib`
预期：编译失败或测试失败，因为 `ClientboundMessage` 尚未实现。

- [ ] **步骤 3：实现最小协议 enum**

实现 `ClientboundMessage`、`FileUploadResult`、`parse_text()`，未知消息返回 `Unknown { message_type }`，无 `type` 返回 `MissingType`。

- [ ] **步骤 4：替换 `ws_client.rs` 裸 JSON 分发**

把 `serde_json::Value` 的 `val["type"]` 分支替换成 `ClientboundMessage` match，保持原行为和日志语义。

- [ ] **步骤 5：验证协议切片**

运行：`cargo test protocol --lib && cargo check`
预期：测试通过；`cargo check` 只保留既有 warning，不新增错误。

### 任务 2：气泡逻辑拆出 `lib.rs`

**文件：**
- 创建：`client/src-tauri/src/bubble.rs`
- 修改：`client/src-tauri/src/lib.rs`

- [ ] **步骤 1：移动气泡数据结构与函数**

将 `BubbleInfo`、`take_bubble_content`、`anchor_xy`、`reposition_all`、`resize_bubble`、`create_message_bubble`、`close_bubble_by_label` 移到 `bubble.rs`。

- [ ] **步骤 2：暴露必要 app state 字段**

将 `AppState` 及气泡相关字段设为 `pub(crate)`，让 sibling module 可访问；不要扩大到 `pub`。

- [ ] **步骤 3：更新命令注册与调用点**

在 `lib.rs` 中使用 `bubble::create_message_bubble`、`bubble::close_bubble_by_label`、`bubble::resize_bubble`、`bubble::take_bubble_content`。

- [ ] **步骤 4：验证拆分行为不破坏构建**

运行：`cargo check && npm run build`
预期：构建通过。

### 任务 3：最终验证与提交

**文件：**
- 修改：所有上述文件

- [ ] **步骤 1：运行完整验证**

运行：`cargo test --lib && cargo check && npm run build`
预期：全部通过；记录既有 warning。

- [ ] **步骤 2：分提交**

提交 1：`refactor: add typed WebSocket protocol parsing`
提交 2：`refactor: extract Tauri bubble management`
提交 3：`docs: add Tauri refactor implementation plan`
