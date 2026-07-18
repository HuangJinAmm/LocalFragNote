//! 进程管理：spawn / kill / 状态查询 / 日志捕获
//!
//! 两种生命周期模型：
//! - 前台模式（`llama-cpp`）：spawn 长驻子进程，跟踪 PID，stop 时 kill 子进程
//! - 守护模式（`lms`）：`lms server start` 立即返回，再调用 `lms load <model>` 加载模型
//!   模型由 LM Studio 守护进程管理；stop 时调用 `lms server stop`；不持有子进程句柄

use crate::llm_runner::config::{is_daemon_runner, LlmRunnerConfig, RUNNER_TYPE_LMS};
use serde::Serialize;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use thiserror::Error;

/// 日志环缓冲区最大行数
const LOG_BUFFER_LINES: usize = 500;

/// 启动器错误
#[derive(Debug, Error)]
pub enum LlmRunnerError {
    #[error("LLM 启动器错误: {0}")]
    Other(String),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("配置错误: {0}")]
    Config(String),
}

/// 运行状态快照（IPC 返回）
#[derive(Debug, Clone, Serialize)]
pub struct LlmRunnerStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub base_url: String,
    pub started_at: Option<i64>,
    pub last_error: Option<String>,
    pub recent_logs: Vec<String>,
    pub config: LlmRunnerConfig,
}

/// 运行时状态：持有子进程、运行标志、日志缓冲
pub struct LlmRunnerState {
    pub config: RwLock<LlmRunnerConfig>,
    /// 前台模式：跟踪子进程；守护模式：始终 None
    child: Mutex<Option<Child>>,
    running: AtomicBool,
    logs: RwLock<VecDeque<String>>,
    started_at: RwLock<Option<i64>>,
    last_error: RwLock<Option<String>>,
}

impl LlmRunnerState {
    pub fn new(config: LlmRunnerConfig) -> Self {
        Self {
            config: RwLock::new(config),
            child: Mutex::new(None),
            running: AtomicBool::new(false),
            logs: RwLock::new(VecDeque::with_capacity(LOG_BUFFER_LINES)),
            started_at: RwLock::new(None),
            last_error: RwLock::new(None),
        }
    }

    /// 当前是否仍在运行
    pub fn is_running(&self) -> bool {
        if !self.running.load(Ordering::SeqCst) {
            return false;
        }
        // 前台模式：双检 child 状态
        let cfg = self.config.read().unwrap();
        if is_daemon_runner(&cfg) {
            return true; // 守护模式：只看 running 标志
        }
        drop(cfg);
        let mut guard = self.child.lock().unwrap();
        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    // 进程已退出，回收并清空
                    *guard = None;
                    self.running.store(false, Ordering::SeqCst);
                    *self.started_at.write().unwrap() = None;
                    false
                }
                Ok(None) => true,
                Err(_) => {
                    *guard = None;
                    self.running.store(false, Ordering::SeqCst);
                    *self.started_at.write().unwrap() = None;
                    false
                }
            }
        } else {
            self.running.store(false, Ordering::SeqCst);
            false
        }
    }

    /// 当前 base_url（OpenAI 兼容端点）
    ///
    /// 优先返回用户在配置中显式填写的 `base_url`；为空时按 runner 类型派生默认值。
    pub fn base_url(&self) -> String {
        let cfg = self.config.read().unwrap();
        crate::llm_runner::effective_base_url(&cfg)
    }

    /// 状态快照
    pub fn status(&self) -> LlmRunnerStatus {
        let running = self.is_running();
        let pid = self
            .child
            .lock()
            .unwrap()
            .as_ref()
            .map(|c| c.id());
        LlmRunnerStatus {
            running,
            pid,
            base_url: self.base_url(),
            started_at: *self.started_at.read().unwrap(),
            last_error: self.last_error.read().unwrap().clone(),
            recent_logs: self.logs.read().unwrap().iter().cloned().collect(),
            config: self.config.read().unwrap().clone(),
        }
    }

    fn set_last_error(&self, msg: String) {
        *self.last_error.write().unwrap() = Some(msg);
    }

    fn append_log(&self, line: String) {
        let mut logs = self.logs.write().unwrap();
        if logs.len() >= LOG_BUFFER_LINES {
            logs.pop_front();
        }
        logs.push_back(line);
    }

    /// 更新运行配置（仅在未运行时调用，避免运行中改配置导致状态不一致）
    pub fn update_config(&self, config: LlmRunnerConfig) {
        *self.config.write().unwrap() = config;
    }
}

/// 启动本地 LLM 服务
pub fn start_runner(
    state: Arc<LlmRunnerState>,
    app_handle: AppHandle,
) -> Result<(), LlmRunnerError> {
    if state.is_running() {
        return Ok(());
    }

    let config = state.config.read().unwrap().clone();

    if is_daemon_runner(&config) {
        start_daemon(state, app_handle, config)
    } else {
        start_foreground(state, app_handle, config)
    }
}

/// 停止本地 LLM 服务
pub fn stop_runner(
    state: Arc<LlmRunnerState>,
    app_handle: AppHandle,
) -> Result<(), LlmRunnerError> {
    let config = state.config.read().unwrap().clone();
    if is_daemon_runner(&config) {
        // 守护模式：调用 `lms server stop`
        let mut cmd = Command::new(&config.executable_path);
        cmd.arg("server").arg("stop");
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        match cmd.output() {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
                if !stderr.is_empty() {
                    state.append_log(format!("[stop] {stderr}"));
                }
            }
            Err(e) => {
                // 停止失败不致命，仍然标记为已停止
                state.append_log(format!("[stop] 调用 lms server stop 失败: {e}"));
            }
        }
        state.running.store(false, Ordering::SeqCst);
        *state.started_at.write().unwrap() = None;
        let _ = app_handle.emit("llm:status-changed", ());
        Ok(())
    } else {
        // 前台模式：kill 子进程
        let mut guard = state.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        drop(guard);
        state.running.store(false, Ordering::SeqCst);
        *state.started_at.write().unwrap() = None;
        let _ = app_handle.emit("llm:status-changed", ());
        Ok(())
    }
}

/// 前台模式启动：spawn 长驻子进程
fn start_foreground(
    state: Arc<LlmRunnerState>,
    app_handle: AppHandle,
    config: LlmRunnerConfig,
) -> Result<(), LlmRunnerError> {
    let mut cmd = match build_command(&config) {
        Ok(c) => c,
        Err(e) => {
            state.set_last_error(e.to_string());
            let _ = app_handle.emit("llm:status-changed", ());
            return Err(e);
        }
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    tracing::info!(?cmd, "LLM 启动器: spawn 前台进程");

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("启动失败: {e}");
            state.set_last_error(msg.clone());
            state.append_log(format!("[error] {msg}"));
            let _ = app_handle.emit("llm:status-changed", ());
            return Err(LlmRunnerError::Io(e));
        }
    };

    let pid = child.id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    *state.child.lock().unwrap() = Some(child);
    state.running.store(true, Ordering::SeqCst);
    *state.started_at.write().unwrap() = Some(now_epoch_secs());
    *state.last_error.write().unwrap() = None;
    state.append_log(format!("[info] 进程已启动 (pid={pid})"));

    // 日志捕获 + 退出监控线程
    let state_clone = Arc::clone(&state);
    let app_clone = app_handle.clone();
    std::thread::spawn(move || {
        // 先消费 stderr（llama-server 主要日志输出到 stderr），再消费 stdout
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) if !l.is_empty() => {
                        state_clone.append_log(l.clone());
                        let _ = app_clone.emit("llm:log", l);
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) if !l.is_empty() => {
                        state_clone.append_log(l.clone());
                        let _ = app_clone.emit("llm:log", l);
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
        // 进程已退出（管道 EOF），回收子进程并清理状态
        let mut guard = state_clone.child.lock().unwrap();
        if let Some(mut child) = guard.take() {
            let _ = child.wait();
        }
        drop(guard);
        state_clone.running.store(false, Ordering::SeqCst);
        *state_clone.started_at.write().unwrap() = None;
        state_clone.append_log("[info] 进程已退出".to_string());
        let _ = app_clone.emit("llm:status-changed", ());
    });

    let _ = app_handle.emit("llm:status-changed", ());
    Ok(())
}

/// 守护模式启动：调用 `lms server start`（立即返回）
fn start_daemon(
    state: Arc<LlmRunnerState>,
    app_handle: AppHandle,
    config: LlmRunnerConfig,
) -> Result<(), LlmRunnerError> {
    let mut cmd = match build_command(&config) {
        Ok(c) => c,
        Err(e) => {
            state.set_last_error(e.to_string());
            let _ = app_handle.emit("llm:status-changed", ());
            return Err(e);
        }
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    tracing::info!(?cmd, "LLM 启动器: 调用 lms server start");

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            let msg = format!("调用 lms server start 失败: {e}");
            state.set_last_error(msg.clone());
            state.append_log(format!("[error] {msg}"));
            let _ = app_handle.emit("llm:status-changed", ());
            return Err(LlmRunnerError::Io(e));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    for line in stdout.lines().chain(stderr.lines()) {
        if !line.is_empty() {
            state.append_log(line.to_string());
            let _ = app_handle.emit("llm:log", line.to_string());
        }
    }

    if !output.status.success() {
        let msg = format!(
            "lms server start 失败 (exit={}): {stderr}",
            output.status.code().unwrap_or(-1)
        );
        state.set_last_error(msg.clone());
        state.append_log(format!("[error] {msg}"));
        let _ = app_handle.emit("llm:status-changed", ());
        return Err(LlmRunnerError::Other(msg));
    }

    // lms server start 成功返回后，若配置了 model_path 则调用 `lms load` 加载模型
    state.running.store(true, Ordering::SeqCst);
    *state.started_at.write().unwrap() = Some(now_epoch_secs());
    *state.last_error.write().unwrap() = None;
    state.append_log("[info] lms server start 已返回".to_string());
    let _ = app_handle.emit("llm:status-changed", ());

    if !config.model_path.trim().is_empty() {
        if let Err(e) = load_model_after_server_start(&state, &app_handle, &config) {
            // 模型加载失败不回滚 server start 状态，仅记录错误
            let msg = format!("lms load 失败: {e}");
            state.set_last_error(msg.clone());
            state.append_log(format!("[error] {msg}"));
            let _ = app_handle.emit("llm:status-changed", ());
            return Err(e);
        }
    } else {
        state.append_log(
            "[info] 未配置 model_path，跳过 lms load，请通过 LM Studio 手动加载模型".to_string(),
        );
    }

    let _ = app_handle.emit("llm:status-changed", ());
    Ok(())
}

/// 在 `lms server start` 成功后调用 `lms load <model>` 加载指定模型
fn load_model_after_server_start(
    state: &Arc<LlmRunnerState>,
    app_handle: &AppHandle,
    config: &LlmRunnerConfig,
) -> Result<(), LlmRunnerError> {
    let mut cmd = Command::new(&config.executable_path);
    cmd.arg("load").arg(&config.model_path);
    // 附加参数透传（如 --gpu max --context-length 8192 等 lms load 支持的选项）
    for arg in config.extra_args.split_whitespace() {
        cmd.arg(arg);
    }
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    tracing::info!(?cmd, "LLM 启动器: 调用 lms load 加载模型");
    state.append_log(format!("[info] 调用 lms load {}", config.model_path));

    let output = cmd.output().map_err(LlmRunnerError::Io)?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    for line in stdout.lines().chain(stderr.lines()) {
        if !line.is_empty() {
            state.append_log(line.to_string());
            let _ = app_handle.emit("llm:log", line.to_string());
        }
    }

    if !output.status.success() {
        let msg = format!(
            "lms load 失败 (exit={}): {stderr}",
            output.status.code().unwrap_or(-1)
        );
        return Err(LlmRunnerError::Other(msg));
    }

    state.append_log("[info] lms load 已返回，模型加载由 LM Studio 守护进程管理".to_string());
    Ok(())
}

/// 根据配置构造启动命令
fn build_command(config: &LlmRunnerConfig) -> Result<Command, LlmRunnerError> {
    if config.executable_path.trim().is_empty() {
        return Err(LlmRunnerError::Config("可执行文件路径不能为空".into()));
    }
    let mut cmd = Command::new(&config.executable_path);

    match config.runner_type.as_str() {
        RUNNER_TYPE_LMS => {
            // lms server start --port <port>
            // 注意：lms server start 不支持 --model / --host 选项：
            //   - 模型需在 server start 成功后通过 `lms load <model>` 加载（见 start_daemon）
            //   - 监听 host 由 LM Studio 守护进程配置决定（默认 127.0.0.1），CLI 不可改
            cmd.arg("server").arg("start");
            cmd.arg("--port").arg(config.port.to_string());
        }
        _ => {
            // llama-server [-m <model>] --port <port> --host <host> -c <ctx> [-ngl <ngl>]
            if !config.model_path.trim().is_empty() {
                cmd.arg("-m").arg(&config.model_path);
            }
            cmd.arg("--port").arg(config.port.to_string());
            cmd.arg("--host").arg(&config.host);
            cmd.arg("-c").arg(config.context_size.to_string());
            if config.gpu_layers > 0 {
                cmd.arg("-ngl").arg(config.gpu_layers.to_string());
            }
        }
    }

    // 附加参数：按空白分隔透传（不做 shell 解析，避免注入风险）
    for arg in config.extra_args.split_whitespace() {
        cmd.arg(arg);
    }

    Ok(cmd)
}

fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config(runner: &str) -> LlmRunnerConfig {
        LlmRunnerConfig {
            runner_type: runner.to_string(),
            executable_path: "/bin/echo".to_string(),
            model_path: "/models/x.gguf".to_string(),
            host: "127.0.0.1".to_string(),
            port: 8080,
            context_size: 4096,
            gpu_layers: 10,
            extra_args: "--verbose --jinja".to_string(),
            base_url: String::new(),
            auto_start: false,
        }
    }

    #[test]
    fn test_build_command_llama_cpp_includes_all_args() {
        let cfg = sample_config("llama-cpp");
        let cmd = build_command(&cfg).unwrap();
        let prog = cmd.get_program().to_string_lossy().into_owned();
        assert_eq!(prog, "/bin/echo");
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        // -m model --port 8080 --host 127.0.0.1 -c 4096 -ngl 10 --verbose --jinja
        assert!(args.contains(&"-m".to_string()));
        assert!(args.contains(&"/models/x.gguf".to_string()));
        assert!(args.contains(&"--port".to_string()));
        assert!(args.contains(&"8080".to_string()));
        assert!(args.contains(&"--host".to_string()));
        assert!(args.contains(&"127.0.0.1".to_string()));
        assert!(args.contains(&"-c".to_string()));
        assert!(args.contains(&"4096".to_string()));
        assert!(args.contains(&"-ngl".to_string()));
        assert!(args.contains(&"10".to_string()));
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--jinja".to_string()));
    }

    #[test]
    fn test_build_command_lms_uses_server_start_subcommand() {
        let cfg = sample_config("lms");
        let cmd = build_command(&cfg).unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        // lms server start --port Y --verbose --jinja
        // 注意：lms server start 不支持 --model / --host，模型通过 lms load 单独加载，
        // 监听 host 由 LM Studio 守护进程决定
        assert_eq!(args[0], "server");
        assert_eq!(args[1], "start");
        // lms server start 不应包含 --model / -m / --host
        assert!(!args.contains(&"--model".to_string()));
        assert!(!args.contains(&"-m".to_string()));
        assert!(!args.contains(&"--host".to_string()));
        assert!(args.contains(&"--port".to_string()));
        assert!(args.contains(&"8080".to_string()));
        // lms 模式不应包含 -c / -ngl
        assert!(!args.contains(&"-c".to_string()));
        assert!(!args.contains(&"-ngl".to_string()));
        // 附加参数透传（用于 lms load 的 --gpu / --context-length 等选项）
        assert!(args.contains(&"--verbose".to_string()));
        assert!(args.contains(&"--jinja".to_string()));
    }

    #[test]
    fn test_build_command_rejects_empty_executable() {
        let mut cfg = sample_config("llama-cpp");
        cfg.executable_path = "   ".to_string();
        let err = build_command(&cfg).unwrap_err();
        assert!(matches!(err, LlmRunnerError::Config(_)));
    }

    #[test]
    fn test_build_command_skips_empty_model_path() {
        let mut cfg = sample_config("llama-cpp");
        cfg.model_path = String::new();
        let cmd = build_command(&cfg).unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(!args.contains(&"-m".to_string()));
    }

    #[test]
    fn test_build_command_gpu_layers_zero_omits_ngl() {
        let mut cfg = sample_config("llama-cpp");
        cfg.gpu_layers = 0;
        let cmd = build_command(&cfg).unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(!args.contains(&"-ngl".to_string()));
    }

    #[test]
    fn test_llm_runner_state_default_not_running() {
        let state = LlmRunnerState::new(LlmRunnerConfig::default());
        assert!(!state.is_running());
        let status = state.status();
        assert!(!status.running);
        assert!(status.pid.is_none());
        assert_eq!(status.base_url, "http://127.0.0.1:8080/v1");
        assert!(status.recent_logs.is_empty());
    }

    #[test]
    fn test_append_log_caps_buffer() {
        let state = LlmRunnerState::new(LlmRunnerConfig::default());
        // 直接塞入超过上限的日志，验证不会无限增长
        for i in 0..(LOG_BUFFER_LINES + 100) {
            state.append_log(format!("line {i}"));
        }
        let logs = state.logs.read().unwrap();
        assert_eq!(logs.len(), LOG_BUFFER_LINES);
        // 最早的日志已被淘汰，最后一条是最新写入的
        assert_eq!(logs.back().unwrap(), &format!("line {}", LOG_BUFFER_LINES + 99));
    }

    #[test]
    fn test_update_config_replaces_runtime_config() {
        let state = LlmRunnerState::new(LlmRunnerConfig::default());
        let mut cfg = LlmRunnerConfig::default();
        cfg.port = 9999;
        cfg.host = "0.0.0.0".to_string();
        state.update_config(cfg);
        assert_eq!(state.base_url(), "http://0.0.0.0:9999/v1");
    }

    #[test]
    fn test_base_url_reflects_config() {
        let mut cfg = LlmRunnerConfig::default();
        cfg.host = "192.168.1.5".to_string();
        cfg.port = 1234;
        let state = LlmRunnerState::new(cfg);
        assert_eq!(state.base_url(), "http://192.168.1.5:1234/v1");
    }

    #[test]
    fn test_base_url_uses_override_when_set() {
        let mut cfg = LlmRunnerConfig::default();
        cfg.runner_type = RUNNER_TYPE_LMS.to_string();
        cfg.host = "0.0.0.0".to_string(); // lms 模式下 host 不应影响
        cfg.port = 1234;
        cfg.base_url = "http://10.0.0.5:8080/v1".to_string();
        let state = LlmRunnerState::new(cfg);
        // 用户显式配置的 base_url 优先于派生默认值
        assert_eq!(state.base_url(), "http://10.0.0.5:8080/v1");
    }

    #[test]
    fn test_base_url_lms_default_ignores_host() {
        let mut cfg = LlmRunnerConfig::default();
        cfg.runner_type = RUNNER_TYPE_LMS.to_string();
        cfg.host = "0.0.0.0".to_string();
        cfg.port = 1234;
        let state = LlmRunnerState::new(cfg);
        // 未设置 override 时，lms 派生默认值固定为 127.0.0.1
        assert_eq!(state.base_url(), "http://127.0.0.1:1234/v1");
    }
}
