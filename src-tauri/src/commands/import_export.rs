//! 笔记导入导出 IPC 命令
//!
//! - JSON：完整数据备份/恢复（含所有字段）

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::memo::{CreateMemo, FindMemo, Memo};
use memos_core::types::{RowStatus, Visibility};
use memos_core::Store;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

/// 生成 16 字符 hex UID（与 ai/tools.rs 风格一致）
fn new_uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // 加一个随机后缀减少碰撞（纳秒低 64 位 + 计数器思想）
    format!("{:016x}", now & 0xFFFF_FFFF_FFFF_FFFF)
}

/// 查询所有主笔记（不含评论）
fn list_all_main_memos(store: &Store) -> IpcResult<Vec<Memo>> {
    let find = FindMemo {
        main_only: true,
        limit: None,
        offset: None,
        ..Default::default()
    };
    let memos = store.with_conn(|c| memos_core::memo::list(c, &find))?;
    Ok(memos)
}

// ==================== JSON 导出/导入 ====================

#[tauri::command]
pub fn export_memos_json(state: tauri::State<'_, AppState>) -> IpcResult<String> {
    let store = state.store();
    let memos = list_all_main_memos(&store)?;
    serde_json::to_string_pretty(&memos).map_err(|e| IpcError::Internal(format!("序列化失败: {e}")))
}

/// JSON 导入的单条记录（uid 可选，不提供则自动生成）
#[derive(Debug, Deserialize)]
struct ImportMemoRecord {
    #[serde(default)]
    uid: Option<String>,
    #[serde(default)]
    content: String,
    #[serde(default)]
    visibility: Visibility,
    #[serde(default)]
    pinned: bool,
    #[serde(default = "default_payload")]
    payload: Value,
    #[serde(default)]
    location: Option<memos_core::memo::MemoLocation>,
    #[serde(default)]
    created_ts: Option<i64>,
    #[serde(default)]
    row_status: Option<RowStatus>,
}

fn default_payload() -> Value {
    Value::Object(Default::default())
}

#[tauri::command]
pub fn import_memos_json(
    state: tauri::State<'_, AppState>,
    json_str: String,
) -> IpcResult<i32> {
    let records: Vec<ImportMemoRecord> =
        serde_json::from_str(&json_str).map_err(|e| IpcError::Internal(format!("JSON 解析失败: {e}")))?;

    let store = state.store();
    let conn = store.lock_conn();
    let mut imported = 0i32;
    for rec in &records {
        let uid = rec.uid.clone().unwrap_or_else(new_uid);
        // 如果 uid 已存在，跳过（幂等导入）
        if uid_exists(&conn, &uid)? {
            continue;
        }
        let memo = create_with_timestamp(
            &conn,
            &CreateMemo {
                uid,
                content: rec.content.clone(),
                visibility: rec.visibility,
                pinned: rec.pinned,
                payload: rec.payload.clone(),
                location: rec.location.clone(),
                parent_id: None,
            },
            rec.created_ts,
        )?;
        // 若标记为归档，创建后更新 row_status
        if let Some(RowStatus::Archived) = rec.row_status {
            archive_memo(&conn, memo.id)?;
        }
        imported += 1;
    }

    Ok(imported)
}

/// 检查 uid 是否已存在
fn uid_exists(c: &Connection, uid: &str) -> IpcResult<bool> {
    let count: i32 = c
        .query_row(
            "SELECT COUNT(*) FROM memo WHERE uid = ?1",
            rusqlite::params![uid],
            |r| r.get(0),
        )
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(count > 0)
}

/// 创建 memo 并覆盖 created_ts（用于导入时保留原始时间）
fn create_with_timestamp(
    c: &Connection,
    create: &CreateMemo,
    created_ts: Option<i64>,
) -> IpcResult<Memo> {
    let memo = memos_core::memo::create(c, create)?;
    if let Some(ts) = created_ts {
        c.execute(
            "UPDATE memo SET created_ts = ?1 WHERE id = ?2",
            rusqlite::params![ts, memo.id],
        )
        .map_err(|e| IpcError::Internal(e.to_string()))?;
        // 返回更新后的 memo
        let find = FindMemo { id: Some(memo.id), ..Default::default() };
        return Ok(memos_core::memo::get(c, &find)?
            .ok_or_else(|| IpcError::Internal("导入后查询失败".into()))?);
    }
    Ok(memo)
}

/// 将 memo 标记为归档
fn archive_memo(c: &Connection, id: i32) -> IpcResult<()> {
    c.execute(
        "UPDATE memo SET row_status = 'ARCHIVED' WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(())
}
