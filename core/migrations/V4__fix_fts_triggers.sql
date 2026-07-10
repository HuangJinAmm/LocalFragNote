-- 修复 FTS5 触发器：用 DELETE FROM 代替 FTS5 'delete' 命令
-- FTS5 'delete' 命令在 trigram 分词器下报 "SQL logic error"

DROP TRIGGER IF EXISTS memo_ad;
DROP TRIGGER IF EXISTS memo_au;

CREATE TRIGGER IF NOT EXISTS memo_ad AFTER DELETE ON memo BEGIN
    DELETE FROM memo_fts WHERE rowid = old.id;
END;

CREATE TRIGGER IF NOT EXISTS memo_au AFTER UPDATE ON memo BEGIN
    DELETE FROM memo_fts WHERE rowid = old.id;
    INSERT INTO memo_fts(rowid, content, content_uid) VALUES (new.id, new.content, new.uid);
END;
