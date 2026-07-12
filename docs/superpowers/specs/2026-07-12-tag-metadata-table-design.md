# Tag Metadata Table Design

> **Date**: 2026-07-12
> **Status**: Approved
> **Goal**: 将标签元数据（名称 + 使用次数）存入数据库 `tag` 表，消除 `list_tags` 的全表扫描 + 正则提取开销。

## 1. 背景与问题

当前标签系统完全基于 `#tag` 内联在 memo `content` 文本中，没有任何数据库表。标签通过 `core/src/markdown.rs::extract_tags` 状态机实时提取。

**性能问题**：`list_tags` 命令每次调用都 `SELECT content FROM memo WHERE row_status = 'NORMAL'` 读取所有 memo 的完整 content，然后在 Rust 层逐条 `extract_tags` 聚合计数。复杂度 O(N × M)（N=memo 数，M=平均 content 长度）。随着 memo 数增长，这个查询越来越慢。

`ai/tools.rs::execute_list_tags` 和 `commands/memo.rs::list_tags` 逻辑完全重复，都走全表扫描。

## 2. 设计决策

### 2.1 迁移策略：仅元数据表

保留 `#tag` 内联在 content 中的设计不变。新增 `tag` 表存储标签元数据（名称 + count + 时间戳），作为索引/缓存层。content 文本中的 `#tag` 仍是单一真相源。

**不选「索引同步」方案的原因**：虽然索引同步（建 tag + memo_tag 关联表）能加速 tag_search 过滤，但改动范围大（需改 `core/src/memo.rs::list` 的 tag_search 逻辑、LAN ACL 过滤等），YAGNI。当前 tag_search 的全扫描在可预见 memo 量下性能可接受。

**不选「完全迁移」方案的原因**：移除 content 中的 #tag 会破坏现有前端渲染、编辑器补全、remark 插件、导入导出等 10+ 处依赖，改动过大。

### 2.2 count 维护：实时同步

在 memo create/update/delete/archive/unarchive 时，**同一个数据库事务**内同步更新 tag 表的 count 字段。`list_tags` 直接读 tag 表，O(N_tags)。

### 2.3 归档处理

memo 归档（row_status 改为 ARCHIVED）时 tag count -1，恢复时 count +1。`list_tags` 返回的 count 只反映 NORMAL 状态 memo 的标签使用数。

### 2.4 不回填

V6 迁移建空 tag 表，不回填已有 memo 的标签。tag 表从空开始，只有新 create/update 的 memo 才同步。已有标签数据会在 memo 被编辑时逐步补全。

**理由**：回填需要全表扫描 + extract_tags（正是我们要消除的操作），且只需执行一次。用户可以手动编辑旧 memo 触发同步，或后续提供「重建标签表」维护命令。

## 3. 数据模型

### V6 迁移文件

文件：`core/migrations/V6__add_tag_metadata.sql`

```sql
-- 标签元数据表：存储标签名称、使用次数、时间戳
-- content 中的 #tag 仍是单一真相源，此表作为索引/缓存
CREATE TABLE IF NOT EXISTS tag (
    name TEXT PRIMARY KEY,           -- 标签名（不含 #），如 "rust"
    count INTEGER NOT NULL DEFAULT 0, -- 有多少 NORMAL 状态 memo 含此 tag
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now'))
);

-- 按使用次数降序索引，加速 list_tags 排序
CREATE INDEX IF NOT EXISTS idx_tag_count ON tag(count DESC);
```

### 字段说明

| 字段 | 类型 | 说明 |
|---|---|---|
| `name` | TEXT PRIMARY KEY | 标签名（不含 `#`），如 `rust`、`ai/ml` |
| `count` | INTEGER | 有多少 NORMAL 状态 memo 含此 tag，≥ 0 |
| `created_ts` | BIGINT | 首次出现时间戳（Unix 秒） |
| `updated_ts` | BIGINT | 最后一次 count 变更时间戳 |

**不存颜色/模糊等视觉元数据** — 这些已存在 `app_setting` 表 `user_setting:users/local/settings/TAGS` key 中（proto JSON 格式），保持不变。

## 4. Core 层实现

### 4.1 新文件 `core/src/tag.rs`

```rust
use rusqlite::{params, Connection};
use crate::{CoreResult, markdown};

/// 插入或递增标签计数（用于 memo create）
/// 对 content 中的每个 tag，执行 upsert：存在则 count+1，不存在则插入 count=1
pub fn upsert_tags_for_content(conn: &Connection, content: &str) -> CoreResult<()> {
    let tags = markdown::extract_tags(content);
    if tags.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp();
    let mut stmt = conn.prepare(
        "INSERT INTO tag (name, count, created_ts, updated_ts)
         VALUES (?1, 1, ?2, ?2)
         ON CONFLICT(name) DO UPDATE SET count = count + 1, updated_ts = ?2"
    )?;
    for tag in &tags {
        stmt.execute(params![tag, now])?;
    }
    Ok(())
}

/// 递减标签计数（用于 memo delete/archive）
/// 对 content 中的每个 tag，count -1，count <= 0 时删除行
pub fn decrement_tags_for_content(conn: &Connection, content: &str) -> CoreResult<()> {
    let tags = markdown::extract_tags(content);
    if tags.is_empty() {
        return Ok(());
    }
    let now = chrono::Utc::now().timestamp();
    // 先 count -1
    let mut stmt = conn.prepare(
        "UPDATE tag SET count = count - 1, updated_ts = ?2 WHERE name = ?1"
    )?;
    for tag in &tags {
        stmt.execute(params![tag, now])?;
    }
    // 再删除 count <= 0 的行
    conn.execute("DELETE FROM tag WHERE count <= 0", [])?;
    Ok(())
}

/// 同步标签计数（用于 memo update，content 变化时）
/// 计算新旧 content 的 tag 差集，对新增 tag count+1，对移除 tag count-1
pub fn sync_tags_on_update(
    conn: &Connection,
    old_content: &str,
    new_content: &str,
) -> CoreResult<()> {
    let old_tags: std::collections::HashSet<String> = markdown::extract_tags(old_content).into_iter().collect();
    let new_tags: std::collections::HashSet<String> = markdown::extract_tags(new_content).into_iter().collect();

    let added: Vec<&String> = new_tags.iter().filter(|t| !old_tags.contains(*t)).collect();
    let removed: Vec<&String> = old_tags.iter().filter(|t| !new_tags.contains(*t)).collect();

    let now = chrono::Utc::now().timestamp();

    // 新增 tag：upsert count+1
    if !added.is_empty() {
        let mut stmt = conn.prepare(
            "INSERT INTO tag (name, count, created_ts, updated_ts)
             VALUES (?1, 1, ?2, ?2)
             ON CONFLICT(name) DO UPDATE SET count = count + 1, updated_ts = ?2"
        )?;
        for tag in &added {
            stmt.execute(params![tag, now])?;
        }
    }

    // 移除 tag：count-1，然后清理 count<=0
    if !removed.is_empty() {
        let mut stmt = conn.prepare(
            "UPDATE tag SET count = count - 1, updated_ts = ?2 WHERE name = ?1"
        )?;
        for tag in &removed {
            stmt.execute(params![tag, now])?;
        }
        conn.execute("DELETE FROM tag WHERE count <= 0", [])?;
    }

    Ok(())
}

/// 查询所有标签及使用次数（替代全表扫描 + extract_tags）
pub fn list_tags(conn: &Connection) -> CoreResult<Vec<(String, i32)>> {
    let mut stmt = conn.prepare("SELECT name, count FROM tag ORDER BY count DESC, name ASC")?;
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

### 4.2 修改 `core/src/memo.rs`

在 `create_memo`、`update_memo`、`delete_memo`、`archive_memo`（如有）中，**同一事务内**调用 tag 同步函数。

**create_memo**：插入 memo 后调用 `tag::upsert_tags_for_content(conn, &content)`

**update_memo**：若 content 字段在 update mask 中，先从 DB 读旧 content，执行 update 后调用 `tag::sync_tags_on_update(conn, &old_content, &new_content)`

**delete_memo**：删除 memo 前调用 `tag::decrement_tags_for_content(conn, &content)`（先读 content）

**archive_memo / update row_status**：若 row_status 从 NORMAL → ARCHIVED，调用 `decrement_tags_for_content`；若 ARCHIVED → NORMAL，调用 `upsert_tags_for_content`

### 4.3 修改 `core/src/lib.rs`

新增 `pub mod tag;`

## 5. 命令层改造

### 5.1 `src-tauri/src/commands/memo.rs::list_tags`

```rust
#[tauri::command]
pub fn list_tags(state: tauri::State<'_, AppState>) -> IpcResult<Vec<TagWithCount>> {
    let store = state.store();
    let tags = store.with_conn(|c| tag::list_tags(c))?;
    Ok(tags
        .into_iter()
        .map(|(tag, count)| TagWithCount { tag, count })
        .collect())
}
```

从全表扫描 + extract_tags 变为单条 `SELECT name, count FROM tag ORDER BY count DESC`。

### 5.2 `src-tauri/src/ai/tools.rs::execute_list_tags`

同上，改为调用 `tag::list_tags(conn)`，消除与 `commands::memo::list_tags` 的代码重复。

### 5.3 `src-tauri/src/commands/memo.rs::suggest_tags`

当前 `suggest_tags` 命令内部也全表扫描提取 system_tags。改为读 tag 表：

```rust
// 之前：全表扫描 + extract_tags
// 之后：读 tag 表
let system_tags: Vec<String> = store.with_conn(|c| {
    tag::list_tags(c).map(|tags| tags.into_iter().map(|(name, _)| name).collect())
})?;
```

## 6. 不改动的部分

- **前端全部不变**：`#tag` 渲染、filter、TagsSection、TagTree、TagPresets、编辑器补全、remark 插件等
- **`extract_tags` 函数保留**：编辑器补全、LAN ACL 过滤、tag_search 过滤等仍需要从 content 实时提取
- **`core/src/memo.rs::list` 的 tag_search 过滤逻辑不变**：仍走 `extract_tags` 全扫描（本次不优化 tag_search）
- **LAN 模块的 tag 过滤不变**：`lan/auth.rs`、`lan/server.rs` 仍用 `extract_tags`
- **`app_setting` 表的 tag 视觉元数据不变**：颜色/模糊配置仍存 `user_setting:users/local/settings/TAGS`
- **`review_deck.tags` JSON 列不变**

## 7. 边界情况

1. **memo update 但 content 未变** — `build_update_mask` 不含 content，不触发 tag 同步
2. **tag count 降为 0** — `DELETE FROM tag WHERE count <= 0` 自动清理，tag 表不留垃圾
3. **并发写入** — SQLite 单写入者，事务内同步保证一致性
4. **memo import（批量导入）** — 若有批量导入功能，每个 memo 走 create_memo 即可自动同步
5. **tag 名称大小写** — `extract_tags` 保留原始大小写，tag 表 `name` 字段也保留原始大小写（`Rust` 和 `rust` 是不同 tag）

## 8. 测试计划

### 单元测试（`core/tests/tag_test.rs` 或 `src-tauri/tests/`）

1. `test_upsert_tags_for_content` — 创建含 `#rust #ai` 的 memo，tag 表有 2 行 count=1
2. `test_decrement_tags_on_delete` — 删除 memo 后 tag count 归 0 且行被删除
3. `test_sync_tags_on_update` — 更新 content 从 `#rust` 到 `#rust #go`，rust count 不变，go count=1
4. `test_sync_tags_on_update_remove_tag` — 更新 content 从 `#rust #ai` 到 `#rust`，ai count 归 0 删除
5. `test_archive_decrements_count` — 归档 memo 后 count -1
6. `test_unarchive_increments_count` — 恢复归档 memo 后 count +1
7. `test_list_tags_ordered_by_count` — 多个 tag 按 count 降序排列
8. `test_duplicate_tags_in_content` — content 含 `#rust #rust`，count 只 +1（extract_tags 已去重）
9. `test_no_tags_in_content` — content 无 #tag，tag 表无变化

### 集成验证

- `cargo build -p memos-core` 通过
- `cargo test` 全部通过
- `cargo check -p memos-app` 通过
- 手动验证：创建/编辑/删除 memo 后，侧边栏标签列表正确更新
