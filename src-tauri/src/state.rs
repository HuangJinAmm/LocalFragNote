//! 应用状态：持有 Store 与附件存储根目录

use crate::lan::LanState;
use crate::llm_runner::LlmRunnerState;
use crate::mcp::McpState;
use memos_core::Store;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub struct AppState {
    pub store: Mutex<Store>,
    /// 附件本地存储根目录（用户目录/localFragNote/attachments）
    pub attachments_dir: std::path::PathBuf,
    /// LAN 模块运行时状态，支持在设置页里手动启停
    pub lan: RwLock<Option<Arc<LanState>>>,
    /// 本地 LLM 启动器运行时状态，支持在设置页里手动启停
    pub llm: RwLock<Option<Arc<LlmRunnerState>>>,
    /// 本地 MCP 服务器运行时状态，支持在设置页里手动启停
    pub mcp: RwLock<Option<Arc<McpState>>>,
    /// 全局 shutdown 标志：app 退出时设为 true，后台任务据此提前终止
    pub shutdown: AtomicBool,
    /// 保证退出清理只执行一次，避免重复触发退出流程
    pub cleanup_started: AtomicBool,
}

impl AppState {
    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().expect("Store Mutex poisoned")
    }

    /// 获取 LanState，若未初始化则返回错误
    pub fn lan(&self) -> Result<Arc<LanState>, crate::error::IpcError> {
        self.lan
            .read()
            .expect("LAN RwLock poisoned")
            .clone()
            .ok_or_else(|| crate::error::IpcError::Lan("LAN module not initialized".into()))
    }

    /// 覆盖当前 LAN 运行时状态。
    pub fn set_lan(&self, lan: Option<Arc<LanState>>) {
        *self.lan.write().expect("LAN RwLock poisoned") = lan;
    }

    /// 取出当前 LAN 运行时状态并清空。
    pub fn take_lan(&self) -> Option<Arc<LanState>> {
        self.lan.write().expect("LAN RwLock poisoned").take()
    }

    /// 标记应用已进入退出清理阶段，返回 true 表示当前调用者负责执行清理。
    pub fn begin_shutdown(&self) -> bool {
        self.cleanup_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}
