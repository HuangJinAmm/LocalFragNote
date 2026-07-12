-- 标签元数据表：存储标签名称、使用次数、时间戳
-- content 中的 #tag 仍是单一真相源，此表作为索引/缓存
CREATE TABLE IF NOT EXISTS tag (
    name TEXT PRIMARY KEY,
    count INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now')),
    updated_ts BIGINT NOT NULL DEFAULT (strftime('%s', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_tag_count ON tag(count DESC);
