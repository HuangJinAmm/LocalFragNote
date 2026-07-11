//! Integration test: two-endpoint RPC roundtrip.
//!
//! `test_two_endpoints_init` — non-ignored, verifies two endpoints initialize with different peer_ids.
//! `test_two_endpoints_profile_rpc` — #[ignore], requires real mDNS environment.

use iroh::endpoint::RecvStream;
use memos_app::lan::client::call_remote;
use memos_app::lan::endpoint::init_lan_state;
use memos_app::lan::protocol::{ok, Request, ResponseData};
use std::time::Duration;

/// Non-ignored test: two endpoints can initialize and have different peer_ids.
#[tokio::test]
async fn test_two_endpoints_init() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let state_a = init_lan_state(dir_a.path()).await.unwrap();
    let state_b = init_lan_state(dir_b.path()).await.unwrap();

    let id_a = state_a.endpoint.id();
    let id_b = state_b.endpoint.id();
    assert_ne!(id_a, id_b, "two endpoints should have different peer_ids");
}

/// Ignored test: full RPC roundtrip between two endpoints.
///
/// Requires a real mDNS environment (two endpoints on the same machine is fine).
/// Run manually: `cargo test --test lan_integration -- --ignored`
#[tokio::test]
#[ignore = "requires real mDNS environment, run manually"]
async fn test_two_endpoints_profile_rpc() {
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let state_a = init_lan_state(dir_a.path()).await.unwrap();
    let state_b = init_lan_state(dir_b.path()).await.unwrap();

    let peer_b_id = state_b.endpoint.id().to_string();

    // Start a custom accept loop on B (does not depend on tauri::AppHandle).
    // Responds to any request with a hardcoded GetProfile response.
    let b_handle = tokio::spawn(async move {
        loop {
            match state_b.endpoint.accept().await {
                Some(incoming) => {
                    let conn = match incoming.await {
                        Ok(c) => c,
                        Err(_) => continue,
                    };
                    tokio::spawn(async move {
                        loop {
                            match conn.accept_bi().await {
                                Ok((mut send, mut recv)) => {
                                    // Read request frame
                                    let req_bytes = match read_frame_async(&mut recv).await {
                                        Ok(b) => b,
                                        Err(_) => break,
                                    };
                                    let _req: Request = match serde_json::from_slice(&req_bytes) {
                                        Ok(r) => r,
                                        Err(_) => break,
                                    };
                                    // Hardcoded Profile response
                                    let resp = ok(ResponseData::Profile {
                                        display_name: "TestPeer".to_string(),
                                        public_memo_count: 0,
                                        tags: vec![],
                                    });
                                    let resp_json = serde_json::to_vec(&resp).unwrap();
                                    let len = resp_json.len() as u32;
                                    let _ = send.write_all(&len.to_be_bytes()).await;
                                    let _ = send.write_all(&resp_json).await;
                                    let _ = send.finish();
                                }
                                Err(_) => break,
                            }
                        }
                    });
                }
                None => break,
            }
        }
    });

    // Give mDNS time to broadcast (iroh connect resolves address via address lookup).
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Call GetProfile from A to B
    let data = call_remote(&state_a.endpoint, &peer_b_id, &Request::GetProfile)
        .await
        .expect("GetProfile should succeed");

    match data {
        ResponseData::Profile {
            display_name,
            public_memo_count,
            tags,
        } => {
            assert_eq!(display_name, "TestPeer");
            assert_eq!(public_memo_count, 0);
            assert!(tags.is_empty());
        }
        _ => panic!("expected Profile response"),
    }

    b_handle.abort();
}

/// Read a single frame: [4-byte big-endian u32 length][JSON payload].
async fn read_frame_async(r: &mut RecvStream) -> Result<Vec<u8>, ()> {
    let mut len_buf = [0u8; 4];
    r.read_exact(&mut len_buf).await.map_err(|_| ())?;
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload).await.map_err(|_| ())?;
    Ok(payload)
}
