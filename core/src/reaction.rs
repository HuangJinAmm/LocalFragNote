//! Reaction 实体与 CRUD

use crate::error::{CoreError, CoreResult};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// 反应实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reaction {
    pub id: i32,
    pub created_ts: i64,
    pub content_id: String,
    pub reaction_type: String,
}

/// 创建/更新（upsert）
#[derive(Debug, Clone)]
pub struct UpsertReaction {
    pub content_id: String,
    pub reaction_type: String,
}

/// 查询过滤
#[derive(Debug, Clone, Default)]
pub struct FindReaction {
    pub id: Option<i32>,
    pub content_id: Option<String>,
    pub content_id_list: Vec<String>,
}

/// upsert
pub fn upsert(conn: &Connection, upsert: &UpsertReaction) -> CoreResult<Reaction> {
    conn.execute(
        "INSERT INTO reaction (content_id, reaction_type) VALUES (?1, ?2)
         ON CONFLICT(content_id, reaction_type) DO UPDATE SET content_id=excluded.content_id",
        params![&upsert.content_id, &upsert.reaction_type],
    )?;
    get(conn, &FindReaction {
        content_id: Some(upsert.content_id.clone()),
        ..Default::default()
    })?
    .ok_or_else(|| CoreError::NotFound("reaction".into()))
}

/// 查询单条
pub fn get(conn: &Connection, find: &FindReaction) -> CoreResult<Option<Reaction>> {
    let list = list(conn, find)?;
    Ok(list.into_iter().next())
}

/// 查询列表
pub fn list(conn: &Connection, find: &FindReaction) -> CoreResult<Vec<Reaction>> {
    let mut sql = String::from("SELECT id, created_ts, content_id, reaction_type FROM reaction WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(id) = find.id {
        sql.push_str(" AND id = ?");
        args.push(Box::new(id));
    }
    if let Some(ref content_id) = find.content_id {
        sql.push_str(" AND content_id = ?");
        args.push(Box::new(content_id.clone()));
    }
    if !find.content_id_list.is_empty() {
        let placeholders: Vec<&str> = find.content_id_list.iter().map(|_| "?").collect();
        sql.push_str(&format!(" AND content_id IN ({})", placeholders.join(",")));
        for cid in &find.content_id_list {
            args.push(Box::new(cid.clone()));
        }
    }
    sql.push_str(" ORDER BY created_ts ASC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        args.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice(),
        |row| Ok(Reaction {
            id: row.get(0)?,
            created_ts: row.get(1)?,
            content_id: row.get(2)?,
            reaction_type: row.get(3)?,
        }),
    )?;
    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }
    Ok(result)
}

/// 删除
pub fn delete(conn: &Connection, id: i32) -> CoreResult<()> {
    let affected = conn.execute("DELETE FROM reaction WHERE id = ?", params![id])?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("reaction id={id}")));
    }
    Ok(())
}
