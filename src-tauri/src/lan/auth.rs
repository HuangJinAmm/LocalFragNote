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

impl std::str::FromStr for AclMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "allow" => Ok(AclMode::Allow),
            "deny" => Ok(AclMode::Deny),
            _ => Err(format!("invalid acl mode: {s}")),
        }
    }
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
    let single = vec![memo.clone()];
    !filter_memos_for_peer(single, peer_id, rules).is_empty()
}
