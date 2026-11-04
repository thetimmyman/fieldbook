//! Deterministic fuzz/mutation tests ensuring the four register row parsers,
//! vocabulary parsers, and `stream_id_form` never panic.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::schema::conflicts::ConflictRow;
use fieldbook_core::schema::glossary::GlossaryRow;
use fieldbook_core::schema::questions::QuestionRow;
use fieldbook_core::schema::sources::SourceRow;
use fieldbook_core::schema::table::{self, Row};
use fieldbook_core::schema::vocab::{
    Adoption, AuthorityTier, ConflictStatus, DefinitionStatus, DocKind, FamilyBasis, Lifecycle,
    QuestionStatus, SourceKind, SourceRole, WorkCanContinue, stream_id_form,
};
use rng::Rng;

fn make_row(cells: Vec<String>) -> Row {
    let cols = (1..=u32::try_from(cells.len()).unwrap_or(1)).collect();
    Row {
        line: 2,
        cols,
        cells,
    }
}

fn check_vocab_and_stream(s: &str) {
    let _ = SourceKind::parse(s);
    let _ = SourceRole::parse(s);
    let _ = AuthorityTier::parse(s);
    let _ = FamilyBasis::parse(s);
    let _ = Lifecycle::parse(s);
    let _ = QuestionStatus::parse(s);
    let _ = WorkCanContinue::parse(s);
    let _ = ConflictStatus::parse(s);
    let _ = DefinitionStatus::parse(s);
    let _ = DocKind::parse(s);
    let _ = Adoption::parse(s);
    let _ = stream_id_form(s);
}

fn check_row(row: &Row) {
    if let Err(probs) = SourceRow::from_row(row) {
        assert!(!probs.is_empty());
    }
    if let Err(probs) = QuestionRow::from_row(row) {
        assert!(!probs.is_empty());
    }
    if let Err(probs) = ConflictRow::from_row(row) {
        assert!(!probs.is_empty());
    }
    if let Err(probs) = GlossaryRow::from_row(row) {
        assert!(!probs.is_empty());
    }
}

fn mutate_row_bytes(valid_cells: &[&str]) {
    let base: Vec<String> = valid_cells.iter().map(|&s| s.to_owned()).collect();
    check_row(&make_row(base.clone()));

    // Exercise truncated rows (missing cells from 0 up to base.len())
    for len in 0..base.len() {
        check_row(&make_row(base[..len].to_vec()));
    }

    // Exercise single-byte flips and deletions in every cell
    for cell_idx in 0..base.len() {
        let cell_bytes = base[cell_idx].as_bytes();
        for b_idx in 0..cell_bytes.len() {
            let mut flipped = cell_bytes.to_vec();
            flipped[b_idx] ^= 0x01;
            let flipped_str = String::from_utf8_lossy(&flipped).into_owned();
            check_vocab_and_stream(&flipped_str);
            let mut row_cells = base.clone();
            row_cells[cell_idx] = flipped_str;
            check_row(&make_row(row_cells));

            let mut deleted = cell_bytes.to_vec();
            deleted.remove(b_idx);
            let deleted_str = String::from_utf8_lossy(&deleted).into_owned();
            check_vocab_and_stream(&deleted_str);
            let mut row_cells_del = base.clone();
            row_cells_del[cell_idx] = deleted_str;
            check_row(&make_row(row_cells_del));
        }
    }
}

#[test]
fn parser_never_panics_on_200_seeded_inputs() {
    let mut rng = Rng::new(1);
    for _ in 0..200 {
        let bytes = rng.bytes(128);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        check_vocab_and_stream(&text);

        let split_cells: Vec<String> = text.split(',').map(str::to_owned).collect();
        let single_cell_row = make_row(vec![text]);
        check_row(&single_cell_row);
        check_row(&make_row(split_cells));

        if let Ok(tbl) = table::read(&bytes) {
            check_row(&tbl.header);
            for r in &tbl.rows {
                check_row(r);
            }
        }
    }

    let valid_source = [
        "S-0042",
        "2",
        "FAR 15 codified",
        "48 CFR Part 15 Contracting by Negotiation",
        "Office of the Federal Register",
        "primary_law",
        "primary_evidence",
        "A",
        "US Government work, public domain",
        "far_part_15",
        "reviewed",
        "S-0042-v2.pdf",
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        "2025-10",
        "2025-10-15",
        "2025-11-01",
        "2025-11-02",
        "active",
        "S-0042-v1",
        "V1",
        "public",
        "Synthetic sample note.",
        "Answer pricing questions.",
        "erp-core",
        "commitments",
        "PO-2025-0042",
        "quote",
        "commitments",
    ];
    mutate_row_bytes(&valid_source);

    let valid_question = [
        "Q-027",
        "cmp",
        "Which business systems are in scope?",
        "Finance lead",
        "Compliance checklist sign-off",
        "Determines withholding exposure.",
        "Assume all six systems require mapping.",
        "partly",
        "true",
        "open",
        "Accounting and estimating first.",
        "2025-11-03",
        "proposed",
    ];
    mutate_row_bytes(&valid_question);

    let valid_conflict = [
        "XC-14",
        "cmp",
        "Withholding cap percentage",
        "Cap is five percent|Cap is ten percent",
        "Use five percent.",
        "Primary clause text controls.",
        "CL-CMP-0011|S-0042-v2",
        "2025-11-02",
        "resolved",
        "CL-CMP-0011|CL-CMP-0012",
        "Q-027",
    ];
    mutate_row_bytes(&valid_conflict);

    let valid_glossary = [
        "material_weakness",
        "material weakness",
        "A deficiency in internal control.",
        "Distinct from a significant deficiency.",
        "cmp",
        "S-0042-v2|CL-CMP-0011",
        "public_reference",
        "MW|material control weakness",
        "significant_deficiency",
    ];
    mutate_row_bytes(&valid_glossary);
}
