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
