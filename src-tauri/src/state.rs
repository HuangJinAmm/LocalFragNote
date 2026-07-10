//! 应用状态：持有 Store 与附件存储根目录

use memos_core::Store;
use std::sync::Mutex;

pub struct AppState {
    pub store: Mutex<Store>,
    /// 附件本地存储根目录（app_data_dir/attachments）
    pub attachments_dir: std::path::PathBuf,
}

impl AppState {
    pub fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().expect("Store Mutex poisoned")
    }
}
