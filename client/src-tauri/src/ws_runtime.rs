use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, Manager};

use crate::config::AppConfig;
use crate::notify;
use crate::overlay::show_tool_call_overlay;
use crate::ws_client::{self, SignalRequest, UploadRequest, WsEvent};
use crate::AppState;

pub(crate) fn start_ws_client(
    app: &AppHandle,
    config: AppConfig,
    shared_signal_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<SignalRequest>>>>,
) {
    log::info!(
        "[lib] start_ws_client: server_url={}, client_id={}",
        config.server_url,
        config.client_id
    );
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<WsEvent>(100);
    let (upload_tx, upload_rx) = tokio::sync::mpsc::channel::<UploadRequest>(100);
    let (signal_tx, signal_rx) = tokio::sync::mpsc::channel::<SignalRequest>(100);
    // 写入共享 signal_tx 供 ACP 客户端健康检查使用
    *shared_signal_tx.lock().unwrap() = Some(signal_tx.clone());
    let (re_register_tx, re_register_rx) = tokio::sync::mpsc::channel::<()>(10);

    if let Some(s) = app.try_state::<AppState>() {
        if let Ok(mut tx) = s.upload_tx.lock() {
            *tx = Some(upload_tx);
        }
        if let Ok(mut tx) = s.signal_tx.lock() {
            *tx = Some(signal_tx);
        }
        if let Ok(mut tx) = s.re_register_tx.lock() {
            *tx = Some(re_register_tx);
        }
    }
    log::info!("[lib] start_ws_client: channels created, spawning tasks");

    tauri::async_runtime::spawn(async move {
        ws_client::run_client(config, event_tx, upload_rx, signal_rx, re_register_rx).await;
    });

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        log::info!("[lib] ws event handler started");
        while let Some(event) = event_rx.recv().await {
            match &event {
                WsEvent::Connected => {
                    log::info!("[lib] WS event: Connected");
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.connection_status.lock() {
                            *st = "已连接".to_string();
                        }
                    }
                    let _ = handle.emit(
                        "connection-status",
                        serde_json::json!({ "status": "已连接" }),
                    );
                }
                WsEvent::Disconnected => {
                    log::info!("[lib] WS event: Disconnected");
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.connection_status.lock() {
                            *st = "已断开".to_string();
                        }
                    }
                    let _ = handle.emit(
                        "connection-status",
                        serde_json::json!({ "status": "已断开" }),
                    );
                }
                WsEvent::Error(e) => {
                    log::info!("[lib] WS event: Error: {}", e);
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.connection_status.lock() {
                            *st = format!("错误: {}", e);
                        }
                    }
                    let _ = handle.emit(
                        "connection-status",
                        serde_json::json!({ "status": format!("错误: {}", e) }),
                    );
                }
                WsEvent::FileReceived { name, size, path } => {
                    log::info!("[lib] WS event: FileReceived: name={} size={}", name, size);
                    notify::on_file_saved(&handle, name, *size, path);
                }
                WsEvent::AcpInject { text } => {
                    log::info!("[lib] WS event: AcpInject rcvd, len={}", text.len());
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(tx) = s.acp_tx.lock() {
                            if let Some(tx) = tx.as_ref() {
                                if tx.try_send(text.clone()).is_err() {
                                    log::error!("[lib] Failed to forward ACP inject message");
                                } else {
                                    log::info!("[lib] ACP inject forwarded to acp_tx");
                                }
                            } else {
                                log::info!("[lib] ACP inject dropped: ACP client not started yet");
                            }
                        }
                    }
                }
                WsEvent::ToolCallStarted { name } => {
                    log::info!("[lib] Tool call started: {}", name);
                    show_tool_call_overlay(&handle, "running", name);
                }
                WsEvent::ToolCallCompleted { name, is_error } => {
                    log::info!("[lib] Tool call completed: {} is_error={}", name, is_error);
                    let status = if *is_error { "error" } else { "done" };
                    show_tool_call_overlay(&handle, status, name);
                }
            }
        }
        log::info!("[lib] ws event handler exited");
    });
}
