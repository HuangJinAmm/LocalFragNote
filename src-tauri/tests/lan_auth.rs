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
    let memos = vec![make_memo("m1", "#work hello")];
    let rules = vec![rule("peerB", "allow", &["work"])];
    let filtered = filter_memos_for_peer(memos, "peerX", &rules);
    assert_eq!(filtered.len(), 1, "peer_id 不匹配应默认开放");
}

#[test]
fn test_empty_tag_memo_with_allow_rule() {
    let memos = vec![make_memo("m1", "just plain text no tags")];
    let rules = vec![rule("peerB", "allow", &["work"])];
    let filtered = filter_memos_for_peer(memos, "peerB", &rules);
    assert_eq!(filtered.len(), 0, "无 tag 笔记在 allow 规则下应不可见");
}

#[test]
fn test_empty_tag_memo_with_deny_rule() {
    let memos = vec![make_memo("m1", "plain text")];
    let rules = vec![rule("peerC", "deny", &["draft"])];
    let filtered = filter_memos_for_peer(memos, "peerC", &rules);
    assert_eq!(filtered.len(), 1, "无 tag 笔记在 deny 规则下应可见");
}

#[test]
fn test_multiple_allow_tags_union() {
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
    let memos = vec![make_memo("m1", "#shared content")];
    let rules = vec![
        rule("peerG", "allow", &["shared"]),
        rule("peerG", "deny", &["shared"]),
    ];
    let filtered = filter_memos_for_peer(memos, "peerG", &rules);
    assert_eq!(filtered.len(), 0, "deny 应优先于 allow");
}
