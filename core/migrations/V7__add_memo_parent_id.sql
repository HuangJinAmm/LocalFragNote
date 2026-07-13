-- 笔记评论：添加 parent_id 列标识评论所属的父 memo
-- parent_id IS NULL = 主笔记；parent_id = N = 该父 memo 的评论
-- 评论不参与 FTS 全文搜索、不做 embedding、不提取标签

ALTER TABLE memo ADD COLUMN parent_id INTEGER DEFAULT NULL;
CREATE INDEX IF NOT EXISTS idx_memo_parent_id ON memo(parent_id);

-- 重建 FTS 触发器：评论（parent_id IS NOT NULL）不进 FTS 索引，不参与搜索
DROP TRIGGER IF EXISTS memo_ai;
DROP TRIGGER IF EXISTS memo_ad;
DROP TRIGGER IF EXISTS memo_au;

CREATE TRIGGER IF NOT EXISTS memo_ai AFTER INSERT ON memo WHEN NEW.parent_id IS NULL BEGIN
    INSERT INTO memo_fts(rowid, content, content_uid) VALUES (new.id, new.content, new.uid);
END;

CREATE TRIGGER IF NOT EXISTS memo_ad AFTER DELETE ON memo WHEN OLD.parent_id IS NULL BEGIN
    DELETE FROM memo_fts WHERE rowid = old.id;
END;

CREATE TRIGGER IF NOT EXISTS memo_au AFTER UPDATE ON memo WHEN NEW.parent_id IS NULL BEGIN
    DELETE FROM memo_fts WHERE rowid = old.id;
    INSERT INTO memo_fts(rowid, content, content_uid) VALUES (new.id, new.content, new.uid);
END;
