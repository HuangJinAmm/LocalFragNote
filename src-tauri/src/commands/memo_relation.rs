//! MemoRelation 相关 IPC 命令

use crate::error::IpcResult;
use crate::state::AppState;
use memos_core::memo_relation::{FindMemoRelation, MemoRelation, UpsertMemoRelation};
use memos_core::types::MemoRelationType;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct UpsertMemoRelationRequest {
    pub memo_id: i32,
    pub related_memo_id: i32,
    pub r#type: MemoRelationType,
}

#[derive(Debug, Deserialize, Default)]
pub struct ListMemoRelationsRequest {
    pub memo_id: Option<i32>,
    pub related_memo_id: Option<i32>,
    pub r#type: Option<MemoRelationType>,
    pub memo_id_list: Option<Vec<i32>>,
}

#[derive(Debug, Deserialize)]
pub struct DeleteMemoRelationRequest {
    pub memo_id: i32,
    pub related_memo_id: i32,
    pub r#type: MemoRelationType,
}

#[tauri::command]
pub fn upsert_memo_relation(
    state: tauri::State<'_, AppState>,
    req: UpsertMemoRelationRequest,
) -> IpcResult<MemoRelation> {
    let store = state.store();
    Ok(store.with_conn(|c| {
        memos_core::memo_relation::upsert(c, &UpsertMemoRelation {
            memo_id: req.memo_id,
            related_memo_id: req.related_memo_id,
            r#type: req.r#type,
        })
    })?)
}

#[tauri::command]
pub fn list_memo_relations(
    state: tauri::State<'_, AppState>,
    req: ListMemoRelationsRequest,
) -> IpcResult<Vec<MemoRelation>> {
    let store = state.store();
    let find = FindMemoRelation {
        memo_id: req.memo_id,
        related_memo_id: req.related_memo_id,
        r#type: req.r#type,
        memo_id_list: req.memo_id_list.unwrap_or_default(),
    };
    Ok(store.with_conn(|c| memos_core::memo_relation::list(c, &find))?)
}

#[tauri::command]
pub fn delete_memo_relation(
    state: tauri::State<'_, AppState>,
    req: DeleteMemoRelationRequest,
) -> IpcResult<()> {
    let store = state.store();
    store.with_conn(|c| {
        memos_core::memo_relation::delete(c, req.memo_id, req.related_memo_id, req.r#type)
    })?;
    Ok(())
}
