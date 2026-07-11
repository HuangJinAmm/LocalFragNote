//! accept 循环与请求分发
//!
//! 服务端被动接收对端连接，每个 bi-stream 处理一个 JSON-RPC 请求。
//! 帧编解码直接使用 iroh 流的异步 I/O 方法（`read_exact`/`write_all`），
//! 与 `protocol.rs` 的同步函数解耦。

use std::sync::Arc;
use std::time::Duration;

use iroh::endpoint::{ReadExactError, RecvStream, SendStream};
use memos_core::attachment;
use memos_core::markdown;
use memos_core::memo;
use memos_core::types::Visibility;
use tauri::Manager;

use crate::lan::auth::{filter_memos_for_peer, is_memo_visible, load_rules};
use crate::lan::endpoint::{load_acl_rules_json, load_display_name};
use crate::lan::protocol::{
    err, ok, RemoteAttachmentSummary, RemoteMemo, RemoteMemoSummary, Request, Response,
    ResponseData,
};
use crate::lan::{LanError, LanState, MAX_FRAME_SIZE, RPC_TIMEOUT_SECS};
use crate::state::AppState;

/// Handler 结果：成功返回 ResponseData，失败返回 (错误码, 消息)
type HandlerResult = Result<ResponseData, (u16, String)>;

/// 启动 accept 循环，在后台 task 中被动接收对端连接
pub fn spawn_accept_loop(state: Arc<LanState>, app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            match state.endpoint.accept().await {
                Some(incoming) => {
                    let state = state.clone();
                    let app = app_handle.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_incoming(state, app, incoming).await {
                            tracing::warn!("LAN connection handler error: {}", e);
                        }
                    });
                }
                None => break,
            }
        }
    });
}

/// 处理单个连接：完成握手后循环 accept bi-stream
async fn handle_incoming(
    state: Arc<LanState>,
    app: tauri::AppHandle,
    incoming: iroh::endpoint::Incoming,
) -> Result<(), LanError> {
    let conn = incoming
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;
    let peer_id = conn.remote_id().to_string();
    loop {
        match conn.accept_bi().await {
            Ok((mut send, mut recv)) => {
                let state = state.clone();
                let app = app.clone();
                let pid = peer_id.clone();
                tokio::spawn(async move {
                    let result = tokio::time::timeout(
                        Duration::from_secs(RPC_TIMEOUT_SECS),
                        async {
                            let resp = handle_request(&state, &app, &pid, &mut recv).await;
                            if let Err(e) = write_response_async(&mut send, &resp).await {
                                tracing::warn!("LAN write response failed: {}", e);
                            }
                            let _ = send.finish();
                        },
                    )
                    .await;
                    if result.is_err() {
                        tracing::warn!("LAN bi-stream timed out for peer {}", pid);
                    }
                });
            }
            Err(e) => {
                tracing::debug!("LAN accept_bi closed for peer {}: {}", peer_id, e);
                break;
            }
        }
    }
    Ok(())
}

/// 读取请求帧并分发到对应 handler
async fn handle_request(
    state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    recv: &mut RecvStream,
) -> Response {
    let req = match read_request_async(recv).await {
        Ok(r) => r,
        Err(e) => return err(400, e.to_string()),
    };
    let result = match req {
        Request::GetProfile => handle_get_profile(state, app).await,
        Request::ListMemos {
            offset,
            limit,
            tag_filter,
        } => handle_list_memos(state, app, peer_id, offset, limit, tag_filter).await,
        Request::GetMemo { uid } => handle_get_memo(state, app, peer_id, &uid).await,
        Request::GetAttachment { uid } => handle_get_attachment(state, app, peer_id, &uid).await,
    };
    match result {
        Ok(data) => ok(data),
        Err((code, msg)) => err(code, msg),
    }
}

/// 返回展示名 + 公开笔记数 + 标签列表
async fn handle_get_profile(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
) -> HandlerResult {
    let app_state = app.state::<AppState>();
    let store = app_state.store();
    let display_name = load_display_name(&store);

    // 统计 PUBLIC 且 NORMAL 的 memo 数量
    let public_count: i32 = store
        .with_conn(|c| {
            let count: i32 = c.query_row(
                "SELECT count(*) FROM memo WHERE visibility = 'PUBLIC' AND row_status = 'NORMAL'",
                [],
                |r| r.get(0),
            )?;
            Ok(count)
        })
        .map_err(|e| (500, e.to_string()))?;

    // 查询所有 PUBLIC memo 提取 tags
    let memos = store
        .with_conn(|c| {
            memo::list(
                c,
                &memo::FindMemo {
                    visibility_list: vec![Visibility::Public],
                    exclude_content: false,
                    limit: Some(1000),
                    ..Default::default()
                },
            )
        })
        .map_err(|e| (500, e.to_string()))?;

    let mut tags: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for m in &memos {
        for t in markdown::extract_tags(&m.content) {
            if seen.insert(t.clone()) {
                tags.push(t);
            }
        }
    }

    Ok(ResponseData::Profile {
        display_name,
        public_memo_count: public_count as u32,
        tags,
    })
}

/// 返回 ACL 过滤后的笔记列表（分页）
async fn handle_list_memos(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    offset: u32,
    limit: u32,
    tag_filter: Option<Vec<String>>,
) -> HandlerResult {
    // 参数校验
    let limit = limit.clamp(1, 200);

    let app_state = app.state::<AppState>();
    let store = app_state.store();

    // 读取 ACL 规则
    let acl_json = load_acl_rules_json(&store);
    let rules = load_rules(&acl_json);

    // 查询所有 PUBLIC memo（带 tag_filter），先取较大集合再做 ACL 过滤与手动分页
    let mut find = memo::FindMemo {
        visibility_list: vec![Visibility::Public],
        exclude_content: false,
        limit: Some(5000),
        order_by_pinned: true,
        order_by_updated_ts: true,
        ..Default::default()
    };
    if let Some(tags) = &tag_filter {
        find.tag_search = tags.clone();
    }

    let memos = store
        .with_conn(|c| memo::list(c, &find))
        .map_err(|e| (500, e.to_string()))?;

    // ACL 过滤
    let filtered = filter_memos_for_peer(memos, peer_id, &rules);
    let total = filtered.len() as u32;

    // 手动分页
    let start = offset as usize;
    let page: Vec<_> = if start >= filtered.len() {
        Vec::new()
    } else {
        let end = (start + limit as usize).min(filtered.len());
        filtered[start..end].to_vec()
    };

    // 批量查询每个 memo 是否有附件
    let mut has_attachments_map: std::collections::HashMap<i32, bool> =
        std::collections::HashMap::new();
    store
        .with_conn(|c| {
            for m in &page {
                let count: i64 = c.query_row(
                    "SELECT count(*) FROM attachment WHERE memo_id = ?1",
                    rusqlite::params![m.id],
                    |r| r.get(0),
                )?;
                has_attachments_map.insert(m.id, count > 0);
            }
            Ok(())
        })
        .map_err(|e| (500, e.to_string()))?;

    let summaries: Vec<RemoteMemoSummary> = page
        .iter()
        .map(|m| RemoteMemoSummary {
            uid: m.uid.clone(),
            created_ts: m.created_ts,
            updated_ts: m.updated_ts,
            pinned: m.pinned,
            snippet: snippet_text(&m.content, 200),
            tags: markdown::extract_tags(&m.content),
            has_attachments: *has_attachments_map.get(&m.id).unwrap_or(&false),
        })
        .collect();

    Ok(ResponseData::MemoList {
        memos: summaries,
        total,
    })
}

/// 返回单条笔记完整内容（含 ACL 检查）
async fn handle_get_memo(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    uid: &str,
) -> HandlerResult {
    let app_state = app.state::<AppState>();
    let store = app_state.store();

    // 查询 memo by uid
    let memo = store
        .with_conn(|c| {
            memo::get(
                c,
                &memo::FindMemo {
                    uid: Some(uid.to_string()),
                    ..Default::default()
                },
            )
        })
        .map_err(|e| (500, e.to_string()))?
        .ok_or_else(|| (404, format!("memo not found: {}", uid)))?;

    // 验证 visibility == Public
    if memo.visibility != Visibility::Public {
        return Err((403, "memo not public".into()));
    }

    // ACL 检查
    let acl_json = load_acl_rules_json(&store);
    let rules = load_rules(&acl_json);
    if !is_memo_visible(&memo, peer_id, &rules) {
        return Err((403, "memo not visible to peer".into()));
    }

    // 查询关联的附件元数据
    let attachments = store
        .with_conn(|c| {
            attachment::list(
                c,
                &attachment::FindAttachment {
                    memo_id: Some(memo.id),
                    get_blob: false,
                    ..Default::default()
                },
            )
        })
        .map_err(|e| (500, e.to_string()))?;

    let remote_attachments: Vec<RemoteAttachmentSummary> = attachments
        .iter()
        .map(|a| RemoteAttachmentSummary {
            uid: a.uid.clone(),
            filename: a.filename.clone(),
            mime_type: a.r#type.clone(),
            size: a.size as u64,
        })
        .collect();

    Ok(ResponseData::Memo(RemoteMemo {
        uid: memo.uid,
        created_ts: memo.created_ts,
        updated_ts: memo.updated_ts,
        pinned: memo.pinned,
        content: memo.content,
        attachments: remote_attachments,
    }))
}

/// 返回附件字节（含 ACL 检查）
async fn handle_get_attachment(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    uid: &str,
) -> HandlerResult {
    let app_state = app.state::<AppState>();
    let store = app_state.store();

    // 查询附件 by uid
    let att = store
        .with_conn(|c| {
            attachment::get(
                c,
                &attachment::FindAttachment {
                    uid: Some(uid.to_string()),
                    get_blob: false,
                    ..Default::default()
                },
            )
        })
        .map_err(|e| (500, e.to_string()))?
        .ok_or_else(|| (404, format!("attachment not found: {}", uid)))?;

    // 找到关联 memo 并验证可见性 + ACL
    match att.memo_id {
        Some(memo_id) => {
            let memo_opt = store
                .with_conn(|c| {
                    memo::get(
                        c,
                        &memo::FindMemo {
                            id: Some(memo_id),
                            ..Default::default()
                        },
                    )
                })
                .map_err(|e| (500, e.to_string()))?;

            match memo_opt {
                Some(m) => {
                    if m.visibility != Visibility::Public {
                        return Err((403, "associated memo not public".into()));
                    }
                    let acl_json = load_acl_rules_json(&store);
                    let rules = load_rules(&acl_json);
                    if !is_memo_visible(&m, peer_id, &rules) {
                        return Err((403, "associated memo not visible to peer".into()));
                    }
                }
                None => {
                    // memo 已删除但附件 memo_id 未清空，拒绝访问
                    return Err((403, "associated memo not found".into()));
                }
            }
        }
        None => {
            // 附件未关联任何 memo，拒绝访问
            return Err((403, "attachment not associated with any memo".into()));
        }
    }

    // 读取附件字节：LOCAL 从文件，DATABASE 从 blob
    let content = if att.storage_type == attachment::STORAGE_TYPE_LOCAL {
        crate::file_storage::read_file(&app_state.attachments_dir, &att.reference)
            .map_err(|e| (500, e.to_string()))?
    } else {
        store
            .with_conn(|c| {
                attachment::get(
                    c,
                    &attachment::FindAttachment {
                        uid: Some(uid.to_string()),
                        get_blob: true,
                        ..Default::default()
                    },
                )
            })
            .map_err(|e| (500, e.to_string()))?
            .and_then(|a| a.blob)
            .unwrap_or_default()
    };

    Ok(ResponseData::Attachment {
        content,
        mime_type: att.r#type,
    })
}

/// 生成纯文本摘要：简单去除 markdown 标记字符，取前 `max` 个字符
fn snippet_text(content: &str, max: usize) -> String {
    let cleaned: String = content
        .chars()
        .map(|c| match c {
            '#' | '*' | '_' | '`' | '~' | '>' | '|' => ' ',
            _ => c,
        })
        .collect();
    let collapsed: String = cleaned
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let chars: Vec<char> = collapsed.chars().collect();
    if chars.len() <= max {
        collapsed
    } else {
        let mut s: String = chars[..max].iter().collect();
        s.push('\u{2026}');
        s
    }
}

/// 异步读取请求帧并反序列化
async fn read_request_async(r: &mut RecvStream) -> Result<Request, LanError> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf)
        .await
        .map_err(map_read_exact_error)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(LanError::FrameTooLarge(len));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)
        .await
        .map_err(map_read_exact_error)?;
    let req: Request = serde_json::from_slice(&payload)?;
    Ok(req)
}

/// 异步写入响应帧
async fn write_response_async(w: &mut SendStream, resp: &Response) -> Result<(), LanError> {
    let json = serde_json::to_vec(resp)?;
    let len = json.len() as u32;
    w.write_all(&len.to_be_bytes())
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;
    w.write_all(&json)
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;
    Ok(())
}

/// 将 `ReadExactError::FinishedEarly`（对端提前关闭）映射为 `ConnectionClosed`，
/// 其余读错误归入 `Endpoint`。
fn map_read_exact_error(e: ReadExactError) -> LanError {
    match e {
        ReadExactError::FinishedEarly(_) => LanError::ConnectionClosed,
        ReadExactError::ReadError(r) => LanError::Endpoint(r.to_string()),
    }
}
