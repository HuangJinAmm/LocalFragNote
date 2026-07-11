# 回顾模块（AI 记忆卡 + FSRS）实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 LocalFragNote 新增回顾模块：用户按标签选定笔记集，AI 生成 ANKI 风格记忆卡片，FSRS 算法调度复习。

**Architecture:** Core 层新增 `review.rs`（实体 + CRUD + FSRS 封装）+ `V5` 迁移（3 张表）。命令层新增 `commands/review.rs`（12 个 Tauri 命令）。AI 层扩展 `tools.rs`（新增 `list_memos_by_tag` 工具）+ 卡片生成专用 agent loop（捕获 AI 输出的 JSON 卡片）。前端新增 `/review` 路由页面 + 设置 section。

**Tech Stack:** Rust（rs-fsrs 1.2.1、chrono、rusqlite、refinery）、React 19 + TypeScript、Tauri 2、react-router-dom（hash router）

---

## 文件结构

### 新建文件

| 文件 | 职责 |
|---|---|
| `core/migrations/V5__add_review_module.sql` | 建 review_deck / review_card / review_record 三表 |
| `core/src/review.rs` | ReviewDeck / ReviewCard / ReviewRecord 实体 + CRUD + FSRS score_card + 统计 |
| `src-tauri/src/commands/review.rs` | 12 个 Tauri 命令 + 卡片生成 agent loop |
| `src/pages/Review.tsx` | 回顾主页面（路由入口，根据 URL 渲染 DeckList/DeckDetail/CardReview） |
| `src/components/Review/DeckList.tsx` | deck 卡片网格 |
| `src/components/Review/DeckEditor.tsx` | 新建/编辑 deck（名称 + tags + cards_per_memo） |
| `src/components/Review/DeckDetail.tsx` | deck 详情页（统计 + 操作 + 卡片表格） |
| `src/components/Review/DeckStats.tsx` | 统计卡片组件 |
| `src/components/Review/CardReview.tsx` | 复习界面（翻转卡 + 4 评分按钮 + 键盘快捷键） |
| `src/components/Review/CardTable.tsx` | 卡片管理表格 |
| `src/components/Review/GenerationProgress.tsx` | AI 生成进度（监听事件流） |
| `src/components/Review/hooks.ts` | useReviewDecks / useDueCards / useGenerateCards / useScoreCard |
| `src/components/Review/types.ts` | TypeScript 类型定义（对应后端结构体） |
| `src/components/Review/index.ts` | 导出 |
| `src/components/Settings/ReviewSection.tsx` | 回顾设置 section |
| `src-tauri/tests/review_core.rs` | core 层集成测试（FSRS 调度 + 到期查询 + 统计） |

### 修改文件

| 文件 | 改动 |
|---|---|
| `core/src/lib.rs` | 新增 `pub mod review;` |
| `core/Cargo.toml` | 新增 `rs-fsrs = "1.2.1"`、`chrono = { version = "0.4", features = ["serde"] }` |
| `src-tauri/Cargo.toml` | 新增 `rs-fsrs = "1.2.1"`、`chrono = "0.4"` |
| `src-tauri/src/error.rs` | IpcError 新增 `Review(String)` 变体 |
| `src-tauri/src/commands/mod.rs` | 新增 `pub mod review;` |
| `src-tauri/src/main.rs` | generate_handler! 注册 12 个 review 命令 |
| `src-tauri/src/ai/tools.rs` | tool_definitions() 新增 list_memos_by_tag；execute_tool 新增分支 |
| `src/router/routes.ts` | 新增 `REVIEW: "/review"` |
| `src/router/index.tsx` | lazy import Review 页面 + 注册路由 |
| `src/components/Navigation.tsx` | 新增回顾导航项（BookOpenIcon） |
| `src/components/Settings/settingSections.ts` | 新增 review section 注册 |
| `src/locales/en.json` | 新增 review.* 翻译键 |
| `src/locales/zh-Hans.json` | 新增 review.* 中文翻译 |

---

## Task 1: 数据库迁移 + Core 实体定义

**Files:**
- Create: `core/migrations/V5__add_review_module.sql`
- Create: `core/src/review.rs`
- Modify: `core/src/lib.rs`
- Modify: `core/Cargo.toml`

- [ ] **Step 1: 创建迁移 SQL**

Create `core/migrations/V5__add_review_module.sql`:

```sql
-- 回顾模块：牌组、卡片、复习记录

CREATE TABLE review_deck (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    tags TEXT NOT NULL DEFAULT '[]',
    cards_per_memo INTEGER NOT NULL DEFAULT 2,
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    last_reviewed_ts BIGINT,
    memo_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE review_card (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    deck_id INTEGER NOT NULL,
    memo_uid TEXT NOT NULL,
    card_type TEXT NOT NULL,
    front TEXT NOT NULL,
    back TEXT NOT NULL,
    cloze_answer TEXT,
    angle TEXT NOT NULL DEFAULT '',
    stability REAL NOT NULL DEFAULT 0,
    difficulty REAL NOT NULL DEFAULT 0,
    due BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    last_review BIGINT,
    reps INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0,
    state INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    memo_deleted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_review_card_deck_id ON review_card(deck_id);
CREATE INDEX idx_review_card_due ON review_card(due);
CREATE INDEX idx_review_card_memo_uid ON review_card(memo_uid);

CREATE TABLE review_record (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    card_id INTEGER NOT NULL,
    rating INTEGER NOT NULL,
    reviewed_ts BIGINT NOT NULL,
    elapsed_days REAL NOT NULL DEFAULT 0,
    scheduled_days REAL NOT NULL DEFAULT 0,
    state INTEGER NOT NULL
);
CREATE INDEX idx_review_record_card_id ON review_record(card_id);
```

- [ ] **Step 2: 添加 core 依赖**

Modify `core/Cargo.toml`, 在 `[dependencies]` 末尾添加:

```toml
rs-fsrs = "1.2.1"
chrono = { version = "0.4", features = ["serde"] }
```

- [ ] **Step 3: 创建 review.rs 实体定义 + FSRS 转换**

Create `core/src/review.rs`:

```rust
//! 回顾模块：牌组、卡片、复习记录 + FSRS 调度
//!
//! 卡片由 AI 生成（见 commands/review.rs），复习调度由 rs-fsrs 实现。

use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use rs_fsrs::{Card as FsrsCard, FSRS, Rating, ReviewLog as FsrsReviewLog, State as FsrsState};
use serde::{Deserialize, Serialize};

// ==================== 实体 ====================

/// 牌组（笔记集配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDeck {
    pub id: i32,
    pub name: String,
    /// JSON 数组，如 ["rust","ai"]；序列化/反序列化为 Vec<String>
    pub tags: Vec<String>,
    pub cards_per_memo: i32,
    pub created_ts: i64,
    pub last_reviewed_ts: Option<i64>,
    /// 上次生成卡片时的 memo 数（检测新增用）
    pub memo_count: i32,
}

/// 卡片
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCard {
    pub id: i32,
    pub deck_id: i32,
    pub memo_uid: String,
    pub card_type: String,
    pub front: String,
    pub back: String,
    pub cloze_answer: Option<String>,
    pub angle: String,
    // FSRS 字段
    pub stability: f32,
    pub difficulty: f32,
    pub due: i64,
    pub last_review: Option<i64>,
    pub reps: u32,
    pub lapses: u32,
    pub state: u8,
    // 元数据
    pub created_ts: i64,
    pub memo_deleted: bool,
}

/// 复习记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecord {
    pub id: i32,
    pub card_id: i32,
    pub rating: u8,
    pub reviewed_ts: i64,
    pub elapsed_days: f32,
    pub scheduled_days: f32,
    pub state: u8,
}

// ==================== FSRS 转换 ====================

impl From<&ReviewCard> for FsrsCard {
    fn from(c: &ReviewCard) -> Self {
        FsrsCard {
            stability: c.stability,
            difficulty: c.difficulty,
            due: DateTime::from_timestamp(c.due, 0).unwrap_or_else(Utc::now),
            last_review: c.last_review.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            reps: c.reps,
            lapses: c.lapses,
            state: FsrsState::try_from(c.state).unwrap_or(FsrsState::New),
        }
    }
}

/// 牌组统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckStats {
    pub due_count: i32,
    pub new_count: i32,
    pub total: i32,
    pub learned: i32,
    pub retention_rate: f32,
    pub last_reviewed_ts: Option<i64>,
}
```

- [ ] **Step 4: 在 lib.rs 注册模块**

Modify `core/src/lib.rs`, 在 `pub mod reaction;` 之后添加:

```rust
pub mod review;
```

- [ ] **Step 5: 验证编译**

Run: `cargo build -p memos-core`
Expected: 编译成功（可能有 unused warning，无 error）

- [ ] **Step 6: Commit**

```bash
git add core/migrations/V5__add_review_module.sql core/src/review.rs core/src/lib.rs core/Cargo.toml
git commit -m "feat(review): add V5 migration, ReviewDeck/Card/Record entities with FSRS conversion"
```

---

## Task 2: Core 层 Deck CRUD

**Files:**
- Modify: `core/src/review.rs`（追加 Deck CRUD 函数）

- [ ] **Step 1: 追加 Deck CRUD 到 review.rs**

在 `core/src/review.rs` 末尾追加:

```rust
// ==================== Deck CRUD ====================

/// 创建牌组
pub fn create_deck(conn: &Connection, name: &str, tags: &[String], cards_per_memo: i32) -> CoreResult<ReviewDeck> {
    let tags_json = serde_json::to_string(tags)?;
    conn.execute(
        "INSERT INTO review_deck (name, tags, cards_per_memo) VALUES (?1, ?2, ?3)",
        params![name, &tags_json, cards_per_memo],
    )?;
    let id = conn.last_insert_rowid() as i32;
    get_deck(conn, id)?.ok_or_else(|| CoreError::Other("刚创建的 deck 不存在".into()))
}

/// 获取单个牌组
pub fn get_deck(conn: &Connection, id: i32) -> CoreResult<Option<ReviewDeck>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, tags, cards_per_memo, created_ts, last_reviewed_ts, memo_count
         FROM review_deck WHERE id = ?1",
    )?;
    let deck = stmt
        .query_row(params![id], |row| {
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok(ReviewDeck {
                id: row.get(0)?,
                name: row.get(1)?,
                tags,
                cards_per_memo: row.get(3)?,
                created_ts: row.get(4)?,
                last_reviewed_ts: row.get(5)?,
                memo_count: row.get(6)?,
            })
        })
        .ok();
    Ok(deck)
}

/// 列出所有牌组
pub fn list_decks(conn: &Connection) -> CoreResult<Vec<ReviewDeck>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, tags, cards_per_memo, created_ts, last_reviewed_ts, memo_count
         FROM review_deck ORDER BY created_ts DESC",
    )?;
    let decks = stmt
        .query_map([], |row| {
            let tags_json: String = row.get(2)?;
            let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
            Ok(ReviewDeck {
                id: row.get(0)?,
                name: row.get(1)?,
                tags,
                cards_per_memo: row.get(3)?,
                created_ts: row.get(4)?,
                last_reviewed_ts: row.get(5)?,
                memo_count: row.get(6)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(decks)
}

/// 更新牌组
pub fn update_deck(
    conn: &Connection,
    id: i32,
    name: &str,
    tags: &[String],
    cards_per_memo: i32,
) -> CoreResult<ReviewDeck> {
    let tags_json = serde_json::to_string(tags)?;
    let affected = conn.execute(
        "UPDATE review_deck SET name = ?1, tags = ?2, cards_per_memo = ?3 WHERE id = ?4",
        params![name, &tags_json, cards_per_memo, id],
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("deck id={id}")));
    }
    get_deck(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("deck id={id}")))
}

/// 删除牌组（级联删除卡片和复习记录）
pub fn delete_deck(conn: &Connection, id: i32) -> CoreResult<()> {
    // 先删除该 deck 下所有卡片的复习记录
    let card_ids: Vec<i32> = {
        let mut stmt = conn.prepare("SELECT id FROM review_card WHERE deck_id = ?1")?;
        stmt.query_map(params![id], |r| r.get::<_, i32>(0))?
            .filter_map(|r| r.ok())
            .collect()
    };
    for card_id in &card_ids {
        conn.execute("DELETE FROM review_record WHERE card_id = ?1", params![card_id])?;
    }
    conn.execute("DELETE FROM review_card WHERE deck_id = ?1", params![id])?;
    conn.execute("DELETE FROM review_deck WHERE id = ?1", params![id])?;
    Ok(())
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p memos-core`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add core/src/review.rs
git commit -m "feat(review): add ReviewDeck CRUD functions"
```

---

## Task 3: Core 层 Card CRUD

**Files:**
- Modify: `core/src/review.rs`（追加 Card CRUD）

- [ ] **Step 1: 追加 Card CRUD 到 review.rs**

在 `core/src/review.rs` 末尾追加:

```rust
// ==================== Card CRUD ====================

/// 创建卡片（AI 生成后批量插入用）
pub fn create_card(conn: &Connection, card: &ReviewCard) -> CoreResult<ReviewCard> {
    conn.execute(
        "INSERT INTO review_card
         (deck_id, memo_uid, card_type, front, back, cloze_answer, angle,
          stability, difficulty, due, last_review, reps, lapses, state, created_ts, memo_deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            card.deck_id, card.memo_uid, card.card_type, card.front, card.back,
            card.cloze_answer, card.angle, card.stability, card.difficulty,
            card.due, card.last_review, card.reps, card.lapses, card.state,
            card.created_ts, if card.memo_deleted { 1 } else { 0 },
        ],
    )?;
    let id = conn.last_insert_rowid() as i32;
    get_card(conn, id)?.ok_or_else(|| CoreError::Other("刚创建的 card 不存在".into()))
}

/// 获取单个卡片
pub fn get_card(conn: &Connection, id: i32) -> CoreResult<Option<ReviewCard>> {
    let mut stmt = conn.prepare(
        "SELECT id, deck_id, memo_uid, card_type, front, back, cloze_answer, angle,
                stability, difficulty, due, last_review, reps, lapses, state, created_ts, memo_deleted
         FROM review_card WHERE id = ?1",
    )?;
    Ok(stmt
        .query_row(params![id], row_to_card)
        .ok())
}

/// 行映射函数
fn row_to_card(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewCard> {
    let memo_deleted_int: i32 = row.get(16)?;
    Ok(ReviewCard {
        id: row.get(0)?,
        deck_id: row.get(1)?,
        memo_uid: row.get(2)?,
        card_type: row.get(3)?,
        front: row.get(4)?,
        back: row.get(5)?,
        cloze_answer: row.get(6)?,
        angle: row.get(7)?,
        stability: row.get(8)?,
        difficulty: row.get(9)?,
        due: row.get(10)?,
        last_review: row.get(11)?,
        reps: row.get(12)?,
        lapses: row.get(13)?,
        state: row.get(14)?,
        created_ts: row.get(15)?,
        memo_deleted: memo_deleted_int != 0,
    })
}

/// 列出 deck 下所有卡片
pub fn list_cards(conn: &Connection, deck_id: i32) -> CoreResult<Vec<ReviewCard>> {
    let mut stmt = conn.prepare(
        "SELECT id, deck_id, memo_uid, card_type, front, back, cloze_answer, angle,
                stability, difficulty, due, last_review, reps, lapses, state, created_ts, memo_deleted
         FROM review_card WHERE deck_id = ?1 ORDER BY created_ts DESC",
    )?;
    let cards = stmt
        .query_map(params![deck_id], row_to_card)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(cards)
}

/// 列出到期卡片（due <= now，排除已删除 memo 的卡片）
pub fn list_due_cards(conn: &Connection, deck_id: i32, limit: i32) -> CoreResult<Vec<ReviewCard>> {
    let now = Utc::now().timestamp();
    let mut stmt = conn.prepare(
        "SELECT id, deck_id, memo_uid, card_type, front, back, cloze_answer, angle,
                stability, difficulty, due, last_review, reps, lapses, state, created_ts, memo_deleted
         FROM review_card
         WHERE deck_id = ?1 AND due <= ?2 AND memo_deleted = 0
         ORDER BY due ASC LIMIT ?3",
    )?;
    let cards = stmt
        .query_map(params![deck_id, now, limit], row_to_card)?
        .filter_map(|r| r.ok())
        .collect();
    Ok(cards)
}

/// 删除卡片
pub fn delete_card(conn: &Connection, id: i32) -> CoreResult<()> {
    conn.execute("DELETE FROM review_record WHERE card_id = ?1", params![id])?;
    conn.execute("DELETE FROM review_card WHERE id = ?1", params![id])?;
    Ok(())
}

/// 更新卡片的 memo_deleted 标记（memo 被删除时调用）
pub fn mark_cards_memo_deleted(conn: &Connection, memo_uid: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE review_card SET memo_deleted = 1 WHERE memo_uid = ?1",
        params![memo_uid],
    )?;
    Ok(())
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p memos-core`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add core/src/review.rs
git commit -m "feat(review): add ReviewCard CRUD and due card query"
```

---

## Task 4: Core 层 FSRS 调度 + 统计

**Files:**
- Modify: `core/src/review.rs`（追加 score_card + deck_stats）

- [ ] **Step 1: 追加 FSRS 调度函数**

在 `core/src/review.rs` 末尾追加:

```rust
// ==================== FSRS 调度 ====================

/// 评分：根据 rating 更新卡片调度，返回复习记录
///
/// rating: 1=Again 2=Hard 3=Good 4=Easy
/// fsrs_params: 空切片=默认参数，否则用自定义参数
pub fn score_card(
    conn: &Connection,
    card_id: i32,
    rating: u8,
    fsrs_params: &[f32],
) -> CoreResult<(ReviewCard, ReviewRecord)> {
    let mut card = get_card(conn, card_id)?
        .ok_or_else(|| CoreError::NotFound(format!("card id={card_id}")))?;

    let now = Utc::now();
    let fsrs = if fsrs_params.is_empty() {
        FSRS::default()
    } else {
        FSRS::new(Some(fsrs_params.to_vec()))
    };

    let fsrs_card: FsrsCard = (&card).into();
    let rating_enum = match rating {
        1 => Rating::Again,
        2 => Rating::Hard,
        3 => Rating::Good,
        4 => Rating::Easy,
        _ => Rating::Good,
    };
    let record_log = fsrs.repeat(fsrs_card, now);
    let item = &record_log[rating_enum];
    let new_card = &item.card;
    let log: &FsrsReviewLog = &item.review_log;

    // 更新 card 字段
    card.stability = new_card.stability;
    card.difficulty = new_card.difficulty;
    card.due = new_card.due.timestamp();
    card.last_review = Some(now.timestamp());
    card.reps = new_card.reps;
    card.lapses = new_card.lapses;
    card.state = new_card.state as u8;

    // 写回 DB
    conn.execute(
        "UPDATE review_card SET stability = ?1, difficulty = ?2, due = ?3, last_review = ?4,
         reps = ?5, lapses = ?6, state = ?7 WHERE id = ?8",
        params![
            card.stability, card.difficulty, card.due, card.last_review,
            card.reps, card.lapses, card.state, card.id,
        ],
    )?;

    let record = ReviewRecord {
        id: 0,
        card_id: card.id,
        rating,
        reviewed_ts: now.timestamp(),
        elapsed_days: log.elapsed_days,
        scheduled_days: log.scheduled_days,
        state: log.state as u8,
    };

    // 插入复习记录
    conn.execute(
        "INSERT INTO review_record (card_id, rating, reviewed_ts, elapsed_days, scheduled_days, state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            record.card_id, record.rating, record.reviewed_ts,
            record.elapsed_days, record.scheduled_days, record.state,
        ],
    )?;
    let record_id = conn.last_insert_rowid() as i32;
    let mut record = record;
    record.id = record_id;

    Ok((card, record))
}

/// 更新 deck 的 last_reviewed_ts
pub fn touch_deck_reviewed(conn: &Connection, deck_id: i32) -> CoreResult<()> {
    let now = Utc::now().timestamp();
    conn.execute(
        "UPDATE review_deck SET last_reviewed_ts = ?1 WHERE id = ?2",
        params![now, deck_id],
    )?;
    Ok(())
}

// ==================== 统计 ====================

/// 计算牌组统计
pub fn deck_stats(conn: &Connection, deck_id: i32) -> CoreResult<DeckStats> {
    let now = Utc::now().timestamp();
    let week_ago = now - 7 * 24 * 3600;

    let total: i32 = conn.query_row(
        "SELECT COUNT(*) FROM review_card WHERE deck_id = ?1 AND memo_deleted = 0",
        params![deck_id],
        |r| r.get(0),
    )?;

    let due_count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM review_card WHERE deck_id = ?1 AND due <= ?2 AND memo_deleted = 0",
        params![deck_id, now],
        |r| r.get(0),
    )?;

    let new_count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM review_card WHERE deck_id = ?1 AND state = 0 AND memo_deleted = 0",
        params![deck_id],
        |r| r.get(0),
    )?;

    let learned: i32 = conn.query_row(
        "SELECT COUNT(*) FROM review_card WHERE deck_id = ?1 AND reps > 0 AND memo_deleted = 0",
        params![deck_id],
        |r| r.get(0),
    )?;

    // 最近 7 天掌握率：(Good+Easy) / 总评分
    let retention_rate: f32 = conn.query_row(
        "SELECT
            CASE WHEN COUNT(*) > 0
                THEN CAST(SUM(CASE WHEN r.rating IN (3, 4) THEN 1 ELSE 0 END) AS FLOAT) / COUNT(*)
                ELSE 0.0
            END
         FROM review_record r
         JOIN review_card c ON r.card_id = c.id
         WHERE c.deck_id = ?1 AND r.reviewed_ts >= ?2",
        params![deck_id, week_ago],
        |r| r.get(0),
    )?;

    let last_reviewed_ts: Option<i64> = conn.query_row(
        "SELECT last_reviewed_ts FROM review_deck WHERE id = ?1",
        params![deck_id],
        |r| r.get(0),
    )?;

    Ok(DeckStats {
        due_count,
        new_count,
        total,
        learned,
        retention_rate,
        last_reviewed_ts,
    })
}
```

- [ ] **Step 2: 验证编译**

Run: `cargo build -p memos-core`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add core/src/review.rs
git commit -m "feat(review): add FSRS score_card scheduling and deck_stats"
```

---

## Task 5: Core 层单元测试（TDD 验证）

**Files:**
- Create: `src-tauri/tests/review_core.rs`
- Modify: `src-tauri/src/lib.rs`（暴露 review 模块给测试）

- [ ] **Step 1: 暴露 core review 模块**

检查 `src-tauri/src/lib.rs` 当前内容，确认是否有 `pub mod` 声明。如果没有 lib.rs 或不含 review，追加:

```rust
// src-tauri/src/lib.rs（如已存在则追加，否则新建）
pub mod error;
pub mod state;
```

注意：测试通过 `memos_core::review` 直接访问 core 层，无需经过 src-tauri。

- [ ] **Step 2: 创建测试文件**

Create `src-tauri/tests/review_core.rs`:

```rust
//! Review module core layer tests
//!
//! 测试 FSRS 调度、到期查询、统计、Deck/Card CRUD

use memos_core::review::*;
use memos_core::Store;

fn setup_store() -> Store {
    Store::open_in_memory().expect("无法打开内存数据库")
}

#[test]
fn test_create_deck_stores_tags_json() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Rust 基础", &["rust".into(), "ai".into()], 3)?;
        assert_eq!(deck.name, "Rust 基础");
        assert_eq!(deck.tags, vec!["rust", "ai"]);
        assert_eq!(deck.cards_per_memo, 3);
        assert_eq!(deck.memo_count, 0);
        Ok(())
    }).unwrap();
}

#[test]
fn test_list_decks_returns_all() {
    let store = setup_store();
    store.with_conn(|c| {
        create_deck(c, "A", &["t1".into()], 1)?;
        create_deck(c, "B", &["t2".into()], 2)?;
        let decks = list_decks(c)?;
        assert_eq!(decks.len(), 2);
        Ok(())
    }).unwrap();
}

#[test]
fn test_update_deck() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Old", &["t1".into()], 1)?;
        let updated = update_deck(c, deck.id, "New", &["t2".into(), "t3".into()], 5)?;
        assert_eq!(updated.name, "New");
        assert_eq!(updated.tags, vec!["t2", "t3"]);
        assert_eq!(updated.cards_per_memo, 5);
        Ok(())
    }).unwrap();
}

#[test]
fn test_delete_deck_cascades_cards_and_records() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let card = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "test-uid".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "定义".into(),
            stability: 0.0, difficulty: 0.0,
            due: chrono::Utc::now().timestamp(), last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: chrono::Utc::now().timestamp(),
            memo_deleted: false,
        };
        let card = create_card(c, &card)?;
        // 评分一次产生 record
        score_card(c, card.id, 3, &[])?;
        // 删除 deck
        delete_deck(c, deck.id)?;
        // 验证 card 和 record 都被删除
        assert!(get_card(c, card.id)?.is_none());
        assert!(get_deck(c, deck.id)?.is_none());
        Ok(())
    }).unwrap();
}

#[test]
fn test_list_due_cards_excludes_future() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        // 创建一张到期卡（due=now-100）
        let card_due = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q1".into(), back: "A1".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now - 100, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        };
        create_card(c, &card_due)?;
        // 创建一张未到期卡（due=now+100000）
        let card_future = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u2".into(),
            card_type: "basic".into(), front: "Q2".into(), back: "A2".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now + 100000, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        };
        create_card(c, &card_future)?;

        let due = list_due_cards(c, deck.id, 100)?;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].front, "Q1");
        Ok(())
    }).unwrap();
}

#[test]
fn test_list_due_cards_excludes_deleted_memo() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        let card = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now - 100, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now,
            memo_deleted: true, // 已删除
        };
        create_card(c, &card)?;
        let due = list_due_cards(c, deck.id, 100)?;
        assert_eq!(due.len(), 0);
        Ok(())
    }).unwrap();
}

#[test]
fn test_score_card_new_good_enters_review() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        let card = create_card(c, &ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        })?;

        let (updated, record) = score_card(c, card.id, 3, &[])?; // Good
        assert!(updated.reps >= 1, "reps should increment");
        assert!(updated.due > now, "due should be in the future after Good");
        assert_eq!(record.rating, 3);
        assert_eq!(record.card_id, card.id);
        Ok(())
    }).unwrap();
}

#[test]
fn test_score_card_review_again_lapses() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        // 先创建一张已学过的卡片（state=2 Review, reps=5, lapses=1）
        let card = create_card(c, &ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 5.0, difficulty: 0.5,
            due: now, last_review: Some(now - 86400),
            reps: 5, lapses: 1, state: 2, created_ts: now, memo_deleted: false,
        })?;

        let (updated, _record) = score_card(c, card.id, 1, &[])?; // Again
        assert!(updated.lapses >= 2, "lapses should increment on Again");
        Ok(())
    }).unwrap();
}

#[test]
fn test_deck_stats() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        // 创建 3 张卡：1 张到期，1 张未到期，1 张已学
        for i in 0..3 {
            let card = ReviewCard {
                id: 0, deck_id: deck.id, memo_uid: format!("u{i}"),
                card_type: "basic".into(), front: format!("Q{i}"), back: "A".into(),
                cloze_answer: None, angle: "".into(),
                stability: 0.0, difficulty: 0.0,
                due: if i < 2 { now - 100 } else { now + 100000 },
                last_review: None,
                reps: if i == 0 { 5 } else { 0 },
                lapses: 0, state: if i == 0 { 2 } else { 0 },
                created_ts: now, memo_deleted: false,
            };
            create_card(c, &card)?;
        }
        let stats = deck_stats(c, deck.id)?;
        assert_eq!(stats.total, 3);
        assert_eq!(stats.due_count, 2);
        assert_eq!(stats.new_count, 2);
        assert_eq!(stats.learned, 1);
        Ok(())
    }).unwrap();
}

#[test]
fn test_mark_cards_memo_deleted() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        let card = create_card(c, &ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "test-uid".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        })?;
        mark_cards_memo_deleted(c, "test-uid")?;
        let updated = get_card(c, card.id)?.unwrap();
        assert!(updated.memo_deleted, "memo_deleted should be true");
        Ok(())
    }).unwrap();
}
```

- [ ] **Step 3: 运行测试验证全部通过**

Run: `cd src-tauri && cargo test --test review_core`
Expected: 10 个测试全部 PASS

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tests/review_core.rs src-tauri/src/lib.rs
git commit -m "test(review): add 10 core layer tests for deck/card CRUD, FSRS scheduling, stats"
```

---

## Task 6: AI 工具扩展 list_memos_by_tag

**Files:**
- Modify: `src-tauri/src/ai/tools.rs`

- [ ] **Step 1: 在 tool_definitions() 新增 list_memos_by_tag**

Modify `src-tauri/src/ai/tools.rs`, 在 `tool_definitions()` 函数的 vec 末尾（`list_tags` 之后）追加:

```rust
        json!({
            "type": "function",
            "function": {
                "name": "list_memos_by_tag",
                "description": "List memos that contain ALL specified tags. Returns memo content for card generation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags to filter (memo must contain ALL)" },
                        "limit": { "type": "number", "description": "Max results, default 50" }
                    },
                    "required": ["tags"]
                }
            }
        }),
```

- [ ] **Step 2: 在 execute_tool match 新增分支**

Modify `src-tauri/src/ai/tools.rs` 的 `execute_tool` 函数，在 `"list_tags" => ...` 之后追加:

```rust
        "list_memos_by_tag" => execute_list_memos_by_tag(args, store),
```

- [ ] **Step 3: 实现 execute_list_memos_by_tag 函数**

在 `tools.rs` 末尾（`uuid_like` 函数之前）追加:

```rust
fn execute_list_memos_by_tag(args: &Value, store: &Store) -> memos_core::CoreResult<Value> {
    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if tags.is_empty() {
        return Ok(json!({ "memos": [] }));
    }

    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32)
        .unwrap_or(50)
        .min(200)
        .max(1) as i32;

    let find = FindMemo {
        tag_search: tags.clone(),
        row_status: Some(RowStatus::Normal),
        limit: Some(limit),
        ..Default::default()
    };

    let memos = store.with_conn(|c| memos_core::memo::list(c, &find))?;
    let result: Vec<Value> = memos
        .iter()
        .map(|m| {
            json!({
                "uid": m.uid,
                "content": m.content,
                "tags": markdown::extract_tags(&m.content),
                "created_ts": m.created_ts,
                "updated_ts": m.updated_ts,
            })
        })
        .collect();
    Ok(json!({ "memos": result }))
}
```

- [ ] **Step 4: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/tools.rs
git commit -m "feat(ai): add list_memos_by_tag tool for review card generation"
```

---

## Task 7: IpcError 扩展 + 命令层基础（Deck/Card 命令）

**Files:**
- Modify: `src-tauri/src/error.rs`
- Create: `src-tauri/src/commands/review.rs`
- Modify: `src-tauri/src/commands/mod.rs`

- [ ] **Step 1: IpcError 新增 Review 变体**

Modify `src-tauri/src/error.rs`:

在 `IpcError` enum 中 `Lan(String)` 之后添加:

```rust
    /// 回顾模块错误
    Review(String),
```

在 `impl fmt::Display for IpcError` 的 match 中添加:

```rust
            IpcError::Review(msg) => write!(f, "Review: {msg}"),
```

- [ ] **Step 2: 创建 commands/review.rs（Deck/Card 命令）**

Create `src-tauri/src/commands/review.rs`:

```rust
//! 回顾模块 Tauri 命令

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use memos_core::review::{
    self, DeckStats, ReviewCard, ReviewDeck,
};
use serde::{Deserialize, Serialize};

// ==================== 返回类型 ====================

#[derive(Debug, Serialize)]
pub struct ScoreResult {
    pub updated_card: ReviewCard,
    pub next_card: Option<ReviewCard>,
    pub session_stats: SessionStats,
}

#[derive(Debug, Serialize)]
pub struct SessionStats {
    pub reviewed: u32,
    pub again: u32,
    pub hard: u32,
    pub good: u32,
    pub easy: u32,
    pub retention_rate: f32,
}

// ==================== Deck 命令 ====================

#[tauri::command]
pub fn review_list_decks(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ReviewDeck>> {
    let store = state.store();
    Ok(store.with_conn(|c| review::list_decks(c))?)
}

#[tauri::command]
pub fn review_create_deck(
    state: tauri::State<'_, AppState>,
    name: String,
    tags: Vec<String>,
    cards_per_memo: i32,
) -> IpcResult<ReviewDeck> {
    if name.trim().is_empty() {
        return Err(IpcError::BadRequest("name 不能为空".into()));
    }
    let cards_per_memo = cards_per_memo.clamp(1, 10);
    let store = state.store();
    Ok(store.with_conn(|c| review::create_deck(c, &name, &tags, cards_per_memo))?)
}

#[tauri::command]
pub fn review_update_deck(
    state: tauri::State<'_, AppState>,
    id: i32,
    name: String,
    tags: Vec<String>,
    cards_per_memo: i32,
) -> IpcResult<ReviewDeck> {
    let cards_per_memo = cards_per_memo.clamp(1, 10);
    let store = state.store();
    Ok(store.with_conn(|c| review::update_deck(c, id, &name, &tags, cards_per_memo))?)
}

#[tauri::command]
pub fn review_delete_deck(state: tauri::State<'_, AppState>, id: i32) -> IpcResult<()> {
    let store = state.store();
    Ok(store.with_conn(|c| review::delete_deck(c, id))?)
}

// ==================== Card 命令 ====================

#[tauri::command]
pub fn review_list_cards(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<Vec<ReviewCard>> {
    let store = state.store();
    Ok(store.with_conn(|c| review::list_cards(c, deck_id))?)
}

#[tauri::command]
pub fn review_list_due_cards(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
    limit: Option<i32>,
) -> IpcResult<Vec<ReviewCard>> {
    let limit = limit.unwrap_or(50).clamp(1, 500);
    let store = state.store();
    Ok(store.with_conn(|c| review::list_due_cards(c, deck_id, limit))?)
}

#[tauri::command]
pub fn review_delete_card(state: tauri::State<'_, AppState>, card_id: i32) -> IpcResult<()> {
    let store = state.store();
    Ok(store.with_conn(|c| review::delete_card(c, card_id))?)
}

// ==================== 统计命令 ====================

#[tauri::command]
pub fn review_deck_stats(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<DeckStats> {
    let store = state.store();
    Ok(store.with_conn(|c| review::deck_stats(c, deck_id))?)
}

#[tauri::command]
pub fn review_check_new_memos(
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<i32> {
    let store = state.store();
    let deck = store.with_conn(|c| review::get_deck(c, deck_id))?
        .ok_or_else(|| IpcError::NotFound(format!("deck id={deck_id}")))?;

    // 查询当前 tag 下的 memo 数
    let current_count = store.with_conn(|c| -> memos_core::CoreResult<i32> {
        let find = memos_core::memo::FindMemo {
            tag_search: deck.tags.clone(),
            row_status: Some(memos_core::types::RowStatus::Normal),
            ..Default::default()
        };
        let memos = memos_core::memo::list(c, &find)?;
        Ok(memos.len() as i32)
    })?;

    let new_count = (current_count - deck.memo_count).max(0);
    Ok(new_count)
}
```

- [ ] **Step 3: 在 commands/mod.rs 注册模块**

Modify `src-tauri/src/commands/mod.rs`, 在 `pub mod reaction;` 之后添加:

```rust
pub mod review;
```

- [ ] **Step 4: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/error.rs src-tauri/src/commands/review.rs src-tauri/src/commands/mod.rs
git commit -m "feat(review): add IpcError::Review variant and Deck/Card/Stats Tauri commands"
```

---

## Task 8: 评分命令 + FSRS 参数读取

**Files:**
- Modify: `src-tauri/src/commands/review.rs`（追加 score_card 命令）

- [ ] **Step 1: 追加 score_card 命令**

在 `src-tauri/src/commands/review.rs` 末尾追加:

```rust
// ==================== 复习命令 ====================

/// 评分卡片
///
/// rating: 1=Again 2=Hard 3=Good 4=Easy
#[tauri::command]
pub fn review_score_card(
    state: tauri::State<'_, AppState>,
    card_id: i32,
    rating: u8,
    deck_id: i32,
) -> IpcResult<ScoreResult> {
    let store = state.store();

    // 读取 FSRS 参数（空=默认）
    let fsrs_params: Vec<f32> = store
        .with_conn(|c| {
            store
                .setting
                .app
                .get(c, "fsrs_params")
        })?
        .and_then(|json| serde_json::from_str::<Vec<f32>>(&json).ok())
        .unwrap_or_default();

    // 评分并更新卡片
    let (updated_card, _record) = store.with_conn(|c| {
        review::score_card(c, card_id, rating, &fsrs_params)
    })?;

    // 更新 deck 的 last_reviewed_ts
    store.with_conn(|c| review::touch_deck_reviewed(c, deck_id))?;

    // 查询下一张到期卡片
    let next_cards = store.with_conn(|c| review::list_due_cards(c, deck_id, 1))?;
    let next_card = next_cards.into_iter().next();

    // 计算本次 session 统计（最近 1 小时内该 deck 的评分）
    let session_stats = store.with_conn(|c| compute_session_stats(c, deck_id))?;

    Ok(ScoreResult {
        updated_card,
        next_card,
        session_stats,
    })
}

/// 计算最近 1 小时的 session 统计
fn compute_session_stats(conn: &rusqlite::Connection, deck_id: i32) -> IpcResult<SessionStats> {
    let one_hour_ago = chrono::Utc::now().timestamp() - 3600;

    let stats: (u32, u32, u32, u32, u32, f32) = conn
        .query_row(
            "SELECT
                COUNT(*) as reviewed,
                SUM(CASE WHEN r.rating = 1 THEN 1 ELSE 0 END) as again,
                SUM(CASE WHEN r.rating = 2 THEN 1 ELSE 0 END) as hard,
                SUM(CASE WHEN r.rating = 3 THEN 1 ELSE 0 END) as good,
                SUM(CASE WHEN r.rating = 4 THEN 1 ELSE 0 END) as easy,
                CASE WHEN COUNT(*) > 0
                    THEN CAST(SUM(CASE WHEN r.rating IN (3, 4) THEN 1 ELSE 0 END) AS FLOAT) / COUNT(*)
                    ELSE 0.0
                END as retention
             FROM review_record r
             JOIN review_card c ON r.card_id = c.id
             WHERE c.deck_id = ?1 AND r.reviewed_ts >= ?2",
            rusqlite::params![deck_id, one_hour_ago],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as u32,
                    row.get::<_, i64>(1)? as u32,
                    row.get::<_, i64>(2)? as u32,
                    row.get::<_, i64>(3)? as u32,
                    row.get::<_, i64>(4)? as u32,
                    row.get::<_, f64>(5)? as f32,
                ))
            },
        )
        .map_err(|e| IpcError::Internal(e.to_string()))?;

    Ok(SessionStats {
        reviewed: stats.0,
        again: stats.1,
        hard: stats.2,
        good: stats.3,
        easy: stats.4,
        retention_rate: stats.5,
    })
}
```

- [ ] **Step 2: 在 src-tauri/Cargo.toml 添加 chrono 依赖**

Modify `src-tauri/Cargo.toml`, 在 `[dependencies]` 中添加（如尚无）:

```toml
chrono = "0.4"
```

- [ ] **Step 3: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/review.rs src-tauri/Cargo.toml
git commit -m "feat(review): add review_score_card command with FSRS scheduling and session stats"
```

---

## Task 9: AI 卡片生成命令

**Files:**
- Modify: `src-tauri/src/commands/review.rs`（追加 generate_cards + regenerate_card + agent loop）

- [ ] **Step 1: 追加卡片生成相关代码**

在 `src-tauri/src/commands/review.rs` 顶部 import 区追加:

```rust
use crate::ai::provider::{load_providers, ProviderConfig};
use crate::ai::sse::read_sse_stream;
use crate::ai::tools::{execute_tool, tool_definitions};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tauri::{AppHandle, Emitter, Manager};
```

然后在文件末尾追加:

```rust
// ==================== AI 卡片生成 ====================

static REVIEW_RUN_ID: AtomicU32 = AtomicU32::new(1);

const CARD_GEN_SYSTEM_PROMPT: &str = r#"你是一个记忆卡片生成专家。根据用户的笔记内容，生成 ANKI 风格的记忆卡片。

## 卡片类型
- basic: 问答卡（正面问题，背面答案）
- reversed: 翻转卡（正面术语，背面定义）
- cloze: 填空卡（front 带 {{答案}} 占位，cloze_answer 存答案词）
- concept: 概念解释卡（"请解释：X" → 完整解释）
- compare: 对比卡（"对比 A 和 B" → 异同点）

## 输出格式
返回 JSON 数组，每个元素：
{
  "memo_uid": "来源 memo 的 uid",
  "card_type": "basic|reversed|cloze|concept|compare",
  "front": "正面内容（Markdown）",
  "back": "背面内容（Markdown）",
  "cloze_answer": "填空答案（仅 cloze 类型，其他为 null）",
  "angle": "考核点，如：定义|应用|对比|列举|原理"
}

## 规则
1. 每条 memo 最多生成指定数量的卡片
2. 优先生成核心知识点，避免琐碎细节
3. front/back 必须独立完整，不依赖其他卡片
4. 同一 memo 的不同卡片应覆盖不同 angle
5. 只返回 JSON 数组，不要其他文字"#;

#[derive(Debug, Clone, Serialize)]
struct ReviewGenStarted {
    deck_id: i32,
    run_id: u32,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewGenDone {
    deck_id: i32,
    run_id: u32,
    count: usize,
    errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewGenError {
    deck_id: i32,
    run_id: u32,
    error: String,
}

#[derive(Debug, Clone, Serialize)]
struct ReviewChunk {
    run_id: u32,
    text: String,
}

/// AI 生成的卡片草案（从 JSON 解析）
#[derive(Debug, Deserialize)]
struct CardDraft {
    memo_uid: String,
    card_type: String,
    front: String,
    back: String,
    cloze_answer: Option<String>,
    angle: Option<String>,
}

/// 生成卡片（异步，通过事件推送进度）
#[tauri::command]
pub async fn review_generate_cards(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    deck_id: i32,
) -> IpcResult<u32> {
    let run_id = REVIEW_RUN_ID.fetch_add(1, Ordering::SeqCst);

    // 读取 deck
    let deck = {
        let store = state.store();
        store
            .with_conn(|c| review::get_deck(c, deck_id))?
            .ok_or_else(|| IpcError::NotFound(format!("deck id={deck_id}")))?
    };

    // 读取 AI provider
    let provider = {
        let store = state.store();
        let providers = load_providers(&store);
        // 优先用 review_config.ai_provider_id，否则用第一个
        let config_json = store
            .with_conn(|c| store.setting.app.get(c, "review_config"))?
            .unwrap_or_default();
        let provider_id: String = serde_json::from_str::<Value>(&config_json)
            .ok()
            .and_then(|v| v.get("ai_provider_id").and_then(|s| s.as_str().map(String::from)))
            .unwrap_or_default();
        if !provider_id.is_empty() {
            providers
                .iter()
                .find(|p| p.id == provider_id)
                .cloned()
                .ok_or_else(|| IpcError::BadRequest("review_config 指定的 provider 不存在".into()))?
        } else if let Some(first) = providers.first() {
            first.clone()
        } else {
            return Err(IpcError::BadRequest(
                "未配置 AI provider，请先在设置中配置".into(),
            ));
        }
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        card_generation_loop(app_handle, run_id, deck_id, deck, provider);
    });

    Ok(run_id)
}

/// "换角度"重新生成单张卡片
#[tauri::command]
pub async fn review_regenerate_card(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    card_id: i32,
) -> IpcResult<u32> {
    let run_id = REVIEW_RUN_ID.fetch_add(1, Ordering::SeqCst);

    // 读取原卡片
    let (card, deck_id) = {
        let store = state.store();
        let card = store
            .with_conn(|c| review::get_card(c, card_id))?
            .ok_or_else(|| IpcError::NotFound(format!("card id={card_id}")))?;
        let deck_id = card.deck_id;
        (card, deck_id)
    };

    // 读取 deck
    let deck = {
        let store = state.store();
        store
            .with_conn(|c| review::get_deck(c, deck_id))?
            .ok_or_else(|| IpcError::NotFound(format!("deck id={deck_id}")))?
    };

    // 读取 provider
    let provider = {
        let store = state.store();
        let providers = load_providers(&store);
        providers.first().cloned().ok_or_else(|| {
            IpcError::BadRequest("未配置 AI provider".into())
        })?
    };

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        card_regeneration_loop(app_handle, run_id, deck_id, card, deck, provider);
    });

    Ok(run_id)
}

/// 卡片生成 agent loop
fn card_generation_loop(
    app: AppHandle,
    run_id: u32,
    deck_id: i32,
    deck: ReviewDeck,
    provider: ProviderConfig,
) {
    let _ = app.emit(
        "review:generation-started",
        ReviewGenStarted { deck_id, run_id },
    );

    let tags_str = deck.tags.join(", ");
    let user_prompt = format!(
        "请为以下标签的笔记生成记忆卡片：\n- 标签：{}\n- 每条笔记最多生成 {} 张卡片\n\n使用 list_memos_by_tag 工具读取笔记内容。只返回 JSON 数组。",
        tags_str, deck.cards_per_memo
    );

    let messages = vec![json!({
        "role": "user",
        "content": user_prompt,
    })];

    let result = run_card_agent(&app, run_id, &provider, &messages);

    match result {
        Ok(content) => {
            // 解析 JSON 卡片
            let drafts = parse_card_json(&content);
            let mut errors = Vec::new();

            // 查询当前 tag 下的 memo 数
            let state = app.state::<AppState>();
            let store = state.store();
            let memo_count = store
                .with_conn(|c| -> memos_core::CoreResult<i32> {
                    let find = memos_core::memo::FindMemo {
                        tag_search: deck.tags.clone(),
                        row_status: Some(memos_core::types::RowStatus::Normal),
                        ..Default::default()
                    };
                    Ok(memos_core::memo::list(c, &find)?.len() as i32)
                })
                .unwrap_or(0);

            let mut inserted = 0;
            for draft in &drafts {
                match store.with_conn(|c| {
                    let now = chrono::Utc::now().timestamp();
                    let card = ReviewCard {
                        id: 0,
                        deck_id,
                        memo_uid: draft.memo_uid.clone(),
                        card_type: draft.card_type.clone(),
                        front: draft.front.clone(),
                        back: draft.back.clone(),
                        cloze_answer: draft.cloze_answer.clone(),
                        angle: draft.angle.clone().unwrap_or_default(),
                        stability: 0.0,
                        difficulty: 0.0,
                        due: now,
                        last_review: None,
                        reps: 0,
                        lapses: 0,
                        state: 0,
                        created_ts: now,
                        memo_deleted: false,
                    };
                    review::create_card(c, &card)
                }) {
                    Ok(_) => inserted += 1,
                    Err(e) => errors.push(format!("card memo_uid={}: {e}", draft.memo_uid)),
                }
            }

            // 更新 deck.memo_count
            let _ = store.with_conn(|c| {
                c.execute(
                    "UPDATE review_deck SET memo_count = ?1 WHERE id = ?2",
                    rusqlite::params![memo_count, deck_id],
                )
            });

            let _ = app.emit(
                "review:cards-generated",
                ReviewGenDone {
                    deck_id,
                    run_id,
                    count: inserted,
                    errors,
                },
            );
        }
        Err(e) => {
            let _ = app.emit(
                "review:generation-error",
                ReviewGenError {
                    deck_id,
                    run_id,
                    error: e,
                },
            );
        }
    }
}

/// "换角度"重新生成 loop
fn card_regeneration_loop(
    app: AppHandle,
    run_id: u32,
    deck_id: i32,
    old_card: ReviewCard,
    deck: ReviewDeck,
    provider: ProviderConfig,
) {
    let _ = app.emit(
        "review:generation-started",
        ReviewGenStarted { deck_id, run_id },
    );

    // 读取该 memo 已有的卡片 angle
    let existing_angles = {
        let state = app.state::<AppState>();
        let store = state.store();
        store
            .with_conn(|c| {
                let cards = review::list_cards(c, deck_id)?;
                Ok::<_, memos_core::CoreError>(
                    cards
                        .iter()
                        .filter(|c| c.memo_uid == old_card.memo_uid)
                        .map(|c| c.angle.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
            })
            .unwrap_or_default()
    };

    let user_prompt = format!(
        "请为 memo（uid: {}）生成一张新的记忆卡片，从不同考核点出题。\n已存在的考核点：[{}]，请避免重复。\n使用 get_memo 工具读取该 memo 内容。只返回 JSON 数组（1 个元素）。",
        old_card.memo_uid, existing_angles
    );

    let messages = vec![json!({
        "role": "user",
        "content": user_prompt,
    })];

    let result = run_card_agent(&app, run_id, &provider, &messages);

    match result {
        Ok(content) => {
            let drafts = parse_card_json(&content);
            let state = app.state::<AppState>();
            let store = state.store();
            let mut inserted = 0;
            let mut errors = Vec::new();

            for draft in &drafts {
                match store.with_conn(|c| {
                    let now = chrono::Utc::now().timestamp();
                    let card = ReviewCard {
                        id: 0,
                        deck_id,
                        memo_uid: draft.memo_uid.clone(),
                        card_type: draft.card_type.clone(),
                        front: draft.front.clone(),
                        back: draft.back.clone(),
                        cloze_answer: draft.cloze_answer.clone(),
                        angle: draft.angle.clone().unwrap_or_default(),
                        stability: 0.0,
                        difficulty: 0.0,
                        due: now,
                        last_review: None,
                        reps: 0,
                        lapses: 0,
                        state: 0,
                        created_ts: now,
                        memo_deleted: false,
                    };
                    review::create_card(c, &card)
                }) {
                    Ok(_) => inserted += 1,
                    Err(e) => errors.push(format!("{e}")),
                }
            }

            let _ = app.emit(
                "review:cards-generated",
                ReviewGenDone {
                    deck_id,
                    run_id,
                    count: inserted,
                    errors,
                },
            );
        }
        Err(e) => {
            let _ = app.emit(
                "review:generation-error",
                ReviewGenError {
                    deck_id,
                    run_id,
                    error: e,
                },
            );
        }
    }
}

/// 运行卡片生成 agent loop，返回最终 assistant 内容
fn run_card_agent(
    app: &AppHandle,
    run_id: u32,
    provider: &ProviderConfig,
    messages: &[Value],
) -> Result<String, String> {
    let state = app.state::<AppState>();
    let mut msgs: Vec<Value> = messages.to_vec();
    let system_msg = json!({"role": "system", "content": CARD_GEN_SYSTEM_PROMPT});

    for _round in 0..5 {
        let mut req_messages = vec![system_msg.clone()];
        req_messages.extend(msgs.clone());

        let body = json!({
            "model": provider.model,
            "messages": req_messages,
            "stream": true,
            "tools": tool_definitions(),
        });

        let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
        let mut req = ureq::post(&url).set("Content-Type", "application/json");
        if !provider.api_key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", provider.api_key));
        }

        let response = req
            .send_string(&body.to_string())
            .map_err(|e| format!("HTTP 请求失败: {e}"))?;

        if response.status() >= 400 {
            let body_text = response.into_string().unwrap_or_default();
            return Err(format!("HTTP {}: {}", response.status(), body_text));
        }

        let reader = response.into_reader();
        let chunk_app = app.clone();
        let (content, tool_calls) = read_sse_stream(reader, |delta| {
            let _ = chunk_app.emit(
                "review:chunk",
                ReviewChunk {
                    run_id,
                    text: delta.to_string(),
                },
            );
        })
        .map_err(|e| format!("SSE 读取失败: {e}"))?;

        if tool_calls.is_empty() {
            return Ok(content);
        }

        // 执行工具调用
        let assistant_tool_calls: Vec<Value> = tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": { "name": tc.name, "arguments": tc.arguments },
                })
            })
            .collect();
        msgs.push(json!({
            "role": "assistant",
            "content": content,
            "tool_calls": assistant_tool_calls,
        }));

        let store = state.store();
        for tc in &tool_calls {
            let args: Value = serde_json::from_str(&tc.arguments).unwrap_or(Value::Null);
            let result = match execute_tool(&tc.name, &args, &store) {
                Ok(v) => v,
                Err(e) => json!({ "error": e.to_string() }),
            };
            msgs.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": result.to_string(),
            }));
        }
    }

    Err("超过最大工具调用轮次".into())
}

/// 从 AI 输出中解析卡片 JSON
fn parse_card_json(content: &str) -> Vec<CardDraft> {
    // 尝试直接解析
    if let Ok(drafts) = serde_json::from_str::<Vec<CardDraft>>(content) {
        return drafts;
    }
    // 尝试提取 JSON 数组片段
    if let Some(start) = content.find('[') {
        if let Some(end) = content.rfind(']') {
            if end > start {
                if let Ok(drafts) = serde_json::from_str::<Vec<CardDraft>>(&content[start..=end]) {
                    return drafts;
                }
            }
        }
    }
    Vec::new()
}
```

- [ ] **Step 2: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/review.rs
git commit -m "feat(review): add AI card generation commands (generate_cards + regenerate_card)"
```

---

## Task 10: 注册命令 + main.rs 集成

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 在 generate_handler! 注册 12 个 review 命令**

Modify `src-tauri/src/main.rs`, 在 `commands::lan::lan_copy_memo_to_local,` 之后、`]` 之前添加:

```rust
            // review
            commands::review::review_list_decks,
            commands::review::review_create_deck,
            commands::review::review_update_deck,
            commands::review::review_delete_deck,
            commands::review::review_list_cards,
            commands::review::review_list_due_cards,
            commands::review::review_delete_card,
            commands::review::review_score_card,
            commands::review::review_generate_cards,
            commands::review::review_regenerate_card,
            commands::review::review_deck_stats,
            commands::review::review_check_new_memos,
```

- [ ] **Step 2: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 编译成功

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/main.rs
git commit -m "feat(review): register 12 review Tauri commands in generate_handler"
```

---

## Task 11: 前端类型定义 + hooks

**Files:**
- Create: `src/components/Review/types.ts`
- Create: `src/components/Review/hooks.ts`

- [ ] **Step 1: 创建 types.ts**

Create `src/components/Review/types.ts`:

```typescript
/** 牌组（笔记集配置） */
export interface ReviewDeck {
  id: number;
  name: string;
  tags: string[];
  cards_per_memo: number;
  created_ts: number;
  last_reviewed_ts: number | null;
  memo_count: number;
}

/** 卡片 */
export interface ReviewCard {
  id: number;
  deck_id: number;
  memo_uid: string;
  card_type: "basic" | "reversed" | "cloze" | "concept" | "compare";
  front: string;
  back: string;
  cloze_answer: string | null;
  angle: string;
  stability: number;
  difficulty: number;
  due: number;
  last_review: number | null;
  reps: number;
  lapses: number;
  state: number; // 0=New 1=Learning 2=Review 3=Relearning
  created_ts: number;
  memo_deleted: boolean;
}

/** 复习记录 */
export interface ReviewRecord {
  id: number;
  card_id: number;
  rating: number; // 1=Again 2=Hard 3=Good 4=Easy
  reviewed_ts: number;
  elapsed_days: number;
  scheduled_days: number;
  state: number;
}

/** 牌组统计 */
export interface DeckStats {
  due_count: number;
  new_count: number;
  total: number;
  learned: number;
  retention_rate: number;
  last_reviewed_ts: number | null;
}

/** 评分结果 */
export interface ScoreResult {
  updated_card: ReviewCard;
  next_card: ReviewCard | null;
  session_stats: SessionStats;
}

/** 会话统计 */
export interface SessionStats {
  reviewed: number;
  again: number;
  hard: number;
  good: number;
  easy: number;
  retention_rate: number;
}

/** 评分等级 */
export type Rating = 1 | 2 | 3 | 4;

/** 卡片类型标签 */
export const CARD_TYPE_LABELS: Record<string, string> = {
  basic: "问答",
  reversed: "翻转",
  cloze: "填空",
  concept: "概念",
  compare: "对比",
};

/** 卡片状态标签 */
export const CARD_STATE_LABELS: Record<number, string> = {
  0: "新卡",
  1: "学习中",
  2: "复习中",
  3: "重学中",
};
```

- [ ] **Step 2: 创建 hooks.ts**

Create `src/components/Review/hooks.ts`:

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import type { DeckStats, ReviewCard, ReviewDeck, ScoreResult } from "./types";

/** 列出所有 deck */
export function useReviewDecks() {
  const [decks, setDecks] = useState<ReviewDeck[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<ReviewDeck[]>("review_list_decks");
      setDecks(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { decks, loading, error, refresh };
}

/** 获取 deck 统计 */
export function useDeckStats(deckId: number | null) {
  const [stats, setStats] = useState<DeckStats | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (deckId === null) return;
    setLoading(true);
    try {
      const result = await invoke<DeckStats>("review_deck_stats", { deckId });
      setStats(result);
    } catch {
      setStats(null);
    } finally {
      setLoading(false);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { stats, loading, refresh };
}

/** 获取到期卡片 */
export function useDueCards(deckId: number | null) {
  const [cards, setCards] = useState<ReviewCard[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (deckId === null) return;
    setLoading(true);
    try {
      const result = await invoke<ReviewCard[]>("review_list_due_cards", {
        deckId,
        limit: 100,
      });
      setCards(result);
    } catch {
      setCards([]);
    } finally {
      setLoading(false);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { cards, loading, refresh };
}

/** 列出 deck 所有卡片 */
export function useReviewCards(deckId: number | null) {
  const [cards, setCards] = useState<ReviewCard[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (deckId === null) return;
    setLoading(true);
    try {
      const result = await invoke<ReviewCard[]>("review_list_cards", { deckId });
      setCards(result);
    } catch {
      setCards([]);
    } finally {
      setLoading(false);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { cards, loading, refresh };
}

/** 生成卡片（监听事件流） */
export function useGenerateCards(deckId: number) {
  const [generating, setGenerating] = useState(false);
  const [progress, setProgress] = useState("");
  const [result, setResult] = useState<{ count: number; errors: string[] } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const generate = useCallback(async () => {
    setGenerating(true);
    setProgress("");
    setResult(null);
    setError(null);
    try {
      await invoke("review_generate_cards", { deckId });
    } catch (e) {
      setError(String(e));
      setGenerating(false);
    }
  }, [deckId]);

  useEffect(() => {
    if (!generating) return;
    let unlistenStarted: UnlistenFn | null = null;
    let unlistenDone: UnlistenFn | null = null;
    let unlistenError: UnlistenFn | null = null;
    let unlistenChunk: UnlistenFn | null = null;

    (async () => {
      unlistenStarted = await listen<{ deck_id: number; run_id: number }>(
        "review:generation-started",
        () => {},
      );
      unlistenChunk = await listen<{ run_id: number; text: string }>(
        "review:chunk",
        (e) => setProgress((prev) => prev + e.payload.text),
      );
      unlistenDone = await listen<{ deck_id: number; run_id: number; count: number; errors: string[] }>(
        "review:cards-generated",
        (e) => {
          if (e.payload.deck_id === deckId) {
            setResult({ count: e.payload.count, errors: e.payload.errors });
            setGenerating(false);
          }
        },
      );
      unlistenError = await listen<{ deck_id: number; run_id: number; error: string }>(
        "review:generation-error",
        (e) => {
          if (e.payload.deck_id === deckId) {
            setError(e.payload.error);
            setGenerating(false);
          }
        },
      );
    })();

    return () => {
      unlistenStarted?.();
      unlistenDone?.();
      unlistenError?.();
      unlistenChunk?.();
    };
  }, [generating, deckId]);

  return { generating, progress, result, error, generate };
}

/** 评分卡片 */
export function useScoreCard() {
  const [scoring, setScoring] = useState(false);

  const score = useCallback(
    async (cardId: number, rating: number, deckId: number): Promise<ScoreResult | null> => {
      setScoring(true);
      try {
        const result = await invoke<ScoreResult>("review_score_card", {
          cardId,
          rating,
          deckId,
        });
        return result;
      } catch (e) {
        console.error("评分失败:", e);
        return null;
      } finally {
        setScoring(false);
      }
    },
    [],
  );

  return { scoring, score };
}

/** 检查新 memo 数 */
export function useCheckNewMemos(deckId: number) {
  const [newCount, setNewCount] = useState(0);

  const refresh = useCallback(async () => {
    try {
      const count = await invoke<number>("review_check_new_memos", { deckId });
      setNewCount(count);
    } catch {
      setNewCount(0);
    }
  }, [deckId]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { newCount, refresh };
}
```

- [ ] **Step 3: 验证 tsc**

Run: `npx tsc --noEmit`
Expected: 无新错误（可能有预先存在的 markdown.ts 错误）

- [ ] **Step 4: Commit**

```bash
git add src/components/Review/types.ts src/components/Review/hooks.ts
git commit -m "feat(review): add TypeScript types and React hooks for review module"
```

---

## Task 12: 前端 DeckList + DeckEditor 组件

**Files:**
- Create: `src/components/Review/DeckList.tsx`
- Create: `src/components/Review/DeckEditor.tsx`
- Create: `src/components/Review/DeckStats.tsx`

- [ ] **Step 1: 创建 DeckStats.tsx（统计卡片）**

Create `src/components/Review/DeckStats.tsx`:

```tsx
import type { FC } from "react";
import type { DeckStats } from "./types";

interface Props {
  stats: DeckStats | null;
  loading?: boolean;
}

const DeckStatsView: FC<Props> = ({ stats, loading }) => {
  if (loading || !stats) {
    return (
      <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
        {[...Array(4)].map((_, i) => (
          <div key={i} className="h-20 rounded-lg bg-muted animate-pulse" />
        ))}
      </div>
    );
  }

  const items = [
    { label: "今日到期", value: stats.due_count, color: "text-orange-600" },
    { label: "新卡", value: stats.new_count, color: "text-blue-600" },
    { label: "总卡片", value: stats.total, color: "text-foreground" },
    {
      label: "掌握率",
      value: `${(stats.retention_rate * 100).toFixed(0)}%`,
      color: "text-green-600",
    },
  ];

  return (
    <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
      {items.map((item) => (
        <div key={item.label} className="rounded-lg border border-border p-3">
          <div className={`text-2xl font-bold ${item.color}`}>{item.value}</div>
          <div className="text-xs text-muted-foreground mt-1">{item.label}</div>
        </div>
      ))}
    </div>
  );
};

export default DeckStatsView;
```

- [ ] **Step 2: 创建 DeckEditor.tsx（新建/编辑 deck）**

Create `src/components/Review/DeckEditor.tsx`:

```tsx
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { X, Plus } from "lucide-react";
import { useState } from "react";
import { useTranslate } from "@/utils/i18n";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  initial?: { id: number; name: string; tags: string[]; cards_per_memo: number } | null;
  onSubmit: (data: { name: string; tags: string[]; cards_per_memo: number }) => void;
}

const DeckEditor = ({ open, onOpenChange, initial, onSubmit }: Props) => {
  const t = useTranslate();
  const [name, setName] = useState(initial?.name ?? "");
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [tagInput, setTagInput] = useState("");
  const [cardsPerMemo, setCardsPerMemo] = useState(initial?.cards_per_memo ?? 2);

  const addTag = () => {
    const trimmed = tagInput.trim().replace(/^#/, "");
    if (trimmed && !tags.includes(trimmed)) {
      setTags([...tags, trimmed]);
    }
    setTagInput("");
  };

  const removeTag = (tag: string) => {
    setTags(tags.filter((t) => t !== tag));
  };

  const handleSubmit = () => {
    if (!name.trim()) return;
    onSubmit({ name: name.trim(), tags, cards_per_memo: cardsPerMemo });
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{initial ? t("review.edit-deck") : t("review.create-deck")}</DialogTitle>
        </DialogHeader>
        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <Label>{t("review.deck-name")}</Label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t("review.deck-name-placeholder")}
            />
          </div>
          <div className="space-y-2">
            <Label>{t("review.deck-tags")}</Label>
            <div className="flex gap-2">
              <Input
                value={tagInput}
                onChange={(e) => setTagInput(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    addTag();
                  }
                }}
                placeholder={t("review.deck-tags-placeholder")}
              />
              <Button type="button" variant="outline" onClick={addTag}>
                <Plus className="size-4" />
              </Button>
            </div>
            {tags.length > 0 && (
              <div className="flex flex-wrap gap-2 mt-2">
                {tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-1 rounded-md bg-secondary px-2 py-1 text-sm"
                  >
                    #{tag}
                    <button onClick={() => removeTag(tag)} className="hover:text-destructive">
                      <X className="size-3" />
                    </button>
                  </span>
                ))}
              </div>
            )}
          </div>
          <div className="space-y-2">
            <Label>{t("review.cards-per-memo")}</Label>
            <Input
              type="number"
              min={1}
              max={10}
              value={cardsPerMemo}
              onChange={(e) => setCardsPerMemo(Number(e.target.value) || 1)}
            />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t("common.cancel")}
          </Button>
          <Button onClick={handleSubmit} disabled={!name.trim()}>
            {t("common.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default DeckEditor;
```

- [ ] **Step 3: 创建 DeckList.tsx（deck 卡片网格）**

Create `src/components/Review/DeckList.tsx`:

```tsx
import { Button } from "@/components/ui/button";
import { BookOpenIcon, PlusIcon, RefreshCwIcon, Trash2Icon, PlayIcon } from "lucide-react";
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useGenerateCards, useReviewDecks } from "./hooks";
import type { ReviewDeck } from "./types";
import { useTranslate } from "@/utils/i18n";
import DeckEditor from "./DeckEditor";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onSelectDeck: (deck: ReviewDeck) => void;
}

const DeckList = ({ onSelectDeck }: Props) => {
  const t = useTranslate();
  const navigate = useNavigate();
  const { decks, loading, refresh } = useReviewDecks();
  const [editorOpen, setEditorOpen] = useState(false);
  const [generatingDeckId, setGeneratingDeckId] = useState<number | null>(null);

  const { generating, progress, result, generate } = useGenerateCards(generatingDeckId ?? 0);

  const handleCreate = async (data: { name: string; tags: string[]; cards_per_memo: number }) => {
    await invoke("review_create_deck", data);
    setEditorOpen(false);
    refresh();
  };

  const handleDelete = async (deckId: number) => {
    if (!confirm(t("review.confirm-delete-deck"))) return;
    await invoke("review_delete_deck", { id: deckId });
    refresh();
  };

  const handleGenerate = async (deckId: number) => {
    setGeneratingDeckId(deckId);
    await generate();
  };

  if (loading) {
    return <div className="text-center text-muted-foreground py-8">{t("common.loading")}</div>;
  }

  if (decks.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-16 gap-4">
        <BookOpenIcon className="size-12 text-muted-foreground" />
        <p className="text-muted-foreground">{t("review.no-decks")}</p>
        <Button onClick={() => setEditorOpen(true)}>
          <PlusIcon className="size-4 mr-2" />
          {t("review.create-deck")}
        </Button>
        <DeckEditor open={editorOpen} onOpenChange={setEditorOpen} onSubmit={handleCreate} />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="flex justify-end">
        <Button onClick={() => setEditorOpen(true)} size="sm">
          <PlusIcon className="size-4 mr-2" />
          {t("review.create-deck")}
        </Button>
      </div>
      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
        {decks.map((deck) => (
          <div
            key={deck.id}
            className="rounded-lg border border-border p-4 flex flex-col gap-3 cursor-pointer hover:border-primary transition-colors"
            onClick={() => onSelectDeck(deck)}
          >
            <div className="flex items-start justify-between">
              <div>
                <h3 className="font-semibold text-foreground">{deck.name}</h3>
                <div className="flex flex-wrap gap-1 mt-1">
                  {deck.tags.map((tag) => (
                    <span key={tag} className="text-xs text-muted-foreground">
                      #{tag}
                    </span>
                  ))}
                </div>
              </div>
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  handleDelete(deck.id);
                }}
                className="text-muted-foreground hover:text-destructive"
              >
                <Trash2Icon className="size-4" />
              </button>
            </div>
            <div className="flex gap-2 mt-auto">
              <Button
                size="sm"
                variant="default"
                onClick={(e) => {
                  e.stopPropagation();
                  navigate(`/review/${deck.id}/study`);
                }}
              >
                <PlayIcon className="size-4 mr-1" />
                {t("review.start-review")}
              </Button>
              <Button
                size="sm"
                variant="outline"
                onClick={(e) => {
                  e.stopPropagation();
                  handleGenerate(deck.id);
                }}
                disabled={generating && generatingDeckId === deck.id}
              >
                <RefreshCwIcon className={`size-4 mr-1 ${generating && generatingDeckId === deck.id ? "animate-spin" : ""}`} />
                {t("review.generate-cards")}
              </Button>
            </div>
          </div>
        ))}
      </div>
      {generatingDeckId !== null && generating && (
        <div className="fixed bottom-4 right-4 max-w-md rounded-lg border border-border bg-background p-4 shadow-lg">
          <div className="text-sm font-medium mb-2">{t("review.generating")}</div>
          <div className="text-xs text-muted-foreground max-h-32 overflow-auto">{progress}</div>
        </div>
      )}
      {result && (
        <div className="fixed bottom-4 right-4 rounded-lg border border-green-500 bg-background p-4 shadow-lg">
          <div className="text-sm font-medium text-green-600">
            {t("review.generated", { count: result.count })}
          </div>
          {result.errors.length > 0 && (
            <div className="text-xs text-destructive mt-1">
              {result.errors.length} {t("review.errors")}
            </div>
          )}
        </div>
      )}
      <DeckEditor open={editorOpen} onOpenChange={setEditorOpen} onSubmit={handleCreate} />
    </div>
  );
};

export default DeckList;
```

- [ ] **Step 4: 验证 tsc**

Run: `npx tsc --noEmit`
Expected: 无新错误

- [ ] **Step 5: Commit**

```bash
git add src/components/Review/DeckList.tsx src/components/Review/DeckEditor.tsx src/components/Review/DeckStats.tsx
git commit -m "feat(review): add DeckList, DeckEditor, DeckStats components"
```

---

## Task 13: 前端 CardReview 复习界面

**Files:**
- Create: `src/components/Review/CardReview.tsx`

- [ ] **Step 1: 创建 CardReview.tsx（翻转卡 + 评分）**

Create `src/components/Review/CardReview.tsx`:

```tsx
import { Button } from "@/components/ui/button";
import { ArrowLeftIcon, RotateCcwIcon } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useScoreCard } from "./hooks";
import type { ReviewCard, SessionStats } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  deckId: number;
  onExit: () => void;
}

const CardReview = ({ deckId, onExit }: Props) => {
  const t = useTranslate();
  const [cards, setCards] = useState<ReviewCard[]>([]);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [flipped, setFlipped] = useState(false);
  const [sessionStats, setSessionStats] = useState<SessionStats | null>(null);
  const [finished, setFinished] = useState(false);
  const { scoring, score } = useScoreCard();

  const loadCards = useCallback(async () => {
    const result = await invoke<ReviewCard[]>("review_list_due_cards", {
      deckId,
      limit: 100,
    });
    setCards(result);
    if (result.length === 0) {
      setFinished(true);
    }
  }, [deckId]);

  useEffect(() => {
    loadCards();
  }, [loadCards]);

  const currentCard = cards[currentIndex];

  const handleScore = async (rating: number) => {
    if (!currentCard || scoring) return;
    const result = await score(currentCard.id, rating, deckId);
    if (result) {
      setSessionStats(result.session_stats);
      setFlipped(false);
      if (currentIndex + 1 >= cards.length) {
        setFinished(true);
      } else {
        setCurrentIndex(currentIndex + 1);
      }
    }
  };

  const handleRegenerate = async () => {
    if (!currentCard) return;
    await invoke("review_regenerate_card", { cardId: currentCard.id });
  };

  // 键盘快捷键
  useEffect(() => {
    if (finished || !currentCard) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === " ") {
        e.preventDefault();
        if (!flipped) setFlipped(true);
      } else if (flipped) {
        if (e.key === "1") handleScore(1);
        else if (e.key === "2") handleScore(2);
        else if (e.key === "3") handleScore(3);
        else if (e.key === "4") handleScore(4);
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [flipped, currentCard, finished]);

  if (finished) {
    return (
      <div className="flex flex-col items-center justify-center py-16 gap-6">
        <div className="text-2xl font-bold">{t("review.session-complete")}</div>
        {sessionStats && (
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-4">
            <div className="rounded-lg border p-4 text-center">
              <div className="text-2xl font-bold">{sessionStats.reviewed}</div>
              <div className="text-xs text-muted-foreground">{t("review.reviewed")}</div>
            </div>
            <div className="rounded-lg border p-4 text-center">
              <div className="text-2xl font-bold text-red-600">{sessionStats.again}</div>
              <div className="text-xs text-muted-foreground">{t("review.again")}</div>
            </div>
            <div className="rounded-lg border p-4 text-center">
              <div className="text-2xl font-bold text-green-600">
                {(sessionStats.retention_rate * 100).toFixed(0)}%
              </div>
              <div className="text-xs text-muted-foreground">{t("review.retention")}</div>
            </div>
          </div>
        )}
        <div className="flex gap-2">
          <Button variant="outline" onClick={onExit}>
            {t("review.back-to-decks")}
          </Button>
        </div>
      </div>
    );
  }

  if (!currentCard) {
    return <div className="text-center py-8 text-muted-foreground">{t("common.loading")}</div>;
  }

  return (
    <div className="flex flex-col gap-4">
      {/* 顶部：返回 + 进度 */}
      <div className="flex items-center justify-between">
        <Button variant="ghost" size="sm" onClick={onExit}>
          <ArrowLeftIcon className="size-4 mr-1" />
          {t("common.back")}
        </Button>
        <div className="text-sm text-muted-foreground">
          {currentIndex + 1} / {cards.length}
        </div>
      </div>

      {/* 卡片 */}
      <div
        className="mx-auto w-full max-w-2xl min-h-[300px] rounded-lg border-2 border-border p-8 flex flex-col items-center justify-center cursor-pointer"
        style={{ perspective: "1000px" }}
        onClick={() => !flipped && setFlipped(true)}
      >
        {!flipped ? (
          <>
            <div className="text-xs text-muted-foreground mb-4">
              {t("review.card-type")}: {currentCard.card_type}
            </div>
            <div className="text-lg text-center whitespace-pre-wrap">{currentCard.front}</div>
            <div className="mt-8 text-sm text-muted-foreground">{t("review.click-to-flip")}</div>
          </>
        ) : (
          <>
            <div className="text-xs text-muted-foreground mb-4">{t("review.answer")}</div>
            <div className="text-lg text-center whitespace-pre-wrap">{currentCard.back}</div>
          </>
        )}
      </div>

      {/* 评分按钮 */}
      {flipped && (
        <div className="flex justify-center gap-2">
          <Button variant="destructive" onClick={() => handleScore(1)} disabled={scoring}>
            {t("review.again")} (1)
          </Button>
          <Button variant="outline" onClick={() => handleScore(2)} disabled={scoring}>
            {t("review.hard")} (2)
          </Button>
          <Button variant="default" onClick={() => handleScore(3)} disabled={scoring}>
            {t("review.good")} (3)
          </Button>
          <Button variant="default" onClick={() => handleScore(4)} disabled={scoring}>
            {t("review.easy")} (4)
          </Button>
        </div>
      )}

      {/* 换角度 */}
      {flipped && (
        <div className="flex justify-center">
          <Button variant="ghost" size="sm" onClick={handleRegenerate}>
            <RotateCcwIcon className="size-4 mr-1" />
            {t("review.regenerate-angle")}
          </Button>
        </div>
      )}
    </div>
  );
};

export default CardReview;
```

- [ ] **Step 2: 验证 tsc**

Run: `npx tsc --noEmit`
Expected: 无新错误

- [ ] **Step 3: Commit**

```bash
git add src/components/Review/CardReview.tsx
git commit -m "feat(review): add CardReview component with flip card and FSRS rating"
```

---

## Task 14: 前端 DeckDetail + CardTable 组件

**Files:**
- Create: `src/components/Review/CardTable.tsx`
- Create: `src/components/Review/DeckDetail.tsx`
- Create: `src/components/Review/GenerationProgress.tsx`

- [ ] **Step 1: 创建 CardTable.tsx（卡片管理表格）**

Create `src/components/Review/CardTable.tsx`:

```tsx
import { Button } from "@/components/ui/button";
import { Trash2Icon } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import type { FC } from "react";
import { CARD_STATE_LABELS, CARD_TYPE_LABELS, type ReviewCard } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  cards: ReviewCard[];
  onRefresh: () => void;
}

const CardTable: FC<Props> = ({ cards, onRefresh }) => {
  const t = useTranslate();

  const handleDelete = async (cardId: number) => {
    if (!confirm(t("review.confirm-delete-card"))) return;
    await invoke("review_delete_card", { cardId });
    onRefresh();
  };

  const formatDate = (ts: number) => {
    return new Date(ts * 1000).toLocaleDateString();
  };

  if (cards.length === 0) {
    return (
      <div className="text-center py-8 text-muted-foreground">{t("review.no-cards")}</div>
    );
  }

  return (
    <div className="rounded-lg border border-border overflow-hidden">
      <table className="w-full text-sm">
        <thead className="bg-muted">
          <tr>
            <th className="text-left p-2">{t("review.front")}</th>
            <th className="text-left p-2">{t("review.card-type")}</th>
            <th className="text-left p-2">{t("review.angle")}</th>
            <th className="text-left p-2">{t("review.due")}</th>
            <th className="text-left p-2">{t("review.state")}</th>
            <th className="text-left p-2">{t("review.reps")}</th>
            <th className="p-2"></th>
          </tr>
        </thead>
        <tbody>
          {cards.map((card) => (
            <tr key={card.id} className="border-t border-border">
              <td className="p-2 max-w-xs truncate">{card.front}</td>
              <td className="p-2">{CARD_TYPE_LABELS[card.card_type] ?? card.card_type}</td>
              <td className="p-2">{card.angle || "-"}</td>
              <td className="p-2">{formatDate(card.due)}</td>
              <td className="p-2">{CARD_STATE_LABELS[card.state] ?? card.state}</td>
              <td className="p-2">{card.reps}</td>
              <td className="p-2">
                <button
                  onClick={() => handleDelete(card.id)}
                  className="text-muted-foreground hover:text-destructive"
                >
                  <Trash2Icon className="size-4" />
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};

export default CardTable;
```

- [ ] **Step 2: 创建 DeckDetail.tsx**

Create `src/components/Review/DeckDetail.tsx`:

```tsx
import { Button } from "@/components/ui/button";
import { ArrowLeftIcon, PlayIcon, RefreshCwIcon } from "lucide-react";
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import CardTable from "./CardTable";
import DeckStatsView from "./DeckStats";
import { useDeckStats, useGenerateCards, useReviewCards } from "./hooks";
import type { ReviewDeck } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  deck: ReviewDeck;
  onBack: () => void;
  onStartReview: () => void;
}

const DeckDetail = ({ deck, onBack, onStartReview }: Props) => {
  const t = useTranslate();
  const { stats, refresh: refreshStats } = useDeckStats(deck.id);
  const { cards, refresh: refreshCards } = useReviewCards(deck.id);
  const { generating, progress, result, generate } = useGenerateCards(deck.id);
  const [showProgress, setShowProgress] = useState(false);

  const handleGenerate = async () => {
    setShowProgress(true);
    await generate();
    refreshCards();
    refreshStats();
  };

  return (
    <div className="space-y-4">
      {/* 顶部 */}
      <div className="flex items-center justify-between">
        <Button variant="ghost" size="sm" onClick={onBack}>
          <ArrowLeftIcon className="size-4 mr-1" />
          {t("common.back")}
        </Button>
      </div>

      {/* Deck 信息 */}
      <div>
        <h1 className="text-2xl font-bold">{deck.name}</h1>
        <div className="flex flex-wrap gap-2 mt-1">
          {deck.tags.map((tag) => (
            <span key={tag} className="text-sm text-muted-foreground">
              #{tag}
            </span>
          ))}
        </div>
      </div>

      {/* 统计 */}
      <DeckStatsView stats={stats} />

      {/* 操作 */}
      <div className="flex gap-2">
        <Button onClick={onStartReview} disabled={stats?.due_count === 0}>
          <PlayIcon className="size-4 mr-1" />
          {t("review.start-review")}
        </Button>
        <Button variant="outline" onClick={handleGenerate} disabled={generating}>
          <RefreshCwIcon className={`size-4 mr-1 ${generating ? "animate-spin" : ""}`} />
          {t("review.generate-cards")}
        </Button>
      </div>

      {/* 生成进度 */}
      {showProgress && generating && (
        <div className="rounded-lg border border-border p-3">
          <div className="text-sm font-medium mb-2">{t("review.generating")}</div>
          <div className="text-xs text-muted-foreground max-h-32 overflow-auto whitespace-pre-wrap">
            {progress}
          </div>
        </div>
      )}
      {result && (
        <div className="rounded-lg border border-green-500 p-3">
          <div className="text-sm text-green-600">
            {t("review.generated", { count: result.count })}
          </div>
        </div>
      )}

      {/* 卡片列表 */}
      <div>
        <h2 className="text-lg font-semibold mb-2">{t("review.cards")}</h2>
        <CardTable cards={cards} onRefresh={refreshCards} />
      </div>
    </div>
  );
};

export default DeckDetail;
```

- [ ] **Step 3: 创建 GenerationProgress.tsx（可复用进度组件）**

Create `src/components/Review/GenerationProgress.tsx`:

```tsx
import type { FC } from "react";
import { useTranslate } from "@/utils/i18n";

interface Props {
  generating: boolean;
  progress: string;
  result: { count: number; errors: string[] } | null;
  error: string | null;
}

const GenerationProgress: FC<Props> = ({ generating, progress, result, error }) => {
  const t = useTranslate();

  if (!generating && !result && !error) return null;

  return (
    <div className="rounded-lg border border-border p-3 space-y-2">
      {generating && (
        <>
          <div className="text-sm font-medium">{t("review.generating")}</div>
          <div className="text-xs text-muted-foreground max-h-32 overflow-auto whitespace-pre-wrap">
            {progress}
          </div>
        </>
      )}
      {result && (
        <div className="text-sm text-green-600">
          {t("review.generated", { count: result.count })}
          {result.errors.length > 0 && (
            <span className="text-destructive ml-2">
              {result.errors.length} {t("review.errors")}
            </span>
          )}
        </div>
      )}
      {error && <div className="text-sm text-destructive">{error}</div>}
    </div>
  );
};

export default GenerationProgress;
```

- [ ] **Step 4: 验证 tsc**

Run: `npx tsc --noEmit`
Expected: 无新错误

- [ ] **Step 5: Commit**

```bash
git add src/components/Review/CardTable.tsx src/components/Review/DeckDetail.tsx src/components/Review/GenerationProgress.tsx
git commit -m "feat(review): add DeckDetail, CardTable, GenerationProgress components"
```

---

## Task 15: 前端页面 + 路由 + 导航 + index 导出

**Files:**
- Create: `src/components/Review/index.ts`
- Create: `src/pages/Review.tsx`
- Modify: `src/router/routes.ts`
- Modify: `src/router/index.tsx`
- Modify: `src/components/Navigation.tsx`

- [ ] **Step 1: 创建 index.ts 导出**

Create `src/components/Review/index.ts`:

```typescript
export { default as DeckList } from "./DeckList";
export { default as DeckEditor } from "./DeckEditor";
export { default as DeckDetail } from "./DeckDetail";
export { default as DeckStats } from "./DeckStats";
export { default as CardReview } from "./CardReview";
export { default as CardTable } from "./CardTable";
export { default as GenerationProgress } from "./GenerationProgress";
export * from "./types";
export * from "./hooks";
```

- [ ] **Step 2: 创建 Review.tsx 页面**

Create `src/pages/Review.tsx`:

```tsx
import { ArrowLeftIcon } from "lucide-react";
import { useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import CardReview from "@/components/Review/CardReview";
import DeckDetail from "@/components/Review/DeckDetail";
import DeckList from "@/components/Review/DeckList";
import { useReviewDecks } from "@/components/Review/hooks";
import type { ReviewDeck } from "@/components/Review/types";
import MobileHeader from "@/components/MobileHeader";
import { Button } from "@/components/ui/button";
import { useTranslate } from "@/utils/i18n";

const ReviewPage = () => {
  const t = useTranslate();
  const navigate = useNavigate();
  const params = useParams();
  const { decks, refresh } = useReviewDecks();
  const [selectedDeck, setSelectedDeck] = useState<ReviewDeck | null>(null);
  const [mode, setMode] = useState<"list" | "detail" | "study">("list");

  // URL 参数决定模式
  const deckIdParam = params.deckId;
  const isStudy = window.location.hash.includes("/study");

  // 如果 URL 有 deckId，找到对应 deck
  React.useEffect(() => {
    if (deckIdParam) {
      const deck = decks.find((d) => d.id === Number(deckIdParam));
      if (deck) {
        setSelectedDeck(deck);
        setMode(isStudy ? "study" : "detail");
      }
    } else {
      setMode("list");
      setSelectedDeck(null);
    }
  }, [deckIdParam, isStudy, decks]);

  const handleSelectDeck = (deck: ReviewDeck) => {
    setSelectedDeck(deck);
    setMode("detail");
    navigate(`/review/${deck.id}`);
  };

  const handleBackToList = () => {
    setMode("list");
    setSelectedDeck(null);
    navigate("/review");
  };

  const handleStartReview = () => {
    setMode("study");
    if (selectedDeck) {
      navigate(`/review/${selectedDeck.id}/study`);
    }
  };

  return (
    <section className="@container w-full min-h-full pb-10 sm:pt-3 md:pt-6">
      <MobileHeader />
      <div className="mx-auto w-full max-w-5xl px-4 sm:px-6">
        {mode === "list" && <DeckList onSelectDeck={handleSelectDeck} />}
        {mode === "detail" && selectedDeck && (
          <DeckDetail
            deck={selectedDeck}
            onBack={handleBackToList}
            onStartReview={handleStartReview}
          />
        )}
        {mode === "study" && selectedDeck && (
          <CardReview deckId={selectedDeck.id} onExit={() => navigate(`/review/${selectedDeck.id}`)} />
        )}
      </div>
    </section>
  );
};

export default ReviewPage;
```

注意：页面顶部需 `import React from "react";`（useEffect 需要）。

- [ ] **Step 3: 修改 routes.ts**

Modify `src/router/routes.ts`:

```typescript
export const ROUTES = {
  HOME: "/",
  ABOUT: "/about",
  ATTACHMENTS: "/attachments",
  ARCHIVED: "/archived",
  SETTING: "/setting",
  DISCOVER: "/discover",
  REVIEW: "/review",
} as const;
```

- [ ] **Step 4: 修改 router/index.tsx**

在 lazy import 区添加:

```typescript
const Review = lazyWithReload(() => import("@/pages/Review"));
```

在 routeConfig 的 `MainLayout` children 中添加（与 Home/About/Archived 同级）:

```tsx
              { path: Routes.REVIEW, element: <Review /> },
              { path: "review/:deckId", element: <Review /> },
              { path: "review/:deckId/study", element: <Review /> },
```

- [ ] **Step 5: 修改 Navigation.tsx 添加回顾导航项**

Modify `src/components/Navigation.tsx`, import 添加 `BookOpenIcon`:

```typescript
import { BookOpenIcon, CompassIcon, LibraryIcon, PaperclipIcon } from "lucide-react";
```

在 `primaryNavLinks` 数组中 `discoverNavLink` 之后添加:

```typescript
  const reviewNavLink: NavLinkItem = {
    id: "header-review",
    path: Routes.REVIEW,
    title: t("review.nav-title"),
    icon: <BookOpenIcon className="w-6 h-auto shrink-0" />,
  };

  // 本地单用户应用：主导航包含 home、attachments、discover 和 review
  const primaryNavLinks: NavLinkItem[] = [homeNavLink, attachmentsNavLink, discoverNavLink, reviewNavLink];
```

- [ ] **Step 6: 验证 tsc**

Run: `npx tsc --noEmit`
Expected: 无新错误

- [ ] **Step 7: Commit**

```bash
git add src/components/Review/index.ts src/pages/Review.tsx src/router/routes.ts src/router/index.tsx src/components/Navigation.tsx
git commit -m "feat(review): add Review page, routes, and navigation entry"
```

---

## Task 16: i18n 翻译 + 设置 section

**Files:**
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-Hans.json`
- Create: `src/components/Settings/ReviewSection.tsx`
- Modify: `src/components/Settings/settingSections.ts`

- [ ] **Step 1: 在 en.json 添加 review 翻译键**

在 `src/locales/en.json` 顶层添加 `review` 命名空间（与 `lan` 同级）:

```json
  "review": {
    "nav-title": "Review",
    "create-deck": "Create Deck",
    "edit-deck": "Edit Deck",
    "deck-name": "Deck Name",
    "deck-name-placeholder": "e.g. Rust Basics",
    "deck-tags": "Tags",
    "deck-tags-placeholder": "Type tag and press Enter",
    "cards-per-memo": "Cards per Memo",
    "no-decks": "No decks yet. Create one to start reviewing.",
    "confirm-delete-deck": "Delete this deck and all its cards?",
    "confirm-delete-card": "Delete this card?",
    "start-review": "Start Review",
    "generate-cards": "Generate Cards",
    "generating": "Generating...",
    "generated": "Generated {{count}} cards",
    "errors": "errors",
    "no-cards": "No cards in this deck yet.",
    "front": "Front",
    "card-type": "Type",
    "angle": "Angle",
    "due": "Due",
    "state": "State",
    "reps": "Reps",
    "cards": "Cards",
    "click-to-flip": "Click to flip",
    "answer": "Answer",
    "again": "Again",
    "hard": "Hard",
    "good": "Good",
    "easy": "Easy",
    "regenerate-angle": "New Angle",
    "session-complete": "Session Complete!",
    "reviewed": "Reviewed",
    "retention": "Retention",
    "back-to-decks": "Back to Decks"
  },
  "setting": {
    "review": {
      "label": "Review"
    }
  }
```

注意：`setting` 命名空间可能已存在，只需在其内添加 `review.label`。

- [ ] **Step 2: 在 zh-Hans.json 添加中文翻译**

在 `src/locales/zh-Hans.json` 顶层添加对应中文:

```json
  "review": {
    "nav-title": "回顾",
    "create-deck": "创建牌组",
    "edit-deck": "编辑牌组",
    "deck-name": "牌组名称",
    "deck-name-placeholder": "如：Rust 基础",
    "deck-tags": "标签",
    "deck-tags-placeholder": "输入标签后按回车",
    "cards-per-memo": "每条笔记卡片数",
    "no-decks": "还没有牌组。创建一个开始回顾吧。",
    "confirm-delete-deck": "删除该牌组及其所有卡片？",
    "confirm-delete-card": "删除该卡片？",
    "start-review": "开始复习",
    "generate-cards": "生成卡片",
    "generating": "生成中...",
    "generated": "生成了 {{count}} 张卡片",
    "errors": "个错误",
    "no-cards": "该牌组还没有卡片。",
    "front": "正面",
    "card-type": "类型",
    "angle": "考核点",
    "due": "到期",
    "state": "状态",
    "reps": "次数",
    "cards": "卡片",
    "click-to-flip": "点击翻面",
    "answer": "答案",
    "again": "忘了",
    "hard": "困难",
    "good": "记得",
    "easy": "简单",
    "regenerate-angle": "换角度",
    "session-complete": "复习完成！",
    "reviewed": "已复习",
    "retention": "掌握率",
    "back-to-decks": "返回牌组列表"
  },
  "setting": {
    "review": {
      "label": "回顾"
    }
  }
```

- [ ] **Step 3: 创建 ReviewSection.tsx**

Create `src/components/Settings/ReviewSection.tsx`:

```tsx
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { SettingGroup, SettingList, SettingListItem, SettingSection } from "./SettingSection";
import { useTranslate } from "@/utils/i18n";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

const ReviewSection = () => {
  const t = useTranslate();
  const [dailyLimit, setDailyLimit] = useState(20);
  const [cardsPerMemo, setCardsPerMemo] = useState(2);
  const [providerId, setProviderId] = useState("");
  const [providers, setProviders] = useState<{ id: string; name: string }[]>([]);

  useEffect(() => {
    // 加载配置
    invoke<string>("get_app_setting", { key: "review_config" })
      .then((json) => {
        if (json) {
          const config = JSON.parse(json);
          setDailyLimit(config.daily_new_card_limit ?? 20);
          setCardsPerMemo(config.default_cards_per_memo ?? 2);
          setProviderId(config.ai_provider_id ?? "");
        }
      })
      .catch(() => {});
    // 加载 providers
    invoke<{ id: string; name: string }[]>("list_providers")
      .then(setProviders)
      .catch(() => {});
  }, []);

  const saveConfig = async (key: string, value: unknown) => {
    const current = await invoke<string>("get_app_setting", { key: "review_config" }).catch(
      () => "{}",
    );
    const config = JSON.parse(current || "{}");
    config[key] = value;
    await invoke("upsert_app_setting", { key: "review_config", value: JSON.stringify(config) });
  };

  return (
    <SettingSection title={t("setting.review.label")} description={t("setting.review.label")}>
      <SettingGroup>
        <SettingList>
          <SettingListItem
            title={t("review.daily-new-card-limit")}
            description={t("review.daily-new-card-limit-desc")}
          >
            <Input
              type="number"
              min={0}
              max={200}
              value={dailyLimit}
              onChange={(e) => {
                const v = Number(e.target.value) || 0;
                setDailyLimit(v);
                saveConfig("daily_new_card_limit", v);
              }}
              className="w-24"
            />
          </SettingListItem>
          <SettingListItem
            title={t("review.default-cards-per-memo")}
            description={t("review.default-cards-per-memo-desc")}
          >
            <Input
              type="number"
              min={1}
              max={10}
              value={cardsPerMemo}
              onChange={(e) => {
                const v = Number(e.target.value) || 1;
                setCardsPerMemo(v);
                saveConfig("default_cards_per_memo", v);
              }}
              className="w-24"
            />
          </SettingListItem>
          <SettingListItem
            title={t("review.ai-provider")}
            description={t("review.ai-provider-desc")}
          >
            <select
              value={providerId}
              onChange={(e) => {
                setProviderId(e.target.value);
                saveConfig("ai_provider_id", e.target.value);
              }}
              className="w-48 rounded-md border border-border px-2 py-1"
            >
              <option value="">{t("review.use-default")}</option>
              {providers.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </SettingListItem>
        </SettingList>
      </SettingGroup>
    </SettingSection>
  );
};

export default ReviewSection;
```

- [ ] **Step 4: 在 settingSections.ts 注册 review section**

Modify `src/components/Settings/settingSections.ts`:

在 `SettingSectionKey` 联合类型中添加 `"review"`。

在 `SETTINGS_SECTIONS` 数组中添加（import `ReviewSection`）:

```typescript
import { ... BookOpenIcon } from "lucide-react";
import ReviewSection from "./ReviewSection";

// 数组中添加
{
  key: "review",
  scope: "basic",
  labelKey: "setting.review.label",
  icon: BookOpenIcon,
  component: ReviewSection,
},
```

- [ ] **Step 5: 补充 i18n 键**

在 en.json 和 zh-Hans.json 的 `review` 命名空间中补充设置页用到的键:

```json
    "daily-new-card-limit": "Daily New Card Limit",
    "daily-new-card-limit-desc": "Max new cards to learn per day (0=unlimited)",
    "default-cards-per-memo": "Default Cards per Memo",
    "default-cards-per-memo-desc": "Default card generation limit per memo",
    "ai-provider": "AI Provider",
    "ai-provider-desc": "Provider for card generation (empty=use default)",
    "use-default": "Use Default"
```

中文:

```json
    "daily-new-card-limit": "每日新卡上限",
    "daily-new-card-limit-desc": "每天最多学习的新卡数（0=不限制）",
    "default-cards-per-memo": "默认每条笔记卡片数",
    "default-cards-per-memo-desc": "生成卡片时每条笔记的默认上限",
    "ai-provider": "AI Provider",
    "ai-provider-desc": "用于生成卡片的 AI（空=使用默认）",
    "use-default": "使用默认"
```

- [ ] **Step 6: 验证 tsc**

Run: `npx tsc --noEmit`
Expected: 无新错误

- [ ] **Step 7: Commit**

```bash
git add src/locales/en.json src/locales/zh-Hans.json src/components/Settings/ReviewSection.tsx src/components/Settings/settingSections.ts
git commit -m "feat(review): add i18n translations and Settings section"
```

---

## Task 17: Memo 删除时标记卡片 + 最终集成验证

**Files:**
- Modify: `core/src/memo.rs`（在 delete 函数中标记卡片）
- Modify: `src-tauri/src/commands/memo.rs`（可选，若 delete 在命令层）

- [ ] **Step 1: 在 memo delete 中标记卡片**

检查 `core/src/memo.rs` 的 `delete` 函数，在删除 memo 前/后添加标记卡片的逻辑。

Modify `core/src/memo.rs` 的 `delete` 函数，在 `UPDATE attachment SET memo_id = NULL` 之后添加:

```rust
    // 标记关联的回顾卡片为 memo_deleted
    let _ = conn.execute(
        "UPDATE review_card SET memo_deleted = 1 WHERE memo_uid = ?1",
        params![&uid],
    );
```

注意：需要先找到 `delete` 函数的具体位置和参数（uid 或 id）。

- [ ] **Step 2: 验证编译**

Run: `cargo build -p memos-core`
Expected: 编译成功

- [ ] **Step 3: 运行所有测试**

Run: `cd src-tauri && cargo test`
Expected: 所有测试 PASS（含 review_core 的 10 个测试）

- [ ] **Step 4: 验证 tsc**

Run: `npx tsc --noEmit`
Expected: 无新错误

- [ ] **Step 5: 最终 cargo build**

Run: `cd src-tauri && cargo build`
Expected: 编译成功

- [ ] **Step 6: Commit**

```bash
git add core/src/memo.rs
git commit -m "feat(review): mark review cards as memo_deleted when memo is deleted"
```

---

## Self-Review

### Spec coverage 检查

| Spec 章节 | 覆盖任务 |
|---|---|
| §1 目标与范围 | Task 1-17 全覆盖 |
| §2 架构与模块划分 | Task 1（core）、Task 7-10（commands）、Task 11-16（前端） |
| §3 卡片类型设计 | Task 1（card_type 字段）、Task 9（AI prompt 含 5 种类型）、Task 11（types.ts） |
| §4 数据模型与 Schema | Task 1（V5 迁移 + 实体）、Task 2-3（CRUD）、Task 4（FSRS + 统计） |
| §5 AI 卡片生成流程 | Task 6（list_memos_by_tag 工具）、Task 9（generate_cards + regenerate + agent loop） |
| §6 复习流程与 FSRS 调度 | Task 4（score_card）、Task 8（review_score_card 命令）、Task 13（CardReview UI） |
| §7 前端界面与交互 | Task 11（hooks）、Task 12-15（组件+页面+路由）、Task 16（设置） |
| §8 命令接口 | Task 7-10（12 个命令全覆盖） |
| §9 依赖 | Task 1（core Cargo.toml）、Task 8（src-tauri Cargo.toml） |
| §10 测试策略 | Task 5（10 个 core 测试） |

### Placeholder scan
- 无 TBD/TODO/FIXME
- 所有代码块完整

### Type consistency
- `ReviewDeck` / `ReviewCard` / `ReviewRecord` / `DeckStats` 在 core、commands、types.ts 中字段名一致
- `score_card(card_id, rating, fsrs_params)` 签名在 Task 4 定义、Task 8 调用一致
- `list_memos_by_tag` 工具在 Task 6 定义 schema + 实现
- 事件名 `review:generation-started` / `review:chunk` / `review:cards-generated` / `review:generation-error` 在 Task 9 后端 emit、Task 11 hooks.ts listen 一致

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-07-11-review-module.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
