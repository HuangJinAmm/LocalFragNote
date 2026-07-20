//! MCP 服务器配置：持久化在 app_setting 表的 `mcp_config` key（JSON 字符串）

use memos_core::Store;
use serde::{Deserialize, Serialize};

/// app_setting 中的 key
pub const CONFIG_KEY: &str = "mcp_config";

/// MCP 服务器配置
///
/// - `enabled`：是否启用（持久化标志，决定启动时是否自动拉起）
/// - `host`：监听 host，默认 `"127.0.0.1"`（仅本机；外部 MCP 客户端都跑在本机）
/// - `port`：监听端口，默认 `27100`
/// - `auth_token`：可选 Bearer Token；非空时客户端必须在 `Authorization` 头中携带
/// - `auto_start`：应用启动时是否自动拉起 MCP 服务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default)]
    pub auth_token: String,

    #[serde(default)]
    pub auto_start: bool,
}

fn default_enabled() -> bool {
    false
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    27100
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            host: default_host(),
            port: default_port(),
            auth_token: String::new(),
            auto_start: false,
        }
    }
}

impl McpConfig {
    /// 客户端使用的 MCP 端点 URL
    pub fn endpoint_url(&self) -> String {
        format!("http://{}:{}/mcp", self.host, self.port)
    }

    /// 是否启用鉴权
    pub fn has_auth(&self) -> bool {
        !self.auth_token.trim().is_empty()
    }
}

/// 从 app_setting 读取配置，缺失则返回默认值
pub fn load_config(store: &Store) -> McpConfig {
    let json: Option<String> = store
        .with_conn(|c| store.setting.app.get(c, CONFIG_KEY))
        .unwrap_or(None);
    json.as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

/// 保存配置到 app_setting
pub fn save_config(store: &Store, config: &McpConfig) -> memos_core::CoreResult<()> {
    let json = serde_json::to_string(config).map_err(|e| {
        memos_core::CoreError::Other(format!("序列化 MCP 配置失败: {e}"))
    })?;
    store.with_conn(|c| store.setting.app.upsert(c, CONFIG_KEY, &json))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let c = McpConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 27100);
        assert!(c.auth_token.is_empty());
        assert!(!c.auto_start);
    }

    #[test]
    fn test_endpoint_url() {
        let mut c = McpConfig::default();
        c.host = "0.0.0.0".to_string();
        c.port = 9999;
        assert_eq!(c.endpoint_url(), "http://0.0.0.0:9999/mcp");
    }

    #[test]
    fn test_has_auth() {
        let mut c = McpConfig::default();
        assert!(!c.has_auth());
        c.auth_token = "  tok  ".to_string();
        assert!(c.has_auth());
        c.auth_token = "   ".to_string();
        assert!(!c.has_auth());
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let c = McpConfig {
            enabled: true,
            host: "0.0.0.0".to_string(),
            port: 12345,
            auth_token: "secret-token".to_string(),
            auto_start: true,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: McpConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c.enabled, back.enabled);
        assert_eq!(c.host, back.host);
        assert_eq!(c.port, back.port);
        assert_eq!(c.auth_token, back.auth_token);
        assert_eq!(c.auto_start, back.auto_start);
    }

    #[test]
    fn test_partial_json_uses_defaults() {
        let json = r#"{"port":9999,"auth_token":"x"}"#;
        let c: McpConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.port, 9999);
        assert_eq!(c.auth_token, "x");
        assert!(!c.enabled); // 默认
        assert_eq!(c.host, "127.0.0.1"); // 默认
        assert!(!c.auto_start); // 默认
    }

    #[test]
    fn test_load_config_default_when_absent() {
        let store = Store::open(":memory:").unwrap();
        let c = load_config(&store);
        assert_eq!(c.port, 27100);
        assert!(!c.enabled);
    }

    #[test]
    fn test_save_and_load_config() {
        let store = Store::open(":memory:").unwrap();
        let mut c = McpConfig::default();
        c.port = 9999;
        c.enabled = true;
        c.auth_token = "tok".to_string();
        c.auto_start = true;
        save_config(&store, &c).unwrap();
        let loaded = load_config(&store);
        assert_eq!(loaded.port, 9999);
        assert!(loaded.enabled);
        assert_eq!(loaded.auth_token, "tok");
        assert!(loaded.auto_start);
    }
}
