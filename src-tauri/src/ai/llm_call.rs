//! 非流式 LLM 调用 helper：复用于 suggest_tags 与 document_summary

use crate::ai::provider::{load_providers, ProviderConfig};
use crate::error::{IpcError, IpcResult};
use memos_core::Store;
use serde_json::{json, Value};

/// 选取用于非流式调用的 provider。
///
/// 优先级：
/// 1. `preferred_id` 非空且能匹配到已配置 provider → 使用该 provider
///    （让 suggest_tags / document_summary 与 AI 聊天面板的当前选择同步）
/// 2. 否则回退到 provider 列表中的第一项（保留旧行为，向后兼容）
///
/// 列表为空 → BadRequest
fn pick_provider<'a>(
    providers: &'a [ProviderConfig],
    preferred_id: Option<&str>,
) -> Result<&'a ProviderConfig, IpcError> {
    if let Some(id) = preferred_id.filter(|s| !s.is_empty()) {
        if let Some(p) = providers.iter().find(|p| p.id == id) {
            return Ok(p);
        }
    }
    providers
        .first()
        .ok_or_else(|| IpcError::BadRequest("未配置 AI provider，请先在设置中配置".into()))
}

/// 使用首个已配置 provider 发起非流式 chat completion，返回 assistant 文本。
///
/// - 未配置 provider → BadRequest
/// - HTTP/解析失败 → Internal
///
/// 保留以兼容旧调用方；新调用应使用 `call_provider`。
pub fn call_first_provider(
    store: &Store,
    system_prompt: &str,
    user_message: &str,
) -> IpcResult<String> {
    call_provider(store, None, system_prompt, user_message)
}

/// 与 `call_first_provider` 相同，但可指定优先使用的 provider ID。
/// `preferred_id` 为 None 或匹配不到时回退到首个 provider。
pub fn call_provider(
    store: &Store,
    preferred_id: Option<&str>,
    system_prompt: &str,
    user_message: &str,
) -> IpcResult<String> {
    let providers = load_providers(store);
    let provider = pick_provider(&providers, preferred_id)?.clone();

    let body = json!({
        "model": provider.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_message },
        ],
        "stream": false,
    });

    let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
    let mut req = ureq::post(&url).set("Content-Type", "application/json");
    if !provider.api_key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", provider.api_key));
    }

    let response = req
        .send_string(&body.to_string())
        .map_err(|e| IpcError::Internal(format!("AI 请求失败: {e}")))?;

    if response.status() >= 400 {
        let status = response.status();
        let body_text = response.into_string().unwrap_or_default();
        return Err(IpcError::Internal(format!("HTTP {status}: {body_text}")));
    }

    let resp_json: Value = serde_json::from_str(
        &response
            .into_string()
            .map_err(|e| IpcError::Internal(format!("读取响应失败: {e}")))?,
    )
    .map_err(|e| IpcError::Internal(format!("解析响应 JSON 失败: {e}")))?;

    Ok(resp_json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string())
}
