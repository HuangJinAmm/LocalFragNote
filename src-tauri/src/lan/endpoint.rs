//! iroh Endpoint 初始化与 mDNS 发现
//!
//! - SecretKey 持久化到 app_data_dir/lan_identity.key
//! - mDNS 通过 iroh-mdns-address-lookup 启用
//! - 展示名通过 instance_setting:lan_display_name 存储
//! - mDNS 发现代码在后台 task 中订阅 DiscoveryEvent 并更新 peers 缓存

use crate::lan::{LanError, LanState, PeerInfo, ALPN};
use iroh::endpoint::presets;
use iroh::{Endpoint, SecretKey};
use iroh_mdns_address_lookup::{DiscoveryEvent, MdnsAddressLookup};
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
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

    // 不在 builder 链中注册 MdnsAddressLookup，而是 bind 后手动构建并 add，
    // 这样可以保留 MdnsAddressLookup 的 clone 用于订阅 DiscoveryEvent
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret_key)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;

    let endpoint_id = endpoint.id();
    tracing::info!("LAN Endpoint bound, endpoint_id = {}", endpoint_id);

    // 手动构建 MdnsAddressLookup 并注册到 endpoint，保留 clone 用于订阅发现事件
    let mdns = MdnsAddressLookup::builder()
        .build(endpoint_id)
        .map_err(|e| LanError::Endpoint(e.to_string()))?;
    endpoint
        .address_lookup()
        .map_err(|e| LanError::Endpoint(e.to_string()))?
        .add(mdns.clone());
    tracing::info!("LAN mDNS address lookup registered");

    let display_name = DEFAULT_DISPLAY_NAME.to_string();
    let (shutdown_tx, _shutdown_rx) = tokio::sync::watch::channel(false);
    let state = Arc::new(LanState {
        endpoint,
        mdns,
        peers: RwLock::new(Vec::new()),
        display_name: RwLock::new(display_name),
        shutdown_tx,
    });

    Ok(state)
}

/// 从 LanState 获取本机 endpoint_id 的字符串表示
pub fn local_peer_id(state: &LanState) -> String {
    state.endpoint.id().to_string()
}

/// 当前 epoch seconds，用于 PeerInfo.last_seen
fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 启动 mDNS 发现代理：订阅 MdnsAddressLookup 的 DiscoveryEvent 流，
/// 发现 peer 时更新 peers 缓存并向前端推送 "lan:peers-changed" 事件。
///
/// 采用事件驱动模式（非轮询），mDNS 发现/过期时立即更新缓存。
pub fn spawn_mdns_discovery_loop(state: Arc<LanState>, app_handle: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        use tauri::Emitter;
        use tokio_stream::StreamExt;

        let mut events = state.mdns.subscribe().await;
        let mut shutdown_rx = state.shutdown_tx.subscribe();
        tracing::info!("LAN mDNS discovery loop started");

        loop {
            tokio::select! {
                biased;
                _ = shutdown_rx.changed() => {
                    tracing::info!("LAN mDNS discovery loop shutting down");
                    break;
                }
                event = events.next() => {
                    let Some(event) = event else { break };
                    let changed = match event {
                        DiscoveryEvent::Discovered { endpoint_info, .. } => {
                            let peer_id = endpoint_info.endpoint_id.to_string();
                            let display_name = peer_id_chars_prefix(&peer_id, 8);
                            let addrs: Vec<String> = endpoint_info
                                .data
                                .ip_addrs()
                                .map(|sa| sa.to_string())
                                .collect();
                            let relay_url = endpoint_info
                                .data
                                .relay_urls()
                                .next()
                                .map(|u| u.to_string());
                            let now = now_epoch_secs();

                            let mut peers = state.peers.write().await;
                            let info = PeerInfo {
                                peer_id: peer_id.clone(),
                                display_name,
                                addrs,
                                relay_url,
                                last_seen: now,
                            };
                            if let Some(existing) = peers.iter_mut().find(|p| p.peer_id == peer_id) {
                                *existing = info;
                            } else {
                                peers.push(info);
                            }
                            tracing::debug!(%peer_id, "LAN mDNS discovered peer");
                            true
                        }
                        DiscoveryEvent::Expired { endpoint_id } => {
                            let peer_id = endpoint_id.to_string();
                            let mut peers = state.peers.write().await;
                            let before = peers.len();
                            peers.retain(|p| p.peer_id != peer_id);
                            let removed = before != peers.len();
                            if removed {
                                tracing::debug!(%peer_id, "LAN mDNS peer expired");
                            }
                            removed
                        }
                        _ => false,
                    };

                    if changed {
                        let _ = app_handle.emit("lan:peers-changed", ());
                    }
                }
            }
        }

        tracing::info!("LAN mDNS discovery loop terminated");
    });
}

/// 取 peer_id（hex 字符串）的前 n 个字符作为占位展示名
fn peer_id_chars_prefix(peer_id: &str, n: usize) -> String {
    peer_id.chars().take(n).collect()
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
