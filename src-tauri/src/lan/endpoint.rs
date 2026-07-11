//! iroh Endpoint 初始化与 mDNS 发现
//!
//! - SecretKey 持久化到 app_data_dir/lan_identity.key
//! - mDNS 通过 iroh-mdns-address-lookup 启用
//! - 展示名通过 instance_setting:lan_display_name 存储

use crate::lan::{LanError, LanState, ALPN};
use iroh::endpoint::presets;
use iroh::{Endpoint, SecretKey};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 默认展示名
const DEFAULT_DISPLAY_NAME: &str = "LocalFragNote";
/// 展示名在 instance_setting 的 key
pub const DISPLAY_NAME_KEY: &str = "lan_display_name";
/// ACL 规则在 app_setting 的 key
pub const ACL_RULES_KEY: &str = "lan_acl_rules";

/// 加载或创建 SecretKey，持久化到文件
fn load_or_create_secret(path: &Path) -> Result<SecretKey, LanError> {
    if path.exists() {
        let bytes = std::fs::read(path)?;
        let arr: [u8; 32] = bytes
            .as_slice()
            .try_into()
            .map_err(|_| LanError::LocalStore("invalid secret key file".into()))?;
        Ok(SecretKey::from_bytes(&arr))
    } else {
        let secret = SecretKey::generate();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, secret.to_bytes())?;
        Ok(secret)
    }
}

/// 初始化 LanState：创建 Endpoint，启用 mDNS
pub async fn init_lan_state(data_dir: &Path) -> Result<Arc<LanState>, LanError> {
    let key_path = data_dir.join("lan_identity.key");
    let secret_key = load_or_create_secret(&key_path)?;
    tracing::info!("LAN Endpoint secret key loaded from {}", key_path.display());

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![ALPN.to_vec()])
        .address_lookup(MdnsAddressLookup::builder())
        .bind()
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;

    let endpoint_id = endpoint.id();
    tracing::info!("LAN Endpoint bound, endpoint_id = {}", endpoint_id);

    let display_name = DEFAULT_DISPLAY_NAME.to_string();
    let state = Arc::new(LanState {
        endpoint,
        peers: RwLock::new(Vec::new()),
        display_name: RwLock::new(display_name),
    });

    Ok(state)
}

/// 从 LanState 获取本机 endpoint_id 的字符串表示
pub fn local_peer_id(state: &LanState) -> String {
    state.endpoint.id().to_string()
}

/// 从 instance_setting 读取展示名
pub fn load_display_name(store: &memos_core::Store) -> String {
    store
        .with_conn(|c| store.setting.instance.get(c, DISPLAY_NAME_KEY))
        .unwrap_or(None)
        .unwrap_or_else(|| DEFAULT_DISPLAY_NAME.to_string())
}

/// 保存展示名到 instance_setting
pub fn save_display_name(store: &memos_core::Store, name: &str) -> Result<(), LanError> {
    store
        .with_conn(|c| {
            store
                .setting
                .instance
                .upsert(c, DISPLAY_NAME_KEY, name, "")
        })
        .map_err(|e| LanError::LocalStore(e.to_string()))?;
    Ok(())
}

/// 从 app_setting 读取 ACL 规则 JSON
pub fn load_acl_rules_json(store: &memos_core::Store) -> String {
    store
        .with_conn(|c| store.setting.app.get(c, ACL_RULES_KEY))
        .unwrap_or(None)
        .unwrap_or_else(|| "[]".to_string())
}

/// 保存 ACL 规则 JSON 到 app_setting
pub fn save_acl_rules_json(store: &memos_core::Store, json: &str) -> Result<(), LanError> {
    store
        .with_conn(|c| store.setting.app.upsert(c, ACL_RULES_KEY, json))
        .map_err(|e| LanError::LocalStore(e.to_string()))?;
    Ok(())
}
