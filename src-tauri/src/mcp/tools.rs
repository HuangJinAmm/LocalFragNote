//! MCP 工具定义：memo 卡片的创建 / 修改 / 查询 / 删除
//!
//! 工具以 JSON 对象描述，符合 MCP `Tool` schema：
//! ```json
//! { "name": "...", "description": "...", "inputSchema": { ... } }
//! ```
//!
//! 工具调用结果统一为 `CallToolResult`，包含 `content` 数组（文本块）和可选 `isError`。

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::markdown;
use memos_core::memo::{CreateMemo, FindMemo, Memo, UpdateMemo};
use memos_core::types::{RowStatus, Visibility};
use memos_core::Store;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use tauri::Manager;

// ---------- MCP 协议类型 ----------

/// MCP 工具描述（响应 `tools/list`）
#[derive(Debug, Clone, Serialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// `tools/call` 返回结果
#[derive(Debug, Clone, Serialize)]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ToolContent {
    Text { text: String },
}

impl CallToolResult {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: text.into() }],
            is_error: None,
        }
    }

    pub fn json(value: &Value) -> Self {
        Self::text(serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into()))
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            content: vec![ToolContent::Text { text: message.into() }],
            is_error: Some(true),
        }
    }
}

// ---------- 工具列表 ----------

/// 返回所有暴露给 MCP 客户端的工具定义
pub fn tool_definitions() -> Vec<Tool> {
    vec![
        Tool {
            name: "create_memo".into(),
            description: "Create a new memo card (note). The content is Markdown; use #tag to attach tags. Returns the created memo.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Markdown content of the memo. Use #tag syntax to attach tags."
                    },
                    "visibility": {
                        "type": "string",
                        "enum": ["PUBLIC", "PROTECTED", "PRIVATE"],
                        "default": "PRIVATE",
                        "description": "Visibility of the memo."
                    },
                    "pinned": {
                        "type": "boolean",
                        "default": false,
                        "description": "Whether the memo is pinned to the top."
                    },
                    "uid": {
                        "type": "string",
                        "description": "Optional unique identifier (alphanumeric/underscore, <=64 chars). Auto-generated if omitted."
                    }
                },
                "required": ["content"]
            }),
        },
        Tool {
            name: "update_memo".into(),
            description: "Update an existing memo card by id or uid. Only provided fields are updated.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Numeric id of the memo." },
                    "uid": { "type": "string", "description": "UID of the memo. Used if id is not provided." },
                    "content": { "type": "string", "description": "New Markdown content." },
                    "visibility": {
                        "type": "string",
                        "enum": ["PUBLIC", "PROTECTED", "PRIVATE"]
                    },
                    "pinned": { "type": "boolean" },
                    "archived": { "type": "boolean", "description": "Set to true to archive, false to restore." }
                }
            }),
        },
        Tool {
            name: "delete_memo".into(),
            description: "Permanently delete a memo card by id or uid.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer" },
                    "uid": { "type": "string" }
                }
            }),
        },
        Tool {
            name: "get_memo".into(),
            description: "Get a single memo card by id or uid, including full content.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "integer" },
                    "uid": { "type": "string" }
                }
            }),
        },
        Tool {
            name: "list_memos".into(),
            description: "List memo cards with optional filters. Returns newest first by default.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "default": 20, "minimum": 1, "maximum": 200 },
                    "offset": { "type": "integer", "default": 0, "minimum": 0 },
                    "tag": { "type": "string", "description": "Filter by exact tag (without #)." },
                    "search": { "type": "string", "description": "Full-text search query (FTS5 syntax)." },
                    "pinned_only": { "type": "boolean", "default": false },
                    "include_archived": { "type": "boolean", "default": false }
                }
            }),
        },
        Tool {
            name: "search_memos".into(),
            description: "Full-text search across all memo cards using FTS5 query syntax. Returns matching memos ranked by relevance.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "FTS5 query string. Examples: 'keyword', '\"exact phrase\"', 'term1 OR term2', 'prefix*'."
                    },
                    "limit": { "type": "integer", "default": 20, "minimum": 1, "maximum": 200 }
                },
                "required": ["query"]
            }),
        },
        Tool {
            name: "list_tags".into(),
            description: "List all tags currently used across memos, with usage counts.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

// ---------- 工具调用分发 ----------

/// 根据 `tools/call` 的 name + arguments 执行工具
pub fn dispatch_tool(
    app: &tauri::AppHandle,
    name: &str,
    arguments: &Value,
) -> Result<CallToolResult, IpcError> {
    let state = app.state::<AppState>();
    if state.shutdown.load(Ordering::SeqCst) {
        return Ok(CallToolResult::error("服务正在关闭，无法处理请求"));
    }

    let store = state.store();
    match name {
        "create_memo" => tool_create_memo(app, &store, arguments),
        "update_memo" => tool_update_memo(app, &store, arguments),
        "delete_memo" => tool_delete_memo(app, &store, arguments),
        "get_memo" => tool_get_memo(&store, arguments),
        "list_memos" => tool_list_memos(&store, arguments),
        "search_memos" => tool_search_memos(&store, arguments),
        "list_tags" => tool_list_tags(&store, arguments),
        _ => Ok(CallToolResult::error(format!("未知工具: {name}"))),
    }
}

// ---------- embedding helpers（与 commands/memo.rs 等价的本地实现） ----------
//
// 之所以在此内联而非引用 `crate::commands::memo`：
// lib 目标（供集成测试编译）不声明 `commands` 模块，引用会导致 lib 编译失败。
// 这里的实现与 commands/memo.rs 保持一致，确保行为对齐。

fn should_store_embedding(row_status: RowStatus) -> bool {
    matches!(row_status, RowStatus::Normal)
}

fn delete_memo_embedding(store: &Store, id: i32) -> IpcResult<()> {
    store.with_conn(|c| {
        c.execute("DELETE FROM memo_vec WHERE rowid = ?", params![id])?;
        Ok(())
    })?;
    Ok(())
}

fn upsert_memo_embedding(store: &Store, id: i32, content: &str) -> IpcResult<()> {
    let embedding_json = crate::embedding::embed_to_json(content)?;
    store.with_conn(|c| {
        // vec0 不支持 UPDATE，先删后插以幂等
        c.execute("DELETE FROM memo_vec WHERE rowid = ?", params![id])?;
        c.execute(
            "INSERT INTO memo_vec(rowid, embedding) VALUES (?, ?)",
            params![id, &embedding_json],
        )?;
        Ok(())
    })?;
    Ok(())
}

fn sync_memo_embedding_for_memo(store: &Store, memo: &Memo) -> IpcResult<()> {
    if should_store_embedding(memo.row_status) {
        upsert_memo_embedding(store, memo.id, &memo.content)
    } else {
        delete_memo_embedding(store, memo.id)
    }
}

fn spawn_sync_memo_embedding(app: tauri::AppHandle, memo: Memo, action_label: &'static str) {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        if state.shutdown.load(Ordering::SeqCst) {
            tracing::info!("跳过 memo {} 的 embedding 同步：应用正在退出", memo.id);
            return;
        }

        let result = {
            let store = state.store();
            sync_memo_embedding_for_memo(&store, &memo)
        };

        if let Err(e) = result {
            tracing::warn!("memo {} 在{}后同步 embedding 失败: {}", memo.id, action_label, e);
        }
    });
}

// ---------- 工具实现 ----------

/// 生成 16 字符 hex UID（与 commands/lan.rs 一致）
fn gen_uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", now & 0xFFFF_FFFF_FFFF_FFFF)
}

/// 解析 visibility，默认 PRIVATE
fn parse_visibility(v: Option<&Value>) -> Visibility {
    match v.and_then(Value::as_str) {
        Some("PUBLIC") => Visibility::Public,
        Some("PROTECTED") => Visibility::Protected,
        Some("PRIVATE") => Visibility::Private,
        _ => Visibility::Private,
    }
}

#[derive(Debug, Deserialize)]
struct CreateMemoArgs {
    content: String,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    pinned: Option<bool>,
    #[serde(default)]
    uid: Option<String>,
}

fn tool_create_memo(
    app: &tauri::AppHandle,
    store: &Store,
    args: &Value,
) -> Result<CallToolResult, IpcError> {
    let parsed: CreateMemoArgs = serde_json::from_value(args.clone())
        .map_err(|e| IpcError::BadRequest(format!("参数解析失败: {e}")))?;

    if parsed.content.trim().is_empty() {
        return Ok(CallToolResult::error("content 不能为空"));
    }

    let uid = parsed.uid.unwrap_or_else(gen_uid);
    let visibility = parse_visibility(parsed.visibility.as_deref().map(Value::from).as_ref());
    let pinned = parsed.pinned.unwrap_or(false);

    let memo = store.with_conn(|c| {
        memos_core::memo::create(c, &CreateMemo {
            uid: uid.clone(),
            content: parsed.content.clone(),
            visibility,
            pinned,
            payload: serde_json::json!({}),
            location: None,
            parent_id: None,
        })
    })?;

    // 异步同步 embedding，不阻塞 MCP 响应
    spawn_sync_memo_embedding(app.clone(), memo.clone(), "MCP创建");

    let summary = memo_summary(&memo);
    Ok(CallToolResult::json(&summary))
}

#[derive(Debug, Deserialize)]
struct UpdateMemoArgs {
    id: Option<i32>,
    uid: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    pinned: Option<bool>,
    #[serde(default)]
    archived: Option<bool>,
}

fn tool_update_memo(
    app: &tauri::AppHandle,
    store: &Store,
    args: &Value,
) -> Result<CallToolResult, IpcError> {
    let parsed: UpdateMemoArgs = serde_json::from_value(args.clone())
        .map_err(|e| IpcError::BadRequest(format!("参数解析失败: {e}")))?;

    if parsed.id.is_none() && parsed.uid.is_none() {
        return Ok(CallToolResult::error("必须提供 id 或 uid"));
    }

    // 先找到现有 memo
    let existing = store.with_conn(|c| {
        memos_core::memo::get(c, &FindMemo {
            id: parsed.id,
            uid: parsed.uid.clone(),
            ..Default::default()
        })
    })?;
    let Some(existing) = existing else {
        return Ok(CallToolResult::error("找不到指定的 memo"));
    };

    let row_status = parsed.archived.map(|a| {
        if a {
            RowStatus::Archived
        } else {
            RowStatus::Normal
        }
    });

    let updated = store.with_conn(|c| {
        memos_core::memo::update(c, &UpdateMemo {
            id: existing.id,
            uid: None,
            row_status,
            content: parsed.content.clone(),
            visibility: parsed.visibility.as_deref().map(|s| match s {
                "PUBLIC" => Visibility::Public,
                "PROTECTED" => Visibility::Protected,
                _ => Visibility::Private,
            }),
            pinned: parsed.pinned,
            payload: None,
            location: None,
        })
    })?;

    let should_sync = parsed.content.is_some() || row_status.is_some();
    if should_sync && updated.parent_id.is_none() {
        spawn_sync_memo_embedding(app.clone(), updated.clone(), "MCP更新");
    }

    let summary = memo_summary(&updated);
    Ok(CallToolResult::json(&summary))
}

#[derive(Debug, Deserialize)]
struct DeleteMemoArgs {
    id: Option<i32>,
    uid: Option<String>,
}

fn tool_delete_memo(_app: &tauri::AppHandle, store: &Store, args: &Value) -> Result<CallToolResult, IpcError> {
    let parsed: DeleteMemoArgs = serde_json::from_value(args.clone())
        .map_err(|e| IpcError::BadRequest(format!("参数解析失败: {e}")))?;

    if parsed.id.is_none() && parsed.uid.is_none() {
        return Ok(CallToolResult::error("必须提供 id 或 uid"));
    }

    // 先找到 memo，再删
    let existing = store.with_conn(|c| {
        memos_core::memo::get(c, &FindMemo {
            id: parsed.id,
            uid: parsed.uid.clone(),
            ..Default::default()
        })
    })?;
    let Some(existing) = existing else {
        return Ok(CallToolResult::error("找不到指定的 memo"));
    };

    store.with_conn_mut(|c| memos_core::memo::delete(c, existing.id))?;

    // 同步删除 embedding
    if existing.parent_id.is_none() {
        if let Err(e) = delete_memo_embedding(store, existing.id) {
            tracing::warn!("MCP 删除 memo {} embedding 失败: {}", existing.id, e);
        }
    }

    Ok(CallToolResult::text(format!("已删除 memo id={} uid={}", existing.id, existing.uid)))
}

#[derive(Debug, Deserialize)]
struct GetMemoArgs {
    id: Option<i32>,
    uid: Option<String>,
}

fn tool_get_memo(store: &Store, args: &Value) -> Result<CallToolResult, IpcError> {
    let parsed: GetMemoArgs = serde_json::from_value(args.clone())
        .map_err(|e| IpcError::BadRequest(format!("参数解析失败: {e}")))?;

    let memo = store.with_conn(|c| {
        memos_core::memo::get(c, &FindMemo {
            id: parsed.id,
            uid: parsed.uid,
            ..Default::default()
        })
    })?;

    match memo {
        Some(m) => Ok(CallToolResult::json(&memo_full(&m))),
        None => Ok(CallToolResult::error("找不到指定的 memo")),
    }
}

#[derive(Debug, Deserialize)]
struct ListMemosArgs {
    #[serde(default)]
    limit: Option<i32>,
    #[serde(default)]
    offset: Option<i32>,
    #[serde(default)]
    tag: Option<String>,
    #[serde(default)]
    search: Option<String>,
    #[serde(default)]
    pinned_only: Option<bool>,
    #[serde(default)]
    include_archived: Option<bool>,
}

fn tool_list_memos(store: &Store, args: &Value) -> Result<CallToolResult, IpcError> {
    let parsed: ListMemosArgs = serde_json::from_value(args.clone())
        .map_err(|e| IpcError::BadRequest(format!("参数解析失败: {e}")))?;

    let limit = parsed.limit.unwrap_or(20).clamp(1, 200);
    let offset = parsed.offset.unwrap_or(0).max(0);
    let tag_search: Vec<String> = parsed.tag.map(|t| vec![t]).unwrap_or_default();
    let fts_query = parsed.search.filter(|s| !s.trim().is_empty());
    let pinned_only = parsed.pinned_only.unwrap_or(false);
    let row_status = if parsed.include_archived.unwrap_or(false) {
        None
    } else {
        Some(RowStatus::Normal)
    };

    let memos = store.with_conn(|c| {
        memos_core::memo::list(c, &FindMemo {
            row_status,
            fts_query,
            tag_search,
            pinned_only: Some(pinned_only),
            limit: Some(limit),
            offset: Some(offset),
            order_by_pinned: true,
            order_by_time_asc: false,
            main_only: true,
            ..Default::default()
        })
    })?;

    let summaries: Vec<Value> = memos.iter().map(memo_summary).collect();
    Ok(CallToolResult::json(&json!({ "memos": summaries, "count": summaries.len() })))
}

#[derive(Debug, Deserialize)]
struct SearchMemosArgs {
    query: String,
    #[serde(default)]
    limit: Option<i32>,
}

fn tool_search_memos(store: &Store, args: &Value) -> Result<CallToolResult, IpcError> {
    let parsed: SearchMemosArgs = serde_json::from_value(args.clone())
        .map_err(|e| IpcError::BadRequest(format!("参数解析失败: {e}")))?;

    if parsed.query.trim().is_empty() {
        return Ok(CallToolResult::error("query 不能为空"));
    }

    let limit = parsed.limit.unwrap_or(20).clamp(1, 200);

    let memos = store.with_conn(|c| {
        memos_core::memo::list(c, &FindMemo {
            fts_query: Some(parsed.query.clone()),
            limit: Some(limit),
            row_status: Some(RowStatus::Normal),
            main_only: true,
            ..Default::default()
        })
    })?;

    let summaries: Vec<Value> = memos.iter().map(memo_summary).collect();
    Ok(CallToolResult::json(&json!({ "memos": summaries, "count": summaries.len() })))
}

fn tool_list_tags(store: &Store, _args: &Value) -> Result<CallToolResult, IpcError> {
    let tags = store.with_conn(|c| memos_core::tag::list_tags(c))?;
    let result: Vec<Value> = tags
        .into_iter()
        .map(|(name, count)| json!({ "name": name, "count": count }))
        .collect();
    Ok(CallToolResult::json(&json!({ "tags": result })))
}

// ---------- 响应格式化 ----------

/// 列表场景的精简字段
fn memo_summary(memo: &memos_core::memo::Memo) -> Value {
    let tags = markdown::extract_tags(&memo.content);
    json!({
        "id": memo.id,
        "uid": memo.uid,
        "created_ts": memo.created_ts,
        "updated_ts": memo.updated_ts,
        "visibility": memo.visibility,
        "pinned": memo.pinned,
        "row_status": memo.row_status,
        "tags": tags,
        "snippet": markdown::generate_snippet(&memo.content, 200),
    })
}

/// 详情场景的完整字段
fn memo_full(memo: &memos_core::memo::Memo) -> Value {
    let tags = markdown::extract_tags(&memo.content);
    json!({
        "id": memo.id,
        "uid": memo.uid,
        "created_ts": memo.created_ts,
        "updated_ts": memo.updated_ts,
        "visibility": memo.visibility,
        "pinned": memo.pinned,
        "row_status": memo.row_status,
        "tags": tags,
        "content": memo.content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definitions_count() {
        let tools = tool_definitions();
        assert_eq!(tools.len(), 7);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"create_memo"));
        assert!(names.contains(&"update_memo"));
        assert!(names.contains(&"delete_memo"));
        assert!(names.contains(&"get_memo"));
        assert!(names.contains(&"list_memos"));
        assert!(names.contains(&"search_memos"));
        assert!(names.contains(&"list_tags"));
    }

    #[test]
    fn test_tool_schemas_are_objects() {
        for tool in tool_definitions() {
            assert_eq!(tool.input_schema["type"], "object",
                "tool {} inputSchema must be an object", tool.name);
        }
    }

    #[test]
    fn test_call_tool_result_text() {
        let r = CallToolResult::text("hello");
        assert_eq!(r.content.len(), 1);
        assert!(r.is_error.is_none());
    }

    #[test]
    fn test_call_tool_result_error() {
        let r = CallToolResult::error("oops");
        assert_eq!(r.is_error, Some(true));
    }
}
