//! AI 聊天会话持久化 IPC 命令
//!
//! 对应 core/src/chat_session.rs。前端通过这些命令管理会话列表与消息历史。

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::chat_session::{self, ChatMessageRecord, ChatSession};
use serde::{Deserialize, Serialize};

// ==================== Session 命令 ====================

#[tauri::command]
pub fn chat_list_sessions(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ChatSession>> {
    let store = state.store();
    Ok(store.with_conn(|c| chat_session::list_sessions(c))?)
}

#[tauri::command]
pub fn chat_create_session(
    state: tauri::State<'_, AppState>,
    title: String,
    provider_id: Option<String>,
) -> IpcResult<ChatSession> {
    let title = title.trim();
    if title.is_empty() {
        return Err(IpcError::BadRequest("title 不能为空".into()));
    }
    let store = state.store();
    Ok(store.with_conn(|c| chat_session::create_session(c, title, provider_id.as_deref()))?)
}

#[tauri::command]
pub fn chat_rename_session(
    state: tauri::State<'_, AppState>,
    id: i64,
    title: String,
) -> IpcResult<ChatSession> {
    let title = title.trim();
    if title.is_empty() {
        return Err(IpcError::BadRequest("title 不能为空".into()));
    }
    let store = state.store();
    Ok(store.with_conn(|c| chat_session::rename_session(c, id, title))?)
}

#[tauri::command]
pub fn chat_delete_session(state: tauri::State<'_, AppState>, id: i64) -> IpcResult<()> {
    let store = state.store();
    Ok(store.with_conn(|c| chat_session::delete_session(c, id))?)
}

// ==================== Message 命令 ====================

#[tauri::command]
pub fn chat_list_messages(
    state: tauri::State<'_, AppState>,
    session_id: i64,
) -> IpcResult<Vec<ChatMessageRecord>> {
    let store = state.store();
    Ok(store.with_conn(|c| chat_session::list_messages(c, session_id))?)
}

/// 追加消息（由前端在发送/接收消息后调用）。
/// content/tool_calls/tool_result 均为已序列化的 JSON 字符串。
#[tauri::command]
pub fn chat_append_message(
    state: tauri::State<'_, AppState>,
    session_id: i64,
    role: String,
    content: String,
    tool_calls: Option<String>,
    tool_call_id: Option<String>,
    tool_result: Option<String>,
    is_error: Option<bool>,
) -> IpcResult<ChatMessageRecord> {
    let store = state.store();
    Ok(store.with_conn(|c| {
        chat_session::append_message(
            c,
            session_id,
            &role,
            &content,
            tool_calls.as_deref(),
            tool_call_id.as_deref(),
            tool_result.as_deref(),
            is_error.unwrap_or(false),
        )
    })?)
}

#[tauri::command]
pub fn chat_clear_messages(state: tauri::State<'_, AppState>, session_id: i64) -> IpcResult<()> {
    let store = state.store();
    Ok(store.with_conn(|c| chat_session::clear_messages(c, session_id))?)
}

// ==================== 辅助类型（前端可复用） ====================

/// 用于前端按需返回的字段（仅类型导出，命令返回值复用 ChatMessageRecord）
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
struct ChatMessageSummary {
    id: i64,
    session_id: i64,
    role: String,
    is_error: bool,
    created_ts: i64,
}
