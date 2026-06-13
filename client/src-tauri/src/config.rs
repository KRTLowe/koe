use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::AppHandle;
use tauri::Manager;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub server_url: String,
    pub client_id: String,
    pub passkey: String,
    #[serde(default)]
    pub acp_url: Option<String>,
    #[serde(default)]
    pub storage_path: Option<String>,
    #[serde(default)]
    pub acp_cwd: Option<String>,
    #[serde(default)]
    pub float_image: Option<String>,
    #[serde(default = "default_allowed_read_paths")]
    pub allowed_read_paths: Vec<String>,
    #[serde(default = "default_allowed_write_paths")]
    pub allowed_write_paths: Vec<String>,
    #[serde(default = "default_denied_extensions")]
    pub denied_extensions: Vec<String>,
    #[serde(default = "default_tool_permissions")]
    pub tool_permissions: HashMap<String, bool>,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_allowed_read_paths() -> Vec<String> {
    vec![
        "~/kaya-transfer".into(),
        "~/Desktop".into(),
        "~/Documents".into(),
    ]
}

fn default_allowed_write_paths() -> Vec<String> {
    vec!["~/kaya-transfer".into(), "~/Desktop".into()]
}

fn default_denied_extensions() -> Vec<String> {
    vec!["exe".into(), "dll".into(), "sys".into(), "bin".into()]
}

fn default_log_level() -> String { "info".to_string() }

fn default_tool_permissions() -> HashMap<String, bool> {
    HashMap::from([
        ("take_screenshot".into(), true),
        ("get_clipboard".into(), true),
        ("file_search".into(), true),
        ("get_uia_tree".into(), true),
        ("read_text_file".into(), true),
        ("write_text_file".into(), false),
        ("list_directory".into(), true),
        ("get_file_info".into(), true),
        ("grep_file".into(), true),
        ("pull_file".into(), true),
        ("run_command".into(), false),
        ("write_clipboard".into(), true),
        ("open_path".into(), true),
        ("system_info".into(), true),
        ("list_windows".into(), true),
        ("start_process".into(), false),
        ("kill_process".into(), false),
        ("type_text".into(), false),
        ("key_press".into(), false),
        ("mouse_click".into(), false),
        ("ocr_region".into(), true),
    ])
}

impl AppConfig {
    pub fn is_valid(&self) -> bool {
        !self.server_url.is_empty() && !self.client_id.is_empty() && !self.passkey.is_empty()
    }
}

fn config_path(app: &AppHandle) -> PathBuf {
    let dir = app
        .path()
        .app_config_dir()
        .expect("Failed to get config dir");
    fs::create_dir_all(&dir).ok();
    dir.join("config.json")
}

pub fn load_config(app: &AppHandle) -> Option<AppConfig> {
    let path = config_path(app);
    if !path.exists() {
        return None;
    }
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn save_config(app: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app);
    let content = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}
