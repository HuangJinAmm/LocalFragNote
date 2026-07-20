//! AI 聊天会话持久化：会话与消息的 CRUD
//!
//! 一个会话（ChatSession）对应一次连续对话，包含若干消息（ChatMessageRecord）。
//! 消息保留完整 role/content/tool_calls 等 JSON 字段，便于前端恢复展示与回传模型。

use crate::error::{CoreError, CoreResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

// ==================== 实体 ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: i64,
    pub title: String,
    pub provider_id: Option<String>,
    pub created_ts: i64,
    pub updated_ts: i64,
    /// 仅在 list 接口中填充：会话内消息总数（不含 tool 消息）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_count: Option<i32>,
    /// 仅在 list 接口中填充：最后一条非 tool 消息的预览（截断到 60 字）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageRecord {
    pub id: i64,
    pub session_id: i64,
    pub seq: i32,
    pub role: String,
    /// JSON 字符串：string 或 ContentPart[]
    pub content: String,
    /// JSON 字符串：assistant 的 tool_calls 数组（可空）
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    /// JSON 字符串：tool 执行结果（可空）
    pub tool_result: Option<String>,
    pub is_error: bool,
    pub created_ts: i64,
}

// ==================== Session CRUD ====================

/// 创建新会话。返回新建的会话对象。
pub fn create_session(
    conn: &Connection,
    title: &str,
    provider_id: Option<&str>,
) -> CoreResult<ChatSession> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "INSERT INTO chat_session (title, provider_id, created_ts, updated_ts)
         VALUES (?1, ?2, ?3, ?3)",
        params![title, provider_id, now],
    )?;
    let id = conn.last_insert_rowid();
    get_session(conn, id)?.ok_or_else(|| CoreError::Other("刚创建的 session 不存在".into()))
}

/// 获取单个会话（不含 message_count / preview）
pub fn get_session(conn: &Connection, id: i64) -> CoreResult<Option<ChatSession>> {
    let session = conn
        .query_row(
            "SELECT id, title, provider_id, created_ts, updated_ts
             FROM chat_session WHERE id = ?1",
            params![id],
            |row| {
                Ok(ChatSession {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    provider_id: row.get(2)?,
                    created_ts: row.get(3)?,
                    updated_ts: row.get(4)?,
                    message_count: None,
                    preview: None,
                })
            },
        )
        .ok();
    Ok(session)
}

/// 列出所有会话，按 updated_ts DESC 排序，附带 message_count 与 preview
pub fn list_sessions(conn: &Connection) -> CoreResult<Vec<ChatSession>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.provider_id, s.created_ts, s.updated_ts,
                (SELECT COUNT(*) FROM chat_message m
                    WHERE m.session_id = s.id AND m.role != 'tool') AS msg_count,
                (SELECT m.content FROM chat_message m
                    WHERE m.session_id = s.id
                    ORDER BY m.seq DESC LIMIT 1) AS last_content
         FROM chat_session s
         ORDER BY s.updated_ts DESC",
    )?;
    let sessions = stmt
        .query_map([], |row| {
            let content_json: Option<String> = row.get(6)?;
            let preview = content_json
                .as_deref()
                .and_then(extract_preview)
                .map(|s| {
                    // 截断到 60 字符（Unicode 字符数）
                    let chars: Vec<char> = s.chars().collect();
                    if chars.len() > 60 {
                        chars[..60].iter().collect::<String>() + "..."
                    } else {
                        s
                    }
                });
            Ok(ChatSession {
                id: row.get(0)?,
                title: row.get(1)?,
                provider_id: row.get(2)?,
                created_ts: row.get(3)?,
                updated_ts: row.get(4)?,
                message_count: Some(row.get(5)?),
                preview,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(sessions)
}

/// 重命名会话
pub fn rename_session(conn: &Connection, id: i64, title: &str) -> CoreResult<ChatSession> {
    let now = chrono::Utc::now().timestamp();
    let affected = conn.execute(
        "UPDATE chat_session SET title = ?1, updated_ts = ?2 WHERE id = ?3",
        params![title, now, id],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("session id={id}")));
    }
    get_session(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("session id={id}")))
}

/// 删除会话（外键 ON DELETE CASCADE 会自动清理 chat_message）
pub fn delete_session(conn: &Connection, id: i64) -> CoreResult<()> {
    conn.execute("DELETE FROM chat_session WHERE id = ?1", params![id])?;
    Ok(())
}

/// 更新会话的 updated_ts（用于追加消息时刷新排序）
pub fn touch_session(conn: &Connection, id: i64) -> CoreResult<()> {
    let now = chrono::Utc::now().timestamp();
    conn.execute(
        "UPDATE chat_session SET updated_ts = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    Ok(())
}

// ==================== Message CRUD ====================

/// 追加一条消息到指定会话，seq 自增。
/// content / tool_calls / tool_result 均为 JSON 字符串，由调用方序列化。
pub fn append_message(
    conn: &Connection,
    session_id: i64,
    role: &str,
    content: &str,
    tool_calls: Option<&str>,
    tool_call_id: Option<&str>,
    tool_result: Option<&str>,
    is_error: bool,
) -> CoreResult<ChatMessageRecord> {
    let now = chrono::Utc::now().timestamp();
    let next_seq: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM chat_message WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .unwrap_or(1);
    conn.execute(
        "INSERT INTO chat_message
            (session_id, seq, role, content, tool_calls, tool_call_id, tool_result, is_error, created_ts)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            session_id,
            next_seq,
            role,
            content,
            tool_calls,
            tool_call_id,
            tool_result,
            is_error as i32,
            now,
        ],
    )?;
    let id = conn.last_insert_rowid();
    touch_session(conn, session_id)?;
    Ok(ChatMessageRecord {
        id,
        session_id,
        seq: next_seq,
        role: role.to_string(),
        content: content.to_string(),
        tool_calls: tool_calls.map(|s| s.to_string()),
        tool_call_id: tool_call_id.map(|s| s.to_string()),
        tool_result: tool_result.map(|s| s.to_string()),
        is_error,
        created_ts: now,
    })
}

/// 加载指定会话的所有消息，按 seq ASC 排序
pub fn list_messages(conn: &Connection, session_id: i64) -> CoreResult<Vec<ChatMessageRecord>> {
    let mut stmt = conn.prepare(
        "SELECT id, session_id, seq, role, content, tool_calls, tool_call_id, tool_result, is_error, created_ts
         FROM chat_message
         WHERE session_id = ?1
         ORDER BY seq ASC",
    )?;
    let msgs = stmt
        .query_map(params![session_id], |row| {
            Ok(ChatMessageRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                seq: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                tool_calls: row.get(5)?,
                tool_call_id: row.get(6)?,
                tool_result: row.get(7)?,
                is_error: row.get::<_, i32>(8)? != 0,
                created_ts: row.get(9)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(msgs)
}

/// 清空指定会话的所有消息（保留会话本身）
pub fn clear_messages(conn: &Connection, session_id: i64) -> CoreResult<()> {
    conn.execute(
        "DELETE FROM chat_message WHERE session_id = ?1",
        params![session_id],
    )?;
    touch_session(conn, session_id)?;
    Ok(())
}

/// 从 content JSON 中提取预览文本
fn extract_preview(content_json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(content_json).ok()?;
    if let Some(s) = value.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = value.as_array() {
        let mut buf = String::new();
        for item in arr {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                if !buf.is_empty() {
                    buf.push(' ');
                }
                buf.push_str(text);
            }
        }
        if !buf.is_empty() {
            return Some(buf);
        }
    }
    None
}
