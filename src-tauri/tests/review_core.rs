//! Review module core layer tests
//!
//! 测试 FSRS 调度、到期查询、统计、Deck/Card CRUD

use memos_core::review::*;
use memos_core::Store;

fn setup_store() -> Store {
    Store::open_in_memory().expect("无法打开内存数据库")
}

#[test]
fn test_create_deck_stores_tags_json() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Rust 基础", &["rust".into(), "ai".into()], 3)?;
        assert_eq!(deck.name, "Rust 基础");
        assert_eq!(deck.tags, vec!["rust", "ai"]);
        assert_eq!(deck.cards_per_memo, 3);
        assert_eq!(deck.memo_count, 0);
        Ok(())
    }).unwrap();
}

#[test]
fn test_list_decks_returns_all() {
    let store = setup_store();
    store.with_conn(|c| {
        create_deck(c, "A", &["t1".into()], 1)?;
        create_deck(c, "B", &["t2".into()], 2)?;
        let decks = list_decks(c)?;
        assert_eq!(decks.len(), 2);
        Ok(())
    }).unwrap();
}

#[test]
fn test_update_deck() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Old", &["t1".into()], 1)?;
        let updated = update_deck(c, deck.id, "New", &["t2".into(), "t3".into()], 5)?;
        assert_eq!(updated.name, "New");
        assert_eq!(updated.tags, vec!["t2", "t3"]);
        assert_eq!(updated.cards_per_memo, 5);
        Ok(())
    }).unwrap();
}

#[test]
fn test_delete_deck_cascades_cards_and_records() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let card = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "test-uid".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "定义".into(),
            stability: 0.0, difficulty: 0.0,
            due: chrono::Utc::now().timestamp(), last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: chrono::Utc::now().timestamp(),
            memo_deleted: false,
        };
        let card = create_card(c, &card)?;
        // 评分一次产生 record
        score_card(c, card.id, 3, &[])?;
        // 删除 deck
        delete_deck(c, deck.id)?;
        // 验证 card 和 record 都被删除
        assert!(get_card(c, card.id)?.is_none());
        assert!(get_deck(c, deck.id)?.is_none());
        Ok(())
    }).unwrap();
}

#[test]
fn test_list_due_cards_excludes_future() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        // 创建一张到期卡（due=now-100）
        let card_due = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q1".into(), back: "A1".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now - 100, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        };
        create_card(c, &card_due)?;
        // 创建一张未到期卡（due=now+100000）
        let card_future = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u2".into(),
            card_type: "basic".into(), front: "Q2".into(), back: "A2".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now + 100000, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        };
        create_card(c, &card_future)?;

        let due = list_due_cards(c, deck.id, 100)?;
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].front, "Q1");
        Ok(())
    }).unwrap();
}

#[test]
fn test_list_due_cards_excludes_deleted_memo() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        let card = ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now - 100, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now,
            memo_deleted: true, // 已删除
        };
        create_card(c, &card)?;
        let due = list_due_cards(c, deck.id, 100)?;
        assert_eq!(due.len(), 0);
        Ok(())
    }).unwrap();
}

#[test]
fn test_score_card_new_good_enters_review() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        let card = create_card(c, &ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        })?;

        let (updated, record) = score_card(c, card.id, 3, &[])?; // Good
        assert!(updated.reps >= 1, "reps should increment");
        assert!(updated.due > now, "due should be in the future after Good");
        assert_eq!(record.rating, 3);
        assert_eq!(record.card_id, card.id);
        Ok(())
    }).unwrap();
}

#[test]
fn test_score_card_review_again_lapses() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        // 先创建一张已学过的卡片（state=2 Review, reps=5, lapses=1）
        let card = create_card(c, &ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "u1".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 5.0, difficulty: 0.5,
            due: now, last_review: Some(now - 86400),
            reps: 5, lapses: 1, state: 2, created_ts: now, memo_deleted: false,
        })?;

        let (updated, _record) = score_card(c, card.id, 1, &[])?; // Again
        assert!(updated.lapses >= 2, "lapses should increment on Again");
        Ok(())
    }).unwrap();
}

#[test]
fn test_deck_stats() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        // 创建 3 张卡：1 张到期，1 张未到期，1 张已学
        for i in 0..3 {
            let card = ReviewCard {
                id: 0, deck_id: deck.id, memo_uid: format!("u{i}"),
                card_type: "basic".into(), front: format!("Q{i}"), back: "A".into(),
                cloze_answer: None, angle: "".into(),
                stability: 0.0, difficulty: 0.0,
                due: if i < 2 { now - 100 } else { now + 100000 },
                last_review: None,
                reps: if i == 0 { 5 } else { 0 },
                lapses: 0, state: if i == 0 { 2 } else { 0 },
                created_ts: now, memo_deleted: false,
            };
            create_card(c, &card)?;
        }
        let stats = deck_stats(c, deck.id)?;
        assert_eq!(stats.total, 3);
        assert_eq!(stats.due_count, 2);
        assert_eq!(stats.new_count, 2);
        assert_eq!(stats.learned, 1);
        Ok(())
    }).unwrap();
}

#[test]
fn test_mark_cards_memo_deleted() {
    let store = setup_store();
    store.with_conn(|c| {
        let deck = create_deck(c, "Test", &["t1".into()], 1)?;
        let now = chrono::Utc::now().timestamp();
        let card = create_card(c, &ReviewCard {
            id: 0, deck_id: deck.id, memo_uid: "test-uid".into(),
            card_type: "basic".into(), front: "Q".into(), back: "A".into(),
            cloze_answer: None, angle: "".into(),
            stability: 0.0, difficulty: 0.0,
            due: now, last_review: None,
            reps: 0, lapses: 0, state: 0, created_ts: now, memo_deleted: false,
        })?;
        mark_cards_memo_deleted(c, "test-uid")?;
        let updated = get_card(c, card.id)?.unwrap();
        assert!(updated.memo_deleted, "memo_deleted should be true");
        Ok(())
    }).unwrap();
}
