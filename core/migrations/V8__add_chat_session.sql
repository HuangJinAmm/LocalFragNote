-- AI 聊天会话与消息持久化
-- chat_session: 一个会话对应一次连续对话；可重命名、删除
-- chat_message: 会话内的消息（user/assistant/tool），保留完整 role/content/tool_calls 等 JSON 字段

CREATE TABLE IF NOT EXISTS chat_session (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    title           TEXT    NOT NULL,
    provider_id     TEXT,
    created_ts      INTEGER NOT NULL,
    updated_ts      INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_session_updated ON chat_session(updated_ts DESC);

CREATE TABLE IF NOT EXISTS chat_message (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      INTEGER NOT NULL,
    seq             INTEGER NOT NULL,
    role            TEXT    NOT NULL,                 -- user / assistant / tool
    content         TEXT    NOT NULL,                 -- JSON 字符串：string 或 ContentPart[]
    tool_calls      TEXT,                              -- JSON 字符串：assistant 的 tool_calls 数组（可空）
    tool_call_id    TEXT,                              -- tool 消息的关联 id（可空）
    tool_result     TEXT,                              -- JSON 字符串：tool 执行结果（可空）
    is_error        INTEGER NOT NULL DEFAULT 0,
    created_ts      INTEGER NOT NULL,
    FOREIGN KEY (session_id) REFERENCES chat_session(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_chat_message_session_seq ON chat_message(session_id, seq);
