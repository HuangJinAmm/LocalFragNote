//! 本地 LLM 启动器 IPC 命令
//!
//! 提供：配置读写、启动/停止、状态查询、连接测试

use crate::error::{IpcError, IpcResult};
use crate::llm_runner::{
    self, effective_base_url, load_config, save_config, LlmRunnerConfig, LlmRunnerState,
    LlmRunnerStatus, RUNNER_TYPE_LMS,
};
use crate::state::AppState;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

// ---------- 配置 ----------

/// 读取本地 LLM 启动器配置
#[tauri::command]
pub fn llm_get_config(state: tauri::State<'_, AppState>) -> IpcResult<LlmRunnerConfig> {
    let store = state.store();
    Ok(load_config(&store))
}

/// 保存本地 LLM 启动器配置
#[tauri::command]
pub fn llm_update_config(
    app_handle: AppHandle,
    state: tauri::State<'_, AppState>,
    req: LlmRunnerConfig,
) -> IpcResult<LlmRunnerConfig> {
    // 校验必填字段
    if req.executable_path.trim().is_empty() {
        return Err(IpcError::BadRequest("可执行文件路径不能为空".into()));
    }
    // lms 模式下 host 由 LM Studio 守护进程决定（lms server start 不支持 --host），
    // 不强制要求；llama-cpp 等前台模式必须指定 host
    if req.runner_type != RUNNER_TYPE_LMS && req.host.trim().is_empty() {
        return Err(IpcError::BadRequest("监听 host 不能为空".into()));
    }
    if req.port == 0 {
        return Err(IpcError::BadRequest("监听端口不能为 0".into()));
    }

    // 拒绝 executable_path 中的 shell 元字符（避免命令注入）
    if req.executable_path.contains(|c: char| c == '|' || c == ';' || c == '&') {
        return Err(IpcError::BadRequest(
            "可执行文件路径包含非法字符（| ; &）".into(),
        ));
    }

    let store = state.store();
    save_config(&store, &req)?;

    // 若启动器已初始化，同步更新运行时配置（运行中不强制重启）
    if let Some(runner) = state.llm.read().expect("LLM RwLock poisoned").as_ref() {
        runner.update_config(req.clone());
    }
    let _ = app_handle.emit("llm:config-changed", ());
    Ok(req)
}

// ---------- 生命周期 ----------

/// 启动本地 LLM 服务
///
/// 若 AppState 中尚无 LlmRunnerState，则使用当前配置初始化一个再启动。
/// 已在运行时直接返回成功。
#[tauri::command]
pub async fn llm_start(app_handle: AppHandle) -> IpcResult<LlmRunnerStatus> {
    let runner = ensure_runner_state(&app_handle)?;
    // spawn 逻辑可能阻塞（守护模式下要等待 lms 返回），放到 spawn_blocking
    let runner_clone = Arc::clone(&runner);
    let app_clone = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        llm_runner::runner::start_runner(runner_clone, app_clone)
    })
    .await
    .map_err(|e| IpcError::Internal(format!("启动任务 join 失败: {e}")))??;
    Ok(runner.status())
}

/// 停止本地 LLM 服务
#[tauri::command]
pub async fn llm_stop(app_handle: AppHandle) -> IpcResult<LlmRunnerStatus> {
    let runner = app_handle
        .state::<AppState>()
        .llm
        .read()
        .expect("LLM RwLock poisoned")
        .clone();
    let Some(runner) = runner else {
        // 没有运行时状态，返回一个 stopped 快照
        let cfg = {
            let state = app_handle.state::<AppState>();
            let store = state.store();
            load_config(&store)
        };
        return Ok(stopped_status(cfg));
    };
    let runner_clone = Arc::clone(&runner);
    let app_clone = app_handle.clone();
    tauri::async_runtime::spawn_blocking(move || {
        llm_runner::runner::stop_runner(runner_clone, app_clone)
    })
    .await
    .map_err(|e| IpcError::Internal(format!("停止任务 join 失败: {e}")))??;
    Ok(runner.status())
}

/// 查询当前状态
#[tauri::command]
pub fn llm_get_status(
    state: tauri::State<'_, AppState>,
) -> IpcResult<LlmRunnerStatus> {
    let runner = state
        .llm
        .read()
        .expect("LLM RwLock poisoned")
        .clone();
    let Some(runner) = runner else {
        let store = state.store();
        let cfg = load_config(&store);
        return Ok(stopped_status(cfg));
    };
    Ok(runner.status())
}

/// 测试与本地服务的连接（GET /v1/models，超时 3 秒）
#[tauri::command]
pub async fn llm_test_connection(
    state: tauri::State<'_, AppState>,
) -> IpcResult<TestConnectionResult> {
    let runner = state
        .llm
        .read()
        .expect("LLM RwLock poisoned")
        .clone();
    let base_url = match runner {
        Some(r) => r.base_url(),
        None => {
            let store = state.store();
            let cfg = load_config(&store);
            effective_base_url(&cfg)
        }
    };
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let result = tauri::async_runtime::spawn_blocking(move || {
        let req = ureq::get(&url).timeout(std::time::Duration::from_secs(3));
        match req.call() {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.into_string().unwrap_or_default();
                if status >= 200 && status < 300 {
                    TestConnectionResult {
                        ok: true,
                        status,
                        body_preview: body.chars().take(500).collect(),
                        error: None,
                    }
                } else {
                    TestConnectionResult {
                        ok: false,
                        status,
                        body_preview: body.chars().take(500).collect(),
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
                TestConnectionResult {
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
pub struct TestConnectionResult {
    pub ok: bool,
    pub status: u16,
    pub body_preview: String,
    pub error: Option<String>,
}

/// 已停止状态快照：用于未初始化或停止后返回
fn stopped_status(cfg: LlmRunnerConfig) -> LlmRunnerStatus {
    LlmRunnerStatus {
        running: false,
        pid: None,
        base_url: effective_base_url(&cfg),
        started_at: None,
        last_error: None,
        recent_logs: Vec::new(),
        config: cfg,
    }
}

/// 确保AppState 中存在 LlmRunnerState；若已存在则返回现有实例
fn ensure_runner_state(app_handle: &AppHandle) -> IpcResult<Arc<LlmRunnerState>> {
    let state = app_handle.state::<AppState>();
    {
        let guard = state.llm.read().expect("LLM RwLock poisoned");
        if let Some(existing) = guard.as_ref() {
            return Ok(Arc::clone(existing));
        }
    }
    // 双检：拿写锁前再次确认
    let mut guard = state.llm.write().expect("LLM RwLock poisoned");
    if let Some(existing) = guard.as_ref() {
        return Ok(Arc::clone(existing));
    }
    let cfg = {
        let store = state.store();
        load_config(&store)
    };
    let runner = Arc::new(LlmRunnerState::new(cfg));
    *guard = Some(Arc::clone(&runner));
    Ok(runner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stopped_status_uses_config() {
        let mut cfg = LlmRunnerConfig::default();
        cfg.host = "1.2.3.4".to_string();
        cfg.port = 7777;
        let s = stopped_status(cfg);
        assert!(!s.running);
        assert!(s.pid.is_none());
        assert_eq!(s.base_url, "http://1.2.3.4:7777/v1");
        assert!(s.recent_logs.is_empty());
    }

    #[test]
    fn test_stopped_status_default_config() {
        let cfg = LlmRunnerConfig::default();
        let s = stopped_status(cfg);
        assert_eq!(s.base_url, "http://127.0.0.1:8080/v1");
    }

    #[test]
    fn test_stopped_status_lms_ignores_host() {
        let mut cfg = LlmRunnerConfig::default();
        cfg.runner_type = RUNNER_TYPE_LMS.to_string();
        cfg.host = "0.0.0.0".to_string(); // lms 模式下 host 不影响 base url
        cfg.port = 1234;
        let s = stopped_status(cfg);
        assert_eq!(s.base_url, "http://127.0.0.1:1234/v1");
    }

    #[test]
    fn test_stopped_status_uses_base_url_override() {
        let mut cfg = LlmRunnerConfig::default();
        cfg.runner_type = RUNNER_TYPE_LMS.to_string();
        cfg.port = 1234;
        cfg.base_url = "http://10.0.0.5:8080/v1/".to_string();
        let s = stopped_status(cfg);
        // override 优先，尾部斜杠被去除
        assert_eq!(s.base_url, "http://10.0.0.5:8080/v1");
    }
}
