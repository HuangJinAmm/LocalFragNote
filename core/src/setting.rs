//! 设置（app_setting + instance_setting）

use crate::error::{CoreError, CoreResult};
use moka::sync::Cache;
use rusqlite::{params, Connection};
use std::sync::Arc;

/// 应用设置（原 user_setting 简化版）
pub struct AppSettingStore {
    cache: Cache<String, String>,
}

impl AppSettingStore {
    pub fn new(cache: Cache<String, String>) -> Self {
        Self { cache }
    }

    pub fn get(&self, conn: &Connection, key: &str) -> CoreResult<Option<String>> {
        if let Some(v) = self.cache.get(key) {
            return Ok(Some(v));
        }
        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM app_setting WHERE key = ?",
                params![key],
                |row| row.get(0),
            )
            .map(Some)
            .unwrap_or(None);
        if let Some(ref v) = result {
            self.cache.insert(key.to_string(), v.clone());
        }
        Ok(result)
    }

    pub fn upsert(&self, conn: &Connection, key: &str, value: &str) -> CoreResult<()> {
        conn.execute(
            "INSERT INTO app_setting (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        self.cache.insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub fn delete(&self, conn: &Connection, key: &str) -> CoreResult<()> {
        conn.execute("DELETE FROM app_setting WHERE key = ?", params![key])?;
        self.cache.invalidate(key);
        Ok(())
    }
}

/// 实例设置（原 system_setting）
pub struct InstanceSettingStore {
    cache: Cache<String, String>,
}

impl InstanceSettingStore {
    pub fn new(cache: Cache<String, String>) -> Self {
        Self { cache }
    }

    pub fn get(&self, conn: &Connection, name: &str) -> CoreResult<Option<String>> {
        if let Some(v) = self.cache.get(name) {
            return Ok(Some(v));
        }
        let result: Option<String> = conn
            .query_row(
                "SELECT value FROM instance_setting WHERE name = ?",
                params![name],
                |row| row.get(0),
            )
            .map(Some)
            .unwrap_or(None);
        if let Some(ref v) = result {
            self.cache.insert(name.to_string(), v.clone());
        }
        Ok(result)
    }

    pub fn upsert(&self, conn: &Connection, name: &str, value: &str, description: &str) -> CoreResult<()> {
        conn.execute(
            "INSERT INTO instance_setting (name, value, description) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET value=excluded.value, description=excluded.description",
            params![name, value, description],
        )?;
        self.cache.insert(name.to_string(), value.to_string());
        Ok(())
    }

    pub fn delete(&self, conn: &Connection, name: &str) -> CoreResult<()> {
        conn.execute("DELETE FROM instance_setting WHERE name = ?", params![name])?;
        self.cache.invalidate(name);
        Ok(())
    }
}

/// 设置管理器（统一持有 app/instance 两类设置）
#[derive(Clone)]
pub struct SettingStore {
    pub app: Arc<AppSettingStore>,
    pub instance: Arc<InstanceSettingStore>,
}

impl SettingStore {
    pub fn new(app_cache: Cache<String, String>, instance_cache: Cache<String, String>) -> Self {
        Self {
            app: Arc::new(AppSettingStore::new(app_cache)),
            instance: Arc::new(InstanceSettingStore::new(instance_cache)),
        }
    }
}

/// 抹除"未找到"以方便上层处理
pub fn not_found(key: &str) -> CoreError {
    CoreError::NotFound(format!("setting {key}"))
}
