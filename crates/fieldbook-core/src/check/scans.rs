//! The `check-scans` pass: `FB0400` to `FB0405` and `FB0410`
//! (`docs/reference/corpus-format.md#scans`, `#handling-and-labels`).
//!
//! **Responsibility.**
//! - `FB0400` (error): marking banner shapes in every committed text file under `corpus/`
//!   (extracted text included), unless covered by `corpus/scan-allow.toml`.
//! - `FB0401` (error): identity shapes in hand-written files of `corpus/` (everything
//!   under `corpus/` except `corpus/sources/`).
//! - `FB0402` (error): claim-language phrases in page bodies (excluding `Precise:`
//!   blockquotes and fenced/inline code) and in the `statement` column of `claims.csv`
//!   (never `quote`).
//! - `FB0403` (error): spreadsheet formula triggers (`=`, `+`, `-`, `@`, TAB or CR) at
//!   the start of a text cell in any of the five registers.
//! - `FB0404` (error): malformed, incomplete or duplicate entry in `corpus/scan-allow.toml`.
//! - `FB0405` (warning): an entry in `corpus/scan-allow.toml` that matched nothing in this run.
//! - `FB0410` (error): `handling` of a page or source version in the repository Corpus is
//!   anything other than `["public"]`.

use crate::hash::sha256_hex;
use crate::scan::allow::{self, Allowances};
use crate::scan::{Finding, claim_language, formula, identity, marking};
use crate::schema::Register;
use crate::schema::corpus_toml::byte_offset_to_line_col;
use crate::schema::page::PageMeta;
use crate::schema::table::Row;

use super::markdown::{self, Ev, Event, Tag, TagEnd};
use super::model::RawPageOutcome;
use super::registers::register_path;
use super::{Code, Diagnostic, Entry, Model, Options, Report};

const SCAN_ALLOW_PATH: &str = "corpus/scan-allow.toml";
const SOURCES_PREFIX: &str = "corpus/sources/";
const PUBLIC_ORIGINALS_PREFIX: &str = "corpus/sources/public/";
const PRECISE_PREFIX: &str = "Precise:";

/// Run the `scans` check pass (`FB0400`..`FB0405`, `FB0410`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    let strict = options.strict;
    let mut allowances = load_scan_allow(model, strict, report);

    check_markings_and_identity(model, allowances.as_mut(), strict, report);

    if let Some(ref allow_table) = allowances {
        for unused in allow_table.unused() {
            report.push(Diagnostic::new(
                Code::Fb0405,
                strict,
                SCAN_ALLOW_PATH,
                unused.line,
                unused.col,
                format!(
                    "`scan-allow.toml` entry for `sha256 = \"{}\"` and `match = \"{}\"` matched nothing",
                    unused.sha256, unused.matched
                ),
            ));
        }
    }

    check_claim_language(model, strict, report);
    check_formula_triggers(model, strict, report);
    check_handling_labels(model, strict, report);
}

fn load_scan_allow(model: &Model, strict: bool, report: &mut Report) -> Option<Allowances> {
    let bytes = model.tree().get_bytes(SCAN_ALLOW_PATH)?;
    let Ok(text) = std::str::from_utf8(bytes) else {
        return None;
    };
    match allow::parse(text) {
        Ok(table) => Some(table),
        Err(errs) => {
            for err in errs {
                report.push(Diagnostic::new(
                    Code::Fb0404,
                    strict,
                    SCAN_ALLOW_PATH,
                    err.line,
                    err.col,
                    err.message,
                ));
            }
            None
        }
    }
}

fn line_col_at_byte(text: &str, byte_offset: usize) -> (usize, usize) {
    let (line, col) = byte_offset_to_line_col(text, byte_offset);
    (
        usize::try_from(line).unwrap_or(1),
        usize::try_from(col).unwrap_or(1),
    )
}

fn check_markings_and_identity(
    model: &Model,
    mut allowances: Option<&mut Allowances>,
    strict: bool,
    report: &mut Report,
) {
    for (path, entry) in model.tree().iter() {
        if !path.starts_with("corpus/") || path.starts_with(PUBLIC_ORIGINALS_PREFIX) {
            continue;
        }
        let Entry::Bytes(bytes) = entry else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };

        // FB0400: Markings in every committed text file under `corpus/` (except `scan-allow.toml` itself).
        if path != SCAN_ALLOW_PATH {
            let hits = marking::scan(text);
            if !hits.is_empty() {
                let file_hash = sha256_hex(bytes);
                for hit in hits {
                    let is_allowed = allowances
                        .as_deref_mut()
                        .is_some_and(|a| a.allows(&file_hash, &hit.matched));
                    if !is_allowed {
                        let (line, col) = line_col_at_byte(text, hit.span.start);
                        report.push(Diagnostic::new(
                            Code::Fb0400,
                            strict,
                            path,
                            line,
                            col,
                            format!(
                                "marking banner `{}` is not allowed in the Corpus",
                                hit.matched
                            ),
                        ));
                    }
                }
            }
        }

        // FB0401: Identity shapes in hand-written files of `corpus/` (not `corpus/sources/`).
        if !path.starts_with(SOURCES_PREFIX) {
            for hit in identity::scan(text) {
                let (line, col) = line_col_at_byte(text, hit.span.start);
                report.push(Diagnostic::new(
                    Code::Fb0401,
                    strict,
                    path,
                    line,
                    col,
                    format!(
                        "identity shape `{}` is not allowed in the Corpus",
                        hit.matched
                    ),
                ));
            }
        }
    }
}

/// Scan a Markdown page body (starting at 1-based `body_start_line` in the page file)
/// for forbidden claim-language phrases (`FB0402`), skipping `Precise:` blockquotes and
/// fenced or inline code.
///
/// Returns `(Finding, line, col)` tuples in appearance order.
#[must_use]
pub fn scan_page_body_claim_language(
    body: &str,
    body_start_line: usize,
) -> Vec<(Finding, usize, usize)> {
    let base_line = body_start_line.max(1);
    let evs = markdown::events(body, base_line);
    let mut out = Vec::new();
    collect_claim_language_from_events(&evs, &mut out);
    out
}

fn is_precise_blockquote(bq_evs: &[Ev<'_>]) -> bool {
    let mut text = String::new();
    for ev in bq_evs {
        match &ev.event {
            Event::Text(t) | Event::Code(t) => {
                text.push_str(t);
                if text.trim_start().starts_with(PRECISE_PREFIX) {
                    return true;
                }
            }
            _ => {}
        }
    }
    text.trim_start().starts_with(PRECISE_PREFIX)
}

fn advance_line_col(mut line: usize, mut col: usize, prefix: &str) -> (usize, usize) {
    for ch in prefix.chars() {
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn flush_claim_language_buf(
    buf: &mut String,
    pos_map: &mut Vec<(usize, usize)>,
    out: &mut Vec<(Finding, usize, usize)>,
) {
    if buf.is_empty() {
        return;
    }
    for hit in claim_language::scan(buf) {
        let (line, col) = pos_map.get(hit.span.start).copied().unwrap_or((1, 1));
        out.push((hit, line, col));
    }
    buf.clear();
    pos_map.clear();
}

const fn is_inline_formatting_start(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Emphasis
            | Tag::Strong
            | Tag::Strikethrough
            | Tag::Link { .. }
            | Tag::Superscript
            | Tag::Subscript
    )
}

const fn is_inline_formatting_end(tag: &TagEnd) -> bool {
    matches!(
        tag,
        TagEnd::Emphasis
            | TagEnd::Strong
            | TagEnd::Strikethrough
            | TagEnd::Link
            | TagEnd::Superscript
            | TagEnd::Subscript
    )
}

fn collect_claim_language_from_events(evs: &[Ev<'_>], out: &mut Vec<(Finding, usize, usize)>) {
    let mut buf = String::new();
    let mut pos_map: Vec<(usize, usize)> = Vec::new();
    let mut i = 0usize;
    while i < evs.len() {
        match &evs[i].event {
            Event::Start(Tag::CodeBlock(_)) => {
                flush_claim_language_buf(&mut buf, &mut pos_map, out);
                i += 1;
                while i < evs.len() && !matches!(evs[i].event, Event::End(TagEnd::CodeBlock)) {
                    i += 1;
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush_claim_language_buf(&mut buf, &mut pos_map, out);
                let start = i;
                let mut depth = 1usize;
                i += 1;
                while i < evs.len() && depth > 0 {
                    match &evs[i].event {
                        Event::Start(Tag::BlockQuote(_)) => depth += 1,
                        Event::End(TagEnd::BlockQuote(_)) => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                let slice = &evs[start..i];
                if !is_precise_blockquote(slice) && slice.len() > 2 {
                    collect_claim_language_from_events(&slice[1..slice.len() - 1], out);
                }
                continue;
            }
            Event::Text(text) => {
                for (byte_idx, ch) in text.char_indices() {
                    let (line, col) = advance_line_col(evs[i].line, evs[i].col, &text[..byte_idx]);
                    for _ in 0..ch.len_utf8() {
                        pos_map.push((line, col));
                    }
                    buf.push(ch);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if !buf.is_empty() {
                    pos_map.push((evs[i].line, evs[i].col));
                    buf.push(' ');
                }
            }
            Event::Start(tag) if is_inline_formatting_start(tag) => {}
            Event::End(tag) if is_inline_formatting_end(tag) => {}
            _ => {
                flush_claim_language_buf(&mut buf, &mut pos_map, out);
            }
        }
        i += 1;
    }
    flush_claim_language_buf(&mut buf, &mut pos_map, out);
}

fn col_index(reg: Register, col_name: &str) -> usize {
    reg.columns()
        .iter()
        .position(|c| c.name == col_name)
        .unwrap_or(0)
}

fn col_1based(reg: Register, row: &Row, col_name: &str) -> usize {
    let idx = col_index(reg, col_name);
    row.cols
        .get(idx)
        .copied()
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(1)
}

fn check_claim_language(model: &Model, strict: bool, report: &mut Report) {
    // 1. Page bodies (excluding `Precise:` blockquotes and fenced/inline code).
    for page in model.pages() {
        let base_line = usize::try_from(page.body_line).unwrap_or(1).max(1);
        for (hit, line, col) in scan_page_body_claim_language(&page.body, base_line) {
            report.push(Diagnostic::new(
                Code::Fb0402,
                strict,
                &page.path,
                line,
                col,
                format!(
                    "claim-language phrase `{}` is not allowed in prose",
                    hit.matched
                ),
            ));
        }
    }

    // 2. The `statement` column of `claims.csv` (never `quote`).
    let reg = Register::Claims;
    let stmt_idx = col_index(reg, "statement");
    let path = register_path(reg);
    for row in model.register_rows(reg) {
        let Some(stmt) = row.cells.get(stmt_idx) else {
            continue;
        };
        let base_col = col_1based(reg, row, "statement");
        let line = usize::try_from(row.line).unwrap_or(1);
        for hit in claim_language::scan(stmt) {
            let offset_chars = stmt[..hit.span.start].chars().count();
            report.push(Diagnostic::new(
                Code::Fb0402,
                strict,
                path,
                line,
                base_col + offset_chars,
                format!(
                    "claim-language phrase `{}` is not allowed in a claim `statement`",
                    hit.matched
                ),
            ));
        }
    }
}

fn check_formula_triggers(model: &Model, strict: bool, report: &mut Report) {
    for reg in Register::ALL {
        let path = register_path(reg);
        let cols = reg.columns();
        for row in model.register_rows(reg) {
            let line = usize::try_from(row.line).unwrap_or(1);
            for (idx, cell) in row.cells.iter().enumerate() {
                if let Some(hit) = formula::scan_cell(cell) {
                    let col = row
                        .cols
                        .get(idx)
                        .copied()
                        .and_then(|c| usize::try_from(c).ok())
                        .unwrap_or(1);
                    let col_name = cols.get(idx).map_or("cell", |c| c.name);
                    report.push(Diagnostic::new(
                        Code::Fb0403,
                        strict,
                        path,
                        line,
                        col,
                        format!(
                            "register cell `{col_name}` starts with spreadsheet formula trigger `{}`",
                            hit.matched.escape_default()
                        ),
                    ));
                }
            }
        }
    }
}

fn field_loc(meta: &PageMeta, key: &str) -> (usize, usize) {
    meta.field_location(key)
        .and_then(|(l, c)| Some((usize::try_from(l).ok()?, usize::try_from(c).ok()?)))
        .unwrap_or((1, 1))
}

fn is_public_only(handling: &[String]) -> bool {
    handling.len() == 1 && handling[0] == "public"
}

fn check_handling_labels(model: &Model, strict: bool, report: &mut Report) {
    // 1. Every parsed page's `handling` field (using `PageMeta::field_location("handling")`).
    for raw_page in model.raw_pages() {
        let RawPageOutcome::Parsed(meta) = &raw_page.outcome else {
            continue;
        };
        if !is_public_only(&meta.handling) {
            let (line, col) = field_loc(meta, "handling");
            report.push(Diagnostic::new(
                Code::Fb0410,
                strict,
                &raw_page.path,
                line,
                col,
                format!(
                    "page `handling` must be `[\"public\"]` in the repository Corpus, got {:?}",
                    meta.handling
                ),
            ));
        }
    }

    // 2. Every source version row in `sources.csv` at its `handling` cell.
    let reg = Register::Sources;
    let handling_idx = col_index(reg, "handling");
    let path = register_path(reg);
    for row in model.register_rows(reg) {
        let raw = row.cells.get(handling_idx).map_or("", String::as_str);
        let items: Vec<String> = if raw.is_empty() {
            Vec::new()
        } else {
            raw.split('|').map(str::to_owned).collect()
        };
        if !is_public_only(&items) {
            let line = usize::try_from(row.line).unwrap_or(1);
            let col = col_1based(reg, row, "handling");
            report.push(Diagnostic::new(
                Code::Fb0410,
                strict,
                path,
                line,
                col,
                format!(
                    "source version `handling` must be `public` in the repository Corpus, got `{raw}`"
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Tree, run};
    use crate::date::CivilDate;

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const MINI_ASSISTANT_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const MINI_CHANGES_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const MINI_SCAN_ALLOW_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/scan-allow.toml");
    const MINI_SOURCES_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const MINI_CLAIMS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const MINI_QUESTIONS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const MINI_CONFLICTS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const MINI_GLOSSARY_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");

    const PAGE_CMP_REVIEW_STEPS: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const PAGE_EST_RATE_WINDOW_BRIEF: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        for dir in [
            "corpus",
            "corpus/pages",
            "corpus/pages/compliance",
            "corpus/pages/compliance/business-systems",
            "corpus/pages/estimating",
            "corpus/pages/estimating/pricing",
            "corpus/registers",
            "corpus/sources",
            "corpus/sources/text",
            "corpus/sources/text/S-0001-v1",
            "corpus/sources/text/S-0002-v1",
            "corpus/sources/text/S-0002-v2",
            "corpus/templates",
            "corpus/eval",
        ] {
            tree.insert_dir(dir);
        }
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes().to_vec());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes().to_vec());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes().to_vec());
        tree.insert_bytes(
            "corpus/scan-allow.toml",
            MINI_SCAN_ALLOW_TOML.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/registers/sources.csv",
            MINI_SOURCES_CSV.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/registers/claims.csv",
            MINI_CLAIMS_CSV.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            PAGE_CMP_REVIEW_STEPS.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            PAGE_CMP_SYSTEM_REVIEW.as_bytes().to_vec(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-window-brief.md",
            PAGE_EST_RATE_WINDOW_BRIEF.as_bytes().to_vec(),
        );
        tree
    }

    fn today() -> CivilDate {
        CivilDate::from_ymd(2026, 2, 15).expect("valid date")
    }

    // control: ZN-01
    #[test]
    fn fb0410_fires_for_every_non_public_label_kind() {
        let base = mini_tree();
        let clean_report = run(&Host::new(base.clone(), today()), &Options::new(true));
        assert!(
            clean_report.diagnostics.is_empty(),
            "expected clean mini tree with handling = [\"public\"], got: {:?}",
            clean_report.diagnostics
        );

        for raw_handling in [
            "handling = []",
            "handling = [\"marked\"]",
            "handling = [\"owner-supplied\"]",
            "handling = [\"np-0007\"]",
            "handling = [\"Np 7\"]",
        ] {
            let mut tree = base.clone();
            let updated =
                PAGE_CMP_REVIEW_STEPS.replacen("handling = [\"public\"]", raw_handling, 1);
            tree.insert_bytes(
                "corpus/pages/compliance/business-systems/cmp-review-steps.md",
                updated.into_bytes(),
            );
            let report = run(&Host::new(tree, today()), &Options::new(true));
            let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
            assert_eq!(
                codes,
                vec![Code::Fb0410],
                "expected `{raw_handling}` to raise FB0410 and nothing else, got: {:?}",
                report.diagnostics
            );
            assert_eq!(
                report.diagnostics[0].path,
                "corpus/pages/compliance/business-systems/cmp-review-steps.md"
            );
            assert_eq!(report.diagnostics[0].line, 11);
        }

        // Source row with an empty handling cell raises FB0410 alone:
        let mut tree = base;
        let updated_sources = MINI_SOURCES_CSV.replacen(
            ",2026-02-01,2026-02-01,active,,V2,public,Synthetic fixture source",
            ",2026-02-01,2026-02-01,active,,V2,,Synthetic fixture source",
            1,
        );
        tree.insert_bytes("corpus/registers/sources.csv", updated_sources.into_bytes());
        let report = run(&Host::new(tree, today()), &Options::new(true));
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(
            codes,
            vec![Code::Fb0410],
            "expected source row with empty handling cell to raise FB0410 alone, got: {:?}",
            report.diagnostics
        );
        assert_eq!(report.diagnostics[0].path, "corpus/registers/sources.csv");
        assert_eq!(report.diagnostics[0].line, 2);
    }

    #[test]
    fn handling_field_location_uses_parser_recorded_line_after_comments_and_reordered_keys() {
        let mut tree = mini_tree();
        let reordered = "\
+++
# leading comment 1
# leading comment 2
title = \"Synthetic procedure for business-system review steps\"
id = \"cmp-review-steps\"
area = \"cmp\"
subarea = \"business-systems\"
type = \"procedure\"
stage = 3
readiness = \"SYNTHETIC-NOW\"
status = \"draft\"
as_of = 2026-02-01
terms = [\"system_review\"]
prereq = [\"cmp-system-review\"]
questions = [\"Q-001\"]
conflicts = [\"XC-01\"]
volatility = \"V2\"
# comment before handling
handling = [\"np-0007\"]
+++

"
        .to_owned()
            + PAGE_CMP_REVIEW_STEPS
                .split_once("+++\n\n")
                .map(|(_, body)| body)
                .expect("body");
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            reordered.into_bytes(),
        );
        let report = run(&Host::new(tree, today()), &Options::new(false));
        let fb0410: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0410)
            .collect();
        assert_eq!(fb0410.len(), 1);
        assert_eq!(fb0410[0].line, 19);
        assert_eq!(fb0410[0].col, 1);
    }
}
