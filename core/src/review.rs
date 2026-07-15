//! 回顾模块：牌组、卡片、复习记录 + FSRS 调度
//!
//! 卡片由 AI 生成（见 commands/review.rs），复习调度由 rs-fsrs 实现。

use crate::error::{CoreError, CoreResult};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use rs_fsrs::{Card as FsrsCard, FSRS, Parameters, Rating, State as FsrsState};
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
            stability: c.stability as f64,
            difficulty: c.difficulty as f64,
            due: DateTime::from_timestamp(c.due, 0).unwrap_or_else(Utc::now),
            last_review: c
                .last_review
                .and_then(|ts| DateTime::from_timestamp(ts, 0))
                .unwrap_or_else(Utc::now),
            reps: c.reps as i32,
            lapses: c.lapses as i32,
            state: match c.state {
                1 => FsrsState::Learning,
                2 => FsrsState::Review,
                3 => FsrsState::Relearning,
                _ => FsrsState::New,
            },
            ..Default::default()
        }
    }
}

/// State 枚举转 u8
fn state_to_u8(state: FsrsState) -> u8 {
    match state {
        FsrsState::New => 0,
        FsrsState::Learning => 1,
        FsrsState::Review => 2,
        FsrsState::Relearning => 3,
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
        let rows: Vec<i32> = stmt
            .query_map(params![id], |r| r.get::<_, i32>(0))?
            .filter_map(|r| r.ok())
            .collect();
        rows
    };
    for card_id in &card_ids {
        conn.execute("DELETE FROM review_record WHERE card_id = ?1", params![card_id])?;
    }
    conn.execute("DELETE FROM review_card WHERE deck_id = ?1", params![id])?;
    conn.execute("DELETE FROM review_deck WHERE id = ?1", params![id])?;
    Ok(())
}

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

// ==================== FSRS 调度 ====================

/// 评分：根据 rating 更新卡片调度，返回复习记录
///
/// rating: 1=Again 2=Hard 3=Good 4=Easy
/// fsrs_params: 空切片=默认参数，否则用自定义参数（权重数组，最多 19 个）
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
        let mut params = Parameters::default();
        for (i, &w) in fsrs_params.iter().enumerate().take(19) {
            params.w[i] = w as f64;
        }
        FSRS::new(params)
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
    let item = &record_log[&rating_enum];
    let new_card = &item.card;
    let log = &item.review_log;

    // 更新 card 字段（注意类型转换 f64→f32, i32→u32, State→u8）
    card.stability = new_card.stability as f32;
    card.difficulty = new_card.difficulty as f32;
    card.due = new_card.due.timestamp();
    card.last_review = Some(now.timestamp());
    card.reps = new_card.reps as u32;
    card.lapses = new_card.lapses as u32;
    card.state = state_to_u8(new_card.state);

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
        elapsed_days: log.elapsed_days as f32,
        scheduled_days: log.scheduled_days as f32,
        state: state_to_u8(log.state),
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
        |r| r.get::<_, f64>(0).map(|v| v as f32),
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

/// 全局到期卡片总数（跨所有 deck）
pub fn total_due_count(conn: &Connection) -> CoreResult<i32> {
    let now = Utc::now().timestamp();
    let count: i32 = conn.query_row(
        "SELECT COUNT(*) FROM review_card WHERE due <= ?1 AND memo_deleted = 0",
        params![now],
        |r| r.get(0),
    )?;
    Ok(count)
}

/// 所有复习记录的时间戳（用于热力图）
pub fn list_review_timestamps(conn: &Connection) -> CoreResult<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT reviewed_ts FROM review_record ORDER BY reviewed_ts ASC")?;
    let timestamps: Vec<i64> = stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .filter_map(|r| r.ok())
        .collect();
    Ok(timestamps)
}
