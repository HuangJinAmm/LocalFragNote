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
