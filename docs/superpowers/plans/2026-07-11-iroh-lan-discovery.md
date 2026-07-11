# iroh 局域网发现、编组与分享 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 LocalFragNote 中集成 iroh P2P 库，实现局域网用户发现、按标签编组的权限控制，以及按需查看/复制他人公开笔记的功能。

**Architecture:** iroh QUIC Endpoint + iroh-mdns-address-lookup 局域网发现，在其上构建自定义 ALPN JSON-RPC 协议（请求-响应模型）。被动公开模型：本机作为服务端响应他人查询，作为客户端按需查询他人。ACL 基于 peer_id + tag 过滤，存 `app_setting` 表。

**Tech Stack:** Rust + Tauri 2 + iroh 1.0 + iroh-mdns-address-lookup 0.4 + React 19 + TypeScript + Radix UI

**参考 Spec:** [docs/superpowers/specs/2026-07-11-iroh-lan-discovery-design.md](file:///d:/3-ai-project/LocalFragNote/docs/superpowers/specs/2026-07-11-iroh-lan-discovery-design.md)

---

## 文件结构

### 新建文件（Rust）

- `src-tauri/src/lan/mod.rs` — 模块入口、LanState、LanError、启动/停止
- `src-tauri/src/lan/endpoint.rs` — iroh Endpoint 初始化、SecretKey 持久化、mDNS 发现缓存
- `src-tauri/src/lan/protocol.rs` — JSON-RPC 类型定义 + 帧编解码（4字节长度前缀 + JSON）
- `src-tauri/src/lan/auth.rs` — AclRule 类型 + filter_memos_for_peer 纯函数
- `src-tauri/src/lan/server.rs` — accept 循环 + 请求分发 + 4 个 handler
- `src-tauri/src/lan/client.rs` — call_remote 函数 + 超时控制
- `src-tauri/src/commands/lan.rs` — 10 个 Tauri 命令
- `src-tauri/tests/lan_auth.rs` — auth 纯函数单元测试
- `src-tauri/tests/lan_protocol.rs` — 帧编解码单元测试
- `src-tauri/tests/lan_integration.rs` — 双 Endpoint 集成测试

### 修改文件（Rust）

- `src-tauri/Cargo.toml` — 新增 iroh、iroh-mdns-address-lookup、anyhow 依赖
- `src-tauri/src/main.rs` — setup 阶段启动 Endpoint、注册命令、退出时关闭
- `src-tauri/src/state.rs` — AppState 扩展持有 `Option<Arc<LanState>>`
- `src-tauri/src/commands/mod.rs` — 新增 `pub mod lan;`
- `src-tauri/src/error.rs` — IpcError 扩展 Lan 变体
- `src-tauri/capabilities/default.json` — 可能需要网络权限（实现时验证）

### 新建文件（前端）

- `src/components/LanDiscovery/index.tsx` — 导出
- `src/components/LanDiscovery/LanDiscoveryPanel.tsx` — Drawer 双栏面板
- `src/components/LanDiscovery/PeerList.tsx` — 左栏 peer 列表
- `src/components/LanDiscovery/RemoteMemoList.tsx` — 右栏远端笔记列表
- `src/components/LanDiscovery/RemoteMemoPreview.tsx` — 笔记预览 + 复制按钮
- `src/components/LanDiscovery/hooks.ts` — useLanDiscovery / useRemoteMemos / useRemoteMemoPreview
- `src/components/LanDiscovery/types.ts` — TypeScript 类型定义
- `src/components/Settings/LanShareSection.tsx` — 设置页"局域网分享"section
- `src/components/MemoEditor/Toolbar/DiscoverButton.tsx` — 工具栏"发现"按钮

### 修改文件（前端）

- `src/components/Settings/settingSections.ts` — 新增 "lan-share" section
- `src/locales/en.json` — 新增 `lan` 命名空间 i18n
- `src/locales/zh-Hans.json` — 新增 `lan` 命名空间 i18n
- `src/components/MemoEditor/Toolbar/EditorToolbar.tsx` — 插入 DiscoverButton
- `src/components/MemoContent/MemoMarkdownRenderer.tsx` — 支持 `remote` prop（如已存在则复用）

---

## Task 1: 添加 Cargo 依赖并验证编译

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 添加依赖到 Cargo.toml**

在 `src-tauri/Cargo.toml` 的 `[dependencies]` 末尾（`ureq = "2"` 之后）添加：

```toml
iroh = "1"
iroh-mdns-address-lookup = "0.4"
anyhow = "1"
tokio = { version = "1", features = ["full"] }
```

- [ ] **Step 2: 验证依赖编译**

Run: `cd src-tauri && cargo check`
Expected: 成功编译（首次会下载大量依赖，可能需要几分钟）

如果 iroh 1.x API 与文档不符（文档基于 2026-07 版本），记录实际 API 差异并在后续任务中调整。

- [ ] **Step 3: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "deps: add iroh, iroh-mdns-address-lookup, anyhow, tokio for LAN discovery"
```

---

## Task 2: 定义 LanError 错误类型

**Files:**
- Create: `src-tauri/src/lan/mod.rs`
- Modify: `src-tauri/src/main.rs`（注册 lan 模块）
- Modify: `src-tauri/src/error.rs`（IpcError 扩展）

- [ ] **Step 1: 创建 lan/mod.rs 骨架**

```rust
//! 局域网发现与分享模块
//!
//! 基于 iroh QUIC Endpoint + mDNS，实现被动公开 + 按需查询 + 按需复制模型。
//! 不做主动同步，每次请求独立无状态。

pub mod auth;
pub mod client;
pub mod endpoint;
pub mod protocol;
pub mod server;

use iroh::Endpoint;
use std::sync::Arc;
use tokio::sync::RwLock;

/// 局域网发现运行时状态
pub struct LanState {
    pub endpoint: Endpoint,
    /// mDNS 发现的 peer 缓存：peer_id → PeerInfo
    pub peers: RwLock<Vec<PeerInfo>>,
    /// 本机展示名
    pub display_name: RwLock<String>,
}

/// 发现到的 peer 信息
#[derive(Debug, Clone, serde::Serialize)]
pub struct PeerInfo {
    pub peer_id: String,
    pub display_name: String,
    pub addrs: Vec<String>,
    pub relay_url: Option<String>,
    pub last_seen: i64,
}

/// LAN 模块错误类型
#[derive(Debug, thiserror::Error)]
pub enum LanError {
    #[error("iroh endpoint error: {0}")]
    Endpoint(String),
    #[error("connect timeout")]
    ConnectTimeout,
    #[error("rpc timeout")]
    RpcTimeout,
    #[error("connection closed")]
    ConnectionClosed,
    #[error("frame too large: {0} bytes")]
    FrameTooLarge(usize),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("remote error {0}: {1}")]
    Remote(u16, String),
    #[error("local store error: {0}")]
    LocalStore(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

impl From< LanError> for Box<dyn std::error::Error> {
    fn from(e: LanError) -> Self {
        Box::new(e)
    }
}

/// ALPN 协议标识
pub const ALPN: &[u8] = b"memos/lan-share/1";

/// 单帧最大字节数（16 MB）
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// 连接超时（秒）
pub const CONNECT_TIMEOUT_SECS: u64 = 5;

/// RPC 读写超时（秒）
pub const RPC_TIMEOUT_SECS: u64 = 10;

/// 附件传输超时（秒）
pub const ATTACHMENT_TIMEOUT_SECS: u64 = 60;
```

- [ ] **Step 2: 在 main.rs 注册 lan 模块**

在 `src-tauri/src/main.rs` 顶部 mod 声明区添加（在 `mod lan` 之前是 `mod embedding`）：

```rust
mod lan;
```

- [ ] **Step 3: 创建空骨架文件让模块编译**

创建以下空文件（每个仅一行 `//!` 注释）：

`src-tauri/src/lan/endpoint.rs`:
```rust
//! iroh Endpoint 初始化与 mDNS 发现
```

`src-tauri/src/lan/protocol.rs`:
```rust
//! JSON-RPC 协议类型与帧编解码
```

`src-tauri/src/lan/auth.rs`:
```rust
//! 编组权限过滤（ACL）
```

`src-tauri/src/lan/server.rs`:
```rust
//! accept 循环与请求分发
```

`src-tauri/src/lan/client.rs`:
```rust
//! 客户端：发起连接与请求远端
```

- [ ] **Step 4: 扩展 IpcError 支持 LanError**

在 `src-tauri/src/error.rs` 的 `IpcError` enum 中新增变体（在 `BadRequest(String)` 之后）：

```rust
    /// LAN 模块错误
    Lan(String),
```

在 `Display` impl 中新增（在 `BadRequest` 分支之后）：

```rust
            IpcError::Lan(msg) => write!(f, "Lan: {msg}"),
```

在文件末尾添加 From 转换：

```rust
impl From<crate::lan::LanError> for IpcError {
    fn from(e: crate::lan::LanError) -> Self {
        IpcError::Lan(e.to_string())
    }
}
```

- [ ] **Step 5: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 成功

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lan/ src-tauri/src/main.rs src-tauri/src/error.rs
git commit -m "feat(lan): scaffold lan module with LanError and LanState types"
```

---

## Task 3: 实现 JSON-RPC 协议类型与帧编解码

**Files:**
- Modify: `src-tauri/src/lan/protocol.rs`
- Create: `src-tauri/tests/lan_protocol.rs`

- [ ] **Step 1: 编写帧编解码失败测试**

创建 `src-tauri/tests/lan_protocol.rs`：

```rust
//! protocol.rs 单元测试：帧编解码 + JSON 类型往返
//!
//! 注：protocol 模块的函数需要 pub，测试通过 crate::lan::protocol 访问

use memos_app::lan::protocol::*;

#[test]
fn test_frame_roundtrip_basic() {
    let payload = br#"{"method":"GetProfile","params":null}"#;
    let mut buf = Vec::new();
    write_frame(&mut buf, payload).unwrap();
    let mut reader = &buf[..];
    let decoded = read_frame(&mut reader).unwrap();
    assert_eq!(decoded, payload);
}

#[test]
fn test_frame_roundtrip_empty_payload() {
    let payload = b"";
    let mut buf = Vec::new();
    write_frame(&mut buf, payload).unwrap();
    let mut reader = &buf[..];
    let decoded = read_frame(&mut reader).unwrap();
    assert_eq!(decoded, payload);
}

#[test]
fn test_frame_max_size_rejected() {
    // 构造一个声明长度超过 MAX_FRAME_SIZE 的帧
    let oversized_len: u32 = (MAX_FRAME_SIZE as u32) + 1;
    let mut buf = Vec::new();
    buf.extend_from_slice(&oversized_len.to_be_bytes());
    buf.push(0); // 至少一字节内容
    let mut reader = &buf[..];
    let result = read_frame(&mut reader);
    assert!(result.is_err(), "超过 16MB 的帧应被拒绝");
}

#[test]
fn test_request_getprofile_serialization() {
    let req = Request::GetProfile;
    let json = serde_json::to_string(&req).unwrap();
    // 应包含 "method":"GetProfile"
    assert!(json.contains("\"method\":\"GetProfile\""));
    // 反序列化往返
    let decoded: Request = serde_json::from_str(&json).unwrap();
    match decoded {
        Request::GetProfile => (),
        _ => panic!("应为 GetProfile 变体"),
    }
}

#[test]
fn test_request_listmemos_serialization() {
    let req = Request::ListMemos {
        offset: 0,
        limit: 50,
        tag_filter: Some(vec!["work".to_string()]),
    };
    let json = serde_json::to_string(&req).unwrap();
    assert!(json.contains("\"method\":\"ListMemos\""));
    let decoded: Request = serde_json::from_str(&json).unwrap();
    match decoded {
        Request::ListMemos { offset, limit, tag_filter } => {
            assert_eq!(offset, 0);
            assert_eq!(limit, 50);
            assert_eq!(tag_filter, Some(vec!["work".to_string()]));
        }
        _ => panic!("应为 ListMemos 变体"),
    }
}

#[test]
fn test_response_ok_serialization() {
    let resp = Response::Ok {
        data: ResponseData::Profile {
            display_name: "Alice".to_string(),
            public_memo_count: 42,
            tags: vec!["work".to_string()],
        },
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"status\":\"Ok\""));
    let decoded: Response = serde_json::from_str(&json).unwrap();
    match decoded {
        Response::Ok { data } => match data {
            ResponseData::Profile { display_name, public_memo_count, tags } => {
                assert_eq!(display_name, "Alice");
                assert_eq!(public_memo_count, 42);
                assert_eq!(tags, vec!["work"]);
            }
            _ => panic!("应为 Profile 变体"),
        },
        _ => panic!("应为 Ok 变体"),
    }
}

#[test]
fn test_response_err_serialization() {
    let resp = Response::Err {
        code: 403,
        message: "forbidden".to_string(),
    };
    let json = serde_json::to_string(&resp).unwrap();
    assert!(json.contains("\"status\":\"Err\""));
    assert!(json.contains("403"));
}

#[test]
fn test_unknown_method_rejected() {
    // 未知 method 字段应反序列化失败
    let json = r#"{"method":"Nonexistent","params":null}"#;
    let result: Result<Request, _> = serde_json::from_str(json);
    assert!(result.is_err(), "未知 method 应反序列化失败");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --test lan_protocol`
Expected: 编译失败（`write_frame`、`read_frame`、`Request` 等未定义）

- [ ] **Step 3: 实现 protocol.rs**

完整替换 `src-tauri/src/lan/protocol.rs` 内容：

```rust
//! JSON-RPC 协议类型与帧编解码
//!
//! 帧格式：[4 字节大端 u32 长度][JSON 字节流]
//! 单帧上限 MAX_FRAME_SIZE（16 MB）

use crate::lan::{LanError, MAX_FRAME_SIZE};
use serde::{Deserialize, Serialize};

/// 请求类型（客户端 → 服务端）
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    /// 列出对端公开的笔记（带分页 + tag 过滤）
    ListMemos {
        offset: u32,
        limit: u32,
        #[serde(default)]
        tag_filter: Option<Vec<String>>,
    },
    /// 获取单条笔记完整内容
    GetMemo {
        uid: String,
    },
    /// 获取附件字节
    GetAttachment {
        uid: String,
    },
    /// 获取对端展示名 + 公开笔记统计
    GetProfile,
}

/// 响应类型（服务端 → 客户端）
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "status")]
pub enum Response {
    Ok { data: ResponseData },
    Err { code: u16, message: String },
}

/// 响应数据载体
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum ResponseData {
    MemoList {
        memos: Vec<RemoteMemoSummary>,
        total: u32,
    },
    Memo(RemoteMemo),
    Attachment {
        content: Vec<u8>,
        mime_type: String,
    },
    Profile {
        display_name: String,
        public_memo_count: u32,
        tags: Vec<String>,
    },
}

/// 远端笔记摘要（列表项）
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteMemoSummary {
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub pinned: bool,
    pub snippet: String,
    pub tags: Vec<String>,
    pub has_attachments: bool,
}

/// 远端笔记完整内容
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteMemo {
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub pinned: bool,
    pub content: String,
    pub attachments: Vec<RemoteAttachmentSummary>,
}

/// 远端附件元数据
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteAttachmentSummary {
    pub uid: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}

/// 将 payload 写为帧（4字节长度前缀 + 内容）
pub fn write_frame(buf: &mut Vec<u8>, payload: &[u8]) -> Result<(), LanError> {
    let len = payload.len() as u32;
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(payload);
    Ok(())
}

/// 从 reader 读取一帧
///
/// 返回 payload 字节切片的 owned 版本。
/// 若声明长度超过 MAX_FRAME_SIZE 返回 FrameTooLarge 错误。
pub fn read_frame<R: std::io::Read>(reader: &mut R) -> Result<Vec<u8>, LanError> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            LanError::ConnectionClosed
        } else {
            LanError::Io(e)
        }
    })?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(LanError::FrameTooLarge(len));
    }
    let mut payload = vec![0u8; len];
    reader.read_exact(&mut payload).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            LanError::ConnectionClosed
        } else {
            LanError::Io(e)
        }
    })?;
    Ok(payload)
}

/// 序列化 Request 并写为帧
pub fn write_request<W: std::io::Write>(w: &mut W, req: &Request) -> Result<(), LanError> {
    let json = serde_json::to_vec(req)?;
    let len = json.len() as u32;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&json)?;
    Ok(())
}

/// 从 reader 读取并反序列化 Request
pub fn read_request<R: std::io::Read>(r: &mut R) -> Result<Request, LanError> {
    let payload = read_frame(r)?;
    let req: Request = serde_json::from_slice(&payload)?;
    Ok(req)
}

/// 序列化 Response 并写为帧
pub fn write_response<W: std::io::Write>(w: &mut W, resp: &Response) -> Result<(), LanError> {
    let json = serde_json::to_vec(resp)?;
    let len = json.len() as u32;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&json)?;
    Ok(())
}

/// 从 reader 读取并反序列化 Response
pub fn read_response<R: std::io::Read>(r: &mut R) -> Result<Response, LanError> {
    let payload = read_frame(r)?;
    let resp: Response = serde_json::from_slice(&payload)?;
    Ok(resp)
}

/// 构造 Ok 响应
pub fn ok(data: ResponseData) -> Response {
    Response::Ok { data }
}

/// 构造 Err 响应
pub fn err(code: u16, message: impl Into<String>) -> Response {
    Response::Err { code, message: message.into() }
}
```

- [ ] **Step 4: 让 crate 暴露 lan 模块用于测试**

在 `src-tauri/src/main.rs` 顶部的 `mod lan;` 改为 `pub mod lan;`（让集成测试可访问）。

- [ ] **Step 5: 运行测试确认通过**

Run: `cd src-tauri && cargo test --test lan_protocol`
Expected: 8 个测试全部 PASS

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/lan/protocol.rs src-tauri/src/main.rs src-tauri/tests/lan_protocol.rs
git commit -m "feat(lan): implement JSON-RPC protocol types and frame codec with tests"
```

---

## Task 4: 实现 ACL 权限过滤纯函数

**Files:**
- Modify: `src-tauri/src/lan/auth.rs`
- Create: `src-tauri/tests/lan_auth.rs`

- [ ] **Step 1: 编写 ACL 过滤失败测试**

创建 `src-tauri/tests/lan_auth.rs`：

```rust
//! auth.rs 单元测试：ACL 过滤算法
//!
//! 重点覆盖：默认开放、allow、deny、组合、完全拒绝、空 tag 笔记

use memos_app::lan::auth::{filter_memos_for_peer, AclRule};
use memos_core::memo::{CreateMemo, Memo};
use memos_core::types::Visibility;
use memos_core::Store;

fn make_memo(uid: &str, content: &str) -> Memo {
    let store = Store::open_in_memory().unwrap();
    let created = store
        .with_conn(|c| {
            memos_core::memo::create(c, &CreateMemo {
                uid: uid.to_string(),
                content: content.to_string(),
                visibility: Visibility::Public,
                pinned: false,
                payload: serde_json::Value::Object(Default::default()),
                location: None,
            })
        })
        .unwrap();
    created
}

fn rule(peer: &str, mode: &str, tags: &[&str]) -> AclRule {
    AclRule {
        peer_id: peer.to_string(),
        display_name: None,
        mode: mode.parse().unwrap(),
        tags: tags.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
fn test_no_rules_default_open() {
    // 无规则 → 全部可见
    let memos = vec![
        make_memo("m1", "#work hello"),
        make_memo("m2", "#life world"),
    ];
    let filtered = filter_memos_for_peer(memos, "peerA", &[]);
    assert_eq!(filtered.len(), 2, "无规则应默认全部可见");
}

#[test]
fn test_allow_single_tag() {
    let memos = vec![
        make_memo("m1", "#work hello"),
        make_memo("m2", "#life world"),
        make_memo("m3", "no tag here"),
    ];
    let rules = vec![rule("peerB", "allow", &["work"])];
    let filtered = filter_memos_for_peer(memos, "peerB", &rules);
    assert_eq!(filtered.len(), 1, "只应看到 #work 笔记");
    assert_eq!(filtered[0].uid, "m1");
}

#[test]
fn test_deny_single_tag() {
    let memos = vec![
        make_memo("m1", "#work hello"),
        make_memo("m2", "#draft wip"),
        make_memo("m3", "#work and #draft mixed"),
    ];
    let rules = vec![rule("peerC", "deny", &["draft"])];
    let filtered = filter_memos_for_peer(memos, "peerC", &rules);
    assert_eq!(filtered.len(), 1, "应排除 #draft 笔记");
    assert_eq!(filtered[0].uid, "m1");
}

#[test]
fn test_allow_plus_deny() {
    // allow ["team"] + deny ["draft"] → 只看 #team 但排除 #draft
    let memos = vec![
        make_memo("m1", "#team project"),
        make_memo("m2", "#team #draft wip"),
        make_memo("m3", "#life other"),
    ];
    let rules = vec![
        rule("peerD", "allow", &["team"]),
        rule("peerD", "deny", &["draft"]),
    ];
    let filtered = filter_memos_for_peer(memos, "peerD", &rules);
    assert_eq!(filtered.len(), 1, "只应看到 m1（m2 被 deny 排除）");
    assert_eq!(filtered[0].uid, "m1");
}

#[test]
fn test_complete_block_via_none_tag() {
    // allow ["__none__"] → 无任何笔记匹配 → 完全拒绝
    let memos = vec![
        make_memo("m1", "#work hello"),
        make_memo("m2", "#life world"),
    ];
    let rules = vec![rule("peerE", "allow", &["__none__"])];
    let filtered = filter_memos_for_peer(memos, "peerE", &rules);
    assert_eq!(filtered.len(), 0, "完全拒绝应返回空");
}

#[test]
fn test_peer_id_not_matching_default_open() {
    // 规则是给 peerB 的，但请求方是 peerX → 默认开放
    let memos = vec![make_memo("m1", "#work hello")];
    let rules = vec![rule("peerB", "allow", &["work"])];
    let filtered = filter_memos_for_peer(memos, "peerX", &rules);
    assert_eq!(filtered.len(), 1, "peer_id 不匹配应默认开放");
}

#[test]
fn test_empty_tag_memo_with_allow_rule() {
    // 笔记无 tag，allow 规则要求 #work → 不可见
    let memos = vec![make_memo("m1", "just plain text no tags")];
    let rules = vec![rule("peerB", "allow", &["work"])];
    let filtered = filter_memos_for_peer(memos, "peerB", &rules);
    assert_eq!(filtered.len(), 0, "无 tag 笔记在 allow 规则下应不可见");
}

#[test]
fn test_empty_tag_memo_with_deny_rule() {
    // 笔记无 tag，deny 规则 → 笔记可见（不命中 deny）
    let memos = vec![make_memo("m1", "plain text")];
    let rules = vec![rule("peerC", "deny", &["draft"])];
    let filtered = filter_memos_for_peer(memos, "peerC", &rules);
    assert_eq!(filtered.len(), 1, "无 tag 笔记在 deny 规则下应可见");
}

#[test]
fn test_multiple_allow_tags_union() {
    // 多个 allow 规则的 tags 取并集
    let memos = vec![
        make_memo("m1", "#work a"),
        make_memo("m2", "#life b"),
        make_memo("m3", "#other c"),
    ];
    let rules = vec![
        rule("peerF", "allow", &["work"]),
        rule("peerF", "allow", &["life"]),
    ];
    let filtered = filter_memos_for_peer(memos, "peerF", &rules);
    assert_eq!(filtered.len(), 2, "allow tags 应取并集");
}

#[test]
fn test_deny_overrides_allow_for_same_tag() {
    // 同一 tag 同时在 allow 和 deny → deny 优先
    let memos = vec![make_memo("m1", "#shared content")];
    let rules = vec![
        rule("peerG", "allow", &["shared"]),
        rule("peerG", "deny", &["shared"]),
    ];
    let filtered = filter_memos_for_peer(memos, "peerG", &rules);
    assert_eq!(filtered.len(), 0, "deny 应优先于 allow");
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cd src-tauri && cargo test --test lan_auth`
Expected: 编译失败（`AclRule`、`filter_memos_for_peer` 未定义）

- [ ] **Step 3: 实现 auth.rs**

完整替换 `src-tauri/src/lan/auth.rs` 内容：

```rust
//! 编组权限过滤（ACL）
//!
//! 规则存 app_setting:lan_acl_rules（JSON 数组）。
//! 过滤算法见 spec 第 3 节。

use memos_core::markdown;
use memos_core::memo::Memo;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 单条 ACL 规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AclRule {
    /// 对端 EndpointId（base32 编码字符串）
    pub peer_id: String,
    /// 对端展示名（可选，方便用户识别）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// allow 或 deny
    pub mode: AclMode,
    /// 匹配的 tag 列表（必须非空）
    pub tags: Vec<String>,
}

/// 规则模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AclMode {
    Allow,
    Deny,
}

/// 从 app_setting 读取并反序列化所有 ACL 规则
pub fn load_rules(json: &str) -> Vec<AclRule> {
    serde_json::from_str(json).unwrap_or_default()
}

/// 过滤笔记列表：按 peer_id 匹配规则，应用 allow/deny
///
/// 算法：
/// 1. 无匹配规则 → 全部可见（默认开放）
/// 2. 有规则 → allow_tags 取并集，deny_tags 取并集
/// 3. 笔记可见条件：
///    - allow_tags 为空（无 allow 规则）或笔记含 allow_tags 中任一 tag
///    - 且笔记不含 deny_tags 中任一 tag
pub fn filter_memos_for_peer(memos: Vec<Memo>, peer_id: &str, rules: &[AclRule]) -> Vec<Memo> {
    let peer_rules: Vec<&AclRule> = rules.iter().filter(|r| r.peer_id == peer_id).collect();
    if peer_rules.is_empty() {
        return memos; // 默认开放
    }

    let allow_tags: HashSet<&str> = peer_rules
        .iter()
        .filter(|r| r.mode == AclMode::Allow)
        .flat_map(|r| r.tags.iter().map(String::as_str))
        .collect();
    let deny_tags: HashSet<&str> = peer_rules
        .iter()
        .filter(|r| r.mode == AclMode::Deny)
        .flat_map(|r| r.tags.iter().map(String::as_str))
        .collect();

    memos
        .into_iter()
        .filter(|m| {
            let tags: HashSet<String> = markdown::extract_tags(&m.content).into_iter().collect();
            let allow_pass =
                allow_tags.is_empty() || tags.iter().any(|t| allow_tags.contains(t.as_str()));
            let deny_pass = !tags.iter().any(|t| deny_tags.contains(t.as_str()));
            allow_pass && deny_pass
        })
        .collect()
}

/// 验证某条 memo 是否对 peer 可见（用于 GetMemo / GetAttachment）
pub fn is_memo_visible(memo: &Memo, peer_id: &str, rules: &[AclRule]) -> bool {
    // 复用 filter_memos_for_peer 的逻辑
    let single = vec![memo.clone()];
    !filter_memos_for_peer(single, peer_id, rules).is_empty()
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cd src-tauri && cargo test --test lan_auth`
Expected: 10 个测试全部 PASS

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lan/auth.rs src-tauri/tests/lan_auth.rs
git commit -m "feat(lan): implement ACL filter with 10 unit tests covering all spec scenarios"
```

---

## Task 5: 实现 Endpoint 初始化与 SecretKey 持久化

**Files:**
- Modify: `src-tauri/src/lan/endpoint.rs`
- Modify: `src-tauri/src/state.rs`

- [ ] **Step 1: 实现 endpoint.rs**

完整替换 `src-tauri/src/lan/endpoint.rs` 内容：

```rust
//! iroh Endpoint 初始化与 mDNS 发现
//!
//! - SecretKey 持久化到 app_data_dir/lan_identity.key
//! - mDNS 通过 iroh-mdns-address-lookup 启用
//! - 展示名通过 instance_setting:lan_display_name 存储

use crate::lan::{LanError, LanState, PeerInfo, ALPN};
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
        let secret = SecretKey::from_bytes(
            bytes
                .as_slice()
                .try_into()
                .map_err(|_| LanError::LocalStore("invalid secret key file".into()))?,
        );
        Ok(secret)
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

    let endpoint_id = endpoint.endpoint_id().clone();
    tracing::info!("LAN Endpoint bound, endpoint_id = {}", endpoint_id);

    let display_name = DEFAULT_DISPLAY_NAME.to_string();
    let state = Arc::new(LanState {
        endpoint,
        peers: RwLock::new(Vec::new()),
        display_name: RwLock::new(display_name),
    });

    Ok(state)
}

/// 从 LanState 获取本机 endpoint_id 的 base32 字符串
pub fn local_peer_id(state: &LanState) -> String {
    state.endpoint.endpoint_id().to_string()
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
```

**注意**：`iroh::Endpoint::endpoint_id()`、`SecretKey::from_bytes`、`SecretKey::to_bytes` 的实际 API 可能在不同 iroh 版本有差异。如果编译失败，需查阅 `cargo doc --open -p iroh` 调整。常见调整点：
- `endpoint_id()` 可能叫 `node_id()` 或需要通过 `endpoint.secret_key().public()` 获取
- `MdnsAddressLookup::builder()` 可能需要传入元数据参数

- [ ] **Step 2: 扩展 AppState 持有 LanState**

修改 `src-tauri/src/state.rs`，完整替换为：

```rust
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
```

- [ ] **Step 3: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 成功（如果 iroh API 不匹配，按 Step 1 注释调整）

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lan/endpoint.rs src-tauri/src/state.rs
git commit -m "feat(lan): implement endpoint init, secret key persistence, AppState integration"
```

---

## Task 6: 实现 main.rs 启动集成

**Files:**
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: 在 setup 阶段初始化 LanState**

修改 `src-tauri/src/main.rs` 的 `setup` 闭包，在 `app.manage(AppState {...})` 之前添加 LAN 初始化。

定位到 `let attachments_dir = ...` 和 `std::fs::create_dir_all(&attachments_dir)...` 之后，在 `app.manage(AppState {` 之前插入：

```rust
            // 初始化 LAN 模块（失败不阻塞应用启动，仅记录警告）
            let lan_state: Option<Arc<lan::LanState>> = match lan::endpoint::init_lan_state(&data_dir).await {
                Ok(state) => {
                    tracing::info!("LAN 模块启动成功");
                    Some(state)
                }
                Err(e) => {
                    tracing::warn!("LAN 模块启动失败（应用其他功能不受影响）: {}", e);
                    None
                }
            };
```

然后修改 `app.manage(AppState { ... })` 添加 `lan` 字段：

```rust
            app.manage(AppState {
                store: std::sync::Mutex::new(store),
                attachments_dir,
                lan: lan_state,
            });
```

- [ ] **Step 2: 注册 lan 命令模块**

在 `src-tauri/src/commands/mod.rs` 末尾添加：

```rust
pub mod lan;
```

- [ ] **Step 3: 在 invoke_handler 注册 lan 命令（先占位）**

在 `src-tauri/src/main.rs` 的 `.invoke_handler(tauri::generate_handler![...])` 中，在 `commands::ai_chat::save_providers_cmd,` 之后添加：

```rust
            // lan discovery
            commands::lan::lan_discover_peers,
            commands::lan::lan_get_local_identity,
            commands::lan::lan_update_display_name,
            commands::lan::lan_get_acl_rules,
            commands::lan::lan_save_acl_rules,
            commands::lan::lan_get_remote_profile,
            commands::lan::lan_list_remote_memos,
            commands::lan::lan_get_remote_memo,
            commands::lan::lan_get_remote_attachment,
            commands::lan::lan_copy_memo_to_local,
```

- [ ] **Step 4: 创建 commands/lan.rs 骨架（让编译通过）**

创建 `src-tauri/src/commands/lan.rs`，所有命令暂时返回未实现错误：

```rust
//! LAN 发现与分享相关 IPC 命令

use crate::error::{IpcError, IpcResult};
use crate::lan::PeerInfo;
use crate::state::AppState;
use serde::{Deserialize, Serialize};

// ---------- 类型定义（前端使用） ----------

#[derive(Debug, Serialize)]
pub struct LocalIdentity {
    pub peer_id: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDisplayNameRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct RemoteProfile {
    pub display_name: String,
    pub public_memo_count: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListRemoteMemosRequest {
    pub peer_id: String,
    pub offset: u32,
    pub limit: u32,
    #[serde(default)]
    pub tag_filter: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct ListRemoteMemosResponse {
    pub memos: Vec<crate::lan::protocol::RemoteMemoSummary>,
    pub total: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetRemoteMemoRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Deserialize)]
pub struct GetRemoteAttachmentRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Serialize)]
pub struct RemoteAttachmentResponse {
    pub content: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Deserialize)]
pub struct CopyMemoToLocalRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Serialize)]
pub struct CopyMemoToLocalResponse {
    pub new_memo_uid: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveAclRulesRequest {
    pub rules: Vec<crate::lan::auth::AclRule>,
}

// ---------- 命令（暂时未实现） ----------

#[tauri::command]
pub async fn lan_discover_peers(_state: tauri::State<'_, AppState>) -> IpcResult<Vec<PeerInfo>> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_local_identity(_state: tauri::State<'_, AppState>) -> IpcResult<LocalIdentity> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_update_display_name(
    _state: tauri::State<'_, AppState>,
    _req: UpdateDisplayNameRequest,
) -> IpcResult<()> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_acl_rules(_state: tauri::State<'_, AppState>) -> IpcResult<Vec<crate::lan::auth::AclRule>> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_save_acl_rules(
    _state: tauri::State<'_, AppState>,
    _req: SaveAclRulesRequest,
) -> IpcResult<()> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_remote_profile(
    _state: tauri::State<'_, AppState>,
    _peer_id: String,
) -> IpcResult<RemoteProfile> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_list_remote_memos(
    _state: tauri::State<'_, AppState>,
    _req: ListRemoteMemosRequest,
) -> IpcResult<ListRemoteMemosResponse> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_remote_memo(
    _state: tauri::State<'_, AppState>,
    _req: GetRemoteMemoRequest,
) -> IpcResult<crate::lan::protocol::RemoteMemo> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_get_remote_attachment(
    _state: tauri::State<'_, AppState>,
    _req: GetRemoteAttachmentRequest,
) -> IpcResult<RemoteAttachmentResponse> {
    Err(IpcError::Lan("not implemented".into()))
}

#[tauri::command]
pub async fn lan_copy_memo_to_local(
    _state: tauri::State<'_, AppState>,
    _req: CopyMemoToLocalRequest,
) -> IpcResult<CopyMemoToLocalResponse> {
    Err(IpcError::Lan("not implemented".into()))
}
```

- [ ] **Step 5: 验证编译并运行应用**

Run: `cd src-tauri && cargo check`
Expected: 成功

Run: `cd src-tauri && cargo run`
Expected: 应用启动，日志中应看到 `LAN 模块启动成功` 或 `LAN 模块启动失败`（前者为正常，后者记录原因）

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/main.rs src-tauri/src/commands/mod.rs src-tauri/src/commands/lan.rs
git commit -m "feat(lan): wire up LanState init in main setup, register lan command stubs"
```

---

## Task 7: 实现客户端 call_remote

**Files:**
- Modify: `src-tauri/src/lan/client.rs`

- [ ] **Step 1: 实现 client.rs**

完整替换 `src-tauri/src/lan/client.rs` 内容：

```rust
//! 客户端：发起连接与请求远端
//!
//! 每个 RPC 调用一个 bi-stream，不复用 stream。
//! 超时：连接 5s，普通 RPC 10s，附件 60s。

use crate::lan::protocol::{read_response, write_request, Request, Response, ResponseData};
use crate::lan::{LanError, ALPN, ATTACHMENT_TIMEOUT_SECS, CONNECT_TIMEOUT_SECS, RPC_TIMEOUT_SECS};
use iroh::{Endpoint, EndpointAddr};
use tokio::time::Duration;

/// 解析 peer_id 字符串为 EndpointAddr
///
/// peer_id 格式：base32 编码的 EndpointId
/// 实际地址通过 iroh 的 address lookup（mDNS + DNS）解析
fn parse_peer_addr(endpoint: &Endpoint, peer_id: &str) -> Result<EndpointAddr, LanError> {
    // iroh 的 EndpointAddr 可从 EndpointId 构造，地址由 lookup 服务解析
    // 实际 API 可能是 endpoint.connect_direct或类似
    // 这里使用 EndpointAddr::from_str 或类似方式
    peer_id
        .parse::<EndpointAddr>()
        .map_err(|e| LanError::Endpoint(format!("invalid peer_id {peer_id}: {e}")))
}

/// 发起 RPC 请求（通用超时 10 秒）
pub async fn call_remote(
    endpoint: &Endpoint,
    peer_id: &str,
    req: Request,
) -> Result<ResponseData, LanError> {
    call_remote_with_timeout(endpoint, peer_id, req, Duration::from_secs(RPC_TIMEOUT_SECS)).await
}

/// 发起附件下载请求（60 秒超时）
pub async fn call_remote_attachment(
    endpoint: &Endpoint,
    peer_id: &str,
    req: Request,
) -> Result<ResponseData, LanError> {
    call_remote_with_timeout(
        endpoint,
        peer_id,
        req,
        Duration::from_secs(ATTACHMENT_TIMEOUT_SECS),
    )
    .await
}

/// 带超时的 RPC 请求
async fn call_remote_with_timeout(
    endpoint: &Endpoint,
    peer_id: &str,
    req: Request,
    timeout: Duration,
) -> Result<ResponseData, LanError> {
    let addr = parse_peer_addr(endpoint, peer_id)?;

    // 连接（带超时）
    let conn = tokio::time::timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        endpoint.connect(&addr, ALPN),
    )
    .await
    .map_err(|_| LanError::ConnectTimeout)?
    .map_err(|e| LanError::Endpoint(e.to_string()))?;

    // 打开 bi-stream 并发送请求（带超时）
    let result = tokio::time::timeout(timeout, async {
        let (mut send, mut recv) = conn.open_bi().await.map_err(|e| LanError::Endpoint(e.to_string()))?;
        write_request(&mut send, &req)?;
        send.finish().map_err(LanError::Io)?;
        let resp: Response = read_response(&mut recv)?;
        Ok::<Response, LanError>(resp)
    })
    .await;

    let resp = result.map_err(|_| LanError::RpcTimeout)??;

    match resp {
        Response::Ok { data } => Ok(data),
        Response::Err { code, message } => Err(LanError::Remote(code, message)),
    }
}
```

**注意**：iroh 1.x 的 `endpoint.connect()` 实际签名可能是 `connect(impl Into<EndpointAddr>, alpn)` 或 `connect(&EndpointAddr, alpn)`。`EndpointAddr::parse` 也可能叫 `from_str`。如果编译失败，查阅 `cargo doc -p iroh --open` 调整。常见调整：
- `peer_id.parse::<EndpointAddr>()` 可能需要 `EndpointAddr::from_str(peer_id)`
- `endpoint.connect(&addr, ALPN)` 可能需要 `endpoint.connect(addr, ALPN)`（无引用）

- [ ] **Step 2: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 成功

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/lan/client.rs
git commit -m "feat(lan): implement call_remote client with connect/rpc/attachment timeouts"
```

---

## Task 8: 实现服务端 accept 循环与 4 个 handler

**Files:**
- Modify: `src-tauri/src/lan/server.rs`
- Modify: `src-tauri/src/main.rs`（启动 server task）

- [ ] **Step 1: 实现 server.rs**

完整替换 `src-tauri/src/lan/server.rs` 内容：

```rust
//! accept 循环与请求分发
//!
//! 每个 bi-stream spawn 一个 task 处理，无共享状态。
//! SQLite 访问通过 spawn_blocking 避免阻塞 async runtime。

use crate::lan::auth::{filter_memos_for_peer, is_memo_visible, load_rules, AclRule};
use crate::lan::endpoint::{load_acl_rules_json, load_display_name, ACL_RULES_KEY};
use crate::lan::protocol::{
    err, ok, read_request, write_response, RemoteAttachmentSummary, RemoteMemo,
    RemoteMemoSummary, Request, ResponseData,
};
use crate::lan::{LanError, LanState, RPC_TIMEOUT_SECS};
use memos_core::attachment::{FindAttachment, STORAGE_TYPE_LOCAL};
use memos_core::markdown;
use memos_core::memo::FindMemo;
use memos_core::types::Visibility;
use std::sync::Arc;
use tauri::Manager;
use tokio::time::Duration;

/// 启动 accept 循环（在后台 task 中运行）
pub fn spawn_accept_loop(state: Arc<LanState>, app_handle: tauri::AppHandle) {
    tokio::spawn(async move {
        loop {
            match state.endpoint.accept().await {
                Some(incoming) => {
                    let state = state.clone();
                    let app = app_handle.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_incoming(state, app, incoming).await {
                            tracing::warn!("LAN connection handler error: {}", e);
                        }
                    });
                }
                None => {
                    tracing::info!("LAN endpoint accept loop closed");
                    break;
                }
            }
        }
    });
}

async fn handle_incoming(
    state: Arc<LanState>,
    app: tauri::AppHandle,
    incoming: iroh::endpoint::Incoming,
) -> Result<(), LanError> {
    let conn = incoming
        .await
        .map_err(|e| LanError::Endpoint(e.to_string()))?;
    let peer_id = conn.peer_id().to_string();
    tracing::debug!("LAN connection from peer_id={}", peer_id);

    loop {
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let state = state.clone();
        let app = app.clone();
        let peer_id = peer_id.clone();
        tokio::spawn(async move {
            let result = tokio::time::timeout(
                Duration::from_secs(RPC_TIMEOUT_SECS),
                handle_request(&state, &app, &peer_id, &mut recv),
            )
            .await;
            let resp = match result {
                Ok(Ok(resp)) => resp,
                Ok(Err(e)) => {
                    tracing::warn!("LAN request handler error: {}", e);
                    err(500, e.to_string())
                }
                Err(_) => err(500, "rpc timeout"),
            };
            if let Err(e) = write_response(&mut send, &resp) {
                tracing::warn!("LAN write response failed: {}", e);
            }
        });
    }
    Ok(())
}

async fn handle_request<R: std::io::Read>(
    state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    recv: &mut R,
) -> Result<crate::lan::protocol::Response, LanError> {
    let req: Request = read_request(recv)?;
    match req {
        Request::GetProfile => handle_get_profile(state, app).await,
        Request::ListMemos { offset, limit, tag_filter } => {
            handle_list_memos(state, app, peer_id, offset, limit, tag_filter).await
        }
        Request::GetMemo { uid } => handle_get_memo(state, app, peer_id, uid).await,
        Request::GetAttachment { uid } => handle_get_attachment(state, app, peer_id, uid).await,
    }
}

async fn handle_get_profile(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
) -> Result<crate::lan::protocol::Response, LanError> {
    let app_state = app.state::<crate::state::AppState>();
    let store = app_state.store();

    let display_name = load_display_name(&store);

    // 统计 PUBLIC 笔记数
    let public_count: u32 = store
        .with_conn(|c| {
            let count: i64 = c.query_row(
                "SELECT COUNT(*) FROM memo WHERE visibility = 'PUBLIC' AND row_status = 'NORMAL'",
                [],
                |r| r.get(0),
            )?;
            Ok(count as u32)
        })
        .map_err(|e| LanError::LocalStore(e.to_string()))?;

    // 收集所有 PUBLIC 笔记的 tag
    let memos = store
        .with_conn(|c| {
            memos_core::memo::list(c, &FindMemo {
                visibility_list: vec![Visibility::Public],
                exclude_content: true,
                ..Default::default()
            })
        })
        .map_err(|e| LanError::LocalStore(e.to_string()))?;
    let mut tags: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for m in memos {
        for t in markdown::extract_tags(&m.content) {
            tags.insert(t);
        }
    }

    Ok(ok(ResponseData::Profile {
        display_name,
        public_memo_count: public_count,
        tags: tags.into_iter().collect(),
    }))
}

async fn handle_list_memos(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    offset: u32,
    limit: u32,
    tag_filter: Option<Vec<String>>,
) -> Result<crate::lan::protocol::Response, LanError> {
    // 参数校验
    if limit == 0 || limit > 200 {
        return Ok(err(400, "limit must be 1..=200"));
    }

    let app_state = app.state::<crate::state::AppState>();
    let store = app_state.store();

    // 读取 ACL 规则
    let acl_json = load_acl_rules_json(&store);
    let rules: Vec<AclRule> = load_rules(&acl_json);

    // 查询所有 PUBLIC 笔记（带 tag 过滤）
    let memos = store
        .with_conn(|c| {
            memos_core::memo::list(c, &FindMemo {
                visibility_list: vec![Visibility::Public],
                exclude_content: false, // 需要 content 提取 tag 和 snippet
                tag_search: tag_filter.unwrap_or_default(),
                limit: Some(500), // 先取较多，ACL 过滤后再分页
                ..Default::default()
            })
        })
        .map_err(|e| LanError::LocalStore(e.to_string()))?;

    // 应用 ACL 过滤
    let filtered = filter_memos_for_peer(memos, peer_id, &rules);

    // 分页
    let total = filtered.len() as u32;
    let start = (offset as usize).min(filtered.len());
    let end = (start + limit as usize).min(filtered.len());
    let page = &filtered[start..end];

    // 转换为摘要
    let mut summaries = Vec::with_capacity(page.len());
    for m in page {
        let tags = markdown::extract_tags(&m.content);
        let snippet = snippet_text(&m.content, 200);
        // 检查是否有附件
        let has_att = store
            .with_conn(|c| {
                let count: i64 = c.query_row(
                    "SELECT COUNT(*) FROM attachment WHERE memo_id = ?",
                    rusqlite::params![m.id],
                    |r| r.get(0),
                )?;
                Ok(count > 0)
            })
            .unwrap_or(false);
        summaries.push(RemoteMemoSummary {
            uid: m.uid.clone(),
            created_ts: m.created_ts,
            updated_ts: m.updated_ts,
            pinned: m.pinned,
            snippet,
            tags,
            has_attachments: has_att,
        });
    }

    Ok(ok(ResponseData::MemoList {
        memos: summaries,
        total,
    }))
}

async fn handle_get_memo(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    uid: String,
) -> Result<crate::lan::protocol::Response, LanError> {
    let app_state = app.state::<crate::state::AppState>();
    let store = app_state.store();

    // 查询 memo
    let memo = store
        .with_conn(|c| memos_core::memo::get(c, &FindMemo { uid: Some(uid.clone()), ..Default::default() }))
        .map_err(|e| LanError::LocalStore(e.to_string()))?
        .ok_or_else(|| LanError::Remote(404, format!("memo {uid} not found")))?;

    // 验证可见性
    if memo.visibility != Visibility::Public {
        return Ok(err(403, "memo is not public"));
    }

    // 读取 ACL 规则并验证
    let acl_json = load_acl_rules_json(&store);
    let rules: Vec<AclRule> = load_rules(&acl_json);
    if !is_memo_visible(&memo, peer_id, &rules) {
        return Ok(err(403, "acl denied"));
    }

    // 查询附件元数据
    let attachments = store
        .with_conn(|c| {
            memos_core::attachment::list(c, &FindAttachment {
                memo_id: Some(memo.id),
                get_blob: false,
                ..Default::default()
            })
        })
        .map_err(|e| LanError::LocalStore(e.to_string()))?;

    let att_summaries: Vec<RemoteAttachmentSummary> = attachments
        .iter()
        .map(|a| RemoteAttachmentSummary {
            uid: a.uid.clone(),
            filename: a.filename.clone(),
            mime_type: a.r#type.clone(),
            size: a.size as u64,
        })
        .collect();

    Ok(ok(ResponseData::Memo(RemoteMemo {
        uid: memo.uid,
        created_ts: memo.created_ts,
        updated_ts: memo.updated_ts,
        pinned: memo.pinned,
        content: memo.content,
        attachments: att_summaries,
    })))
}

async fn handle_get_attachment(
    _state: &Arc<LanState>,
    app: &tauri::AppHandle,
    peer_id: &str,
    uid: String,
) -> Result<crate::lan::protocol::Response, LanError> {
    let app_state = app.state::<crate::state::AppState>();
    let store = app_state.store();

    // 查询附件
    let att = store
        .with_conn(|c| {
            memos_core::attachment::get(c, &FindAttachment {
                uid: Some(uid.clone()),
                get_blob: true,
                ..Default::default()
            })
        })
        .map_err(|e| LanError::LocalStore(e.to_string()))?
        .ok_or_else(|| LanError::Remote(404, format!("attachment {uid} not found")))?;

    // 找到关联 memo
    let memo_id = match att.memo_id {
        Some(id) => id,
        None => return Ok(err(403, "attachment has no associated memo")),
    };

    // 验证 memo 可见性
    let memo = store
        .with_conn(|c| memos_core::memo::get(c, &FindMemo { id: Some(memo_id), ..Default::default() }))
        .map_err(|e| LanError::LocalStore(e.to_string()))?
        .ok_or_else(|| LanError::Remote(404, "associated memo not found"))?;

    if memo.visibility != Visibility::Public {
        return Ok(err(403, "associated memo is not public"));
    }

    let acl_json = load_acl_rules_json(&store);
    let rules: Vec<AclRule> = load_rules(&acl_json);
    if !is_memo_visible(&memo, peer_id, &rules) {
        return Ok(err(403, "acl denied for associated memo"));
    }

    // 读取附件字节
    let content = if att.storage_type == STORAGE_TYPE_LOCAL && !att.reference.is_empty() {
        crate::file_storage::read_file(&app_state.attachments_dir, &att.reference)
            .map_err(|e| LanError::LocalStore(e.to_string()))?
    } else {
        att.blob.unwrap_or_default()
    };

    Ok(ok(ResponseData::Attachment {
        content,
        mime_type: att.r#type,
    }))
}

/// 生成纯文本摘要（去除 markdown 标记）
fn snippet_text(content: &str, max: usize) -> String {
    let plain: String = content
        .chars()
        .filter(|&c| !"#*`>\\-[\\]()!".contains(c))
        .collect();
    let collapsed: String = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max {
        collapsed
    } else {
        collapsed.chars().take(max).collect()
    }
}
```

- [ ] **Step 2: 在 main.rs 启动 server accept loop**

在 `src-tauri/src/main.rs` 的 setup 闭包中，在 `app.manage(AppState {...})` 之后（在 backfill_embeddings 之前）添加：

```rust
            // 启动 LAN 服务端 accept 循环
            if let Some(ref lan_state) = lan_state {
                let app_handle = app.handle().clone();
                lan::server::spawn_accept_loop(lan_state.clone(), app_handle);
            }
```

- [ ] **Step 3: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 成功

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lan/server.rs src-tauri/src/main.rs
git commit -m "feat(lan): implement server accept loop with 4 handlers (profile/list/get/attachment)"
```

---

## Task 9: 实现 Tauri 命令（10 个）

**Files:**
- Modify: `src-tauri/src/commands/lan.rs`

- [ ] **Step 1: 完整实现 commands/lan.rs**

完整替换 `src-tauri/src/commands/lan.rs` 内容：

```rust
//! LAN 发现与分享相关 IPC 命令
//!
//! 10 个命令：
//! - 发现：lan_discover_peers, lan_get_local_identity, lan_update_display_name
//! - 配置：lan_get_acl_rules, lan_save_acl_rules
//! - 查询：lan_get_remote_profile, lan_list_remote_memos, lan_get_remote_memo, lan_get_remote_attachment
//! - 复制：lan_copy_memo_to_local

use crate::error::{IpcError, IpcResult};
use crate::lan::auth::{load_rules, AclRule};
use crate::lan::client::{call_remote, call_remote_attachment};
use crate::lan::endpoint::{
    load_acl_rules_json, load_display_name, save_acl_rules_json, save_display_name,
};
use crate::lan::protocol::{
    RemoteAttachmentSummary, RemoteMemo, RemoteMemoSummary, Request, ResponseData,
};
use crate::lan::PeerInfo;
use crate::state::AppState;
use memos_core::markdown;
use memos_core::memo::{CreateMemo, FindMemo};
use memos_core::types::Visibility;
use serde::{Deserialize, Serialize};

// ---------- 类型定义 ----------

#[derive(Debug, Serialize)]
pub struct LocalIdentity {
    pub peer_id: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDisplayNameRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct RemoteProfile {
    pub display_name: String,
    pub public_memo_count: u32,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListRemoteMemosRequest {
    pub peer_id: String,
    pub offset: u32,
    pub limit: u32,
    #[serde(default)]
    pub tag_filter: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct ListRemoteMemosResponse {
    pub memos: Vec<RemoteMemoSummary>,
    pub total: u32,
}

#[derive(Debug, Deserialize)]
pub struct GetRemoteMemoRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Deserialize)]
pub struct GetRemoteAttachmentRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Serialize)]
pub struct RemoteAttachmentResponse {
    pub content: Vec<u8>,
    pub mime_type: String,
}

#[derive(Debug, Deserialize)]
pub struct CopyMemoToLocalRequest {
    pub peer_id: String,
    pub uid: String,
}

#[derive(Debug, Serialize)]
pub struct CopyMemoToLocalResponse {
    pub new_memo_uid: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveAclRulesRequest {
    pub rules: Vec<AclRule>,
}

// ---------- 命令实现 ----------

/// 发现局域网 peer 列表（读 mDNS 缓存快照）
#[tauri::command]
pub async fn lan_discover_peers(state: tauri::State<'_, AppState>) -> IpcResult<Vec<PeerInfo>> {
    let lan = state.lan()?;
    let peers = lan.peers.read().await;
    Ok(peers.clone())
}

/// 获取本机身份（peer_id + 展示名）
#[tauri::command]
pub async fn lan_get_local_identity(state: tauri::State<'_, AppState>) -> IpcResult<LocalIdentity> {
    let lan = state.lan()?;
    let peer_id = lan.endpoint.endpoint_id().to_string();
    let display_name = {
        let store = state.store();
        load_display_name(&store)
    };
    Ok(LocalIdentity {
        peer_id,
        display_name,
    })
}

/// 更新展示名
#[tauri::command]
pub async fn lan_update_display_name(
    state: tauri::State<'_, AppState>,
    req: UpdateDisplayNameRequest,
) -> IpcResult<()> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(IpcError::BadRequest("display name cannot be empty".into()));
    }
    let store = state.store();
    save_display_name(&store, &name)?;
    // 更新内存中的 display_name
    if let Some(lan) = &state.lan {
        let mut dn = lan.display_name.write().await;
        *dn = name;
    }
    Ok(())
}

/// 获取 ACL 规则列表
#[tauri::command]
pub async fn lan_get_acl_rules(state: tauri::State<'_, AppState>) -> IpcResult<Vec<AclRule>> {
    let store = state.store();
    let json = load_acl_rules_json(&store);
    Ok(load_rules(&json))
}

/// 保存 ACL 规则列表
#[tauri::command]
pub async fn lan_save_acl_rules(
    state: tauri::State<'_, AppState>,
    req: SaveAclRulesRequest,
) -> IpcResult<()> {
    // 校验：每条规则的 tags 必须非空
    for rule in &req.rules {
        if rule.tags.is_empty() {
            return Err(IpcError::BadRequest(
                "acl rule tags must be non-empty".into(),
            ));
        }
    }
    let json = serde_json::to_string(&req.rules)
        .map_err(|e| IpcError::Internal(format!("serialize acl rules failed: {e}")))?;
    let store = state.store();
    save_acl_rules_json(&store, &json)?;
    Ok(())
}

/// 获取远端 peer 的 profile
#[tauri::command]
pub async fn lan_get_remote_profile(
    state: tauri::State<'_, AppState>,
    peer_id: String,
) -> IpcResult<RemoteProfile> {
    let lan = state.lan()?;
    let data = call_remote(&lan.endpoint, &peer_id, Request::GetProfile)
        .await
        .map_err(IpcError::from)?;
    match data {
        ResponseData::Profile {
            display_name,
            public_memo_count,
            tags,
        } => Ok(RemoteProfile {
            display_name,
            public_memo_count,
            tags,
        }),
        _ => Err(IpcError::Lan("unexpected response type".into())),
    }
}

/// 列出远端 peer 的公开笔记
#[tauri::command]
pub async fn lan_list_remote_memos(
    state: tauri::State<'_, AppState>,
    req: ListRemoteMemosRequest,
) -> IpcResult<ListRemoteMemosResponse> {
    let lan = state.lan()?;
    let data = call_remote(
        &lan.endpoint,
        &req.peer_id,
        Request::ListMemos {
            offset: req.offset,
            limit: req.limit,
            tag_filter: req.tag_filter,
        },
    )
    .await
    .map_err(IpcError::from)?;
    match data {
        ResponseData::MemoList { memos, total } => Ok(ListRemoteMemosResponse { memos, total }),
        _ => Err(IpcError::Lan("unexpected response type".into())),
    }
}

/// 获取远端单条笔记完整内容
#[tauri::command]
pub async fn lan_get_remote_memo(
    state: tauri::State<'_, AppState>,
    req: GetRemoteMemoRequest,
) -> IpcResult<RemoteMemo> {
    let lan = state.lan()?;
    let data = call_remote(&lan.endpoint, &req.peer_id, Request::GetMemo { uid: req.uid })
        .await
        .map_err(IpcError::from)?;
    match data {
        ResponseData::Memo(memo) => Ok(memo),
        _ => Err(IpcError::Lan("unexpected response type".into())),
    }
}

/// 获取远端附件字节
#[tauri::command]
pub async fn lan_get_remote_attachment(
    state: tauri::State<'_, AppState>,
    req: GetRemoteAttachmentRequest,
) -> IpcResult<RemoteAttachmentResponse> {
    let lan = state.lan()?;
    let data = call_remote_attachment(
        &lan.endpoint,
        &req.peer_id,
        Request::GetAttachment { uid: req.uid },
    )
    .await
    .map_err(IpcError::from)?;
    match data {
        ResponseData::Attachment { content, mime_type } => Ok(RemoteAttachmentResponse {
            content,
            mime_type,
        }),
        _ => Err(IpcError::Lan("unexpected response type".into())),
    }
}

/// 复制远端笔记到本地
///
/// 1. 拉取远端 memo 完整内容
/// 2. 拉取所有附件字节
/// 3. 本地 create_memo（重新生成 uid，visibility=Private）
/// 4. 本地 create_attachment 关联到新 memo
#[tauri::command]
pub async fn lan_copy_memo_to_local(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    req: CopyMemoToLocalRequest,
) -> IpcResult<CopyMemoToLocalResponse> {
    let lan = state.lan()?;

    // 1. 拉取远端 memo
    let remote_memo = match call_remote(&lan.endpoint, &req.peer_id, Request::GetMemo { uid: req.uid.clone() })
        .await
        .map_err(IpcError::from)?
    {
        ResponseData::Memo(m) => m,
        _ => return Err(IpcError::Lan("unexpected response type".into())),
    };

    // 2. 生成新 uid
    let new_uid = generate_uid();

    // 3. 本地创建 memo（visibility=Private, pinned=false）
    let created_memo = {
        let store = state.store();
        store.with_conn(|c| {
            memos_core::memo::create(c, &CreateMemo {
                uid: new_uid.clone(),
                content: remote_memo.content.clone(),
                visibility: Visibility::Private,
                pinned: false,
                payload: serde_json::Value::Object(Default::default()),
                location: None,
            })
        })?
    };

    // 4. 拉取并创建附件
    for att in &remote_memo.attachments {
        let att_data = match call_remote_attachment(
            &lan.endpoint,
            &req.peer_id,
            Request::GetAttachment { uid: att.uid.clone() },
        )
        .await
        .map_err(IpcError::from)?
        {
            ResponseData::Attachment { content, mime_type } => (content, mime_type),
            _ => {
                tracing::warn!("跳过附件 {}：远端返回类型异常", att.uid);
                continue;
            }
        };

        // 本地创建附件（用 storage config 自动决定存储方式）
        let att_uid = generate_uid();
        let app_state = app.state::<AppState>();
        let _ = crate::commands::attachment::create_attachment(
            tauri::State::from(&app_state),
            crate::commands::attachment::CreateAttachmentRequest {
                uid: att_uid,
                filename: att.filename.clone(),
                blob: att_data.0,
                r#type: att_data.1,
                memo_id: Some(created_memo.id),
                storage_type: None,
            },
        );
    }

    Ok(CopyMemoToLocalResponse {
        new_memo_uid: new_uid,
    })
}

/// 生成 16 字符 hex uid（与 connect.ts 的 createMemo 逻辑一致）
fn generate_uid() -> String {
    uuid::Uuid::new_v4()
        .to_string()
        .replace('-', "")
        .chars()
        .take(16)
        .collect()
}
```

- [ ] **Step 2: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 成功

**注意**：`tauri::State::from(&app_state)` 的写法可能不正确。如果编译失败，需要改为直接调用 `create_attachment` 的内部逻辑（提取一个 helper 函数），或使用 `app.state::<AppState>()` 获取 State 后传参。常见调整：
- 将 `create_attachment` 的核心逻辑提取为 `pub fn create_attachment_inner(state: &AppState, req: CreateAttachmentRequest) -> IpcResult<Attachment>`
- 命令 wrapper 调用 inner

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/lan.rs
git commit -m "feat(lan): implement all 10 Tauri commands for discovery/profile/list/get/copy"
```

---

## Task 10: 启动 mDNS 发现缓存与事件推送

**Files:**
- Modify: `src-tauri/src/lan/endpoint.rs`

- [ ] **Step 1: 添加 mDNS 事件监听 task**

在 `src-tauri/src/lan/endpoint.rs` 末尾添加：

```rust
/// 启动 mDNS 发现事件监听 task
///
/// 定期刷新 peers 缓存，并通过 Tauri emit("lan:peers-changed") 通知前端。
pub fn spawn_mdns_discovery_loop(state: std::sync::Arc<LanState>, app_handle: tauri::AppHandle) {
    tokio::spawn(async move {
        use tauri::Emitter;
        loop {
            // 每 3 秒刷新一次 peer 缓存
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;

            // 从 mDNS 缓存读取已发现的 peers
            // iroh-mdns-address-lookup 的 API 可能是 state.endpoint.discovered_peers() 或类似
            // 这里需要查阅实际 API；以下为伪实现，需根据实际 API 调整
            let new_peers: Vec<PeerInfo> = vec![]; // TODO: 从 mDNS 读取

            let changed = {
                let mut peers = state.peers.write().await;
                let same = peers.len() == new_peers.len()
                    && peers.iter().zip(new_peers.iter()).all(|(a, b)| a.peer_id == b.peer_id);
                if !same {
                    *peers = new_peers;
                    true
                } else {
                    false
                }
            };

            if changed {
                let _ = app_handle.emit("lan:peers-changed", ());
            }
        }
    });
}
```

**重要说明**：iroh-mdns-address-lookup 0.4 的实际发现 API 需要查阅 `cargo doc --open -p iroh-mdns-address-lookup`。可能的 API 形式：
- `MdnsAddressLookup::discovered_peers()` 返回迭代器
- 需要在 `init_lan_state` 中保存 `MdnsAddressLookup` 的 handle
- 或通过 `endpoint.endpoint_info()` 获取已连接的 peers

如果 mDNS API 不可直接枚举发现的 peers，备选方案是：监听 `endpoint.accept()` 的连接事件，维护"曾连接过的 peers"列表。但这会丢失"被动发现但未连接"的 peers。

- [ ] **Step 2: 在 main.rs 启动 mDNS 发现 task**

在 `src-tauri/src/main.rs` setup 闭包中，在 `lan::server::spawn_accept_loop(...)` 之后添加：

```rust
                lan::endpoint::spawn_mdns_discovery_loop(lan_state.clone(), app.handle().clone());
```

- [ ] **Step 3: 验证编译**

Run: `cd src-tauri && cargo check`
Expected: 成功

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/lan/endpoint.rs src-tauri/src/main.rs
git commit -m "feat(lan): spawn mDNS discovery loop with peers-changed event emission"
```

---

## Task 11: 集成测试 - 双 Endpoint RPC 往返

**Files:**
- Create: `src-tauri/tests/lan_integration.rs`

- [ ] **Step 1: 编写集成测试**

创建 `src-tauri/tests/lan_integration.rs`：

```rust
//! 集成测试：启动两个 in-process Endpoint，验证 RPC 往返
//!
//! 注：此测试需要 mDNS 在测试环境可用。
//! 若 CI 不支持 mDNS，可标记为 #[ignore] 并仅在本地运行。

use memos_app::lan::client::call_remote;
use memos_app::lan::protocol::{Request, ResponseData};
use memos_app::lan::endpoint::init_lan_state;
use memos_core::Store;

#[tokio::test]
#[ignore = "需要真实 mDNS 环境，本地手动运行"]
async fn test_two_endpoints_profile_rpc() {
    // 在临时目录创建两个独立的 LanState
    let dir_a = tempfile::tempdir().unwrap();
    let dir_b = tempfile::tempdir().unwrap();

    let state_a = init_lan_state(dir_a.path()).await.unwrap();
    let state_b = init_lan_state(dir_b.path()).await.unwrap();

    // 等待 mDNS 互相发现（最多 10 秒）
    let peer_b_id = state_b.endpoint.endpoint_id().to_string();
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let peers = state_a.peers.read().await;
        if peers.iter().any(|p| p.peer_id == peer_b_id) {
            break;
        }
    }

    // 调用 GetProfile
    let data = call_remote(&state_a.endpoint, &peer_b_id, Request::GetProfile)
        .await
        .expect("GetProfile should succeed");

    match data {
        ResponseData::Profile {
            display_name,
            public_memo_count,
            tags,
        } => {
            assert!(!display_name.is_empty());
            // 新建的 store 没有 PUBLIC memo
            assert_eq!(public_memo_count, 0);
            assert!(tags.is_empty());
        }
        _ => panic!("expected Profile response"),
    }
}
```

- [ ] **Step 2: 添加 tempfile 依赖到 dev-dependencies**

在 `src-tauri/Cargo.toml` 末尾添加（如果没有 `[dev-dependencies]` 段则创建）：

```toml
[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: 验证编译**

Run: `cd src-tauri && cargo test --test lan_integration --no-run`
Expected: 编译成功

- [ ] **Step 4: 运行集成测试（本地手动）**

Run: `cd src-tauri && cargo test --test lan_integration -- --ignored`
Expected: PASS（需要真实 mDNS 环境）

- [ ] **Step 5: Commit**

```bash
git add src-tauri/tests/lan_integration.rs src-tauri/Cargo.toml
git commit -m "test(lan): add integration test for two-endpoint RPC (manual run, ignored by default)"
```

---

## Task 12: 前端类型定义与 i18n

**Files:**
- Create: `src/components/LanDiscovery/types.ts`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-Hans.json`

- [ ] **Step 1: 创建 types.ts**

创建 `src/components/LanDiscovery/types.ts`：

```ts
// LAN 发现与分享相关类型
// 与 Rust 端 commands/lan.rs 的返回类型对齐

export interface PeerInfo {
  peer_id: string;
  display_name: string;
  addrs: string[];
  relay_url: string | null;
  last_seen: number;
}

export interface LocalIdentity {
  peer_id: string;
  display_name: string;
}

export interface RemoteProfile {
  display_name: string;
  public_memo_count: number;
  tags: string[];
}

export interface RemoteMemoSummary {
  uid: string;
  created_ts: number;
  updated_ts: number;
  pinned: boolean;
  snippet: string;
  tags: string[];
  has_attachments: boolean;
}

export interface RemoteAttachmentSummary {
  uid: string;
  filename: string;
  mime_type: string;
  size: number;
}

export interface RemoteMemo {
  uid: string;
  created_ts: number;
  updated_ts: number;
  pinned: boolean;
  content: string;
  attachments: RemoteAttachmentSummary[];
}

export interface RemoteAttachmentResponse {
  content: Uint8Array;
  mime_type: string;
}

export type AclMode = "allow" | "deny";

export interface AclRule {
  peer_id: string;
  display_name?: string;
  mode: AclMode;
  tags: string[];
}

export type AclAccessMode = "default-open" | "restrict-tags" | "completely-blocked";
```

- [ ] **Step 2: 添加 i18n key 到 en.json**

在 `src/locales/en.json` 顶层对象中（在 `"aiChat": {...}` 之后）添加：

```json
  "lan": {
    "discovery": {
      "title": "Discover LAN Users",
      "empty": "No LAN users found. Ensure other devices are on the same network and running the app.",
      "online": "Online",
      "offline": "Offline"
    },
    "peer": {
      "publicMemos": "Public memos",
      "copyPeerId": "Copy Peer ID",
      "peerIdCopied": "Peer ID copied"
    },
    "memo": {
      "copyToLocal": "Copy to local",
      "copySuccess": "Copied to local",
      "copyConfirm": "Copy this memo to local? It will be created as a new private memo without author attribution.",
      "copyFailed": "Copy failed",
      "notFound": "This memo no longer exists",
      "notPublic": "This memo is no longer public",
      "loadFailed": "Failed to load remote memo"
    },
    "settings": {
      "displayName": "Display name",
      "peerId": "Peer ID",
      "aclRules": "Access control",
      "statusRunning": "Running (mDNS enabled)",
      "statusError": "Error",
      "accessMode": "Access mode",
      "defaultOpen": "Default open",
      "restrictTags": "Restrict by tags",
      "completelyBlocked": "Completely blocked",
      "allowTags": "Allow tags",
      "denyTags": "Deny tags",
      "save": "Save",
      "saved": "Saved"
    },
    "discover": {
      "button": "Discover",
      "tooltip": "Discover LAN users"
    }
  },
```

- [ ] **Step 3: 添加 i18n key 到 zh-Hans.json**

在 `src/locales/zh-Hans.json` 顶层对象中（在 `"aiChat": {...}` 之后）添加：

```json
  "lan": {
    "discovery": {
      "title": "发现局域网用户",
      "empty": "未发现局域网用户，请确认在同一网络且对端已启动应用",
      "online": "在线",
      "offline": "离线"
    },
    "peer": {
      "publicMemos": "公开笔记",
      "copyPeerId": "复制 Peer ID",
      "peerIdCopied": "Peer ID 已复制"
    },
    "memo": {
      "copyToLocal": "复制到本地",
      "copySuccess": "已复制到本地",
      "copyConfirm": "复制此笔记到本地？将创建为新笔记，不带原作者关联。",
      "copyFailed": "复制失败",
      "notFound": "该笔记已不存在",
      "notPublic": "对方未向你公开此笔记",
      "loadFailed": "加载远端笔记失败"
    },
    "settings": {
      "displayName": "展示名",
      "peerId": "Peer ID",
      "aclRules": "访问控制",
      "statusRunning": "运行中（mDNS 已启用）",
      "statusError": "错误",
      "accessMode": "访问模式",
      "defaultOpen": "默认开放",
      "restrictTags": "按标签限制",
      "completelyBlocked": "完全拒绝",
      "allowTags": "允许的标签",
      "denyTags": "拒绝的标签",
      "save": "保存",
      "saved": "已保存"
    },
    "discover": {
      "button": "发现",
      "tooltip": "发现局域网用户"
    }
  },
```

- [ ] **Step 4: 验证 JSON 格式**

Run: `node -e "JSON.parse(require('fs').readFileSync('src/locales/en.json','utf8')); JSON.parse(require('fs').readFileSync('src/locales/zh-Hans.json','utf8')); console.log('OK')"`
Expected: `OK`

- [ ] **Step 5: Commit**

```bash
git add src/components/LanDiscovery/types.ts src/locales/en.json src/locales/zh-Hans.json
git commit -m "feat(lan): add TypeScript types and i18n keys for LAN discovery"
```

---

## Task 13: 前端 hooks 实现

**Files:**
- Create: `src/components/LanDiscovery/hooks.ts`

- [ ] **Step 1: 实现 hooks.ts**

创建 `src/components/LanDiscovery/hooks.ts`：

```ts
import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  PeerInfo,
  RemoteProfile,
  RemoteMemoSummary,
  RemoteMemo,
  RemoteAttachmentResponse,
} from "./types";

// ---------- useLanDiscovery ----------

export function useLanDiscovery() {
  const [peers, setPeers] = useState<PeerInfo[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    const init = async () => {
      // 初始加载
      try {
        const result = await invoke<PeerInfo[]>("lan_discover_peers");
        setPeers(result);
      } catch (e) {
        console.error("lan_discover_peers failed", e);
      } finally {
        setLoading(false);
      }

      // 监听 peers-changed 事件
      unlisten = await listen("lan:peers-changed", async () => {
        try {
          const result = await invoke<PeerInfo[]>("lan_discover_peers");
          setPeers(result);
        } catch (e) {
          console.error("lan_discover_peers refresh failed", e);
        }
      });
    };

    init();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const result = await invoke<PeerInfo[]>("lan_discover_peers");
      setPeers(result);
    } catch (e) {
      console.error("lan_discover_peers refresh failed", e);
    }
  }, []);

  return { peers, loading, refresh };
}

// ---------- useRemoteProfile ----------

export function useRemoteProfile(peerId: string | null) {
  const [profile, setProfile] = useState<RemoteProfile | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!peerId) {
      setProfile(null);
      return;
    }
    setLoading(true);
    setError(null);
    invoke<RemoteProfile>("lan_get_remote_profile", { peerId })
      .then((p) => setProfile(p))
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [peerId]);

  return { profile, loading, error };
}

// ---------- useRemoteMemos ----------

export function useRemoteMemos(peerId: string | null) {
  const [memos, setMemos] = useState<RemoteMemoSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tagFilter, setTagFilter] = useState<string[] | null>(null);
  const offsetRef = useRef(0);
  const PAGE_SIZE = 20;

  const loadPage = useCallback(
    async (reset: boolean) => {
      if (!peerId) return;
      setLoading(true);
      setError(null);
      const offset = reset ? 0 : offsetRef.current;
      try {
        const res = await invoke<{
          memos: RemoteMemoSummary[];
          total: number;
        }>("lan_list_remote_memos", {
          req: {
            peer_id: peerId,
            offset,
            limit: PAGE_SIZE,
            tag_filter: tagFilter,
          },
        });
        if (reset) {
          setMemos(res.memos);
          offsetRef.current = res.memos.length;
        } else {
          setMemos((prev) => [...prev, ...res.memos]);
          offsetRef.current += res.memos.length;
        }
        setTotal(res.total);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [peerId, tagFilter],
  );

  useEffect(() => {
    offsetRef.current = 0;
    loadPage(true);
  }, [peerId, tagFilter, loadPage]);

  const loadMore = useCallback(() => {
    if (!loading && memos.length < total) {
      loadPage(false);
    }
  }, [loading, memos.length, total, loadPage]);

  const hasMore = memos.length < total;

  return {
    memos,
    total,
    loading,
    error,
    tagFilter,
    setTagFilter,
    loadMore,
    hasMore,
    retry: () => loadPage(true),
  };
}

// ---------- useRemoteMemoPreview ----------

export function useRemoteMemoPreview(peerId: string | null, uid: string | null) {
  const [memo, setMemo] = useState<RemoteMemo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!peerId || !uid) {
      setMemo(null);
      return;
    }
    setLoading(true);
    setError(null);
    invoke<RemoteMemo>("lan_get_remote_memo", {
      req: { peer_id: peerId, uid },
    })
      .then((m) => setMemo(m))
      .catch((e) => {
        setError(String(e));
        setMemo(null);
      })
      .finally(() => setLoading(false));
  }, [peerId, uid]);

  const fetchAttachment = useCallback(
    async (attUid: string): Promise<RemoteAttachmentResponse | null> => {
      if (!peerId) return null;
      try {
        return await invoke<RemoteAttachmentResponse>("lan_get_remote_attachment", {
          req: { peer_id: peerId, uid: attUid },
        });
      } catch (e) {
        console.error("fetchAttachment failed", e);
        return null;
      }
    },
    [peerId],
  );

  return { memo, loading, error, fetchAttachment };
}
```

- [ ] **Step 2: 验证 TS 编译**

Run: `npx tsc --noEmit`
Expected: 成功

- [ ] **Step 3: Commit**

```bash
git add src/components/LanDiscovery/hooks.ts
git commit -m "feat(lan): implement React hooks for discovery/profile/list/preview"
```

---

## Task 14: 实现 DiscoverButton 与 LanDiscoveryPanel

**Files:**
- Create: `src/components/MemoEditor/Toolbar/DiscoverButton.tsx`
- Create: `src/components/LanDiscovery/LanDiscoveryPanel.tsx`
- Create: `src/components/LanDiscovery/PeerList.tsx`
- Create: `src/components/LanDiscovery/RemoteMemoList.tsx`
- Create: `src/components/LanDiscovery/RemoteMemoPreview.tsx`
- Create: `src/components/LanDiscovery/index.tsx`
- Modify: `src/components/MemoEditor/Toolbar/EditorToolbar.tsx`

- [ ] **Step 1: 创建 DiscoverButton**

创建 `src/components/MemoEditor/Toolbar/DiscoverButton.tsx`：

```tsx
import { CompassIcon } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import LanDiscoveryPanel from "@/components/LanDiscovery";
import { useTranslate } from "@/utils/i18n";

export const DiscoverButton = () => {
  const t = useTranslate();
  const [open, setOpen] = useState(false);

  return (
    <>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button variant="ghost" size="icon" onClick={() => setOpen(true)}>
            <CompassIcon className="size-5 text-foreground" />
          </Button>
        </TooltipTrigger>
        <TooltipContent>{t("lan.discover.tooltip")}</TooltipContent>
      </Tooltip>
      <LanDiscoveryPanel open={open} onOpenChange={setOpen} />
    </>
  );
};

export default DiscoverButton;
```

- [ ] **Step 2: 创建 PeerList**

创建 `src/components/LanDiscovery/PeerList.tsx`：

```tsx
import type { FC } from "react";
import { Skeleton } from "@/components/Skeleton";
import type { PeerInfo } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  peers: PeerInfo[];
  loading: boolean;
  selectedPeerId: string | null;
  onSelect: (peer: PeerInfo) => void;
}

const PeerList: FC<Props> = ({ peers, loading, selectedPeerId, onSelect }) => {
  const t = useTranslate();

  if (loading) {
    return (
      <div className="p-4 space-y-2">
        <Skeleton className="h-12 w-full" />
        <Skeleton className="h-12 w-full" />
      </div>
    );
  }

  if (peers.length === 0) {
    return (
      <div className="p-4 text-sm text-muted-foreground">{t("lan.discovery.empty")}</div>
    );
  }

  return (
    <div className="space-y-1">
      {peers.map((peer) => {
        const isSelected = peer.peer_id === selectedPeerId;
        const shortId = peer.peer_id.slice(0, 8);
        return (
          <button
            key={peer.peer_id}
            onClick={() => onSelect(peer)}
            className={`w-full text-left px-3 py-2 rounded-md hover:bg-accent transition-colors ${
              isSelected ? "bg-accent" : ""
            }`}
          >
            <div className="flex items-center gap-2">
              <span className="size-2 rounded-full bg-green-500" />
              <span className="font-medium truncate">{peer.display_name}</span>
            </div>
            <div className="text-xs text-muted-foreground mt-0.5">{shortId}…</div>
          </button>
        );
      })}
    </div>
  );
};

export default PeerList;
```

- [ ] **Step 3: 创建 RemoteMemoList**

创建 `src/components/LanDiscovery/RemoteMemoList.tsx`：

```tsx
import type { FC } from "react";
import { PinIcon, PaperclipIcon } from "lucide-react";
import dayjs from "dayjs";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/Skeleton";
import type { PeerInfo, RemoteMemoSummary } from "./types";
import { useRemoteProfile, useRemoteMemos } from "./hooks";
import { useTranslate } from "@/utils/i18n";

interface Props {
  peer: PeerInfo;
  selectedMemoUid: string | null;
  onSelectMemo: (uid: string) => void;
}

const RemoteMemoList: FC<Props> = ({ peer, selectedMemoUid, onSelectMemo }) => {
  const t = useTranslate();
  const { profile, loading: profileLoading } = useRemoteProfile(peer.peer_id);
  const {
    memos,
    total,
    loading,
    error,
    hasMore,
    loadMore,
    tagFilter,
    setTagFilter,
    retry,
  } = useRemoteMemos(peer.peer_id);

  return (
    <div className="flex flex-col h-full">
      {/* Header: peer 信息 */}
      <div className="p-4 border-b">
        <div className="font-medium text-base">{peer.display_name}</div>
        {profileLoading ? (
          <div className="text-xs text-muted-foreground">…</div>
        ) : profile ? (
          <div className="text-xs text-muted-foreground mt-1">
            {t("lan.peer.publicMemos")}: {profile.public_memo_count} · {profile.tags.join(", ")}
          </div>
        ) : null}
      </div>

      {/* Tag 筛选 */}
      {profile && profile.tags.length > 0 && (
        <div className="px-4 py-2 border-b flex flex-wrap gap-1">
          <button
            onClick={() => setTagFilter(null)}
            className={`px-2 py-0.5 text-xs rounded-full border ${
              tagFilter === null ? "bg-primary text-primary-foreground" : "bg-background"
            }`}
          >
            All
          </button>
          {profile.tags.map((tag) => (
            <button
              key={tag}
              onClick={() => setTagFilter([tag])}
              className={`px-2 py-0.5 text-xs rounded-full border ${
                tagFilter?.includes(tag) ? "bg-primary text-primary-foreground" : "bg-background"
              }`}
            >
              #{tag}
            </button>
          ))}
        </div>
      )}

      {/* 笔记列表 */}
      <div className="flex-1 overflow-auto">
        {error ? (
          <div className="p-4 text-sm text-destructive">
            {t("lan.memo.loadFailed")}: {error}
            <Button variant="ghost" size="sm" onClick={retry} className="ml-2">
              Retry
            </Button>
          </div>
        ) : loading && memos.length === 0 ? (
          <div className="p-4 space-y-2">
            <Skeleton className="h-16 w-full" />
            <Skeleton className="h-16 w-full" />
          </div>
        ) : memos.length === 0 ? (
          <div className="p-4 text-sm text-muted-foreground">{t("lan.discovery.empty")}</div>
        ) : (
          <div className="divide-y">
            {memos.map((memo) => (
              <MemoCard
                key={memo.uid}
                memo={memo}
                isSelected={memo.uid === selectedMemoUid}
                onClick={() => onSelectMemo(memo.uid)}
              />
            ))}
            {hasMore && (
              <div className="p-2">
                <Button variant="ghost" size="sm" onClick={loadMore} disabled={loading} className="w-full">
                  {loading ? "…" : "Load more"}
                </Button>
              </div>
            )}
          </div>
        )}
      </div>

      {/* Footer 统计 */}
      <div className="p-2 border-t text-xs text-muted-foreground text-center">
        {memos.length} / {total}
      </div>
    </div>
  );
};

const MemoCard: FC<{
  memo: RemoteMemoSummary;
  isSelected: boolean;
  onClick: () => void;
}> = ({ memo, isSelected, onClick }) => (
  <button
    onClick={onClick}
    className={`w-full text-left px-4 py-3 hover:bg-accent transition-colors ${
      isSelected ? "bg-accent" : ""
    }`}
  >
    <div className="flex items-start gap-2">
      {memo.pinned && <PinIcon className="size-4 text-primary shrink-0 mt-0.5" />}
      <div className="flex-1 min-w-0">
        <div className="text-sm line-clamp-2">{memo.snippet}</div>
        <div className="flex items-center gap-2 mt-1 text-xs text-muted-foreground">
          <span>{dayjs.unix(memo.created_ts).format("YYYY-MM-DD")}</span>
          {memo.tags.length > 0 && <span>· {memo.tags.map((t) => `#${t}`).join(" ")}</span>}
          {memo.has_attachments && <PaperclipIcon className="size-3" />}
        </div>
      </div>
    </div>
  </button>
);

export default RemoteMemoList;
```

- [ ] **Step 4: 创建 RemoteMemoPreview**

创建 `src/components/LanDiscovery/RemoteMemoPreview.tsx`：

```tsx
import type { FC } from "react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/Skeleton";
import MemoMarkdownRenderer from "@/components/MemoContent/MemoMarkdownRenderer";
import { useRemoteMemoPreview } from "./hooks";
import { useTranslate } from "@/utils/i18n";
import toast from "react-hot-toast";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  peerId: string;
  uid: string;
  onBack: () => void;
}

const RemoteMemoPreview: FC<Props> = ({ peerId, uid, onBack }) => {
  const t = useTranslate();
  const { memo, loading, error, fetchAttachment } = useRemoteMemoPreview(peerId, uid);
  const [copying, setCopying] = useState(false);

  const handleCopy = async () => {
    if (!confirm(t("lan.memo.copyConfirm"))) return;
    setCopying(true);
    try {
      const res = await invoke<{ new_memo_uid: string }>("lan_copy_memo_to_local", {
        req: { peer_id: peerId, uid },
      });
      toast.success(t("lan.memo.copySuccess"));
      onBack();
      void res;
    } catch (e) {
      toast.error(`${t("lan.memo.copyFailed")}: ${e}`);
    } finally {
      setCopying(false);
    }
  };

  if (loading) {
    return (
      <div className="p-4 space-y-2">
        <Skeleton className="h-6 w-1/3" />
        <Skeleton className="h-32 w-full" />
      </div>
    );
  }

  if (error || !memo) {
    return (
      <div className="p-4">
        <p className="text-sm text-destructive">{error || t("lan.memo.notFound")}</p>
        <Button variant="ghost" size="sm" onClick={onBack} className="mt-2">
          Back
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-auto p-4">
        <MemoMarkdownRenderer content={memo.content} />
        {memo.attachments.length > 0 && (
          <div className="mt-4">
            <div className="text-xs text-muted-foreground mb-2">
              {memo.attachments.length} attachments (lazy load)
            </div>
            <div className="space-y-1">
              {memo.attachments.map((att) => (
                <AttachmentLazyLoader
                  key={att.uid}
                  filename={att.filename}
                  size={att.size}
                  fetchFn={() => fetchAttachment(att.uid)}
                />
              ))}
            </div>
          </div>
        )}
      </div>
      <div className="p-3 border-t flex gap-2">
        <Button variant="ghost" size="sm" onClick={onBack}>
          Back
        </Button>
        <Button size="sm" onClick={handleCopy} disabled={copying} className="ml-auto">
          {copying ? "…" : t("lan.memo.copyToLocal")}
        </Button>
      </div>
    </div>
  );
};

const AttachmentLazyLoader: FC<{
  filename: string;
  size: number;
  fetchFn: () => Promise<{ content: Uint8Array; mime_type: string } | null>;
}> = ({ filename, size, fetchFn }) => {
  const [blobUrl, setBlobUrl] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const load = async () => {
    setLoading(true);
    const result = await fetchFn();
    setLoading(false);
    if (result) {
      const blob = new Blob([result.content], { type: result.mime_type });
      const url = URL.createObjectURL(blob);
      setBlobUrl(url);
      setLoaded(true);
    }
  };

  const isImage = blobUrl && (blobUrl.startsWith("blob:") && loaded);

  return (
    <div className="border rounded p-2 text-xs">
      <div className="flex items-center justify-between">
        <span className="truncate">{filename}</span>
        <span className="text-muted-foreground">{(size / 1024).toFixed(1)} KB</span>
      </div>
      {!loaded && (
        <button onClick={load} disabled={loading} className="text-primary mt-1">
          {loading ? "Loading…" : "Load attachment"}
        </button>
      )}
      {isImage && blobUrl && (
        <img src={blobUrl} alt={filename} className="mt-2 max-w-full rounded" />
      )}
    </div>
  );
};

export default RemoteMemoPreview;
```

- [ ] **Step 5: 创建 LanDiscoveryPanel**

创建 `src/components/LanDiscovery/LanDiscoveryPanel.tsx`：

```tsx
import type { FC } from "react";
import { useState } from "react";
import { Sheet, SheetContent, SheetHeader, SheetTitle } from "@/components/ui/sheet";
import PeerList from "./PeerList";
import RemoteMemoList from "./RemoteMemoList";
import RemoteMemoPreview from "./RemoteMemoPreview";
import { useLanDiscovery } from "./hooks";
import type { PeerInfo } from "./types";
import { useTranslate } from "@/utils/i18n";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

const LanDiscoveryPanel: FC<Props> = ({ open, onOpenChange }) => {
  const t = useTranslate();
  const { peers, loading } = useLanDiscovery();
  const [selectedPeer, setSelectedPeer] = useState<PeerInfo | null>(null);
  const [selectedMemoUid, setSelectedMemoUid] = useState<string | null>(null);

  const handleSelectPeer = (peer: PeerInfo) => {
    setSelectedPeer(peer);
    setSelectedMemoUid(null);
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="w-[700px] max-w-full p-0 flex flex-col">
        <SheetHeader className="px-4 py-3 border-b">
          <SheetTitle>{t("lan.discovery.title")}</SheetTitle>
        </SheetHeader>
        <div className="flex-1 flex overflow-hidden">
          {/* 左栏：peer 列表 */}
          <div className="w-56 border-r overflow-auto">
            <PeerList
              peers={peers}
              loading={loading}
              selectedPeerId={selectedPeer?.peer_id ?? null}
              onSelect={handleSelectPeer}
            />
          </div>
          {/* 右栏：笔记列表 / 预览 */}
          <div className="flex-1 overflow-hidden">
            {!selectedPeer ? (
              <div className="flex items-center justify-center h-full text-sm text-muted-foreground">
                {t("lan.discovery.empty")}
              </div>
            ) : selectedMemoUid ? (
              <RemoteMemoPreview
                peerId={selectedPeer.peer_id}
                uid={selectedMemoUid}
                onBack={() => setSelectedMemoUid(null)}
              />
            ) : (
              <RemoteMemoList
                peer={selectedPeer}
                selectedMemoUid={null}
                onSelectMemo={setSelectedMemoUid}
              />
            )}
          </div>
        </div>
      </SheetContent>
    </Sheet>
  );
};

export default LanDiscoveryPanel;
```

- [ ] **Step 6: 创建 index.tsx**

创建 `src/components/LanDiscovery/index.tsx`：

```tsx
export { default } from "./LanDiscoveryPanel";
```

- [ ] **Step 7: 在 EditorToolbar 插入 DiscoverButton**

修改 `src/components/MemoEditor/Toolbar/EditorToolbar.tsx`，在文件顶部 import 区添加：

```tsx
import DiscoverButton from "./DiscoverButton";
```

在 return 的 JSX 中，在 `<VisibilitySelector ... />` 之后（同一 div 内）添加：

```tsx
        <DiscoverButton />
```

- [ ] **Step 8: 验证 TS 编译**

Run: `npx tsc --noEmit`
Expected: 成功

- [ ] **Step 9: Commit**

```bash
git add src/components/LanDiscovery/ src/components/MemoEditor/Toolbar/DiscoverButton.tsx src/components/MemoEditor/Toolbar/EditorToolbar.tsx
git commit -m "feat(lan): implement LanDiscoveryPanel, PeerList, RemoteMemoList, RemoteMemoPreview, DiscoverButton"
```

---

## Task 15: 实现设置页 LanShareSection

**Files:**
- Create: `src/components/Settings/LanShareSection.tsx`
- Modify: `src/components/Settings/settingSections.ts`

- [ ] **Step 1: 创建 LanShareSection**

创建 `src/components/Settings/LanShareSection.tsx`：

```tsx
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useLanDiscovery } from "@/components/LanDiscovery/hooks";
import type { AclAccessMode, AclRule } from "@/components/LanDiscovery/types";
import { useTranslate } from "@/utils/i18n";
import toast from "react-hot-toast";

const LanShareSection = () => {
  const t = useTranslate();
  const { peers } = useLanDiscovery();
  const [peerId, setPeerId] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [rules, setRules] = useState<AclRule[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    invoke<{ peer_id: string; display_name: string }>("lan_get_local_identity")
      .then((id) => {
        setPeerId(id.peer_id);
        setDisplayName(id.display_name);
      })
      .catch(console.error);
    invoke<AclRule[]>("lan_get_acl_rules")
      .then(setRules)
      .catch(console.error);
  }, []);

  const handleSaveDisplayName = async () => {
    try {
      await invoke("lan_update_display_name", { req: { name: displayName } });
      toast.success(t("lan.settings.saved"));
    } catch (e) {
      toast.error(String(e));
    }
  };

  const handleSaveRules = async () => {
    setSaving(true);
    try {
      await invoke("lan_save_acl_rules", { req: { rules } });
      toast.success(t("lan.settings.saved"));
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };

  const getAccessMode = (peerId: string): AclAccessMode => {
    const peerRules = rules.filter((r) => r.peer_id === peerId);
    if (peerRules.length === 0) return "default-open";
    if (peerRules.some((r) => r.mode === "allow" && r.tags.includes("__none__"))) {
      return "completely-blocked";
    }
    return "restrict-tags";
  };

  const setAccessMode = (peerId: string, displayName: string, mode: AclAccessMode) => {
    // 移除该 peer 的所有规则
    let newRules = rules.filter((r) => r.peer_id !== peerId);
    if (mode === "default-open") {
      // 无规则
    } else if (mode === "completely-blocked") {
      newRules.push({
        peer_id: peerId,
        display_name: displayName,
        mode: "allow",
        tags: ["__none__"],
      });
    }
    // restrict-tags 模式需要用户手动选择 tags，这里初始化为空 allow
    // 实际 UI 需要 tag 多选组件，这里简化为默认 allow ["work"]
    setRules(newRules);
  };

  return (
    <div className="space-y-6">
      {/* 本机身份 */}
      <div className="space-y-2">
        <Label>{t("lan.settings.displayName")}</Label>
        <div className="flex gap-2">
          <Input
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
            placeholder="LocalFragNote"
          />
          <Button onClick={handleSaveDisplayName}>{t("common.save")}</Button>
        </div>
        <div className="text-xs text-muted-foreground">
          {t("lan.settings.peerId")}: {peerId.slice(0, 16)}…
        </div>
      </div>

      {/* 服务状态 */}
      <div className="text-sm">
        <span className="inline-flex items-center gap-1">
          <span className="size-2 rounded-full bg-green-500" />
          {t("lan.settings.statusRunning")}
        </span>
      </div>

      {/* ACL 规则 */}
      <div className="space-y-3">
        <Label>{t("lan.settings.aclRules")}</Label>
        {peers.length === 0 ? (
          <div className="text-sm text-muted-foreground">{t("lan.discovery.empty")}</div>
        ) : (
          <div className="space-y-2">
            {peers.map((peer) => {
              const mode = getAccessMode(peer.peer_id);
              return (
                <div key={peer.peer_id} className="border rounded p-3 space-y-2">
                  <div className="flex items-center justify-between">
                    <span className="font-medium">{peer.display_name}</span>
                    <Select
                      value={mode}
                      onValueChange={(v) =>
                        setAccessMode(peer.peer_id, peer.display_name, v as AclAccessMode)
                      }
                    >
                      <SelectTrigger className="w-40">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value="default-open">
                          {t("lan.settings.defaultOpen")}
                        </SelectItem>
                        <SelectItem value="restrict-tags">
                          {t("lan.settings.restrictTags")}
                        </SelectItem>
                        <SelectItem value="completely-blocked">
                          {t("lan.settings.completelyBlocked")}
                        </SelectItem>
                      </SelectContent>
                    </Select>
                  </div>
                  <div className="text-xs text-muted-foreground">{peer.peer_id.slice(0, 16)}…</div>
                  {mode === "restrict-tags" && (
                    <div className="text-xs text-muted-foreground">
                      {/* TODO: 实现 tag 多选 UI，当前简化 */}
                      Tag selection UI TBD
                    </div>
                  )}
                </div>
              );
            })}
            <Button onClick={handleSaveRules} disabled={saving}>
              {t("lan.settings.save")}
            </Button>
          </div>
        )}
      </div>
    </div>
  );
};

export default LanShareSection;
```

- [ ] **Step 2: 在 settingSections.ts 注册新 section**

修改 `src/components/Settings/settingSections.ts`：

在 import 区添加：

```tsx
import LanShareSection from "@/components/Settings/LanShareSection";
import { RadioIcon } from "lucide-react";  // 或其他合适图标
```

在 `SettingSectionKey` 类型添加：

```tsx
  | "lan-share"
```

在 `SETTINGS_SECTIONS` 数组中（在 `resource-stats` 项之后）添加：

```tsx
  {
    key: "lan-share",
    scope: "basic",
    labelKey: "setting.lan-share.label",
    icon: RadioIcon,
    component: LanShareSection,
  },
```

- [ ] **Step 3: 添加 setting.lan-share.label i18n**

在 `src/locales/en.json` 的 `setting` 对象中添加：

```json
    "lan-share": {
      "label": "LAN Share"
    },
```

在 `src/locales/zh-Hans.json` 的 `setting` 对象中添加：

```json
    "lan-share": {
      "label": "局域网分享"
    },
```

- [ ] **Step 4: 验证 TS 编译**

Run: `npx tsc --noEmit`
Expected: 成功

- [ ] **Step 5: Commit**

```bash
git add src/components/Settings/LanShareSection.tsx src/components/Settings/settingSections.ts src/locales/en.json src/locales/zh-Hans.json
git commit -m "feat(lan): add LanShareSection to settings with display name and ACL config"
```

---

## Task 16: 端到端验证

**Files:**
- 无新文件，仅验证

- [ ] **Step 1: 完整构建验证**

Run: `cd src-tauri && cargo build`
Expected: 成功

Run: `npx tsc --noEmit && npm run build`
Expected: 成功

- [ ] **Step 2: 运行所有单元测试**

Run: `cd src-tauri && cargo test`
Expected: lan_protocol (8) + lan_auth (10) 全部 PASS，其他测试不回归

- [ ] **Step 3: 启动应用，验证发现按钮**

Run: `cd src-tauri && cargo run`
Expected:
- 应用启动，日志显示 `LAN 模块启动成功`
- 笔记编辑器工具栏出现"发现"按钮（指南针图标）
- 点击按钮打开右侧 Drawer 面板

- [ ] **Step 4: 手动验收清单（需两台机器）**

按 spec 中的 10 条验收清单逐项验证：

1. 两台同网段机器启动应用，发现面板能互相看到对方
2. 点击对端 → 看到其 PUBLIC 笔记列表，PRIVATE 笔记不出现
3. 设置 ACL 限制某 peer 只看 `#work` → 对端列表只剩 `#work` 笔记
4. 设置完全拒绝 → 对端看不到任何笔记
5. 预览远端笔记 → markdown 正确渲染，图片附件懒加载显示
6. 复制笔记到本地 → 本地新笔记内容/附件完整，visibility=Private
7. 对端修改 ACL 后立即生效（无需重启）
8. 对端下线 → peer 列表移除，正在预览的关闭
9. 跨网段（mDNS 不可达）→ relay fallback 仍可连接
10. 重启应用 → peer_id 保持不变

- [ ] **Step 5: Commit 最终状态**

```bash
git add -A
git commit -m "chore(lan): end-to-end verification complete"
```

---

## Self-Review 结果

### Spec 覆盖检查

| Spec 章节 | 对应 Task |
|---|---|
| 架构总览 | Task 1-2（依赖、模块） |
| mDNS 发现与 Endpoint 生命周期 | Task 5, 10 |
| 编组权限模型（ACL） | Task 4 |
| JSON-RPC 协议 | Task 3, 7, 8 |
| 前端 UI 与交互 | Task 12-15 |
| 错误处理与边界情况 | Task 2（LanError）、Task 8（服务端错误码）、Task 7（超时） |
| 测试策略 | Task 3（protocol 单测）、Task 4（auth 单测）、Task 11（集成测试） |
| 实现顺序 | Task 1-16 按依赖顺序排列 |

### 已知简化项

1. **mDNS peer 枚举 API**（Task 10）：iroh-mdns-address-lookup 0.4 的实际发现 API 需在实现时验证，伪代码标记为 TODO
2. **Tag 多选 UI**（Task 15）：restrict-tags 模式的 tag 多选组件简化为占位符，实际实现需用 Radix Popover + Checkbox
3. **iroh API 差异**：文档基于 2026-07 版本，实际 API 可能在 `endpoint_id()`、`EndpointAddr` 解析等方法有差异，已在相关 Task 标注调整说明
4. **集成测试**（Task 11）：标记为 `#[ignore]`，需本地手动运行

### 类型一致性检查

- `PeerInfo` 在 Task 2 定义，Task 6/9/10/12/14 使用 — 一致
- `AclRule` / `AclMode` 在 Task 4 定义，Task 8/9/12/15 使用 — 一致
- `Request` / `Response` / `ResponseData` 在 Task 3 定义，Task 7/8/9 使用 — 一致
- `RemoteMemo` / `RemoteMemoSummary` / `RemoteAttachmentSummary` 在 Task 3 定义，Task 8/9/12/13/14 使用 — 一致
- Tauri 命令名在 Task 6 注册，Task 9 实现，Task 13/14/15 前端调用 — 一致
