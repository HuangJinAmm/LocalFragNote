//! LAN 发现与分享相关 IPC 命令

use crate::commands::setting::{load_storage_config, StorageConfig};
use crate::error::{IpcError, IpcResult};
use crate::file_storage;
use crate::lan::auth::AclRule;
use crate::lan::client::{call_remote, call_remote_attachment};
use crate::lan::endpoint::{
    load_acl_rules_json, load_display_name, save_acl_rules_json, save_display_name,
};
use crate::lan::protocol::{RemoteMemo, RemoteMemoSummary, Request, ResponseData};
use crate::lan::PeerInfo;
use crate::state::AppState;
use memos_core::attachment::{CreateAttachment, STORAGE_TYPE_DATABASE, STORAGE_TYPE_LOCAL};
use memos_core::memo::CreateMemo;
use memos_core::types::Visibility;
use serde::{Deserialize, Serialize};

// ---------- 类型定义 ----------

#[derive(Debug, Serialize)]
pub struct LocalIdentity {
    pub peer_id: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDisplayNameRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct RemoteProfile {
    pub display_name: String,
    pub public_memo_count: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListRemoteMemosRequest {
    pub peer_id: String,
    pub offset: u32,
    pub limit: u32,
    #[serde(default)]
    pub tag_filter: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct ListRemoteMemosResponse {
    pub memos: Vec<RemoteMemoSummary>,
    pub total: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetRemoteMemoRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Deserialize)]
pub struct GetRemoteAttachmentRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Serialize)]
pub struct RemoteAttachmentResponse {
    pub content: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Deserialize)]
pub struct CopyMemoToLocalRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Serialize)]
pub struct CopyMemoToLocalResponse {
    pub new_memo_uid: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveAclRulesRequest {
    pub rules: Vec<AclRule>,
}

// ---------- 内部 helper ----------

/// 生成 16 字符 hex UID（时间纳秒，不引入 uuid crate，与 ai/tools.rs 风格一致）
fn gen_uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:016x}", now & 0xFFFF_FFFF_FFFF_FFFF)
}

/// 根据存储配置解析存储类型（AUTO 模式按阈值判断）
fn resolve_storage_type(cfg: &StorageConfig, blob_len: usize) -> &'static str {
    match cfg.storage_type.as_str() {
        "DATABASE" => STORAGE_TYPE_DATABASE,
        "LOCAL" => STORAGE_TYPE_LOCAL,
        _ => {
            if blob_len as u64 >= cfg.auto_threshold {
                STORAGE_TYPE_LOCAL
            } else {
                STORAGE_TYPE_DATABASE
            }
        }
    }
}

/// 内部创建附件：根据配置写入本地文件或数据库，并关联到指定 memo
///
/// 不走 Tauri State，直接用 store 引用 + 配置，供 lan_copy_memo_to_local 复用。
#[allow(clippy::too_many_arguments)]
fn create_attachment_internal(
    store: &memos_core::Store,
    attachments_dir: &std::path::Path,
    cfg: &StorageConfig,
    uid: String,
    filename: String,
    blob: Vec<u8>,
    mime_type: String,
    memo_id: Option<i32>,
) -> IpcResult<()> {
    let storage_type = resolve_storage_type(cfg, blob.len());
    let (blob_for_db, reference, size) = if storage_type == STORAGE_TYPE_LOCAL {
        let reference = file_storage::write_file(
            attachments_dir,
            &uid,
            &filename,
            &blob,
            &cfg.filepath_template,
        )?;
        (Vec::new(), reference, blob.len() as i64)
    } else {
        (blob.clone(), String::new(), blob.len() as i64)
    };
    store.with_conn(|c| {
        memos_core::attachment::create(c, &CreateAttachment {
            uid,
            filename,
            blob: blob_for_db,
            r#type: mime_type,
            memo_id,
            storage_type: storage_type.to_string(),
            reference,
            size: Some(size),
        })
    })?;
    Ok(())
}

// ---------- 命令 ----------

/// 1. 发现局域网 peer（读 mDNS 缓存）
#[tauri::command]
pub async fn lan_discover_peers(state: tauri::State<'_, AppState>) -> IpcResult<Vec<PeerInfo>> {
    let lan = state.lan()?;
    let peers = lan.peers.read().await;
    Ok(peers.clone())
}

/// 2. 获取本机身份（peer_id + display_name）
#[tauri::command]
pub async fn lan_get_local_identity(state: tauri::State<'_, AppState>) -> IpcResult<LocalIdentity> {
    let lan = state.lan()?;
    let peer_id = lan.endpoint.id().to_string();
    let display_name = {
        let store = state.store();
        load_display_name(&store)
    };
    Ok(LocalIdentity { peer_id, display_name })
}

/// 3. 更新本机展示名
#[tauri::command]
pub async fn lan_update_display_name(
    state: tauri::State<'_, AppState>,
    req: UpdateDisplayNameRequest,
) -> IpcResult<()> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(IpcError::BadRequest("展示名不能为空".into()));
    }
    {
        let store = state.store();
        save_display_name(&store, &name)?;
    }
    let lan = state.lan()?;
    *lan.display_name.write().await = name;
    Ok(())
}

/// 4. 读取 ACL 规则
#[tauri::command]
pub async fn lan_get_acl_rules(state: tauri::State<'_, AppState>) -> IpcResult<Vec<AclRule>> {
    let store = state.store();
    let json = load_acl_rules_json(&store);
    Ok(crate::lan::auth::load_rules(&json))
}

/// 5. 保存 ACL 规则
#[tauri::command]
pub async fn lan_save_acl_rules(
    state: tauri::State<'_, AppState>,
    req: SaveAclRulesRequest,
) -> IpcResult<()> {
    for rule in &req.rules {
        if rule.tags.is_empty() {
            return Err(IpcError::BadRequest(format!(
                "规则 tags 不能为空: peer_id={}",
                rule.peer_id
            )));
        }
    }
    let json = serde_json::to_string(&req.rules)?;
    let store = state.store();
    save_acl_rules_json(&store, &json)?;
    Ok(())
}

/// 6. 获取对端资料（展示名 + 公开笔记统计 + tags）
#[tauri::command]
pub async fn lan_get_remote_profile(
    state: tauri::State<'_, AppState>,
    peer_id: String,
) -> IpcResult<RemoteProfile> {
    let lan = state.lan()?;
    let data = call_remote(&lan.endpoint, &peer_id, &Request::GetProfile).await?;
    match data {
        ResponseData::Profile {
            display_name,
            public_memo_count,
            tags,
        } => Ok(RemoteProfile {
            display_name,
            public_memo_count,
            tags,
        }),
        other => Err(IpcError::Lan(format!("意外的响应类型: {other:?}"))),
    }
}

/// 7. 列出对端公开笔记
#[tauri::command]
pub async fn lan_list_remote_memos(
    state: tauri::State<'_, AppState>,
    req: ListRemoteMemosRequest,
) -> IpcResult<ListRemoteMemosResponse> {
    let lan = state.lan()?;
    let rpc_req = Request::ListMemos {
        offset: req.offset,
        limit: req.limit,
        tag_filter: req.tag_filter,
    };
    let data = call_remote(&lan.endpoint, &req.peer_id, &rpc_req).await?;
    match data {
        ResponseData::MemoList { memos, total } => Ok(ListRemoteMemosResponse { memos, total }),
        other => Err(IpcError::Lan(format!("意外的响应类型: {other:?}"))),
    }
}

/// 8. 获取对端单条笔记完整内容
#[tauri::command]
pub async fn lan_get_remote_memo(
    state: tauri::State<'_, AppState>,
    req: GetRemoteMemoRequest,
) -> IpcResult<RemoteMemo> {
    let lan = state.lan()?;
    let rpc_req = Request::GetMemo { uid: req.uid.clone() };
    let data = call_remote(&lan.endpoint, &req.peer_id, &rpc_req).await?;
    match data {
        ResponseData::Memo(memo) => Ok(memo),
        other => Err(IpcError::Lan(format!("意外的响应类型: {other:?}"))),
    }
}

/// 9. 获取对端附件字节
#[tauri::command]
pub async fn lan_get_remote_attachment(
    state: tauri::State<'_, AppState>,
    req: GetRemoteAttachmentRequest,
) -> IpcResult<RemoteAttachmentResponse> {
    let lan = state.lan()?;
    let rpc_req = Request::GetAttachment { uid: req.uid.clone() };
    let data = call_remote_attachment(&lan.endpoint, &req.peer_id, &rpc_req).await?;
    match data {
        ResponseData::Attachment { content, mime_type } => Ok(RemoteAttachmentResponse {
            content,
            mime_type,
        }),
        other => Err(IpcError::Lan(format!("意外的响应类型: {other:?}"))),
    }
}

/// 10. 复制远端笔记到本地（拉取 memo + 附件，本地创建为私有笔记）
#[tauri::command]
pub async fn lan_copy_memo_to_local(
    state: tauri::State<'_, AppState>,
    req: CopyMemoToLocalRequest,
) -> IpcResult<CopyMemoToLocalResponse> {
    let lan = state.lan()?;

    // 1. 拉取远端 memo 完整内容
    let rpc_req = Request::GetMemo { uid: req.uid.clone() };
    let data = call_remote(&lan.endpoint, &req.peer_id, &rpc_req).await?;
    let remote_memo = match data {
        ResponseData::Memo(m) => m,
        other => return Err(IpcError::Lan(format!("意外的响应类型: {other:?}"))),
    };

    // 2. 生成新 uid
    let new_uid = gen_uid();

    // 3. 本地创建 memo（visibility=Private, pinned=false）
    let new_memo_id: i32 = {
        let store = state.store();
        let create = CreateMemo {
            uid: new_uid.clone(),
            content: remote_memo.content.clone(),
            visibility: Visibility::Private,
            pinned: false,
            payload: serde_json::json!({}),
            location: None,
        };
        let memo = store.with_conn(|c| memos_core::memo::create(c, &create))?;
        memo.id
    };

    // 4. 读取存储配置 + clone 附件目录（store guard 在块内释放，不跨 await）
    let cfg: StorageConfig = {
        let store = state.store();
        load_storage_config(&store)
    };
    let attachments_dir = state.attachments_dir.clone();

    // 5. 逐个拉取附件并本地创建
    for att in &remote_memo.attachments {
        // 5a. 拉取附件字节（await）
        let att_req = Request::GetAttachment { uid: att.uid.clone() };
        let att_data = call_remote_attachment(&lan.endpoint, &req.peer_id, &att_req).await?;
        let content = match att_data {
            ResponseData::Attachment { content, .. } => content,
            other => return Err(IpcError::Lan(format!("意外的响应类型: {other:?}"))),
        };

        // 5b. 本地创建附件（同步块，store guard 在块内释放）
        let att_uid = gen_uid();
        let store = state.store();
        create_attachment_internal(
            &store,
            &attachments_dir,
            &cfg,
            att_uid,
            att.filename.clone(),
            content,
            att.mime_type.clone(),
            Some(new_memo_id),
        )?;
    }

    Ok(CopyMemoToLocalResponse { new_memo_uid: new_uid })
}
