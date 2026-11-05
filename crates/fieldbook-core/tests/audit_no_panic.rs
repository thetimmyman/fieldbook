//! Seeded no-panic test for the audit journal line parser and chain verifier (`audit-journal` step 9).

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    Actor, Event, EventEnvelope, Kind, Now, Principal, Role, StoreId, Via, ZERO_HASH,
    sample_payload, verify_line,
};
use rng::Rng;

fn probe_journal(text: &str) {
    let mut prev = ZERO_HASH.to_owned();
    for line in text.lines() {
        if let Ok(verified) = verify_line(&prev, line) {
            prev = verified.hash.as_str().to_owned();
        }
        let _ = Event::parse_line(line);
    }
    let _ = verify_line(ZERO_HASH, text);
    let _ = Event::parse_line(text);
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let mut rng = Rng::new(1);
    for i in 0..200usize {
        let len = i % 129;
        let raw = rng.bytes(len);
        let text = String::from_utf8_lossy(&raw);
        probe_journal(&text);
    }

    let time = Now::new(
        CivilDate::parse("2026-03-01").expect("valid date"),
        12 * 3600,
    )
    .expect("valid time");
    let env = EventEnvelope::from_actor(
        &Actor::new(Principal::local(), Role::Lead, Via::Cli),
        StoreId::Corpus,
    );

    let (line1, h1) = sample_payload(Kind::JournalOpen, 1).format_line(1, time, &env, ZERO_HASH);
    let (line2, h2) = sample_payload(Kind::WriteBegin, 2).format_line(2, time, &env, h1.as_str());
    let (line3, _h3) = sample_payload(Kind::WriteEnd, 3).format_line(3, time, &env, h2.as_str());
    let valid = format!("{line1}\n{line2}\n{line3}\n");

    for line in valid.lines() {
        assert!(Event::parse_line(line).is_ok());
    }

    let valid_bytes = valid.as_bytes();
    for idx in 0..valid_bytes.len() {
        let mut flipped = valid_bytes.to_vec();
        flipped[idx] ^= 0x5a;
        let flipped_str = String::from_utf8_lossy(&flipped);
        probe_journal(&flipped_str);

        let mut deleted = valid_bytes.to_vec();
        deleted.remove(idx);
        let deleted_str = String::from_utf8_lossy(&deleted);
        probe_journal(&deleted_str);
    }
}
