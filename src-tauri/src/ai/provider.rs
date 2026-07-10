//! Provider 配置：存储在 app_setting 表，key = "ai_providers"

use memos_core::Store;
use serde::{Deserialize, Serialize};

/// OpenAI 兼容 provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 唯一 ID（uuid 字符串）
    pub id: String,
    /// 显示名，如 "OpenAI" / "本地 Ollama"
    pub name: String,
    /// API base URL，如 "https://api.openai.com/v1"
    pub base_url: String,
    /// API key，Ollama 可为空字符串
    #[serde(default)]
    pub api_key: String,
    /// 模型名，如 "gpt-4o-mini"
    pub model: String,
}

const AI_PROVIDERS_KEY: &str = "ai_providers";

/// 从 app_setting 读取所有 provider 配置
pub fn load_providers(store: &Store) -> Vec<ProviderConfig> {
    let json: Option<String> = store
        .with_conn(|c| store.setting.app.get(c, AI_PROVIDERS_KEY))
        .unwrap_or(None);
    json.as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

/// 保存 provider 配置到 app_setting
pub fn save_providers(store: &Store, providers: &[ProviderConfig]) -> memos_core::CoreResult<()> {
    let json = serde_json::to_string(providers)
        .map_err(|e| memos_core::CoreError::Other(format!("序列化 provider 配置失败: {e}")))?;
    store.with_conn(|c| store.setting.app.upsert(c, AI_PROVIDERS_KEY, &json))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_serde_roundtrip() {
        let p = ProviderConfig {
            id: "abc-123".to_string(),
            name: "Test".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "sk-xxx".to_string(),
            model: "gpt-4o-mini".to_string(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(p.id, back.id);
        assert_eq!(p.name, back.name);
        assert_eq!(p.base_url, back.base_url);
        assert_eq!(p.api_key, back.api_key);
        assert_eq!(p.model, back.model);
    }

    #[test]
    fn test_load_providers_empty() {
        let store = Store::open(":memory:").unwrap();
        let providers = load_providers(&store);
        assert!(providers.is_empty());
    }

    #[test]
    fn test_save_and_load_providers() {
        let store = Store::open(":memory:").unwrap();
        let providers = vec![ProviderConfig {
            id: "p1".to_string(),
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
        }];
        save_providers(&store, &providers).unwrap();
        let loaded = load_providers(&store);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "p1");
        assert_eq!(loaded[0].name, "OpenAI");
    }

    #[test]
    fn test_provider_config_ollama_empty_api_key() {
        let json = r#"{"id":"o1","name":"Ollama","base_url":"http://localhost:11434/v1","model":"qwen2.5:7b"}"#;
        let p: ProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(p.api_key, "");
    }
}
