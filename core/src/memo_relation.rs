//! MemoRelation 实体与 CRUD

use crate::error::{CoreError, CoreResult};
use crate::types::MemoRelationType;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// 笔记关系实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoRelation {
    pub memo_id: i32,
    pub related_memo_id: i32,
    pub r#type: MemoRelationType,
}

/// upsert
#[derive(Debug, Clone)]
pub struct UpsertMemoRelation {
    pub memo_id: i32,
    pub related_memo_id: i32,
    pub r#type: MemoRelationType,
}

/// 查询过滤
#[derive(Debug, Clone, Default)]
pub struct FindMemoRelation {
    pub memo_id: Option<i32>,
    pub related_memo_id: Option<i32>,
    pub r#type: Option<MemoRelationType>,
    pub memo_id_list: Vec<i32>,
}

/// upsert
pub fn upsert(conn: &Connection, upsert: &UpsertMemoRelation) -> CoreResult<MemoRelation> {
    conn.execute(
        "INSERT INTO memo_relation (memo_id, related_memo_id, type) VALUES (?1, ?2, ?3)
         ON CONFLICT(memo_id, related_memo_id, type) DO UPDATE SET type=excluded.type",
        params![upsert.memo_id, upsert.related_memo_id, upsert.r#type],
    )?;
    Ok(MemoRelation {
        memo_id: upsert.memo_id,
        related_memo_id: upsert.related_memo_id,
        r#type: upsert.r#type,
    })
}

/// 查询列表
pub fn list(conn: &Connection, find: &FindMemoRelation) -> CoreResult<Vec<MemoRelation>> {
    let mut sql = String::from("SELECT memo_id, related_memo_id, type FROM memo_relation WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(mid) = find.memo_id {
        sql.push_str(" AND memo_id = ?");
        args.push(Box::new(mid));
    }
    if let Some(rid) = find.related_memo_id {
        sql.push_str(" AND related_memo_id = ?");
        args.push(Box::new(rid));
    }
    if let Some(t) = find.r#type {
        sql.push_str(" AND type = ?");
        args.push(Box::new(t));
    }
    if !find.memo_id_list.is_empty() {
        let placeholders: Vec<&str> = find.memo_id_list.iter().map(|_| "?").collect();
        sql.push_str(&format!(" AND (memo_id IN ({}) OR related_memo_id IN ({}))", placeholders.join(","), placeholders.join(",")));
        // 占位符数量是 list 长度的 2 倍（两个 IN 子句各一份）
        for mid in &find.memo_id_list {
            args.push(Box::new(*mid));
        }
        for mid in &find.memo_id_list {
            args.push(Box::new(*mid));
        }
    }

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        args.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice(),
        |row| Ok(MemoRelation {
            memo_id: row.get(0)?,
            related_memo_id: row.get(1)?,
            r#type: row.get(2)?,
        }),
    )?;
    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }
    Ok(result)
}

/// 删除
pub fn delete(conn: &Connection, memo_id: i32, related_memo_id: i32, r#type: MemoRelationType) -> CoreResult<()> {
    let affected = conn.execute(
        "DELETE FROM memo_relation WHERE memo_id = ?1 AND related_memo_id = ?2 AND type = ?3",
        params![memo_id, related_memo_id, r#type],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("memo_relation {memo_id}->{related_memo_id}")));
    }
    Ok(())
}
