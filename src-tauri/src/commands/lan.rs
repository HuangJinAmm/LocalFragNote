//! LAN 发现与分享相关 IPC 命令

use crate::error::{IpcError, IpcResult};
use crate::lan::PeerInfo;
use crate::state::AppState;
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
    pub memos: Vec<crate::lan::protocol::RemoteMemoSummary>,
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
    pub rules: Vec<crate::lan::auth::AclRule>,
}

// ---------- 命令骨架（Task 9 实现） ----------

#[tauri::command]
pub async fn lan_discover_peers(_state: tauri::State<'_, AppState>) -> IpcResult<Vec<PeerInfo>> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_local_identity(_state: tauri::State<'_, AppState>) -> IpcResult<LocalIdentity> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_update_display_name(
    _state: tauri::State<'_, AppState>,
    _req: UpdateDisplayNameRequest,
) -> IpcResult<()> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_acl_rules(_state: tauri::State<'_, AppState>) -> IpcResult<Vec<crate::lan::auth::AclRule>> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_save_acl_rules(
    _state: tauri::State<'_, AppState>,
    _req: SaveAclRulesRequest,
) -> IpcResult<()> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_remote_profile(
    _state: tauri::State<'_, AppState>,
    _peer_id: String,
) -> IpcResult<RemoteProfile> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_list_remote_memos(
    _state: tauri::State<'_, AppState>,
    _req: ListRemoteMemosRequest,
) -> IpcResult<ListRemoteMemosResponse> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_remote_memo(
    _state: tauri::State<'_, AppState>,
    _req: GetRemoteMemoRequest,
) -> IpcResult<crate::lan::protocol::RemoteMemo> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_remote_attachment(
    _state: tauri::State<'_, AppState>,
    _req: GetRemoteAttachmentRequest,
) -> IpcResult<RemoteAttachmentResponse> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_copy_memo_to_local(
    _state: tauri::State<'_, AppState>,
    _req: CopyMemoToLocalRequest,
) -> IpcResult<CopyMemoToLocalResponse> {
    Err(IpcError::Lan("not implemented".into()))
}
