//! Tests of the question log rules (`ask-log`): the CSV format, ids, repeats, expiry, the
//! summary and outcome changes.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "integration tests assert on parsed fixture rows"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::asked::{
    self, AskedId, AskedRow, AskedSummary, HEADER, Outcome, RowState, Via,
};
use fieldbook_core::audit::{LabelId, LabelSet, Now};
use fieldbook_core::labels::{ItemRef, StoreId};
use rng::Rng;

const FIXTURE: &[u8] = include_bytes!("../../../tests/fixtures/state-asked/asked/asked.csv");

fn fixture_rows() -> Vec<AskedRow> {
    asked::parse(FIXTURE).expect("the fixture parses")
}

fn now(text: &str) -> Now {
    Now::parse(text).expect("valid timestamp")
}

fn id(n: u16) -> AskedId {
    AskedId::new(n).expect("valid id")
}

fn row(n: u16) -> AskedRow {
    let mut row = fixture_rows().swap_remove(1);
    row.id = id(n);
    row
}

#[test]
fn csv_round_trips() {
    let rows = fixture_rows();
    assert_eq!(rows.len(), 6);
    assert_eq!(asked::render(&rows).as_bytes(), FIXTURE);
    assert_eq!(
        asked::parse(asked::render(&rows).as_bytes()),
        Ok(rows.clone())
    );

    // A question with a comma, quotes, a tab and non-ASCII text survives.
    let mut odd = rows[1].clone();
    odd.question = "a, \"b\"\tc|d Cafe\u{301}?".to_owned();
    let text = asked::render(std::slice::from_ref(&odd));
    assert_eq!(asked::parse(text.as_bytes()), Ok(vec![odd]));

    // An empty log is the header alone.
    assert_eq!(asked::render(&[]), format!("{}\n", HEADER.join(",")));
    assert_eq!(asked::parse(asked::render(&[]).as_bytes()), Ok(Vec::new()));
}

#[test]
fn unknown_column_is_rejected() {
    let text = String::from_utf8(FIXTURE.to_vec()).expect("utf-8");
    let extra = text
        .replace(",state\n", ",state,note\n")
        .replace(",open\n", ",open,x\n")
        .replace(",promoted\n", ",promoted,x\n")
        .replace(",dismissed\n", ",dismissed,x\n");
    let err = asked::parse(extra.as_bytes()).expect_err("an unknown column");
    assert!(err.message.contains("note"), "{err}");

    let missing = text.replacen("id,at,", "at,", 1);
    assert!(asked::parse(missing.as_bytes()).is_err());

    let moved = text.replacen("id,at,via", "at,id,via", 1);
    assert!(asked::parse(moved.as_bytes()).is_err());
}

#[test]
fn bare_id_is_detected() {
    for bare in [
        "S-0001",
        "S-0001-v1",
        "CL-CMP-0001",
        "Q-027",
        "XC-14",
        "cmp-system-review",
        "FB0412",
        "A-0001",
        "P-0001",
        "  S-0001  ",
    ] {
        assert!(asked::is_bare_id(bare), "{bare}");
    }
    for question in [
        "synthetic",
        "material_weakness",
        "cmp",
        "S-0001 rate",
        "what is S-0001?",
        "",
        "A-0000",
        "A-10000",
    ] {
        assert!(!asked::is_bare_id(question), "{question}");
    }
}

#[test]
fn repeat_within_a_minute() {
    let mut last = row(1);
    last.at = now("2026-03-01T12:00:00Z");
    last.question = "synthetic".to_owned();
    let labels = last.labels.clone();
    let inputs = last.inputs.clone();
    let at = |t: &str| now(t);

    assert!(asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &inputs,
        at("2026-03-01T12:00:00Z")
    ));
    assert!(asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &inputs,
        at("2026-03-01T12:01:00Z")
    ));
    assert!(!asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &inputs,
        at("2026-03-01T12:01:01Z")
    ));
    assert!(!asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &inputs,
        at("2026-03-01T11:59:59Z")
    ));
    assert!(!asked::is_repeat(
        &last,
        "other",
        Via::Search,
        &labels,
        &inputs,
        at("2026-03-01T12:00:10Z")
    ));
    assert!(!asked::is_repeat(
        &last,
        "synthetic",
        Via::Chat,
        &labels,
        &inputs,
        at("2026-03-01T12:00:10Z")
    ));

    let other_labels = labels.union(&LabelSet::from_ids([
        LabelId::parse("np-0007").expect("label")
    ]));
    assert!(!asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &other_labels,
        &inputs,
        at("2026-03-01T12:00:10Z")
    ));

    let other_store: Vec<ItemRef> = inputs
        .iter()
        .map(|item| ItemRef::new(StoreId::State, item.id.clone()))
        .collect();
    assert!(!asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &other_store,
        at("2026-03-01T12:00:10Z")
    ));
    assert!(!asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &[],
        at("2026-03-01T12:00:10Z")
    ));

    // Repeated pairs count once, and a hashed row repeats its question.
    let doubled: Vec<ItemRef> = inputs.iter().chain(inputs.iter()).cloned().collect();
    assert!(asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &doubled,
        at("2026-03-01T12:00:10Z")
    ));
    last.question = asked::hashed("synthetic");
    assert!(asked::is_repeat(
        &last,
        "synthetic",
        Via::Search,
        &labels,
        &inputs,
        at("2026-03-01T12:00:10Z")
    ));
}

#[test]
fn expiry_by_days() {
    let rows = fixture_rows();
    let (kept, removed) = asked::expire(rows.clone(), now("2026-03-01T12:00:00Z"), 365);
    assert_eq!(
        removed.iter().map(|r| r.id.to_string()).collect::<Vec<_>>(),
        ["A-0001"]
    );
    assert_eq!(kept.len(), 5);

    let (kept, removed) = asked::expire(rows.clone(), now("2026-03-01T12:00:00Z"), 10);
    assert_eq!(
        kept.iter().map(|r| r.id.to_string()).collect::<Vec<_>>(),
        ["A-0005", "A-0006"]
    );
    assert_eq!(removed.len(), 4);

    let (kept, removed) = asked::expire(rows, now("2026-03-01T12:00:00Z"), 100_000);
    assert_eq!((kept.len(), removed.len()), (6, 0));
}

#[test]
fn next_id_is_above_the_rows_and_the_journal() {
    let rows: Vec<AskedRow> = (1..=9).map(row).collect();
    assert_eq!(asked::next_id(&rows, Some(id(12))), Ok(id(13)));
    assert_eq!(asked::next_id(&rows, Some(id(3))), Ok(id(10)));
    assert_eq!(asked::next_id(&rows, None), Ok(id(10)));
    assert_eq!(asked::next_id(&[], Some(id(12))), Ok(id(13)));
    assert_eq!(asked::next_id(&[], None), Ok(id(1)));
    assert_eq!(asked::next_id(&[row(9998)], None), Ok(id(9999)));
    assert!(asked::next_id(&[row(9999)], None).is_err());
    assert!(asked::next_id(&[], Some(id(9999))).is_err());
    assert_eq!(id(9999).to_string(), "A-9999");
    assert!(AskedId::new(0).is_none());
    assert!(AskedId::new(10_000).is_none());
}

#[test]
fn summary_counts_misses_and_promoted() {
    let summary = asked::summary(&fixture_rows());
    assert_eq!(
        summary,
        AskedSummary {
            asked: 6,
            misses: 3,
            promoted: 2
        }
    );
    assert_eq!(
        summary.to_string(),
        "6 questions asked, 3 misses, 2 promoted"
    );
    assert_eq!(
        asked::summary(&[]).to_string(),
        "0 questions asked, 0 misses, 0 promoted"
    );
}

#[test]
fn with_outcome_changes_only_an_answered_row() {
    let rows = fixture_rows();
    // A-0002 is answered.
    for to in [Outcome::CiteFailed, Outcome::Flagged] {
        let next = asked::with_outcome(&rows, id(2), to).expect("an answered row changes");
        assert_eq!(next[1].outcome, to);
        assert_eq!(next[1].inputs, rows[1].inputs);
        assert_eq!(next[1].labels, rows[1].labels);
        let mut expected = rows.clone();
        expected[1].outcome = to;
        assert_eq!(next, expected, "only that cell changes");
    }
    // A-0001 is no_result, A-0003 cite_failed, A-0004 flagged.
    assert!(asked::with_outcome(&rows, id(1), Outcome::Flagged).is_none());
    assert!(asked::with_outcome(&rows, id(3), Outcome::Flagged).is_none());
    assert!(asked::with_outcome(&rows, id(4), Outcome::CiteFailed).is_none());
    assert!(asked::with_outcome(&rows, id(77), Outcome::Flagged).is_none());
    assert!(asked::with_outcome(&rows, id(2), Outcome::Answered).is_none());
    assert!(asked::with_outcome(&rows, id(2), Outcome::NoResult).is_none());
    assert_eq!(rows[4].state, RowState::Dismissed);
}

#[test]
fn hashed_form() {
    let hashed = asked::hashed("synthetic");
    assert!(hashed.starts_with("sha256:"));
    let hex = &hashed["sha256:".len()..];
    assert_eq!(hex.len(), 64);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(hex, fieldbook_core::hash::sha256_hex(b"synthetic"));
    assert_ne!(asked::hashed("synthetic"), asked::hashed("Synthetic"));
}

#[test]
fn labels_column_round_trips() {
    let rows = fixture_rows();
    assert_eq!(rows[2].labels.to_cell(), "np-0007|owner-supplied");
    assert_eq!(rows[1].labels, LabelSet::owner_supplied());

    let mut row = rows[1].clone();
    row.labels = LabelSet::owner_supplied()
        .union(&LabelSet::public())
        .union(&LabelSet::from_ids([
            LabelId::parse("np-0007").expect("label")
        ]));
    let back = asked::parse(asked::render(std::slice::from_ref(&row)).as_bytes()).expect("parses");
    assert_eq!(back[0].labels, row.labels);

    let text = String::from_utf8(FIXTURE.to_vec()).expect("utf-8");
    let empty = text.replacen(",np-0007|owner-supplied,", ",,", 1);
    assert!(asked::parse(empty.as_bytes()).is_err(), "a row has labels");
    let bad = text.replacen(",np-0007|owner-supplied,", ",NP 7,", 1);
    assert!(asked::parse(bad.as_bytes()).is_err());
}

#[test]
fn asked_parse_never_panics_on_random_and_mutated_bytes() {
    let mut rng = Rng::new(0x0a5c_ed01);
    for _ in 0..200 {
        let len = (rng.next_u64() % 512) as usize;
        let bytes = rng.bytes(len);
        let _ = asked::parse(&bytes);
        let mut prefixed = FIXTURE[..FIXTURE.len().min(140)].to_vec();
        prefixed.extend_from_slice(&bytes);
        let _ = asked::parse(&prefixed);
    }
    for i in 0..FIXTURE.len() {
        for delta in [1u8, 0x20, 0x80, 0xff] {
            let mut mutated = FIXTURE.to_vec();
            mutated[i] = mutated[i].wrapping_add(delta);
            if let Ok(rows) = asked::parse(&mutated) {
                let _ = asked::render(&rows);
                let _ = asked::summary(&rows);
            }
        }
    }
}
