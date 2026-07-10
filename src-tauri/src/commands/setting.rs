//! Setting 相关 IPC 命令（app_setting + instance_setting + stats）

use crate::error::{IpcError, IpcResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};

// ---------- Storage Config ----------

/// 存储配置：决定附件的存储类型、本地路径、文件名模板
///
/// 持久化在 `app_setting` 表的 `storage_config` key（JSON 字符串）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    /// 存储类型：`"AUTO"` | `"DATABASE"` | `"LOCAL"`
    ///
    /// - AUTO：根据文件大小自动选择（>= auto_threshold 走 LOCAL，否则 DATABASE）
    /// - DATABASE：所有附件 blob 存 SQLite
    /// - LOCAL：所有附件存本地文件系统
    #[serde(default = "default_storage_type")]
    pub storage_type: String,

    /// 本地存储相对路径（相对 data_dir），默认 `"attachments"`
    ///
    /// 支持绝对路径；相对路径会写入 app data 目录下。
    #[serde(default = "default_local_storage_path")]
    pub local_storage_path: String,

    /// 文件名模板，支持 `{uid}`、`{filename}`、`{timestamp}`、`{uuid}`
    ///
    /// 默认 `"{uid}_{filename}"`。可含子目录，如 `"assets/{timestamp}_{uuid}_{filename}"`。
    #[serde(default = "default_filepath_template")]
    pub filepath_template: String,

    /// AUTO 模式阈值（字节），默认 1MB
    #[serde(default = "default_auto_threshold")]
    pub auto_threshold: u64,
}

fn default_storage_type() -> String {
    "AUTO".to_string()
}
fn default_local_storage_path() -> String {
    "attachments".to_string()
}
fn default_filepath_template() -> String {
    "{uid}_{filename}".to_string()
}
fn default_auto_threshold() -> u64 {
    1024 * 1024
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            storage_type: default_storage_type(),
            local_storage_path: default_local_storage_path(),
            filepath_template: default_filepath_template(),
            auto_threshold: default_auto_threshold(),
        }
    }
}

const STORAGE_CONFIG_KEY: &str = "storage_config";

/// 从 app_setting 读取存储配置，缺失则返回默认值
pub fn load_storage_config(store: &memos_core::Store) -> StorageConfig {
    let json: Option<String> = store
        .with_conn(|c| store.setting.app.get(c, STORAGE_CONFIG_KEY))
        .unwrap_or(None);
    json.as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default()
}

/// 获取存储配置
#[tauri::command]
pub fn get_storage_config(state: tauri::State<'_, AppState>) -> IpcResult<StorageConfig> {
    let store = state.store();
    Ok(load_storage_config(&store))
}

/// 更新存储配置
#[tauri::command]
pub fn update_storage_config(
    state: tauri::State<'_, AppState>,
    req: StorageConfig,
) -> IpcResult<StorageConfig> {
    // 校验模板不为空
    let template = req.filepath_template.trim().to_string();
    if template.is_empty() {
        return Err(IpcError::BadRequest("文件名模板不能为空".into()));
    }
    // 拒绝模板中的路径穿越
    if template.contains("..") {
        return Err(IpcError::BadRequest("文件名模板包含非法路径".into()));
    }
    let mut req = req;
    req.filepath_template = template;

    let json = serde_json::to_string(&req)
        .map_err(|e| IpcError::Internal(format!("序列化存储配置失败: {e}")))?;
    let store = state.store();
    store.with_conn(|c| store.setting.app.upsert(c, STORAGE_CONFIG_KEY, &json))?;
    Ok(req)
}

// ---------- App Setting ----------

#[derive(Debug, Deserialize)]
pub struct UpsertAppSettingRequest {
    pub key: String,
    pub value: String,
}

#[tauri::command]
pub fn get_app_setting(
    state: tauri::State<'_, AppState>,
    key: String,
) -> IpcResult<Option<String>> {
    let store = state.store();
    Ok(store.with_conn(|c| store.setting.app.get(c, &key))?)
}

#[tauri::command]
pub fn upsert_app_setting(
    state: tauri::State<'_, AppState>,
    req: UpsertAppSettingRequest,
) -> IpcResult<()> {
    let store = state.store();
    store.with_conn(|c| store.setting.app.upsert(c, &req.key, &req.value))?;
    Ok(())
}

#[tauri::command]
pub fn delete_app_setting(state: tauri::State<'_, AppState>, key: String) -> IpcResult<()> {
    let store = state.store();
    store.with_conn(|c| store.setting.app.delete(c, &key))?;
    Ok(())
}

// ---------- Instance Setting ----------

#[derive(Debug, Deserialize)]
pub struct UpsertInstanceSettingRequest {
    pub name: String,
    pub value: String,
    #[serde(default)]
    pub description: String,
}

#[tauri::command]
pub fn get_instance_setting(
    state: tauri::State<'_, AppState>,
    name: String,
) -> IpcResult<Option<String>> {
    let store = state.store();
    Ok(store.with_conn(|c| store.setting.instance.get(c, &name))?)
}

#[tauri::command]
pub fn upsert_instance_setting(
    state: tauri::State<'_, AppState>,
    req: UpsertInstanceSettingRequest,
) -> IpcResult<()> {
    let store = state.store();
    store.with_conn(|c| {
        store
            .setting
            .instance
            .upsert(c, &req.name, &req.value, &req.description)
    })?;
    Ok(())
}

#[tauri::command]
pub fn delete_instance_setting(state: tauri::State<'_, AppState>, name: String) -> IpcResult<()> {
    let store = state.store();
    store.with_conn(|c| store.setting.instance.delete(c, &name))?;
    Ok(())
}

// ---------- Instance Stats ----------

/// 时间戳
#[derive(Debug, Serialize)]
pub struct Timestamp {
    pub seconds: i64,
    pub nanos: i32,
}

/// 数据库统计
#[derive(Debug, Serialize)]
pub struct DatabaseStats {
    pub driver: String,
    pub size_bytes: i64,
}

/// 资源统计响应
#[derive(Debug, Serialize)]
pub struct InstanceStatsResponse {
    pub generated_time: Timestamp,
    pub database: DatabaseStats,
    pub local_storage_bytes: i64,
}

/// 获取实例资源统计：数据库大小、本地存储大小
#[tauri::command]
pub fn get_instance_stats(
    state: tauri::State<'_, AppState>,
) -> IpcResult<InstanceStatsResponse> {
    let store = state.store();
    // SQLite 数据库文件大小
    let db_size = store.with_conn(|c| -> memos_core::CoreResult<i64> {
        // 通过 PRAGMA page_count * page_size 获取数据库大小
        let page_count: i64 = c.query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let page_size: i64 = c.query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok(page_count * page_size)
    })?;

    // 本地附件目录大小
    let local_storage_bytes = dir_size(&state.attachments_dir);

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    Ok(InstanceStatsResponse {
        generated_time: Timestamp { seconds: now, nanos: 0 },
        database: DatabaseStats {
            driver: "SQLite".to_string(),
            size_bytes: db_size,
        },
        local_storage_bytes,
    })
}

/// 递归计算目录大小
fn dir_size(path: &std::path::Path) -> i64 {
    let mut total: i64 = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total += meta.len() as i64;
                } else if meta.is_dir() {
                    total += dir_size(&entry.path());
                }
            }
        }
    }
    total
}
