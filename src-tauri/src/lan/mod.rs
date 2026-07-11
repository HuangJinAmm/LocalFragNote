//! 局域网发现与分享模块
//!
//! 基于 iroh QUIC Endpoint + mDNS，实现被动公开 + 按需查询 + 按需复制模型。
//! 不做主动同步，每次请求独立无状态。

pub mod auth;
pub mod client;
pub mod endpoint;
pub mod protocol;
pub mod server;

use iroh::Endpoint;
use iroh_mdns_address_lookup::MdnsAddressLookup;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 局域网发现运行时状态
pub struct LanState {
    pub endpoint: Endpoint,
    /// mDNS 地址查找服务句柄，用于订阅发现事件
    pub mdns: MdnsAddressLookup,
    /// mDNS 发现的 peer 缓存：peer_id → PeerInfo
    pub peers: RwLock<Vec<PeerInfo>>,
    /// 本机展示名
    pub display_name: RwLock<String>,
}

/// 发现到的 peer 信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub display_name: String,
    pub addrs: Vec<String>,
    pub relay_url: Option<String>,
    pub last_seen: i64,
}

/// LAN 模块错误类型
#[derive(Debug, thiserror::Error)]
pub enum LanError {
    #[error("iroh endpoint error: {0}")]
    Endpoint(String),
    #[error("connect timeout")]
    ConnectTimeout,
    #[error("rpc timeout")]
    RpcTimeout,
    #[error("connection closed")]
    ConnectionClosed,
    #[error("frame too large: {0} bytes")]
    FrameTooLarge(usize),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("remote error {0}: {1}")]
    Remote(u16, String),
    #[error("local store error: {0}")]
    LocalStore(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// ALPN 协议标识
pub const ALPN: &[u8] = b"memos/lan-share/1";

/// 单帧最大字节数（16 MB）
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// 连接超时（秒）
pub const CONNECT_TIMEOUT_SECS: u64 = 5;

/// RPC 读写超时（秒）
pub const RPC_TIMEOUT_SECS: u64 = 10;

/// 附件传输超时（秒）
pub const ATTACHMENT_TIMEOUT_SECS: u64 = 60;
