# Tag Metadata Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将标签元数据（名称 + 使用次数）存入数据库 `tag` 表，消除 `list_tags` 的全表扫描开销，memo CRUD 时实时同步 tag count。

**Architecture:** 新增 V6 迁移建 `tag` 表 + `core/src/tag.rs` 模块提供同步/查询函数。在 `memo::create`/`update`/`delete` 中同事务调用 tag 同步函数。命令层 `list_tags`、`suggest_tags`、AI `execute_list_tags` 改为读 tag 表。

**Tech Stack:** Rust, rusqlite, refinery migrations, chrono

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `core/migrations/V6__add_tag_metadata.sql` | Create | 建 tag 表 + 索引 |
| `core/src/tag.rs` | Create | tag 表 CRUD：upsert/decrement/sync/list |
| `core/src/lib.rs` | Modify | 注册 `pub mod tag;` |
| `core/src/memo.rs` | Modify | create/update/delete 内同步 tag 表 |
| `src-tauri/src/commands/memo.rs` | Modify | list_tags + suggest_tags 改读 tag 表 |
| `src-tauri/src/ai/tools.rs` | Modify | execute_list_tags 改读 tag 表 |
| `core/tests/crud.rs` | Modify | 新增 tag 同步测试 |

---

### Task 1: V6 迁移文件 + tag 模块骨架

**Files:**
- Create: `core/migrations/V6__add_tag_metadata.sql`
- Create: `core/src/tag.rs`
- Modify: `core/src/lib.rs:6` (after `pub mod store;` line, add `pub mod tag;`)

- [ ] **Step 1: 创建 V6 迁移文件**

```sql
-- core/migrations/V6__add_tag_metadata.sql
-- 标签元数据表：存储标签名称、使用次数、时间戳
-- content 中的 #tag 仍是单一真相源，此表作为索引/缓存
CREATE TABLE IF NOT EXISTS tag (
    name TEXT PRIMARY KEY,
    count INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_tag_count ON tag(count DESC);
```

- [ ] **Step 2: 创建 `core/src/tag.rs`（全部函数）**

```rust
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
```

- [ ] **Step 3: 注册模块**

在 `core/src/lib.rs` 的模块列表中（`pub mod store;` 之后）添加：

```rust
pub mod tag;
```

- [ ] **Step 4: 验证编译**

Run: `cargo build -p memos-core`
Expected: 编译通过，无错误

- [ ] **Step 5: Commit**

```bash
git add core/migrations/V6__add_tag_metadata.sql core/src/tag.rs core/src/lib.rs
git commit -m "feat(core): add tag metadata table V6 migration and tag module"
```

---

### Task 2: 在 memo::create 中同步 tag 表

**Files:**
- Modify: `core/src/memo.rs:95-128` (create 函数)
- Test: `core/tests/crud.rs` (新增测试)

- [ ] **Step 1: 写失败测试**

在 `core/tests/crud.rs` 末尾添加：

```rust
#[test]
fn tag_table_syncs_on_create() {
    let store = open_test_store();
    let conn = store.lock_conn();

    memo::create(&conn, &CreateMemo {
        uid: "test-tag-create".to_string(),
        content: "hello #rust #ai".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let names: Vec<&str> = tags.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"rust"), "tag 表应包含 rust");
    assert!(names.contains(&"ai"), "tag 表应包含 ai");

    let rust_count = tags.iter().find(|(n, _)| n == "rust").map(|(_, c)| *c).unwrap();
    assert_eq!(rust_count, 1, "rust count 应为 1");
}

#[test]
fn tag_table_empty_when_no_tags() {
    let store = open_test_store();
    let conn = store.lock_conn();

    memo::create(&conn, &CreateMemo {
        uid: "test-no-tags".to_string(),
        content: "just plain text no tags".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    assert!(tags.is_empty(), "无 tag 的 memo 不应产生 tag 表记录");
}
```

在 `core/tests/crud.rs` 顶部 `use` 区域确认有 `use memos_core::tag;`（如果没有则添加）。

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p memos-core --test crud tag_table_syncs_on_create`
Expected: FAIL（create 后 tag 表为空，因为还没同步逻辑）

- [ ] **Step 3: 修改 `memo::create` 同步 tag**

在 `core/src/memo.rs` 的 `create` 函数中，在 `get(conn, ...)` 返回前插入 tag 同步调用。在文件顶部添加 `use crate::tag;` 到 import 区。

修改 `create` 函数（第 124-128 行附近），在 `let id = conn.last_insert_rowid() as i32;` 之后、`get(...)` 之前添加：

```rust
    let id = conn.last_insert_rowid() as i32;
    tag::upsert_tags_for_content(conn, &create.content)?;
    get(conn, &FindMemo { id: Some(id), ..Default::default() })?
        .ok_or_else(|| CoreError::NotFound(format!("memo id={id}")))
```

同时在 `core/src/memo.rs` 顶部 import 区添加：

```rust
use crate::tag;
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p memos-core --test crud tag_table`
Expected: 2 个测试 PASS

- [ ] **Step 5: Commit**

```bash
git add core/src/memo.rs core/tests/crud.rs
git commit -m "feat(core): sync tag table on memo create"
```

---

### Task 3: 在 memo::update 中同步 tag 表

**Files:**
- Modify: `core/src/memo.rs:358-411` (update 函数)
- Test: `core/tests/crud.rs`

- [ ] **Step 1: 写失败测试**

在 `core/tests/crud.rs` 末尾添加：

```rust
#[test]
fn tag_table_syncs_on_update_content() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test-tag-update".to_string(),
        content: "hello #rust".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    // 更新：添加 #go，保留 #rust
    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: None,
        content: Some("hello #rust #go".to_string()),
        visibility: None,
        pinned: None,
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let rust_count = tags.iter().find(|(n, _)| n == "rust").map(|(_, c)| *c).unwrap_or(0);
    let go_count = tags.iter().find(|(n, _)| n == "go").map(|(_, c)| *c).unwrap_or(0);
    assert_eq!(rust_count, 1, "rust count 应保持 1");
    assert_eq!(go_count, 1, "go count 应为 1");
}

#[test]
fn tag_table_removes_tag_when_removed_from_content() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test-tag-remove".to_string(),
        content: "hello #rust #ai".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    // 更新：移除 #ai
    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: None,
        content: Some("hello #rust".to_string()),
        visibility: None,
        pinned: None,
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let names: Vec<&str> = tags.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"rust"), "rust 应仍在 tag 表");
    assert!(!names.contains(&"ai"), "ai count 归 0 应被删除");
}

#[test]
fn tag_table_no_sync_when_content_unchanged() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test-tag-nosync".to_string(),
        content: "hello #rust".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    // 只更新 pinned，不改 content
    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: None,
        content: None,
        visibility: None,
        pinned: Some(true),
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let rust_count = tags.iter().find(|(n, _)| n == "rust").map(|(_, c)| *c).unwrap_or(0);
    assert_eq!(rust_count, 1, "content 未变时 tag count 不应改变");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p memos-core --test crud tag_table_syncs_on_update_content`
Expected: FAIL（update 后 go count 仍为 0，因为还没同步逻辑）

- [ ] **Step 3: 修改 `memo::update` 同步 tag**

在 `core/src/memo.rs` 的 `update` 函数中，需要：
1. 在更新前读取旧 content（如果 content 字段在 update mask 中）
2. 执行 SQL update
3. 调用 `tag::sync_tags_on_update(conn, &old_content, &new_content)?`

将 `update` 函数（第 358-411 行）替换为：

```rust
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

    // 若 content 变化，先读取旧 content 用于 tag 同步
    let old_content: Option<String> = if update.content.is_some() {
        conn.query_row(
            "SELECT content FROM memo WHERE id = ?",
            params![update.id],
            |r| r.get(0),
        )
        .ok()
    } else {
        None
    };

    // 若 row_status 变化，先读取旧 row_status 和 content 用于 tag 同步
    let old_row_status: Option<RowStatus> = if update.row_status.is_some() {
        conn.query_row(
            "SELECT row_status FROM memo WHERE id = ?",
            params![update.id],
            |r| r.get(0),
        )
        .ok()
    } else {
        None
    };
    let content_for_row_status: Option<String> = if old_row_status.is_some() && update.content.is_none() {
        // row_status 变了但 content 没变，需要读 content 用于 tag 同步
        conn.query_row(
            "SELECT content FROM memo WHERE id = ?",
            params![update.id],
            |r| r.get(0),
        )
        .ok()
    } else {
        None
    };

    let sql = format!("UPDATE memo SET {} WHERE id = ?", sets.join(", "));
    let affected = conn.execute(&sql, args.iter().map(|b| b.as_ref()).collect::<Vec<_>>().as_slice())?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("memo id={}", update.id)));
    }

    // 同步 tag 表
    if let Some(ref old_content) = old_content {
        let new_content = update.content.as_deref().unwrap_or(old_content);
        tag::sync_tags_on_update(conn, old_content, new_content)?;
    }

    // row_status 变化时同步 tag count
    if let (Some(old_rs), Some(new_rs)) = (old_row_status, update.row_status) {
        let content_ref = update.content.as_deref().or(content_for_row_status.as_deref()).unwrap_or("");
        let was_normal = old_rs == RowStatus::Normal;
        let is_normal = new_rs == RowStatus::Normal;
        if !was_normal && is_normal {
            // ARCHIVED → NORMAL：tag count +1
            tag::upsert_tags_for_content(conn, content_ref)?;
        } else if was_normal && !is_normal {
            // NORMAL → ARCHIVED：tag count -1
            tag::decrement_tags_for_content(conn, content_ref)?;
        }
    }

    get(conn, &FindMemo { id: Some(update.id), ..Default::default() })?
        .ok_or_else(|| CoreError::NotFound(format!("memo id={}", update.id)))
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p memos-core --test crud tag_table`
Expected: 全部 tag_table 测试 PASS

- [ ] **Step 5: Commit**

```bash
git add core/src/memo.rs core/tests/crud.rs
git commit -m "feat(core): sync tag table on memo update (content + row_status)"
```

---

### Task 4: 在 memo::delete 中同步 tag 表

**Files:**
- Modify: `core/src/memo.rs:414-442` (delete 函数)
- Test: `core/tests/crud.rs`

- [ ] **Step 1: 写失败测试**

在 `core/tests/crud.rs` 末尾添加：

```rust
#[test]
fn tag_table_decrements_on_delete() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test-tag-delete".to_string(),
        content: "hello #rust #ai".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    // delete 需要 &mut Connection
    drop(conn);
    let mut conn_mut = store.conn.lock().unwrap();
    memo::delete(&mut conn_mut, created.id).unwrap();
    drop(conn_mut);

    let conn = store.lock_conn();
    let tags = tag::list_tags(&conn).unwrap();
    assert!(tags.is_empty(), "删除 memo 后 tag 表应被清空");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p memos-core --test crud tag_table_decrements_on_delete`
Expected: FAIL（删除后 tag 表仍有记录）

注意：`store.conn` 是私有字段。如果测试无法直接访问，改用 `store.with_conn_mut`：

```rust
#[test]
fn tag_table_decrements_on_delete() {
    let store = open_test_store();
    {
        let conn = store.lock_conn();
        let created = memo::create(&conn, &CreateMemo {
            uid: "test-tag-delete".to_string(),
            content: "hello #rust #ai".to_string(),
            visibility: Visibility::Private,
            pinned: false,
            payload: json!({}),
            location: None,
        }).unwrap();
        drop(conn);
        store.with_conn_mut(|c| memo::delete(c, created.id)).unwrap();
    }
    let tags = store.with_conn(|c| tag::list_tags(c)).unwrap();
    assert!(tags.is_empty(), "删除 memo 后 tag 表应被清空");
}
```

- [ ] **Step 3: 修改 `memo::delete` 同步 tag**

在 `core/src/memo.rs` 的 `delete` 函数中，在 `DELETE FROM memo` 之前读取 content 并递减 tag。

修改 `delete` 函数（第 414-442 行）。在 `let uid: Option<String> = ...` 之后、`DELETE FROM memo` 之前添加读取 content + decrement：

```rust
/// 删除（含级联清理 memo_relation、attachment 关联、向量索引）
pub fn delete(conn: &mut Connection, id: i32) -> CoreResult<()> {
    let tx = conn.transaction()?;
    // 查询 memo uid，用于标记回顾卡片
    let uid: Option<String> = tx
        .query_row("SELECT uid FROM memo WHERE id = ?", params![id], |r| r.get(0))
        .ok();
    // 查询 content，用于递减 tag 计数
    let content: Option<String> = tx
        .query_row("SELECT content FROM memo WHERE id = ?", params![id], |r| r.get(0))
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
    // 递减 tag 计数
    if let Some(ref content) = content {
        tag::decrement_tags_for_content(&tx, content)?;
    }
    tx.commit()?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("memo id={id}")));
    }
    Ok(())
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p memos-core --test crud tag_table_decrements_on_delete`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add core/src/memo.rs core/tests/crud.rs
git commit -m "feat(core): sync tag table on memo delete"
```

---

### Task 5: 命令层 list_tags 改读 tag 表

**Files:**
- Modify: `src-tauri/src/commands/memo.rs:246-270` (list_tags 函数)
- Modify: `src-tauri/src/commands/memo.rs:314-415` (suggest_tags 函数中 system_tags 查询)
- Modify: `src-tauri/src/ai/tools.rs:188-210` (execute_list_tags 函数)

- [ ] **Step 1: 修改 `commands::memo::list_tags`**

将 `src-tauri/src/commands/memo.rs` 第 246-270 行的 `list_tags` 函数替换为：

```rust
#[tauri::command]
pub fn list_tags(state: tauri::State<'_, AppState>) -> IpcResult<Vec<TagWithCount>> {
    let store = state.store();
    let tags = store.with_conn(|c| memos_core::tag::list_tags(c))?;
    Ok(tags
        .into_iter()
        .map(|(tag, count)| TagWithCount { tag, count })
        .collect())
}
```

在 `src-tauri/src/commands/memo.rs` 顶部确认有 `use memos_core::tag;`（若没有可使用全路径 `memos_core::tag::list_tags`）。

- [ ] **Step 2: 修改 `commands::memo::suggest_tags` 中的 system_tags 查询**

在 `suggest_tags` 函数中（第 331-349 行附近），将全表扫描提取 system_tags 的代码块替换为：

```rust
    // 查询系统已有标签，提供给 AI 优先复用
    let system_tags: Vec<String> = store.with_conn(|c| -> memos_core::CoreResult<Vec<String>> {
        Ok(memos_core::tag::list_tags(c)?
            .into_iter()
            .map(|(name, _)| name)
            .collect())
    })?;
```

- [ ] **Step 3: 修改 `ai::tools::execute_list_tags`**

将 `src-tauri/src/ai/tools.rs` 第 188-210 行的 `execute_list_tags` 函数替换为：

```rust
fn execute_list_tags(store: &Store) -> memos_core::CoreResult<Value> {
    let tags = store.with_conn(|c| memos_core::tag::list_tags(c))?;
    let tags: Vec<Value> = tags
        .into_iter()
        .map(|(tag, count)| json!({ "tag": tag, "count": count }))
        .collect();
    Ok(json!({ "tags": tags }))
}
```

- [ ] **Step 4: 验证编译**

Run: `cargo check -p memos-app`
Expected: 编译通过

- [ ] **Step 5: 运行全部测试**

Run: `cargo test -p memos-core --test crud`
Expected: 所有测试 PASS（包括原有测试和新增的 tag_table 测试）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/memo.rs src-tauri/src/ai/tools.rs
git commit -m "refactor: list_tags and suggest_tags read from tag table instead of full scan"
```

---

### Task 6: 归档/恢复测试 + 最终验证

**Files:**
- Test: `core/tests/crud.rs`

- [ ] **Step 1: 写归档/恢复测试**

在 `core/tests/crud.rs` 末尾添加：

```rust
#[test]
fn tag_table_decrements_on_archive() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test-tag-archive".to_string(),
        content: "hello #rust".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    // 归档
    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: Some(RowStatus::Archived),
        content: None,
        visibility: None,
        pinned: None,
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    assert!(tags.is_empty(), "归档后 tag count 应归 0 且被删除");
}

#[test]
fn tag_table_increments_on_unarchive() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test-tag-unarchive".to_string(),
        content: "hello #rust".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    // 先归档
    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: Some(RowStatus::Archived),
        content: None,
        visibility: None,
        pinned: None,
        payload: None,
        location: None,
    }).unwrap();

    // 再恢复
    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: Some(RowStatus::Normal),
        content: None,
        visibility: None,
        pinned: None,
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let rust_count = tags.iter().find(|(n, _)| n == "rust").map(|(_, c)| *c).unwrap_or(0);
    assert_eq!(rust_count, 1, "恢复后 tag count 应为 1");
}

#[test]
fn tag_table_list_ordered_by_count() {
    let store = open_test_store();
    let conn = store.lock_conn();

    // 创建 3 个 memo：rust 出现 2 次，ai 出现 1 次
    memo::create(&conn, &CreateMemo {
        uid: "t1".to_string(), content: "#rust".to_string(),
        visibility: Visibility::Private, pinned: false, payload: json!({}), location: None,
    }).unwrap();
    memo::create(&conn, &CreateMemo {
        uid: "t2".to_string(), content: "#rust #ai".to_string(),
        visibility: Visibility::Private, pinned: false, payload: json!({}), location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    assert_eq!(tags[0].0, "rust", "count 最高的应排第一");
    assert_eq!(tags[0].1, 2);
    assert_eq!(tags[1].0, "ai");
    assert_eq!(tags[1].1, 1);
}
```

- [ ] **Step 2: 运行测试确认通过**

Run: `cargo test -p memos-core --test crud`
Expected: 所有测试 PASS

- [ ] **Step 3: 前端类型检查**

Run: `npx tsc --noEmit`
Expected: 无新增错误（预先存在的 markdown.ts 错误可忽略）

- [ ] **Step 4: 后端最终编译检查**

Run: `cargo check -p memos-app`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add core/tests/crud.rs
git commit -m "test(core): add archive/unarchive and ordering tests for tag table"
```
