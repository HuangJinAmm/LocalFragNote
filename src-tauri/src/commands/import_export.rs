//! 笔记导入导出 IPC 命令
//!
//! - JSON：完整数据备份/恢复（含所有字段）
//! - Markdown：人类可读格式，YAML-like frontmatter + 正文

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use chrono::{DateTime, Utc};
use memos_core::memo::{CreateMemo, FindMemo, Memo};
use memos_core::types::{RowStatus, Visibility};
use memos_core::Store;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::Value;

/// 生成 16 字符 hex UID（与 ai/tools.rs 风格一致）
fn new_uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // 加一个随机后缀减少碰撞（纳秒低 64 位 + 计数器思想）
    format!("{:016x}", now & 0xFFFF_FFFF_FFFF_FFFF)
}

/// 查询所有主笔记（不含评论）
fn list_all_main_memos(store: &Store) -> IpcResult<Vec<Memo>> {
    let find = FindMemo {
        main_only: true,
        limit: None,
        offset: None,
        ..Default::default()
    };
    let memos = store.with_conn(|c| memos_core::memo::list(c, &find))?;
    Ok(memos)
}

// ==================== JSON 导出/导入 ====================

#[tauri::command]
pub fn export_memos_json(state: tauri::State<'_, AppState>) -> IpcResult<String> {
    let store = state.store();
    let memos = list_all_main_memos(&store)?;
    serde_json::to_string_pretty(&memos).map_err(|e| IpcError::Internal(format!("序列化失败: {e}")))
}

/// JSON 导入的单条记录（uid 可选，不提供则自动生成）
#[derive(Debug, Deserialize)]
struct ImportMemoRecord {
    #[serde(default)]
    uid: Option<String>,
    #[serde(default)]
    content: String,
    #[serde(default)]
    visibility: Visibility,
    #[serde(default)]
    pinned: bool,
    #[serde(default = "default_payload")]
    payload: Value,
    #[serde(default)]
    location: Option<memos_core::memo::MemoLocation>,
    #[serde(default)]
    created_ts: Option<i64>,
    #[serde(default)]
    row_status: Option<RowStatus>,
}

fn default_payload() -> Value {
    Value::Object(Default::default())
}

#[tauri::command]
pub fn import_memos_json(
    state: tauri::State<'_, AppState>,
    json_str: String,
) -> IpcResult<i32> {
    let records: Vec<ImportMemoRecord> =
        serde_json::from_str(&json_str).map_err(|e| IpcError::Internal(format!("JSON 解析失败: {e}")))?;

    let store = state.store();
    let conn = store.lock_conn();
    let mut imported = 0i32;
    for rec in &records {
        let uid = rec.uid.clone().unwrap_or_else(new_uid);
        // 如果 uid 已存在，跳过（幂等导入）
        if uid_exists(&conn, &uid)? {
            continue;
        }
        let memo = create_with_timestamp(
            &conn,
            &CreateMemo {
                uid,
                content: rec.content.clone(),
                visibility: rec.visibility,
                pinned: rec.pinned,
                payload: rec.payload.clone(),
                location: rec.location.clone(),
                parent_id: None,
            },
            rec.created_ts,
        )?;
        // 若标记为归档，创建后更新 row_status
        if let Some(RowStatus::Archived) = rec.row_status {
            archive_memo(&conn, memo.id)?;
        }
        imported += 1;
    }

    Ok(imported)
}

/// 检查 uid 是否已存在
fn uid_exists(c: &Connection, uid: &str) -> IpcResult<bool> {
    let count: i32 = c
        .query_row(
            "SELECT COUNT(*) FROM memo WHERE uid = ?1",
            rusqlite::params![uid],
            |r| r.get(0),
        )
        .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(count > 0)
}

/// 创建 memo 并覆盖 created_ts（用于导入时保留原始时间）
fn create_with_timestamp(
    c: &Connection,
    create: &CreateMemo,
    created_ts: Option<i64>,
) -> IpcResult<Memo> {
    let memo = memos_core::memo::create(c, create)?;
    if let Some(ts) = created_ts {
        c.execute(
            "UPDATE memo SET created_ts = ?1 WHERE id = ?2",
            rusqlite::params![ts, memo.id],
        )
        .map_err(|e| IpcError::Internal(e.to_string()))?;
        // 返回更新后的 memo
        let find = FindMemo { id: Some(memo.id), ..Default::default() };
        return Ok(memos_core::memo::get(c, &find)?
            .ok_or_else(|| IpcError::Internal("导入后查询失败".into()))?);
    }
    Ok(memo)
}

/// 将 memo 标记为归档
fn archive_memo(c: &Connection, id: i32) -> IpcResult<()> {
    c.execute(
        "UPDATE memo SET row_status = 'ARCHIVED' WHERE id = ?1",
        rusqlite::params![id],
    )
    .map_err(|e| IpcError::Internal(e.to_string()))?;
    Ok(())
}

// ==================== Markdown 导出/导入 ====================

fn ts_to_iso(ts: i64) -> String {
    DateTime::<Utc>::from_timestamp(ts, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| ts.to_string())
}

fn iso_to_ts(iso: &str) -> Option<i64> {
    // 尝试 RFC3339 解析
    if let Ok(dt) = DateTime::parse_from_rfc3339(iso) {
        return Some(dt.timestamp());
    }
    // fallback: 纯数字 → 直接当 unix 秒
    iso.trim().parse::<i64>().ok()
}

/// 导出为 Markdown（YAML-like frontmatter + 正文，多条以空行分隔）
#[tauri::command]
pub fn export_memos_markdown(state: tauri::State<'_, AppState>) -> IpcResult<String> {
    let store = state.store();
    let memos = list_all_main_memos(&store)?;

    let mut out = String::new();
    for memo in &memos {
        // frontmatter
        out.push_str("---\n");
        out.push_str(&format!("uid: {}\n", memo.uid));
        out.push_str(&format!("created: {}\n", ts_to_iso(memo.created_ts)));
        out.push_str(&format!("updated: {}\n", ts_to_iso(memo.updated_ts)));
        out.push_str(&format!("visibility: {}\n", memo.visibility));
        out.push_str(&format!("pinned: {}\n", memo.pinned));
        out.push_str(&format!(
            "archived: {}\n",
            matches!(memo.row_status, RowStatus::Archived)
        ));
        out.push_str("---\n\n");
        out.push_str(&memo.content);
        out.push_str("\n\n");
    }
    Ok(out)
}

/// 从 Markdown 文本导入（解析 frontmatter + 正文）
#[tauri::command]
pub fn import_memos_markdown(
    state: tauri::State<'_, AppState>,
    markdown_str: String,
) -> IpcResult<i32> {
    let sections = parse_markdown_sections(&markdown_str);

    let store = state.store();
    let conn = store.lock_conn();
    let mut imported = 0i32;
    for sec in &sections {
        let uid = sec.uid.clone().unwrap_or_else(new_uid);
        if uid_exists(&conn, &uid)? {
            continue;
        }
        let memo = create_with_timestamp(
            &conn,
            &CreateMemo {
                uid,
                content: sec.content.clone(),
                visibility: sec.visibility,
                pinned: sec.pinned,
                payload: Value::Object(Default::default()),
                location: None,
                parent_id: None,
            },
            sec.created_ts,
        )?;
        if sec.archived {
            archive_memo(&conn, memo.id)?;
        }
        imported += 1;
    }

    Ok(imported)
}

struct ParsedMemo {
    uid: Option<String>,
    content: String,
    visibility: Visibility,
    pinned: bool,
    archived: bool,
    created_ts: Option<i64>,
}

/// 将 markdown 文本拆分为多个 memo section
/// 每个 section 以 --- 开头的 frontmatter 块标识，后跟正文
fn parse_markdown_sections(text: &str) -> Vec<ParsedMemo> {
    let mut result = Vec::new();
    // 按 "\n---\n" 或文件开头的 "---\n" 拆分
    // 简单状态机：寻找 frontmatter 起始标记
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        // 跳过空行
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        // 期望 "---" 作为 frontmatter 开始
        if lines[i].trim() == "---" {
            i += 1;
            let mut meta: std::collections::HashMap<String, String> = std::collections::HashMap::new();
            // 收集 frontmatter 键值对直到下一个 "---"
            while i < lines.len() && lines[i].trim() != "---" {
                if let Some((k, v)) = lines[i].split_once(':') {
                    meta.insert(k.trim().to_string(), v.trim().to_string());
                }
                i += 1;
            }
            // 跳过结束 "---"
            if i < lines.len() {
                i += 1;
            }

            // 收集正文直到下一个 "---" frontmatter 或文件结束
            let mut content_lines = Vec::new();
            while i < lines.len() {
                // 检查是否到了下一个 frontmatter 开始（行是 "---" 且下一行包含 ":"）
                if lines[i].trim() == "---" && i + 1 < lines.len() && lines[i + 1].contains(':') {
                    break;
                }
                // 文件末尾的 "---" 也可能是 frontmatter 结束标记
                if lines[i].trim() == "---" && i + 1 >= lines.len() {
                    break;
                }
                content_lines.push(lines[i]);
                i += 1;
            }

            // 去除前导/尾部空行
            while content_lines.first().map_or(false, |l| l.trim().is_empty()) {
                content_lines.remove(0);
            }
            while content_lines.last().map_or(false, |l| l.trim().is_empty()) {
                content_lines.pop();
            }

            let content = content_lines.join("\n");
            if content.is_empty() && meta.is_empty() {
                continue;
            }

            result.push(ParsedMemo {
                uid: meta.get("uid").cloned(),
                content,
                visibility: meta
                    .get("visibility")
                    .and_then(|v| match v.to_uppercase().as_str() {
                        "PUBLIC" => Some(Visibility::Public),
                        "PROTECTED" => Some(Visibility::Protected),
                        "PRIVATE" => Some(Visibility::Private),
                        _ => None,
                    })
                    .unwrap_or(Visibility::Private),
                pinned: meta
                    .get("pinned")
                    .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                    .unwrap_or(false),
                archived: meta
                    .get("archived")
                    .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                    .unwrap_or(false),
                created_ts: meta.get("created").and_then(|v| iso_to_ts(v)),
            });
        } else {
            // 没有 frontmatter 的行，作为无 metadata 的纯文本内容
            let mut content_lines = Vec::new();
            while i < lines.len() {
                if lines[i].trim() == "---" && i + 1 < lines.len() && lines[i + 1].contains(':') {
                    break;
                }
                content_lines.push(lines[i]);
                i += 1;
            }
            let content = content_lines.join("\n").trim().to_string();
            if !content.is_empty() {
                result.push(ParsedMemo {
                    uid: None,
                    content,
                    visibility: Visibility::Private,
                    pinned: false,
                    archived: false,
                    created_ts: None,
                });
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_memo() {
        let md = "---\nuid: abc123\ncreated: 2024-01-01T00:00:00Z\nvisibility: PRIVATE\npinned: false\narchived: false\n---\n\nHello world\n";
        let sections = parse_markdown_sections(md);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].uid.as_deref(), Some("abc123"));
        assert_eq!(sections[0].content, "Hello world");
        assert_eq!(sections[0].visibility, Visibility::Private);
    }

    #[test]
    fn test_parse_multiple_memos() {
        let md = "---\nuid: a\nvisibility: PUBLIC\npinned: true\narchived: false\n---\n\nFirst\n\n---\nuid: b\nvisibility: PRIVATE\npinned: false\narchived: false\n---\n\nSecond\n";
        let sections = parse_markdown_sections(md);
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].uid.as_deref(), Some("a"));
        assert_eq!(sections[0].content, "First");
        assert_eq!(sections[0].visibility, Visibility::Public);
        assert!(sections[0].pinned);
        assert_eq!(sections[1].uid.as_deref(), Some("b"));
        assert_eq!(sections[1].content, "Second");
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let md = "Just some text\nwithout frontmatter\n";
        let sections = parse_markdown_sections(md);
        assert_eq!(sections.len(), 1);
        assert!(sections[0].uid.is_none());
        assert_eq!(sections[0].content, "Just some text\nwithout frontmatter");
    }
}
