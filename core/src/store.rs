//! Store facade：统一管理 SQLite 连接与缓存

use crate::cache::{new_string_cache, CacheConfig};
use crate::error::CoreResult;
use crate::migration;
use crate::setting::SettingStore;
use rusqlite::Connection;
use std::path::Path;
use std::sync::{Mutex, Once};

/// 确保 sqlite-vec 扩展已注册（全局只需一次）
/// 必须在打开任何 Connection 之前调用，之后所有连接自动加载 vec0 虚拟表
static VEC_EXT_INIT: Once = Once::new();

fn ensure_vec_extension_loaded() {
    VEC_EXT_INIT.call_once(|| {
        // 通过 sqlite3_auto_extension 注册 sqlite-vec 为自动加载扩展
        // 之后 Connection::open* 打开的连接都会自动启用 vec_* SQL 函数与 vec0 虚拟表
        use rusqlite::ffi::sqlite3_auto_extension;
        use sqlite_vec::sqlite3_vec_init;
        unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute(sqlite3_vec_init as *const ())));
        }
    });
}

/// Store 是应用的数据层入口
pub struct Store {
    conn: Mutex<Connection>,
    pub setting: SettingStore,
}

impl Store {
    /// 打开/创建数据库并执行迁移
    pub fn open<P: AsRef<Path>>(db_path: P) -> CoreResult<Self> {
        ensure_vec_extension_loaded();
        let conn = Connection::open(db_path)?;
        // 启用外键支持
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        // 迁移（V3 含 vec0 建表语句，必须在扩展注册之后）
        let mut conn_mut = conn;
        migration::run(&mut conn_mut)?;
        let conn = conn_mut;

        let cfg = CacheConfig::default();
        let app_cache = new_string_cache(&cfg);
        let instance_cache = new_string_cache(&cfg);
        let setting = SettingStore::new(app_cache, instance_cache);

        Ok(Self {
            conn: Mutex::new(conn),
            setting,
        })
    }

    /// 内存数据库（用于测试）
    pub fn open_in_memory() -> CoreResult<Self> {
        ensure_vec_extension_loaded();
        let conn = Connection::open_in_memory()?;
        let mut conn_mut = conn;
        migration::run(&mut conn_mut)?;
        let conn = conn_mut;

        let cfg = CacheConfig::default();
        let app_cache = new_string_cache(&cfg);
        let instance_cache = new_string_cache(&cfg);
        let setting = SettingStore::new(app_cache, instance_cache);

        Ok(Self {
            conn: Mutex::new(conn),
            setting,
        })
    }

    /// 获取连接（锁住内部 Mutex）
    pub fn with_conn<F, T>(&self, f: F) -> CoreResult<T>
    where
        F: FnOnce(&Connection) -> CoreResult<T>,
    {
        let conn = self.conn.lock().expect("Mutex poisoned");
        f(&conn)
    }

    /// 获取可变连接（用于事务）
    pub fn with_conn_mut<F, T>(&self, f: F) -> CoreResult<T>
    where
        F: FnOnce(&mut Connection) -> CoreResult<T>,
    {
        let mut conn = self.conn.lock().expect("Mutex poisoned");
        f(&mut conn)
    }
}

/// 公开的连接锁方法（测试用）
impl Store {
    pub fn lock_conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("Mutex poisoned")
    }
}
