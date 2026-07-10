-- 添加 memo.location 列，存储为 JSON 字符串
-- JSON 格式：{"placeholder": "...", "latitude": 0.0, "longitude": 0.0}
-- NULL 表示无位置信息
ALTER TABLE memo ADD COLUMN location TEXT DEFAULT NULL;
