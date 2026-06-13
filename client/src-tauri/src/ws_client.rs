use crate::config::AppConfig;
use crate::file_handler;
use crate::protocol::ClientboundMessage;
use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use url::Url;

/// 前端上传请求，通过 Tauri IPC 发送到 ws_client 任务。
#[derive(Debug)]
pub struct UploadRequest {
    pub file_path: String,
}

/// 前端外部信号请求。
#[derive(Debug)]
pub struct SignalRequest {
    pub name: String,
    pub sticky: bool,
    pub priority: String,
    pub notify_once: bool,
    pub data: serde_json::Value,
}

/// WebSocket 事件，用于通知 Tauri 前端
#[derive(Debug, Clone)]
pub enum WsEvent {
    Connected,
    Disconnected,
    FileReceived {
        name: String,
        size: u64,
        path: PathBuf,
    },
    AcpInject {
        text: String,
    },
    Error(String),
    ToolCallStarted {
        name: String,
    },
    ToolCallCompleted {
        name: String,
        is_error: bool,
    },
}

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

    #[test]
    fn upload_heartbeat_is_due_after_interval() {
        let interval = std::time::Duration::from_secs(20);
        let now = std::time::Instant::now();

        assert!(!upload_heartbeat_due(now, now, interval));
        assert!(!upload_heartbeat_due(now + interval - std::time::Duration::from_millis(1), now, interval));
        assert!(upload_heartbeat_due(now + interval, now, interval));
    }
}

/// 指数退避重连延迟（秒）
const RECONNECT_BASE_DELAY: u64 = 1;
const RECONNECT_MAX_DELAY: u64 = 60;
const FILE_CHUNK_SIZE: usize = 1024 * 1024;
const UPLOAD_HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(20);

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

fn upload_heartbeat_due(
    now: std::time::Instant,
    last_sent: std::time::Instant,
    interval: std::time::Duration,
) -> bool {
    now.duration_since(last_sent) >= interval
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// 等待 file_upload_result 后发 tool_result 的上下文。
struct PendingToolResult {
    request_id: String,
    file_name: String,
    tool_name: String,
    local_path: String,
}

/// 在 &str 的 byte 索引安全截断，保证落在合法字符边界上。
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// 运行 WebSocket 客户端
///
/// * `config` - 客户端配置（server_url, client_id, passkey）
/// * `event_tx` - 事件发送通道（向 Tauri 主线程发送事件）
pub async fn run_client(
    config: AppConfig,
    event_tx: mpsc::Sender<WsEvent>,
    mut upload_rx: mpsc::Receiver<UploadRequest>,
    mut signal_rx: mpsc::Receiver<SignalRequest>,
    mut re_register_rx: mpsc::Receiver<()>,
) {
    // 验证 URL 格式
    if let Err(e) = Url::parse(&config.server_url) {
        let _ = event_tx
            .send(WsEvent::Error(format!("Invalid URL: {}", e)))
            .await;
        return;
    }

    let mut retry_delay = RECONNECT_BASE_DELAY;
    let mut connect_attempt = 0u64;

    loop {
        connect_attempt += 1;
        log::info!(
            "[WSClient] connection attempt #{} to {} (retry_delay={}s)",
            connect_attempt,
            config.server_url,
            retry_delay
        );

        let connect_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            connect_async(&config.server_url),
        )
        .await;

        let (ws_stream, _response) = match connect_result {
            Ok(Ok(r)) => {
                log::info!(
                    "[WSClient] connected successfully on attempt #{}",
                    connect_attempt
                );
                r
            }
            Ok(Err(e)) => {
                log::info!(
                    "[WSClient] connection failed: {} (will retry in {}s)",
                    e,
                    retry_delay
                );
                let _ = event_tx
                    .send(WsEvent::Error(format!("Connection failed: {}", e)))
                    .await;
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
                continue;
            }
            Err(_) => {
                log::info!(
                    "[WSClient] connection timeout (5s) after attempt #{}",
                    connect_attempt
                );
                let _ = event_tx
                    .send(WsEvent::Error("连接超时（5 秒），服务器不可达".to_string()))
                    .await;
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
                continue;
            }
        };

        // 连接成功，重置退避
        log::info!(
            "[WSClient] retry_delay reset from {} to {}",
            retry_delay,
            RECONNECT_BASE_DELAY
        );
        retry_delay = RECONNECT_BASE_DELAY;
        connect_attempt = 0;

        let _ = event_tx.send(WsEvent::Connected).await;

        let (mut write, mut read) = ws_stream.split();

        log::info!("[WSClient] sending auth: client_id={}", config.client_id);
        let auth_payload = serde_json::json!({
            "type": "auth",
            "client_id": config.client_id,
            "passkey": config.passkey,
        });
        if write
            .send(Message::Text(auth_payload.to_string()))
            .await
            .is_err()
        {
            log::info!("[WSClient] auth send failed");
            continue;
        }
        log::info!("[WSClient] auth sent, waiting for auth_result...");

        // 协议层 Ping 保活（保持 TCP 连接不闲置断开；ConnectionManager 不走这一层，需要下面的 heartbeat JSON 同步 last_heartbeat）
        let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(25));
        ping_interval.reset();

        let mut file_receive_state: Option<file_handler::FileReceive> = None;
        // file_id → PendingToolResult，等待 upload 完成后再发 tool_result
        let mut pending_tool_results: HashMap<String, PendingToolResult> = HashMap::new();
        // 工具执行结果通道：request_id, tool_name, ToolResult
        let (tool_done_tx, mut tool_done_rx) =
            tokio::sync::mpsc::channel::<(String, String, crate::tool_executor::ToolResult)>(10);

        'inner: loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    // 协议层 Ping（tokio-tungstenite + Python websockets 自动回 Pong）
                    if write.send(Message::Ping(vec![].into())).await.is_err() {
                        log::info!("[WSClient] ping failed, reconnecting");
                        break 'inner;
                    }
                    // 应用层 heartbeat：服务端 ConnectionManager 只认 {"type": "heartbeat"}
                    if write.send(Message::Text(
                        r#"{"type":"heartbeat"}"#.into(),
                    )).await.is_err() {
                        log::info!("[WSClient] heartbeat send failed");
                        break 'inner;
                    }
                }
                upload = upload_rx.recv() => {
                    if let Some(req) = upload {
                        let name = Path::new(&req.file_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file");
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
                        let file_id = format!("up_{:x}", std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default().as_nanos());
                        let start = serde_json::json!({
                            "type": "file_upload_start", "file_id": file_id,
                            "name": name, "size": size,
                        });
                        let _ = write.send(Message::Text(start.to_string())).await;
                        let mut last_upload_heartbeat = std::time::Instant::now();
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
                            let now = std::time::Instant::now();
                            if upload_heartbeat_due(now, last_upload_heartbeat, UPLOAD_HEARTBEAT_INTERVAL) {
                                if write.send(Message::Ping(vec![].into())).await.is_err() {
                                    break 'inner;
                                }
                                if write.send(Message::Text(r#"{"type":"heartbeat"}"#.into())).await.is_err() {
                                    break 'inner;
                                }
                                last_upload_heartbeat = now;
                            }
                        }
                        let end = serde_json::json!({
                            "type": "file_upload_end", "file_id": file_id,
                        });
                        let _ = write.send(Message::Text(end.to_string())).await;
                    } else {
                        // channel closed, stop listening
                        log::info!("[WSClient] upload_rx closed, breaking inner loop");
                        break 'inner;
                    }
                }
                signal = signal_rx.recv() => {
                    if let Some(req) = signal {
                        if req.name == "__copilot_clear__" {
                            if let Some(signal_name) = req.data.get("clear_signal").and_then(|v| v.as_str()) {
                                let clear_msg = serde_json::json!({
                                    "type": "signal_clear",
                                    "name": signal_name,
                                });
                                log::info!("Signal clear: {}", signal_name);
                                let _ = write.send(Message::Text(clear_msg.to_string())).await;
                            }
                            continue;
                        }
                        let msg = serde_json::json!({
                            "type": "signal",
                            "name": req.name,
                            "sticky": req.sticky,
                            "priority": req.priority,
                            "notify_once": req.notify_once,
                            "data": req.data,
                        });
                        log::info!("Signal triggered: {} (sticky={}, priority={})", req.name, req.sticky, req.priority);
                        let _ = write.send(Message::Text(msg.to_string())).await;
                    } else {
                        log::info!("[WSClient] signal_rx closed, breaking inner loop");
                        break 'inner;
                    }
                }
                Some(()) = re_register_rx.recv() => {
                    log::info!("Re-registering tools after permission change");
                    let defs = crate::tool_manager_defs();
                    let tools_msg = serde_json::json!({
                        "type": "register_tools",
                        "tools": defs.iter().map(|t| serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                            "inputSchema": t.input_schema,
                        })).collect::<Vec<_>>(),
                    });
                    let _ = write.send(Message::Text(tools_msg.to_string())).await;
                }
                Some((request_id, tool_name, result)) = tool_done_rx.recv() => {
                    if result.is_error {
                        let content_text: String = result.content.iter()
                            .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                            .collect::<Vec<_>>()
                            .join(" | ");
                        log::info!("[WSClient] >>> sending error tool_result for request_id={}: {}",
                            request_id, content_text);
                        log::debug!("[WSClient] >>> error tool_result full content: {:?}", result.content);
                        let response = serde_json::json!({
                            "type": "tool_result",
                            "request_id": &request_id,
                            "content": result.content,
                            "is_error": true,
                        });
                        let _ = write.send(Message::Text(response.to_string())).await;
                        let _ = event_tx.send(WsEvent::ToolCallCompleted {
                            name: tool_name,
                            is_error: true,
                        }).await;
                    } else if let Some(ref upload_path) = result.upload_path {
                        log::info!("[WSClient] >>> upload path present: {} (will upload before tool_result)", upload_path);
                        let name = std::path::Path::new(upload_path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("screenshot.png");
                        let size = match std::fs::metadata(upload_path) {
                            Ok(meta) => meta.len(),
                            Err(e) => {
                                log::info!("[WSClient] stat upload file failed: {}", e);
                                let response = serde_json::json!({
                                    "type": "tool_result",
                                    "request_id": &request_id,
                                    "content": [{
                                        "type": "text",
                                        "text": format!("❌ 读取文件失败: {}", e),
                                    }],
                                    "is_error": true,
                                });
                                let _ = write.send(Message::Text(response.to_string())).await;
                                let _ = event_tx.send(WsEvent::ToolCallCompleted {
                                    name: tool_name,
                                    is_error: true,
                                }).await;
                                continue;
                            }
                        };
                        let mut file = match std::fs::File::open(upload_path) {
                            Ok(file) => file,
                            Err(e) => {
                                log::info!("[WSClient] read upload file failed: {}", e);
                                let response = serde_json::json!({
                                    "type": "tool_result",
                                    "request_id": &request_id,
                                    "content": [{
                                        "type": "text",
                                        "text": format!("❌ 读取文件失败: {}", e),
                                    }],
                                    "is_error": true,
                                });
                                let _ = write.send(Message::Text(response.to_string())).await;
                                let _ = event_tx.send(WsEvent::ToolCallCompleted {
                                    name: tool_name,
                                    is_error: true,
                                }).await;
                                continue;
                            }
                        };
                                let file_id = format!("up_{:x}", std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default().as_nanos());
                                log::info!("[WSClient] uploading {} bytes as file_id={}", size, file_id);

                                let start_upload = std::time::Instant::now();
                                let start = serde_json::json!({
                                    "type": "file_upload_start",
                                    "file_id": file_id,
                                    "name": name,
                                    "size": size,
                                });
                                let _ = write.send(Message::Text(start.to_string())).await;
                                let mut last_upload_heartbeat = std::time::Instant::now();
                                loop {
                                    let chunk = match read_next_chunk(&mut file, FILE_CHUNK_SIZE) {
                                        Ok(Some(chunk)) => chunk,
                                        Ok(None) => break,
                                        Err(e) => {
                                            log::info!("[WSClient] read upload chunk failed: {}", e);
                                            let response = serde_json::json!({
                                                "type": "tool_result",
                                                "request_id": &request_id,
                                                "content": [{
                                                    "type": "text",
                                                    "text": format!("❌ 读取文件失败: {}", e),
                                                }],
                                                "is_error": true,
                                            });
                                            let _ = write.send(Message::Text(response.to_string())).await;
                                            let _ = event_tx.send(WsEvent::ToolCallCompleted {
                                                name: tool_name,
                                                is_error: true,
                                            }).await;
                                            continue 'inner;
                                        }
                                    };
                                    if write.send(Message::Binary(chunk)).await.is_err() {
                                        break 'inner;
                                    }
                                    let now = std::time::Instant::now();
                                    if upload_heartbeat_due(now, last_upload_heartbeat, UPLOAD_HEARTBEAT_INTERVAL) {
                                        if write.send(Message::Ping(vec![].into())).await.is_err() {
                                            break 'inner;
                                        }
                                        if write.send(Message::Text(r#"{"type":"heartbeat"}"#.into())).await.is_err() {
                                            break 'inner;
                                        }
                                        last_upload_heartbeat = now;
                                    }
                                }
                                let end = serde_json::json!({
                                    "type": "file_upload_end",
                                    "file_id": file_id,
                                });
                                let _ = write.send(Message::Text(end.to_string())).await;
                                log::info!("[WSClient] upload frames sent in {:?}, waiting for file_upload_result...",
                                    start_upload.elapsed());

                                pending_tool_results.insert(file_id, PendingToolResult {
                                    request_id,
                                    file_name: name.to_string(),
                                    tool_name: tool_name.clone(),
                                    local_path: upload_path.to_string(),
                                });
                    } else {
                        log::info!("[WSClient] >>> sending direct tool_result for request_id={}", request_id);
                        let response = serde_json::json!({
                            "type": "tool_result",
                            "request_id": &request_id,
                            "content": result.content,
                            "is_error": false,
                        });
                        let _ = write.send(Message::Text(response.to_string())).await;
                        let _ = event_tx.send(WsEvent::ToolCallCompleted {
                            name: tool_name.clone(),
                            is_error: false,
                        }).await;
                        // 工具执行成功后自动发送关联信号
                        if let Some(signal) = crate::signal_emitter::signal_for_tool(&tool_name) {
                            log::info!("[WSClient] auto-signal: {} (tool={})", signal.name(), tool_name);
                            let _ = write.send(Message::Text(signal.to_ws_message())).await;
                        }
                    }
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            log::info!("[WSClient] received text frame: len={}, preview={}",
                                text.len(), safe_truncate(&text, 120));
                            if let Ok(message) = ClientboundMessage::parse_text(&text) {
                                match message {
                                    ClientboundMessage::AuthResult { ok, error } => {
                                        if ok {
                                            log::info!("[WSClient] auth_result: ok=true, registering tools...");
                                            let defs = crate::tool_manager_defs();
                                            log::info!("[WSClient] registering {} tools", defs.len());
                                            let tools_msg = serde_json::json!({
                                                "type": "register_tools",
                                                "tools": defs.iter().map(|t| serde_json::json!({
                                                    "name": t.name,
                                                    "description": t.description,
                                                    "inputSchema": t.input_schema,
                                                })).collect::<Vec<_>>(),
                                            });
                                            let _ = write.send(Message::Text(tools_msg.to_string())).await;
                                            log::info!("[WSClient] register_tools sent");
                                        } else {
                                            let err = error.unwrap_or_else(|| "unknown".to_string());
                                            log::info!("[WSClient] auth_result: ok=false, error={}", err);
                                            let _ = event_tx
                                                .send(WsEvent::Error(format!("Auth failed: {}", err)))
                                                .await;
                                            break 'inner;
                                        }
                                    }
                                    ClientboundMessage::Pong => {} // 心跳响应
                                    ClientboundMessage::FileMeta { file_id, name, size } => {
                                        log::info!("[WSClient] file_meta: id={} name={} size={}", file_id, name, size);
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
                                    }
                                    ClientboundMessage::FileEnd { file_id, checksum } => {
                                        log::info!("[WSClient] file_end received, processing...");
                                        if let Some(state) = file_receive_state.take() {
                                            let file_id_s = file_id;
                                            let received = state.bytes_received();
                                            let file_name = state.name.clone();
                                            let file_size = state.size;
                                            log::info!("[WSClient] file_end: id={} name={} received={} expected={}",
                                                file_id_s, file_name, received, file_size);
                                            match state.finalize(&checksum) {
                                                Ok(path) => {
                                                    log::info!("[WSClient] file received OK, checksum verified, forwarding to frontend");
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
                                                    log::info!("[WSClient] file finalize failed: {}", error);
                                                    let _ = event_tx.send(WsEvent::Error(error.clone())).await;
                                                    let _ = write.send(Message::Text(serde_json::json!({
                                                        "type": "file_ack",
                                                        "file_id": file_id_s,
                                                        "status": "error",
                                                        "error": error,
                                                    }).to_string().into())).await;
                                                }
                                            }
                                        } else {
                                            log::info!("[WSClient] file_end but no receive state");
                                        }
                                    }
                                    ClientboundMessage::RegisterToolsResult { registered } => {
                                        log::info!("[WSClient] register_tools_result: {} tools accepted", registered);
                                    }
                                    ClientboundMessage::FileUploadStartAck => {
                                        // 服务端确认上传开始，等待 file_upload_result
                                    }
                                    ClientboundMessage::FileUploadResult { file_id, ok, path, error } => {
                                        log::info!("[WSClient] file_upload_result: id={} ok={}", file_id, ok);
                                        if let Some(pending) = pending_tool_results.remove(&file_id) {
                                            let display_path = if ok {
                                                path.as_deref().unwrap_or(&pending.file_name)
                                            } else {
                                                &pending.file_name
                                            };
                                            log::info!("[WSClient] sending tool_result after upload: request_id={} ok={} path={}",
                                                pending.request_id, ok, display_path);
                                            let tool_label = match pending.tool_name.as_str() {
                                                "take_screenshot" => "截图",
                                                "pull_file" => "文件",
                                                _ => "文件",
                                            };
                                            let response = serde_json::json!({
                                                "type": "tool_result",
                                                "request_id": pending.request_id,
                                                "content": [{
                                                    "type": "text",
                                                    "text": format!(
                                                        "{}已保存到本地: {}\n已上传到服务端: {}",
                                                        tool_label, pending.local_path, display_path,
                                                    ),
                                                }],
                                                "is_error": !ok,
                                            });
                                            let _ = write.send(Message::Text(response.to_string())).await;
                                            let _ = event_tx.send(WsEvent::ToolCallCompleted {
                                                name: pending.tool_name.clone(),
                                                is_error: !ok,
                                            }).await;
                                            // 上传成功后自动发送关联信号
                                            if ok {
                                                if let Some(signal) = crate::signal_emitter::signal_for_tool(&pending.tool_name) {
                                                    log::info!("[WSClient] auto-signal after upload: {} (tool={})", signal.name(), pending.tool_name);
                                                    let _ = write.send(Message::Text(signal.to_ws_message())).await;
                                                }
                                            }
                                            log::info!("[WSClient] tool_result sent after upload");
                                        } else {
                                            if ok {
                                                log::info!("[WSClient] upload complete (no pending tool): path={}", path.unwrap_or_default());
                                            } else {
                                                let err = error.unwrap_or_else(|| "unknown".to_string());
                                                log::info!("[WSClient] upload failed: {}", err);
                                            }
                                        }
                                    }
                                    ClientboundMessage::CallTool { request_id, name, arguments } => {
                                        let tool_name = name;
                                        let args = arguments;
                                        log::info!("[WSClient] <<< call_tool: request_id={} tool={}", request_id, tool_name);
                                        log::debug!("[WSClient] <<< call_tool args: {}", args);

                                        let _ = event_tx.send(WsEvent::ToolCallStarted {
                                            name: tool_name.clone(),
                                        }).await;

                                        // 把耗时的 execute_tool 移到 spawn_blocking 中，
                                        // 避免阻塞 select loop 导致心跳超时。
                                        let tool_done = tool_done_tx.clone();
                                        let tn = tool_name.clone();
                                        let a = args.clone();
                                        let tn_for_blocking = tn.clone();
                                        tokio::spawn(async move {
                                            let start_exec = std::time::Instant::now();
                                            let result = tokio::task::spawn_blocking(move || {
                                                crate::tool_executor::execute_tool(&tn_for_blocking, &a)
                                            }).await.unwrap_or_else(|e| {
                                                crate::tool_executor::ToolResult::err(
                                                    format!("Tool execution panicked: {}", e),
                                                )
                                            });
                                            log::info!("[WSClient] execute_tool done: elapsed={:?}, is_error={}, has_upload={}",
                                                start_exec.elapsed(), result.is_error, result.upload_path.is_some());
                                            let _ = tool_done.send((request_id, tn, result)).await;
                                        });
                                    }
                                    ClientboundMessage::AcpInject { text } => {
                                        if !text.is_empty() {
                                            let text_preview = safe_truncate(&text, 80);
                                            log::info!("[WSClient] acp_inject: preview={}", text_preview);
                                            if event_tx.try_send(WsEvent::AcpInject {
                                                text,
                                            }).is_err() {
                                                log::error!("[WSClient] acp_inject: event channel full or closed");
                                            }
                                        }
                                    }
                                    ClientboundMessage::SignalAck => {
                                        // 服务端确认收到信号，不需要额外处理
                                    }
                                    ClientboundMessage::Unknown { message_type } => {
                                        log::info!("[WSClient] unknown message type: {}", message_type);
                                    }
                                    ClientboundMessage::MissingType => {
                                        log::info!("[WSClient] message without type field");
                                    }
                                }
                            } else {
                                log::info!("[WSClient] failed to parse JSON: preview={}", safe_truncate(&text, 80));
                            }
                        }
                        Some(Ok(Message::Binary(data))) => {
                            log::info!("[WSClient] binary frame: {} bytes", data.len());
                            if let Some(ref mut state) = file_receive_state {
                                if let Err(error) = state.append_data(data) {
                                    log::error!("[WSClient] failed to append file chunk: {}", error);
                                }
                            } else {
                                log::info!("[WSClient] binary frame but no file_receive_state");
                            }
                        }
                        Some(Ok(Message::Close(_))) => {
                            log::info!("[WSClient] connection closed by server (Close frame)");
                            break 'inner;
                        }
                        Some(Err(e)) => {
                            log::info!("[WSClient] WebSocket error: {}", e);
                            let _ = event_tx
                                .send(WsEvent::Error(format!("WebSocket error: {}", e)))
                                .await;
                            break 'inner;
                        }
                        None => {
                            log::info!("[WSClient] read stream ended (None)");
                            break 'inner;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            // 必须回复 Pong，否则服务端 ping_timeout=10 后会断开连接
                            let _ = write.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Pong(_))) => {
                            // WebSocket 协议层 Pong 响应
                        }
                        _ => {
                            log::info!("[WSClient] unexpected message type");
                        }
                    }
                }

            }
        }

        log::info!("[WSClient] inner loop exited, sending Disconnected event");
        let _ = event_tx.send(WsEvent::Disconnected).await;

        log::info!(
            "[WSClient] reconnecting in {}s (max delay {}s)",
            retry_delay,
            RECONNECT_MAX_DELAY
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
        retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
        log::info!("[WSClient] next retry_delay={}s", retry_delay);
    }
}
