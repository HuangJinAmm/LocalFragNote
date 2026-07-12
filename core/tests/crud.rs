use memos_core::*;
use memos_core::tag;
use memos_core::attachment::{CreateAttachment, FindAttachment};
use memos_core::memo::{CreateMemo, FindMemo, UpdateMemo};
use memos_core::memo_relation::{FindMemoRelation, UpsertMemoRelation};
use memos_core::reaction::{FindReaction, UpsertReaction};
use memos_core::types::{MemoRelationType, RowStatus, Visibility};
use serde_json::json;

fn open_test_store() -> Store {
    Store::open_in_memory().expect("打开内存数据库失败")
}

#[test]
fn store_open_runs_migrations() {
    let store = open_test_store();
    let conn = store.lock_conn();
    for table in ["memo", "attachment", "memo_relation", "reaction", "app_setting", "instance_setting"] {
        let count: i32 = conn
            .query_row(
                &format!("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='{table}'"),
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "表 {table} 应存在");
    }
}

#[test]
fn memo_crud_full_cycle() {
    let store = open_test_store();
    let conn = store.lock_conn();

    // 创建
    let created = memo::create(&conn, &CreateMemo {
        uid: "test_memo_1".into(),
        content: "Hello world".into(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({"tags": ["#test"]}),
        location: None,
    })
    .expect("创建 memo 失败");
    assert_eq!(created.uid, "test_memo_1");
    assert_eq!(created.content, "Hello world");
    assert_eq!(created.visibility, Visibility::Private);
    assert!(!created.pinned);
    assert_eq!(created.payload["tags"][0], "#test");

    // 查询
    let got = memo::get(&conn, &FindMemo { uid: Some("test_memo_1".into()), ..Default::default() })
        .expect("查询失败")
        .expect("memo 不存在");
    assert_eq!(got.id, created.id);

    // 更新
    let updated = memo::update(&conn, &UpdateMemo {
        id: created.id,
        content: Some("Updated".into()),
        pinned: Some(true),
        visibility: Some(Visibility::Public),
        ..Default::default()
    })
    .expect("更新失败");
    assert_eq!(updated.content, "Updated");
    assert!(updated.pinned);
    assert_eq!(updated.visibility, Visibility::Public);

    // 列表过滤
    let list = memo::list(&conn, &FindMemo {
        visibility_list: vec![Visibility::Public],
        ..Default::default()
    })
    .expect("列表查询失败");
    assert_eq!(list.len(), 1);

    // 删除
    drop(conn);
    store.with_conn_mut(|c| memo::delete(c, created.id)).expect("删除失败");
    let after = store.with_conn(|c| memo::get(c, &FindMemo { id: Some(created.id), ..Default::default() }))
        .expect("查询失败");
    assert!(after.is_none());
}

#[test]
fn memo_uid_conflict() {
    let store = open_test_store();
    let conn = store.lock_conn();
    memo::create(&conn, &CreateMemo {
        uid: "dup".into(), content: "".into(), visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    let err = memo::create(&conn, &CreateMemo {
        uid: "dup".into(), content: "".into(), visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    })
    .expect_err("应拒绝重复 UID");
    assert!(matches!(err, CoreError::UidConflict(_)));
}

#[test]
fn memo_invalid_uid() {
    let store = open_test_store();
    let conn = store.lock_conn();
    let err = memo::create(&conn, &CreateMemo {
        uid: "invalid uid!".into(), content: "".into(), visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    })
    .expect_err("应拒绝非法 UID");
    assert!(matches!(err, CoreError::InvalidUid));
}

#[test]
fn memo_archive_and_filter() {
    let store = open_test_store();
    let conn = store.lock_conn();
    let m1 = memo::create(&conn, &CreateMemo {
        uid: "m1".into(), content: "a".into(), visibility: Visibility::Public, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    let _m2 = memo::create(&conn, &CreateMemo {
        uid: "m2".into(), content: "b".into(), visibility: Visibility::Public, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    memo::update(&conn, &UpdateMemo { id: m1.id, row_status: Some(RowStatus::Archived), ..Default::default() }).unwrap();

    let normal = memo::list(&conn, &FindMemo { row_status: Some(RowStatus::Normal), ..Default::default() }).unwrap();
    assert_eq!(normal.len(), 1);

    let archived = memo::list(&conn, &FindMemo { row_status: Some(RowStatus::Archived), ..Default::default() }).unwrap();
    assert_eq!(archived.len(), 1);
    assert_eq!(archived[0].id, m1.id);
}

#[test]
fn attachment_crud() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let att = attachment::create(&conn, &CreateAttachment {
        uid: "att1".into(),
        filename: "test.png".into(),
        blob: vec![0x89, 0x50, 0x4E, 0x47],
        r#type: "image/png".into(),
        memo_id: None,
        storage_type: attachment::STORAGE_TYPE_DATABASE.into(),
        reference: String::new(),
        size: None,
    })
    .expect("创建附件失败");
    assert_eq!(att.filename, "test.png");
    assert_eq!(att.size, 4);
    assert_eq!(att.r#type, "image/png");
    assert_eq!(att.storage_type, "DATABASE");

    // 带 blob 查询
    let with_blob = attachment::get(&conn, &FindAttachment {
        uid: Some("att1".into()),
        get_blob: true,
        ..Default::default()
    })
    .unwrap()
    .unwrap();
    assert_eq!(with_blob.blob, Some(vec![0x89, 0x50, 0x4E, 0x47]));

    // 不带 blob 查询
    let without_blob = attachment::get(&conn, &FindAttachment {
        id: Some(att.id),
        get_blob: false,
        ..Default::default()
    })
    .unwrap()
    .unwrap();
    assert_eq!(without_blob.blob, None);

    // 删除并验证返回 storage_type/reference
    let deleted_meta = attachment::delete(&conn, att.id).expect("删除失败");
    assert!(deleted_meta.is_some());
    let (st, _ref) = deleted_meta.unwrap();
    assert_eq!(st, "DATABASE");
}

#[test]
fn attachment_local_storage_mode() {
    let store = open_test_store();
    let conn = store.lock_conn();

    // LOCAL 模式：blob 留空，size 显式传入
    let att = attachment::create(&conn, &CreateAttachment {
        uid: "att_local1".into(),
        filename: "big.pdf".into(),
        blob: Vec::new(),
        r#type: "application/pdf".into(),
        memo_id: None,
        storage_type: attachment::STORAGE_TYPE_LOCAL.into(),
        reference: "attachments/att_local1_big.pdf".into(),
        size: Some(1024 * 1024),
    })
    .expect("创建 LOCAL 附件失败");
    assert_eq!(att.storage_type, "LOCAL");
    assert_eq!(att.size, 1024 * 1024);
    assert_eq!(att.reference, "attachments/att_local1_big.pdf");

    // 查询时 blob 应为 None
    let got = attachment::get(&conn, &FindAttachment {
        id: Some(att.id),
        get_blob: true,
        ..Default::default()
    })
    .unwrap()
    .unwrap();
    assert!(got.blob.is_none(), "LOCAL 模式查询不应返回 blob");

    attachment::delete(&conn, att.id).unwrap();
}

#[test]
fn reaction_upsert_and_list() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let r1 = reaction::upsert(&conn, &UpsertReaction {
        content_id: "memo_uid_1".into(),
        reaction_type: "👍".into(),
    })
    .unwrap();
    assert_eq!(r1.reaction_type, "👍");

    // 幂等 upsert
    let r2 = reaction::upsert(&conn, &UpsertReaction {
        content_id: "memo_uid_1".into(),
        reaction_type: "👍".into(),
    })
    .unwrap();
    assert_eq!(r1.id, r2.id);

    // 不同 reaction
    reaction::upsert(&conn, &UpsertReaction {
        content_id: "memo_uid_1".into(),
        reaction_type: "❤️".into(),
    })
    .unwrap();

    let list = reaction::list(&conn, &FindReaction {
        content_id: Some("memo_uid_1".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(list.len(), 2);

    reaction::delete(&conn, r1.id).unwrap();
    let after = reaction::list(&conn, &FindReaction {
        content_id: Some("memo_uid_1".into()),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(after.len(), 1);
}

#[test]
fn memo_relation_crud() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let m1 = memo::create(&conn, &CreateMemo {
        uid: "rm1".into(), content: "".into(), visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    let m2 = memo::create(&conn, &CreateMemo {
        uid: "rm2".into(), content: "".into(), visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();

    let rel = memo_relation::upsert(&conn, &UpsertMemoRelation {
        memo_id: m1.id,
        related_memo_id: m2.id,
        r#type: MemoRelationType::Reference,
    })
    .unwrap();
    assert_eq!(rel.memo_id, m1.id);

    let list = memo_relation::list(&conn, &FindMemoRelation {
        memo_id: Some(m1.id),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(list.len(), 1);

    memo_relation::delete(&conn, m1.id, m2.id, MemoRelationType::Reference).unwrap();
    let after = memo_relation::list(&conn, &FindMemoRelation {
        memo_id: Some(m1.id),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(after.len(), 0);
}

#[test]
fn memo_delete_cascades_relations() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let m1 = memo::create(&conn, &CreateMemo {
        uid: "cm1".into(), content: "".into(), visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    let m2 = memo::create(&conn, &CreateMemo {
        uid: "cm2".into(), content: "".into(), visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();

    memo_relation::upsert(&conn, &UpsertMemoRelation {
        memo_id: m1.id, related_memo_id: m2.id, r#type: MemoRelationType::Reference,
    }).unwrap();

    // 删除 m1 应级联清理关系
    drop(conn);
    store.with_conn_mut(|c| memo::delete(c, m1.id)).unwrap();

    let rels = store.with_conn(|c| memo_relation::list(c, &FindMemoRelation {
        memo_id_list: vec![m1.id, m2.id],
        ..Default::default()
    }))
    .unwrap();
    assert_eq!(rels.len(), 0);
}

#[test]
fn app_setting_crud_with_cache() {
    let store = open_test_store();
    store.with_conn(|c| store.setting.app.upsert(c, "locale", "zh-CN")).unwrap();

    // 读取（应命中缓存）
    let v = store.with_conn(|c| store.setting.app.get(c, "locale")).unwrap();
    assert_eq!(v.as_deref(), Some("zh-CN"));

    // 更新
    store.with_conn(|c| store.setting.app.upsert(c, "locale", "en")).unwrap();
    let v = store.with_conn(|c| store.setting.app.get(c, "locale")).unwrap();
    assert_eq!(v.as_deref(), Some("en"));

    // 删除
    store.with_conn(|c| store.setting.app.delete(c, "locale")).unwrap();
    let v = store.with_conn(|c| store.setting.app.get(c, "locale")).unwrap();
    assert!(v.is_none());
}

#[test]
fn instance_setting_crud() {
    let store = open_test_store();
    store.with_conn(|c| store.setting.instance.upsert(c, "BASIC", "{\"title\":\"Memos\"}", "")).unwrap();
    let v = store.with_conn(|c| store.setting.instance.get(c, "BASIC")).unwrap();
    assert_eq!(v.as_deref(), Some("{\"title\":\"Memos\"}"));

    store.with_conn(|c| store.setting.instance.delete(c, "BASIC")).unwrap();
    let v = store.with_conn(|c| store.setting.instance.get(c, "BASIC")).unwrap();
    assert!(v.is_none());
}

#[test]
fn memo_filter_by_content() {
    let store = open_test_store();
    let conn = store.lock_conn();
    memo::create(&conn, &CreateMemo {
        uid: "fc1".into(), content: "Rust 学习笔记".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    memo::create(&conn, &CreateMemo {
        uid: "fc2".into(), content: "Go 与 Rust 对比".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();

    let list = memo::list(&conn, &FindMemo {
        content_contains: Some("Rust".into()),
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 2);

    let list = memo::list(&conn, &FindMemo {
        content_contains: Some("学习".into()),
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].uid, "fc1");
}

#[test]
fn memo_filter_by_tag() {
    let store = open_test_store();
    let conn = store.lock_conn();
    memo::create(&conn, &CreateMemo {
        uid: "ft1".into(), content: "Hello #rust world".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    memo::create(&conn, &CreateMemo {
        uid: "ft2".into(), content: "More #rust and #cli".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    memo::create(&conn, &CreateMemo {
        uid: "ft3".into(), content: "Unrelated #rusty text".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();

    // 单 tag
    let list = memo::list(&conn, &FindMemo {
        tag_search: vec!["rust".into()],
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 2, "应只匹配 #rust，不匹配 #rusty");

    // 多 tag
    let list = memo::list(&conn, &FindMemo {
        tag_search: vec!["rust".into(), "cli".into()],
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].uid, "ft2");
}

#[test]
fn memo_filter_by_tag_with_exclude_content() {
    let store = open_test_store();
    let conn = store.lock_conn();
    memo::create(&conn, &CreateMemo {
        uid: "fe1".into(), content: "Secret #note here".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();

    let list = memo::list(&conn, &FindMemo {
        tag_search: vec!["note".into()],
        exclude_content: true,
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].content.is_empty(), "exclude_content 应清空 content 字段");
}

#[test]
fn memo_filter_by_time_range() {
    let store = open_test_store();
    let conn = store.lock_conn();
    let m1 = memo::create(&conn, &CreateMemo {
        uid: "tm1".into(), content: "old".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();
    let _m2 = memo::create(&conn, &CreateMemo {
        uid: "tm2".into(), content: "new".into(),
        visibility: Visibility::Private, pinned: false, payload: json!({}),
        location: None,
    }).unwrap();

    // 显式把 m1 的 created_ts 设为很久以前
    conn.execute("UPDATE memo SET created_ts = ? WHERE id = ?", rusqlite::params![1000_i64, m1.id])
        .unwrap();

    // after 边界：只匹配 created_ts >= 1001（即只 m2）
    let list = memo::list(&conn, &FindMemo {
        created_ts_after: Some(1001),
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].uid, "tm2");

    // before 边界：只匹配 created_ts < 1001（即只 m1）
    let list = memo::list(&conn, &FindMemo {
        created_ts_before: Some(1001),
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].uid, "tm1");
}

#[test]
fn memo_filter_with_pagination() {
    let store = open_test_store();
    let conn = store.lock_conn();
    for i in 0..5 {
        memo::create(&conn, &CreateMemo {
            uid: format!("pg{i}"), content: format!("item {i}").into(),
            visibility: Visibility::Private, pinned: false, payload: json!({}),
            location: None,
        }).unwrap();
    }

    let page1 = memo::list(&conn, &FindMemo {
        limit: Some(2), offset: Some(0),
        ..Default::default()
    }).unwrap();
    assert_eq!(page1.len(), 2);

    let page2 = memo::list(&conn, &FindMemo {
        limit: Some(2), offset: Some(2),
        ..Default::default()
    }).unwrap();
    assert_eq!(page2.len(), 2);
    assert_ne!(page1[0].id, page2[0].id);

    // tag 过滤下分页
    let list = memo::list(&conn, &FindMemo {
        tag_search: vec![],
        limit: Some(2), offset: Some(0),
        ..Default::default()
    }).unwrap();
    assert_eq!(list.len(), 2);
}

#[test]
fn tag_table_syncs_on_create() {
    let store = open_test_store();
    let conn = store.lock_conn();

    memo::create(&conn, &CreateMemo {
        uid: "test_tag_create".to_string(),
        content: "hello #rust #ai".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let names: Vec<&str> = tags.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"rust"), "tag 表应包含 rust");
    assert!(names.contains(&"ai"), "tag 表应包含 ai");

    let rust_count = tags.iter().find(|(n, _)| n == "rust").map(|(_, c)| *c).unwrap();
    assert_eq!(rust_count, 1, "rust count 应为 1");
}

#[test]
fn tag_table_empty_when_no_tags() {
    let store = open_test_store();
    let conn = store.lock_conn();

    memo::create(&conn, &CreateMemo {
        uid: "test_no_tags".to_string(),
        content: "just plain text no tags".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    assert!(tags.is_empty(), "无 tag 的 memo 不应产生 tag 表记录");
}

#[test]
fn tag_table_syncs_on_update_content() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test_tag_update".to_string(),
        content: "hello #rust".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: None,
        content: Some("hello #rust #go".to_string()),
        visibility: None,
        pinned: None,
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let rust_count = tags.iter().find(|(n, _)| n == "rust").map(|(_, c)| *c).unwrap_or(0);
    let go_count = tags.iter().find(|(n, _)| n == "go").map(|(_, c)| *c).unwrap_or(0);
    assert_eq!(rust_count, 1, "rust count 应保持 1");
    assert_eq!(go_count, 1, "go count 应为 1");
}

#[test]
fn tag_table_removes_tag_when_removed_from_content() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test_tag_remove".to_string(),
        content: "hello #rust #ai".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: None,
        content: Some("hello #rust".to_string()),
        visibility: None,
        pinned: None,
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let names: Vec<&str> = tags.iter().map(|(n, _)| n.as_str()).collect();
    assert!(names.contains(&"rust"), "rust 应仍在 tag 表");
    assert!(!names.contains(&"ai"), "ai count 归 0 应被删除");
}

#[test]
fn tag_table_no_sync_when_content_unchanged() {
    let store = open_test_store();
    let conn = store.lock_conn();

    let created = memo::create(&conn, &CreateMemo {
        uid: "test_tag_nosync".to_string(),
        content: "hello #rust".to_string(),
        visibility: Visibility::Private,
        pinned: false,
        payload: json!({}),
        location: None,
    }).unwrap();

    memo::update(&conn, &UpdateMemo {
        id: created.id,
        uid: None,
        row_status: None,
        content: None,
        visibility: None,
        pinned: Some(true),
        payload: None,
        location: None,
    }).unwrap();

    let tags = tag::list_tags(&conn).unwrap();
    let rust_count = tags.iter().find(|(n, _)| n == "rust").map(|(_, c)| *c).unwrap_or(0);
    assert_eq!(rust_count, 1, "content 未变时 tag count 不应改变");
}

#[test]
fn tag_table_decrements_on_delete() {
    let store = open_test_store();
    {
        let conn = store.lock_conn();
        let created = memo::create(&conn, &CreateMemo {
            uid: "test_tag_delete".to_string(),
            content: "hello #rust #ai".to_string(),
            visibility: Visibility::Private,
            pinned: false,
            payload: json!({}),
            location: None,
        }).unwrap();
        drop(conn);
        store.with_conn_mut(|c| memo::delete(c, created.id)).unwrap();
    }
    let tags = store.with_conn(|c| tag::list_tags(c)).unwrap();
    assert!(tags.is_empty(), "删除 memo 后 tag 表应被清空");
}

