# iroh 局域网发现、编组与分享

参考 [iroh](https://github.com/n0-computer/iroh) P2P 网络栈，在 LocalFragNote 中实现局域网用户发现、按标签编组的权限控制，以及按需查看/复制他人公开笔记的功能。采用被动公开 + 按需发现 + 按需复制模型，不实现主动同步。

## 目标

- 工具栏新增"发现"按钮，点击后展示当前局域网内运行本应用的其他用户
- 默认可查看其他用户 `PUBLIC` 可见性的笔记（只读预览，不下载到本地）
- 通过标签编组：配置哪些 peer 可看哪些标签对应的笔记
- 可将他人笔记（含附件）单条复制到本地，作为新笔记独立存在
- 全程基于 iroh QUIC + mDNS，无需中心服务器

## 范围

- **发现机制**：`iroh-mdns-address-lookup` 局域网自动发现，relay 作为跨网段 fallback
- **协议**：自定义 ALPN `memos/lan-share/1` 上的 JSON-RPC（请求-响应模型）
- **权限**：基于 peer_id（公钥）+ tag 的 ACL，存 `app_setting` 表
- **复制语义**：单条复制，重新生成 uid，visibility=Private，不保留来源元数据
- **附件**：查看时懒加载（按需拉取），复制时默认连同附件一起复制

### 不包含

- 批量复制
- 附件断点续传（单帧整体返回，60 秒超时）
- 实时同步/订阅（被动模型，无主动推送）
- 历史发现记录持久化
- 限流/反滥用
- 跨版本协议兼容（首版只有 ALPN v1）
- E2E 自动化测试（用手动验收清单替代）

## 架构

### 技术栈

- **iroh** `=1.0` — QUIC Endpoint，提供加密的 P2P 连接
- **iroh-mdns-address-lookup** `=0.4` — mDNS 局域网自动发现
- 不引入 iroh-blobs / iroh-docs / iroh-gossip

### 模块边界

```
src-tauri/src/
├── lan/                          # 新增模块
│   ├── mod.rs                    # 模块入口 + 启动/停止 + LanError
│   ├── endpoint.rs               # iroh Endpoint 初始化、mDNS 发现
│   ├── server.rs                 # accept 循环 + 请求分发
│   ├── protocol.rs               # JSON-RPC 请求/响应类型 + 编解码
│   ├── auth.rs                   # 编组权限过滤（基于 peer id 查 ACL）
│   └── client.rs                 # 发起连接 + 请求远端
├── commands/
│   └── lan.rs                    # 新增 Tauri 命令
└── state.rs                      # 扩展：持有 LanState（Endpoint + 发现缓存）

src/components/LanDiscovery/
├── index.tsx
├── LanDiscoveryPanel.tsx         # 发现面板（用户列表 + 笔记列表）
├── RemoteMemoList.tsx            # 远端笔记列表
├── RemoteMemoPreview.tsx         # 远端笔记预览（含附件懒加载）
└── hooks.ts                      # useLanDiscovery / useRemoteMemos

src/components/MemoEditor/Toolbar/  # 新增"发现"按钮入口
src/components/Settings/
└── LanShareSection.tsx           # 设置：本机展示名、编组配置
```

### 数据流概览

```
[本地用户]                                    [局域网其他用户]
    │                                              │
    │  ① mDNS 广播 + 发现                          │
    │◀───────────────────────────────────────────▶│
    │                                              │
    │  ② 点击"发现"按钮                             │
    │     → invoke lan_discover_peers              │
    │     → 返回 [{peer_id, display_name, addr}]   │
    │                                              │
    │  ③ 选择用户                                   │
    │     → invoke lan_list_remote_memos(peer)     │
    │     → QUIC 连接 + JSON-RPC list_public_memos │
    │     → 远端按 ACL 过滤后返回笔记元数据           │
    │◀───────────────────────────────────────────▶│
    │                                              │
    │  ④ 预览笔记                                   │
    │     → invoke lan_get_remote_memo(peer, uid)  │
    │     → JSON-RPC get_memo → 返回 content        │
    │     → 图片附件按需 lan_get_remote_attachment  │
    │◀───────────────────────────────────────────▶│
    │                                              │
    │  ⑤ 复制到本地                                 │
    │     → invoke lan_copy_memo_to_local(peer,uid)│
    │     → 拉 content + 附件 → create_memo 本地    │
    │                                              │
```

### 关键决策

- **被动服务**：iroh Endpoint 在应用启动时即绑定并监听，不需要用户主动"上线"。其他用户随时可以发现并查询
- **无状态查询**：每次请求独立，不维护会话。复制操作是把远端数据拉过来再走本地 `create_memo`，不在本地保留任何"远端来源"标记
- **权限模型位置**：ACL 存在本地 SQLite（`app_setting`），服务端在响应 `list_public_memos` 时按请求方 peer_id 过滤
- **异步隔离**：所有 iroh 操作在 tokio runtime，Tauri 命令用 `async`，不阻塞 UI。复用现有 `spawn_blocking` 模式处理 SQLite 访问

## mDNS 发现与 Endpoint 生命周期

### Endpoint 初始化

应用启动时（`main.rs` 的 `setup` 阶段）初始化 iroh Endpoint：

- **SecretKey 持久化**：生成一次 Ed25519 密钥对，存到 `app_data_dir/lan_identity.key`（32 字节原始密钥）。重启保持同一 `EndpointId`，其他设备能把"用户"与稳定身份关联
- **ALPN 注册**：`b"memos/lan-share/1"`（带版本号，未来协议升级留余地）
- **mDNS 启用**：`MdnsAddressLookup::builder()` 与默认 DNS lookup 并存——局域网内走 mDNS 直连，跨网段时 fallback 到 relay
- **Relay 模式**：`RelayMode::Default`（n0 公共 relay 作为 fallback，仅在 mDNS 失败时才用到）

```rust
// 伪代码
let secret_key = load_or_create_secret(&data_dir.join("lan_identity.key"))?;
let endpoint = Endpoint::builder(presets::N0)
    .secret_key(secret_key)
    .alpns(vec![ALPN.to_vec()])
    .address_lookup(MdnsAddressLookup::builder())
    .bind()
    .await?;
```

### 展示名广播

mDNS 只暴露 `EndpointId`（公钥），但用户需要看到可读名字：

- **机制**：在 mDNS 服务广播的 TXT 记录里附加 `display_name`（`iroh-mdns-address-lookup` 支持自定义 metadata）
- **来源**：从 `instance_setting:lan_display_name` 读取，默认值 `"LocalFragNote"`，用户可在设置页修改
- **更新**：修改展示名后调用 `MdnsAddressLookup::update_metadata()` 重新广播

### 发现 API

Tauri 命令 `lan_discover_peers`：

```rust
#[tauri::command]
async fn lan_discover_peers(state: tauri::State<'_, AppState>) -> Result<Vec<PeerInfo>, String>
```

返回：
```ts
type PeerInfo = {
  peer_id: string;        // EndpointId 的 base32 编码
  display_name: string;   // 从 mDNS TXT 记录读取
  addrs: string[];        // 直连地址（IPv4/IPv6）
  relay_url: string | null;
  last_seen: number;      // epoch seconds
};
```

**实现策略**：
- `MdnsAddressLookup` 维护一个发现缓存，内部已订阅 mDNS 服务变更事件
- `lan_discover_peers` 直接读缓存快照（不阻塞、不发起额外网络请求）
- 启动一个后台 tokio task 监听 mDNS 事件，更新缓存并通过 Tauri `emit("lan:peers-changed")` 通知前端实时刷新

### Endpoint 停止

应用退出时 `endpoint.close().await` 优雅关闭，mDNS 自动注销服务广播。

### Tauri 权限

`src-tauri/capabilities/default.json` 可能需要添加网络权限（QUIC UDP 端口监听 + mDNS 多播）。实现时验证 Tauri 2 capability 体系是否需要显式声明。

## 编组权限模型（ACL）

### 数据模型

ACL 规则存在 `app_setting`，key = `lan_acl_rules`，值为 JSON 数组：

```ts
type AclRules = AclRule[];

type AclRule = {
  peer_id: string;       // 对端 EndpointId（base32）
  display_name?: string; // 备注（方便用户识别），UI 首次发现 peer 时自动填充
  mode: "allow" | "deny";
  tags: string[];        // 匹配的 tag 列表，必须非空（见下方算法）
};
```

### 过滤算法

对每个请求方 `peer_id`，服务端在 `list_public_memos` 时按以下顺序过滤：

1. **基础可见性**：只查 `visibility = 'PUBLIC'` 的笔记
2. **匹配该 peer 的规则**：收集所有 `peer_id` 匹配的规则
3. **规则匹配**（所有规则的 `tags` 必须非空，UI 校验）：
   - `allow` 规则的 tags 取并集 → `allow_tags`；`deny` 规则的 tags 取并集 → `deny_tags`
   - 笔记可见条件：`allow_tags` 为空（无 allow 规则）**或** 笔记含 `allow_tags` 中任一 tag；**且** 笔记不含 `deny_tags` 中任一 tag
   - 即 deny 优先：被 deny 的 tag 一定不可见，无论是否在 allow 中
4. **无任何匹配规则**：默认允许查看所有 PUBLIC 笔记（"默认开放"语义）

### 举例

| peer_id | mode | tags | 效果 |
|---|---|---|---|
| peerA | — | — | 无规则 → 可见我所有 PUBLIC 笔记 |
| peerB | allow | ["work"] | peerB 只能看带 `#work` tag 的 PUBLIC 笔记 |
| peerC | deny | ["private-diary"] | peerC 看不到带 `#private-diary` 的 PUBLIC 笔记 |
| peerD | allow ["team"] + deny ["draft"] | — | peerD 只看 `#team` 但排除 `#draft` |
| peerE | allow | ["__none__"] | 不可能匹配的 tag → 完全拒绝 |

### 实现

```rust
// lan/auth.rs
pub fn filter_memos_for_peer(memos: Vec<Memo>, peer_id: &str, rules: &[AclRule]) -> Vec<Memo> {
    let peer_rules: Vec<&AclRule> = rules.iter().filter(|r| r.peer_id == peer_id).collect();
    if peer_rules.is_empty() {
        return memos; // 默认开放
    }
    let allow_tags: HashSet<&str> = peer_rules.iter()
        .filter(|r| r.mode == "allow").flat_map(|r| r.tags.iter().map(String::as_str)).collect();
    let deny_tags: HashSet<&str> = peer_rules.iter()
        .filter(|r| r.mode == "deny").flat_map(|r| r.tags.iter().map(String::as_str)).collect();

    memos.into_iter().filter(|m| {
        let tags: HashSet<String> = markdown::extract_tags(&m.content).into_iter().collect();
        let allow_pass = allow_tags.is_empty() || tags.iter().any(|t| allow_tags.contains(t.as_str()));
        let deny_pass = !tags.iter().any(|t| deny_tags.contains(t.as_str()));
        allow_pass && deny_pass
    }).collect()
}
```

### 关键决策

- **"默认开放"**：无规则 = 全部 PUBLIC 可见
- **不引入白名单概念**：所有规则都是 tag 维度的过滤。"完全拒绝某 peer"通过 `allow: ["__none__"]` 兜底实现（不可能匹配的 tag）。UI 提供"完全拒绝"快捷选项，背后生成该规则
- **ACL 只在服务端生效**：客户端不做权限判断，收到什么就显示什么。复制操作同样受 ACL 约束（ACL 在 `GetMemo` 时也应用）
- **ACL 规则记住对端 display_name**：方便用户识别，但权限匹配基于 peer_id

## JSON-RPC 协议

### ALPN 与版本

- ALPN：`b"memos/lan-share/1"`
- 协议版本号嵌入 ALPN，未来升级用 `memos/lan-share/2`，旧版本可并行支持

### 帧编解码

每条请求/响应在 bi-stream 上传输，格式为 **长度前缀 + JSON**：

```
[4 字节大端 u32 长度][JSON 字节流]
```

- 请求方打开 `open_bi`，先写请求帧，再读响应帧
- 一个 bi-stream 一次请求-响应，不复用（QUIC stream 创建极廉价，无需多路复用）
- 单帧上限 16 MB（防止恶意大帧；笔记 + 元数据远小于此）

### 请求类型

```rust
// lan/protocol.rs
#[derive(Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    /// 列出对端公开的笔记（带分页 + tag 过滤）
    ListMemos {
        offset: u32,
        limit: u32,
        tag_filter: Option<Vec<String>>,
    },
    /// 获取单条笔记完整内容
    GetMemo {
        uid: String,
    },
    /// 获取附件字节（按 attachment uid）
    GetAttachment {
        uid: String,
    },
    /// 获取对端展示名 + 公开笔记统计
    GetProfile,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum Response {
    Ok { data: ResponseData },
    Err { code: u16, message: String },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ResponseData {
    MemoList { memos: Vec<RemoteMemoSummary>, total: u32 },
    Memo(RemoteMemo),
    Attachment { content: Vec<u8>, mime_type: String },
    Profile { display_name: String, public_memo_count: u32, tags: Vec<String> },
}

#[derive(Serialize, Deserialize)]
pub struct RemoteMemoSummary {
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub pinned: bool,
    pub snippet: String,
    pub tags: Vec<String>,
    pub has_attachments: bool,
}

#[derive(Serialize, Deserialize)]
pub struct RemoteMemo {
    pub uid: String,
    pub created_ts: i64,
    pub updated_ts: i64,
    pub pinned: bool,
    pub content: String,
    pub attachments: Vec<RemoteAttachmentSummary>,
}

#[derive(Serialize, Deserialize)]
pub struct RemoteAttachmentSummary {
    pub uid: String,
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
}
```

### 服务端处理流程

```rust
// lan/server.rs
async fn handle_conn(conn: Connection, state: Arc<LanState>) {
    loop {
        let (mut send, mut recv) = match conn.accept_bi().await {
            Ok(s) => s,
            Err(_) => break,
        };
        let state = state.clone();
        tokio::spawn(async move {
            let req: Request = read_frame(&mut recv).await?;
            let peer_id = conn.peer_id();
            let resp = match req {
                Request::ListMemos { offset, limit, tag_filter } =>
                    handle_list_memos(state, peer_id, offset, limit, tag_filter).await,
                Request::GetMemo { uid } =>
                    handle_get_memo(state, peer_id, uid).await,
                Request::GetAttachment { uid } =>
                    handle_get_attachment(state, peer_id, uid).await,
                Request::GetProfile =>
                    handle_get_profile(state, peer_id).await,
            };
            write_frame(&mut send, &resp).await?;
            Ok::<_, anyhow::Error>(())
        });
    }
}
```

### 权限应用点

| 方法 | 权限检查 |
|---|---|
| `ListMemos` | 应用 ACL filter |
| `GetMemo` | 先查 memo，再用 ACL 验证该 memo 对该 peer 可见 |
| `GetAttachment` | 查 attachment → memo_id → memo，再验证 memo 可见 |
| `GetProfile` | 无 ACL 限制（展示名 + 公开统计是公开信息）|

### 错误码

| code | 含义 |
|---|---|
| 400 | 请求格式错误 |
| 403 | 权限不足（ACL 拒绝）|
| 404 | 资源不存在 |
| 500 | 服务端内部错误 |

### 客户端调用

```rust
// lan/client.rs
pub async fn call_remote(
    endpoint: &Endpoint,
    peer: &EndpointAddr,
    req: Request,
) -> Result<ResponseData, LanError> {
    let conn = endpoint.connect(peer, ALPN).await?;
    let (mut send, mut recv) = conn.open_bi().await?;
    write_frame(&mut send, &req).await?;
    let resp: Response = read_frame(&mut recv).await?;
    match resp {
        Response::Ok { data } => Ok(data),
        Response::Err { code, message } => Err(LanError::Remote(code, message)),
    }
}
```

### 关键决策

- **不复用连接**：每个请求一个 bi-stream，但底层 QUIC 连接可复用（iroh 自动管理）
- **附件单独请求**：`GetMemo` 不带附件字节，只带附件元数据。图片懒加载由前端按需调 `GetAttachment`
- **不加密额外层**：QUIC 已端到端加密，协议层不再加密。ACL 在应用层，连接层信任 peer_id
- **GetProfile 不受 ACL**：只暴露展示名和公开笔记数量统计，不泄露具体内容

## 前端 UI 与交互

### 入口：工具栏"发现"按钮

在 `MemoEditor/Toolbar/` 新增按钮（与 InsertMenu、VisibilitySelector 同级）：

- **图标**：`Compass` 或 `Radar`（lucide-react）
- **位置**：工具栏右侧
- **点击行为**：打开 `LanDiscoveryPanel`（Drawer 形态）

### LanDiscoveryPanel 布局

左右两栏 Drawer（参考现有 `MemoExplorerDrawer` 模式）：

```
┌─────────────────────────────────────────────────┐
│ 发现局域网用户                          [×]      │
├──────────────┬──────────────────────────────────┤
│ 在线用户      │ 张三的 Mac (peerA3f...)          │
│              │                                  │
│ ● 张三的 Mac  │ 公开笔记 42 条  标签: work, life  │
│   peerA3f... │                                  │
│              │ [搜索框]  [标签筛选▼]             │
│ ● 李四的PC    │                                  │
│   peerB7c... │ 📌 项目周报                      │
│              │    #work · 2026-07-10            │
│ ● 王五的手机  │    本周完成了 iroh 集成...        │
│   peerC9d... │                                  │
│              │ 读书笔记                          │
│              │    #life · 2026-07-08            │
│              │    《人类简史》第三章...           │
│              │                                  │
│              │ [加载更多]                        │
├──────────────┴──────────────────────────────────┤
│              [预览笔记]  [复制到本地]            │
└─────────────────────────────────────────────────┘
```

### 组件树

```
LanDiscoveryPanel
├── PeerList (左栏)
│   ├── PeerItem × N
│   │   ├── 在线状态点（绿/灰）
│   │   ├── display_name
│   │   └── peer_id 缩略（前 8 位）
│   └── 空状态："未发现局域网用户"
│
├── RemoteMemoList (右栏)
│   ├── RemoteProfileHeader
│   │   ├── display_name
│   │   ├── 公开笔记数 + 标签列表
│   │   └── peer_id（可复制）
│   ├── FilterBar
│   │   ├── 搜索框（本地过滤已加载列表）
│   │   └── 标签筛选（从对端 GetProfile.tags 选）
│   ├── RemoteMemoCard × N
│   │   ├── pinned 标记
│   │   ├── snippet（纯文本摘要）
│   │   ├── tags + 创建时间
│   │   └── has_attachments 图标
│   └── LoadMoreButton（分页）
│
└── RemoteMemoPreview (点击卡片展开)
    ├── 完整 markdown 渲染（复用 MemoMarkdownRenderer）
    ├── 附件列表（懒加载缩略图）
    └── Footer: [复制到本地] 按钮
```

### 复制到本地流程

点击"复制到本地"按钮：

```ts
async function handleCopy(peerId: string, uid: string) {
  // 1. 确认弹窗："复制此笔记到本地？将创建为新笔记，不带原作者关联。"
  // 2. 调 lan_copy_memo_to_local(peerId, uid)
  //    → 后端拉 content + 所有附件 → 本地 create_memo + create_attachment
  // 3. 成功 toast："已复制到本地"
  // 4. 可选：跳转到新笔记详情页
}
```

**复制语义**：
- 创建为本地新笔记，uid 重新生成
- `visibility = Private`（复制来的内容默认私有，用户可改）
- `pinned = false`
- content 原样保留（包括 tag）
- 附件全部拉取并关联到新笔记
- **不保留**"来源 peer_id / 原作者"等元数据（无状态模型）

### 设置页：LanShareSection

在 `Setting.tsx` 的设置分组中新增"局域网分享"section：

```
局域网分享
├── 本机身份
│   ├── 展示名: [LocalFragNote____]  ← input
│   └── Peer ID: peerA3f7e2b1...     [复制]
├── 编组权限
│   ├── 已发现用户列表
│   │   ├── 张三的 Mac (peerA3f...)
│   │   │   └── [默认开放 ▼] → 选项: 默认开放 / 限制可见标签 / 完全拒绝
│   │   │       若限制: allow tags [work, team] / deny tags [draft]
│   │   └── 李四的PC (peerB7c...)
│   │       └── [完全拒绝 ▼]
│   └── 保存
└── 服务状态: ● 运行中 (mDNS 已启用)
```

### 复用现有组件

- **MemoMarkdownRenderer**：预览远端笔记时复用，通过 `MarkdownRenderContext` 传 `remote: true` flag，禁用 mention/tag 的本地跳转链接
- **AttachmentCard / AttachmentMediaGrid**：附件懒加载后用 `URL.createObjectURL(blob)` 生成临时 URL 渲染
- **toast**：复制成功/失败反馈
- **ConfirmDialog**：复制确认

### Tauri 命令清单（前端调用）

```ts
// 发现与查询
lan_discover_peers() → PeerInfo[]
lan_get_remote_profile(peer_id) → Profile
lan_list_remote_memos(peer_id, offset, limit, tag_filter?) → { memos, total }
lan_get_remote_memo(peer_id, uid) → RemoteMemo
lan_get_remote_attachment(peer_id, uid) → { content: Uint8Array, mime_type: string }
lan_copy_memo_to_local(peer_id, uid) → { new_memo_uid: string }

// 本机配置
lan_get_local_identity() → { peer_id, display_name }
lan_update_display_name(name) → void
lan_get_acl_rules() → AclRule[]
lan_save_acl_rules(rules) → void
```

### 事件

```ts
// 后端 → 前端
listen("lan:peers-changed", (e) => void)  // peer 上线/下线/更新
```

### i18n

新增 i18n key（en.json / zh-Hans.json）：
- `lan.discovery.title` / `lan.discovery.empty`
- `lan.peer.online` / `lan.peer.publicMemos` / `lan.peer.copyPeerId`
- `lan.memo.copyToLocal` / `lan.memo.copySuccess` / `lan.memo.copyConfirm`
- `lan.settings.displayName` / `lan.settings.aclRules` / `lan.settings.statusRunning`

## 错误处理与边界情况

### 网络层错误

| 场景 | 处理 |
|---|---|
| mDNS 初始化失败（端口被占） | 启动时 `tracing::warn`，Endpoint 仍启动（仅 relay fallback），前端显示"mDNS 不可用，局域网发现受限" |
| 连接对端超时（5 秒） | 返回 `LanError::ConnectTimeout`，前端 toast "无法连接到 {peer}" |
| bi-stream 读写超时（10 秒） | 返回 `LanError::RpcTimeout`，前端 toast "请求超时" |
| 对端突然下线（连接断开） | 返回 `LanError::ConnectionClosed`，前端从 peer 列表移除该 peer，正在预览则关闭预览 |
| 帧长度超过 16 MB | 返回 `LanError::FrameTooLarge`，拒绝处理，记录 warn 日志 |

### 协议层错误

| 场景 | 处理 |
|---|---|
| JSON 反序列化失败 | 返回 `Response::Err { code: 400, message }` |
| 请求未知 method | 返回 `Response::Err { code: 400, message: "unknown method" }` |
| 参数校验失败（如 limit > 200） | 返回 `Response::Err { code: 400, message }` |

### 权限错误

| 场景 | 处理 |
|---|---|
| `GetMemo` 时 ACL 拒绝 | 返回 `code: 403`，前端 toast "对方未向你公开此笔记" |
| `GetAttachment` 时 memo 对该 peer 不可见 | 返回 `code: 403`，同上 |
| `ListMemos` 返回空（ACL 过滤后无可见笔记） | 正常返回空列表，不是错误 |

### 数据层错误

| 场景 | 处理 |
|---|---|
| 对端请求的 memo uid 不存在 | `code: 404` |
| 对端请求的 attachment uid 不存在 | `code: 404` |
| 本地 SQLite 锁竞争（store mutex） | 重试 1 次（50ms 后），仍失败则 `code: 500` |
| 复制时本地 create_memo 失败（uid 冲突） | 重新生成 uid 重试 1 次，仍失败则前端 toast "复制失败" |

### 边界情况

**并发与状态**
- **多个 peer 同时请求**：iroh 的 `accept_bi` 循环每请求 spawn 一个 task，无共享状态。SQLite 通过现有 `Mutex<Store>` 串行化，复用 `spawn_blocking` 模式避免阻塞 async runtime
- **本机同时作为客户端和服务端**：Endpoint 全双工，不冲突
- **ACL 规则热更新**：每次请求实时读 `app_setting:lan_acl_rules`，不缓存。用户改 ACL 立即生效

**数据一致性**
- **预览期间对端笔记被删除**：`GetMemo` 返回 404，前端关闭预览并提示"该笔记已不存在"
- **预览期间对端笔记被改为非 PUBLIC**：`GetMemo` 返回 403，同上提示
- **复制中途网络断开**：已拉取的 content/附件丢弃，前端 toast "复制失败：连接中断"。不保留半成品数据
- **复制大附件（如 100MB 音频）**：单附件分块读取，但当前协议是单帧整体返回。本次不实现断点续传，超时设为 60 秒（附件专用超时）

**安全边界**
- **peer 伪造 display_name**：mDNS TXT 记录可被任意设置。仅作展示用，ACL 依赖 `peer_id`（公钥），不依赖 display_name。用户在设置页配置 ACL 时看到的是"首次发现的 display_name"，但实际匹配基于 peer_id
- **恶意 peer 频繁请求**：本次不实现限流。iroh 层可 `endpoint.reject_connection()` 拒绝特定 peer_id，留作未来增强
- **附件内容恶意**：附件按 `mime_type` 存储和展示，前端用现有 `AttachmentMediaGrid` 渲染（已沙箱化）。不执行任何附件内容

### 错误类型定义

```rust
// lan/mod.rs
#[derive(Debug, thiserror::Error)]
pub enum LanError {
    #[error("iroh endpoint error: {0}")]
    Endpoint(#[from] iroh::endpoint::ConnectionError),
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
}
```

### 前端错误展示策略

- **peer 列表为空**：空状态提示"未发现局域网用户，请确认在同一网络且对端已启动应用"
- **加载远端笔记失败**：右栏显示错误状态 + 重试按钮
- **预览失败**：预览区显示错误信息 + 返回列表按钮
- **复制失败**：toast 提示原因，不关闭预览（用户可重试）

## 测试策略

### Rust 单元测试

**`lan/protocol.rs` 测试**
- 帧编解码往返：序列化 Request → 写帧 → 读帧 → 反序列化，验证一致性
- 边界：空帧、最大帧（16MB-1）、超限帧（拒绝）
- JSON 反序列化：未知 method 字段 → 错误（而非静默忽略）

**`lan/auth.rs` 测试**（核心逻辑，重点覆盖）
- 无规则 → 全部 PUBLIC 可见
- `allow ["work"]` → 仅 `#work` 可见
- `deny ["draft"]` → 排除 `#draft`
- `allow ["team"] + deny ["draft"]` → 交集
- `allow ["__none__"]` → 完全拒绝（空结果）
- 笔记多 tag 混合（含 allow 和 deny tag）→ deny 优先
- peer_id 不匹配任何规则 → 默认开放
- 空内容笔记（无 tag）+ allow 规则 → 不可见（无 tag 匹配）

测试用 `Store::open_in_memory()`，复用现有 `make_memo` helper 模式。

**`lan/server.rs` 集成测试**
- 启动两个 in-process Endpoint（用 `iroh::test_utils` 或手动 bind 不同端口）
- client 调 `ListMemos` → 验证返回受 ACL 过滤的结果
- client 调 `GetMemo` 404/403 场景
- client 调 `GetAttachment` 权限通过/拒绝
- client 调 `GetProfile` → 返回正确统计

### Rust 不测试的部分

- mDNS 实际网络发现（依赖真实网络环境，CI 不可靠）——仅测试 `filter_memos_for_peer` 纯函数
- Endpoint 绑定/连接（iroh 自身已测试）
- 展示名广播（依赖 mDNS TXT 记录，集成测试环境不稳定）

### 前端测试

**`hooks.ts` 单元测试**（用 vitest + msw 模拟 Tauri invoke）
- `useLanDiscovery`：监听 `lan:peers-changed` 事件后正确更新状态
- `useRemoteMemos`：分页加载、tag 过滤切换、error 状态
- `useRemoteMemoPreview`：加载/成功/失败三态

**组件交互测试**（React Testing Library）
- `PeerList` 空状态渲染
- `RemoteMemoCard` 渲染 snippet/tags/附件图标
- `LanShareSection` ACL 配置：默认开放 → 切换到"限制可见标签" → 保存

### 手动验收测试清单

实现完成后需手动验证：

1. 两台同网段机器启动应用，发现面板能互相看到对方
2. 点击对端 → 看到其 PUBLIC 笔记列表，PRIVATE 笔记不出现
3. 设置 ACL 限制某 peer 只看 `#work` → 对端列表只剩 `#work` 笔记
4. 设置完全拒绝 → 对端看不到任何笔记
5. 预览远端笔记 → markdown 正确渲染，图片附件懒加载显示
6. 复制笔记到本地 → 本地新笔记内容/附件完整，visibility=Private
7. 对端修改 ACL 后立即生效（无需重启）
8. 对端下线 → peer 列表移除，正在预览的关闭
9. 跨网段（mDNS 不可达）→ relay fallback 仍可连接（验证一次即可）
10. 重启应用 → peer_id 保持不变（SecretKey 持久化生效）

### 测试边界

- **不测试**：真实 mDNS 多播网络行为、relay 服务器可用性、并发 100+ peer 压测
- **不实现**：自动化 E2E 测试（Tauri WebDriver 集成成本高，首版用手动验收清单替代）

## 实现顺序与里程碑

### 依赖与版本约束

新增到 `src-tauri/Cargo.toml`：
```toml
iroh = "1"
iroh-mdns-address-lookup = "0.4"
anyhow = "1"  # lan 模块内部用，对外转 IpcError
```

**潜在风险**：iroh 1.x 依赖 tokio 1.x + quinn，需验证与现有 `tauri::async_runtime`（基于 tokio）兼容。预计无冲突，但标记为实现时第一步验证。

### 实现里程碑（8 个阶段）

**阶段 1：基础架构与 Endpoint 生命周期**
- 添加 Cargo 依赖，验证编译
- `lan/mod.rs` + `lan/endpoint.rs`：SecretKey 持久化、Endpoint 初始化、mDNS 启用
- 扩展 `AppState` 持有 `Option<Arc<LanState>>`
- `main.rs` setup 阶段启动 Endpoint
- 验证：应用启动无报错，日志显示 EndpointId

**阶段 2：mDNS 发现与 peer 列表**
- `lan_discover_peers` 命令
- 后台 task 监听 mDNS 事件 + `emit("lan:peers-changed")`
- 展示名广播（TXT 记录）+ `lan_get_local_identity` / `lan_update_display_name`
- 验证：两台机器互相发现

**阶段 3：JSON-RPC 协议层**
- `lan/protocol.rs`：帧编解码 + 类型定义
- `lan/client.rs`：`call_remote` 函数
- `lan/server.rs`：accept 循环 + 路由分发（先空实现，返回 400）
- 单元测试：帧编解码往返
- 验证：编译通过，单元测试绿

**阶段 4：服务端业务实现**
- `lan/auth.rs`：`filter_memos_for_peer` + 单元测试（重点）
- `handle_list_memos` / `handle_get_memo` / `handle_get_attachment` / `handle_get_profile`
- ACL 规则读写：`lan_get_acl_rules` / `lan_save_acl_rules`
- 集成测试：双 Endpoint 调用验证
- 验证：集成测试绿

**阶段 5：复制到本地**
- `lan_copy_memo_to_local` 命令：拉 content + 附件 → 本地 `create_memo` + `create_attachment`
- uid 重新生成、visibility=Private
- 错误处理：连接中断清理
- 验证：集成测试覆盖复制流程

**阶段 6：前端基础 UI**
- `LanDiscovery/` 组件骨架
- 工具栏"发现"按钮
- `useLanDiscovery` hook + peer 列表渲染
- `useRemoteMemos` hook + 笔记列表渲染
- i18n key 补充
- 验证：能发现 peer、加载远端笔记列表

**阶段 7：前端预览与复制交互**
- `RemoteMemoPreview`（复用 MemoMarkdownRenderer，加 `remote` flag）
- 附件懒加载渲染
- 复制确认弹窗 + toast
- 错误状态展示
- 验证：完整查看-复制流程跑通

**阶段 8：设置页与 ACL 配置**
- `LanShareSection` 组件
- ACL 三档配置 UI（默认开放/限制标签/完全拒绝）
- 服务状态显示
- 验证：ACL 变更立即生效

### 阶段间依赖

```
1 (Endpoint) → 2 (mDNS)
1 → 3 (协议层) → 4 (服务端业务) → 5 (复制)
2 + 4 → 6 (前端基础) → 7 (前端预览/复制)
4 → 8 (设置页 ACL)
```

阶段 2 与 3 可并行（都依赖阶段 1），阶段 8 可与 6/7 并行。

### 范围排除（YAGNI）

明确**不做**：
- 批量复制
- 断点续传
- 实时同步/订阅（被动模型，无主动推送）
- 历史发现记录持久化
- 限流/反滥用
- 跨版本协议兼容（首版只有 ALPN v1）
- E2E 自动化测试
- 附件传输进度条（单帧整体返回）

### 验收标准

对应手动验收清单 10 条，全部通过即视为完成。
