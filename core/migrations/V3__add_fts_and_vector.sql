-- 全文搜索（FTS5）+ 向量搜索（sqlite-vec vec0）

-- FTS5 全文搜索虚拟表
-- trigram 分词器：3 字符组索引，天然支持 CJK 子串匹配，无需中文分词库
CREATE VIRTUAL TABLE IF NOT EXISTS memo_fts USING fts5(
    content,
    content_uid UNINDEXED,
    tokenize = 'trigram'
);

-- 回填已有 NORMAL 状态数据到 FTS 表
INSERT INTO memo_fts(rowid, content, content_uid)
    SELECT id, content, uid FROM memo WHERE row_status = 'NORMAL';

-- 触发器：memo 增删改时自动同步 FTS 表
CREATE TRIGGER IF NOT EXISTS memo_ai AFTER INSERT ON memo BEGIN
    INSERT INTO memo_fts(rowid, content, content_uid) VALUES (new.id, new.content, new.uid);
END;

CREATE TRIGGER IF NOT EXISTS memo_ad AFTER DELETE ON memo BEGIN
    INSERT INTO memo_fts(memo_fts, rowid, content, content_uid) VALUES ('delete', old.id, old.content, old.uid);
END;

CREATE TRIGGER IF NOT EXISTS memo_au AFTER UPDATE ON memo BEGIN
    INSERT INTO memo_fts(memo_fts, rowid, content, content_uid) VALUES ('delete', old.id, old.content, old.uid);
    INSERT INTO memo_fts(rowid, content, content_uid) VALUES (new.id, new.content, new.uid);
END;

-- sqlite-vec 向量存储虚拟表
-- 384 维，对应 all-MiniLM-L6-v2 模型输出维度
-- 需先通过 sqlite3_auto_extension 注册 sqlite-vec 扩展，否则建表失败
CREATE VIRTUAL TABLE IF NOT EXISTS memo_vec USING vec0(
    embedding float[384]
);
