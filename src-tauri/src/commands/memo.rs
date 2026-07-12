//! Memo 相关 IPC 命令

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::markdown;
use memos_core::memo::{CreateMemo, FindMemo, Memo, MemoLocation, UpdateMemo};
use memos_core::types::{RowStatus, Visibility};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::Manager;

/// 创建 memo 的请求
#[derive(Debug, Deserialize)]
pub struct CreateMemoRequest {
    pub uid: String,
    pub content: String,
    #[serde(default)]
    pub visibility: Visibility,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default = "default_payload")]
    pub payload: Value,
    #[serde(default)]
    pub location: Option<MemoLocation>,
}

fn default_payload() -> Value {
    Value::Object(Default::default())
}

/// 更新 memo 的请求（所有字段可选，id 必填）
#[derive(Debug, Deserialize)]
pub struct UpdateMemoRequest {
    pub id: i32,
    pub uid: Option<String>,
    pub row_status: Option<RowStatus>,
    pub content: Option<String>,
    pub visibility: Option<Visibility>,
    pub pinned: Option<bool>,
    pub payload: Option<Value>,
    /// None = 不更新；Some(None) = 清除；Some(Some(loc)) = 设置
    #[serde(default)]
    pub location: Option<Option<MemoLocation>>,
}

/// 查询 memo 的请求
#[derive(Debug, Deserialize, Default)]
pub struct ListMemosRequest {
    pub id: Option<i32>,
    pub uid: Option<String>,
    pub id_list: Option<Vec<i32>>,
    pub uid_list: Option<Vec<String>>,
    pub row_status: Option<RowStatus>,
    pub visibility_list: Option<Vec<Visibility>>,
    #[serde(default)]
    pub exclude_content: bool,
    pub content_contains: Option<String>,
    /// FTS5 全文搜索查询（MATCH 语法）
    pub fts_query: Option<String>,
    /// 向量搜索的 embedding（JSON 字符串，384维）
    pub vector_embedding: Option<String>,
    /// 向量搜索返回数量
    pub vector_top_k: Option<u32>,
    pub tag_search: Option<Vec<String>>,
    pub created_ts_after: Option<i64>,
    pub created_ts_before: Option<i64>,
    pub updated_ts_after: Option<i64>,
    pub updated_ts_before: Option<i64>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    #[serde(default)]
    pub order_by_pinned: bool,
    #[serde(default)]
    pub order_by_updated_ts: bool,
    #[serde(default)]
    pub order_by_time_asc: bool,
}

/// 列表响应：附带统计信息
#[derive(Debug, Serialize)]
pub struct ListMemosResponse {
    pub memos: Vec<Memo>,
    /// 总数（不带 limit/offset）
    pub total: i32,
}

/// 提取的 markdown 元数据
#[derive(Debug, Serialize)]
pub struct MemoMetadata {
    pub tags: Vec<String>,
    pub mentions: Vec<String>,
    pub html: String,
    pub snippet: String,
}

// ---------- 命令 ----------

#[tauri::command]
pub fn create_memo(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    req: CreateMemoRequest,
) -> IpcResult<Memo> {
    let memo = {
        let store = state.store();
        store.with_conn(|c| {
            memos_core::memo::create(c, &CreateMemo {
                uid: req.uid,
                content: req.content,
                visibility: req.visibility,
                pinned: req.pinned,
                payload: req.payload,
                location: req.location,
            })
        })?
    };

    // 异步生成 embedding 并插入 vec0 表（不阻塞 memo 创建返回）
    let content = memo.content.clone();
    let id = memo.id;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        match crate::embedding::embed_to_json(&content) {
            Ok(embedding_json) => {
                // vec0 不支持 UPDATE，先删后插以幂等
                if let Err(e) = state.store().with_conn(|c| {
                    c.execute("DELETE FROM memo_vec WHERE rowid = ?", params![id])?;
                    c.execute(
                        "INSERT INTO memo_vec(rowid, embedding) VALUES (?, ?)",
                        params![id, &embedding_json],
                    )?;
                    Ok(())
                }) {
                    tracing::warn!("为 memo {} 插入 embedding 失败: {}", id, e);
                }
            }
            Err(e) => tracing::warn!("为 memo {} 生成 embedding 失败: {}", id, e),
        }
    });

    Ok(memo)
}

#[tauri::command]
pub fn get_memo(
    state: tauri::State<'_, AppState>,
    id: Option<i32>,
    uid: Option<String>,
) -> IpcResult<Option<Memo>> {
    let store = state.store();
    let find = FindMemo { id, uid, ..Default::default() };
    Ok(store.with_conn(|c| memos_core::memo::get(c, &find))?)
}

#[tauri::command]
pub fn list_memos(
    state: tauri::State<'_, AppState>,
    req: ListMemosRequest,
) -> IpcResult<ListMemosResponse> {
    let store = state.store();
    let find = build_find(req);
    let memos = store.with_conn(|c| memos_core::memo::list(c, &find))?;
    // 统计总数：复用相同过滤但不带 limit/offset
    let count_find = FindMemo { limit: None, offset: None, ..find.clone() };
    let total = store.with_conn(|c| memos_core::memo::list(c, &count_find))?.len() as i32;
    Ok(ListMemosResponse { memos, total })
}

#[tauri::command]
pub fn update_memo(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    req: UpdateMemoRequest,
) -> IpcResult<Memo> {
    let content_updated = req.content.is_some();
    let updated = {
        let store = state.store();
        store.with_conn(|c| {
            memos_core::memo::update(c, &UpdateMemo {
                id: req.id,
                uid: req.uid,
                row_status: req.row_status,
                content: req.content,
                visibility: req.visibility,
                pinned: req.pinned,
                payload: req.payload,
                location: req.location,
            })
        })?
    };

    // 当 content 更新时，异步重建 embedding（vec0 不支持 UPDATE，先删后插）
    if content_updated {
        let content = updated.content.clone();
        let id = updated.id;
        tauri::async_runtime::spawn_blocking(move || {
            let state = app.state::<AppState>();
            match crate::embedding::embed_to_json(&content) {
                Ok(embedding_json) => {
                    if let Err(e) = state.store().with_conn(|c| {
                        c.execute("DELETE FROM memo_vec WHERE rowid = ?", params![id])?;
                        c.execute(
                            "INSERT INTO memo_vec(rowid, embedding) VALUES (?, ?)",
                            params![id, &embedding_json],
                        )?;
                        Ok(())
                    }) {
                        tracing::warn!("为 memo {} 重建 embedding 失败: {}", id, e);
                    }
                }
                Err(e) => tracing::warn!("为 memo {} 生成 embedding 失败: {}", id, e),
            }
        });
    }

    Ok(updated)
}

#[tauri::command]
pub fn delete_memo(state: tauri::State<'_, AppState>, id: i32) -> IpcResult<()> {
    let store = state.store();
    store.with_conn_mut(|c| memos_core::memo::delete(c, id))?;
    Ok(())
}

/// 渲染 memo 内容：返回 tag/mention/html/snippet
#[tauri::command]
pub fn render_memo_content(content: String) -> IpcResult<MemoMetadata> {
    let extracted = markdown::extract_all(&content);
    Ok(MemoMetadata {
        html: markdown::render_html(&content),
        snippet: markdown::generate_snippet(&content, 200),
        tags: extracted.tags,
        mentions: extracted.mentions,
    })
}

/// 获取所有使用过的 tag 及其使用数量（去重，按字母序）
#[derive(Debug, Serialize)]
pub struct TagWithCount {
    pub tag: String,
    pub count: i32,
}

#[tauri::command]
pub fn list_tags(state: tauri::State<'_, AppState>) -> IpcResult<Vec<TagWithCount>> {
    let store = state.store();
    let contents = store.with_conn(|c| -> memos_core::CoreResult<Vec<String>> {
        let mut stmt = c.prepare("SELECT content FROM memo WHERE row_status = 'NORMAL'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    // 统计每个 tag 的使用数量
    let mut counts: std::collections::BTreeMap<String, i32> = std::collections::BTreeMap::new();
    for content in contents {
        for tag in markdown::extract_tags(&content) {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }
    Ok(counts
        .into_iter()
        .map(|(tag, count)| TagWithCount { tag, count })
        .collect())
}

/// 获取所有 NORMAL 状态 memo 的创建和更新时间戳，用于热力图统计
#[derive(Debug, Serialize)]
pub struct MemoTimestamps {
    pub created_timestamps: Vec<i64>,
    pub updated_timestamps: Vec<i64>,
}

#[tauri::command]
pub fn list_memo_timestamps(state: tauri::State<'_, AppState>) -> IpcResult<MemoTimestamps> {
    let store = state.store();
    let (mut created, mut updated) = store.with_conn(|c| -> memos_core::CoreResult<(Vec<i64>, Vec<i64>)> {
        let mut stmt = c.prepare("SELECT created_ts, updated_ts FROM memo WHERE row_status = 'NORMAL'")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        let mut c_list = Vec::new();
        let mut u_list = Vec::new();
        for r in rows {
            let (c_ts, u_ts) = r?;
            c_list.push(c_ts);
            u_list.push(u_ts);
        }
        Ok((c_list, u_list))
    })?;
    // 按时间升序
    created.sort_unstable();
    updated.sort_unstable();
    Ok(MemoTimestamps {
        created_timestamps: created,
        updated_timestamps: updated,
    })
}

/// 生成文本的 embedding（JSON 字符串），供前端语义搜索查询使用
/// 异步执行：模型下载与推理可能耗时数秒到数十秒，避免阻塞 Tauri 主线程
#[tauri::command]
pub async fn embed_text(text: String) -> IpcResult<String> {
    tauri::async_runtime::spawn_blocking(move || crate::embedding::embed_to_json(&text))
        .await
        .map_err(|e| IpcError::Internal(format!("embed_text 任务失败: {e}")))?
}

/// AI 建议标签：根据笔记内容调用 LLM 生成标签建议
/// 将系统已有标签一并发送给 AI，优先复用已有标签，排除笔记中已存在的标签
#[tauri::command]
pub async fn suggest_tags(
    state: tauri::State<'_, AppState>,
    content: String,
) -> IpcResult<Vec<String>> {
    let store = state.store();
    let providers = crate::ai::provider::load_providers(&store);
    let provider = providers
        .first()
        .cloned()
        .ok_or_else(|| IpcError::BadRequest("未配置 AI provider，请先在设置中配置".into()))?;

    // 笔记中已有的标签，用于排除
    let existing_tags: Vec<String> = markdown::extract_tags(&content);

    // 查询系统中所有已使用的标签，提供给 AI 优先复用
    let system_tags: Vec<String> = {
        let contents = store.with_conn(|c| -> memos_core::CoreResult<Vec<String>> {
            let mut stmt = c.prepare("SELECT content FROM memo WHERE row_status = 'NORMAL'")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            Ok(out)
        })?;
        let mut all: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for c in contents {
            for t in markdown::extract_tags(&c) {
                all.insert(t);
            }
        }
        all.into_iter().collect()
    };

    let system_prompt = "你是一个标签建议专家。根据用户提供的笔记内容，建议 3-5 个合适的标签。\n\n规则：\n1. 只返回标签名，不含 # 号\n2. 用逗号分隔\n3. 优先从「系统已有标签」中选择与笔记相关的标签，避免创建含义重复的新标签\n4. 只有当已有标签都无法概括笔记主题时，才创建新标签\n5. 不要返回笔记中已经包含的标签\n6. 标签应简短（1-4个字/词），能概括笔记主题\n7. 只返回标签列表，不要其他文字";

    let user_message = if system_tags.is_empty() {
        format!("笔记内容：\n\n{}", content)
    } else {
        format!(
            "系统已有标签：\n{}\n\n笔记内容：\n\n{}",
            system_tags.join(", "),
            content
        )
    };

    let body = serde_json::json!({
        "model": provider.model,
        "messages": [
            { "role": "system", "content": system_prompt },
            { "role": "user", "content": user_message },
        ],
        "stream": false,
    });

    let url = format!(
        "{}/chat/completions",
        provider.base_url.trim_end_matches('/')
    );
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

    let ai_text = resp_json
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    // 解析 AI 返回的标签（逗号或顿号分隔），去除 # 前缀，排除笔记中已有的标签
    let suggested: Vec<String> = ai_text
        .split([',', '，', '、'])
        .map(|s| s.trim().replace('#', "").trim().to_string())
        .filter(|s| !s.is_empty() && !existing_tags.contains(s))
        .take(10)
        .collect();

    Ok(suggested)
}

// ---------- 辅助 ----------

fn build_find(req: ListMemosRequest) -> FindMemo {
    FindMemo {
        id: req.id,
        uid: req.uid,
        id_list: req.id_list.unwrap_or_default(),
        uid_list: req.uid_list.unwrap_or_default(),
        row_status: req.row_status,
        visibility_list: req.visibility_list.unwrap_or_default(),
        exclude_content: req.exclude_content,
        content_contains: req.content_contains,
        fts_query: req.fts_query,
        vector_embedding: req.vector_embedding,
        vector_top_k: req.vector_top_k,
        tag_search: req.tag_search.unwrap_or_default(),
        created_ts_after: req.created_ts_after,
        created_ts_before: req.created_ts_before,
        updated_ts_after: req.updated_ts_after,
        updated_ts_before: req.updated_ts_before,
        limit: req.limit,
        offset: req.offset,
        order_by_pinned: req.order_by_pinned,
        order_by_updated_ts: req.order_by_updated_ts,
        order_by_time_asc: req.order_by_time_asc,
    }
}
