//! 自定义协议处理：通过 HTTP 风格 URL 提供附件 blob 访问
//!
//! 前端通过 `http://attachment.localhost/attachments/{uid}` 访问附件，
//! 支持 `?thumbnail=true` 生成缩略图。

use crate::file_storage;
use crate::state::AppState;
use crate::thumbnail;
use memos_core::attachment::{FindAttachment, STORAGE_TYPE_LOCAL};
use std::borrow::Cow;
use tauri::http::Response;

/// 处理 attachment 协议请求
///
/// URL 路径格式：`/attachments/{uid}`
/// Query 参数：`thumbnail=true`（可选，生成缩略图）
pub fn handle_attachment_request(
    state: &AppState,
    request: &tauri::http::Request<Vec<u8>>,
) -> Response<Cow<'static, [u8]>> {
    let uri = request.uri();
    let path = uri.path();

    // 解析 uid：path 格式 "/attachments/{uid}"
    let uid = path.rsplit('/').next().unwrap_or("");
    if uid.is_empty() {
        return text_response(400, "missing attachment uid");
    }

    let query = uri.query().unwrap_or("");
    let want_thumbnail = query.contains("thumbnail=true");

    let store = state.store();
    let att = store.with_conn(|c| {
        memos_core::attachment::get(c, &FindAttachment {
            uid: Some(uid.to_string()),
            get_blob: false,
            ..Default::default()
        })
    });

    let att = match att {
        Ok(Some(a)) => a,
        Ok(None) => return text_response(404, &format!("attachment not found: {uid}")),
        Err(e) => return text_response(500, &format!("db error: {e:?}")),
    };

    // 读取原始 blob
    let blob = if att.storage_type == STORAGE_TYPE_LOCAL && !att.reference.is_empty() {
        match file_storage::read_file(&state.attachments_dir, &att.reference) {
            Ok(b) => b,
            Err(e) => return text_response(404, &format!("file read error: {e:?}")),
        }
    } else {
        // DATABASE 模式：重新查询带 blob
        match store.with_conn(|c| {
            memos_core::attachment::get(c, &FindAttachment {
                uid: Some(uid.to_string()),
                get_blob: true,
                ..Default::default()
            })
        }) {
            Ok(Some(a)) => a.blob.unwrap_or_default(),
            _ => return text_response(404, "attachment blob not found"),
        }
    };

    // 缩略图处理
    let (final_blob, content_type) = if want_thumbnail && thumbnail::is_thumbnailable_image(&att.r#type) {
        match thumbnail::generate_thumbnail(&blob, &att.r#type, thumbnail::DEFAULT_THUMBNAIL_SIZE) {
            Ok(Some(thumb)) => (thumb, "image/jpeg".to_string()),
            _ => (blob, att.r#type.clone()),
        }
    } else {
        (blob, att.r#type.clone())
    };

    Response::builder()
        .status(200)
        .header("Content-Type", &content_type)
        .header("Cache-Control", "public, max-age=3600")
        .header("Access-Control-Allow-Origin", "*")
        .body(Cow::Owned(final_blob))
        .unwrap_or_else(|_| text_response(500, "response build error"))
}

fn text_response(status: u16, msg: &str) -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(status)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(Cow::Owned(msg.as_bytes().to_vec()))
        .unwrap_or_else(|_| {
            Response::new(Cow::Owned(b"internal error".to_vec()))
        })
}
