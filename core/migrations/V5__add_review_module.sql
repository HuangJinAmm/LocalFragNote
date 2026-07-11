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
