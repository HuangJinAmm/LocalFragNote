//! MCP 模块错误类型

use thiserror::Error;

/// MCP 模块错误
#[derive(Debug, Error)]
#[allow(dead_code)]
pub enum McpError {
    #[error("MCP 服务错误: {0}")]
    Other(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("序列化错误: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("本地存储错误: {0}")]
    LocalStore(String),
}
