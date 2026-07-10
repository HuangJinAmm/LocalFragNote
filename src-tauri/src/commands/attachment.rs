//! Attachment 相关 IPC 命令
//!
//! 双存储模式：
//! - DATABASE：blob 存 SQLite（小文件，<阈值）
//! - LOCAL：blob 存 attachments_dir（大文件，>=阈值），DB 仅存 reference 相对路径
//!
//! 存储类型、阈值、文件名模板均由 `StorageConfig` 控制（见 `commands::setting::StorageConfig`）。

use crate::commands::setting::{load_storage_config, StorageConfig};
use crate::error::{IpcError, IpcResult};
use crate::file_storage;
use crate::state::AppState;
use crate::thumbnail;
use memos_core::attachment::{
    Attachment, CreateAttachment, FindAttachment, UpdateAttachment, STORAGE_TYPE_DATABASE,
    STORAGE_TYPE_LOCAL,
};
use serde::{Deserialize, Serialize};

/// 创建附件请求
///
/// `storage_type` 可选：
/// - 不传或 `null`：按 `StorageConfig.storage_type` 决定（AUTO/DATABASE/LOCAL）
/// - `"DATABASE"`：强制存数据库
/// - `"LOCAL"`：强制存本地文件
#[derive(Debug, Deserialize)]
pub struct CreateAttachmentRequest {
    pub uid: String,
    pub filename: String,
    pub blob: Vec<u8>,
    pub r#type: String,
    pub memo_id: Option<i32>,
    pub storage_type: Option<String>,
}

/// 更新附件请求
#[derive(Debug, Deserialize)]
pub struct UpdateAttachmentRequest {
    pub id: i32,
    pub filename: Option<String>,
    pub memo_id: Option<Option<i32>>,
    pub payload: Option<serde_json::Value>,
}

/// 列表请求
#[derive(Debug, Deserialize, Default)]
pub struct ListAttachmentsRequest {
    pub id: Option<i32>,
    pub uid: Option<String>,
    pub memo_id: Option<i32>,
    /// 批量按 memo_id 查询（OR 条件）
    #[serde(default)]
    pub memo_id_list: Vec<i32>,
    /// 若为 true，过滤 memo_id IS NULL（未关联 memo 的附件）
    #[serde(default)]
    pub memo_id_is_null: bool,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub get_blob: Option<bool>,
}

/// 带 blob 的单条附件
#[derive(Debug, Serialize)]
pub struct AttachmentWithBlob {
    #[serde(flatten)]
    pub attachment: Attachment,
    pub blob: Option<Vec<u8>>,
}

/// 缩略图响应
#[derive(Debug, Serialize)]
pub struct ThumbnailResponse {
    pub blob: Vec<u8>,
    pub mime_type: String,
}

// ---------- 命令 ----------

#[tauri::command]
pub fn create_attachment(
    state: tauri::State<'_, AppState>,
    req: CreateAttachmentRequest,
) -> IpcResult<Attachment> {
    let store = state.store();
    let attachments_dir = state.attachments_dir.clone();

    // 读取存储配置
    let cfg: StorageConfig = load_storage_config(&store);

    let storage_type = req
        .storage_type
        .as_deref()
        .map(|s| if s == STORAGE_TYPE_LOCAL { STORAGE_TYPE_LOCAL } else { STORAGE_TYPE_DATABASE })
        .unwrap_or_else(|| resolve_storage_type(&cfg, req.blob.len()));

    let (blob_for_db, reference, size) = if storage_type == STORAGE_TYPE_LOCAL {
        // LOCAL 模式：按模板写文件，DB 不存 blob
        let reference = file_storage::write_file(
            &attachments_dir,
            &req.uid,
            &req.filename,
            &req.blob,
            &cfg.filepath_template,
        )?;
        (Vec::new(), reference, req.blob.len() as i64)
    } else {
        // DATABASE 模式：blob 存 DB
        (req.blob.clone(), String::new(), req.blob.len() as i64)
    };

    let att = store.with_conn(|c| {
        memos_core::attachment::create(c, &CreateAttachment {
            uid: req.uid,
            filename: req.filename,
            blob: blob_for_db,
            r#type: req.r#type,
            memo_id: req.memo_id,
            storage_type: storage_type.to_string(),
            reference,
            size: Some(size),
        })
    })?;
    Ok(att)
}

/// 根据配置解析存储类型（AUTO 模式按阈值判断）
fn resolve_storage_type(cfg: &StorageConfig, blob_len: usize) -> &'static str {
    match cfg.storage_type.as_str() {
        "DATABASE" => STORAGE_TYPE_DATABASE,
        "LOCAL" => STORAGE_TYPE_LOCAL,
        _ => {
            // AUTO
            if blob_len as u64 >= cfg.auto_threshold {
                STORAGE_TYPE_LOCAL
            } else {
                STORAGE_TYPE_DATABASE
            }
        }
    }
}

#[tauri::command]
pub fn get_attachment(
    state: tauri::State<'_, AppState>,
    id: Option<i32>,
    uid: Option<String>,
    get_blob: Option<bool>,
) -> IpcResult<Option<AttachmentWithBlob>> {
    let store = state.store();
    let want_blob = get_blob.unwrap_or(false);
    let find = FindAttachment { id, uid, get_blob: want_blob, ..Default::default() };
    let att = store.with_conn(|c| memos_core::attachment::get(c, &find))?;

    match att {
        Some(a) => {
            // LOCAL 模式下若需要 blob，从文件系统读取
            let blob = if want_blob {
                if a.storage_type == STORAGE_TYPE_LOCAL && !a.reference.is_empty() {
                    Some(file_storage::read_file(&state.attachments_dir, &a.reference)?)
                } else {
                    a.blob.clone()
                }
            } else {
                None
            };
            Ok(Some(AttachmentWithBlob {
                attachment: Attachment { blob: None, ..a },
                blob,
            }))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn list_attachments(
    state: tauri::State<'_, AppState>,
    req: ListAttachmentsRequest,
) -> IpcResult<Vec<AttachmentWithBlob>> {
    let store = state.store();
    let find = FindAttachment {
        id: req.id,
        uid: req.uid,
        memo_id: req.memo_id,
        memo_id_list: req.memo_id_list,
        memo_id_is_null: req.memo_id_is_null,
        limit: req.limit,
        offset: req.offset,
        get_blob: req.get_blob.unwrap_or(false),
    };
    let list = store.with_conn(|c| memos_core::attachment::list(c, &find))?;
    Ok(list
        .into_iter()
        .map(|a| AttachmentWithBlob {
            blob: a.blob.clone(),
            attachment: Attachment { blob: None, ..a },
        })
        .collect())
}

#[tauri::command]
pub fn update_attachment(
    state: tauri::State<'_, AppState>,
    req: UpdateAttachmentRequest,
) -> IpcResult<Attachment> {
    let store = state.store();
    let updated = store.with_conn(|c| {
        memos_core::attachment::update(c, &UpdateAttachment {
            id: req.id,
            filename: req.filename,
            memo_id: req.memo_id,
            payload: req.payload,
        })
    })?;
    Ok(updated)
}

#[tauri::command]
pub fn delete_attachment(state: tauri::State<'_, AppState>, id: i32) -> IpcResult<()> {
    let store = state.store();
    let deleted_meta = store.with_conn(|c| memos_core::attachment::delete(c, id))?;
    // 清理本地文件
    if let Some((storage_type, reference)) = deleted_meta {
        if storage_type == STORAGE_TYPE_LOCAL && !reference.is_empty() {
            file_storage::delete_file(&state.attachments_dir, &reference)?;
        }
    }
    Ok(())
}

/// 生成附件缩略图
///
/// 对于图片类型附件，返回 JPEG 缩略图（最长边 512px）。
/// 非图片或过小图片返回 NotFound。
#[tauri::command]
pub fn get_attachment_thumbnail(
    state: tauri::State<'_, AppState>,
    id: i32,
    max_size: Option<u32>,
) -> IpcResult<ThumbnailResponse> {
    let store = state.store();
    // 查附件元数据（不带 blob）
    let att = store
        .with_conn(|c| memos_core::attachment::get(c, &FindAttachment { id: Some(id), get_blob: false, ..Default::default() }))?
        .ok_or_else(|| IpcError::NotFound(format!("attachment id={id}")))?;

    if !thumbnail::is_thumbnailable_image(&att.r#type) {
        return Err(IpcError::BadRequest(format!("附件类型不支持缩略图: {}", att.r#type)));
    }

    // 读取原始 blob
    let blob = if att.storage_type == STORAGE_TYPE_LOCAL && !att.reference.is_empty() {
        file_storage::read_file(&state.attachments_dir, &att.reference)?
    } else {
        // DATABASE 模式：重新查询带 blob
        let with_blob = store.with_conn(|c| {
            memos_core::attachment::get(c, &FindAttachment { id: Some(id), get_blob: true, ..Default::default() })
        })?
        .ok_or_else(|| IpcError::NotFound(format!("attachment id={id}")))?;
        with_blob.blob.ok_or_else(|| IpcError::Internal("DATABASE 附件缺少 blob".into()))?
    };

    let size = max_size.unwrap_or(thumbnail::DEFAULT_THUMBNAIL_SIZE);
    let thumb = thumbnail::generate_thumbnail(&blob, &att.r#type, size)
        .map_err(IpcError::Internal)?
        .ok_or_else(|| IpcError::NotFound("原图过小，无需缩略图".into()))?;

    Ok(ThumbnailResponse {
        blob: thumb,
        mime_type: "image/jpeg".into(),
    })
}
