use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ClientboundMessage {
    AuthResult {
        ok: bool,
        error: Option<String>,
    },
    Pong,
    FileMeta {
        file_id: String,
        name: String,
        size: u64,
    },
    FileEnd {
        file_id: String,
        checksum: String,
    },
    RegisterToolsResult {
        registered: u64,
    },
    FileUploadStartAck,
    FileUploadResult {
        file_id: String,
        ok: bool,
        path: Option<String>,
        error: Option<String>,
    },
    CallTool {
        request_id: String,
        name: String,
        arguments: Value,
    },
    AcpInject {
        text: String,
    },
    SignalAck,
    Unknown {
        message_type: String,
    },
    MissingType,
}

impl ClientboundMessage {
    pub(crate) fn parse_text(text: &str) -> Result<Self, serde_json::Error> {
        let value: Value = serde_json::from_str(text)?;
        let Some(message_type) = value.get("type").and_then(Value::as_str) else {
            return Ok(Self::MissingType);
        };

        let message = match message_type {
            "auth_result" => Self::AuthResult {
                ok: value.get("ok").and_then(Value::as_bool).unwrap_or(false),
                error: optional_string(&value, "error"),
            },
            "pong" => Self::Pong,
            "file_meta" => Self::FileMeta {
                file_id: string_or_default(&value, "file_id"),
                name: value
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
            },
            "file_end" => Self::FileEnd {
                file_id: string_or_default(&value, "file_id"),
                checksum: string_or_default(&value, "checksum"),
            },
            "register_tools_result" => Self::RegisterToolsResult {
                registered: value.get("registered").and_then(Value::as_u64).unwrap_or(0),
            },
            "file_upload_start_ack" => Self::FileUploadStartAck,
            "file_upload_result" => Self::FileUploadResult {
                file_id: string_or_default(&value, "file_id"),
                ok: value.get("ok").and_then(Value::as_bool).unwrap_or(false),
                path: optional_string(&value, "path"),
                error: optional_string(&value, "error"),
            },
            "call_tool" => Self::CallTool {
                request_id: string_or_default(&value, "request_id"),
                name: string_or_default(&value, "name"),
                arguments: value
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
            },
            "acp_inject" => Self::AcpInject {
                text: string_or_default(&value, "text"),
            },
            "signal_ack" => Self::SignalAck,
            unknown => Self::Unknown {
                message_type: unknown.to_string(),
            },
        };

        Ok(message)
    }
}

fn string_or_default(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn optional_string(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::ClientboundMessage;

    #[test]
    fn parses_auth_result_message() {
        let message = ClientboundMessage::parse_text(r#"{"type":"auth_result","ok":false,"error":"auth failed"}"#)
            .expect("auth_result should parse");

        assert_eq!(
            message,
            ClientboundMessage::AuthResult {
                ok: false,
                error: Some("auth failed".to_string()),
            },
        );
    }

    #[test]
    fn parses_file_meta_message() {
        let message = ClientboundMessage::parse_text(r#"{"type":"file_meta","file_id":"f_1","name":"a.png","size":42}"#)
            .expect("file_meta should parse");

        assert_eq!(
            message,
            ClientboundMessage::FileMeta {
                file_id: "f_1".to_string(),
                name: "a.png".to_string(),
                size: 42,
            },
        );
    }

    #[test]
    fn parses_call_tool_message() {
        let message = ClientboundMessage::parse_text(
            r#"{"type":"call_tool","request_id":"req_1","name":"get_clipboard","arguments":{"plain":true}}"#,
        )
        .expect("call_tool should parse");

        assert_eq!(
            message,
            ClientboundMessage::CallTool {
                request_id: "req_1".to_string(),
                name: "get_clipboard".to_string(),
                arguments: serde_json::json!({"plain": true}),
            },
        );
    }

    #[test]
    fn keeps_unknown_message_type_visible() {
        let message = ClientboundMessage::parse_text(r#"{"type":"future_message","value":1}"#)
            .expect("unknown message should parse as Unknown");

        assert_eq!(
            message,
            ClientboundMessage::Unknown {
                message_type: "future_message".to_string(),
            },
        );
    }
}
