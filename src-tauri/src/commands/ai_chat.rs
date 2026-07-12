//! AI 聊天命令：agent 循环 + 流式推送 + 中断机制

use crate::ai::provider::{load_providers, save_providers, ProviderConfig};
use crate::ai::sse::read_sse_stream;
use crate::ai::tools::{execute_tool, tool_definitions};
use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tauri::{AppHandle, Emitter, Manager};

/// 全局 run_id 计数器
static RUN_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

/// 全局 abort 标记：run_id → abort flag
static ABORTS: LazyLock<Mutex<HashMap<u32, Arc<AtomicBool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn next_run_id() -> u32 {
    RUN_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

pub(crate) fn abort_all() {
    let aborts = ABORTS.lock().unwrap();
    for flag in aborts.values() {
        flag.store(true, Ordering::SeqCst);
    }
}

/// 前端传入的聊天消息
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// 助手消息的 tool_calls（OpenAI 格式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    /// tool 角色消息的 tool_call_id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// 流式 chunk 事件 payload
#[derive(Debug, Clone, Serialize)]
struct ChunkPayload {
    run_id: u32,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct ToolPayload {
    run_id: u32,
    name: String,
    args: Value,
}

#[derive(Debug, Clone, Serialize)]
struct DonePayload {
    run_id: u32,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorPayload {
    run_id: u32,
    message: String,
}

const SYSTEM_PROMPT: &str = "你是 LocalFragNote 的 AI 助手，帮助用户管理他们的笔记（memo）。
你可以通过工具搜索、读取、创建 memo，以及列出标签。
回答使用用户提问的语言。memo 内容是 Markdown 格式。
创建 memo 前不需要确认，直接创建并告知用户。";

const MAX_AGENT_ROUNDS: u32 = 5;

/// 启动 AI 聊天，立即返回 run_id，流式通过 events 推送
#[tauri::command]
pub async fn ai_chat(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    provider_id: String,
    messages: Vec<ChatMessage>,
) -> IpcResult<u32> {
    let run_id = next_run_id();
    let abort_flag = Arc::new(AtomicBool::new(false));
    ABORTS.lock().unwrap().insert(run_id, abort_flag.clone());

    // 提前检查 provider 是否存在，避免 spawn 后才发现错误
    let store = state.store();
    let providers = load_providers(&store);
    let provider = providers
        .iter()
        .find(|p| p.id == provider_id)
        .cloned()
        .ok_or_else(|| IpcError::BadRequest("Provider 不存在".to_string()))?;
    drop(store);

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        agent_loop(app_handle, run_id, provider, messages, abort_flag);
    });

    Ok(run_id)
}

/// 中断指定 run
#[tauri::command]
pub fn ai_abort(run_id: u32) -> IpcResult<()> {
    if let Some(flag) = ABORTS.lock().unwrap().get(&run_id) {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

fn agent_loop(
    app: AppHandle,
    run_id: u32,
    provider: ProviderConfig,
    messages: Vec<ChatMessage>,
    abort_flag: Arc<AtomicBool>,
) {
    let state = app.state::<AppState>();
    let mut msgs: Vec<Value> = messages
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
        .collect();

    // 首轮注入 system prompt
    let system_msg = json!({"role": "system", "content": SYSTEM_PROMPT});

    for _round in 0..MAX_AGENT_ROUNDS {
        if abort_flag.load(Ordering::SeqCst) || state.shutdown.load(Ordering::SeqCst) {
            cleanup_abort(run_id);
            return;
        }

        // 构造请求 messages：system + 用户对话
        let mut req_messages = vec![system_msg.clone()];
        req_messages.extend(msgs.clone());

        let body = json!({
            "model": provider.model,
            "messages": req_messages,
            "stream": true,
            "tools": tool_definitions(),
        });

        let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
        let mut req = ureq::post(&url).set("Content-Type", "application/json");
        if !provider.api_key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", provider.api_key));
        }

        let response = match req.send_string(&body.to_string()) {
            Ok(r) => r,
            Err(e) => {
                let msg = format_http_error(e);
                let _ = app.emit("ai:error", ErrorPayload { run_id, message: msg });
                cleanup_abort(run_id);
                return;
            }
        };

        let status = response.status();
        if status >= 400 {
            let body_text = response.into_string().unwrap_or_default();
            let message = format!("HTTP {status}: {body_text}");
            let _ = app.emit("ai:error", ErrorPayload { run_id, message });
            cleanup_abort(run_id);
            return;
        }

        // 读取 SSE 流
        let reader = response.into_reader();
        let chunk_app = app.clone();
        let (content, tool_calls) = match read_sse_stream(reader, |delta| {
            let _ = chunk_app.emit("ai:chunk", ChunkPayload {
                run_id,
                text: delta.to_string(),
            });
        }) {
            Ok(r) => r,
            Err(e) => {
                let _ = app.emit("ai:error", ErrorPayload {
                    run_id,
                    message: format!("SSE 读取失败: {e}"),
                });
                cleanup_abort(run_id);
                return;
            }
        };

        if abort_flag.load(Ordering::SeqCst) || state.shutdown.load(Ordering::SeqCst) {
            cleanup_abort(run_id);
            return;
        }

        // 无工具调用：流式结束
        if tool_calls.is_empty() {
            let _ = app.emit("ai:done", DonePayload { run_id });
            cleanup_abort(run_id);
            return;
        }

        // 有工具调用：执行并继续循环
        let assistant_tool_calls: Vec<Value> = tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments,
                    }
                })
            })
            .collect();
        msgs.push(json!({
            "role": "assistant",
            "content": content,
            "tool_calls": assistant_tool_calls,
        }));

        // 执行每个工具调用：每个工具调用单独获取/释放 Store 锁，
        // 避免在一次循环中长时间持锁阻塞其他 DB 操作（如保存笔记、列表查询）。
        for tc in &tool_calls {
            if abort_flag.load(Ordering::SeqCst) || state.shutdown.load(Ordering::SeqCst) {
                cleanup_abort(run_id);
                return;
            }
            let _ = app.emit("ai:tool", ToolPayload {
                run_id,
                name: tc.name.clone(),
                args: serde_json::from_str(&tc.arguments).unwrap_or(Value::Null),
            });

            let args: Value = serde_json::from_str(&tc.arguments).unwrap_or(Value::Null);
            let result = {
                let store = state.store();
                execute_tool(&tc.name, &args, &store, Some(&app))
            };
            let result = match result {
                Ok(v) => v,
                Err(e) => json!({ "error": e.to_string() }),
            };
            msgs.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": result.to_string(),
            }));
        }
    }

    // 超过最大轮次
    let _ = app.emit("ai:error", ErrorPayload {
        run_id,
        message: format!("超过最大工具调用轮次 ({MAX_AGENT_ROUNDS})"),
    });
    cleanup_abort(run_id);
}

fn cleanup_abort(run_id: u32) {
    ABORTS.lock().unwrap().remove(&run_id);
}

fn format_http_error(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            format!("HTTP {code}: {body}")
        }
        ureq::Error::Transport(t) => {
            format!("网络错误: {t}")
        }
    }
}

/// 列出所有已配置的 provider
#[tauri::command]
pub fn list_providers(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ProviderConfig>> {
    let store = state.store();
    Ok(load_providers(&store))
}

/// 保存 provider 列表（全量替换）
#[tauri::command]
pub fn save_providers_cmd(
    state: tauri::State<'_, AppState>,
    providers: Vec<ProviderConfig>,
) -> IpcResult<Vec<ProviderConfig>> {
    let store = state.store();
    save_providers(&store, &providers)?;
    Ok(providers)
}
