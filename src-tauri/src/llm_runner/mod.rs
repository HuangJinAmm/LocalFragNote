//! 本地 LLM 启动器：通过 llama.cpp 的 `llama-server` 或 LM Studio 的 `lms` CLI
//! 启动 OpenAI 兼容的本地服务端点（`/v1/chat/completions`）。
//!
//! 模块组成：
//! - [`config`]：配置持久化（app_setting 表的 `llm_runner_config` key）
//! - [`runner`]：进程管理（spawn / kill / 状态查询 / 日志捕获）

pub mod config;
pub mod runner;

pub use config::{
    default_base_url, effective_base_url, load_config, save_config, LlmRunnerConfig,
    RUNNER_TYPE_LMS,
};
pub use runner::{LlmRunnerError, LlmRunnerState, LlmRunnerStatus};
