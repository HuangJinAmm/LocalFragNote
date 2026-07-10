# AI 聊天面板与 Agent 后端 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在左下角实现浮动 AI 聊天面板，支持多 provider 配置，后端实现带工具调用的流式 AI agent。

**Architecture:** 前端 React 浮动面板 + Tauri events 流式推送；后端 Rust 用 `ureq` 调 OpenAI 兼容 `/v1/chat/completions` SSE 端点，agent 循环最多 5 轮工具调用。配置存 `app_setting`，工具直接调 `memos_core` 层。

**Tech Stack:** Rust + Tauri 2 + ureq（已有）；React 19 + Radix UI + Tailwind 4 + react-hot-toast（已有）

**Spec:** [docs/specs/2026-07-11-ai-chat-panel-design.md](file:///d:/3-ai-project/LocalFragNote/docs/specs/2026-07-11-ai-chat-panel-design.md)

---

## 文件结构

### 新增文件

| 文件 | 职责 |
|---|---|
| `src-tauri/src/ai/mod.rs` | ai 模块汇总 |
| `src-tauri/src/ai/provider.rs` | `ProviderConfig` 结构 + load/save |
| `src-tauri/src/ai/tools.rs` | 4 个工具的 schema 定义 + 执行分发 |
| `src-tauri/src/ai/sse.rs` | SSE 流式响应解析器 |
| `src-tauri/src/commands/ai_chat.rs` | `ai_chat` + `ai_abort` 命令 + agent loop |
| `src/components/AiChat/index.tsx` | 导出 |
| `src/components/AiChat/types.ts` | TS 类型定义 |
| `src/components/AiChat/AiChatPanel.tsx` | 浮动按钮 + 面板容器 |
| `src/components/AiChat/AiChatMessages.tsx` | 消息列表渲染 |
| `src/components/AiChat/AiChatComposer.tsx` | 输入框 + 发送 |
| `src/components/AiChat/AiChatProviderPicker.tsx` | provider 下拉选择 |
| `src/components/AiChat/AiChatSettings.tsx` | provider 配置弹窗 |
| `src/components/AiChat/hooks.ts` | `useAiChat` hook |

### 修改文件

| 文件 | 修改内容 |
|---|---|
| `src-tauri/src/commands/mod.rs` | 新增 `pub mod ai_chat;` |
| `src-tauri/src/main.rs` | 注册 `ai_chat`, `ai_abort` 命令 |
| `src/layouts/MainLayout.tsx` | 引入 `<AiChatPanel />` |
| `src/locales/en.json` | 新增 `aiChat` 命名空间文案 |
| `src/locales/zh-Hans.json` | 新增 `aiChat` 命名空间文案 |

---

## Task 1: Provider 配置模块（后端）

**Files:**
- Create: `src-tauri/src/ai/mod.rs`
- Create: `src-tauri/src/ai/provider.rs`
- Modify: `src-tauri/src/main.rs` (新增 `mod ai;`)

- [ ] **Step 1: 创建 ai 模块入口**

Create `src-tauri/src/ai/mod.rs`:

```rust
//! AI 相关模块：provider 配置、工具、SSE 解析

pub mod provider;
```

- [ ] **Step 2: 实现 ProviderConfig 与 load/save**

Create `src-tauri/src/ai/provider.rs`:

```rust
//! Provider 配置：存储在 app_setting 表，key = "ai_providers"

use memos_core::Store;
use serde::{Deserialize, Serialize};

/// OpenAI 兼容 provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// 唯一 ID（uuid 字符串）
    pub id: String,
    /// 显示名，如 "OpenAI" / "本地 Ollama"
    pub name: String,
    /// API base URL，如 "https://api.openai.com/v1"
    pub base_url: String,
    /// API key，Ollama 可为空字符串
    #[serde(default)]
    pub api_key: String,
    /// 模型名，如 "gpt-4o-mini"
    pub model: String,
}

const AI_PROVIDERS_KEY: &str = "ai_providers";

/// 从 app_setting 读取所有 provider 配置
pub fn load_providers(store: &Store) -> Vec<ProviderConfig> {
    let json: Option<String> = store
        .with_conn(|c| store.setting.app.get(c, AI_PROVIDERS_KEY))
        .unwrap_or(None);
    json.as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

/// 保存 provider 配置到 app_setting
pub fn save_providers(store: &Store, providers: &[ProviderConfig]) -> memos_core::CoreResult<()> {
    let json = serde_json::to_string(providers)
        .map_err(|e| memos_core::CoreError::Other(format!("序列化 provider 配置失败: {e}")))?;
    store.with_conn(|c| store.setting.app.upsert(c, AI_PROVIDERS_KEY, &json))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_config_serde_roundtrip() {
        let p = ProviderConfig {
            id: "abc-123".to_string(),
            name: "Test".to_string(),
            base_url: "https://example.com/v1".to_string(),
            api_key: "sk-xxx".to_string(),
            model: "gpt-4o-mini".to_string(),
        };
        let json = serde_json::to_string(&p).unwrap();
        let back: ProviderConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(p.id, back.id);
        assert_eq!(p.name, back.name);
        assert_eq!(p.base_url, back.base_url);
        assert_eq!(p.api_key, back.api_key);
        assert_eq!(p.model, back.model);
    }

    #[test]
    fn test_load_providers_empty() {
        let store = Store::open(":memory:").unwrap();
        let providers = load_providers(&store);
        assert!(providers.is_empty());
    }

    #[test]
    fn test_save_and_load_providers() {
        let store = Store::open(":memory:").unwrap();
        let providers = vec![ProviderConfig {
            id: "p1".to_string(),
            name: "OpenAI".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o-mini".to_string(),
        }];
        save_providers(&store, &providers).unwrap();
        let loaded = load_providers(&store);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "p1");
        assert_eq!(loaded[0].name, "OpenAI");
    }

    #[test]
    fn test_provider_config_ollama_empty_api_key() {
        // Ollama 无 api_key，serde default 处理缺失字段
        let json = r#"{"id":"o1","name":"Ollama","base_url":"http://localhost:11434/v1","model":"qwen2.5:7b"}"#;
        let p: ProviderConfig = serde_json::from_str(json).unwrap();
        assert_eq!(p.api_key, "");
    }
}
```

- [ ] **Step 3: 在 main.rs 注册 ai 模块**

Modify `src-tauri/src/main.rs` line 4-10, 在 `mod commands;` 后新增 `mod ai;`:

```rust
mod commands;
mod ai;
mod embedding;
mod error;
mod file_storage;
mod protocol;
mod state;
mod thumbnail;
```

- [ ] **Step 4: 运行测试验证**

Run: `cd src-tauri ; cargo test provider -- --nocapture`
Expected: 4 个测试全部 PASS

- [ ] **Step 5: 编译检查**

Run: `cd src-tauri ; cargo build`
Expected: 编译成功，无错误

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/ai/ src-tauri/src/main.rs
git commit -m "feat(ai): add provider config module with load/save"
```

---

## Task 2: 工具定义与执行模块（后端）

**Files:**
- Create: `src-tauri/src/ai/tools.rs`
- Modify: `src-tauri/src/ai/mod.rs`

- [ ] **Step 1: 实现工具定义与执行**

Create `src-tauri/src/ai/tools.rs`:

```rust
//! AI agent 工具：定义 OpenAI function-calling schema + 执行分发

use memos_core::markdown;
use memos_core::memo::{CreateMemo, FindMemo};
use memos_core::types::{RowStatus, Visibility};
use memos_core::Store;
use serde_json::{json, Value};

/// 返回 OpenAI function-calling 格式的工具定义
pub fn tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "type": "function",
            "function": {
                "name": "list_memos",
                "description": "搜索用户的笔记。支持全文搜索（FTS）和列出最近的笔记。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "全文搜索关键词，留空则返回最近笔记" },
                        "limit": { "type": "number", "description": "返回数量，默认 10，最大 50" }
                    }
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "get_memo",
                "description": "获取单条笔记的完整内容。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "uid": { "type": "string", "description": "笔记的唯一 ID" }
                    },
                    "required": ["uid"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "create_memo",
                "description": "创建一条新笔记。",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "content": { "type": "string", "description": "笔记内容，Markdown 格式" }
                    },
                    "required": ["content"]
                }
            }
        }),
        json!({
            "type": "function",
            "function": {
                "name": "list_tags",
                "description": "列出用户所有标签及其使用次数。",
                "parameters": { "type": "object", "properties": {} }
            }
        }),
    ]
}

/// 执行工具调用，返回结果 JSON
pub fn execute_tool(name: &str, args: &Value, store: &Store) -> memos_core::CoreResult<Value> {
    match name {
        "list_memos" => execute_list_memos(args, store),
        "get_memo" => execute_get_memo(args, store),
        "create_memo" => execute_create_memo(args, store),
        "list_tags" => execute_list_tags(store),
        _ => Err(memos_core::CoreError::Other(format!("未知工具: {name}"))),
    }
}

fn execute_list_memos(args: &Value, store: &Store) -> memos_core::CoreResult<Value> {
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32)
        .unwrap_or(10)
        .min(50)
        .max(1);

    let mut find = FindMemo {
        limit: Some(limit),
        row_status: Some(RowStatus::Normal),
        order_by_time_asc: false,
        ..Default::default()
    };
    if !query.is_empty() {
        // 短词 fallback 到 LIKE，长词用 FTS phrase
        let words: Vec<&str> = query.split_whitespace().filter(|w| !w.is_empty()).collect();
        let has_short = words.iter().any(|w| w.len() < 3);
        if has_short {
            find.content_contains = Some(query.to_string());
        } else {
            find.fts_query = Some(
                words
                    .iter()
                    .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
    }

    let memos = store.with_conn(|c| memos_core::memo::list(c, &find))?;
    let result: Vec<Value> = memos
        .iter()
        .map(|m| {
            json!({
                "uid": m.uid,
                "snippet": markdown::generate_snippet(&m.content, 200),
                "tags": markdown::extract_tags(&m.content),
                "created_ts": m.created_ts,
                "updated_ts": m.updated_ts,
            })
        })
        .collect();
    Ok(json!({ "memos": result }))
}

fn execute_get_memo(args: &Value, store: &Store) -> memos_core::CoreResult<Value> {
    let uid = args
        .get("uid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| memos_core::CoreError::Other("缺少 uid 参数".to_string()))?;

    let find = FindMemo {
        uid: Some(uid.to_string()),
        ..Default::default()
    };
    let memo = store.with_conn(|c| memos_core::memo::get(c, &find))?;
    match memo {
        Some(m) => Ok(json!({
            "uid": m.uid,
            "content": m.content,
            "tags": markdown::extract_tags(&m.content),
            "created_ts": m.created_ts,
            "updated_ts": m.updated_ts,
            "visibility": format!("{:?}", m.visibility),
            "pinned": m.pinned,
        })),
        None => Ok(json!({ "error": "未找到该笔记" })),
    }
}

fn execute_create_memo(args: &Value, store: &Store) -> memos_core::CoreResult<Value> {
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| memos_core::CoreError::Other("缺少 content 参数".to_string()))?;

    let uid = uuid_like();
    let create = CreateMemo {
        uid: uid.clone(),
        content: content.to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: serde_json::Value::Object(Default::default()),
        location: None,
    };
    let memo = store.with_conn(|c| memos_core::memo::create(c, &create))?;
    Ok(json!({
        "uid": memo.uid,
        "id": memo.id,
        "created_ts": memo.created_ts,
    }))
}

fn execute_list_tags(store: &Store) -> memos_core::CoreResult<Value> {
    let contents = store.with_conn(|c| -> memos_core::CoreResult<Vec<String>> {
        let mut stmt = c.prepare("SELECT content FROM memo WHERE row_status = 'NORMAL'")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    })?;

    let mut counts: std::collections::BTreeMap<String, i32> = std::collections::BTreeMap::new();
    for content in contents {
        for tag in markdown::extract_tags(&content) {
            *counts.entry(tag).or_insert(0) += 1;
        }
    }
    let tags: Vec<Value> = counts
        .into_iter()
        .map(|(tag, count)| json!({ "tag": tag, "count": count }))
        .collect();
    Ok(json!({ "tags": tags }))
}

/// 生成 16 字符 hex ID（与前端 uid 生成一致）
fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", now & 0xFFFF_FFFF_FFFF_FFFF)
}

#[cfg(test)]
mod tests {
    use super::*;
    use memos_core::memo::CreateMemo;
    use memos_core::types::Visibility;

    fn setup_store_with_memos() -> Store {
        let store = Store::open(":memory:").unwrap();
        for i in 0..3 {
            let create = CreateMemo {
                uid: format!("uid{i}"),
                content: format!("#rust 笔记 {i}：关于 Rust 所有权的内容"),
                visibility: Visibility::Private,
                pinned: false,
                payload: serde_json::Value::Object(Default::default()),
                location: None,
            };
            store
                .with_conn(|c| memos_core::memo::create(c, &create))
                .unwrap();
        }
        store
    }

    #[test]
    fn test_tool_definitions_count() {
        let defs = tool_definitions();
        assert_eq!(defs.len(), 4);
        let names: Vec<&str> = defs
            .iter()
            .map(|d| d["function"]["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"list_memos"));
        assert!(names.contains(&"get_memo"));
        assert!(names.contains(&"create_memo"));
        assert!(names.contains(&"list_tags"));
    }

    #[test]
    fn test_list_memos_all() {
        let store = setup_store_with_memos();
        let result = execute_list_memos(&json!({}), &store).unwrap();
        let memos = result["memos"].as_array().unwrap();
        assert_eq!(memos.len(), 3);
        assert!(memos[0]["snippet"].as_str().unwrap().contains("Rust"));
    }

    #[test]
    fn test_list_memos_with_fts_query() {
        let store = setup_store_with_memos();
        let result = execute_list_memos(&json!({"query": "Rust"}), &store).unwrap();
        let memos = result["memos"].as_array().unwrap();
        assert_eq!(memos.len(), 3);
    }

    #[test]
    fn test_get_memo_found() {
        let store = setup_store_with_memos();
        let result = execute_get_memo(&json!({"uid": "uid0"}), &store).unwrap();
        assert_eq!(result["uid"].as_str().unwrap(), "uid0");
        assert!(result["content"].as_str().unwrap().contains("Rust"));
        let tags = result["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].as_str().unwrap(), "rust");
    }

    #[test]
    fn test_get_memo_not_found() {
        let store = setup_store_with_memos();
        let result = execute_get_memo(&json!({"uid": "nonexistent"}), &store).unwrap();
        assert!(result.get("error").is_some());
    }

    #[test]
    fn test_create_memo() {
        let store = Store::open(":memory:").unwrap();
        let result = execute_create_memo(&json!({"content": "#test 新笔记"}), &store).unwrap();
        assert!(result["uid"].as_str().unwrap().len() > 0);
        assert!(result["id"].as_i64().unwrap() > 0);
    }

    #[test]
    fn test_create_memo_missing_content() {
        let store = Store::open(":memory:").unwrap();
        let result = execute_create_memo(&json!({}), &store);
        assert!(result.is_err());
    }

    #[test]
    fn test_list_tags() {
        let store = setup_store_with_memos();
        let result = execute_list_tags(&store).unwrap();
        let tags = result["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0]["tag"].as_str().unwrap(), "rust");
        assert_eq!(tags[0]["count"].as_i64().unwrap(), 3);
    }

    #[test]
    fn test_execute_tool_unknown() {
        let store = Store::open(":memory:").unwrap();
        let result = execute_tool("unknown_tool", &json!({}), &store);
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: 在 ai/mod.rs 注册 tools 模块**

Modify `src-tauri/src/ai/mod.rs`:

```rust
//! AI 相关模块：provider 配置、工具、SSE 解析

pub mod provider;
pub mod tools;
```

- [ ] **Step 3: 运行测试验证**

Run: `cd src-tauri ; cargo test tools -- --nocapture`
Expected: 8 个测试全部 PASS

- [ ] **Step 4: 编译检查**

Run: `cd src-tauri ; cargo build`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/tools.rs src-tauri/src/ai/mod.rs
git commit -m "feat(ai): add tool definitions and execution (list_memos, get_memo, create_memo, list_tags)"
```

---

## Task 3: SSE 流式解析器（后端）

**Files:**
- Create: `src-tauri/src/ai/sse.rs`
- Modify: `src-tauri/src/ai/mod.rs`

- [ ] **Step 1: 实现 SSE 解析器**

Create `src-tauri/src/ai/sse.rs`:

```rust
//! SSE 流式响应解析：解析 OpenAI chat/completions 的 stream 格式
//!
//! SSE 协议：每行 `data: {json}\n\n`，最后 `data: [DONE]\n\n`
//! tool_calls 分多块到达，需按 index 拼接

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};

/// 累积的 tool_call（OpenAI 流式协议中按 index 拼接）
#[derive(Debug, Clone, Default)]
pub struct ToolCallAccumulator {
    pub index: u32,
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// 单条 SSE 事件解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    /// 文本内容增量（可能为空）
    pub content_delta: Option<String>,
    /// tool_calls 增量（按 index）
    pub tool_call_delta: Option<ToolCallDelta>,
    /// finish_reason（流结束时出现）
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallDelta {
    pub index: u32,
    pub id: Option<String>,
    pub function_name: Option<String>,
    pub arguments_chunk: Option<String>,
}

/// 解析一行 SSE data，返回 SseEvent
/// 输入行应为 `data: {...}` 或 `data: [DONE]`
pub fn parse_sse_line(line: &str) -> Option<SseEvent> {
    let line = line.trim();
    if !line.starts_with("data:") {
        return None;
    }
    let data = line.trim_start_matches("data:").trim();
    if data == "[DONE]" {
        return Some(SseEvent {
            content_delta: None,
            tool_call_delta: None,
            finish_reason: Some("[DONE]".to_string()),
        });
    }

    let json: Value = serde_json::from_str(data).ok()?;
    let choice = json.get("choices")?.get(0)?;
    let delta = choice.get("delta")?;
    let finish_reason = choice
        .get("finish_reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let content_delta = delta
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let tool_call_delta = delta.get("tool_calls").and_then(|tcs| {
        let tc = tcs.get(0)?;
        let index = tc.get("index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let id = tc.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
        let function_name = tc
            .get("function")
            .and_then(|f| f.get("name"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let arguments_chunk = tc
            .get("function")
            .and_then(|f| f.get("arguments"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if id.is_none() && function_name.is_none() && arguments_chunk.is_none() {
            None
        } else {
            Some(ToolCallDelta {
                index,
                id,
                function_name,
                arguments_chunk,
            })
        }
    });

    Some(SseEvent {
        content_delta,
        tool_call_delta,
        finish_reason,
    })
}

/// 从 reader 读取完整 SSE 流，累积 tool_calls，返回 (完整文本, tool_calls)
/// 每读到 content_delta 时调用 on_chunk 回调（用于流式推送）
pub fn read_sse_stream<R: Read, F: FnMut(&str)>(
    reader: R,
    mut on_chunk: F,
) -> std::io::Result<(String, Vec<ToolCallAccumulator>)> {
    let buf_reader = BufReader::new(reader);
    let mut full_content = String::new();
    let mut tool_calls: Vec<ToolCallAccumulator> = Vec::new();

    for line_result in buf_reader.lines() {
        let line = line_result?;
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(event) = parse_sse_line(&line) {
            if let Some(delta) = &event.content_delta {
                full_content.push_str(delta);
                on_chunk(delta);
            }
            if let Some(tc_delta) = &event.tool_call_delta {
                let idx = tc_delta.index as usize;
                while tool_calls.len() <= idx {
                    tool_calls.push(ToolCallAccumulator::default());
                    tool_calls[idx].index = idx as u32;
                }
                if let Some(id) = &tc_delta.id {
                    tool_calls[idx].id = id.clone();
                }
                if let Some(name) = &tc_delta.function_name {
                    tool_calls[idx].name = name.clone();
                }
                if let Some(args) = &tc_delta.arguments_chunk {
                    tool_calls[idx].arguments.push_str(args);
                }
            }
            if let Some(fr) = &event.finish_reason {
                if fr == "[DONE]" {
                    break;
                }
            }
        }
    }

    Ok((full_content, tool_calls))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_sse_line_content() {
        let line = r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#;
        let event = parse_sse_line(line).unwrap();
        assert_eq!(event.content_delta.as_deref(), Some("hello"));
        assert!(event.tool_call_delta.is_none());
    }

    #[test]
    fn test_parse_sse_line_done() {
        let line = "data: [DONE]";
        let event = parse_sse_line(line).unwrap();
        assert_eq!(event.finish_reason.as_deref(), Some("[DONE]"));
    }

    #[test]
    fn test_parse_sse_line_non_data() {
        assert!(parse_sse_line(": comment").is_none());
        assert!(parse_sse_line("").is_none());
    }

    #[test]
    fn test_parse_sse_line_tool_call_start() {
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_abc","function":{"name":"list_memos","arguments":""}}]}}]}"#;
        let event = parse_sse_line(line).unwrap();
        let tc = event.tool_call_delta.unwrap();
        assert_eq!(tc.index, 0);
        assert_eq!(tc.id.as_deref(), Some("call_abc"));
        assert_eq!(tc.function_name.as_deref(), Some("list_memos"));
    }

    #[test]
    fn test_parse_sse_line_tool_call_args_chunk() {
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"quer"}}]}}]}"#;
        let event = parse_sse_line(line).unwrap();
        let tc = event.tool_call_delta.unwrap();
        assert_eq!(tc.arguments_chunk.as_deref(), Some("{\"quer"));
        assert!(tc.id.is_none());
    }

    #[test]
    fn test_read_sse_stream_simple_content() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\" there\"}}]}\n\ndata: [DONE]\n\n";
        let cursor = Cursor::new(input);
        let mut chunks = Vec::new();
        let (content, tool_calls) = read_sse_stream(cursor, |c| chunks.push(c.to_string())).unwrap();
        assert_eq!(content, "Hi there");
        assert_eq!(chunks, vec!["Hi", " there"]);
        assert!(tool_calls.is_empty());
    }

    #[test]
    fn test_read_sse_stream_tool_calls_accumulated() {
        let input = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"list_memos\",\"arguments\":\"\"}}]}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"query\\\":\\\"Rust\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n";
        let cursor = Cursor::new(input);
        let (content, tool_calls) = read_sse_stream(cursor, |_| {}).unwrap();
        assert_eq!(content, "");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].id, "call_1");
        assert_eq!(tool_calls[0].name, "list_memos");
        assert_eq!(tool_calls[0].arguments, r#"{"query":"Rust"}"#);
    }

    #[test]
    fn test_read_sse_stream_mixed_content_and_tool_calls() {
        let input = "data: {\"choices\":[{\"delta\":{\"content\":\"让我查一下\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"list_tags\",\"arguments\":\"{}\"}}]}}]}\n\ndata: [DONE]\n\n";
        let cursor = Cursor::new(input);
        let (content, tool_calls) = read_sse_stream(cursor, |_| {}).unwrap();
        assert_eq!(content, "让我查一下");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].name, "list_tags");
    }
}
```

- [ ] **Step 2: 在 ai/mod.rs 注册 sse 模块**

Modify `src-tauri/src/ai/mod.rs`:

```rust
//! AI 相关模块：provider 配置、工具、SSE 解析

pub mod provider;
pub mod sse;
pub mod tools;
```

- [ ] **Step 3: 运行测试验证**

Run: `cd src-tauri ; cargo test sse -- --nocapture`
Expected: 7 个测试全部 PASS

- [ ] **Step 4: 编译检查**

Run: `cd src-tauri ; cargo build`
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/ai/sse.rs src-tauri/src/ai/mod.rs
git commit -m "feat(ai): add SSE stream parser for OpenAI streaming responses"
```

---

## Task 4: Agent 循环与命令注册（后端）

**Files:**
- Create: `src-tauri/src/commands/ai_chat.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 实现 ai_chat 命令与 agent loop**

Create `src-tauri/src/commands/ai_chat.rs`:

```rust
//! AI 聊天命令：agent 循环 + 流式推送 + 中断机制

use crate::ai::provider::{load_providers, ProviderConfig};
use crate::ai::sse::read_sse_stream;
use crate::ai::tools::execute_tool;
use crate::ai::tools::tool_definitions;
use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use OnceLock from std::sync::OnceLock will be used via once_cell or direct static

/// 全局 run_id 计数器
static RUN_ID_COUNTER: AtomicU32 = AtomicU32::new(1);

/// 全局 abort 标记：run_id → abort flag
static ABORTS: std::sync::LazyLock<Mutex<HashMap<u32, Arc<AtomicBool>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn next_run_id() -> u32 {
    RUN_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

/// 前端传入的聊天消息
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// 助手消息的 tool_calls（OpenAI 格式）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Value>,
    /// tool 角色消息的 tool_call_id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// 流式 chunk 事件 payload
#[derive(Debug, Clone, Serialize)]
struct ChunkPayload {
    run_id: u32,
    text: String,
}

#[derive(Debug, Clone, Serialize)]
struct ToolPayload {
    run_id: u32,
    name: String,
    args: Value,
}

#[derive(Debug, Clone, Serialize)]
struct DonePayload {
    run_id: u32,
}

#[derive(Debug, Clone, Serialize)]
struct ErrorPayload {
    run_id: u32,
    message: String,
}

const SYSTEM_PROMPT: &str = "你是 LocalFragNote 的 AI 助手，帮助用户管理他们的笔记（memo）。
你可以通过工具搜索、读取、创建 memo，以及列出标签。
回答使用用户提问的语言。memo 内容是 Markdown 格式。
创建 memo 前不需要确认，直接创建并告知用户。";

const MAX_AGENT_ROUNDS: u32 = 5;

/// 启动 AI 聊天，立即返回 run_id，流式通过 events 推送
#[tauri::command]
pub async fn ai_chat(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    provider_id: String,
    messages: Vec<ChatMessage>,
) -> IpcResult<u32> {
    let run_id = next_run_id();
    let abort_flag = Arc::new(AtomicBool::new(false));
    ABORTS
        .lock()
        .unwrap()
        .insert(run_id, abort_flag.clone());

    // 提前检查 provider 是否存在，避免 spawn 后才发现错误
    let store = state.store();
    let providers = load_providers(&store);
    let provider = providers
        .iter()
        .find(|p| p.id == provider_id)
        .cloned()
        .ok_or_else(|| IpcError::BadRequest("Provider 不存在".to_string()))?;
    drop(store);

    let app_handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        agent_loop(app_handle, run_id, provider, messages, abort_flag);
    });

    Ok(run_id)
}

/// 中断指定 run
#[tauri::command]
pub fn ai_abort(run_id: u32) -> IpcResult<()> {
    if let Some(flag) = ABORTS.lock().unwrap().get(&run_id) {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

fn agent_loop(
    app: AppHandle,
    run_id: u32,
    provider: ProviderConfig,
    messages: Vec<ChatMessage>,
    abort_flag: Arc<AtomicBool>,
) {
    let state = app.state::<AppState>();
    let mut msgs: Vec<Value> = messages
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
        .collect();

    // 首轮注入 system prompt
    let system_msg = json!({"role": "system", "content": SYSTEM_PROMPT});

    for round in 0..MAX_AGENT_ROUNDS {
        if abort_flag.load(Ordering::SeqCst) {
            cleanup_abort(run_id);
            return;
        }

        // 构造请求 messages：system + 用户对话
        let mut req_messages = vec![system_msg.clone()];
        req_messages.extend(msgs.clone());

        let body = json!({
            "model": provider.model,
            "messages": req_messages,
            "stream": true,
            "tools": tool_definitions(),
        });

        let url = format!("{}/chat/completions", provider.base_url.trim_end_matches('/'));
        let mut req = ureq::post(&url).set("Content-Type", "application/json");
        if !provider.api_key.is_empty() {
            req = req.set("Authorization", &format!("Bearer {}", provider.api_key));
        }

        let response = match req.send_string(&body.to_string()) {
            Ok(r) => r,
            Err(e) => {
                let msg = format_http_error(&e);
                let _ = app.emit("ai:error", ErrorPayload { run_id, message: msg });
                cleanup_abort(run_id);
                return;
            }
        };

        let status = response.status();
        if status >= 400 {
            let body_text = response.into_string().unwrap_or_default();
            let message = format!("HTTP {status}: {body_text}");
            let _ = app.emit("ai:error", ErrorPayload { run_id, message });
            cleanup_abort(run_id);
            return;
        }

        // 读取 SSE 流
        let reader = response.into_reader();
        let chunk_app = app.clone();
        let (content, tool_calls) = match read_sse_stream(reader, |delta| {
            let _ = chunk_app.emit("ai:chunk", ChunkPayload {
                run_id,
                text: delta.to_string(),
            });
        }) {
            Ok(r) => r,
            Err(e) => {
                let _ = app.emit("ai:error", ErrorPayload {
                    run_id,
                    message: format!("SSE 读取失败: {e}"),
                });
                cleanup_abort(run_id);
                return;
            }
        };

        if abort_flag.load(Ordering::SeqCst) {
            cleanup_abort(run_id);
            return;
        }

        // 无工具调用：流式结束
        if tool_calls.is_empty() {
            let _ = app.emit("ai:done", DonePayload { run_id });
            cleanup_abort(run_id);
            return;
        }

        // 有工具调用：执行并继续循环
        // 构造 assistant 消息（含 tool_calls）
        let assistant_tool_calls: Vec<Value> = tool_calls
            .iter()
            .map(|tc| {
                json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {
                        "name": tc.name,
                        "arguments": tc.arguments,
                    }
                })
            })
            .collect();
        msgs.push(json!({
            "role": "assistant",
            "content": content,
            "tool_calls": assistant_tool_calls,
        }));

        // 执行每个工具调用
        let store = state.store();
        for tc in &tool_calls {
            let _ = app.emit("ai:tool", ToolPayload {
                run_id,
                name: tc.name.clone(),
                args: serde_json::from_str(&tc.arguments).unwrap_or(Value::Null),
            });

            let args: Value = serde_json::from_str(&tc.arguments).unwrap_or(Value::Null);
            let result = match execute_tool(&tc.name, &args, &store) {
                Ok(v) => v,
                Err(e) => json!({ "error": e.to_string() }),
            };
            msgs.push(json!({
                "role": "tool",
                "tool_call_id": tc.id,
                "content": result.to_string(),
            }));
        }
        drop(store);
    }

    // 超过最大轮次
    let _ = app.emit("ai:error", ErrorPayload {
        run_id,
        message: format!("超过最大工具调用轮次 ({MAX_AGENT_ROUNDS})"),
    });
    cleanup_abort(run_id);
}

fn cleanup_abort(run_id: u32) {
    ABORTS.lock().unwrap().remove(&run_id);
}

fn format_http_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            format!("HTTP {code}: {body}")
        }
        ureq::Error::Transport(t) => {
            format!("网络错误: {t}")
        }
    }
}
```

注意：上面的代码有一个语法问题，`OnceLock` 那行是注释笔误。删除那一行注释，实际代码用 `std::sync::LazyLock`（Rust 1.80+ 稳定）。确认项目 Rust 版本支持 `LazyLock`，否则改用 `once_cell::sync::Lazy`。

- [ ] **Step 2: 修复 LazyLock 导入**

修正 `src-tauri/src/commands/ai_chat.rs` 顶部，删除笔误行，确保使用：

```rust
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
```

确认 `LazyLock` 可用。若项目 Rust < 1.80，改用 `once_cell`。运行 `rustc --version` 确认。

- [ ] **Step 3: 在 commands/mod.rs 注册模块**

Modify `src-tauri/src/commands/mod.rs`:

```rust
//! IPC 命令模块汇总
//!
//! 每个子模块对应一个领域：memo、attachment、reaction、memo_relation、setting

pub mod ai_chat;
pub mod attachment;
pub mod memo;
pub mod memo_relation;
pub mod reaction;
pub mod setting;
```

- [ ] **Step 4: 在 main.rs 注册命令**

Modify `src-tauri/src/main.rs` 的 `invoke_handler` 部分，在 `commands::setting::update_storage_config,` 之后新增：

```rust
        .invoke_handler(tauri::generate_handler![
            ping,
            // memo
            commands::memo::create_memo,
            commands::memo::get_memo,
            commands::memo::list_memos,
            commands::memo::update_memo,
            commands::memo::delete_memo,
            commands::memo::render_memo_content,
            commands::memo::list_tags,
            commands::memo::list_memo_timestamps,
            commands::memo::embed_text,
            // attachment
            commands::attachment::create_attachment,
            commands::attachment::get_attachment,
            commands::attachment::list_attachments,
            commands::attachment::update_attachment,
            commands::attachment::delete_attachment,
            commands::attachment::get_attachment_thumbnail,
            // reaction
            commands::reaction::upsert_reaction,
            commands::reaction::list_reactions,
            commands::reaction::delete_reaction,
            // memo_relation
            commands::memo_relation::upsert_memo_relation,
            commands::memo_relation::list_memo_relations,
            commands::memo_relation::delete_memo_relation,
            // setting
            commands::setting::get_app_setting,
            commands::setting::upsert_app_setting,
            commands::setting::delete_app_setting,
            commands::setting::get_instance_setting,
            commands::setting::upsert_instance_setting,
            commands::setting::delete_instance_setting,
            commands::setting::get_instance_stats,
            commands::setting::get_storage_config,
            commands::setting::update_storage_config,
            // ai chat
            commands::ai_chat::ai_chat,
            commands::ai_chat::ai_abort,
        ])
```

- [ ] **Step 5: 编译检查**

Run: `cd src-tauri ; cargo build`
Expected: 编译成功。若 `LazyLock` 不可用，改用 `once_cell`（需在 Cargo.toml 加 `once_cell = "1"`）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/ai_chat.rs src-tauri/src/commands/mod.rs src-tauri/src/main.rs
git commit -m "feat(ai): add ai_chat command with agent loop and abort support"
```

---

## Task 5: Provider 管理 IPC 命令（后端）

**Files:**
- Modify: `src-tauri/src/commands/ai_chat.rs`

spec 中 provider 配置通过现有 `get_app_setting`/`upsert_app_setting` 读写。但为了让前端更方便，新增两个封装命令。

- [ ] **Step 1: 新增 list_providers 和 save_providers 命令**

在 `src-tauri/src/commands/ai_chat.rs` 末尾新增：

```rust
use crate::ai::provider::save_providers;

/// 列出所有已配置的 provider
#[tauri::command]
pub fn list_providers(state: tauri::State<'_, AppState>) -> IpcResult<Vec<ProviderConfig>> {
    let store = state.store();
    Ok(load_providers(&store))
}

/// 保存 provider 列表（全量替换）
#[tauri::command]
pub fn save_providers_cmd(
    state: tauri::State<'_, AppState>,
    providers: Vec<ProviderConfig>,
) -> IpcResult<Vec<ProviderConfig>> {
    let store = state.store();
    save_providers(&store, &providers)?;
    Ok(providers)
}
```

- [ ] **Step 2: 在 main.rs 注册新命令**

在 `commands::ai_chat::ai_abort,` 后新增：

```rust
            commands::ai_chat::ai_abort,
            commands::ai_chat::list_providers,
            commands::ai_chat::save_providers_cmd,
```

- [ ] **Step 3: 编译检查**

Run: `cd src-tauri ; cargo build`
Expected: 编译成功

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/ai_chat.rs src-tauri/src/main.rs
git commit -m "feat(ai): add list_providers and save_providers commands"
```

---

## Task 6: 前端类型定义与 i18n（前端）

**Files:**
- Create: `src/components/AiChat/types.ts`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-Hans.json`

- [ ] **Step 1: 创建类型定义**

Create `src/components/AiChat/types.ts`:

```typescript
/// AI Provider 配置
export interface ProviderConfig {
  id: string;
  name: string;
  base_url: string;
  api_key: string;
  model: string;
}

/// 聊天消息（前端状态）
export interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "tool";
  content: string;
  /** 助手消息流式中的标记 */
  streaming?: boolean;
  /** tool 消息的展示标记 */
  isToolCall?: boolean;
  /** 错误标记 */
  isError?: boolean;
}

/// 发送给后端的消息格式
export interface WireMessage {
  role: string;
  content: string;
  tool_calls?: unknown;
  tool_call_id?: string;
}

/// Tauri 事件 payload
export interface ChunkPayload {
  run_id: number;
  text: string;
}

export interface ToolPayload {
  run_id: number;
  name: string;
  args: unknown;
}

export interface DonePayload {
  run_id: number;
}

export interface ErrorPayload {
  run_id: number;
  message: string;
}

/// Provider 预设模板
export interface ProviderPreset {
  label: string;
  name: string;
  base_url: string;
  model: string;
}

export const PROVIDER_PRESETS: ProviderPreset[] = [
  { label: "OpenAI", name: "OpenAI", base_url: "https://api.openai.com/v1", model: "gpt-4o-mini" },
  { label: "DeepSeek", name: "DeepSeek", base_url: "https://api.deepseek.com/v1", model: "deepseek-chat" },
  { label: "Ollama", name: "本地 Ollama", base_url: "http://localhost:11434/v1", model: "qwen2.5:7b" },
];
```

- [ ] **Step 2: 新增 en.json aiChat 文案**

在 `src/locales/en.json` 顶层对象内（与 `"common"` 同级）新增 `"aiChat"` 命名空间：

```json
  "aiChat": {
    "title": "AI Assistant",
    "open": "Open AI chat",
    "close": "Close",
    "settings": "Settings",
    "provider": "Provider",
    "selectProvider": "Select provider",
    "configureFirst": "Please configure a provider first",
    "inputPlaceholder": "Type a message...",
    "send": "Send",
    "toolCall": "Tool call",
    "streaming": "Thinking...",
    "error": "Error",
    "settings": {
      "title": "AI Provider Settings",
      "add": "Add Provider",
      "edit": "Edit",
      "delete": "Delete",
      "name": "Name",
      "baseUrl": "Base URL",
      "apiKey": "API Key",
      "model": "Model",
      "presets": "Presets",
      "save": "Save",
      "cancel": "Cancel"
    }
  },
```

- [ ] **Step 3: 新增 zh-Hans.json aiChat 文案**

在 `src/locales/zh-Hans.json` 顶层对象内对应位置新增：

```json
  "aiChat": {
    "title": "AI 助手",
    "open": "打开 AI 聊天",
    "close": "关闭",
    "settings": "设置",
    "provider": "Provider",
    "selectProvider": "选择 Provider",
    "configureFirst": "请先配置 Provider",
    "inputPlaceholder": "输入消息...",
    "send": "发送",
    "toolCall": "调用工具",
    "streaming": "思考中...",
    "error": "错误",
    "settings": {
      "title": "AI Provider 设置",
      "add": "添加 Provider",
      "edit": "编辑",
      "delete": "删除",
      "name": "名称",
      "baseUrl": "Base URL",
      "apiKey": "API Key",
      "model": "模型",
      "presets": "预设模板",
      "save": "保存",
      "cancel": "取消"
    }
  },
```

- [ ] **Step 4: TypeScript 编译检查**

Run: `npx tsc --noEmit`
Expected: 无错误

- [ ] **Step 5: Commit**

```bash
git add src/components/AiChat/types.ts src/locales/en.json src/locales/zh-Hans.json
git commit -m "feat(ai): add TypeScript types and i18n strings for AI chat"
```

---

## Task 7: useAiChat Hook（前端）

**Files:**
- Create: `src/components/AiChat/hooks.ts`

- [ ] **Step 1: 实现 useAiChat hook**

Create `src/components/AiChat/hooks.ts`:

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
import toast from "react-hot-toast";
import type { ChatMessage, WireMessage } from "./types";

const MAX_MESSAGES_TO_SEND = 20;

interface UseAiChatOptions {
  providerId: string | null;
}

export function useAiChat({ providerId }: UseAiChatOptions) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [isStreaming, setIsStreaming] = useState(false);
  const currentRunId = useRef<number | null>(null);
  const unlistenersRef = useRef<UnlistenFn[]>([]);

  // 设置事件监听
  useEffect(() => {
    let mounted = true;
    const unlisteners: UnlistenFn[] = [];

    const setup = async () => {
      unlisteners.push(
        await listen<{ run_id: number; text: string }>("ai:chunk", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          setMessages((prev) => {
            const next = [...prev];
            for (let i = next.length - 1; i >= 0; i--) {
              if (next[i].role === "assistant" && next[i].streaming) {
                next[i] = { ...next[i], content: next[i].content + e.payload.text };
                break;
              }
            }
            return next;
          });
        }),
      );

      unlisteners.push(
        await listen<{ run_id: number; name: string; args: unknown }>("ai:tool", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          setMessages((prev) => [
            ...prev,
            {
              id: crypto.randomUUID(),
              role: "tool",
              content: `🔧 ${e.payload.name}(${JSON.stringify(e.payload.args)})`,
              isToolCall: true,
            },
          ]);
        }),
      );

      unlisteners.push(
        await listen<{ run_id: number }>("ai:done", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          setMessages((prev) =>
            prev.map((m, i) =>
              i === prev.length - 1 && m.role === "assistant"
                ? { ...m, streaming: false }
                : m,
            ),
          );
          setIsStreaming(false);
          currentRunId.current = null;
        }),
      );

      unlisteners.push(
        await listen<{ run_id: number; message: string }>("ai:error", (e) => {
          if (e.payload.run_id !== currentRunId.current) return;
          toast.error(e.payload.message);
          setMessages((prev) =>
            prev.map((m, i) =>
              i === prev.length - 1 && m.role === "assistant"
                ? { ...m, streaming: false, isError: true }
                : m,
            ),
          );
          setIsStreaming(false);
          currentRunId.current = null;
        }),
      );

      if (mounted) {
        unlistenersRef.current = unlisteners;
      } else {
        unlisteners.forEach((fn) => fn());
      }
    };

    setup();

    return () => {
      mounted = false;
      unlistenersRef.current.forEach((fn) => fn());
      unlistenersRef.current = [];
    };
  }, []);

  const send = useCallback(
    async (text: string) => {
      if (!providerId) {
        toast.error("请先选择 Provider");
        return;
      }
      if (isStreaming) return;

      const userMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "user",
        content: text,
      };
      const assistantMsg: ChatMessage = {
        id: crypto.randomUUID(),
        role: "assistant",
        content: "",
        streaming: true,
      };

      // 构造发送给后端的消息（截断到最近 20 条）
      const wireMessages: WireMessage[] = [...messages, userMsg]
        .filter((m) => m.role !== "tool" && !m.isToolCall)
        .slice(-MAX_MESSAGES_TO_SEND)
        .map((m) => ({ role: m.role, content: m.content }));

      setMessages((prev) => [...prev, userMsg, assistantMsg]);
      setIsStreaming(true);

      try {
        const runId = await invoke<number>("ai_chat", {
          providerId,
          messages: wireMessages,
        });
        currentRunId.current = runId;
      } catch (e) {
        toast.error(String(e));
        setMessages((prev) =>
          prev.map((m, i) =>
            i === prev.length - 1 && m.role === "assistant"
              ? { ...m, streaming: false, isError: true }
              : m,
          ),
        );
        setIsStreaming(false);
      }
    },
    [providerId, isStreaming, messages],
  );

  const abort = useCallback(async () => {
    if (currentRunId.current !== null) {
      await invoke("ai_abort", { runId: currentRunId.current });
      currentRunId.current = null;
      setIsStreaming(false);
      setMessages((prev) =>
        prev.map((m, i) =>
          i === prev.length - 1 && m.role === "assistant" && m.streaming
            ? { ...m, streaming: false, content: m.content + " [已中断]" }
            : m,
        ),
      );
    }
  }, []);

  const clear = useCallback(() => {
    if (isStreaming) return;
    setMessages([]);
  }, [isStreaming]);

  return { messages, isStreaming, send, abort, clear };
}
```

- [ ] **Step 2: TypeScript 编译检查**

Run: `npx tsc --noEmit`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add src/components/AiChat/hooks.ts
git commit -m "feat(ai): add useAiChat hook with streaming event listeners"
```

---

## Task 8: Provider 配置 UI（前端）

**Files:**
- Create: `src/components/AiChat/AiChatSettings.tsx`

- [ ] **Step 1: 实现 provider 配置弹窗**

Create `src/components/AiChat/AiChatSettings.tsx`:

```tsx
import { invoke } from "@tauri-apps/api/core";
import { PencilIcon, PlusIcon, TrashIcon } from "lucide-react";
import { useEffect, useState } from "react";
import toast from "react-hot-toast";
import { useTranslate } from "@/utils/i18n";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { PROVIDER_PRESETS, type ProviderConfig, type ProviderPreset } from "./types";

interface AiChatSettingsProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSaved: () => void;
}

export function AiChatSettings({ open, onOpenChange, onSaved }: AiChatSettingsProps) {
  const t = useTranslate();
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [editing, setEditing] = useState<ProviderConfig | null>(null);

  useEffect(() => {
    if (open) {
      invoke<ProviderConfig[]>("list_providers").then(setProviders).catch(toast.error);
    }
  }, [open]);

  const handleSave = async (provider: ProviderConfig) => {
    const existing = providers.findIndex((p) => p.id === provider.id);
    const next = existing >= 0
      ? providers.map((p) => (p.id === provider.id ? provider : p))
      : [...providers, provider];
    try {
      await invoke<ProviderConfig[]>("save_providers_cmd", { providers: next });
      setProviders(next);
      setEditing(null);
      onSaved();
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleDelete = async (id: string) => {
    const next = providers.filter((p) => p.id !== id);
    try {
      await invoke<ProviderConfig[]>("save_providers_cmd", { providers: next });
      setProviders(next);
      onSaved();
    } catch (e) {
      toast.error(String(e));
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent size="lg">
        <DialogHeader>
          <DialogTitle>{t("aiChat.settings.title")}</DialogTitle>
        </DialogHeader>

        {editing ? (
          <ProviderForm
            provider={editing}
            onSave={handleSave}
            onCancel={() => setEditing(null)}
          />
        ) : (
          <div className="flex flex-col gap-3">
            {providers.length === 0 && (
              <p className="text-sm text-muted-foreground">{t("aiChat.configureFirst")}</p>
            )}
            {providers.map((p) => (
              <div key={p.id} className="flex items-center justify-between rounded-md border p-3">
                <div className="min-w-0 flex-1">
                  <div className="font-medium">{p.name}</div>
                  <div className="truncate text-xs text-muted-foreground">
                    {p.base_url} · {p.model}
                  </div>
                </div>
                <div className="flex gap-1">
                  <Button size="icon" variant="ghost" onClick={() => setEditing(p)}>
                    <PencilIcon className="size-4" />
                  </Button>
                  <Button size="icon" variant="ghost" onClick={() => handleDelete(p.id)}>
                    <TrashIcon className="size-4" />
                  </Button>
                </div>
              </div>
            ))}
            <Button
              variant="outline"
              onClick={() =>
                setEditing({
                  id: crypto.randomUUID(),
                  name: "",
                  base_url: "",
                  api_key: "",
                  model: "",
                })
              }
            >
              <PlusIcon className="size-4 mr-1" />
              {t("aiChat.settings.add")}
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}

function ProviderForm({
  provider,
  onSave,
  onCancel,
}: {
  provider: ProviderConfig;
  onSave: (p: ProviderConfig) => void;
  onCancel: () => void;
}) {
  const t = useTranslate();
  const [form, setForm] = useState<ProviderConfig>(provider);

  const applyPreset = (preset: ProviderPreset) => {
    setForm({
      ...form,
      name: preset.name,
      base_url: preset.base_url,
      model: preset.model,
    });
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap gap-2">
        {PROVIDER_PRESETS.map((preset) => (
          <Button key={preset.label} size="sm" variant="outline" onClick={() => applyPreset(preset)}>
            {preset.label}
          </Button>
        ))}
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.settings.name")}</Label>
        <Input
          value={form.name}
          onChange={(e) => setForm({ ...form, name: e.target.value })}
          placeholder="OpenAI"
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.settings.baseUrl")}</Label>
        <Input
          value={form.base_url}
          onChange={(e) => setForm({ ...form, base_url: e.target.value })}
          placeholder="https://api.openai.com/v1"
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.settings.apiKey")}</Label>
        <Input
          type="password"
          value={form.api_key}
          onChange={(e) => setForm({ ...form, api_key: e.target.value })}
          placeholder="sk-..."
        />
      </div>
      <div className="flex flex-col gap-2">
        <Label>{t("aiChat.settings.model")}</Label>
        <Input
          value={form.model}
          onChange={(e) => setForm({ ...form, model: e.target.value })}
          placeholder="gpt-4o-mini"
        />
      </div>
      <div className="flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          {t("aiChat.settings.cancel")}
        </Button>
        <Button onClick={() => onSave(form)} disabled={!form.name || !form.base_url || !form.model}>
          {t("aiChat.settings.save")}
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: TypeScript 编译检查**

Run: `npx tsc --noEmit`
Expected: 无错误（若 `Label`/`Input` 路径不对，检查 `src/components/ui/` 下是否有 `label.tsx`/`input.tsx`）

- [ ] **Step 3: Commit**

```bash
git add src/components/AiChat/AiChatSettings.tsx
git commit -m "feat(ai): add provider settings dialog with presets"
```

---

## Task 9: Provider 选择器（前端）

**Files:**
- Create: `src/components/AiChat/AiChatProviderPicker.tsx`

- [ ] **Step 1: 实现 provider 下拉选择器**

Create `src/components/AiChat/AiChatProviderPicker.tsx`:

```tsx
import { invoke } from "@tauri-apps/api/core";
import { SettingsIcon } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslate } from "@/utils/i18n";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import type { ProviderConfig } from "./types";

const STORAGE_KEY = "ai_chat.active_provider";

interface AiChatProviderPickerProps {
  onOpenSettings: () => void;
  onProviderChange: (id: string | null) => void;
}

export function AiChatProviderPicker({ onOpenSettings, onProviderChange }: AiChatProviderPickerProps) {
  const t = useTranslate();
  const [providers, setProviders] = useState<ProviderConfig[]>([]);
  const [selected, setSelected] = useState<string | null>(null);

  useEffect(() => {
    invoke<ProviderConfig[]>("list_providers")
      .then((list) => {
        setProviders(list);
        // 从 localStorage 恢复选择，若不存在则选第一个
        const saved = localStorage.getItem(STORAGE_KEY);
        if (saved && list.some((p) => p.id === saved)) {
          setSelected(saved);
          onProviderChange(saved);
        } else if (list.length > 0) {
          setSelected(list[0].id);
          onProviderChange(list[0].id);
        } else {
          setSelected(null);
          onProviderChange(null);
        }
      })
      .catch(() => {
        setSelected(null);
        onProviderChange(null);
      });
  }, [onProviderChange]);

  const handleChange = (value: string) => {
    if (value === "__settings__") {
      onOpenSettings();
      return;
    }
    setSelected(value);
    localStorage.setItem(STORAGE_KEY, value);
    onProviderChange(value);
  };

  return (
    <Select value={selected ?? undefined} onValueChange={handleChange}>
      <SelectTrigger size="sm" className="min-w-[120px]">
        <SelectValue placeholder={t("aiChat.selectProvider")} />
      </SelectTrigger>
      <SelectContent>
        {providers.length === 0 && (
          <SelectItem value="__empty__" disabled>
            {t("aiChat.configureFirst")}
          </SelectItem>
        )}
        {providers.map((p) => (
          <SelectItem key={p.id} value={p.id}>
            {p.name}
          </SelectItem>
        ))}
        <SelectItem value="__settings__">
          <SettingsIcon className="size-3.5" />
          {t("aiChat.settings")}
        </SelectItem>
      </SelectContent>
    </Select>
  );
}
```

- [ ] **Step 2: TypeScript 编译检查**

Run: `npx tsc --noEmit`
Expected: 无错误

- [ ] **Step 3: Commit**

```bash
git add src/components/AiChat/AiChatProviderPicker.tsx
git commit -m "feat(ai): add provider picker with localStorage persistence"
```

---

## Task 10: 消息列表与输入框（前端）

**Files:**
- Create: `src/components/AiChat/AiChatMessages.tsx`
- Create: `src/components/AiChat/AiChatComposer.tsx`

- [ ] **Step 1: 实现消息列表**

Create `src/components/AiChat/AiChatMessages.tsx`:

```tsx
import { BotIcon, UserIcon } from "lucide-react";
import { useEffect, useRef } from "react";
import { MemoMarkdownRenderer } from "@/components/MemoContent/MemoMarkdownRenderer";
import { cn } from "@/lib/utils";
import type { ChatMessage } from "./types";

interface AiChatMessagesProps {
  messages: ChatMessage[];
}

export function AiChatMessages({ messages }: AiChatMessagesProps) {
  const scrollRef = useRef<HTMLDivElement>(null);

  // 自动滚动到底部
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages]);

  if (messages.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground">
        <BotIcon className="size-8" />
        <p className="text-sm">有什么可以帮你的？</p>
      </div>
    );
  }

  return (
    <div ref={scrollRef} className="flex-1 overflow-y-auto px-3 py-2 space-y-3">
      {messages.map((msg) => {
        if (msg.role === "tool") {
          return (
            <div key={msg.id} className="text-xs text-muted-foreground px-2 py-1 rounded bg-muted/50">
              {msg.content}
            </div>
          );
        }
        const isUser = msg.role === "user";
        return (
          <div
            key={msg.id}
            className={cn("flex gap-2", isUser ? "flex-row-reverse" : "flex-row")}
          >
            <div className="shrink-0 mt-0.5">
              {isUser ? (
                <UserIcon className="size-5 text-muted-foreground" />
              ) : (
                <BotIcon className="size-5 text-primary" />
              )}
            </div>
            <div
              className={cn(
                "max-w-[85%] rounded-lg px-3 py-2 text-sm",
                isUser
                  ? "bg-primary text-primary-foreground"
                  : msg.isError
                    ? "bg-destructive/10 text-destructive"
                    : "bg-muted",
              )}
            >
              {isUser ? (
                <p className="whitespace-pre-wrap break-words">{msg.content}</p>
              ) : msg.content ? (
                <div className="break-words">
                  <MemoMarkdownRenderer
                    content={msg.content}
                    resolvedMentionUsernames={new Set()}
                  />
                  {msg.streaming && (
                    <span className="inline-block w-1 h-4 ml-0.5 bg-current animate-pulse" />
                  )}
                </div>
              ) : msg.streaming ? (
                <span className="text-muted-foreground text-xs">思考中...</span>
              ) : null}
            </div>
          </div>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 2: 实现输入框**

Create `src/components/AiChat/AiChatComposer.tsx`:

```tsx
import { SendIcon, SquareIcon } from "lucide-react";
import { useState } from "react";
import { useTranslate } from "@/utils/i18n";
import { cn } from "@/lib/utils";

interface AiChatComposerProps {
  isStreaming: boolean;
  disabled: boolean;
  onSend: (text: string) => void;
  onAbort: () => void;
}

export function AiChatComposer({ isStreaming, disabled, onSend, onAbort }: AiChatComposerProps) {
  const t = useTranslate();
  const [text, setText] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = text.trim();
    if (!trimmed || isStreaming || disabled) return;
    onSend(trimmed);
    setText("");
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送，Shift+Enter 换行
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e as unknown as React.FormEvent);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="border-t border-border p-2 flex gap-2 items-end">
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={disabled ? t("aiChat.configureFirst") : t("aiChat.inputPlaceholder")}
        disabled={disabled}
        rows={1}
        className={cn(
          "flex-1 resize-none rounded-md border border-border bg-background px-3 py-2 text-sm",
          "max-h-32 min-h-[36px] focus:outline-none focus:ring-1 focus:ring-primary",
          "disabled:cursor-not-allowed disabled:opacity-50",
        )}
        style={{ height: "auto" }}
      />
      {isStreaming ? (
        <button
          type="button"
          onClick={onAbort}
          className="shrink-0 size-9 rounded-md border border-border flex items-center justify-center hover:bg-muted"
          aria-label="Stop"
        >
          <SquareIcon className="size-4" />
        </button>
      ) : (
        <button
          type="submit"
          disabled={!text.trim() || disabled}
          className="shrink-0 size-9 rounded-md bg-primary text-primary-foreground flex items-center justify-center disabled:opacity-50 disabled:cursor-not-allowed hover:opacity-90"
          aria-label={t("aiChat.send")}
        >
          <SendIcon className="size-4" />
        </button>
      )}
    </form>
  );
}
```

- [ ] **Step 3: TypeScript 编译检查**

Run: `npx tsc --noEmit`
Expected: 无错误

- [ ] **Step 4: Commit**

```bash
git add src/components/AiChat/AiChatMessages.tsx src/components/AiChat/AiChatComposer.tsx
git commit -m "feat(ai): add message list with markdown rendering and composer input"
```

---

## Task 11: AiChatPanel 容器与集成（前端）

**Files:**
- Create: `src/components/AiChat/AiChatPanel.tsx`
- Create: `src/components/AiChat/index.tsx`
- Modify: `src/layouts/MainLayout.tsx`

- [ ] **Step 1: 实现 AiChatPanel 容器**

Create `src/components/AiChat/AiChatPanel.tsx`:

```tsx
import { BotIcon, SettingsIcon, XIcon } from "lucide-react";
import { useState } from "react";
import { useTranslate } from "@/utils/i18n";
import { cn } from "@/lib/utils";
import { AiChatComposer } from "./AiChatComposer";
import { AiChatMessages } from "./AiChatMessages";
import { AiChatProviderPicker } from "./AiChatProviderPicker";
import { AiChatSettings } from "./AiChatSettings";
import { useAiChat } from "./hooks";

export function AiChatPanel() {
  const t = useTranslate();
  const [open, setOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [providerId, setProviderId] = useState<string | null>(null);
  const { messages, isStreaming, send, abort } = useAiChat({ providerId });

  return (
    <>
      <div className="fixed left-4 bottom-4 z-50">
        {open ? (
          <div className="flex flex-col w-[400px] h-[560px] rounded-xl border border-border bg-popover shadow-lg overflow-hidden">
            {/* Header */}
            <div className="flex items-center gap-2 border-b border-border px-3 py-2">
              <BotIcon className="size-4 text-primary" />
              <span className="font-medium text-sm flex-1">{t("aiChat.title")}</span>
              <AiChatProviderPicker
                onOpenSettings={() => setSettingsOpen(true)}
                onProviderChange={setProviderId}
              />
              <button
                onClick={() => setSettingsOpen(true)}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
                aria-label={t("aiChat.settings")}
              >
                <SettingsIcon className="size-3.5" />
              </button>
              <button
                onClick={() => setOpen(false)}
                className="size-7 rounded-md hover:bg-muted flex items-center justify-center"
                aria-label={t("aiChat.close")}
              >
                <XIcon className="size-3.5" />
              </button>
            </div>

            {/* Messages */}
            <AiChatMessages messages={messages} />

            {/* Composer */}
            <AiChatComposer
              isStreaming={isStreaming}
              disabled={!providerId}
              onSend={send}
              onAbort={abort}
            />
          </div>
        ) : (
          <button
            onClick={() => setOpen(true)}
            className={cn(
              "size-11 rounded-full bg-primary text-primary-foreground shadow-lg",
              "flex items-center justify-center hover:scale-110 active:scale-90 transition-transform",
            )}
            aria-label={t("aiChat.open")}
          >
            <BotIcon className="size-5" />
          </button>
        )}
      </div>

      <AiChatSettings
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        onSaved={() => {
          // 触发 picker 重新加载（通过 key 重新挂载）
          // 简单方案：关闭设置后重新打开 picker 会因 useEffect 重新加载
        }}
      />
    </>
  );
}
```

- [ ] **Step 2: 创建 index.tsx 导出**

Create `src/components/AiChat/index.tsx`:

```tsx
export { AiChatPanel } from "./AiChatPanel";
```

- [ ] **Step 3: 在 MainLayout 引入 AiChatPanel**

Modify `src/layouts/MainLayout.tsx`，在文件顶部 import 部分新增：

```tsx
import { AiChatPanel } from "@/components/AiChat";
```

在 return 的 `<section>` 末尾、`</section>` 之前新增：

```tsx
      <div className={MAIN_CONTENT_CLASS_NAME}>
        <div className={cn("w-full mx-auto px-4 sm:px-6 pt-2 md:pt-6 pb-8")}>
          <Outlet />
        </div>
      </div>
      <AiChatPanel />
    </section>
```

完整修改后的 return 部分：

```tsx
  return (
    <section className="@container w-full min-h-full flex flex-col justify-start items-center md:flex-row md:items-start">
      {!md && <MobileHeader>{showMemoExplorer && <MemoExplorerDrawer {...memoExplorerProps} />}</MobileHeader>}
      {md && showMemoExplorer && (
        <div className={DESKTOP_EXPLORER_CLASS_NAME}>
          <MemoExplorer className="px-3 py-6" {...memoExplorerProps} />
        </div>
      )}
      <div className={MAIN_CONTENT_CLASS_NAME}>
        <div className={cn("w-full mx-auto px-4 sm:px-6 pt-2 md:pt-6 pb-8")}>
          <Outlet />
        </div>
      </div>
      <AiChatPanel />
    </section>
  );
```

- [ ] **Step 4: TypeScript 编译检查**

Run: `npx tsc --noEmit`
Expected: 无错误

- [ ] **Step 5: 前端构建检查**

Run: `npm run build`
Expected: 构建成功

- [ ] **Step 6: Commit**

```bash
git add src/components/AiChat/AiChatPanel.tsx src/components/AiChat/index.tsx src/layouts/MainLayout.tsx
git commit -m "feat(ai): integrate AiChatPanel into MainLayout (bottom-left floating)"
```

---

## Task 12: 端到端手动验证

**Files:** 无修改

- [ ] **Step 1: 启动开发服务器**

Run: `npm run tauri dev`
Expected: 应用启动，左下角出现圆形 AI 按钮

- [ ] **Step 2: 验证 provider 配置**

1. 点击左下角 AI 按钮，面板展开
2. 点击齿轮设置图标，打开配置弹窗
3. 点击 "Ollama" 预设按钮，确认字段自动填充
4. 点击保存，确认 provider 出现在列表中
5. 关闭弹窗，确认 picker 中可选择该 provider

- [ ] **Step 3: 验证普通对话（无工具调用）**

1. 确保选择了 provider
2. 输入 "你好"，按发送
3. 预期：助手消息流式出现，光标闪烁，结束后光标消失

- [ ] **Step 4: 验证工具调用对话**

1. 先创建几条 memo（在主页正常创建）
2. 在 AI 面板输入 "我有哪些标签？"
3. 预期：出现 "🔧 调用工具: list_tags({})" 灰色提示，然后助手基于结果回复
4. 输入 "帮我搜索关于 Rust 的笔记"
5. 预期：出现 "🔧 调用工具: list_memos({"query":"Rust"})"，助手返回搜索结果

- [ ] **Step 5: 验证 create_memo 工具**

1. 输入 "帮我创建一条笔记：今天学了 AI agent"
2. 预期：出现 "🔧 调用工具: create_memo({"content":"今天学了 AI agent"})"
3. 助手回复创建成功
4. 返回主页，确认新 memo 出现在列表中

- [ ] **Step 6: 验证中断功能**

1. 输入一个长问题，在流式输出过程中点击停止按钮
2. 预期：输出停止，消息末尾出现 "[已中断]"

- [ ] **Step 7: 验证错误处理**

1. 配置一个错误的 provider（错误的 base_url 或 api_key）
2. 发送消息
3. 预期：toast 弹出错误信息，助手消息标记为错误状态

- [ ] **Step 8: 最终 Commit（如有修复）**

若手动验证发现 bug 并修复：

```bash
git add -A
git commit -m "fix(ai): fixes from manual e2e testing"
```

---

## Self-Review

**1. Spec coverage:**
- ✅ 左下角浮动按钮 + 面板 → Task 11
- ✅ Provider 配置（OpenAI/DeepSeek/Ollama 预设）→ Task 5 + Task 8
- ✅ Provider 切换 → Task 9
- ✅ Agent 循环 + 工具调用 → Task 2 + Task 4
- ✅ 4 个工具 → Task 2
- ✅ 流式输出（ai:chunk 事件）→ Task 3 + Task 7
- ✅ 工具调用展示（ai:tool 事件）→ Task 4 + Task 7 + Task 10
- ✅ 中断机制 → Task 4 + Task 7
- ✅ 错误处理 → Task 4 + Task 7
- ✅ 不持久化聊天历史 → Task 7（仅 useState）
- ✅ MainLayout 集成 → Task 11
- ✅ 系统提示词 → Task 4

**2. Placeholder scan:** 无 TBD/TODO；所有代码步骤都有完整代码。

**3. Type consistency:**
- `ProviderConfig` 字段 `{id, name, base_url, api_key, model}` 在 Task 1/5/6/8/9 一致
- `ChatMessage` 字段 `{id, role, content, streaming?, isToolCall?, isError?}` 在 Task 6/7/10 一致
- 事件 payload 结构 `{run_id, text/name/args/message}` 在 Task 4/6/7 一致
- `ai_chat` 命令参数 `{providerId, messages}` 在 Task 4/7 一致
- `ai_abort` 命令参数 `{runId}` 在 Task 4/7 一致
