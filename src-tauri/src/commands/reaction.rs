//! Reaction 相关 IPC 命令

use crate::error::IpcResult;
use crate::state::AppState;
use memos_core::reaction::{FindReaction, Reaction, UpsertReaction};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpsertReactionRequest {
    pub content_id: String,
    pub reaction_type: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListReactionsRequest {
    pub id: Option<i32>,
    pub content_id: Option<String>,
    pub content_id_list: Option<Vec<String>>,
}

#[tauri::command]
pub fn upsert_reaction(
    state: tauri::State<'_, AppState>,
    req: UpsertReactionRequest,
) -> IpcResult<Reaction> {
    let store = state.store();
    Ok(store.with_conn(|c| {
        memos_core::reaction::upsert(c, &UpsertReaction {
            content_id: req.content_id,
            reaction_type: req.reaction_type,
        })
    })?)
}

#[tauri::command]
pub fn list_reactions(
    state: tauri::State<'_, AppState>,
    req: ListReactionsRequest,
) -> IpcResult<Vec<Reaction>> {
    let store = state.store();
    let find = FindReaction {
        id: req.id,
        content_id: req.content_id,
        content_id_list: req.content_id_list.unwrap_or_default(),
    };
    Ok(store.with_conn(|c| memos_core::reaction::list(c, &find))?)
}

#[tauri::command]
pub fn delete_reaction(state: tauri::State<'_, AppState>, id: i32) -> IpcResult<()> {
    let store = state.store();
    store.with_conn(|c| memos_core::reaction::delete(c, id))?;
    Ok(())
}
