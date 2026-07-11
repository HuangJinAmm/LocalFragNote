//! JSON-RPC 协议类型与帧编解码
//!
//! 帧格式：[4 字节大端 u32 长度][JSON 字节流]
//! 单帧上限 MAX_FRAME_SIZE（16 MB）

use crate::lan::LanError;
use serde::{Deserialize, Serialize};

/// 重新导出帧大小上限，方便测试与调用方通过 protocol::* 访问
pub use crate::lan::MAX_FRAME_SIZE;

/// 请求类型（客户端 → 服务端）
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    /// 列出对端公开的笔记（带分页 + tag 过滤）
    ListMemos {
        offset: u32,
        limit: u32,
        #[serde(default)]
        tag_filter: Option<Vec<String>>,
    },
    /// 获取单条笔记完整内容
    GetMemo {
        uid: String,
    },
    /// 获取附件字节
    GetAttachment {
        uid: String,
    },
    /// 获取对端展示名 + 公开笔记统计
    GetProfile,
}

/// 响应类型（服务端 → 客户端）
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "status")]
pub enum Response {
    Ok { data: ResponseData },
    Err { code: u16, message: String },
}

/// 响应数据载体
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ResponseData {
    MemoList {
        memos: Vec<RemoteMemoSummary>,
        total: u32,
    },
    Memo(RemoteMemo),
    Attachment {
        content: Vec<u8>,
        mime_type: String,
    },
    Profile {
        display_name: String,
        public_memo_count: u32,
        tags: Vec<String>,
    },
}

/// 远端笔记摘要（列表项）
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteMemoSummary {
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub pinned: bool,
    pub snippet: String,
    pub tags: Vec<String>,
    pub has_attachments: bool,
}

/// 远端笔记完整内容
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteMemo {
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub pinned: bool,
    pub content: String,
    pub attachments: Vec<RemoteAttachmentSummary>,
}

/// 远端附件元数据
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteAttachmentSummary {
    pub uid: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// 将 payload 写为帧（4字节长度前缀 + 内容）
pub fn write_frame(buf: &mut Vec<u8>, payload: &[u8]) -> Result<(), LanError> {
    let len = payload.len() as u32;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(payload);
    Ok(())
}

/// 从 reader 读取一帧
///
/// 返回 payload 字节切片的 owned 版本。
/// 若声明长度超过 MAX_FRAME_SIZE 返回 FrameTooLarge 错误。
pub fn read_frame<R: std::io::Read>(reader: &mut R) -> Result<Vec<u8>, LanError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            LanError::ConnectionClosed
        } else {
            LanError::Io(e)
        }
    })?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(LanError::FrameTooLarge(len));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            LanError::ConnectionClosed
        } else {
            LanError::Io(e)
        }
    })?;
    Ok(payload)
}

/// 序列化 Request 并写为帧
pub fn write_request<W: std::io::Write>(w: &mut W, req: &Request) -> Result<(), LanError> {
    let json = serde_json::to_vec(req)?;
    let len = json.len() as u32;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&json)?;
    Ok(())
}

/// 从 reader 读取并反序列化 Request
pub fn read_request<R: std::io::Read>(r: &mut R) -> Result<Request, LanError> {
    let payload = read_frame(r)?;
    let req: Request = serde_json::from_slice(&payload)?;
    Ok(req)
}

/// 序列化 Response 并写为帧
pub fn write_response<W: std::io::Write>(w: &mut W, resp: &Response) -> Result<(), LanError> {
    let json = serde_json::to_vec(resp)?;
    let len = json.len() as u32;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&json)?;
    Ok(())
}

/// 从 reader 读取并反序列化 Response
pub fn read_response<R: std::io::Read>(r: &mut R) -> Result<Response, LanError> {
    let payload = read_frame(r)?;
    let resp: Response = serde_json::from_slice(&payload)?;
    Ok(resp)
}

/// 构造 Ok 响应
pub fn ok(data: ResponseData) -> Response {
    Response::Ok { data }
}

/// 构造 Err 响应
pub fn err(code: u16, message: impl Into<String>) -> Response {
    Response::Err { code, message: message.into() }
}
