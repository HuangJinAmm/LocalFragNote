//! 回顾模块 Tauri 命令

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::review::{self, DeckStats, ReviewCard, ReviewDeck};
use serde::{Deserialize, Serialize};

use crate::ai::provider::{load_providers, ProviderConfig};
use crate::ai::sse::read_sse_stream;
use crate::ai::tools::{execute_tool, tool_definitions};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU32, Ordering};
use tauri::{AppHandle, Emitter, Manager};

fn is_app_shutting_down(app: &AppHandle) -> bool {
    app.state::<AppState>().shutdown.load(Ordering::SeqCst)
}

// ==================== 返回类型 ====================

#[derive(Debug, Serialize)]
pub struct ScoreResult {
    pub updated_card: ReviewCard,
    pub next_card: Option<ReviewCard>,
    pub session_stats: SessionStats,
}

#[derive(Debug, Serialize)]
pub struct SessionStats {
    pub reviewed: u32,
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
    pub retention_rate: f32,
}

// ==================== Deck 命令 ====================

#[tauri::command]
pub fn review_list_decks(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ReviewDeck>> {
    let store = state.store();
    Ok(store.with_conn(|c| review::list_decks(c))?)
}

#[tauri::command]
pub fn review_create_deck(
    state: tauri::State<'_, AppState>,
    name: String,
    tags: Vec<String>,
    cards_per_memo: i32,
) -> IpcResult<ReviewDeck> {
    if name.trim().is_empty() {
        return Err(IpcError::BadRequest("name 不能为空".into()));
    }
    let cards_per_memo = cards_per_memo.clamp(1, 10);
    let store = state.store();
    Ok(store.with_conn(|c| review::create_deck(c, &name, &tags, cards_per_memo))?)
}

#[tauri::command]
pub fn review_update_deck(
    state: tauri::State<'_, AppState>,
    id: i32,
    name: String,
    tags: Vec<String>,
    cards_per_memo: i32,
) -> IpcResult<ReviewDeck> {
    let cards_per_memo = cards_per_memo.clamp(1, 10);
    let store = state.store();
    Ok(store.with_conn(|c| review::update_deck(c, id, &name, &tags, cards_per_memo))?)
}

#[tauri::command]
pub fn review_delete_deck(state: tauri::State<'_, AppState>, id: i32) -> IpcResult<()> {
    let store = state.store();
    Ok(store.with_conn(|c| review::delete_deck(c, id))?)
}

// ==================== Card 命令 ====================

#[tauri::command]
pub fn review_list_cards(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<Vec<ReviewCard>> {
    let store = state.store();
    Ok(store.with_conn(|c| review::list_cards(c, deck_id))?)
}

#[tauri::command]
pub fn review_list_due_cards(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
    limit: Option<i32>,
) -> IpcResult<Vec<ReviewCard>> {
    let limit = limit.unwrap_or(50).clamp(1, 500);
    let store = state.store();
    Ok(store.with_conn(|c| review::list_due_cards(c, deck_id, limit))?)
}

#[tauri::command]
pub fn review_delete_card(state: tauri::State<'_, AppState>, card_id: i32) -> IpcResult<()> {
    let store = state.store();
    Ok(store.with_conn(|c| review::delete_card(c, card_id))?)
}

// ==================== 统计命令 ====================

#[tauri::command]
pub fn review_deck_stats(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<DeckStats> {
    let store = state.store();
    Ok(store.with_conn(|c| review::deck_stats(c, deck_id))?)
}

#[tauri::command]
pub fn review_check_new_memos(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<i32> {
    let store = state.store();
    let deck = store
        .with_conn(|c| review::get_deck(c, deck_id))?
        .ok_or_else(|| IpcError::NotFound(format!("deck id={deck_id}")))?;

    // 查询当前 tag 下的 memo 数
    let current_count = store.with_conn(|c| -> memos_core::CoreResult<i32> {
        let find = memos_core::memo::FindMemo {
            tag_search: deck.tags.clone(),
            row_status: Some(memos_core::types::RowStatus::Normal),
            ..Default::default()
        };
        let memos = memos_core::memo::list(c, &find)?;
        Ok(memos.len() as i32)
    })?;

    let new_count = (current_count - deck.memo_count).max(0);
    Ok(new_count)
}

// ==================== 复习命令 ====================

/// 评分卡片
///
/// rating: 1=Again 2=Hard 3=Good 4=Easy
#[tauri::command]
pub fn review_score_card(
    state: tauri::State<'_, AppState>,
    card_id: i32,
    rating: u8,
    deck_id: i32,
) -> IpcResult<ScoreResult> {
    let store = state.store();

    // 读取 FSRS 参数（空=默认）
    let fsrs_params: Vec<f32> = store
        .with_conn(|c| store.setting.app.get(c, "fsrs_params"))?
        .and_then(|json| serde_json::from_str::<Vec<f32>>(&json).ok())
        .unwrap_or_default();

    // 评分并更新卡片
    let (updated_card, _record) =
        store.with_conn(|c| review::score_card(c, card_id, rating, &fsrs_params))?;

    // 更新 deck 的 last_reviewed_ts
    store.with_conn(|c| review::touch_deck_reviewed(c, deck_id))?;

    // 查询下一张到期卡片
    let next_cards = store.with_conn(|c| review::list_due_cards(c, deck_id, 1))?;
    let next_card = next_cards.into_iter().next();

    // 计算本次 session 统计（最近 1 小时内该 deck 的评分）
    let session_stats = store.with_conn(|c| compute_session_stats(c, deck_id))?;

    Ok(ScoreResult {
        updated_card,
        next_card,
        session_stats,
    })
}

/// 计算最近 1 小时的 session 统计
fn compute_session_stats(
    conn: &rusqlite::Connection,
    deck_id: i32,
) -> memos_core::CoreResult<SessionStats> {
    let one_hour_ago = chrono::Utc::now().timestamp() - 3600;

    let stats: (u32, u32, u32, u32, u32, f32) = conn.query_row(
        "SELECT
            COUNT(*) as reviewed,
            SUM(CASE WHEN r.rating = 1 THEN 1 ELSE 0 END) as again,
            SUM(CASE WHEN r.rating = 2 THEN 1 ELSE 0 END) as hard,
            SUM(CASE WHEN r.rating = 3 THEN 1 ELSE 0 END) as good,
            SUM(CASE WHEN r.rating = 4 THEN 1 ELSE 0 END) as easy,
            CASE WHEN COUNT(*) > 0
                THEN CAST(SUM(CASE WHEN r.rating IN (3, 4) THEN 1 ELSE 0 END) AS FLOAT) / COUNT(*)
                ELSE 0.0
            END as retention
         FROM review_record r
         JOIN review_card c ON r.card_id = c.id
         WHERE c.deck_id = ?1 AND r.reviewed_ts >= ?2",
        rusqlite::params![deck_id, one_hour_ago],
        |row| {
            Ok((
                row.get::<_, i64>(0)? as u32,
                row.get::<_, i64>(1)? as u32,
                row.get::<_, i64>(2)? as u32,
                row.get::<_, i64>(3)? as u32,
                row.get::<_, i64>(4)? as u32,
                row.get::<_, f64>(5)? as f32,
            ))
        },
    )?;

    Ok(SessionStats {
        reviewed: stats.0,
        again: stats.1,
        hard: stats.2,
        good: stats.3,
        easy: stats.4,
        retention_rate: stats.5,
    })
}

// ==================== AI 卡片生成 ====================

static REVIEW_RUN_ID: AtomicU32 = AtomicU32::new(1);

const CARD_GEN_SYSTEM_PROMPT: &str = r#"你是一个记忆卡片生成专家。根据用户的笔记内容，生成 ANKI 风格的记忆卡片。

## 卡片类型
- basic: 问答卡（正面问题，背面答案）
- reversed: 翻转卡（正面术语，背面定义）
- cloze: 填空卡（front 带 {{答案}} 占位，cloze_answer 存答案词）
- concept: 概念解释卡（"请解释：X" → 完整解释）
- compare: 对比卡（"对比 A 和 B" → 异同点）

## 输出格式
返回 JSON 数组，每个元素：
{
  "memo_uid": "来源 memo 的 uid",
  "card_type": "basic|reversed|cloze|concept|compare",
  "front": "正面内容（Markdown）",
  "back": "背面内容（Markdown）",
  "cloze_answer": "填空答案（仅 cloze 类型，其他为 null）",
  "angle": "考核点，如：定义|应用|对比|列举|原理"
}

## 规则
1. 每条 memo 最多生成指定数量的卡片
2. 优先生成核心知识点，避免琐碎细节
3. front/back 必须独立完整，不依赖其他卡片
4. 同一 memo 的不同卡片应覆盖不同 angle
5. 只返回 JSON 数组，不要其他文字"#;

#[derive(Debug, Clone, Serialize)]
struct ReviewGenStarted {
    deck_id: i32,
    run_id: u32,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewGenDone {
    deck_id: i32,
    run_id: u32,
    count: usize,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewGenError {
    deck_id: i32,
    run_id: u32,
    error: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewChunk {
    run_id: u32,
    text: String,
}

/// AI 生成的卡片草案（从 JSON 解析）
#[derive(Debug, Deserialize)]
struct CardDraft {
    memo_uid: String,
    card_type: String,
    front: String,
    back: String,
    cloze_answer: Option<String>,
    angle: Option<String>,
}

/// 生成卡片（异步，通过事件推送进度）
#[tauri::command]
pub async fn review_generate_cards(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<u32> {
    let run_id = REVIEW_RUN_ID.fetch_add(1, Ordering::SeqCst);

    // 读取 deck
    let deck = {
        let store = state.store();
        store
            .with_conn(|c| review::get_deck(c, deck_id))?
            .ok_or_else(|| IpcError::NotFound(format!("deck id={deck_id}")))?
    };

    // 读取 AI provider
    let provider = {
        let store = state.store();
        let providers = load_providers(&store);
        let config_json = store
            .with_conn(|c| store.setting.app.get(c, "review_config"))?
            .unwrap_or_default();
        let provider_id: String = serde_json::from_str::<Value>(&config_json)
            .ok()
            .and_then(|v| {
                v.get("ai_provider_id")
                    .and_then(|s| s.as_str().map(String::from))
            })
            .unwrap_or_default();
        if !provider_id.is_empty() {
            providers
                .iter()
                .find(|p| p.id == provider_id)
                .cloned()
                .ok_or_else(|| {
                    IpcError::BadRequest("review_config 指定的 provider 不存在".into())
                })?
        } else if let Some(first) = providers.first() {
            first.clone()
        } else {
            return Err(IpcError::BadRequest(
                "未配置 AI provider，请先在设置中配置".into(),
            ));
        }
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        card_generation_loop(app_handle, run_id, deck_id, deck, provider);
    });

    Ok(run_id)
}

/// "换角度"重新生成单张卡片
#[tauri::command]
pub async fn review_regenerate_card(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    card_id: i32,
) -> IpcResult<u32> {
    let run_id = REVIEW_RUN_ID.fetch_add(1, Ordering::SeqCst);

    // 读取原卡片
    let (card, deck_id) = {
        let store = state.store();
        let card = store
            .with_conn(|c| review::get_card(c, card_id))?
            .ok_or_else(|| IpcError::NotFound(format!("card id={card_id}")))?;
        let deck_id = card.deck_id;
        (card, deck_id)
    };

    // 读取 deck
    let deck = {
        let store = state.store();
        store
            .with_conn(|c| review::get_deck(c, deck_id))?
            .ok_or_else(|| IpcError::NotFound(format!("deck id={deck_id}")))?
    };

    // 读取 provider
    let provider = {
        let store = state.store();
        let providers = load_providers(&store);
        providers
            .first()
            .cloned()
            .ok_or_else(|| IpcError::BadRequest("未配置 AI provider".into()))?
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        card_regeneration_loop(app_handle, run_id, deck_id, card, deck, provider);
    });

    Ok(run_id)
}

/// 卡片生成 agent loop
fn card_generation_loop(
    app: AppHandle,
    run_id: u32,
    deck_id: i32,
    deck: ReviewDeck,
    provider: ProviderConfig,
) {
    if is_app_shutting_down(&app) {
        return;
    }

    let _ = app.emit(
        "review:generation-started",
        ReviewGenStarted { deck_id, run_id },
    );

    let tags_str = deck.tags.join(", ");
    let user_prompt = format!(
        "请为以下标签的笔记生成记忆卡片：\n- 标签：{}\n- 每条笔记最多生成 {} 张卡片\n\n使用 list_memos_by_tag 工具读取笔记内容。只返回 JSON 数组。",
        tags_str, deck.cards_per_memo
    );

    let messages = vec![json!({
        "role": "user",
        "content": user_prompt,
    })];

    let result = run_card_agent(&app, run_id, &provider, &messages);

    match result {
        Ok(content) => {
            if is_app_shutting_down(&app) {
                return;
            }
            let drafts = parse_card_json(&content);
            let mut errors = Vec::new();

            let state = app.state::<AppState>();
            let store = state.store();

            // 查询当前 tag 下的 memo 数
            let memo_count = store
                .with_conn(|c| -> memos_core::CoreResult<i32> {
                    let find = memos_core::memo::FindMemo {
                        tag_search: deck.tags.clone(),
                        row_status: Some(memos_core::types::RowStatus::Normal),
                        ..Default::default()
                    };
                    Ok(memos_core::memo::list(c, &find)?.len() as i32)
                })
                .unwrap_or(0);

            let mut inserted = 0;
            for draft in &drafts {
                if is_app_shutting_down(&app) {
                    return;
                }
                match store.with_conn(|c| {
                    let now = chrono::Utc::now().timestamp();
                    let card = ReviewCard {
                        id: 0,
                        deck_id,
                        memo_uid: draft.memo_uid.clone(),
                        card_type: draft.card_type.clone(),
                        front: draft.front.clone(),
                        back: draft.back.clone(),
                        cloze_answer: draft.cloze_answer.clone(),
                        angle: draft.angle.clone().unwrap_or_default(),
                        stability: 0.0,
                        difficulty: 0.0,
                        due: now,
                        last_review: None,
                        reps: 0,
                        lapses: 0,
                        state: 0,
                        created_ts: now,
                        memo_deleted: false,
                    };
                    review::create_card(c, &card)
                }) {
                    Ok(_) => inserted += 1,
                    Err(e) => errors.push(format!("card memo_uid={}: {e}", draft.memo_uid)),
                }
            }

            // 更新 deck.memo_count
            let _ = store.with_conn(|c| -> memos_core::CoreResult<usize> {
                Ok(c.execute(
                    "UPDATE review_deck SET memo_count = ?1 WHERE id = ?2",
                    rusqlite::params![memo_count, deck_id],
                )?)
            });

            let _ = app.emit(
                "review:cards-generated",
                ReviewGenDone {
                    deck_id,
                    run_id,
                    count: inserted,
                    errors,
                },
            );
        }
        Err(e) => {
            let _ = app.emit(
                "review:generation-error",
                ReviewGenError {
                    deck_id,
                    run_id,
                    error: e,
                },
            );
        }
    }
}

/// "换角度"重新生成 loop
fn card_regeneration_loop(
    app: AppHandle,
    run_id: u32,
    deck_id: i32,
    old_card: ReviewCard,
    _deck: ReviewDeck,
    provider: ProviderConfig,
) {
    if is_app_shutting_down(&app) {
        return;
    }

    let _ = app.emit(
        "review:generation-started",
        ReviewGenStarted { deck_id, run_id },
    );

    // 读取该 memo 已有的卡片 angle
    let existing_angles = {
        let state = app.state::<AppState>();
        let store = state.store();
        store
            .with_conn(|c| {
                let cards = review::list_cards(c, deck_id)?;
                Ok::<_, memos_core::CoreError>(
                    cards
                        .iter()
                        .filter(|c| c.memo_uid == old_card.memo_uid)
                        .map(|c| c.angle.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            })
            .unwrap_or_default()
    };

    let user_prompt = format!(
        "请为 memo（uid: {}）生成一张新的记忆卡片，从不同考核点出题。\n已存在的考核点：[{}]，请避免重复。\n使用 get_memo 工具读取该 memo 内容。只返回 JSON 数组（1 个元素）。",
        old_card.memo_uid, existing_angles
    );

    let messages = vec![json!({
        "role": "user",
        "content": user_prompt,
    })];

    let result = run_card_agent(&app, run_id, &provider, &messages);

    match result {
        Ok(content) => {
            if is_app_shutting_down(&app) {
                return;
            }
            let drafts = parse_card_json(&content);
            let state = app.state::<AppState>();
            let store = state.store();
            let mut inserted = 0;
            let mut errors = Vec::new();

            for draft in &drafts {
                if is_app_shutting_down(&app) {
                    return;
                }
                match store.with_conn(|c| {
                    let now = chrono::Utc::now().timestamp();
                    let card = ReviewCard {
                        id: 0,
                        deck_id,
                        memo_uid: draft.memo_uid.clone(),
                        card_type: draft.card_type.clone(),
                        front: draft.front.clone(),
                        back: draft.back.clone(),
                        cloze_answer: draft.cloze_answer.clone(),
                        angle: draft.angle.clone().unwrap_or_default(),
                        stability: 0.0,
                        difficulty: 0.0,
                        due: now,
                        last_review: None,
                        reps: 0,
                        lapses: 0,
                        state: 0,
                        created_ts: now,
                        memo_deleted: false,
                    };
                    review::create_card(c, &card)
                }) {
                    Ok(_) => inserted += 1,
                    Err(e) => errors.push(format!("{e}")),
                }
            }

            let _ = app.emit(
                "review:cards-generated",
                ReviewGenDone {
                    deck_id,
                    run_id,
                    count: inserted,
                    errors,
                },
            );
        }
        Err(e) => {
            let _ = app.emit(
                "review:generation-error",
                ReviewGenError {
                    deck_id,
                    run_id,
                    error: e,
                },
            );
        }
    }
}

/// 运行卡片生成 agent loop，返回最终 assistant 内容
fn run_card_agent(
    app: &AppHandle,
    run_id: u32,
    provider: &ProviderConfig,
    messages: &[Value],
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let mut msgs: Vec<Value> = messages.to_vec();
    let system_msg = json!({"role": "system", "content": CARD_GEN_SYSTEM_PROMPT});

    for _round in 0..5 {
        if state.shutdown.load(Ordering::SeqCst) {
            return Err("应用正在退出，已取消卡片生成".into());
        }

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

        let response = req
            .send_string(&body.to_string())
            .map_err(|e| format!("HTTP 请求失败: {e}"))?;

        let status = response.status();
        if status >= 400 {
            let body_text = response.into_string().unwrap_or_default();
            return Err(format!("HTTP {status}: {body_text}"));
        }

        let reader = response.into_reader();
        let chunk_app = app.clone();
        let (content, tool_calls) = read_sse_stream(reader, |delta| {
            let _ = chunk_app.emit(
                "review:chunk",
                ReviewChunk {
                    run_id,
                    text: delta.to_string(),
                },
            );
        })
        .map_err(|e| format!("SSE 读取失败: {e}"))?;

        if state.shutdown.load(Ordering::SeqCst) {
            return Err("应用正在退出，已取消卡片生成".into());
        }

        if tool_calls.is_empty() {
            return Ok(content);
        }

        // 执行工具调用
        let assistant_tool_calls: Vec<Value> = tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments },
                })
            })
            .collect();
        msgs.push(json!({
            "role": "assistant",
            "content": content,
            "tool_calls": assistant_tool_calls,
        }));

        // 每个工具调用单独获取/释放 Store 锁，避免长时间持锁阻塞其他 DB 操作
        for tc in &tool_calls {
            if state.shutdown.load(Ordering::SeqCst) {
                return Err("应用正在退出，已取消卡片生成".into());
            }
            let args: Value = serde_json::from_str(&tc.arguments).unwrap_or(Value::Null);
            let result = {
                let store = state.store();
                execute_tool(&tc.name, &args, &store)
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

    Err("超过最大工具调用轮次".into())
}

/// 从 AI 输出中解析卡片 JSON
fn parse_card_json(content: &str) -> Vec<CardDraft> {
    // 尝试直接解析
    if let Ok(drafts) = serde_json::from_str::<Vec<CardDraft>>(content) {
        return drafts;
    }
    // 尝试提取 JSON 数组片段
    if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            if end > start {
                if let Ok(drafts) =
                    serde_json::from_str::<Vec<CardDraft>>(&content[start..=end])
                {
                    return drafts;
                }
            }
        }
    }
    Vec::new()
}
