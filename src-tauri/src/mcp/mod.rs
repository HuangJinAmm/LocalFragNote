//! 本地 MCP（Model Context Protocol）服务器
//!
//! 在应用内启动一个 Streamable HTTP MCP 服务端，向同机其他 MCP 客户端
//! （Claude Desktop、Cline、Continue 等）暴露 memo 卡片的创建 / 修改 / 查询能力。
//!
//! 模块组成：
//! - [`config`]：配置持久化（app_setting 表的 `mcp_config` key）
//! - [`error`]：错误类型
//! - [`protocol`]：JSON-RPC 2.0 与 MCP 方法处理
//! - [`server`]：基于 tokio TcpListener 的最小 HTTP/1.1 实现
//! - [`tools`]：memo CRUD 工具定义
//!
//! 协议参考：MCP 2025-03-26 Streamable HTTP 传输
//! 单一端点 `POST /mcp`，响应 `application/json`（不支持 SSE 流式）。

pub mod config;
pub mod error;
pub mod protocol;
pub mod server;
pub mod tools;

#[allow(unused_imports)]
pub use config::{load_config, save_config, McpConfig, CONFIG_KEY};
pub use error::McpError;
pub use server::{McpState, McpStatus, start_mcp_module, stop_mcp_module};
