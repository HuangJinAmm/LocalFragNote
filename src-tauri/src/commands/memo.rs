//! Memo 相关 IPC 命令

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::markdown;
use memos_core::memo::{CreateMemo, FindMemo, Memo, MemoLocation, UpdateMemo};
use memos_core::Store;
use memos_core::types::{RowStatus, Visibility};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::atomic::Ordering;
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
    /// 父 memo id；Some(id) 创建评论，None 创建主笔记
    #[serde(default)]
    pub parent_id: Option<i32>,
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
    /// 仅查置顶 memo
    pub pinned_only: Option<bool>,
    /// 过滤包含链接的 memo
    pub has_link: Option<bool>,
    /// 过滤包含任务列表的 memo
    pub has_task_list: Option<bool>,
    /// 过滤包含代码块的 memo
    pub has_code: Option<bool>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    #[serde(default)]
    pub order_by_pinned: bool,
    #[serde(default)]
    pub order_by_updated_ts: bool,
    #[serde(default)]
    pub order_by_time_asc: bool,
    /// Some(id) = 查指定父 memo 的评论；None = 查主笔记（默认）
    #[serde(default)]
    pub comments_of: Option<i32>,
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

// ---------- embedding helper ----------

fn should_store_embedding(row_status: RowStatus) -> bool {
    matches!(row_status, RowStatus::Normal)
}

pub(crate) fn delete_memo_embedding(store: &Store, id: i32) -> IpcResult<()> {
    store.with_conn(|c| {
        c.execute("DELETE FROM memo_vec WHERE rowid = ?", params![id])?;
        Ok(())
    })?;
    Ok(())
}

pub(crate) fn upsert_memo_embedding(store: &Store, id: i32, content: &str) -> IpcResult<()> {
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

pub(crate) fn sync_memo_embedding_for_memo(store: &Store, memo: &Memo) -> IpcResult<()> {
    if should_store_embedding(memo.row_status) {
        upsert_memo_embedding(store, memo.id, &memo.content)
    } else {
        delete_memo_embedding(store, memo.id)
    }
}

pub(crate) fn spawn_sync_memo_embedding(app: tauri::AppHandle, memo: Memo, action_label: &'static str) {
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

// ---------- 命令 ----------

#[tauri::command]
pub fn create_memo(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    req: CreateMemoRequest,
) -> IpcResult<Memo> {
    let is_comment = req.parent_id.is_some();
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
                parent_id: req.parent_id,
            })
        })?
    };
    // 评论不做 embedding（仅主笔记参与向量搜索）
    if !is_comment {
        spawn_sync_memo_embedding(app, memo.clone(), "创建");
    }

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

/// 列出指定 memo 的评论
/// limit = Some(n) 时只返回前 n 条；order_desc = true 时按创建时间降序（最新在前）
#[tauri::command]
pub fn list_memo_comments(
    state: tauri::State<'_, AppState>,
    parent_id: i32,
    limit: Option<i32>,
    order_desc: Option<bool>,
) -> IpcResult<Vec<Memo>> {
    let store = state.store();
    let memos = store.with_conn(|c| {
        memos_core::memo::list(c, &FindMemo {
            comments_of: Some(parent_id),
            order_by_time_asc: !order_desc.unwrap_or(false),
            limit,
            ..Default::default()
        })
    })?;
    Ok(memos)
}

/// 批量查询指定 memos 的评论数（仅主笔记有评论，评论返回 0）
#[tauri::command]
pub fn count_memo_comments_batch(
    state: tauri::State<'_, AppState>,
    parent_ids: Vec<i32>,
) -> IpcResult<Vec<(i32, i32)>> {
    if parent_ids.is_empty() {
        return Ok(Vec::new());
    }
    let store = state.store();
    let counts = store.with_conn(|c| -> memos_core::error::CoreResult<Vec<(i32, i32)>> {
        let placeholders = parent_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT parent_id, COUNT(*) FROM memo WHERE parent_id IN ({}) GROUP BY parent_id",
            placeholders
        );
        let mut stmt = c.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(parent_ids.iter()),
            |r| Ok((r.get::<_, i32>(0)?, r.get::<_, i32>(1)?)),
        )?;
        let mut result: Vec<(i32, i32)> = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    })?;
    Ok(counts)
}

#[tauri::command]
pub fn update_memo(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    req: UpdateMemoRequest,
) -> IpcResult<Memo> {
    let should_sync_embedding = req.content.is_some() || req.row_status.is_some();
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

    // 评论不做 embedding（仅主笔记参与向量搜索）
    if should_sync_embedding && updated.parent_id.is_none() {
        spawn_sync_memo_embedding(app, updated.clone(), "更新");
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
    let tags = store.with_conn(|c| memos_core::tag::list_tags(c))?;
    Ok(tags
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

    // 查询系统已有标签，提供给 AI 优先复用
    let system_tags: Vec<String> = store.with_conn(|c| -> memos_core::CoreResult<Vec<String>> {
        Ok(memos_core::tag::list_tags(c)?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    })?;

    let system_prompt = r"你是一位专业的笔记标签建议专家，擅长精准提取笔记的核心主题和所属科目。
    标签分两种类型：
    1. 分类标签: 用于笔记归类的科目类别（至少包含1个）。
    2. 主题标签: 精准概括笔记的核心主题。
    根据用户提供的笔记内容，建议 3-5 个合适的标签。至少需要1个分类标签。
    规则：
    1. 只返回标签名，不含 # 号
    2. 用逗号分隔
    3. 优先从「系统已有标签」中选择与笔记相关的标签，避免创建含义重复的新标签
    4. 只有当已有标签都无法归类的科目类别或概括笔记主题时，才创建新标签
    5. 不要返回笔记中已经包含的标签
    6. 标签应简短（1-4个字/词），能概括笔记主题
    7. 只返回标签列表，不要其他文字";

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
    // comments_of 优先：查评论时不过滤 main_only；否则默认只查主笔记
    let (main_only, comments_of) = if let Some(parent_id) = req.comments_of {
        (false, Some(parent_id))
    } else {
        (true, None)
    };
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
        pinned_only: req.pinned_only,
        has_link: req.has_link,
        has_task_list: req.has_task_list,
        has_code: req.has_code,
        limit: req.limit,
        offset: req.offset,
        order_by_pinned: req.order_by_pinned,
        order_by_updated_ts: req.order_by_updated_ts,
        order_by_time_asc: req.order_by_time_asc,
        main_only,
        comments_of,
    }
}
