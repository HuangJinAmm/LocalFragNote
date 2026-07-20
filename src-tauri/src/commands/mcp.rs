//! 本地 MCP 服务器 IPC 命令
//!
//! 提供：配置读写、启动/停止、状态查询、连接测试

use crate::error::{IpcError, IpcResult};
use crate::mcp::{
    load_config, save_config, start_mcp_module, stop_mcp_module, McpConfig, McpStatus,
};
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager};

// ---------- 配置 ----------

/// 读取 MCP 服务器配置
#[tauri::command]
pub fn mcp_get_config(state: tauri::State<'_, AppState>) -> IpcResult<McpConfig> {
    let store = state.store();
    Ok(load_config(&store))
}

/// 保存 MCP 服务器配置
#[tauri::command]
pub fn mcp_update_config(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    req: McpConfig,
) -> IpcResult<McpConfig> {
    if req.host.trim().is_empty() {
        return Err(IpcError::BadRequest("监听 host 不能为空".into()));
    }
    if req.port == 0 {
        return Err(IpcError::BadRequest("监听端口不能为 0".into()));
    }

    let mut req = req;
    req.host = req.host.trim().to_string();
    req.auth_token = req.auth_token.trim().to_string();

    let store = state.store();
    save_config(&store, &req)?;

    // 同步更新运行时配置（运行中不强制重启）
    if let Some(runner) = state.mcp.read().expect("MCP RwLock poisoned").as_ref() {
        runner.update_config(req.clone());
    }
    let _ = app_handle.emit("mcp:config-changed", ());
    Ok(req)
}

// ---------- 生命周期 ----------

/// 启动 MCP 服务器
#[tauri::command]
pub async fn mcp_start(app_handle: AppHandle) -> IpcResult<McpStatus> {
    let state = start_mcp_module(&app_handle).await?;
    Ok(state.status())
}

/// 停止 MCP 服务器
#[tauri::command]
pub async fn mcp_stop(app_handle: AppHandle) -> IpcResult<McpStatus> {
    stop_mcp_module(&app_handle).await?;
    // 返回最新状态
    let cfg = {
        let state = app_handle.state::<AppState>();
        let store = state.store();
        load_config(&store)
    };
    Ok(stopped_status(cfg))
}

/// 查询 MCP 服务器状态
#[tauri::command]
pub fn mcp_get_status(state: tauri::State<'_, AppState>) -> IpcResult<McpStatus> {
    let runner = state
        .mcp
        .read()
        .expect("MCP RwLock poisoned")
        .clone();
    let Some(runner) = runner else {
        let store = state.store();
        let cfg = load_config(&store);
        return Ok(stopped_status(cfg));
    };
    Ok(runner.status())
}

/// 测试连接：调用 `tools/list` 验证 MCP 端点可达且鉴权配置正确
#[tauri::command]
pub async fn mcp_test_connection(
    state: tauri::State<'_, AppState>,
) -> IpcResult<McpTestResult> {
    let cfg = {
        let store = state.store();
        load_config(&store)
    };

    let url = format!("{}/mcp", cfg.endpoint_url().trim_end_matches("/mcp"));
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {}
    });
    let body_str = serde_json::to_string(&body).unwrap();

    let cfg_clone = cfg.clone();
    let url_clone = url.clone();
    let body_clone = body_str.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut req = ureq::post(&url_clone)
            .timeout(std::time::Duration::from_secs(5))
            .set("Content-Type", "application/json")
            .set("Accept", "application/json, text/event-stream");
        if cfg_clone.has_auth() {
            req = req.set("Authorization", &format!("Bearer {}", cfg_clone.auth_token.trim()));
        }
        match req.send_string(&body_clone) {
            Ok(resp) => {
                let status = resp.status();
                let text = resp.into_string().unwrap_or_default();
                if status >= 200 && status < 300 {
                    // 检查响应是否包含 tools 数组
                    let ok = text.contains("\"tools\"")
                        && (text.contains("create_memo") || text.contains("list_memos"));
                    McpTestResult {
                        ok,
                        status,
                        body_preview: text.chars().take(500).collect(),
                        error: if ok {
                            None
                        } else {
                            Some("响应未包含预期工具列表".into())
                        },
                    }
                } else {
                    McpTestResult {
                        ok: false,
                        status,
                        body_preview: text.chars().take(500).collect(),
                        error: Some(format!("HTTP {status}")),
                    }
                }
            }
            Err(e) => {
                let msg = match e {
                    ureq::Error::Status(code, resp) => {
                        format!("HTTP {code}: {}", resp.into_string().unwrap_or_default())
                    }
                    ureq::Error::Transport(t) => format!("网络错误: {t}"),
                };
                McpTestResult {
                    ok: false,
                    status: 0,
                    body_preview: String::new(),
                    error: Some(msg),
                }
            }
        }
    })
    .await
    .map_err(|e| IpcError::Internal(format!("连接测试 join 失败: {e}")))?;
    Ok(result)
}

// ---------- 内部 helper ----------

#[derive(Debug, serde::Serialize)]
pub struct McpTestResult {
    pub ok: bool,
    pub status: u16,
    pub body_preview: String,
    pub error: Option<String>,
}

/// 已停止状态快照：用于未初始化或停止后返回
fn stopped_status(cfg: McpConfig) -> McpStatus {
    McpStatus {
        running: false,
        endpoint_url: cfg.endpoint_url(),
        started_at: None,
        last_error: None,
        config: cfg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stopped_status_uses_config() {
        let mut cfg = McpConfig::default();
        cfg.host = "1.2.3.4".to_string();
        cfg.port = 7777;
        let s = stopped_status(cfg);
        assert!(!s.running);
        assert!(s.started_at.is_none());
        assert_eq!(s.endpoint_url, "http://1.2.3.4:7777/mcp");
    }

    #[test]
    fn test_stopped_status_default_config() {
        let cfg = McpConfig::default();
        let s = stopped_status(cfg);
        assert_eq!(s.endpoint_url, "http://127.0.0.1:27100/mcp");
    }
}
