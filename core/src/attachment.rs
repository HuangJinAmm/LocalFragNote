//! Attachment 实体与 CRUD

use crate::error::{CoreError, CoreResult};
use crate::types::validate_uid;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 附件存储类型
pub const STORAGE_TYPE_DATABASE: &str = "DATABASE";
pub const STORAGE_TYPE_LOCAL: &str = "LOCAL";

/// 附件实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub id: i32,
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub filename: String,
    #[serde(skip_serializing)]
    pub blob: Option<Vec<u8>>,
    pub r#type: String,
    pub size: i64,
    pub memo_id: Option<i32>,
    pub storage_type: String,
    pub reference: String,
    pub payload: Value,
}

/// 创建附件
#[derive(Debug, Clone)]
pub struct CreateAttachment {
    pub uid: String,
    pub filename: String,
    /// DATABASE 模式：原始 blob；LOCAL 模式：可传空 Vec
    pub blob: Vec<u8>,
    pub r#type: String,
    pub memo_id: Option<i32>,
    /// "DATABASE" 或 "LOCAL"
    pub storage_type: String,
    /// LOCAL 模式下的相对路径；DATABASE 模式留空
    pub reference: String,
    /// 文件大小（字节）。LOCAL 模式必须正确传入；DATABASE 模式从 blob.len() 推导
    pub size: Option<i64>,
}

/// 更新附件
#[derive(Debug, Clone, Default)]
pub struct UpdateAttachment {
    pub id: i32,
    pub filename: Option<String>,
    pub memo_id: Option<Option<i32>>,
    pub payload: Option<Value>,
}

/// 查询过滤
#[derive(Debug, Clone, Default)]
pub struct FindAttachment {
    pub id: Option<i32>,
    pub uid: Option<String>,
    pub memo_id: Option<i32>,
    /// 批量按 memo_id 查询（OR 条件）
    pub memo_id_list: Vec<i32>,
    /// 若为 true，过滤 memo_id IS NULL（未关联 memo 的附件）
    pub memo_id_is_null: bool,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub get_blob: bool,
}

/// 创建
pub fn create(conn: &Connection, create: &CreateAttachment) -> CoreResult<Attachment> {
    if !validate_uid(&create.uid) {
        return Err(CoreError::InvalidUid);
    }
    let (size, blob_param): (i64, Option<&[u8]>) = if create.storage_type == STORAGE_TYPE_LOCAL {
        // LOCAL 模式：blob 字段留 NULL，size 由调用方传入
        (create.size.unwrap_or(0), None)
    } else {
        // DATABASE 模式：blob 存数据库
        (create.size.unwrap_or(create.blob.len() as i64), Some(&create.blob))
    };

    conn.execute(
        "INSERT INTO attachment (uid, filename, blob, type, size, memo_id, storage_type, reference)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            &create.uid,
            &create.filename,
            blob_param,
            &create.r#type,
            size,
            create.memo_id,
            &create.storage_type,
            &create.reference,
        ],
    )
    .map_err(|e| {
        if let rusqlite::Error::SqliteFailure(ref f, _) = e {
            if f.code == rusqlite::ErrorCode::ConstraintViolation {
                return CoreError::UidConflict(create.uid.clone());
            }
        }
        CoreError::Db(e)
    })?;

    let id = conn.last_insert_rowid() as i32;
    get(conn, &FindAttachment { id: Some(id), get_blob: false, ..Default::default() })?
        .ok_or_else(|| CoreError::NotFound(format!("attachment id={id}")))
}

/// 查询单条
pub fn get(conn: &Connection, find: &FindAttachment) -> CoreResult<Option<Attachment>> {
    let list = list(conn, find)?;
    Ok(list.into_iter().next())
}

/// 查询列表
pub fn list(conn: &Connection, find: &FindAttachment) -> CoreResult<Vec<Attachment>> {
    let mut sql = String::from("SELECT id, uid, created_ts, updated_ts, filename, ");
    if find.get_blob {
        sql.push_str("blob, ");
    } else {
        sql.push_str("NULL AS blob, ");
    }
    sql.push_str("type, size, memo_id, storage_type, reference, payload FROM attachment WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(id) = find.id {
        sql.push_str(" AND id = ?");
        args.push(Box::new(id));
    }
    if let Some(ref uid) = find.uid {
        sql.push_str(" AND uid = ?");
        args.push(Box::new(uid.clone()));
    }
    if let Some(memo_id) = find.memo_id {
        sql.push_str(" AND memo_id = ?");
        args.push(Box::new(memo_id));
    }
    if !find.memo_id_list.is_empty() {
        let placeholders: Vec<&str> = find.memo_id_list.iter().map(|_| "?").collect();
        sql.push_str(&format!(" AND memo_id IN ({})", placeholders.join(",")));
        for mid in &find.memo_id_list {
            args.push(Box::new(*mid));
        }
    }
    if find.memo_id_is_null {
        sql.push_str(" AND memo_id IS NULL");
    }
    sql.push_str(" ORDER BY created_ts DESC");

    if let Some(limit) = find.limit {
        sql.push_str(" LIMIT ?");
        args.push(Box::new(limit));
    }
    if let Some(offset) = find.offset {
        sql.push_str(" OFFSET ?");
        args.push(Box::new(offset));
    }

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        args.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice(),
        |row| {
            let payload_str: String = row.get(10)?;
            Ok(Attachment {
                id: row.get(0)?,
                uid: row.get(1)?,
                created_ts: row.get(2)?,
                updated_ts: row.get(3)?,
                filename: row.get(4)?,
                blob: row.get(5)?,
                r#type: row.get(6)?,
                size: row.get(7)?,
                memo_id: row.get(8)?,
                storage_type: row.get(9)?,
                reference: row.get(10)?,
                payload: serde_json::from_str(&payload_str).unwrap_or(Value::Object(Default::default())),
            })
        },
    )?;
    let mut result = Vec::new();
    for r in rows {
        result.push(r?);
    }
    Ok(result)
}

/// 更新
pub fn update(conn: &Connection, update: &UpdateAttachment) -> CoreResult<Attachment> {
    let mut sets: Vec<&str> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref filename) = update.filename {
        sets.push("filename = ?");
        args.push(Box::new(filename.clone()));
    }
    if let Some(ref memo_id) = update.memo_id {
        sets.push("memo_id = ?");
        args.push(Box::new(*memo_id));
    }
    if let Some(ref payload) = update.payload {
        sets.push("payload = ?");
        args.push(Box::new(serde_json::to_string(payload)?));
    }
    sets.push("updated_ts = ?");
    args.push(Box::new(chrono::Utc::now().timestamp()));

    args.push(Box::new(update.id));
    let sql = format!("UPDATE attachment SET {} WHERE id = ?", sets.join(", "));
    let affected = conn.execute(&sql, args.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice())?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("attachment id={}", update.id)));
    }
    get(conn, &FindAttachment { id: Some(update.id), get_blob: false, ..Default::default() })?
        .ok_or_else(|| CoreError::NotFound(format!("attachment id={}", update.id)))
}

/// 删除（返回被删附件的 storage_type 与 reference，便于上层清理本地文件）
pub fn delete(conn: &Connection, id: i32) -> CoreResult<Option<(String, String)>> {
    // 先查出 storage_type 与 reference（用于上层清理本地文件）
    let stored: Option<(String, String)> = conn
        .query_row(
            "SELECT storage_type, reference FROM attachment WHERE id = ?",
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();
    let affected = conn.execute("DELETE FROM attachment WHERE id = ?", params![id])?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("attachment id={id}")));
    }
    Ok(stored)
}
