-- 初始 schema：单用户本地应用
-- 基于原 memos store schema，移除多用户字段与相关表

-- 笔记表（移除 creator_id）
CREATE TABLE IF NOT EXISTS memo (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uid TEXT NOT NULL UNIQUE,
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    row_status TEXT NOT NULL CHECK (row_status IN ('NORMAL', 'ARCHIVED')) DEFAULT 'NORMAL',
    content TEXT NOT NULL DEFAULT '',
    visibility TEXT NOT NULL CHECK (visibility IN ('PUBLIC', 'PROTECTED', 'PRIVATE')) DEFAULT 'PRIVATE',
    pinned INTEGER NOT NULL CHECK (pinned IN (0, 1)) DEFAULT 0,
    payload TEXT NOT NULL DEFAULT '{}'
);

-- 附件表（移除 creator_id，storage_type 固定 LOCAL）
CREATE TABLE IF NOT EXISTS attachment (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    uid TEXT NOT NULL UNIQUE,
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    filename TEXT NOT NULL DEFAULT '',
    blob BLOB DEFAULT NULL,
    type TEXT NOT NULL DEFAULT '',
    size INTEGER NOT NULL DEFAULT 0,
    memo_id INTEGER DEFAULT NULL,
    storage_type TEXT NOT NULL DEFAULT 'LOCAL',
    reference TEXT NOT NULL DEFAULT '',
    payload TEXT NOT NULL DEFAULT '{}'
);

-- 笔记关系表（无变化）
CREATE TABLE IF NOT EXISTS memo_relation (
    memo_id INTEGER NOT NULL,
    related_memo_id INTEGER NOT NULL,
    type TEXT NOT NULL,
    UNIQUE(memo_id, related_memo_id, type)
);

-- 反应表（移除 creator_id，UNIQUE 调整）
CREATE TABLE IF NOT EXISTS reaction (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    content_id TEXT NOT NULL,
    reaction_type TEXT NOT NULL,
    UNIQUE(content_id, reaction_type)
);

-- 应用设置表（原 user_setting 简化，移除 user_id）
CREATE TABLE IF NOT EXISTS app_setting (
    key TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL
);

-- 实例设置表（原 system_setting，无变化）
CREATE TABLE IF NOT EXISTS instance_setting (
    name TEXT NOT NULL UNIQUE,
    value TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT ''
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_attachment_memo_id ON attachment(memo_id);
CREATE INDEX IF NOT EXISTS idx_reaction_content_id ON reaction(content_id);
CREATE INDEX IF NOT EXISTS idx_memo_row_status ON memo(row_status);
CREATE INDEX IF NOT EXISTS idx_memo_created_ts ON memo(created_ts);
