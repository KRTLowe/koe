# File Transfer Hub 通信协议

## 传输层

WebSocket（ws://），默认端口 9765。

## 消息格式

所有控制消息为 JSON 文本帧，文件内容为二进制帧。

---

## 认证

客户端 → 服务端：

```json
{"type": "auth", "client_id": "pc-01", "passkey": "xxx"}
```

服务端 → 客户端：

```json
{"type": "auth_result", "ok": true}
{"type": "auth_result", "ok": false, "error": "auth failed"}
```

认证失败后服务端断开连接。

---

## 心跳

客户端 → 服务端（每 30 秒）：

```json
{"type": "heartbeat"}
```

服务端 → 客户端：

```json
{"type": "pong"}
```

心跳超时 90 秒未收到 → 服务端标记客户端离线。

---

## 文件推送

服务端 → 客户端（三帧序列）：

### 第 1 帧：元数据（文本帧）

```json
{"type": "file_meta", "file_id": "f_abc123", "name": "screenshot.png", "size": 1048576}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `type` | string | 固定为 `"file_meta"` |
| `file_id` | string | 服务端生成的文件传输 ID（如 `f_abc123`） |
| `name` | string | 文件名（不包含路径） |
| `size` | integer | 文件大小（字节） |

### 第 2 帧：文件内容（二进制帧）

原始文件字节流，可由一个或多个二进制帧组成。客户端接收到 `file_meta` 后必须按顺序把所有二进制帧追加到当前文件接收状态，直到收到 `file_end`。接收端不应假设文件内容只在单个二进制帧中出现。

### 第 3 帧：结束（文本帧）

```json
{"type": "file_end", "file_id": "f_abc123", "checksum": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `type` | string | 固定为 `"file_end"` |
| `file_id` | string | 对应的文件传输 ID |
| `checksum` | string | SHA256 校验和（格式 `sha256:<hex>`） |

### 确认

客户端 → 服务端（可选）：

```json
{"type": "file_ack", "file_id": "f_abc123", "status": "ok"}
```

## 文件上传

客户端 → 服务端使用同样的控制帧 + 二进制帧模式：

1. `file_upload_start` 声明 `file_id`、文件名和字节数。
2. 后续一个或多个二进制帧携带文件内容。
3. `file_upload_end` 表示当前上传结束。
4. 服务端返回 `file_upload_result`，其中 `path` 是服务端最终保存路径。

上传接收端必须按 `file_upload_start` 声明的 `size` 校验实际接收字节数。字节数不匹配时，服务端丢弃临时文件并返回失败结果。

---

## 错误处理

| 场景 | 行为 |
|------|------|
| 未认证消息 | 服务端忽略 |
| 认证失败 | 服务端发送 `auth_result: false` 后断开连接 |
| 心跳超时（90s） | 服务端标记客户端离线 |
| WS 连接断开 | 服务端注销客户端；客户端自动重连（5 秒间隔） |
| 校验和不匹配 | 客户端丢弃文件并记录日志 |
| JSON 解析失败 | 服务端记录警告日志，继续处理下一条消息 |

---

## 状态流转

```
客户端                    服务端
  │                        │
  ├── auth ──────────────► │ 认证
  │ ◄── auth_result ──────┤
  │                        │
  ├── heartbeat (30s) ───► │ 心跳
  │ ◄── pong ─────────────┤
  │                        │
  │ ◄── file_meta ────────┤ 文件推送
  │ ◄── [binary] ─────────┤
  │ ◄── file_end ─────────┤
  ├── file_ack ──────────► │
```

## 安全性说明

- 当前协议使用明文 WebSocket（ws://），认证通过 Bearer 风格的 passkey
- 如需传输加密，建议升级为 wss://（WebSocket over TLS）
- passkey 在服务端以 bcrypt 哈希存储
