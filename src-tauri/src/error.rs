//! IPC 错误类型：统一序列化以便前端处理

use memos_core::CoreError;
use serde::Serialize;
use std::fmt;

/// IPC 命令返回的错误，自动序列化为 JSON
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "message")]
pub enum IpcError {
    /// 资源不存在
    NotFound(String),
    /// UID 冲突
    UidConflict(String),
    /// UID 非法
    InvalidUid,
    /// 数据库或内部错误
    Internal(String),
    /// IO 错误（文件操作）
    Io(String),
    /// 参数非法
    BadRequest(String),
    /// LAN 模块错误
    Lan(String),
    /// 回顾模块错误
    Review(String),
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IpcError::NotFound(msg) => write!(f, "NotFound: {msg}"),
            IpcError::UidConflict(uid) => write!(f, "UidConflict: {uid}"),
            IpcError::InvalidUid => write!(f, "InvalidUid"),
            IpcError::Internal(msg) => write!(f, "Internal: {msg}"),
            IpcError::Io(msg) => write!(f, "Io: {msg}"),
            IpcError::BadRequest(msg) => write!(f, "BadRequest: {msg}"),
            IpcError::Lan(msg) => write!(f, "Lan: {msg}"),
            IpcError::Review(msg) => write!(f, "Review: {msg}"),
        }
    }
}

impl From<CoreError> for IpcError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::NotFound(msg) => IpcError::NotFound(msg),
            CoreError::UidConflict(uid) => IpcError::UidConflict(uid),
            CoreError::InvalidUid => IpcError::InvalidUid,
            CoreError::Db(e) => IpcError::Internal(e.to_string()),
            CoreError::Migration(e) => IpcError::Internal(e.to_string()),
            CoreError::Serde(e) => IpcError::Internal(e.to_string()),
            CoreError::Io(e) => IpcError::Io(e.to_string()),
            CoreError::Other(msg) => IpcError::Internal(msg),
        }
    }
}

impl From<std::io::Error> for IpcError {
    fn from(e: std::io::Error) -> Self {
        IpcError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for IpcError {
    fn from(e: serde_json::Error) -> Self {
        IpcError::Internal(format!("serde: {e}"))
    }
}

impl From<crate::lan::LanError> for IpcError {
    fn from(e: crate::lan::LanError) -> Self {
        IpcError::Lan(e.to_string())
    }
}

/// 通用 Result 别名
pub type IpcResult<T> = Result<T, IpcError>;
