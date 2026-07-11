//! Memo 实体与 CRUD

use crate::error::{CoreError, CoreResult};
use crate::markdown;
use crate::types::{validate_uid, RowStatus, Visibility};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 位置信息（JSON 序列化存储）
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct MemoLocation {
    pub placeholder: String,
    pub latitude: f64,
    pub longitude: f64,
}

/// 笔记实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memo {
    pub id: i32,
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub row_status: RowStatus,
    pub content: String,
    pub visibility: Visibility,
    pub pinned: bool,
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<MemoLocation>,
}

/// 创建笔记
#[derive(Debug, Clone)]
pub struct CreateMemo {
    pub uid: String,
    pub content: String,
    pub visibility: Visibility,
    pub pinned: bool,
    pub payload: Value,
    pub location: Option<MemoLocation>,
}

/// 更新笔记（所有字段可选）
#[derive(Debug, Clone, Default)]
pub struct UpdateMemo {
    pub id: i32,
    pub uid: Option<String>,
    pub row_status: Option<RowStatus>,
    pub content: Option<String>,
    pub visibility: Option<Visibility>,
    pub pinned: Option<bool>,
    pub payload: Option<Value>,
    /// None = 不更新；Some(None) = 清除 location；Some(Some(loc)) = 设置 location
    pub location: Option<Option<MemoLocation>>,
}

/// 查询过滤
#[derive(Debug, Clone, Default)]
pub struct FindMemo {
    pub id: Option<i32>,
    pub uid: Option<String>,
    pub id_list: Vec<i32>,
    pub uid_list: Vec<String>,
    pub row_status: Option<RowStatus>,
    pub visibility_list: Vec<Visibility>,
    pub exclude_content: bool,
    /// 内容模糊匹配（SQL LIKE %...%）
    pub content_contains: Option<String>,
    /// FTS5 全文搜索查询（MATCH 语法）；与 content_contains 互斥，优先使用
    pub fts_query: Option<String>,
    /// 向量搜索的 embedding（JSON 字符串，384维）
    pub vector_embedding: Option<String>,
    /// 向量搜索返回数量（默认 20）
    pub vector_top_k: Option<u32>,
    /// tag 精确搜索（在 Rust 层用 markdown 提取验证）
    pub tag_search: Vec<String>,
    /// 创建时间过滤（>=）
    pub created_ts_after: Option<i64>,
    /// 创建时间过滤（<）
    pub created_ts_before: Option<i64>,
    /// 更新时间过滤（>=）
    pub updated_ts_after: Option<i64>,
    /// 更新时间过滤（<）
    pub updated_ts_before: Option<i64>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub order_by_pinned: bool,
    pub order_by_updated_ts: bool,
    pub order_by_time_asc: bool,
}

/// 创建
pub fn create(conn: &Connection, create: &CreateMemo) -> CoreResult<Memo> {
    if !validate_uid(&create.uid) {
        return Err(CoreError::InvalidUid);
    }
    let payload_str = serde_json::to_string(&create.payload)?;
    let pinned_int = if create.pinned { 1 } else { 0 };
    let location_str = match &create.location {
        Some(loc) => Some(serde_json::to_string(loc)?),
        None => None,
    };
    conn.execute(
        "INSERT INTO memo (uid, content, visibility, pinned, payload, location) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            &create.uid,
            &create.content,
            create.visibility,
            pinned_int,
            &payload_str,
            &location_str,
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
    get(conn, &FindMemo { id: Some(id), ..Default::default() })?
        .ok_or_else(|| CoreError::NotFound(format!("memo id={id}")))
}

/// 查询单条
pub fn get(conn: &Connection, find: &FindMemo) -> CoreResult<Option<Memo>> {
    let list = list(conn, find)?;
    Ok(list.into_iter().next())
}

/// 查询列表
pub fn list(conn: &Connection, find: &FindMemo) -> CoreResult<Vec<Memo>> {
    // tag_search 需要在 Rust 层用 markdown 提取验证，因此强制读取 content
    let need_content_for_tags = !find.tag_search.is_empty();
    let exclude_content = find.exclude_content && !need_content_for_tags;

    // 向量搜索预处理：先执行 KNN 查询获取按距离排序的 id 列表
    // sqlite-vec 的 vec0 KNN 查询必须有 LIMIT，且 distance 列只在 KNN 上下文中可用
    let knn_order: Option<Vec<i32>> = if let Some(ref embedding_json) = find.vector_embedding {
        let top_k = find.vector_top_k.unwrap_or(20) as i32;
        let mut stmt = conn.prepare(
            "SELECT rowid FROM memo_vec WHERE embedding MATCH ? ORDER BY distance LIMIT ?",
        )?;
        let rows = stmt.query_map(params![embedding_json, top_k], |r| r.get::<_, i32>(0))?;
        let ids: Vec<i32> = rows.filter_map(|r| r.ok()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Some(ids)
    } else {
        None
    };

    let mut sql = String::from("SELECT id, uid, created_ts, updated_ts, row_status, ");
    if exclude_content {
        sql.push_str("'' AS content, ");
    } else {
        sql.push_str("content, ");
    }
    sql.push_str("visibility, pinned, payload, location FROM memo WHERE 1=1");
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(id) = find.id {
        sql.push_str(" AND id = ?");
        args.push(Box::new(id));
    }
    if let Some(ref uid) = find.uid {
        sql.push_str(" AND uid = ?");
        args.push(Box::new(uid.clone()));
    }
    if !find.id_list.is_empty() {
        let placeholders: Vec<&str> = find.id_list.iter().map(|_| "?").collect();
        sql.push_str(&format!(" AND id IN ({})", placeholders.join(",")));
        for id in &find.id_list {
            args.push(Box::new(*id));
        }
    }
    if !find.uid_list.is_empty() {
        let placeholders: Vec<&str> = find.uid_list.iter().map(|_| "?").collect();
        sql.push_str(&format!(" AND uid IN ({})", placeholders.join(",")));
        for uid in &find.uid_list {
            args.push(Box::new(uid.clone()));
        }
    }
    if let Some(rs) = find.row_status {
        sql.push_str(" AND row_status = ?");
        args.push(Box::new(rs));
    }
    if !find.visibility_list.is_empty() {
        let placeholders: Vec<&str> = find.visibility_list.iter().map(|_| "?").collect();
        sql.push_str(&format!(" AND visibility IN ({})", placeholders.join(",")));
        for v in &find.visibility_list {
            args.push(Box::new(*v));
        }
    }
    if let Some(ref needle) = find.content_contains {
        // 转义 LIKE 通配符：% _ \
        let escaped = needle
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        sql.push_str(" AND content LIKE '%' || ? || '%' ESCAPE '\\'");
        args.push(Box::new(escaped));
    }
    // FTS5 全文搜索（与 content_contains 互斥，优先使用）
    if let Some(ref fts_q) = find.fts_query {
        sql.push_str(" AND id IN (SELECT rowid FROM memo_fts WHERE memo_fts MATCH ?)");
        args.push(Box::new(fts_q.clone()));
    }
    // 向量语义搜索：用 KNN 预查询的 id 列表过滤（distance 列在非 KNN 上下文不可用）
    if let Some(ref ids) = knn_order {
        let placeholders: Vec<&str> = ids.iter().map(|_| "?").collect();
        sql.push_str(&format!(" AND id IN ({})", placeholders.join(",")));
        for id in ids {
            args.push(Box::new(*id));
        }
    }
    if let Some(ts) = find.created_ts_after {
        sql.push_str(" AND created_ts >= ?");
        args.push(Box::new(ts));
    }
    if let Some(ts) = find.created_ts_before {
        sql.push_str(" AND created_ts < ?");
        args.push(Box::new(ts));
    }
    if let Some(ts) = find.updated_ts_after {
        sql.push_str(" AND updated_ts >= ?");
        args.push(Box::new(ts));
    }
    if let Some(ts) = find.updated_ts_before {
        sql.push_str(" AND updated_ts < ?");
        args.push(Box::new(ts));
    }

    // 排序：FTS 按相关度排序；向量搜索在 Rust 层按 KNN 顺序排序；否则按默认时间排序
    if find.fts_query.is_some() {
        // FTS5 rank 越小越相关
        sql.push_str(" ORDER BY (SELECT rank FROM memo_fts WHERE rowid = memo.id)");
    } else if knn_order.is_some() {
        // 向量搜索：不指定 SQL ORDER BY，后续在 Rust 层按 KNN 距离顺序排序
    } else {
        let mut order_parts: Vec<&str> = Vec::new();
        if find.order_by_pinned {
            order_parts.push("pinned DESC");
        }
        if find.order_by_updated_ts {
            order_parts.push(if find.order_by_time_asc {
                "updated_ts ASC"
            } else {
                "updated_ts DESC"
            });
        } else {
            order_parts.push(if find.order_by_time_asc {
                "created_ts ASC"
            } else {
                "created_ts DESC"
            });
        }
        sql.push_str(&format!(" ORDER BY {}", order_parts.join(", ")));
    }

    // 向量搜索时，默认 top_k 作为 limit
    let effective_limit = if knn_order.is_some() && find.limit.is_none() {
        Some(find.vector_top_k.unwrap_or(20) as i32)
    } else {
        find.limit
    };

    // 当需要 Rust 层 tag 过滤时，不在 SQL 层应用 limit/offset
    let apply_sql_paging = find.tag_search.is_empty();
    if apply_sql_paging {
        if let Some(limit) = effective_limit {
            sql.push_str(" LIMIT ?");
            args.push(Box::new(limit));
        }
        if let Some(offset) = find.offset {
            sql.push_str(" OFFSET ?");
            args.push(Box::new(offset));
        }
    }

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        args.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice(),
        |row| {
            let pinned_int: i32 = row.get(7)?;
            let payload_str: String = row.get(8)?;
            let location_str: Option<String> = row.get(9)?;
            let location = location_str
                .as_deref()
                .and_then(|s| serde_json::from_str::<MemoLocation>(s).ok());
            Ok(Memo {
                id: row.get(0)?,
                uid: row.get(1)?,
                created_ts: row.get(2)?,
                updated_ts: row.get(3)?,
                row_status: row.get(4)?,
                content: row.get(5)?,
                visibility: row.get(6)?,
                pinned: pinned_int != 0,
                payload: serde_json::from_str(&payload_str).unwrap_or(Value::Object(Default::default())),
                location,
            })
        },
    )?;

    let mut result: Vec<Memo> = Vec::new();
    for r in rows {
        let memo = r?;
        // Rust 层 tag 精确过滤
        if !find.tag_search.is_empty() {
            let tags = markdown::extract_tags(&memo.content);
            if !find.tag_search.iter().all(|t| tags.iter().any(|gt| gt == t)) {
                continue;
            }
        }
        // 如果是因为 tag 过滤而强制读取 content 但用户要求 exclude_content，则清空 content
        if need_content_for_tags && find.exclude_content {
            let mut memo = memo;
            memo.content.clear();
            result.push(memo);
        } else {
            result.push(memo);
        }
    }

    // 向量搜索：按 KNN 距离顺序排序（距离越小越相似）
    if let Some(ref order) = knn_order {
        result.sort_by_key(|m| {
            order.iter().position(|&id| id == m.id).unwrap_or(usize::MAX)
        });
    }

    // 应用 Rust 层分页（仅在 tag 过滤时）
    if !apply_sql_paging {
        let start = find.offset.unwrap_or(0).max(0) as usize;
        if start >= result.len() {
            result.clear();
        } else {
            result.drain(0..start);
            if let Some(limit) = find.limit {
                if (limit as usize) < result.len() {
                    result.truncate(limit as usize);
                }
            }
        }
    }

    Ok(result)
}

/// 更新
pub fn update(conn: &Connection, update: &UpdateMemo) -> CoreResult<Memo> {
    let mut sets: Vec<&str> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref uid) = update.uid {
        if !validate_uid(uid) {
            return Err(CoreError::InvalidUid);
        }
        sets.push("uid = ?");
        args.push(Box::new(uid.clone()));
    }
    if let Some(rs) = update.row_status {
        sets.push("row_status = ?");
        args.push(Box::new(rs));
    }
    if let Some(ref content) = update.content {
        sets.push("content = ?");
        args.push(Box::new(content.clone()));
    }
    if let Some(v) = update.visibility {
        sets.push("visibility = ?");
        args.push(Box::new(v));
    }
    if let Some(pinned) = update.pinned {
        sets.push("pinned = ?");
        args.push(Box::new(if pinned { 1 } else { 0 }));
    }
    if let Some(ref payload) = update.payload {
        sets.push("payload = ?");
        args.push(Box::new(serde_json::to_string(payload)?));
    }
    if let Some(loc_opt) = &update.location {
        match loc_opt {
            Some(loc) => {
                sets.push("location = ?");
                args.push(Box::new(serde_json::to_string(loc)?));
            }
            None => {
                sets.push("location = NULL");
            }
        }
    }
    sets.push("updated_ts = ?");
    args.push(Box::new(chrono::Utc::now().timestamp()));

    args.push(Box::new(update.id));
    let sql = format!("UPDATE memo SET {} WHERE id = ?", sets.join(", "));
    let affected = conn.execute(&sql, args.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice())?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("memo id={}", update.id)));
    }
    get(conn, &FindMemo { id: Some(update.id), ..Default::default() })?
        .ok_or_else(|| CoreError::NotFound(format!("memo id={}", update.id)))
}

/// 删除（含级联清理 memo_relation、attachment 关联、向量索引）
pub fn delete(conn: &mut Connection, id: i32) -> CoreResult<()> {
    let tx = conn.transaction()?;
    // 查询 memo uid，用于标记回顾卡片
    let uid: Option<String> = tx
        .query_row("SELECT uid FROM memo WHERE id = ?", params![id], |r| r.get(0))
        .ok();
    // 标记关联的回顾卡片为 memo_deleted
    if let Some(ref uid) = uid {
        let _ = tx.execute(
            "UPDATE review_card SET memo_deleted = 1 WHERE memo_uid = ?1",
            params![uid],
        );
    }
    // 级联清理关系
    tx.execute(
        "DELETE FROM memo_relation WHERE memo_id = ?1 OR related_memo_id = ?1",
        params![id],
    )?;
    // 解绑附件的 memo_id（不删附件本身，保留用户资源）
    tx.execute("UPDATE attachment SET memo_id = NULL WHERE memo_id = ?", params![id])?;
    // 删除向量索引（FTS 由 memo_ad 触发器自动清理）
    let _ = tx.execute("DELETE FROM memo_vec WHERE rowid = ?", params![id]);
    let affected = tx.execute("DELETE FROM memo WHERE id = ?", params![id])?;
    tx.commit()?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("memo id={id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    fn make_memo(uid: &str, content: &str) -> CreateMemo {
        CreateMemo {
            uid: uid.to_string(),
            content: content.to_string(),
            visibility: Visibility::Private,
            pinned: false,
            payload: serde_json::Value::Object(Default::default()),
            location: None,
        }
    }

    #[test]
    fn test_fts_search() {
        let store = Store::open_in_memory().unwrap();

        // 插入测试数据
        let _m1 = store.with_conn(|c| create(c, &make_memo("test1", "Hello world from Rust"))).unwrap();
        let _m2 = store.with_conn(|c| create(c, &make_memo("test2", "Goodbye world"))).unwrap();
        let _m3 = store.with_conn(|c| create(c, &make_memo("test3", "Random text here"))).unwrap();

        // 验证 FTS 表是否有数据
        let fts_count: i64 = store
            .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM memo_fts", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(fts_count, 3, "FTS 表应有 3 条记录");

        // 测试单词搜索（phrase query，带双引号）
        let results = store
            .with_conn(|c| list(c, &FindMemo {
                fts_query: Some("\"world\"".to_string()),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(results.len(), 2, "搜索 'world' 应返回 2 条结果");
        eprintln!("phrase query '\"world\"' 返回 {} 条", results.len());

        // 测试多词 AND 搜索（前端 connect.ts 生成的格式：每个词用 phrase 包裹，空格连接）
        // "hello" "world" 应匹配同时包含 hello 和 world 的文档（不要求连续）
        let results = store
            .with_conn(|c| list(c, &FindMemo {
                fts_query: Some("\"hello\" \"world\"".to_string()),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(results.len(), 1, "AND 搜索 'hello world' 应返回 1 条（只有 m1 同时包含两者）");
        eprintln!("AND query '\"hello\" \"world\"' 返回 {} 条", results.len());

        // 测试中文搜索（>= 3 字符，trigram 可匹配）
        let _m4 = store.with_conn(|c| create(c, &make_memo("test4", "你好世界"))).unwrap();
        let results = store
            .with_conn(|c| list(c, &FindMemo {
                fts_query: Some("\"你好世界\"".to_string()),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(results.len(), 1, "搜索 '你好世界' 应返回 1 条结果");
        eprintln!("中文查询 '你好世界' 返回 {} 条", results.len());

        // 测试中文短查询 fallback（< 3 字符用 LIKE）
        let results = store
            .with_conn(|c| list(c, &FindMemo {
                content_contains: Some("你好".to_string()),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(results.len(), 1, "LIKE 搜索 '你好' 应返回 1 条结果");
        eprintln!("LIKE fallback '你好' 返回 {} 条", results.len());

        eprintln!("FTS5 搜索测试全部通过！");
    }

    /// 辅助：生成 384 维假向量（全 0，只在指定维度设 1.0），用于测试 KNN 查询
    fn fake_embedding(dim: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; 384];
        if dim < 384 {
            v[dim] = 1.0;
        }
        v
    }

    #[test]
    fn test_vector_search() {
        let store = Store::open_in_memory().unwrap();

        // 插入 3 条 memo
        let m1 = store.with_conn(|c| create(c, &make_memo("vtest1", "Rust programming"))).unwrap();
        let m2 = store.with_conn(|c| create(c, &make_memo("vtest2", "Python scripting"))).unwrap();
        let m3 = store.with_conn(|c| create(c, &make_memo("vtest3", "Go concurrency"))).unwrap();

        // 手动插入假 embedding 到 memo_vec（dim 0/1/2 分别对应三个 memo）
        store
            .with_conn(|c| {
                let emb1 = serde_json::to_string(&fake_embedding(0)).unwrap();
                let emb2 = serde_json::to_string(&fake_embedding(1)).unwrap();
                let emb3 = serde_json::to_string(&fake_embedding(2)).unwrap();
                c.execute("INSERT INTO memo_vec(rowid, embedding) VALUES (?1, ?2)", params![m1.id, &emb1])?;
                c.execute("INSERT INTO memo_vec(rowid, embedding) VALUES (?1, ?2)", params![m2.id, &emb2])?;
                c.execute("INSERT INTO memo_vec(rowid, embedding) VALUES (?1, ?2)", params![m3.id, &emb3])?;
                Ok(())
            })
            .unwrap();

        // 验证 memo_vec 有数据
        let vec_count: i64 = store
            .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM memo_vec", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(vec_count, 3, "memo_vec 应有 3 条记录");

        // 通过 list() 函数进行向量搜索（修复后：两步查询 + Rust 层排序）
        let query_emb = serde_json::to_string(&fake_embedding(0)).unwrap();
        let results = store
            .with_conn(|c| list(c, &FindMemo {
                vector_embedding: Some(query_emb),
                vector_top_k: Some(3),
                limit: Some(3),
                ..Default::default()
            }))
            .expect("向量搜索应成功");
        assert_eq!(results.len(), 3, "向量搜索应返回 3 条结果");
        // 查询向量与 m1 的 embedding 相同（dim 0），m1 应排在第一位
        assert_eq!(results[0].id, m1.id, "最相似的结果应是 m1");

        eprintln!("向量搜索测试通过！");
    }

    #[test]
    fn test_fts_trigger_update() {
        let store = Store::open_in_memory().unwrap();
        // 创建一条 memo
        let m = store.with_conn(|c| create(c, &make_memo("trig1", "Original content"))).unwrap();

        // 直接执行 UPDATE（触发 memo_au 触发器）
        store.with_conn(|c| {
            let updated = update(c, &UpdateMemo {
                id: m.id,
                content: Some("Updated content".into()),
                ..Default::default()
            })?;
            Ok(updated)
        }).expect("UPDATE 应成功，FTS5 触发器应正常工作");

        // 验证 FTS 表已更新
        let fts_content: String = store
            .with_conn(|c| Ok(c.query_row("SELECT content FROM memo_fts WHERE rowid = ?", params![m.id], |r| r.get(0))?))
            .unwrap();
        assert_eq!(fts_content, "Updated content", "FTS 表应反映更新后的内容");

        // 验证搜索能找到更新后的内容
        let results = store
            .with_conn(|c| list(c, &FindMemo {
                fts_query: Some("\"Updated\"".to_string()),
                ..Default::default()
            }))
            .unwrap();
        assert_eq!(results.len(), 1, "搜索 'Updated' 应返回 1 条");
        assert_eq!(results[0].id, m.id);

        // 测试 DELETE（触发 memo_ad 触发器）
        store.with_conn_mut(|c| delete(c, m.id)).expect("DELETE 应成功");
        let fts_count: i64 = store
            .with_conn(|c| Ok(c.query_row("SELECT COUNT(*) FROM memo_fts", [], |r| r.get(0))?))
            .unwrap();
        assert_eq!(fts_count, 0, "DELETE 后 FTS 表应为空");
    }
}
