//! MCP HTTP 服务器：基于 tokio TcpListener 的最小 HTTP/1.1 实现
//!
//! 仅支持 MCP Streamable HTTP 传输所需的最小子集：
//! - `POST /mcp`：JSON-RPC 请求（单条或批量），响应 `application/json`
//! - 其他方法/路径返回 4xx
//!
//! 设计权衡：避免引入 hyper/axum 等额外依赖，二进制体积更小。
//! 单端点、单 JSON 响应足够覆盖 Claude Desktop / Cline / Continue 等客户端。

use crate::mcp::config::McpConfig;
use crate::mcp::error::McpError;
use crate::mcp::protocol::{
    handle_batch, handle_raw_message, RpcResponse, INVALID_REQUEST, PARSE_ERROR,
};
use crate::state::AppState;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 单次请求体最大 8MB（避免恶意大包拖死服务）
const MAX_REQUEST_BODY: usize = 8 * 1024 * 1024;
/// 请求头最大 16KB
const MAX_HEADERS_BYTES: usize = 16 * 1024;

/// MCP 服务器运行时状态
pub struct McpState {
    pub config: RwLock<McpConfig>,
    running: AtomicBool,
    started_at: RwLock<Option<i64>>,
    last_error: RwLock<Option<String>>,
    /// JoinHandle 用于优雅停机
    shutdown_tx: tokio::sync::watch::Sender<bool>,
}

impl McpState {
    pub fn new(config: McpConfig) -> Self {
        let (shutdown_tx, _) = tokio::sync::watch::channel(false);
        Self {
            config: RwLock::new(config),
            running: AtomicBool::new(false),
            started_at: RwLock::new(None),
            last_error: RwLock::new(None),
            shutdown_tx,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn endpoint_url(&self) -> String {
        self.config.read().unwrap().endpoint_url()
    }

    fn set_last_error(&self, msg: String) {
        *self.last_error.write().unwrap() = Some(msg);
    }

    fn append_started_at(&self) {
        *self.started_at.write().unwrap() = Some(now_epoch_secs());
    }

    fn clear_runtime(&self) {
        self.running.store(false, Ordering::SeqCst);
        *self.started_at.write().unwrap() = None;
    }

    /// 更新配置（不强制重启；下次启动时生效）
    pub fn update_config(&self, config: McpConfig) {
        *self.config.write().unwrap() = config;
    }

    /// 状态快照
    pub fn status(&self) -> McpStatus {
        McpStatus {
            running: self.is_running(),
            endpoint_url: self.endpoint_url(),
            started_at: *self.started_at.read().unwrap(),
            last_error: self.last_error.read().unwrap().clone(),
            config: self.config.read().unwrap().clone(),
        }
    }
}

/// IPC 返回的状态快照
#[derive(Debug, Clone, Serialize)]
pub struct McpStatus {
    pub running: bool,
    pub endpoint_url: String,
    pub started_at: Option<i64>,
    pub last_error: Option<String>,
    pub config: McpConfig,
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------- 生命周期 ----------

/// 启动 MCP 模块：若已运行直接返回；否则初始化并 spawn accept 循环
pub async fn start_mcp_module(app_handle: &AppHandle) -> Result<Arc<McpState>, McpError> {
    let state = ensure_state(app_handle)?;

    if state.is_running() {
        return Ok(state);
    }

    let config = state.config.read().unwrap().clone();

    // 绑定监听端口
    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = match TcpListener::bind(&bind_addr).await {
        Ok(l) => l,
        Err(e) => {
            let msg = format!("绑定 {bind_addr} 失败: {e}");
            state.set_last_error(msg.clone());
            let _ = app_handle.emit("mcp:status-changed", ());
            return Err(McpError::Io(e));
        }
    };
    tracing::info!("MCP 服务器监听: {}", bind_addr);

    state.running.store(true, Ordering::SeqCst);
    state.append_started_at();
    *state.last_error.write().unwrap() = None;
    let _ = app_handle.emit("mcp:status-changed", ());

    let state_clone = Arc::clone(&state);
    let app_clone = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        run_accept_loop(state_clone, app_clone, listener).await;
    });

    Ok(state)
}

/// 停止 MCP 模块
pub async fn stop_mcp_module(app_handle: &AppHandle) -> Result<(), McpError> {
    let state = app_handle
        .state::<AppState>()
        .mcp
        .read()
        .expect("MCP RwLock poisoned")
        .clone();

    let Some(state) = state else {
        let _ = app_handle.emit("mcp:status-changed", ());
        return Ok(());
    };

    let _ = state.shutdown_tx.send(true);
    state.clear_runtime();
    let _ = app_handle.emit("mcp:status-changed", ());
    tracing::info!("MCP 服务器已停止");
    Ok(())
}

/// 确保 AppState 中存在 McpState；若已存在则返回现有实例
fn ensure_state(app_handle: &AppHandle) -> Result<Arc<McpState>, McpError> {
    let app_state = app_handle.state::<AppState>();
    {
        let guard = app_state.mcp.read().expect("MCP RwLock poisoned");
        if let Some(existing) = guard.as_ref() {
            return Ok(Arc::clone(existing));
        }
    }
    let mut guard = app_state.mcp.write().expect("MCP RwLock poisoned");
    if let Some(existing) = guard.as_ref() {
        return Ok(Arc::clone(existing));
    }
    let config = {
        let store = app_state.store();
        crate::mcp::config::load_config(&store)
    };
    let state = Arc::new(McpState::new(config));
    *guard = Some(Arc::clone(&state));
    Ok(state)
}

// ---------- accept 循环 ----------

async fn run_accept_loop(state: Arc<McpState>, app: AppHandle, listener: TcpListener) {
    let mut shutdown_rx = state.shutdown_tx.subscribe();
    loop {
        tokio::select! {
            biased;
            _ = shutdown_rx.changed() => {
                tracing::info!("MCP accept loop: 收到 shutdown 信号");
                break;
            }
            accept_result = listener.accept() => {
                let Ok((stream, peer)) = accept_result else {
                    let err = accept_result.unwrap_err();
                    tracing::warn!("MCP accept 失败: {}", err);
                    continue;
                };
                let state = Arc::clone(&state);
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(e) = handle_connection(state, app, stream).await {
                        tracing::debug!(%peer, "MCP 连接处理结束: {}", e);
                    }
                });
            }
        }
    }
    tracing::info!("MCP accept loop terminated");
}

/// 处理单个 TCP 连接：读取 HTTP 请求 → 分发 → 写响应
async fn handle_connection(
    state: Arc<McpState>,
    app: AppHandle,
    mut stream: tokio::net::TcpStream,
) -> Result<(), McpError> {
    // 1. 读取请求头（直到 \r\n\r\n 或超过上限）
    let mut header_buf: Vec<u8> = Vec::with_capacity(2048);
    let mut byte = [0u8; 1];
    loop {
        if header_buf.len() >= MAX_HEADERS_BYTES {
            write_simple_response(&mut stream, 413, "text/plain", "headers too large").await?;
            return Ok(());
        }
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Ok(()); // 连接关闭
        }
        header_buf.push(byte[0]);
        if header_buf.ends_with(b"\r\n\r\n") {
            break;
        }
    }

    // 2. 解析请求行和头部
    let header_str = String::from_utf8_lossy(&header_buf);
    let mut lines = header_str.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("");

    let mut content_length: usize = 0;
    let mut content_type = String::new();
    let mut authorization = String::new();
    let mut accept = String::new();
    let mut mcp_session_id: Option<String> = None;

    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            let name_lower = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            match name_lower.as_str() {
                "content-length" => {
                    content_length = value.parse().unwrap_or(0);
                }
                "content-type" => content_type = value,
                "authorization" => authorization = value,
                "accept" => accept = value,
                "mcp-session-id" => mcp_session_id = Some(value),
                _ => {}
            }
        }
    }
    let _ = (&content_type, &accept, &mcp_session_id);

    // 3. 路由
    if method != "POST" || path != "/mcp" {
        write_simple_response(
            &mut stream,
            404,
            "text/plain",
            "Not Found: MCP endpoint is POST /mcp",
        )
        .await?;
        return Ok(());
    }

    // 4. 鉴权
    let config = state.config.read().unwrap().clone();
    if config.has_auth() {
        let expected = format!("Bearer {}", config.auth_token.trim());
        if authorization != expected {
            write_simple_response(
                &mut stream,
                401,
                "text/plain",
                "Unauthorized: invalid or missing Bearer token",
            )
            .await?;
            return Ok(());
        }
    }

    // 5. 读取请求体
    if content_length > MAX_REQUEST_BODY {
        write_simple_response(&mut stream, 413, "text/plain", "request body too large").await?;
        return Ok(());
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        stream.read_exact(&mut body).await?;
    }
    let body_str = String::from_utf8_lossy(&body).to_string();

    // 6. 解析 JSON-RPC 请求（单条或批量）
    let parsed: Value = match serde_json::from_str(&body_str) {
        Ok(v) => v,
        Err(e) => {
            let resp = RpcResponse::error(Value::Null, PARSE_ERROR, format!("JSON 解析失败: {e}"));
            write_json_response(&mut stream, &serde_json::to_value(&resp).unwrap()).await?;
            return Ok(());
        }
    };

    // 7. 分发到协议层
    let responses: Vec<RpcResponse> = if parsed.is_array() {
        let batch = parsed.as_array().unwrap();
        if batch.is_empty() {
            // RFC：空批量 → invalid request
            let resp = RpcResponse::error(Value::Null, INVALID_REQUEST, "空批量请求");
            vec![resp]
        } else {
            handle_batch(&app, batch)
        }
    } else {
        match handle_raw_message(&app, &parsed) {
            Some(resp) => vec![resp],
            None => vec![], // 通知，无响应
        }
    };

    // 8. 写响应
    if responses.is_empty() {
        // 全是通知：HTTP 202 Accepted，空 body
        write_simple_response(&mut stream, 202, "application/json", "").await?;
    } else {
        let body = if responses.len() == 1 {
            serde_json::to_value(&responses[0]).unwrap()
        } else {
            serde_json::to_value(&responses).unwrap()
        };
        write_json_response(&mut stream, &body).await?;
    }

    Ok(())
}

/// 写入 HTTP 响应（JSON body）
async fn write_json_response(
    stream: &mut tokio::net::TcpStream,
    body: &Value,
) -> Result<(), McpError> {
    let body_str = serde_json::to_string(body).unwrap_or_else(|_| "{}".into());
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        body_str.len(),
        body_str
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

/// 写入简单的 HTTP 响应（任意状态码与 body）
async fn write_simple_response(
    stream: &mut tokio::net::TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> Result<(), McpError> {
    let status_text = match status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {} {}\r\n\
         Content-Type: {}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        status,
        status_text,
        content_type,
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_state_new_not_running() {
        let state = McpState::new(McpConfig::default());
        assert!(!state.is_running());
        assert!(state.status().started_at.is_none());
    }

    #[test]
    fn test_endpoint_url_reflects_config() {
        let mut cfg = McpConfig::default();
        cfg.host = "0.0.0.0".into();
        cfg.port = 9999;
        let state = McpState::new(cfg);
        assert_eq!(state.endpoint_url(), "http://0.0.0.0:9999/mcp");
    }

    #[test]
    fn test_update_config_changes_endpoint() {
        let state = McpState::new(McpConfig::default());
        let mut cfg = McpConfig::default();
        cfg.port = 12345;
        state.update_config(cfg);
        assert_eq!(state.endpoint_url(), "http://127.0.0.1:12345/mcp");
    }

    #[test]
    fn test_clear_runtime_resets_state() {
        let state = McpState::new(McpConfig::default());
        state.running.store(true, Ordering::SeqCst);
        *state.started_at.write().unwrap() = Some(123);
        state.clear_runtime();
        assert!(!state.is_running());
        assert!(state.status().started_at.is_none());
    }
}
