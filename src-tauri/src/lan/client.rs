//! 客户端：发起连接与请求远端
//!
//! 通过 iroh `Endpoint` 连接到对端 peer，打开 bi-stream 发送 JSON-RPC 请求帧
//! 并读取响应帧。连接阶段使用 `CONNECT_TIMEOUT_SECS` 独立超时；打开流 + 收发
//! 整体受调用方传入的 `timeout` 约束（RPC 10s / 附件 60s）。
//!
//! 帧编解码直接使用 noq 流的异步 I/O 方法（`write_all`/`read_exact`），
//! 与 `protocol.rs` 的同步函数解耦。

use std::time::Duration;

use iroh::endpoint::{ReadExactError, RecvStream, SendStream};
use iroh::{Endpoint, PublicKey};

use crate::lan::protocol::{Request, Response, ResponseData};
use crate::lan::{
    ALPN, ATTACHMENT_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS, LanError, MAX_FRAME_SIZE,
    RPC_TIMEOUT_SECS,
};

/// 通用 RPC 请求（`RPC_TIMEOUT_SECS` 超时）
pub async fn call_remote(
    endpoint: &Endpoint,
    peer_id: &str,
    req: &Request,
) -> Result<ResponseData, LanError> {
    call_remote_with_timeout(endpoint, peer_id, req, Duration::from_secs(RPC_TIMEOUT_SECS)).await
}

/// 附件下载请求（`ATTACHMENT_TIMEOUT_SECS` 超时）
pub async fn call_remote_attachment(
    endpoint: &Endpoint,
    peer_id: &str,
    req: &Request,
) -> Result<ResponseData, LanError> {
    call_remote_with_timeout(endpoint, peer_id, req, Duration::from_secs(ATTACHMENT_TIMEOUT_SECS))
        .await
}

/// 带超时的 RPC 实现
///
/// 连接阶段使用 `CONNECT_TIMEOUT_SECS`（5s）独立超时；
/// 打开 bi-stream + 发送请求 + 读取响应整体受 `timeout` 约束。
async fn call_remote_with_timeout(
    endpoint: &Endpoint,
    peer_id: &str,
    req: &Request,
    timeout: Duration,
) -> Result<ResponseData, LanError> {
    // 1. 解析 peer_id（hex 编码的 EndpointId）为 PublicKey
    let public_key: PublicKey = peer_id
        .parse()
        .map_err(|e| LanError::Endpoint(format!("invalid peer_id: {e}")))?;

    // 2. 发起连接（5 秒超时）
    let conn = tokio::time::timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        endpoint.connect(public_key, ALPN),
    )
    .await
    .map_err(|_| LanError::ConnectTimeout)?
    .map_err(|e| LanError::Endpoint(e.to_string()))?;

    // 3. 打开 bi-stream + 发送请求 + 读取响应（整体受 timeout 约束）
    let rpc = async move {
        let (mut send, mut recv) = conn
            .open_bi()
            .await
            .map_err(|e| LanError::Endpoint(e.to_string()))?;

        write_request_async(&mut send, req).await?;
        // 关闭发送方向，通知对端请求已发完；已缓冲数据仍会送达
        let _ = send.finish();

        let resp = read_response_async(&mut recv).await?;
        match resp {
            Response::Ok { data } => Ok(data),
            Response::Err { code, message } => Err(LanError::Remote(code, message)),
        }
    };

    tokio::time::timeout(timeout, rpc)
        .await
        .map_err(|_| LanError::RpcTimeout)?
}

/// 异步写入请求帧：[4 字节大端 u32 长度][JSON]
async fn write_request_async(w: &mut SendStream, req: &Request) -> Result<(), LanError> {
    let json = serde_json::to_vec(req)?;
    let len = json.len() as u32;
    w.write_all(&len.to_be_bytes())
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;
    w.write_all(&json)
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;
    Ok(())
}

/// 异步读取响应帧并反序列化
async fn read_response_async(r: &mut RecvStream) -> Result<Response, LanError> {
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
    let resp: Response = serde_json::from_slice(&payload)?;
    Ok(resp)
}

/// 将 `ReadExactError::FinishedEarly`（对端提前关闭）映射为 `ConnectionClosed`，
/// 其余读错误归入 `Endpoint`。
fn map_read_exact_error(e: ReadExactError) -> LanError {
    match e {
        ReadExactError::FinishedEarly(_) => LanError::ConnectionClosed,
        ReadExactError::ReadError(r) => LanError::Endpoint(r.to_string()),
    }
}
