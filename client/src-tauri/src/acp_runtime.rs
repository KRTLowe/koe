use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use crate::acp_client::{self, AcpEvent};
use crate::bubble::{close_bubble_by_label, create_message_bubble};
use crate::config::AppConfig;
use crate::ws_client::SignalRequest;
use crate::AppState;

pub(crate) fn start_acp_client(
    app: &AppHandle,
    config: &AppConfig,
    shared_signal_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<SignalRequest>>>>,
) {
    use tokio::sync::mpsc;

    let (event_tx, mut event_rx) = mpsc::channel::<AcpEvent>(100);
    let (msg_tx, msg_rx) = mpsc::channel::<String>(100);

    // Save msg_tx to AppState so Tauri commands can send messages
    if let Some(s) = app.try_state::<AppState>() {
        if let Ok(mut tx) = s.acp_tx.lock() {
            *tx = Some(msg_tx);
        }
    }

    let acp_url = acp_url_from_config(config);
    let acp_cwd = config.acp_cwd.clone().unwrap_or_default();

    tauri::async_runtime::spawn(async move {
        acp_client::run_acp_client(acp_url, event_tx, msg_rx, acp_cwd, shared_signal_tx).await;
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match &event {
                AcpEvent::Connected => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.acp_connected.lock() {
                            *st = true;
                        }
                    }
                    let _ = handle.emit("acp-status", serde_json::json!({"status": "已连接"}));
                }
                AcpEvent::Disconnected => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.acp_connected.lock() {
                            *st = false;
                        }
                    }
                    let _ = handle.emit("acp-status", serde_json::json!({"status": "已断开"}));
                }
                AcpEvent::StreamChunk {
                    response_text,
                    response_thinking,
                } => {
                    log::info!(
                        "[ACP][runtime] StreamChunk received: text_len={} thinking_len={} text_preview={} thinking_preview={}",
                        response_text.len(),
                        response_thinking.len(),
                        safe_preview(response_text, 80),
                        safe_preview(response_thinking, 80),
                    );
                    let _ =
                        handle.emit("acp-message", serde_json::json!({"content": response_text}));
                    let state = handle.state::<AppState>();
                    *state.debounce_thinking.lock().unwrap() = response_thinking.clone();
                    *state.debounce_text.lock().unwrap() = response_text.clone();
                    *state.debounce_last.lock().unwrap() = Instant::now();

                    // ── thinking 状态气泡 ──
                    // 只有在 thinking 流有内容、text 流还没开始时才展示
                    if !response_thinking.trim().is_empty() && response_text.trim().is_empty() {
                        let mut label = state.thinking_bubble_label.lock().unwrap();
                        if label.is_none() {
                            let lbl = create_message_bubble(&handle, "Kaya is thinking…");
                            log::info!(
                                "[ACP][runtime] thinking bubble created: label={} content=Kaya is thinking…",
                                lbl,
                            );
                            *label = Some(lbl);
                        } else {
                            log::info!(
                                "[ACP][runtime] thinking bubble already visible: label={}",
                                label.as_deref().unwrap_or("<none>"),
                            );
                        }
                    } else if !response_text.trim().is_empty() {
                        // text 开始输出 → 关闭 thinking 气泡（如果还开着）
                        let mut label = state.thinking_bubble_label.lock().unwrap();
                        if let Some(lbl) = label.take() {
                            log::info!(
                                "[ACP][runtime] thinking bubble closing because text started: label={} text_len={}",
                                lbl,
                                response_text.len(),
                            );
                            drop(label);
                            close_bubble_by_label(&handle, &lbl);
                        } else {
                            log::info!(
                                "[ACP][runtime] no thinking bubble to close; text already present text_len={} thinking_len={}",
                                response_text.len(),
                                response_thinking.len(),
                            );
                        }
                    } else {
                        log::info!(
                            "[ACP][runtime] StreamChunk has neither visible thinking nor text"
                        );
                    }
                }
                AcpEvent::ResponseDone { response_text } => {
                    let state = handle.state::<AppState>();
                    let current_kaya_session_id = state.current_kaya_session_id.lock().unwrap().clone();
                    let remote_session_id = state.session_id.lock().unwrap().clone();

                    if let Some(kaya_session_id) = current_kaya_session_id {
                        match crate::history_db_path(&handle)
                            .and_then(|path| crate::chat_history::open_history_db(&path))
                            .and_then(|db| {
                                let local_acp_session_id = remote_session_id
                                    .as_deref()
                                    .map(|remote| {
                                        crate::chat_history::create_or_switch_acp_session(
                                            &db,
                                            &kaya_session_id,
                                            remote,
                                        )
                                        .map(|record| record.id)
                                    })
                                    .transpose()?;

                                if response_text.trim().is_empty() {
                                    return Ok(());
                                }

                                crate::chat_history::append_chat_message(
                                    &db,
                                    &kaya_session_id,
                                    local_acp_session_id.as_deref(),
                                    "assistant",
                                    &response_text,
                                )
                                .map(|_| ())
                            }) {
                            Ok(()) => log::info!("[ACP][runtime] persisted assistant reply for kaya_session={} len={}", kaya_session_id, response_text.len()),
                            Err(e) => log::error!("[ACP][runtime] failed to persist assistant reply: {}", e),
                        }
                    }

                    let _ = handle.emit("acp-done", serde_json::json!({"done": true}));
                    // 回复完成，立即冲刷去抖缓冲（把 last 设到过去让下一个 tick 直接触发）
                    *state.debounce_last.lock().unwrap() = Instant::now() - Duration::from_secs(10);
                    // 关闭 thinking 气泡（防止只有 thinking 没有 text 的边界情况）
                    let mut label = state.thinking_bubble_label.lock().unwrap();
                    if let Some(lbl) = label.take() {
                        log::info!(
                            "[ACP][runtime] thinking bubble closing on ResponseDone: label={}",
                            lbl,
                        );
                        drop(label);
                        close_bubble_by_label(&handle, &lbl);
                    } else {
                        log::info!("[ACP][runtime] ResponseDone with no thinking bubble visible");
                    }
                }
                AcpEvent::SessionReady { session_id } => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut sid) = s.session_id.lock() {
                            *sid = Some(session_id.clone());
                        }
                    }
                    let _ =
                        handle.emit("acp-session", serde_json::json!({"sessionId": session_id}));
                    // 通过气泡通知新会话已创建，5s 后自动消失
                    create_message_bubble(&handle, "Kalos-lab004 Kaya ONLINE");
                    let h = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        // 找到 session 气泡的 label 并关闭
                        let label = {
                            let state = h.state::<AppState>();
                            let bubbles = state.active_bubbles.lock().unwrap();
                            bubbles
                                .iter()
                                .find(|b| {
                                    state
                                        .bubble_content
                                        .lock()
                                        .unwrap()
                                        .get(&b.label)
                                        .map(|c| c == "Kalos-lab004 Kaya ONLINE")
                                        .unwrap_or(false)
                                })
                                .map(|b| b.label.clone())
                        };
                        if let Some(label) = label {
                            let state = h.state::<AppState>();
                            state.bubble_content.lock().unwrap().remove(&label);
                            state
                                .active_bubbles
                                .lock()
                                .unwrap()
                                .retain(|b| b.label != label);
                            if let Some(win) = h.get_webview_window(&label) {
                                let _ = win.close();
                            }
                        }
                    });
                }
                AcpEvent::Error(e) => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.acp_connected.lock() {
                            *st = false;
                        }
                    }
                    let _ = handle.emit(
                        "acp-status",
                        serde_json::json!({"status": format!("错误: {}", e)}),
                    );
                }
            }
        }
    });
}

pub(crate) fn acp_url_from_config(config: &AppConfig) -> String {
    if let Some(acp_url) = config.acp_url.as_ref().filter(|url| !url.is_empty()) {
        return acp_url.clone();
    }

    if let Some(host) = config
        .server_url
        .strip_prefix("ws://")
        .and_then(host_from_url)
    {
        return format!("ws://{}:8765", host);
    }

    if let Some(host) = config
        .server_url
        .strip_prefix("wss://")
        .and_then(host_from_url)
    {
        return format!("wss://{}:8765", host);
    }

    "ws://127.0.0.1:8765".to_string()
}

fn host_from_url(url_without_scheme: &str) -> Option<&str> {
    url_without_scheme
        .split(':')
        .next()
        .filter(|host| !host.is_empty())
}

fn safe_preview(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn config(server_url: &str, acp_url: Option<&str>) -> AppConfig {
        AppConfig {
            server_url: server_url.to_string(),
            client_id: "client".to_string(),
            passkey: "pass".to_string(),
            acp_url: acp_url.map(str::to_string),
            storage_path: None,
            acp_cwd: None,
            float_image: None,
            allowed_read_paths: vec![],
            allowed_write_paths: vec![],
            denied_extensions: vec![],
            tool_permissions: HashMap::new(),
        }
    }

    #[test]
    fn explicit_acp_url_takes_priority() {
        let cfg = config("ws://server:9765", Some("ws://acp-host:8765"));

        assert_eq!(acp_url_from_config(&cfg), "ws://acp-host:8765");
    }

    #[test]
    fn empty_explicit_acp_url_falls_back_to_server_host() {
        let cfg = config("ws://server:9765", Some(""));

        assert_eq!(acp_url_from_config(&cfg), "ws://server:8765");
    }

    #[test]
    fn derives_acp_url_from_ws_server_host() {
        let cfg = config("ws://10.0.0.2:9765", None);

        assert_eq!(acp_url_from_config(&cfg), "ws://10.0.0.2:8765");
    }

    #[test]
    fn derives_acp_url_from_wss_server_host() {
        let cfg = config("wss://example.test:9765", None);

        assert_eq!(acp_url_from_config(&cfg), "wss://example.test:8765");
    }

    #[test]
    fn unsupported_server_url_uses_localhost_default() {
        let cfg = config("http://example.test:9765", None);

        assert_eq!(acp_url_from_config(&cfg), "ws://127.0.0.1:8765");
    }
}
