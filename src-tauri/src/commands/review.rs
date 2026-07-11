//! 回顾模块 Tauri 命令

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::review::{self, DeckStats, ReviewCard, ReviewDeck};
use serde::{Deserialize, Serialize};

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
