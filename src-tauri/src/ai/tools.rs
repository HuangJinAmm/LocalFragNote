//! AI agent 工具：定义 OpenAI function-calling schema + 执行分发

use memos_core::markdown;
use memos_core::memo::{CreateMemo, FindMemo};
use memos_core::types::{RowStatus, Visibility};
use memos_core::Store;
use serde_json::{json, Value};
use tauri::AppHandle;

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
        json!({
            "type": "function",
            "function": {
                "name": "list_memos_by_tag",
                "description": "List memos that contain ALL specified tags. Returns memo content for card generation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags to filter (memo must contain ALL)" },
                        "limit": { "type": "number", "description": "Max results, default 50" }
                    },
                    "required": ["tags"]
                }
            }
        }),
    ]
}

/// 执行工具调用，返回结果 JSON
///
/// `app` 用于在 create_memo 后异步调度 embedding 同步（fire-and-forget，不阻塞当前调用）。
/// 传 None 则跳过 embedding 调度（供单元测试使用）。
pub fn execute_tool(
    name: &str,
    args: &Value,
    store: &Store,
    app: Option<&AppHandle>,
) -> memos_core::CoreResult<Value> {
    match name {
        "list_memos" => execute_list_memos(args, store),
        "get_memo" => execute_get_memo(args, store),
        "create_memo" => execute_create_memo(args, store, app),
        "list_tags" => execute_list_tags(store),
        "list_memos_by_tag" => execute_list_memos_by_tag(args, store),
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

fn execute_create_memo(
    args: &Value,
    store: &Store,
    app: Option<&AppHandle>,
) -> memos_core::CoreResult<Value> {
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
        parent_id: None,
    };
    let memo = store.with_conn(|c| memos_core::memo::create(c, &create))?;
    // 异步同步 embedding（fire-and-forget）：在独立 spawn_blocking 中执行，
    // 不在此持 Store 锁做 ONNX 推理，避免阻塞 agent_loop 期间的所有 DB 操作。
    if let Some(app) = app {
        crate::commands::memo::spawn_sync_memo_embedding(app.clone(), memo.clone(), "AI创建");
    }
    Ok(json!({
        "uid": memo.uid,
        "id": memo.id,
        "created_ts": memo.created_ts,
    }))
}

fn execute_list_tags(store: &Store) -> memos_core::CoreResult<Value> {
    let tags = store.with_conn(|c| memos_core::tag::list_tags(c))?;
    let tags: Vec<Value> = tags
        .into_iter()
        .map(|(tag, count)| json!({ "tag": tag, "count": count }))
        .collect();
    Ok(json!({ "tags": tags }))
}

fn execute_list_memos_by_tag(args: &Value, store: &Store) -> memos_core::CoreResult<Value> {
    let tags: Vec<String> = args
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    if tags.is_empty() {
        return Ok(json!({ "memos": [] }));
    }

    let limit = args
        .get("limit")
        .and_then(|v| v.as_i64())
        .map(|n| n as i32)
        .unwrap_or(50)
        .min(200)
        .max(1) as i32;

    let find = FindMemo {
        tag_search: tags.clone(),
        row_status: Some(RowStatus::Normal),
        limit: Some(limit),
        ..Default::default()
    };

    let memos = store.with_conn(|c| memos_core::memo::list(c, &find))?;
    let result: Vec<Value> = memos
        .iter()
        .map(|m| {
            json!({
                "uid": m.uid,
                "content": m.content,
                "tags": markdown::extract_tags(&m.content),
                "created_ts": m.created_ts,
                "updated_ts": m.updated_ts,
            })
        })
        .collect();
    Ok(json!({ "memos": result }))
}

/// 生成 16 字符 hex ID
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
        assert_eq!(defs.len(), 5);
        let names: Vec<&str> = defs
            .iter()
            .map(|d| d["function"]["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"list_memos"));
        assert!(names.contains(&"get_memo"));
        assert!(names.contains(&"create_memo"));
        assert!(names.contains(&"list_tags"));
        assert!(names.contains(&"list_memos_by_tag"));
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
        let result = execute_create_memo(&json!({"content": "#test 新笔记"}), &store, None).unwrap();
        assert!(result["uid"].as_str().unwrap().len() > 0);
        assert!(result["id"].as_i64().unwrap() > 0);
    }

    #[test]
    fn test_create_memo_missing_content() {
        let store = Store::open(":memory:").unwrap();
        let result = execute_create_memo(&json!({}), &store, None);
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
        let result = execute_tool("unknown_tool", &json!({}), &store, None);
        assert!(result.is_err());
    }
}
