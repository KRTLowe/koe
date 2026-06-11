mod config;
mod copilot;
mod file_handler;
mod uia_tree;
mod notify;
mod tray;
mod ws_client;
mod acp_client;
mod tool_executor;
mod signal_emitter;
mod protocol;
mod bubble;
mod overlay;
mod ws_runtime;
mod tools;

use bubble::{
    close_bubble_by_label, create_message_bubble, resize_bubble, take_bubble_content, BubbleInfo,
};
use config::{AppConfig, load_config as load_config_impl, save_config as save_config_impl};
use overlay::{
    close_tool_call_overlay, copilot_close, copilot_enter_monitor, quick_chat_close,
    toggle_copilot_window, toggle_quick_chat,
};
use std::sync::{Arc, Mutex};
use std::sync::OnceLock;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Emitter, Manager,
    PhysicalPosition, PhysicalSize,
    WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tools::ToolManager;
use ws_runtime::start_ws_client;

pub(crate) static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

/// ws_client.rs 等模块通过此函数获取当前已启用的工具定义列表
pub(crate) fn tool_manager_defs() -> Vec<ws_client::ToolDef> {
    if let Some(app) = APP_HANDLE.get() {
        if let Some(state) = app.try_state::<AppState>() {
            if let Ok(mgr) = state.tool_manager.lock() {
                if let Some(ref mgr) = *mgr {
                    let defs = mgr.enabled_defs();
                    log::info!("[lib] tool_manager_defs: returning {} tool defs", defs.len());
                    return defs;
                } else {
                    log::info!("[lib] tool_manager_defs: ToolManager not initialized");
                }
            } else {
                log::info!("[lib] tool_manager_defs: mutex poisoned");
            }
        } else {
            log::info!("[lib] tool_manager_defs: AppState not available");
        }
    } else {
        log::info!("[lib] tool_manager_defs: APP_HANDLE not set");
    }
    vec![]
}

pub(crate) struct AppState {
    pub(crate) config: Mutex<Option<AppConfig>>,
    pub(crate) connection_status: Mutex<String>,
    pub(crate) ws_started: Mutex<bool>,
    pub(crate) acp_started: Mutex<bool>,
    pub(crate) acp_tx: Mutex<Option<tokio::sync::mpsc::Sender<String>>>,
    pub(crate) acp_connected: Mutex<bool>,
    pub(crate) session_id: Mutex<Option<String>>,
    pub(crate) upload_tx: Mutex<Option<tokio::sync::mpsc::Sender<ws_client::UploadRequest>>>,
    pub(crate) signal_tx: Mutex<Option<tokio::sync::mpsc::Sender<ws_client::SignalRequest>>>,
    pub(crate) tool_manager: Mutex<Option<ToolManager>>,
    pub(crate) re_register_tx: Mutex<Option<tokio::sync::mpsc::Sender<()>>>,
    /// 气泡序号计数器
    pub(crate) bubble_seq: Mutex<u64>,
    /// 活跃气泡栈（最新的在末尾）
    pub(crate) active_bubbles: Mutex<Vec<BubbleInfo>>,
    /// 最后一条消息时间（用于 30s 超时清理）
    pub(crate) last_msg_time: Mutex<Option<Instant>>,
    /// 待取走的气泡内容 label → text
    pub(crate) bubble_content: Mutex<HashMap<String, String>>,
    /// 去抖累积：thinking 原始字段（透传，不做拼装）
    pub(crate) debounce_thinking: Mutex<String>,
    /// 去抖累积：text 原始字段
    pub(crate) debounce_text: Mutex<String>,
    /// 上次收到 chunk 的时间
    pub(crate) debounce_last: Mutex<Instant>,
    /// 已通过气泡展示的 display 文本（用于前缀 diff）
    pub(crate) displayed: Mutex<String>,
    /// "Kaya is thinking…" 气泡的 label，文字到达时关闭
    pub(crate) thinking_bubble_label: Mutex<Option<String>>,
}

/// 从配置获取 ACP 桥接地址，优先使用独立配置
fn acp_url_from_config(config: &AppConfig) -> String {
    if let Some(ref acp_url) = config.acp_url {
        if !acp_url.is_empty() {
            return acp_url.clone();
        }
    }
    // 回退：从 server_url 推导
    if let Some(rest) = config.server_url.strip_prefix("ws://") {
        if let Some(host) = rest.split(':').next() {
            return format!("ws://{}:8765", host);
        }
    }
    "ws://127.0.0.1:8765".to_string()
}

fn start_acp_client(app: &AppHandle, config: &AppConfig, shared_signal_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<ws_client::SignalRequest>>>>) {
    use tokio::sync::mpsc;

    let (event_tx, mut event_rx) = mpsc::channel::<acp_client::AcpEvent>(100);
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
                acp_client::AcpEvent::Connected => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.acp_connected.lock() {
                            *st = true;
                        }
                    }
                    let _ = handle.emit("acp-status", serde_json::json!({"status": "已连接"}));
                }
                acp_client::AcpEvent::Disconnected => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.acp_connected.lock() {
                            *st = false;
                        }
                    }
                    let _ = handle.emit("acp-status", serde_json::json!({"status": "已断开"}));
                }
                acp_client::AcpEvent::StreamChunk { response_text, response_thinking } => {
                    let _ = handle.emit("acp-message", serde_json::json!({"content": response_text}));
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
                            *label = Some(lbl);
                        }
                    } else if !response_text.trim().is_empty() {
                        // text 开始输出 → 关闭 thinking 气泡（如果还开着）
                        let mut label = state.thinking_bubble_label.lock().unwrap();
                        if let Some(lbl) = label.take() {
                            drop(label);
                            close_bubble_by_label(&handle, &lbl);
                        }
                    }
                }
                acp_client::AcpEvent::ResponseDone => {
                    let _ = handle.emit("acp-done", serde_json::json!({"done": true}));
                    // 回复完成，立即冲刷去抖缓冲（把 last 设到过去让下一个 tick 直接触发）
                    let state = handle.state::<AppState>();
                    *state.debounce_last.lock().unwrap() = Instant::now() - Duration::from_secs(10);
                    // 关闭 thinking 气泡（防止只有 thinking 没有 text 的边界情况）
                    let mut label = state.thinking_bubble_label.lock().unwrap();
                    if let Some(lbl) = label.take() {
                        drop(label);
                        close_bubble_by_label(&handle, &lbl);
                    }
                }
                acp_client::AcpEvent::SessionReady { session_id } => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut sid) = s.session_id.lock() {
                            *sid = Some(session_id.clone());
                        }
                    }
                    let _ = handle.emit("acp-session", serde_json::json!({"sessionId": session_id}));
                    // 通过气泡通知新会话已创建，5s 后自动消失
                    create_message_bubble(&handle, "Kalos-lab004 Kaya ONLINE");
                    let h = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        // 找到 session 气泡的 label 并关闭
                        let label = {
                            let state = h.state::<AppState>();
                            let bubbles = state.active_bubbles.lock().unwrap();
                            bubbles.iter()
                                .find(|b| {
                                    state.bubble_content.lock().unwrap()
                                        .get(&b.label)
                                        .map(|c| c == "Kalos-lab004 Kaya ONLINE")
                                        .unwrap_or(false)
                                })
                                .map(|b| b.label.clone())
                        };
                        if let Some(label) = label {
                            let state = h.state::<AppState>();
                            state.bubble_content.lock().unwrap().remove(&label);
                            state.active_bubbles.lock().unwrap().retain(|b| b.label != label);
                            if let Some(win) = h.get_webview_window(&label) {
                                let _ = win.close();
                            }
                        }
                    });
                }
                acp_client::AcpEvent::Error(e) => {
                    if let Some(s) = handle.try_state::<AppState>() {
                        if let Ok(mut st) = s.acp_connected.lock() {
                            *st = false;
                        }
                    }
                    let _ = handle.emit("acp-status", serde_json::json!({"status": format!("错误: {}", e)}));
                }
            }
        }
    });
}

#[tauri::command]
fn load_config(state: tauri::State<AppState>, app: tauri::AppHandle) -> Result<Option<AppConfig>, String> {
    let cfg = load_config_impl(&app);
    *state.config.lock().map_err(|e| e.to_string())? = cfg.clone();
    Ok(cfg)
}

#[tauri::command]
fn save_config(
    config: AppConfig,
    state: tauri::State<AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    if !config.is_valid() {
        return Err("配置不完整：请填写 server_url、client_id、passkey".to_string());
    }
    save_config_impl(&app, &config)?;
    let mut cfg = state.config.lock().map_err(|e| e.to_string())?;
    let had_config = cfg.is_some();
    *cfg = Some(config.clone());
    drop(cfg);
    // 首次保存配置时启动 WS 客户端和 ACP 客户端
    if !had_config {
        let mut ws_started = state.ws_started.lock().map_err(|e| e.to_string())?;
        if !*ws_started {
            *ws_started = true;
            drop(ws_started);
            *state.acp_started.lock().map_err(|e| e.to_string())? = true;
            let shared_signal_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<ws_client::SignalRequest>>>> = Arc::new(Mutex::new(None));
            start_acp_client(&app, &config, shared_signal_tx.clone());
            start_ws_client(&app, config, shared_signal_tx);
        }
    }
    Ok(())
}

#[tauri::command]
fn get_connection_status(state: tauri::State<AppState>) -> Result<String, String> {
    Ok(state.connection_status.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
fn open_file(path: String) -> Result<(), String> {
    open::that(&path).map_err(|e| format!("打开文件失败: {}", e))
}

#[tauri::command]
fn get_session_id(state: tauri::State<AppState>) -> Result<Option<String>, String> {
    Ok(state.session_id.lock().map_err(|e| e.to_string())?.clone())
}

#[tauri::command]
fn send_acp_message(text: String, state: tauri::State<AppState>) -> Result<(), String> {
    // 支持 ACP 指令
    let cmd = text.trim();
    let msg = match cmd {
        "/session new" => "__new_session__".to_string(),
        "/cancel" | "/session cancel" => "__cancel__".to_string(),
        _ => {
            let config = state.config.lock().map_err(|e| e.to_string())?;
            let config = config.as_ref().ok_or("配置未加载")?;
            format!(
                "[kaya-transfer-hub | client: {}]\n{}",
                config.client_id, text
            )
        }
    };
    let tx = state.acp_tx.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = tx.as_ref() {
        tx.try_send(msg).map_err(|e| format!("发送失败: {}", e))
    } else {
        Err("ACP 客户端未启动".to_string())
    }
}

#[tauri::command]
fn set_tool_enabled(name: String, enabled: bool, state: tauri::State<AppState>) -> Result<(), String> {
    if let Ok(mut mgr) = state.tool_manager.lock() {
        if let Some(ref mut mgr) = *mgr {
            mgr.set_enabled(&name, enabled);
        }
    }
    if let Ok(mut cfg) = state.config.lock() {
        if let Some(ref mut config) = *cfg {
            config.tool_permissions.insert(name.clone(), enabled);
            if let Some(app) = APP_HANDLE.get() {
                let _ = save_config_impl(app, config);
            }
        }
    }
    if let Ok(tx) = state.re_register_tx.lock() {
        if let Some(tx) = tx.as_ref() {
            let _ = tx.try_send(());
        }
    }
    Ok(())
}

#[tauri::command]
fn upload_file(path: String, state: tauri::State<AppState>) -> Result<(), String> {
    let tx = state.upload_tx.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = tx.as_ref() {
        tx.try_send(ws_client::UploadRequest { file_path: path })
            .map_err(|e| format!("上传失败: {}", e))
    } else {
        Err("WebSocket 客户端未启动".to_string())
    }
}

#[tauri::command]
fn upload_file_data(name: String, data: Vec<u8>, state: tauri::State<AppState>) -> Result<(), String> {
    let tmp_dir = std::env::temp_dir().join("kaya-beam-uploads");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let path = tmp_dir.join(&name);
    std::fs::write(&path, &data).map_err(|e| e.to_string())?;
    let tx = state.upload_tx.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = tx.as_ref() {
        tx.try_send(ws_client::UploadRequest {
            file_path: path.to_string_lossy().to_string(),
        }).map_err(|e| format!("上传失败: {}", e))
    } else {
        Err("WebSocket 客户端未启动".to_string())
    }
}

#[tauri::command]
fn send_signal(
    name: String,
    sticky: bool,
    priority: String,
    notify_once: bool,
    data: serde_json::Value,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let tx = state.signal_tx.lock().map_err(|e| e.to_string())?;
    if let Some(tx) = tx.as_ref() {
        tx.try_send(ws_client::SignalRequest {
            name,
            sticky,
            priority,
            notify_once,
            data,
        })
        .map_err(|e| format!("发送信号失败: {}", e))
    } else {
        Err("WebSocket 客户端未启动".to_string())
    }
}

#[tauri::command]
fn execute_copilot(
    app: tauri::AppHandle,
    question: String,
    mode: String,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    let signal_tx = state
        .signal_tx
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("WebSocket 客户端未启动")?;

    let copilot_mode = if mode == "continuous" {
        copilot::CopilotMode::Continuous
    } else {
        copilot::CopilotMode::Single
    };

    tauri::async_runtime::spawn(async move {
        copilot::execute_copilot(&app, &signal_tx, question, copilot_mode).await;
    });

    Ok(())
}

#[tauri::command]
fn cancel_copilot(state: tauri::State<AppState>) -> Result<(), String> {
    let signal_tx = state
        .signal_tx
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or("WebSocket 客户端未启动")?;
    tauri::async_runtime::spawn(async move {
        copilot::cancel_copilot(&signal_tx).await;
    });
    Ok(())
}

#[tauri::command]
fn start_acp(app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    let mut started = state.acp_started.lock().map_err(|e| e.to_string())?;
    if *started {
        // 已经启动过了，把当前连接状态重新发给前端
        let connected = state.acp_connected.lock().map_err(|e| e.to_string())?;
        if *connected {
            let _ = app.emit("acp-status", serde_json::json!({"status": "已连接"}));
        } else {
            let _ = app.emit("acp-status", serde_json::json!({"status": "已断开"}));
        }
        drop(connected);
        drop(started);
        // 重新发送 session 就绪状态（页面重载时 listener 可能没收到首次事件）
        let session = state.session_id.lock().map_err(|e| e.to_string())?;
        if let Some(sid) = session.as_ref() {
            let _ = app.emit("acp-session", serde_json::json!({"sessionId": sid}));
        }
        return Ok(());
    }
    // 从 state 读取配置，启动 ACP 客户端
    let cfg = state.config.lock().map_err(|e| e.to_string())?;
    let config = cfg.clone();
    if let Some(config) = config {
        *started = true;
        drop(started);
        drop(cfg);
        start_acp_client(&app, &config, Arc::new(Mutex::new(None)));
        Ok(())
    } else {
        Err("请先配置服务器连接".to_string())
    }
}

#[tauri::command]
fn resize_float_window(app: tauri::AppHandle, width: f64, height: f64) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("kaya-float") {
        window.set_size(PhysicalSize::new(width, height)).map_err(|e| e.to_string())?;
        if let Ok(Some(m)) = app.primary_monitor() {
            let logical_h = m.size().height as f64 / m.scale_factor();
            let y = logical_h - height - 48.0;
            let current_pos = window.inner_position().map_err(|e| e.to_string())?;
            window.set_position(PhysicalPosition::new(
                current_pos.x,
                y as i32,
            )).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn run() {
    // 启动诊断：直接写文件（绕过 logger，因为 windows_subsystem=windows 会隐藏 stdout）
    let _ = std::fs::write(
        "kaya-startup.log",
        format!("STARTUP {}\n", chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")),
    );

    // 自定义 Logger：同时输出到 stdout 和文件
    use log::{LevelFilter, Log, Metadata, Record};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::sync::Mutex;

    struct DualLogger {
        file: Mutex<Option<std::fs::File>>,
    }

    impl Log for DualLogger {
        fn enabled(&self, metadata: &Metadata) -> bool {
            metadata.level() <= log::Level::Info
        }

        fn log(&self, record: &Record) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let msg = format!(
                "{} [{}] {} - {}\n",
                chrono::Local::now().format("%H:%M:%S%.3f"),
                record.level(),
                record.module_path().unwrap_or("-"),
                record.args(),
            );
            let _ = std::io::stdout().lock().write_all(msg.as_bytes());
            if let Ok(ref mut file_guard) = self.file.lock() {
                if let Some(ref mut f) = file_guard.as_mut() {
                    let _ = f.write_all(msg.as_bytes());
                    let _ = f.flush();
                }
            }
        }

        fn flush(&self) {
            if let Ok(ref mut file_guard) = self.file.lock() {
                if let Some(ref mut f) = file_guard.as_mut() {
                    let _ = f.flush();
                }
            }
        }
    }

    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open("kaya-client.log")
        .ok();

    log::set_boxed_logger(Box::new(DualLogger {
        file: Mutex::new(log_file),
    }))
    .map(|()| log::set_max_level(LevelFilter::Info))
    .ok();

    // panic hook：tokio task 内 panic 默认不打印到 log 文件，这里手动落地
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = format!(
            "PANIC: {}\n",
            info.to_string()
        );
        let _ = std::io::stderr().write_all(msg.as_bytes());
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("kaya-client.log")
            .and_then(|mut f| f.write_all(msg.as_bytes()));
        default_hook(info);
    }));

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            config: Mutex::new(None),
            connection_status: Mutex::new("未连接".to_string()),
            ws_started: Mutex::new(false),
            acp_started: Mutex::new(false),
            acp_tx: Mutex::new(None),
            acp_connected: Mutex::new(false),
            session_id: Mutex::new(None),
            upload_tx: Mutex::new(None),
            signal_tx: Mutex::new(None),
            tool_manager: Mutex::new(None),
            re_register_tx: Mutex::new(None),
            bubble_seq: Mutex::new(0),
            active_bubbles: Mutex::new(Vec::new()),
            last_msg_time: Mutex::new(None),
            bubble_content: Mutex::new(HashMap::new()),
            debounce_thinking: Mutex::new(String::new()),
            debounce_text: Mutex::new(String::new()),
            debounce_last: Mutex::new(Instant::now()),
            displayed: Mutex::new(String::new()),
            thinking_bubble_label: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            load_config, save_config, get_connection_status,
            open_file, send_acp_message, start_acp, get_session_id,
            upload_file, upload_file_data, send_signal,
            execute_copilot, cancel_copilot,
            copilot_enter_monitor, copilot_close,
            resize_float_window, resize_bubble, take_bubble_content, quick_chat_close,
            set_tool_enabled, close_tool_call_overlay,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let _ = APP_HANDLE.set(handle.clone());
            let cfg = load_config_impl(&handle);
            let state = app.state::<AppState>();
            *state.config.lock().unwrap() = cfg.clone();

            // 初始化 ToolManager
            if let Some(ref config) = cfg {
                *state.tool_manager.lock().unwrap() = Some(ToolManager::new(config));
            }

            if let Some(config) = cfg {
                *state.ws_started.lock().unwrap() = true;
                *state.acp_started.lock().unwrap() = true;
                let shared_signal_tx: Arc<Mutex<Option<tokio::sync::mpsc::Sender<ws_client::SignalRequest>>>> = Arc::new(Mutex::new(None));
                start_acp_client(&handle, &config, shared_signal_tx.clone());
                start_ws_client(&handle, config, shared_signal_tx);
            }

            app.handle().plugin(
                tauri_plugin_global_shortcut::Builder::new()
                    .with_handler(|app, shortcut, event| {
                        if event.state != ShortcutState::Pressed {
                            return;
                        }
                        let id = shortcut.id();
                        let mods = Modifiers::ALT | Modifiers::CONTROL;
                        if id == Shortcut::new(Some(mods), Code::KeyK).id() {
                            toggle_quick_chat(app);
                        } else if id == Shortcut::new(Some(mods), Code::KeyS).id() {
                            toggle_copilot_window(app, "single");
                        } else if id == Shortcut::new(Some(mods), Code::KeyC).id() {
                            toggle_copilot_window(app, "continuous");
                        }
                    })
                    .build(),
            )?;
            if let Err(e) = app.global_shortcut().register(
                Shortcut::new(Some(Modifiers::ALT | Modifiers::CONTROL), Code::KeyK),
            ) {
                log::error!("Failed to register KeyK shortcut: {}", e);
            }
            if let Err(e) = app.global_shortcut().register(
                Shortcut::new(Some(Modifiers::ALT | Modifiers::CONTROL), Code::KeyS),
            ) {
                log::warn!("Failed to register KeyS shortcut (will not affect other features): {}", e);
            }
            if let Err(e) = app.global_shortcut().register(
                Shortcut::new(Some(Modifiers::ALT | Modifiers::CONTROL), Code::KeyC),
            ) {
                log::warn!("Failed to register KeyC shortcut (will not affect other features): {}", e);
            }

            let _ = tray::setup_tray(&handle);

            // 创建透明悬浮图窗口（独立，固定右下角）
            if let Ok(Some(m)) = app.primary_monitor() {
                let size = m.size();
                let _ = WebviewWindowBuilder::new(
                    app,
                    "kaya-float",
                    WebviewUrl::App("float".into()),
                )
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .shadow(false)
                .inner_size(320.0, 320.0)
                .position(
                    (size.width as f64 / m.scale_factor()) - 320.0 - 12.0,
                    12.0,
                )
                .build();
            }

            // 主窗口点击 × 时隐藏到托盘而不是退出
            if let Some(main_window) = app.get_webview_window("main") {
                let win = main_window.clone();
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win.hide();
                    }
                });
            }

            // 气泡 5s 去抖冲刷 + 30s 超时清理
            let bg = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1)).await;

                    // 1.5s 去抖：距离上次 chunk 超过 1.5s → 创建气泡
                    let (chunk_thinking, chunk_text) = {
                        let state = bg.state::<AppState>();
                        let last = *state.debounce_last.lock().unwrap();
                        if last.elapsed() > Duration::from_secs_f32(1.5) {
                            let mut thk = state.debounce_thinking.lock().unwrap();
                            let mut txt = state.debounce_text.lock().unwrap();
                            if thk.is_empty() && txt.is_empty() {
                                (String::new(), String::new())
                            } else {
                                (std::mem::take(&mut *thk), std::mem::take(&mut *txt))
                            }
                        } else {
                            (String::new(), String::new())
                        }
                    };
                    if !chunk_text.is_empty() {
                        // 组装 display（由气泡层负责格式）
                        let new_display = if chunk_thinking.is_empty() {
                            chunk_text.clone()
                        } else {
                            format!("<think>{}</think>\n\n{}", chunk_thinking, chunk_text)
                        };
                        let new_len = new_display.len();
                        // 前缀 diff：去掉已展示的部分，只留新增内容
                        let displayed = bg.state::<AppState>().displayed.lock().unwrap().clone();
                        let remaining = if new_display.starts_with(&displayed) && new_display.len() > displayed.len() {
                            new_display[displayed.len()..].trim_start().to_string()
                        } else {
                            new_display.clone()
                        };
                        // 更新已展示的 display
                        *bg.state::<AppState>().displayed.lock().unwrap() = new_display;
                        log::info!("[bubble] prefix diff: displayed={} new={} remaining={}",
                            displayed.len(), new_len, remaining.len());
                        if remaining.is_empty() {
                            continue;
                        }
                        // 按空行分段，块大小智能合并
                        let mut acc = String::new();
                        for block in remaining.split("\n\n") {
                            let b = block.trim();
                            if b.is_empty() { continue; }
                            if acc.is_empty() {
                                acc = b.to_string();
                            } else if b.len() < 100 || acc.len() + b.len() < 500 {
                                if !acc.is_empty() { acc.push('\n'); }
                                acc.push_str(b);
                            } else {
                                create_message_bubble(&bg, &acc);
                                acc = b.to_string();
                            }
                        }
                        if !acc.is_empty() {
                            create_message_bubble(&bg, &acc);
                        }
                    }

                    // 30s 超时清理气泡
                    let expired = {
                        let state = bg.state::<AppState>();
                        let last = state.last_msg_time.lock().unwrap();
                        last.map(|t| t.elapsed() > Duration::from_secs(30)).unwrap_or(false)
                    };
                    if expired {
                        let labels: Vec<String> = {
                            let state = bg.state::<AppState>();
                            let mut bubbles = state.active_bubbles.lock().unwrap();
                            bubbles.drain(..).map(|b| b.label).collect()
                        };
                        let state = bg.state::<AppState>();
                        let mut content = state.bubble_content.lock().unwrap();
                        for label in &labels {
                            content.remove(label);
                            if let Some(win) = bg.get_webview_window(label) {
                                let _ = win.close();
                            }
                        }
                        state.displayed.lock().unwrap().clear();
                        // thinking 气泡也被清理了，重置追踪状态
                        *state.thinking_bubble_label.lock().unwrap() = None;
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
