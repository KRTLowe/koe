# 流式文件传输实现计划

> **面向 AI 代理的工作者：** 必需子技能：使用 superpowers:subagent-driven-development（推荐）或 superpowers:executing-plans 逐任务实现此计划。步骤使用复选框（`- [ ]`）语法来跟踪进度。

**目标：** 在不改变现有 WebSocket 文件传输协议消息类型的前提下，把服务端与客户端的文件接收改为临时文件流式落盘，把文件发送改为分块读取和分块发送，避免 500MB 文件全量常驻内存。

**架构：** 保留现有 `file_meta` → binary frame(s) → `file_end` 和 `file_upload_start` → binary frame(s) → `file_upload_end` 协议。客户端接收服务端文件时创建同目录 `.part` 临时文件，binary 帧到达时逐块写入并更新字节数与 SHA256，`file_end` 时校验大小和服务端 checksum 后原子 rename 到最终文件。服务端接收客户端上传时创建同目录 `.part` 临时文件，binary 帧到达时逐块写入并更新字节数，`file_upload_end` 时校验实际字节数等于声明 `size` 后原子 rename 到最终文件；本计划不扩展客户端上传 checksum 字段。发送端从文件描述符循环读取固定大小 chunk，并将每个 chunk 作为 binary frame 发出。

**技术栈：** Python 3.11+ `websockets` / `tempfile` / `hashlib`，Rust Tauri 2 / `tokio-tungstenite` / `std::fs` / `sha2`，pytest，cargo test。

---

## 文件结构

- 修改：`server/src/kaya_server/ws_handler.py`
  - `UploadState` 从 `bytearray` 改为临时文件写入状态。
  - `send_file_to_client()` 从 `fd.read()` 改为循环读取 `FILE_CHUNK_SIZE` 后发送多个 binary frame。
  - 新增小型 helper：上传目录创建、唯一最终路径、临时路径清理。
- 修改：`server/tests/test_ws_handler.py`
  - 增加服务端上传流式写入、上传中断清理、发送文件分块发送测试。
  - 调整现有 `test_send_success`，不再假设只有一个 binary frame。
- 修改：`client/src-tauri/src/file_handler.rs`
  - `FileReceive` 从 `Vec<u8>` 改为 `.part` 文件、最终路径、字节计数、SHA256 状态。
  - 增加 `finalize()` / `abort()` 行为。
  - 保留 `save_file()` 给截图等本地生成小文件使用。
- 修改：`client/src-tauri/src/ws_client.rs`
  - 接收服务端文件时调用 `FileReceive::append_data()` 流式写文件。
  - `file_end` 时调用 `finalize()`，不再把 `Vec<u8>` 发给 runtime。
  - 上传文件时用分块读取发送多个 `Message::Binary(chunk)`，替换两处 `std::fs::read()`。
- 修改：`client/src-tauri/src/ws_runtime.rs`
  - `WsEvent::FileReceived` 从 `{ name, size, data }` 改为 `{ name, size, path }`。
  - 收到事件后直接通知前端，不再调用 `notify::on_file_received()` 进行二次保存。
- 修改：`client/src-tauri/src/notify.rs`
  - 增加 `on_file_saved(app, name, size, path)`，负责 emit 前端事件和聚焦窗口。
  - 保留 `on_file_received(app, name, size, data)` 作为截图/小文件兼容入口，如果仍有调用。
- 检查：`client/src/App.vue`、`client/src/lib/tauri.ts`、`client/src/lib/ws-client.ts`
  - 确认前端 `file-received` 事件已经使用 `{ name, size, path }` 负载，不再依赖 `data`。
- 修改：`docs/protocol.md`
  - 明确 binary 内容可以是一个或多个二进制帧，接收端按 `file_end` / `file_upload_end` 收束。

---

### 任务 1：服务端上传状态改为临时文件流式写入

**文件：**
- 修改：`server/src/kaya_server/ws_handler.py:36-55`
- 修改：`server/tests/test_ws_handler.py:242-270`

- [ ] **步骤 1：编写失败的 UploadState 单元测试**

在 `server/tests/test_ws_handler.py` 追加：

```python
class TestUploadStateStreaming:
    def test_upload_state_streams_chunks_to_temp_file(self, tmp_path):
        import kaya_server.ws_handler as ws_handler

        final_path = tmp_path / "final.txt"
        temp_path = tmp_path / "final.txt.part"
        state = ws_handler.UploadState(
            file_id="up_1",
            name="final.txt",
            size=8,
            temp_path=temp_path,
            final_path=final_path,
        )

        assert state.append(b"abc") is True
        assert state.append(b"defgh") is True
        saved_path = state.finalize()

        assert saved_path == final_path
        assert final_path.read_bytes() == b"abcdefgh"
        assert not temp_path.exists()
        assert state.bytes_received == 8
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cd /tmp/opencode/kaya-beam-security-hardening/server
python3 -m pytest tests/test_ws_handler.py::TestUploadStateStreaming::test_upload_state_streams_chunks_to_temp_file -q
```

预期：FAIL，报错包含 `__init__() got an unexpected keyword argument 'temp_path'`。

- [ ] **步骤 3：实现最少 UploadState 流式写入**

在 `server/src/kaya_server/ws_handler.py` 中把 `UploadState` 改为：

```python
class UploadState:
    SIZE_GRACE = 1024 * 1024

    def __init__(self, file_id: str, name: str, size: int, temp_path: Path, final_path: Path):
        self.file_id = file_id
        self.name = name
        self.size = size
        self.temp_path = temp_path
        self.final_path = final_path
        self.bytes_received = 0
        self._file = temp_path.open("wb")

    def append(self, chunk: bytes) -> bool:
        self._file.write(chunk)
        self.bytes_received += len(chunk)
        return self.bytes_received <= self.size + self.SIZE_GRACE

    @property
    def over_limit(self) -> bool:
        return self.bytes_received > self.size + self.SIZE_GRACE

    def finalize(self) -> Path:
        self._file.close()
        if self.bytes_received != self.size:
            self.temp_path.unlink(missing_ok=True)
            raise ValueError(
                f"File size mismatch: received {self.bytes_received} bytes, "
                f"expected {self.size} bytes"
            )
        self.temp_path.replace(self.final_path)
        return self.final_path

    def abort(self) -> None:
        if not self._file.closed:
            self._file.close()
        self.temp_path.unlink(missing_ok=True)
```

- [ ] **步骤 4：运行测试验证通过**

运行：

```bash
python3 -m pytest tests/test_ws_handler.py::TestUploadStateStreaming::test_upload_state_streams_chunks_to_temp_file -q
```

预期：PASS。

---

### 任务 2：服务端 `_handle_client()` 使用流式 UploadState

**文件：**
- 修改：`server/src/kaya_server/ws_handler.py:188-239`
- 修改：`server/tests/test_ws_handler.py:242-270`

- [ ] **步骤 1：编写失败的集成测试，验证上传不保留内存数据并返回最终路径**

在 `TestFileUploadSecurity` 中追加：

```python
    async def test_upload_streams_to_temp_file_and_returns_final_path(self, handler, db, cm, tmp_path):
        import kaya_server.ws_handler as ws_handler

        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        websocket = FakeWebSocket(
            [
                json.dumps({"type": "auth", "client_id": "pc-01", "passkey": "secret"}),
                json.dumps({
                    "type": "file_upload_start",
                    "file_id": "up_1",
                    "name": "stream.txt",
                    "size": 8,
                }),
                b"abc",
                b"defgh",
                json.dumps({"type": "file_upload_end", "file_id": "up_1"}),
            ]
        )

        with patch.object(ws_handler, "UPLOAD_DIR", str(tmp_path)):
            await handler._handle_client(websocket)

        result = [json.loads(msg) for msg in websocket.sent if json.loads(msg).get("type") == "file_upload_result"][-1]
        saved_path = Path(result["path"])

        assert result["ok"] is True
        assert result["size"] == 8
        assert saved_path.read_bytes() == b"abcdefgh"
        assert not list(tmp_path.rglob("*.part"))

    async def test_upload_size_mismatch_aborts_temp_file(self, handler, db, cm, tmp_path):
        import kaya_server.ws_handler as ws_handler

        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        websocket = FakeWebSocket(
            [
                json.dumps({"type": "auth", "client_id": "pc-01", "passkey": "secret"}),
                json.dumps({
                    "type": "file_upload_start",
                    "file_id": "up_1",
                    "name": "short.txt",
                    "size": 8,
                }),
                b"abc",
                json.dumps({"type": "file_upload_end", "file_id": "up_1"}),
            ]
        )

        with patch.object(ws_handler, "UPLOAD_DIR", str(tmp_path)):
            await handler._handle_client(websocket)

        result = [json.loads(msg) for msg in websocket.sent if json.loads(msg).get("type") == "file_upload_result"][-1]
        assert result["ok"] is False
        assert "size mismatch" in result["error"].lower()
        assert not list(tmp_path.rglob("*.part"))
        assert not list(tmp_path.rglob("short.txt"))
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
python3 -m pytest tests/test_ws_handler.py::TestFileUploadSecurity::test_upload_streams_to_temp_file_and_returns_final_path -q
```

预期：FAIL，第一个测试因当前 `UploadState` 构造参数不匹配或 `finalize()` 未被 `_handle_client()` 调用失败；第二个测试因当前实现会保存短文件而不是拒绝失败。

- [ ] **步骤 3：实现上传路径 helper 并接入 `_handle_client()`**

在 `ws_handler.py` 增加：

```python
def upload_month_dir() -> Path:
    from datetime import datetime

    now = datetime.now()
    date_dir = Path(UPLOAD_DIR) / f"{now.year:04d}-{now.month:02d}"
    date_dir.mkdir(parents=True, exist_ok=True)
    return date_dir


def unique_upload_path(directory: Path, name: str) -> Path:
    from datetime import datetime

    stem, ext = os.path.splitext(name)
    candidate = directory / name
    if not candidate.exists():
        return candidate
    return directory / f"{stem}_{datetime.now().strftime('%Y%m%d%H%M%S')}_{uuid.uuid4().hex[:8]}{ext}"


def upload_temp_path(directory: Path) -> Path:
    return directory / f".upload-{uuid.uuid4().hex}.part"
```

在 `file_upload_start` 分支替换 state 创建：

```python
date_dir = upload_month_dir()
final_path = unique_upload_path(date_dir, name)
temp_path = upload_temp_path(date_dir)
self._uploads[client_id] = UploadState(file_id, name, size, temp_path, final_path)
```

不要把客户端提供的 `file_id` 或原始文件名拼进临时路径；临时文件名必须由服务端用 `uuid.uuid4()` 生成。

在 `file_upload_end` 分支替换写文件逻辑：

```python
try:
    if data.get("file_id") != state.file_id:
        state.abort()
        await websocket.send(json.dumps({
            "type": "file_upload_result", "file_id": data.get("file_id", ""),
            "ok": False, "error": "file_id mismatch",
        }))
        continue

    path = state.finalize()
    await websocket.send(json.dumps({
        "type": "file_upload_result", "file_id": state.file_id,
        "ok": True, "path": str(path), "name": state.name,
        "size": state.bytes_received,
    }))
    logger.info(f"Upload saved: {path} from {client_id}")
except (IOError, ValueError) as e:
    state.abort()
    await websocket.send(json.dumps({
        "type": "file_upload_result", "file_id": state.file_id,
        "ok": False, "error": f"Save failed: {e}",
    }))
```

在 binary 超限分支返回错误并调用 `abort()`：

```python
state = self._uploads.pop(client_id, None)
if state:
    state.abort()
    await websocket.send(json.dumps({
        "type": "file_upload_result", "file_id": state.file_id,
        "ok": False, "error": "Upload exceeded declared size limit",
    }))
break
```

在 `finally` 清理中调用 `abort()`：

```python
state = self._uploads.pop(client_id, None)
if state:
    state.abort()
```

- [ ] **步骤 4：运行服务端上传相关测试**

运行：

```bash
python3 -m pytest tests/test_ws_handler.py::TestFileUploadSecurity tests/test_ws_handler.py::TestUploadStateStreaming -q
```

预期：PASS。

---

### 任务 3：服务端发送文件改为分块读取和分块发送

**文件：**
- 修改：`server/src/kaya_server/ws_handler.py:330-400`
- 修改：`server/tests/test_ws_handler.py:93-129`

- [ ] **步骤 1：编写失败测试，验证大于 chunk 的文件发送为多个 binary frame**

在 `TestSendFile` 中追加：

```python
    async def test_send_file_streams_binary_frames_in_chunks(self, handler, db, cm, tmp_path):
        import kaya_server.ws_handler as ws_handler

        h = hash_passkey("secret")
        db.register_client("pc-01", "Test PC", h)
        mock_ws = AsyncMock()
        cm.register("pc-01", mock_ws)
        test_file = tmp_path / "chunked.bin"
        test_file.write_bytes(b"a" * (ws_handler.FILE_CHUNK_SIZE + 3))

        async def mock_ack(file_id, timeout=30):
            return {"status": "ok"}

        with patch.object(handler, "_wait_for_ack", mock_ack):
            result = await handler.send_file_to_client("pc-01", str(test_file))

        binary_frames = [call.args[0] for call in mock_ws.send.await_args_list if isinstance(call.args[0], bytes)]
        assert result["ok"] is True
        assert [len(frame) for frame in binary_frames] == [ws_handler.FILE_CHUNK_SIZE, 3]
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
python3 -m pytest tests/test_ws_handler.py::TestSendFile::test_send_file_streams_binary_frames_in_chunks -q
```

预期：FAIL，当前没有 `FILE_CHUNK_SIZE` 或只有一个 binary frame。

- [ ] **步骤 3：实现分块发送**

在 `ws_handler.py` 顶部增加：

```python
FILE_CHUNK_SIZE = 1024 * 1024
```

在 `send_file_to_client()` 内重构打开文件后的发送逻辑。不要在 `finally` 中提前关闭 `fd` 后再读取；用 `with fd:` 包住 stat、发送和读取循环。保留原有离线、绝对路径、文件不存在、文件过大、ack 处理和返回结构：

```python
try:
    with fd:
        st_size = os.fstat(fd.fileno()).st_size
        if st_size > MAX_FILE_SIZE:
            return {"ok": False, "error": f"File too large: {st_size} bytes (max {MAX_FILE_SIZE})"}

        file_id = f"f_{uuid.uuid4().hex[:12]}"
        file_name = path.name
        file_size = st_size
        hasher = hashlib.sha256()

        meta = json.dumps({
            "type": "file_meta",
            "file_id": file_id,
            "name": file_name,
            "size": file_size,
        })
        await ws.send(meta)

        while True:
            chunk = fd.read(FILE_CHUNK_SIZE)
            if not chunk:
                break
            hasher.update(chunk)
            await ws.send(chunk)

        end = json.dumps({
            "type": "file_end",
            "file_id": file_id,
            "checksum": f"sha256:{hasher.hexdigest()}",
        })
        await ws.send(end)
except IOError as e:
    return {"ok": False, "error": f"Failed to read file: {e}"}
```

删除旧的 `file_data = fd.read()`、`file_size = len(file_data)`、`checksum = hashlib.sha256(file_data).hexdigest()` 和单帧 `await ws.send(file_data)` 逻辑，后续等待 ack 的代码继续使用上面作用域中定义的 `file_id`、`file_name`、`file_size`。

- [ ] **步骤 4：运行发送文件测试**

运行：

```bash
python3 -m pytest tests/test_ws_handler.py::TestSendFile -q
```

预期：PASS。

---

### 任务 4：客户端 FileReceive 改为临时文件流式接收

**文件：**
- 修改：`client/src-tauri/src/file_handler.rs:1-150`

- [ ] **步骤 1：编写失败测试，验证 FileReceive 落盘和校验**

在 `client/src-tauri/src/file_handler.rs` 的 tests 模块追加：

```rust
    #[test]
    fn file_receive_streams_chunks_to_temp_file_and_finalizes() {
        let home = isolated_home("kaya-file-receive-stream");
        std::env::set_var("HOME", &home);
        std::env::set_var("USERPROFILE", &home);
        let mut receive = FileReceive::new("f_1".into(), "stream.txt".into(), 8).unwrap();

        receive.append_data(b"abc".to_vec()).unwrap();
        receive.append_data(b"defgh".to_vec()).unwrap();
        let path = receive.finalize("sha256:9c56cc51b374c3ba189210d5b6d4bf57790d351c96c47c02190ecf1e430635ab").unwrap();

        assert!(path.starts_with(home.join("kaya-transfer")));
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("stream.txt"));
        assert_eq!(std::fs::read(path).unwrap(), b"abcdefgh");
    }
```

同时把重复的临时 home 构造提取为测试 helper：

```rust
    fn isolated_home(prefix: &str) -> PathBuf {
        let home = std::env::temp_dir().join(format!(
            "{}-{}",
            prefix,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&home).unwrap();
        home
    }
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cd /tmp/opencode/kaya-beam-security-hardening/client/src-tauri
cargo test file_handler::tests::file_receive_streams_chunks_to_temp_file_and_finalizes
```

预期：FAIL，当前 `FileReceive::new()` 不返回 `Result`，且没有 `finalize()`。

- [ ] **步骤 3：实现 FileReceive 流式接收**

先在 `file_handler.rs` 顶部补充写入与哈希所需 import：

```rust
use sha2::Digest;
use std::io::Write;
```

把 `save_file()` 中的同名文件处理逻辑提取为 helper，供 `save_file()` 和 `FileReceive::new()` 复用：

```rust
fn unique_file_path(dir: &Path, safe_name: &str) -> PathBuf {
    let path = dir.join(safe_name);
    if path.exists() {
        let ts = chrono::Local::now().format("%Y%m%d%H%M%S");
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_str().unwrap_or("")))
            .unwrap_or_default();
        dir.join(format!("{}_{}{}", stem, ts, ext))
    } else {
        path
    }
}
```

同步把 `save_file()` 中的路径选择改为：

```rust
let path = unique_file_path(&dir, &safe_name);
```

将 `FileReceive` 改为：

```rust
pub struct FileReceive {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    bytes_received: u64,
    temp_path: PathBuf,
    final_path: PathBuf,
    file: std::fs::File,
    hasher: sha2::Sha256,
}
```

实现签名：

```rust
impl FileReceive {
    pub fn new(file_id: String, name: String, size: u64) -> Result<Self, String>;
    pub fn append_data(&mut self, chunk: Vec<u8>) -> Result<(), String>;
    pub fn finalize(mut self, checksum: &str) -> Result<PathBuf, String>;
    pub fn abort(self);
    pub fn bytes_received(&self) -> u64;
}
```

`new()` 使用 `date_dir()`、`safe_received_filename()`、`unique_file_path()` 和随机临时文件名：

```rust
let dir = date_dir();
let safe_name = safe_received_filename(&name);
let final_path = unique_file_path(&dir, &safe_name);
let temp_path = dir.join(format!(
    ".receive-{}-{}.part",
    std::process::id(),
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
));
let file = std::fs::File::create(&temp_path).map_err(|e| e.to_string())?;
```

不要把服务端提供的 `file_id` 或原始文件名直接拼进临时路径；`safe_name` 只用于最终文件名。

`append_data()` 写入文件并更新 hasher：

```rust
self.file.write_all(&chunk).map_err(|e| e.to_string())?;
self.hasher.update(&chunk);
self.bytes_received += chunk.len() as u64;
Ok(())
```

`finalize()` 校验 size 与 checksum 后 rename：

```rust
self.file.flush().map_err(|e| e.to_string())?;
drop(self.file);
if self.bytes_received != self.size {
    let _ = std::fs::remove_file(&self.temp_path);
    return Err(format!("File size mismatch: received {} bytes, expected {} bytes", self.bytes_received, self.size));
}
let local = format!("sha256:{:x}", self.hasher.finalize());
if !checksum.is_empty() && local != checksum {
    let _ = std::fs::remove_file(&self.temp_path);
    return Err("Checksum mismatch".to_string());
}
std::fs::rename(&self.temp_path, &self.final_path).map_err(|e| e.to_string())?;
Ok(self.final_path)
```

`abort()` 删除临时文件：

```rust
pub fn abort(self) {
    let temp_path = self.temp_path.clone();
    drop(self.file);
    let _ = std::fs::remove_file(temp_path);
}
```

`bytes_received()` 返回当前累计字节数：

```rust
pub fn bytes_received(&self) -> u64 {
    self.bytes_received
}
```

- [ ] **步骤 4：运行 file_handler 测试**

运行：

```bash
cargo test file_handler::tests
```

预期：PASS。

---

### 任务 5：客户端接收路径事件替换内存 data 事件

**文件：**
- 修改：`client/src-tauri/src/ws_client.rs:30-40, 430-487, 602-608`
- 修改：`client/src-tauri/src/ws_runtime.rs:86-95`
- 修改：`client/src-tauri/src/notify.rs:1-43`

- [ ] **步骤 1：编写失败测试，验证 notify 可直接发已保存文件事件**

在 `client/src-tauri/src/notify.rs` 增加一个纯函数 helper：

```rust
pub fn file_received_payload(name: &str, size: u64, path: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "size": size,
        "path": path.to_string_lossy(),
    })
}
```

先写测试：

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_received_payload_contains_saved_path() {
        let payload = file_received_payload("a.txt", 3, std::path::Path::new("/tmp/a.txt"));

        assert_eq!(payload["name"], "a.txt");
        assert_eq!(payload["size"], 3);
        assert_eq!(payload["path"], "/tmp/a.txt");
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cargo test notify::tests::file_received_payload_contains_saved_path
```

预期：FAIL，`file_received_payload` 不存在。

- [ ] **步骤 3：实现事件改造**

在 `ws_client.rs` 中把事件改为：

```rust
FileReceived {
    name: String,
    size: u64,
    path: std::path::PathBuf,
},
```

在 `file_meta` 分支替换 `FileReceive::new(...)` 调用，处理 `Result`：

```rust
match file_handler::FileReceive::new(file_id.clone(), name.clone(), size) {
    Ok(state) => {
        file_receive_state = Some(state);
    }
    Err(error) => {
        log::error!("[WSClient] failed to start file receive: {}", error);
        let _ = event_tx.send(WsEvent::Error(error.clone())).await;
        let _ = write.send(Message::Text(serde_json::json!({
            "type": "file_ack",
            "file_id": file_id,
            "status": "error",
            "error": error,
        }).to_string().into())).await;
    }
}
```

在 `file_end` 分支替换内存校验逻辑：

```rust
match state.finalize(&checksum) {
    Ok(path) => {
        let _ = event_tx.send(WsEvent::FileReceived {
            name: file_name,
            size: file_size,
            path,
        }).await;
        let _ = write.send(Message::Text(serde_json::json!({
            "type": "file_ack",
            "file_id": file_id_s,
            "status": "ok",
        }).to_string().into())).await;
    }
    Err(error) => {
        let _ = event_tx.send(WsEvent::Error(error.clone())).await;
        let _ = write.send(Message::Text(serde_json::json!({
            "type": "file_ack",
            "file_id": file_id_s,
            "status": "error",
            "error": error,
        }).to_string().into())).await;
    }
}
```

在 binary 分支改为：

```rust
if let Some(ref mut state) = file_receive_state {
    if let Err(error) = state.append_data(data) {
        log::error!("[WSClient] failed to append file chunk: {}", error);
    }
}
```

在 `ws_runtime.rs` 中替换：

```rust
WsEvent::FileReceived { name, size, path } => {
    log::info!("[lib] WS event: FileReceived: name={} size={}", name, size);
    notify::on_file_saved(&handle, name, *size, path);
}
```

在 `notify.rs` 增加：

```rust
pub fn on_file_saved(app: &AppHandle, name: &str, size: u64, path: &PathBuf) {
    let _ = app.emit("file-received", file_received_payload(name, size, path));
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
    log::info!("File received: {} ({}) -> {}", name, size, path.display());
}
```

- [ ] **步骤 4：运行相关 Rust 测试**

运行：

```bash
cargo test file_handler::tests notify::tests
cargo check
```

预期：测试 PASS，`cargo check` 无新增错误；现有 warning 不在本任务内处理。

---

### 任务 6：客户端上传改为分块读取和分块发送

**文件：**
- 修改：`client/src-tauri/src/ws_client.rs:217-237, 301-342`

- [ ] **步骤 1：抽取可测试的单 chunk 读取函数并写失败测试**

在 `ws_client.rs` 增加常量和函数：

```rust
const FILE_CHUNK_SIZE: usize = 1024 * 1024;

fn read_next_chunk(file: &mut std::fs::File, chunk_size: usize) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read;

    let mut buf = vec![0_u8; chunk_size];
    let read = file.read(&mut buf).map_err(|e| e.to_string())?;
    if read == 0 {
        return Ok(None);
    }
    buf.truncate(read);
    Ok(Some(buf))
}
```

先写测试：

```rust
#[cfg(test)]
mod upload_tests {
    use super::*;

    #[test]
    fn read_next_chunk_returns_one_chunk_at_a_time() {
        let path = std::env::temp_dir().join(format!(
            "kaya-upload-chunks-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, b"abcdefgh").unwrap();
        let mut file = std::fs::File::open(&path).unwrap();

        assert_eq!(read_next_chunk(&mut file, 3).unwrap(), Some(b"abc".to_vec()));
        assert_eq!(read_next_chunk(&mut file, 3).unwrap(), Some(b"def".to_vec()));
        assert_eq!(read_next_chunk(&mut file, 3).unwrap(), Some(b"gh".to_vec()));
        assert_eq!(read_next_chunk(&mut file, 3).unwrap(), None);
    }
}
```

- [ ] **步骤 2：运行测试验证失败**

运行：

```bash
cargo test ws_client::upload_tests::read_next_chunk_returns_one_chunk_at_a_time
```

预期：FAIL，函数不存在。

- [ ] **步骤 3：接入上传发送路径**

把直接上传分支中的：

```rust
if let Ok(data) = std::fs::read(&req.file_path) {
```

改为先 stat 获取 `size` 并打开文件，成功后再发 start，然后逐次读取一个 chunk 发送；不要收集所有 chunk。注意 `run_client()` 返回 `()`，这里不能使用 `?`：

```rust
let size = match std::fs::metadata(&req.file_path) {
    Ok(meta) => meta.len(),
    Err(e) => {
        log::error!("Upload: failed to stat {}: {}", req.file_path, e);
        continue;
    }
};
let mut file = match std::fs::File::open(&req.file_path) {
    Ok(file) => file,
    Err(e) => {
        log::error!("Upload: failed to open {}: {}", req.file_path, e);
        continue;
    }
};
let start = serde_json::json!({
    "type": "file_upload_start", "file_id": file_id,
    "name": name, "size": size,
});
let _ = write.send(Message::Text(start.to_string())).await;
loop {
    let chunk = match read_next_chunk(&mut file, FILE_CHUNK_SIZE) {
        Ok(Some(chunk)) => chunk,
        Ok(None) => break,
        Err(e) => {
            log::error!("Upload: failed to read {}: {}", req.file_path, e);
            break;
        }
    };
    if write.send(Message::Binary(chunk)).await.is_err() {
        break 'inner;
    }
}
let end = serde_json::json!({
    "type": "file_upload_end", "file_id": file_id,
});
let _ = write.send(Message::Text(end.to_string())).await;
```

对 `result.upload_path` 分支做同样替换，保留 `pending_tool_results.insert(...)` 行为。该分支读文件失败时不能静默跳过，必须发送 `tool_result` error 并触发 `WsEvent::ToolCallCompleted { is_error: true }`，沿用现有 `Ok(Err(e))` 分支的错误响应格式。

- [ ] **步骤 4：运行 upload chunk 测试和 cargo check**

运行：

```bash
cargo test ws_client::upload_tests::read_next_chunk_returns_one_chunk_at_a_time
cargo check
```

预期：测试 PASS，`cargo check` 无新增错误；现有 warning 不在本任务内处理。

---

### 任务 7：确认前端事件负载兼容

**文件：**
- 检查：`client/src/lib/tauri.ts`
- 检查：`client/src/App.vue`
- 检查：`client/src/lib/ws-client.ts`

- [ ] **步骤 1：确认 Tauri 事件监听类型使用 path 字段**

运行：

```bash
cd /tmp/opencode/kaya-beam-security-hardening
grep -R "file-received\|onFileReceived" -n client/src
```

预期：`client/src/lib/tauri.ts` 的 `listen` 类型为：

```ts
listen<{ name: string; size: number; path: string }>("file-received", (e) => {
```

并且回调调用形态为：

```ts
cb(e.payload.name, e.payload.size, e.payload.path);
```

- [ ] **步骤 2：确认 UI 调用方不依赖 data 字段**

检查 `client/src/App.vue`，预期 `onFileReceived` 回调签名使用 `path`：

```ts
unlistenFile = await onFileReceived((name, size, path) => {
```

检查 `client/src/lib/ws-client.ts`，预期 mock/浏览器 fallback 的 callback 也传递 path 字符串，不传递文件 bytes。

- [ ] **步骤 3：如发现前端仍使用 data，改为 path**

如果步骤 1 或步骤 2 发现 `data` 字段依赖，修改对应文件，把 `data` 替换为 `path`，并只展示/记录已保存文件路径，不再尝试在前端保存二进制内容。

预期：全量搜索 `file-received` 和 `onFileReceived` 后，没有任何前端路径依赖 `data` 字段。

---

### 任务 8：更新协议文档并跑最终验证

**文件：**
- 修改：`docs/protocol.md:67-90`

- [ ] **步骤 1：更新协议文本**

把文件推送二进制帧说明从：

```markdown
原始文件字节流。客户端接收到 `file_meta` 后开始组装二进制帧数据，直到收到 `file_end`。
```

改为：

```markdown
原始文件字节流，可由一个或多个二进制帧组成。客户端接收到 `file_meta` 后必须按顺序把所有二进制帧追加到当前文件接收状态，直到收到 `file_end`。接收端不应假设文件内容只在单个二进制帧中出现。
```

在确认段落前增加客户端上传说明：

```markdown
## 文件上传

客户端 → 服务端使用同样的控制帧 + 二进制帧模式：

1. `file_upload_start` 声明 `file_id`、文件名和字节数。
2. 后续一个或多个二进制帧携带文件内容。
3. `file_upload_end` 表示当前上传结束。
4. 服务端返回 `file_upload_result`，其中 `path` 是服务端最终保存路径。
```

- [ ] **步骤 2：运行最终验证**

运行：

```bash
cd /tmp/opencode/kaya-beam-security-hardening/server
python3 -m pytest

cd /tmp/opencode/kaya-beam-security-hardening/client/src-tauri
cargo test file_handler::tests notify::tests ws_client::upload_tests
cargo check

cd /tmp/opencode/kaya-beam-security-hardening
grep -R "file-received\|onFileReceived" -n client/src
```

预期：

- 服务端：`63+ passed`，允许现有 websockets deprecation warnings。
- Rust：相关测试 PASS，`cargo check` 无新增 error，允许现有 unused warnings。
- 前端事件：`file-received` / `onFileReceived` 调用方使用 `{ name, size, path }`，没有恢复对 `data` 字段的依赖。

---

## 自检

**规格覆盖度：**
- 服务端上传流式临时文件：任务 1、2 覆盖。
- 服务端发送分块读取和发送：任务 3 覆盖。
- 客户端接收临时文件流式落盘：任务 4、5 覆盖。
- 客户端上传分块读取和发送：任务 6 覆盖。
- 前端事件负载兼容：任务 7 覆盖。
- 协议兼容文档：任务 8 覆盖。

**占位符扫描：** 未使用“待定”“后续实现”“添加适当错误处理”等占位表达。每个任务都有具体路径、测试代码、实现代码和验证命令。

**类型一致性：** `FileReceive::new()` 在任务 4 改为 `Result<Self, String>`，任务 5 的 `ws_client.rs` 必须使用 `match` 或 `if let Ok(state)` 接入；`WsEvent::FileReceived` 的 `data` 字段在任务 5 改为 `path`，`ws_runtime.rs` 同步调用 `notify::on_file_saved()`。
