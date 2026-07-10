//! 缓存层（moka）
//!
//! 缓存 instance_setting 与 app_setting，与原 Go 版本语义对齐。

use moka::sync::Cache;
use std::time::Duration;

/// 缓存配置
pub struct CacheConfig {
    pub max_capacity: u64,
    pub ttl: Duration,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_capacity: 1000,
            ttl: Duration::from_secs(600), // 10 分钟
        }
    }
}

/// 创建一个带 TTL 的字符串缓存
pub fn new_string_cache(config: &CacheConfig) -> Cache<String, String> {
    Cache::builder()
        .max_capacity(config.max_capacity)
        .time_to_live(config.ttl)
        .build()
}
