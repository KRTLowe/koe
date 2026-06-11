use crate::config::AppConfig;

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
