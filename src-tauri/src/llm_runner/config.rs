//! 本地 LLM 启动器配置：持久化在 app_setting 表的 `llm_runner_config` key（JSON 字符串）

use memos_core::Store;
use serde::{Deserialize, Serialize};

/// 启动器类型常量
pub const RUNNER_TYPE_LLAMA_CPP: &str = "llama-cpp";
pub const RUNNER_TYPE_LMS: &str = "lms";

/// app_setting 中的 key
pub const CONFIG_KEY: &str = "llm_runner_config";

/// 本地 LLM 启动器配置
///
/// - `runner_type = "llama-cpp"`：前台长驻进程（如 `llama-server`、`llamafile`、`vllm` 等）
///   生命周期由本模块直接管理：spawn 后跟踪 PID，stop 时 kill 子进程
/// - `runner_type = "lms"`：守护模式（LM Studio CLI）
///   `lms server start` 会立即返回，再调用 `lms load <model>` 加载模型，
///   模型由 LM Studio 后台守护进程管理
///   stop 时调用 `lms server stop`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRunnerConfig {
    /// 启动器类型：`"llama-cpp"` | `"lms"`
    #[serde(default = "default_runner_type")]
    pub runner_type: String,

    /// 可执行文件路径，如 `"llama-server"`、`"./llamafile/llama-server.exe"`、`"lms"`
    #[serde(default = "default_executable_path")]
    pub executable_path: String,

    /// 模型路径（llama-cpp 用 `.gguf` 文件路径；lms 用 LM Studio 中的模型名）
    #[serde(default)]
    pub model_path: String,

    /// 监听 host，默认 `"127.0.0.1"`
    #[serde(default = "default_host")]
    pub host: String,

    /// 监听端口，默认 `8080`
    #[serde(default = "default_port")]
    pub port: u16,

    /// 上下文长度（llama-cpp 的 `-c` 参数），默认 `4096`
    #[serde(default = "default_context_size")]
    pub context_size: u32,

    /// GPU 层数（llama-cpp 的 `-ngl` 参数）；0 表示仅 CPU
    #[serde(default)]
    pub gpu_layers: u32,

    /// 附加 CLI 参数（按空白分隔透传给可执行文件）
    #[serde(default)]
    pub extra_args: String,

    /// 自定义 Base URL（OpenAI 兼容端点）。
    /// 留空表示按 runner 类型派生默认值：
    /// - `llama-cpp`：`http://{host}:{port}/v1`
    /// - `lms`：`http://127.0.0.1:{port}/v1`（lms server start 不支持 --host，
    ///   监听地址由 LM Studio 守护进程决定）
    #[serde(default)]
    pub base_url: String,

    /// 是否在应用启动时自动启动
    #[serde(default)]
    pub auto_start: bool,
}

fn default_runner_type() -> String {
    RUNNER_TYPE_LLAMA_CPP.to_string()
}
fn default_executable_path() -> String {
    "llama-server".to_string()
}
fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_context_size() -> u32 {
    4096
}

impl Default for LlmRunnerConfig {
    fn default() -> Self {
        Self {
            runner_type: default_runner_type(),
            executable_path: default_executable_path(),
            model_path: String::new(),
            host: default_host(),
            port: default_port(),
            context_size: default_context_size(),
            gpu_layers: 0,
            extra_args: String::new(),
            base_url: String::new(),
            auto_start: false,
        }
    }
}

/// 根据 runner 类型派生默认 Base URL。
/// - `llama-cpp`：`http://{host}:{port}/v1`
/// - `lms`：`http://127.0.0.1:{port}/v1`（lms server start 不支持 --host）
///
/// 若 `config.base_url` 非空，调用方应优先使用该字段而非本函数。
pub fn default_base_url(config: &LlmRunnerConfig) -> String {
    match config.runner_type.as_str() {
        RUNNER_TYPE_LMS => format!("http://127.0.0.1:{}/v1", config.port),
        _ => format!("http://{}:{}/v1", config.host, config.port),
    }
}

/// 返回有效的 Base URL：优先使用用户配置的 `base_url`，为空时回退到按 runner 类型派生的默认值。
pub fn effective_base_url(config: &LlmRunnerConfig) -> String {
    let trimmed = config.base_url.trim();
    if !trimmed.is_empty() {
        trimmed.trim_end_matches('/').to_string()
    } else {
        default_base_url(config)
    }
}

/// 从 app_setting 读取配置，缺失则返回默认值
pub fn load_config(store: &Store) -> LlmRunnerConfig {
    let json: Option<String> = store
        .with_conn(|c| store.setting.app.get(c, CONFIG_KEY))
        .unwrap_or(None);
    json.as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

/// 保存配置到 app_setting
pub fn save_config(store: &Store, config: &LlmRunnerConfig) -> memos_core::CoreResult<()> {
    let json = serde_json::to_string(config).map_err(|e| {
        memos_core::CoreError::Other(format!("序列化 LLM 启动器配置失败: {e}"))
    })?;
    store.with_conn(|c| store.setting.app.upsert(c, CONFIG_KEY, &json))?;
    Ok(())
}

/// 判断是否守护模式（lms）
pub fn is_daemon_runner(config: &LlmRunnerConfig) -> bool {
    config.runner_type == RUNNER_TYPE_LMS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let c = LlmRunnerConfig::default();
        assert_eq!(c.runner_type, RUNNER_TYPE_LLAMA_CPP);
        assert_eq!(c.executable_path, "llama-server");
        assert_eq!(c.host, "127.0.0.1");
        assert_eq!(c.port, 8080);
        assert_eq!(c.context_size, 4096);
        assert_eq!(c.gpu_layers, 0);
        assert!(!c.auto_start);
        assert!(c.model_path.is_empty());
        assert!(c.base_url.is_empty());
    }

    #[test]
    fn test_config_serde_roundtrip() {
        let c = LlmRunnerConfig {
            runner_type: RUNNER_TYPE_LMS.to_string(),
            executable_path: "lms".to_string(),
            model_path: "qwen2.5-7b-instruct".to_string(),
            host: "0.0.0.0".to_string(),
            port: 1234,
            context_size: 8192,
            gpu_layers: 99,
            extra_args: "--verbose --jinja".to_string(),
            base_url: "http://192.168.1.10:1234/v1".to_string(),
            auto_start: true,
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: LlmRunnerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(c.runner_type, back.runner_type);
        assert_eq!(c.executable_path, back.executable_path);
        assert_eq!(c.model_path, back.model_path);
        assert_eq!(c.host, back.host);
        assert_eq!(c.port, back.port);
        assert_eq!(c.context_size, back.context_size);
        assert_eq!(c.gpu_layers, back.gpu_layers);
        assert_eq!(c.extra_args, back.extra_args);
        assert_eq!(c.base_url, back.base_url);
        assert_eq!(c.auto_start, back.auto_start);
    }

    #[test]
    fn test_partial_json_uses_defaults() {
        // 缺失字段应使用 serde default 函数（包括 base_url 默认为空）
        let json = r#"{"model_path":"/models/test.gguf","port":9999}"#;
        let c: LlmRunnerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(c.model_path, "/models/test.gguf");
        assert_eq!(c.port, 9999);
        assert_eq!(c.runner_type, RUNNER_TYPE_LLAMA_CPP); // 默认
        assert_eq!(c.host, "127.0.0.1"); // 默认
        assert_eq!(c.context_size, 4096); // 默认
        assert_eq!(c.executable_path, "llama-server"); // 默认
        assert!(c.base_url.is_empty()); // 默认
    }

    #[test]
    fn test_load_config_default_when_absent() {
        let store = Store::open(":memory:").unwrap();
        let c = load_config(&store);
        assert_eq!(c.port, 8080);
        assert_eq!(c.runner_type, RUNNER_TYPE_LLAMA_CPP);
    }

    #[test]
    fn test_save_and_load_config() {
        let store = Store::open(":memory:").unwrap();
        let mut c = LlmRunnerConfig::default();
        c.port = 9999;
        c.model_path = "/models/test.gguf".to_string();
        c.gpu_layers = 33;
        c.auto_start = true;
        c.base_url = "http://example.com:9999/v1".to_string();
        save_config(&store, &c).unwrap();
        let loaded = load_config(&store);
        assert_eq!(loaded.port, 9999);
        assert_eq!(loaded.model_path, "/models/test.gguf");
        assert_eq!(loaded.gpu_layers, 33);
        assert!(loaded.auto_start);
        assert_eq!(loaded.base_url, "http://example.com:9999/v1");
    }

    #[test]
    fn test_is_daemon_runner() {
        let mut c = LlmRunnerConfig::default();
        assert!(!is_daemon_runner(&c));
        c.runner_type = RUNNER_TYPE_LMS.to_string();
        assert!(is_daemon_runner(&c));
    }

    #[test]
    fn test_default_base_url_llama_cpp() {
        let mut c = LlmRunnerConfig::default();
        c.host = "0.0.0.0".to_string();
        c.port = 9999;
        assert_eq!(default_base_url(&c), "http://0.0.0.0:9999/v1");
    }

    #[test]
    fn test_default_base_url_lms_ignores_host() {
        let mut c = LlmRunnerConfig::default();
        c.runner_type = RUNNER_TYPE_LMS.to_string();
        c.host = "0.0.0.0".to_string(); // lms 模式下 host 不影响 base url
        c.port = 1234;
        assert_eq!(default_base_url(&c), "http://127.0.0.1:1234/v1");
    }

    #[test]
    fn test_effective_base_url_uses_override() {
        let mut c = LlmRunnerConfig::default();
        c.runner_type = RUNNER_TYPE_LMS.to_string();
        c.port = 1234;
        c.base_url = "http://10.0.0.5:8080/v1/".to_string();
        // override 优先，尾部斜杠被去除
        assert_eq!(effective_base_url(&c), "http://10.0.0.5:8080/v1");
    }

    #[test]
    fn test_effective_base_url_falls_back_to_default() {
        let mut c = LlmRunnerConfig::default();
        c.runner_type = RUNNER_TYPE_LMS.to_string();
        c.port = 1234;
        c.base_url = "   ".to_string(); // 空白视为未设置
        assert_eq!(effective_base_url(&c), "http://127.0.0.1:1234/v1");
    }
}
