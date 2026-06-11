use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    VisualInputAvailable {
        source: String,
        timestamp: String,
        sticky: bool,
        priority: &'static str,
    },
    ClipboardChanged {
        timestamp: String,
        sticky: bool,
        priority: &'static str,
    },
}

impl Signal {
    pub fn name(&self) -> &'static str {
        match self {
            Signal::VisualInputAvailable { .. } => "visual_input_available",
            Signal::ClipboardChanged { .. } => "clipboard_changed",
        }
    }

    pub fn data(&self) -> Value {
        match self {
            Signal::VisualInputAvailable { source, timestamp, sticky, priority } => {
                serde_json::json!({
                    "source": source,
                    "timestamp": timestamp,
                    "sticky": sticky,
                    "priority": priority,
                })
            }
            Signal::ClipboardChanged { timestamp, sticky, priority } => {
                serde_json::json!({
                    "timestamp": timestamp,
                    "sticky": sticky,
                    "priority": priority,
                })
            }
        }
    }

    pub fn to_ws_message(&self) -> String {
        serde_json::json!({
            "type": "signal",
            "name": self.name(),
            "sticky": self.is_sticky(),
            "priority": self.priority_str(),
            "data": self.data(),
        })
        .to_string()
    }

    pub fn is_sticky(&self) -> bool {
        match self {
            Signal::VisualInputAvailable { sticky, .. } => *sticky,
            Signal::ClipboardChanged { sticky, .. } => *sticky,
        }
    }

    pub fn priority_str(&self) -> &'static str {
        match self {
            Signal::VisualInputAvailable { priority, .. } => priority,
            Signal::ClipboardChanged { priority, .. } => priority,
        }
    }
}

/// 生成 signal_clear 消息（取消粘性信号）。
pub fn clear_signal(name: &str) -> String {
    serde_json::json!({
        "type": "signal_clear",
        "name": name,
    })
    .to_string()
}

pub fn signal_for_tool(tool_name: &str) -> Option<Signal> {
    let now = chrono::Utc::now().to_rfc3339();
    match tool_name {
        "take_screenshot" => Some(Signal::VisualInputAvailable {
            source: "screenshot".to_string(),
            timestamp: now,
            sticky: false,
            priority: "normal",
        }),
        "get_clipboard" => Some(Signal::ClipboardChanged {
            timestamp: now,
            sticky: false,
            priority: "normal",
        }),
        _ => None,
    }
}
