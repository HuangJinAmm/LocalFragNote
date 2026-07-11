//! 应用状态：持有 Store 与附件存储根目录

use crate::lan::LanState;
use memos_core::Store;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub store: Mutex<Store>,
    /// 附件本地存储根目录（app_data_dir/attachments）
    pub attachments_dir: std::path::PathBuf,
    /// LAN 模块运行时状态（启动失败则为 None，应用其他功能不受影响）
    pub lan: Option<Arc<LanState>>,
}

impl AppState {
    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().expect("Store Mutex poisoned")
    }

    /// 获取 LanState，若未初始化则返回错误
    pub fn lan(&self) -> Result<Arc<LanState>, crate::error::IpcError> {
        self.lan
            .clone()
            .ok_or_else(|| crate::error::IpcError::Lan("LAN module not initialized".into()))
    }
}
