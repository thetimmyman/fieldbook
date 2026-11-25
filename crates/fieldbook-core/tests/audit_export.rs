//! Tests for the pure audit export planner and verifier (`audit-export` steps 1 and 2).

#![expect(
    clippy::expect_used,
    reason = "integration tests assert pure plan and verification results"
)]

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::CivilDate;
use fieldbook_core::audit::export::{ExportError, ExportLine, is_uuid, plan_export, verify_export};
use fieldbook_core::audit::{
    Actor, EventEnvelope, Kind, Now, Principal, Role, Sha256, StoreId, Via, ZERO_HASH,
    chain::seal_body, sample_payload, split_hash_suffix,
};
use fieldbook_core::check::Code;
use rng::Rng;

fn now() -> Now {
    Now::new(
        CivilDate::parse("2026-03-01").expect("valid date"),
        12 * 3600,
    )
    .expect("valid time")
}

fn env() -> EventEnvelope {
    EventEnvelope::from_actor(
        &Actor::new(Principal::local(), Role::Lead, Via::Cli),
        StoreId::Corpus,
    )
}

fn line_for(kind: Kind, seq: u64, prev: &str) -> (ExportLine, Sha256) {
    let (line, hash) = sample_payload(kind, seq).format_line(seq, now(), &env(), prev);
    (
        ExportLine {
            seq,
            hash: hash.clone(),
            raw: format!("{line}\n"),
        },
        hash,
    )
}

fn three_lines() -> Vec<ExportLine> {
    let (first, h1) = line_for(Kind::JournalOpen, 1, ZERO_HASH);
    let (second, h2) = line_for(Kind::WriteBegin, 2, h1.as_str());
    let (third, _) = line_for(Kind::WriteEnd, 3, h2.as_str());
    vec![first, second, third]
}

fn sha(text: &str) -> Sha256 {
    Sha256::digest(text.as_bytes())
}

fn chain_id(lines: &[ExportLine]) -> Sha256 {
    lines
        .first()
        .map(|line| line.hash.clone())
        .unwrap_or_else(Sha256::zero)
}

fn plan(lines: &[ExportLine], since: u64) -> fieldbook_core::audit::export::ExportPlan {
    plan_export(
        lines,
        chain_id(lines),
        0,
        "00000000-0000-0000-0000-000000000000",
        since,
        sha("policy"),
        "0.1.0",
    )
    .expect("plan")
}

#[test]
fn plan_selects_from_since_to_the_head() {
    let lines = three_lines();
    let planned = plan(&lines, 2);
    assert_eq!(planned.first_seq, 2);
    assert_eq!(planned.last_seq, 3);
    assert_eq!(planned.events(), 2);
    let expected: String = lines[1..].iter().map(|line| line.raw.as_str()).collect();
    assert_eq!(planned.bytes, expected.as_bytes());
}

#[test]
fn the_exported_bytes_are_the_journal_lines_byte_for_byte() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let expected: Vec<u8> = lines
        .iter()
        .flat_map(|line| line.raw.as_bytes().to_vec())
        .collect();
    assert_eq!(planned.bytes, expected);
}

#[test]
fn the_manifest_is_deterministic() {
    let lines = three_lines();
    let first = plan(&lines, 1).manifest_text();
    let second = plan(&lines, 1).manifest_text();
    assert_eq!(first, second);
}

#[test]
fn the_manifest_carries_the_store_uid() {
    let lines = three_lines();
    let uid = "11111111-2222-3333-4444-555555555555";
    let planned =
        plan_export(&lines, chain_id(&lines), 3, uid, 1, sha("policy"), "0.1.0").expect("plan");
    assert!(
        planned
            .manifest_text()
            .contains(&format!("store_uid = \"{uid}\""))
    );
}

#[test]
fn a_machine_without_a_company_store_exports_the_nil_uuid() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    assert!(
        planned
            .manifest_text()
            .contains("store_uid = \"00000000-0000-0000-0000-000000000000\"")
    );
}

#[test]
fn prev_is_the_hash_before_the_first_line_or_zeros() {
    let lines = three_lines();
    assert_eq!(plan(&lines, 1).prev.as_str(), ZERO_HASH);
    assert_eq!(plan(&lines, 2).prev, lines[0].hash);
}

#[test]
fn names_carry_chain_writer_and_range() {
    let lines = three_lines();
    let chain = chain_id(&lines);
    let planned = plan_export(
        &lines,
        chain.clone(),
        7,
        "00000000-0000-0000-0000-000000000000",
        2,
        sha("policy"),
        "0.1.0",
    )
    .expect("plan");
    let prefix = &chain.as_str()[..12.min(chain.as_str().len())];
    assert!(planned.file_name.contains("w7"));
    assert!(planned.file_name.contains(prefix));
    assert!(planned.file_name.ends_with("-000002-3.jsonl"));
    assert_eq!(
        planned.manifest_name,
        format!(
            "{}.manifest.toml",
            planned
                .file_name
                .strip_suffix(".jsonl")
                .expect("jsonl suffix")
        )
    );
}

#[test]
fn since_after_the_head_is_nothing_new() {
    let lines = three_lines();
    let err = plan_export(
        &lines,
        chain_id(&lines),
        0,
        "00000000-0000-0000-0000-000000000000",
        4,
        sha("policy"),
        "0.1.0",
    )
    .expect_err("nothing new");
    assert_eq!(err, ExportError::NothingNew { last_seq: 3 });
}

#[test]
fn two_chains_get_different_names() {
    let lines = three_lines();
    let other = Sha256::digest(b"another chain");
    let first = plan_export(
        &lines,
        chain_id(&lines),
        0,
        "00000000-0000-0000-0000-000000000000",
        1,
        sha("policy"),
        "0.1.0",
    )
    .expect("plan");
    let second = plan_export(
        &lines,
        other,
        0,
        "00000000-0000-0000-0000-000000000000",
        1,
        sha("policy"),
        "0.1.0",
    )
    .expect("plan");
    assert_ne!(first.file_name, second.file_name);
}

#[test]
fn verify_accepts_a_clean_export() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let findings = verify_export(&planned.manifest_text(), &planned.bytes, None, None);
    assert_eq!(findings, Vec::new());
}

#[test]
fn a_changed_byte_is_fb0800_naming_the_line() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let mut bytes = planned.bytes.clone();
    let second = planned.first_seq + 1;
    let offset = lines[0].raw.len() + 10;
    bytes[offset] = if bytes[offset] == b'x' { b'y' } else { b'x' };
    let findings = verify_export(&planned.manifest_text(), &bytes, None, None);
    assert!(
        findings
            .iter()
            .any(|finding| finding.code == Code::Fb0800 && finding.seq == second),
        "findings: {findings:?}"
    );
}

#[test]
fn a_truncated_copy_is_fb0802() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let truncated: String = lines[..2].iter().map(|line| line.raw.as_str()).collect();
    let findings = verify_export(&planned.manifest_text(), truncated.as_bytes(), None, None);
    assert!(
        findings.iter().any(|finding| finding.code == Code::Fb0802),
        "findings: {findings:?}"
    );
}

#[test]
fn a_manifest_with_an_unknown_key_is_refused() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let text = format!("{}evil = 1\n", planned.manifest_text());
    let findings = verify_export(&text, &planned.bytes, None, None);
    assert!(!findings.is_empty());
    assert!(findings.iter().any(|finding| finding.code == Code::Fb0802));
}

#[test]
fn a_line_with_a_free_text_key_is_fb0801() {
    let (first, h1) = line_for(Kind::JournalOpen, 1, ZERO_HASH);
    let (second_raw, _) =
        sample_payload(Kind::WriteBegin, 2).format_line(2, now(), &env(), h1.as_str());
    let (body, _) = split_hash_suffix(&second_raw).expect("sealed line");
    let with_note = format!("{body},\"note\":\"free text\"");
    let (sealed, sealed_hash) = seal_body(h1.as_str(), &with_note);
    let second = ExportLine {
        seq: 2,
        hash: sealed_hash,
        raw: format!("{sealed}\n"),
    };
    let lines = vec![first, second];
    let planned = plan(&lines, 1);
    let findings = verify_export(&planned.manifest_text(), &planned.bytes, None, None);
    assert!(
        findings
            .iter()
            .any(|finding| finding.code == Code::Fb0801 && finding.seq == 2),
        "findings: {findings:?}"
    );
}

#[test]
fn a_wrong_lines_hash_is_fb0802() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let text = planned
        .manifest_text()
        .replace(planned.lines_sha256.as_str(), ZERO_HASH);
    let findings = verify_export(&text, &planned.bytes, None, None);
    assert!(findings.iter().any(|finding| finding.code == Code::Fb0802));
}

#[test]
fn expect_head_is_checked_against_the_segment() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let manifest = planned.manifest_text();
    let good = verify_export(&manifest, &planned.bytes, Some(planned.head.as_str()), None);
    assert_eq!(good, Vec::new());
    let bad = verify_export(&manifest, &planned.bytes, Some(ZERO_HASH), None);
    assert!(bad.iter().any(|finding| finding.code == Code::Fb0802));
}

#[test]
fn a_manifest_with_a_bad_store_uid_is_fb0802() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let text = planned
        .manifest_text()
        .replace("00000000-0000-0000-0000-000000000000", "not-a-uuid");
    let findings = verify_export(&text, &planned.bytes, None, None);
    assert!(findings.iter().any(|finding| finding.code == Code::Fb0802));
}

#[test]
fn a_manifest_naming_a_path_instead_of_a_bare_file_is_fb0802() {
    let lines = three_lines();
    let planned = plan(&lines, 1);
    let text = planned.manifest_text();
    let name = text
        .lines()
        .find_map(|line| line.strip_prefix("file = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_else(|| panic!("the manifest has a file line"))
        .to_owned();
    for bad in ["../x.jsonl", "/etc/passwd", "sub/x.jsonl", "C:\\x.jsonl"] {
        let changed = text.replace(&format!("file = \"{name}\""), &format!("file = \"{bad}\""));
        assert_ne!(changed, text);
        let findings = verify_export(&changed, &planned.bytes, None, None);
        assert!(
            findings.iter().any(|finding| finding.code == Code::Fb0802),
            "{bad} must be refused"
        );
    }
}

#[test]
fn verify_export_never_panics_on_random_and_mutated_bytes() {
    let mut rng = Rng::new(1);
    for index in 0..200usize {
        let len = index % 257;
        let manifest = String::from_utf8_lossy(&rng.bytes(len)).into_owned();
        let lines = rng.bytes(len);
        let _ = verify_export(&manifest, &lines, None, None);
    }

    let lines = three_lines();
    let planned = plan(&lines, 1);
    let manifest_text = planned.manifest_text();
    let manifest_bytes = manifest_text.as_bytes();
    for index in 0..manifest_bytes.len() {
        let mut mutated = manifest_bytes.to_vec();
        mutated[index] ^= 0xff;
        let _ = verify_export(
            &String::from_utf8_lossy(&mutated),
            &planned.bytes,
            None,
            None,
        );
        let mut deleted = manifest_bytes.to_vec();
        deleted.remove(index);
        let _ = verify_export(
            &String::from_utf8_lossy(&deleted),
            &planned.bytes,
            None,
            None,
        );
    }
    for index in 0..planned.bytes.len() {
        let mut mutated = planned.bytes.clone();
        mutated[index] ^= 0xff;
        let _ = verify_export(&manifest_text, &mutated, None, None);
        let mut deleted = planned.bytes.clone();
        deleted.remove(index);
        let _ = verify_export(&manifest_text, &deleted, None, None);
    }
}

#[test]
fn a_uuid_is_lower_case_eight_four_four_four_twelve() {
    assert!(is_uuid("00000000-0000-0000-0000-000000000000"));
    assert!(is_uuid("abcdef01-2345-6789-abcd-ef0123456789"));
    assert!(!is_uuid("ABCDEF01-2345-6789-abcd-ef0123456789"));
    assert!(!is_uuid("00000000-0000-0000-0000-00000000000"));
}
