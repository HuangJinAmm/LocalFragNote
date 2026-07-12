//! 标签元数据表 CRUD：维护 tag 名称与使用次数的索引。
//!
//! content 中的 #tag 仍是单一真相源；此模块在 memo CRUD 时同步更新 tag 表，
//! 使 list_tags 从 O(N×M) 全表扫描降为 O(N_tags) 单表查询。

use crate::markdown;
use crate::error::CoreResult;
use rusqlite::{params, Connection};

/// 插入或递增标签计数（用于 memo create / unarchive）
pub fn upsert_tags_for_content(conn: &Connection, content: &str) -> CoreResult<()> {
    let tags = markdown::extract_tags(content);
    if tags.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare(
        "INSERT INTO tag (name, count, created_ts, updated_ts)
         VALUES (?1, 1, ?2, ?2)
         ON CONFLICT(name) DO UPDATE SET count = count + 1, updated_ts = ?2",
    )?;
    for tag in &tags {
        stmt.execute(params![tag, now])?;
    }
    Ok(())
}

/// 递减标签计数（用于 memo delete / archive）
/// count <= 0 的行会被自动删除
pub fn decrement_tags_for_content(conn: &Connection, content: &str) -> CoreResult<()> {
    let tags = markdown::extract_tags(content);
    if tags.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare(
        "UPDATE tag SET count = count - 1, updated_ts = ?2 WHERE name = ?1",
    )?;
    for tag in &tags {
        stmt.execute(params![tag, now])?;
    }
    conn.execute("DELETE FROM tag WHERE count <= 0", [])?;
    Ok(())
}

/// 同步标签计数（用于 memo update，content 变化时）
/// 对新增 tag count+1，对移除 tag count-1
pub fn sync_tags_on_update(
    conn: &Connection,
    old_content: &str,
    new_content: &str,
) -> CoreResult<()> {
    let old_tags: std::collections::HashSet<String> =
        markdown::extract_tags(old_content).into_iter().collect();
    let new_tags: std::collections::HashSet<String> =
        markdown::extract_tags(new_content).into_iter().collect();

    let added: Vec<&String> = new_tags.iter().filter(|t| !old_tags.contains(*t)).collect();
    let removed: Vec<&String> = old_tags.iter().filter(|t| !new_tags.contains(*t)).collect();

    if added.is_empty() && removed.is_empty() {
        return Ok(());
    }

    let now = chrono::Utc::now().timestamp();

    if !added.is_empty() {
        let mut stmt = conn.prepare(
            "INSERT INTO tag (name, count, created_ts, updated_ts)
             VALUES (?1, 1, ?2, ?2)
             ON CONFLICT(name) DO UPDATE SET count = count + 1, updated_ts = ?2",
        )?;
        for tag in &added {
            stmt.execute(params![tag, now])?;
        }
    }

    if !removed.is_empty() {
        let mut stmt = conn.prepare(
            "UPDATE tag SET count = count - 1, updated_ts = ?2 WHERE name = ?1",
        )?;
        for tag in &removed {
            stmt.execute(params![tag, now])?;
        }
        conn.execute("DELETE FROM tag WHERE count <= 0", [])?;
    }

    Ok(())
}

/// 查询所有标签及使用次数（按 count 降序、name 升序）
pub fn list_tags(conn: &Connection) -> CoreResult<Vec<(String, i32)>> {
    let mut stmt =
        conn.prepare("SELECT name, count FROM tag ORDER BY count DESC, name ASC")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i32>(1)?))
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
