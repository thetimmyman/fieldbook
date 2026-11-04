//! Deterministic fuzz/mutation tests ensuring `AppliesTo::parse`, `ClaimUnit::parse`,
//! `ClaimRow::from_row`, and the extension locator parser never panic.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::schema::claims::{AppliesTo, ClaimRow, ClaimUnit};
use fieldbook_core::schema::locator::{Locator, LocatorKind};
use fieldbook_core::schema::table::Row;
use rng::Rng;

fn make_row(cells: Vec<String>) -> Row {
    let cols = (1..=u32::try_from(cells.len()).unwrap_or(1)).collect();
    Row {
        line: 2,
        cols,
        cells,
    }
}

const VALID_CLAIM: [&str; 35] = [
    "CL-CMP-0011",
    "cmp",
    "fact",
    "Single-system payment withholding is capped at five percent.",
    "2025-11-01",
    "S-0042",
    "2",
    "pdf_page_box",
    "p. 14",
    "five percent for a single business system",
    "true",
    "codex",
    "unchecked",
    "pending_company_confirmation",
    "proposed",
    "unknown",
    "V1",
    "5",
    "percent",
    "2025-10-15",
    "2026-10-15",
    "dfars_codified",
    "48 CFR 252.242-7005",
    "RFO-252-7005",
    "2025-10",
    "final",
    "event=award_date; entered_from=2025-01-01; contract_type=fixed_price",
    "E",
    "2025-11-04",
    "owner",
    "CL-CMP-0009",
    "CL-CMP-0012",
    "XC-14",
    "Q-027",
    "Synthetic claim note.",
];

fn exercise_string(s: &str) {
    let _ = AppliesTo::parse(s);
    let _ = ClaimUnit::parse(s);

    // Exercise as a single-cell row and inside each column of a valid claim row
    let _ = ClaimRow::from_row(&make_row(vec![s.to_owned()]));
    let base: Vec<String> = VALID_CLAIM.iter().map(|&c| c.to_owned()).collect();
    for idx in 0..base.len() {
        let mut cells = base.clone();
        cells[idx] = s.to_owned();
        if let Err(probs) = ClaimRow::from_row(&make_row(cells)) {
            assert!(!probs.is_empty());
        }
    }
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let mut rng = Rng::new(1);
    for i in 0..200usize {
        let len = i % 129;
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        exercise_string(&text);
    }

    // Mutate valid `applies_to` and `unit` cells (every byte flipped and deleted)
    for seed_str in [
        "event=award_date; entered_from=2025-01-01; contract_type=fixed_price",
        "weeks",
        "EUR",
    ] {
        let raw_bytes = seed_str.as_bytes();
        for b_idx in 0..raw_bytes.len() {
            let mut flipped = raw_bytes.to_vec();
            flipped[b_idx] ^= 0x01;
            let flipped_s = String::from_utf8_lossy(&flipped).into_owned();
            let _ = AppliesTo::parse(&flipped_s);
            let _ = ClaimUnit::parse(&flipped_s);

            let mut deleted = raw_bytes.to_vec();
            deleted.remove(b_idx);
            let deleted_s = String::from_utf8_lossy(&deleted).into_owned();
            let _ = AppliesTo::parse(&deleted_s);
            let _ = ClaimUnit::parse(&deleted_s);
        }
    }

    // Mutate the valid synthetic claim row one cell at a time (every byte flipped and deleted)
    let base: Vec<String> = VALID_CLAIM.iter().map(|&c| c.to_owned()).collect();
    for len in 0..base.len() {
        let _ = ClaimRow::from_row(&make_row(base[..len].to_vec()));
    }
    for cell_idx in 0..base.len() {
        let cell_bytes = base[cell_idx].as_bytes();
        for b_idx in 0..cell_bytes.len() {
            let mut flipped = cell_bytes.to_vec();
            flipped[b_idx] ^= 0x01;
            let flipped_s = String::from_utf8_lossy(&flipped).into_owned();
            let _ = AppliesTo::parse(&flipped_s);
            let _ = ClaimUnit::parse(&flipped_s);
            let mut row_flipped = base.clone();
            row_flipped[cell_idx] = flipped_s;
            let _ = ClaimRow::from_row(&make_row(row_flipped));

            let mut deleted = cell_bytes.to_vec();
            deleted.remove(b_idx);
            let deleted_s = String::from_utf8_lossy(&deleted).into_owned();
            let _ = AppliesTo::parse(&deleted_s);
            let _ = ClaimUnit::parse(&deleted_s);
            let mut row_deleted = base.clone();
            row_deleted[cell_idx] = deleted_s;
            let _ = ClaimRow::from_row(&make_row(row_deleted));
        }
    }
}

fn check_extension_candidate(candidate: &str, base_claim: &[String]) {
    let direct = Locator::parse(candidate);
    let direct_ext = Locator::parse_extension(candidate);
    if let Ok(ref loc) = direct
        && matches!(loc, Locator::Extension { .. })
    {
        assert_eq!(loc.to_canonical_string(), candidate);
    }
    if let Ok(ref loc) = direct_ext {
        assert_eq!(loc.to_canonical_string(), candidate);
    }

    let mut cells = base_claim.to_vec();
    cells[7] = "ext:structured_path".to_owned();
    cells[8] = candidate.to_owned();
    if let Ok(parsed_row) = ClaimRow::from_row(&make_row(cells.clone())) {
        assert_eq!(
            parsed_row.locator_kind,
            Some(LocatorKind::Extension("structured_path".to_owned()))
        );
        assert_eq!(parsed_row.locator.as_deref(), Some(candidate));
        assert_eq!(parsed_row.to_cells(), cells);
    }
}

#[test]
fn extension_locator_parser_never_panics() {
    let base: Vec<String> = VALID_CLAIM.iter().map(|&c| c.to_owned()).collect();
    let canonical = "ext structured_path sec%2F%CE%B1%CE%B2%2525%3B%5D%0Aline2";
    check_extension_candidate(canonical, &base);

    // Explicit truncated-escape boundary inputs ending in `%` and `%0`
    for boundary in [
        "ext structured_path %",
        "ext structured_path %0",
        "ext structured_path item%",
        "ext structured_path item%0",
        "%",
        "%0",
    ] {
        assert!(Locator::parse(boundary).is_err());
        assert!(Locator::parse_extension(boundary).is_err());
        check_extension_candidate(boundary, &base);
    }

    let mut rng = Rng::new(1);
    for i in 0..200usize {
        let len = i % 129;
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        check_extension_candidate(&text, &base);
        let prefixed = format!("ext structured_path {text}");
        check_extension_candidate(&prefixed, &base);
    }

    let raw_bytes = canonical.as_bytes();
    for b_idx in 0..raw_bytes.len() {
        let mut flipped = raw_bytes.to_vec();
        flipped[b_idx] ^= 0x01;
        let flipped_s = String::from_utf8_lossy(&flipped).into_owned();
        check_extension_candidate(&flipped_s, &base);

        let mut deleted = raw_bytes.to_vec();
        deleted.remove(b_idx);
        let deleted_s = String::from_utf8_lossy(&deleted).into_owned();
        check_extension_candidate(&deleted_s, &base);
    }
}
