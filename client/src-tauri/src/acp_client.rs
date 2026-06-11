use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use std::cell::Cell;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::time::{Duration, Instant};
use url::Url;

/// 从 ACP 消息字段中提取文本内容，兼容三种格式：
/// - 纯字符串: `"Hello!"`
/// - ContentBlock 数组: `[{"type": "text", "text": "Hello!"}]`
/// - 嵌套对象: `{"role": "assistant", "content": [...]}`
fn extract_text_content(val: &Value) -> Option<String> {
    // 纯字符串
    if let Some(s) = val.as_str() {
        return Some(s.to_string());
    }
    // ContentBlock 数组
    if let Some(arr) = val.as_array() {
        let mut parts: Vec<&str> = Vec::new();
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    parts.push(text);
                }
            }
        }
        if !parts.is_empty() {
            return Some(parts.concat());
        }
    }
    // 对象：可能是 { "content": ... } 或 { "role": ..., "content": ... }
    if let Some(obj) = val.as_object() {
        if let Some(content) = obj.get("content") {
            return extract_text_content(content);
        }
    }
    None
}

#[derive(Debug, Clone)]
pub enum AcpEvent {
    Connected,
    Disconnected,
    /// 流式 chunk：透传 thinking 和 text 原始字段，不做拼装
    StreamChunk { response_text: String, response_thinking: String },
    ResponseDone,
    SessionReady { session_id: String },
    Error(String),
}

/// writer 任务消息类型：文本（JSON-RPC）或协议层 Ping
#[derive(Debug, Clone)]
enum WriterMsg {
    Text(String),
    Ping,
}

/// 僵尸检测结果
#[derive(Debug, Clone, Copy, PartialEq)]
enum HealthStatus {
    Healthy,
    Dead,
    Stuck,
    Timeout,
}

type RequestId = u64;

/// 在 &str 的 byte 索引安全截断，保证落在合法字符边界上。
fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() { return s; }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) { end -= 1; }
    &s[..end]
}

const RECONNECT_BASE_DELAY: u64 = 1;
const RECONNECT_MAX_DELAY: u64 = 60;
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

/// session/new 的 mcpServers 参数，当前返回空数组。
/// kaya-transfer-hub 已注册在桥的 opencode.jsonc 全局 mcp 中，所有 session 自动继承。
fn mcp_server_config() -> serde_json::Value {
    serde_json::json!([])
}


pub async fn run_acp_client(
    server_url: String,
    event_tx: mpsc::Sender<AcpEvent>,
    mut msg_rx: mpsc::Receiver<String>,
    acp_cwd: String,
    shared_signal_tx: Arc<Mutex<Option<mpsc::Sender<crate::ws_client::SignalRequest>>>>,
) {
    if let Err(e) = Url::parse(&server_url) {
        log::info!("[ACP] invalid URL: {}", e);
        let _ = event_tx.send(AcpEvent::Error(format!("Invalid URL: {}", e))).await;
        return;
    }

    let mut retry_delay = RECONNECT_BASE_DELAY;
    let mut next_id: RequestId = 1;
    let mut connect_attempt = 0u64;
    let mut session_id: Option<String> = None;

    loop {
        connect_attempt += 1;
        log::info!("[ACP] connection attempt #{} to {} (delay={}s)", connect_attempt, server_url, retry_delay);

        let connect_result = tokio::time::timeout(
            tokio::time::Duration::from_secs(5),
            connect_async(&server_url),
        ).await;

        let (ws_stream, _) = match connect_result {
            Ok(Ok(r)) => {
                log::info!("[ACP] connected on attempt #{}", connect_attempt);
                r
            },
            Ok(Err(e)) => {
                log::info!("[ACP] connection failed: {}", e);
                let _ = event_tx.send(AcpEvent::Error(format!("Connection failed: {}", e))).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
                continue;
            }
            Err(_) => {
                log::info!("[ACP] connection timeout (5s)");
                let _ = event_tx.send(AcpEvent::Error("连接超时（5 秒）".to_string())).await;
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
                retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
                continue;
            }
        };

        log::info!("[ACP] retry_delay reset, attempt count reset");
        retry_delay = RECONNECT_BASE_DELAY;
        connect_attempt = 0;
        let mut initialized = false;
        log::info!("[ACP] connected to {}, sending initialize...", server_url);
        let _ = event_tx.send(AcpEvent::Connected).await;

        let (mut write, mut read) = ws_stream.split();

        let init_id = next_id;
        next_id += 1;
        let init_req = serde_json::json!({
            "jsonrpc": "2.0",
            "id": init_id,
            "method": "initialize",
            "params": {
                "protocolVersion": 1,
                "clientInfo": {
                    "name": "kaya-beam",
                    "version": "0.1.0"
                }
            }
        });
        log::info!("[ACP] sending initialize (id={})", init_id);
        let init_msg = format!("{}\n", init_req.to_string());
        match tokio::time::timeout(WRITE_TIMEOUT, write.send(Message::Text(init_msg.into()))).await {
            Ok(Ok(())) => {}
            _ => {
                log::info!("[ACP] initialize send failed or timed out");
                let _ = event_tx.send(AcpEvent::Disconnected).await;
                continue;
            }
        }

        // ── 专用 writer 任务 ───────────────────────────
        // 将 write 移到独立任务，select loop 不再直接执行 WebSocket 写操作
        let (out_tx, mut out_rx) = tokio::sync::mpsc::unbounded_channel::<WriterMsg>();
        let (writer_done_tx, mut writer_done_rx) = tokio::sync::mpsc::channel::<()>(1);
        tokio::spawn(async move {
            while let Some(msg) = out_rx.recv().await {
                let frame = match msg {
                    WriterMsg::Text(s) => Message::Text(s.into()),
                    WriterMsg::Ping => Message::Ping(vec![].into()),
                };
                if tokio::time::timeout(WRITE_TIMEOUT, write.send(frame)).await.is_err() {
                    log::info!("[ACP] writer: send timed out, exiting");
                    let _ = writer_done_tx.try_send(());
                    break;
                }
            }
            let _ = writer_done_tx.try_send(());
        });

        let mut heartbeat = tokio::time::interval(tokio::time::Duration::from_secs(25));
        heartbeat.reset();

        let mut msg_rx_open = true;
        let mut response_text = String::new();
        let mut response_thinking = String::new();
        let mut last_prompt_id: Option<RequestId> = None;
        let mut response_in_flight = false;

        // 僵尸检测共享状态
        let last_real_activity: Cell<Instant> = Cell::new(Instant::now());
        let last_ping_ok = AtomicBool::new(true);
        let last_pong: Cell<Instant> = Cell::new(Instant::now());
        let health_result: Arc<Mutex<Option<HealthStatus>>> = Arc::new(Mutex::new(None));
        let mut zombie_state: Option<Instant> = None; // 非阻塞僵尸检测：记录检查开始时间

        'inner: loop {
            macro_rules! ws_send {
                ($msg:expr) => {
                    if out_tx.send(WriterMsg::Text($msg)).is_err() {
                        log::info!("[ACP] channel closed, writer task exited");
                        break 'inner;
                    }
                };
            }
            tokio::select! {
                _ = heartbeat.tick() => {
                    // ── 非阻塞僵尸检测 ────────────────────
                    // 如果有正在进行的健康检查，先看结果
                    if let Some(started) = zombie_state {
                        let result = health_result.lock().unwrap().take();
                        if let Some(result) = result {
                            log::info!("[ACP] health check: result={:?}", result);
                            zombie_state = None;
                            match result {
                                HealthStatus::Healthy => {
                                    last_real_activity.set(Instant::now());
                                }
                                HealthStatus::Dead | HealthStatus::Stuck => {
                                    if last_ping_ok.load(Ordering::Relaxed) {
                                        if let Some(ref sid) = session_id {
                                            log::info!("[ACP] zombie: ws alive, recovering in-place");
                                            let cancel = serde_json::json!({
                                                "jsonrpc":"2.0","method":"session/cancel",
                                                "params":{"sessionId":sid}
                                            });
                                            ws_send!(format!("{}\n", cancel));
                                            tokio::time::sleep(Duration::from_millis(300)).await;
                                            let recovery = "[系统通知] 连接中断后已恢复。请用几句话简述你刚才做了什么、得出什么结论、下一步要做什么。";
                                            let recovery_id = next_id; next_id += 1;
                                            let req = serde_json::json!({
                                                "jsonrpc":"2.0","id":recovery_id,"method":"session/prompt",
                                                "params":{"sessionId":sid,"prompt":[{"type":"text","text":recovery}]}
                                            });
                                            response_text.clear();
                                            response_thinking.clear();
                                            last_prompt_id = Some(recovery_id);
                                            response_in_flight = true;
                                            ws_send!(format!("{}\n", req));
                                            log::info!("[ACP] zombie: recovery prompt sent (id={})", recovery_id);
                                            last_real_activity.set(Instant::now());
                                        } else {
                                            break 'inner;
                                        }
                                    } else {
                                        log::info!("[ACP] zombie: ws dead, reconnecting");
                                        break 'inner;
                                    }
                                }
                                HealthStatus::Timeout => {}
                            }
                        } else if started.elapsed() > Duration::from_secs(15) {
                            log::info!("[ACP] health check: timed out after 15s");
                            zombie_state = None;
                            log::info!("[ACP] zombie: health check timeout, reconnecting");
                            break 'inner;
                        }
                        // else: still waiting for health result, continue
                    } else {
                        // 无进行中的检查 → 检测是否僵尸
                        // 仅在有进行中的 prompt 时才检测——空闲挂机不需要健康检查
                        let idle = last_real_activity.get().elapsed();
                        if idle > Duration::from_secs(60) && response_in_flight {
                            log::info!("[ACP] zombie check: idle={}s, sending health check", idle.as_secs());
                            if let Ok(guard) = shared_signal_tx.lock() {
                                if let Some(tx) = guard.as_ref() {
                                    let req = crate::ws_client::SignalRequest {
                                        name: "check_acp_health".into(),
                                        sticky: false,
                                        priority: "high".into(),
                                        notify_once: true,
                                        data: serde_json::json!({"client_side": true}),
                                    };
                                    if tx.try_send(req).is_ok() {
                                        zombie_state = Some(Instant::now());
                                        log::info!("[ACP] health check signal sent");
                                    }
                                }
                            }
                        }
                    }
                    // ── 协议层 Ping ──────────────────────
                    if out_tx.send(WriterMsg::Ping).is_err() {
                        break 'inner;
                    }
                    // 超过 30s 没收到 Pong → 标记协议层已死
                    if last_pong.get().elapsed() > Duration::from_secs(30) {
                        last_ping_ok.store(false, Ordering::Relaxed);
                    }
                }
                user_msg = msg_rx.recv(), if msg_rx_open => {
                    match user_msg {
                        Some(text) => {
                            // HEALTH: 结果处理（来自 check_acp_health 信号的 acp_inject 回复）
                            // 直接就地处理，不等心跳 tick —— 避免 15s 超时在 25s 心跳间隔内先行触发
                            if text.starts_with("HEALTH:") {
                                let status = if text.contains("HEALTH:healthy") {
                                    HealthStatus::Healthy
                                } else if text.contains("HEALTH:dead") {
                                    HealthStatus::Dead
                                } else {
                                    HealthStatus::Stuck
                                };
                                log::info!("[ACP] health check result: {:?} (text={})", status, safe_truncate(&text, 80));
                                match status {
                                    HealthStatus::Healthy => {
                                        last_real_activity.set(Instant::now());
                                        zombie_state = None;
                                    }
                                    HealthStatus::Dead | HealthStatus::Stuck => {
                                        if last_ping_ok.load(Ordering::Relaxed) {
                                            if let Some(ref sid) = session_id {
                                                log::info!("[ACP] zombie: ws alive, recovering in-place");
                                                let cancel = serde_json::json!({
                                                    "jsonrpc":"2.0","method":"session/cancel",
                                                    "params":{"sessionId":sid}
                                                });
                                                ws_send!(format!("{}\n", cancel));
                                                tokio::time::sleep(Duration::from_millis(300)).await;
                                                let recovery = "[系统通知] 连接中断后已恢复。请用几句话简述你刚才做了什么、得出什么结论、下一步要做什么。";
                                                let recovery_id = next_id; next_id += 1;
                                                let req = serde_json::json!({
                                                    "jsonrpc":"2.0","id":recovery_id,"method":"session/prompt",
                                                    "params":{"sessionId":sid,"prompt":[{"type":"text","text":recovery}]}
                                                });
                                                response_text.clear();
                                                response_thinking.clear();
                                                last_prompt_id = Some(recovery_id);
                                                response_in_flight = true;
                                                ws_send!(format!("{}\n", req));
                                                log::info!("[ACP] zombie: recovery prompt sent (id={})", recovery_id);
                                                last_real_activity.set(Instant::now());
                                        } else {
                                            break 'inner;
                                        }
                                        } else {
                                            log::info!("[ACP] zombie: ws dead, reconnecting");
                                            break 'inner;
                                        }
                                        zombie_state = None;
                                    }
                                    HealthStatus::Timeout => {}
                                }
                                continue;
                            }
                            if text == "__cancel__" {
                                response_in_flight = false;
                                if let Some(ref sid) = session_id {
                                    let cancel = serde_json::json!({
                                        "jsonrpc": "2.0",
                                        "method": "session/cancel",
                                        "params": {"sessionId": sid}
                                    });
                                    ws_send!(format!("{}\n", cancel.to_string()));
                                }
                            } else if text == "__new_session__" {
                                let new_id = next_id;
                                next_id += 1;
                                let req = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": new_id,
                                    "method": "session/new",
                                    "params": {"title": "kaya-beam chat", "cwd": acp_cwd, "mcpServers": mcp_server_config()}
                                });
                                ws_send!(format!("{}\n", req.to_string()));
                            } else if let Some(ref sid) = session_id {
                                let prompt_id = next_id;
                                next_id += 1;
                                let text_preview = safe_truncate(&text, 80);
                                log::info!("[ACP] sending session/prompt: id={} sessionId={} preview={}",
                                    prompt_id, sid, text_preview);
                                let req = serde_json::json!({
                                    "jsonrpc": "2.0",
                                    "id": prompt_id,
                                    "method": "session/prompt",
                                    "params": {
                                        "sessionId": sid,
                                        "prompt": [{"type": "text", "text": text}]
                                    }
                                });
                                response_text.clear();
                                response_thinking.clear();
                                last_prompt_id = Some(prompt_id);
                                response_in_flight = true;
                                ws_send!(format!("{}\n", req.to_string()));
                                log::info!("[ACP] session/prompt sent (id={})", prompt_id);
                            } else {
                                let _ = event_tx.try_send(AcpEvent::Error("会话未就绪".to_string()));
                            }
                        }
                        None => { msg_rx_open = false; }
                    }
                }
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            // 使用流式解析处理可能拼接在同一个帧里的多个 JSON 对象
                            let mut deser = serde_json::Deserializer::from_str(&text);
                            while let Ok(val) = Value::deserialize(&mut deser) {
                                if val.get("type").and_then(|t| t.as_str()) == Some("connected") {
                                    continue;
                                }

                                let Some(id) = val.get("id").and_then(|i| i.as_u64()) else {
                                    if let Some(method) = val.get("method").and_then(|m| m.as_str()) {
                                        if method == "session/update" {
                                            last_real_activity.set(Instant::now());
                                            let update = val.get("params").and_then(|p| p.get("update"));
                                            if let Some(update) = update {
                                                let update_type = update.get("sessionUpdate").and_then(|s| s.as_str());
                                                match update_type {
                                                    Some("agent_message_chunk") => {
                                                        let content_type = update.get("content").and_then(|c| c.get("type")).and_then(|t| t.as_str());
                                                        match content_type {
                                                            Some("thinking") => {
                                                                if let Some(text) = update.get("content").and_then(|c| c.get("thinking")).and_then(|t| t.as_str()) {
                                                                    response_thinking.push_str(text);
                                                                }
                                                            }
                                                            _ => {
                                                                // text chunk (default) — also fallback for untyped content
                                                                if let Some(text) = update.get("content").and_then(|c| c.get("text")).and_then(|t| t.as_str()) {
                                                                    response_text.push_str(text);
                                                                }
                                                            }
                                                        }
                                                        // 透传原始字段，不拼装 display
                                                        let _ = event_tx.try_send(AcpEvent::StreamChunk {
                                                            response_text: response_text.clone(),
                                                            response_thinking: response_thinking.clone(),
                                                        });
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                    continue;
                                };

                                if let Some(error) = val.get("error") {
                                    let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("unknown");
                                    let detail = error.get("data").map(|d| d.to_string()).unwrap_or_default();
                                    let err_text = if detail.is_empty() || detail == "null" {
                                        format!("ACP error: {} (req id: {})", msg, id)
                                    } else {
                                        format!("ACP error: {} - {} (req id: {})", msg, detail, id)
                                    };
                                    let _ = event_tx.try_send(AcpEvent::Error(err_text));
                                    // 如果是旧会话失效，清掉等用户 /session new
                                    if msg.contains("Session not found") || msg.contains("sessionId") {
                                        log::info!("[ACP] old session invalid, clearing session_id");
                                        session_id = None;
                                    }
                                    continue;
                                }

                                // 先检查是否 session/prompt 完成（id 匹配 last_prompt_id）
                                // 必须在 result 处理之前检查，避免 prompt 响应被 session/new 分支误判
                                let is_prompt_done = last_prompt_id.map_or(false, |pid| id == pid);
                                if is_prompt_done {
                                    response_in_flight = false;
                                    let _ = event_tx.try_send(AcpEvent::ResponseDone);
                                }

                                if let Some(result) = val.get("result") {
                                    // initialize 响应
                                    if !initialized && id == init_id {
                                        initialized = true;
                                        log::info!("ACP initialized (id={})", id);
                                        if session_id.is_some() {
                                            // 重连 → 复用旧会话（Python 桥不杀 opencode 进程）
                                            log::info!("[ACP] reconnect: reusing session {}", session_id.as_ref().unwrap());
                                            let _ = event_tx.try_send(AcpEvent::SessionReady {
                                                session_id: session_id.clone().unwrap(),
                                            });
                                        } else {
                                            // 首次启动 → 自动创建会话
                                            log::info!("[ACP] first start, creating session");
                                            let new_id = next_id; next_id += 1;
                                            let req = serde_json::json!({
                                                "jsonrpc": "2.0", "id": new_id,
                                                "method": "session/new",
                                                "params": {"title": "kaya-beam chat", "cwd": acp_cwd, "mcpServers": mcp_server_config()}
                                            });
                                            ws_send!(format!("{}\n", req));
                                        }
                                    // session/new 响应 → 取 sessionId
                                    } else if initialized && id != init_id {
                                        if let Some(sid) = result.get("sessionId").and_then(|s| s.as_str()) {
                                        log::info!("ACP session ready: {}", sid);
                                        session_id = Some(sid.to_string());
                                        let _ = event_tx.try_send(AcpEvent::SessionReady {
                                            session_id: sid.to_string(),
                                        });
                                        }
                                    }

                                    // session/prompt 完成的 fallback：有 stopReason 就算完
                                    // 仅在 is_prompt_done 未命中时触发，避免重复
                                    if !is_prompt_done && result.get("stopReason").is_some() {
                                        response_in_flight = false;
                                        log::info!("ACP prompt done (stopReason, id={}, last_id={:?})", id, last_prompt_id);
                                        let _ = event_tx.try_send(AcpEvent::ResponseDone);
                                    }
                                }
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {
                            last_ping_ok.store(true, Ordering::Relaxed);
                            last_pong.set(Instant::now());
                        }
                        Some(Ok(Message::Close(_))) => break 'inner,
                        Some(Err(e)) => {
                            let _ = event_tx.try_send(AcpEvent::Error(format!("WebSocket error: {}", e)));
                            break 'inner;
                        }
                        None => break 'inner,
                        _ => {}
                    }
                }
                _ = writer_done_rx.recv() => {
                    log::info!("[ACP] writer task exited, reconnecting");
                    break 'inner;
                }
            }
        }

        // 丢弃发送端以终止 writer 任务
        drop(out_tx);
        let _ = event_tx.try_send(AcpEvent::Disconnected);
        tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
        retry_delay = (retry_delay * 2).min(RECONNECT_MAX_DELAY);
    }
}
