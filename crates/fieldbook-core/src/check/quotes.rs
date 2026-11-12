//! Check pass `quotes`: `FB0304` and `FB0308`
//! (`docs/reference/corpus-format.md#check-catalogue`).
//!
//! **Responsibility.**
//! - `FB0304`: verify that citation locators on claims and pages resolve against the cited source
//!   version's extraction (page numbers within page count, section labels in locators table).
//! - `FB0308`: verify that claim quotes match the extracted text at the claim's locator.
//!
//! **Rules.** Pure: no I/O, no clock, no environment, no process access.

use std::collections::BTreeMap;
use std::ops::RangeInclusive;

use crate::check::registers::register_path;
use crate::check::{Code, Diagnostic, Model, Options, Report, Tree};
use crate::cite::KeyKind;
use crate::extraction::ExtractionToml;
use crate::ids::{ClaimId, VersionId};
use crate::quote;
use crate::schema::Register;
use crate::schema::claims::ClaimRow;
use crate::schema::locator::{Locator, LocatorKind};

/// The parsed extraction.toml of `vid`, or None when the folder or file is absent, not UTF-8 or unparsable.
#[must_use]
pub fn read_extraction(tree: &Tree, vid: &VersionId) -> Option<ExtractionToml> {
    let path = format!("corpus/sources/text/{vid}/extraction.toml");
    let bytes = tree.get_bytes(&path)?;
    let text = std::str::from_utf8(bytes).ok()?;
    ExtractionToml::parse(text).ok()
}

/// The text of pages 1..=range.end() of `vid`, as borrowed strings from the tree: entries before `range.start()` are the empty string "" placeholders (never read), page N is `corpus/sources/text/<vid>/pNNNN.txt`. None when a page file inside `range` is missing or not UTF-8, or when `range` is empty/reversed or its end is above 9_999.
#[must_use]
pub fn read_pages<'t>(
    tree: &'t Tree,
    vid: &VersionId,
    range: RangeInclusive<u32>,
) -> Option<Vec<&'t str>> {
    let start = *range.start();
    let end = *range.end();
    if range.is_empty() || start > end || start < 1 || end > 9_999 {
        return None;
    }
    let total_len = usize::try_from(end).ok()?;
    let mut pages = vec![""; total_len];
    for page_num in start..=end {
        let file_name = crate::extraction::page_file_name(page_num);
        let path = format!("corpus/sources/text/{vid}/{file_name}");
        let bytes = tree.get_bytes(&path)?;
        let text = std::str::from_utf8(bytes).ok()?;
        let idx = usize::try_from(page_num.saturating_sub(1)).ok()?;
        if let Some(slot) = pages.get_mut(idx) {
            *slot = text;
        }
    }
    Some(pages)
}

fn col_1based(model: &Model, reg: Register, line: u32, col_name: &str) -> usize {
    let idx = reg
        .columns()
        .iter()
        .position(|c| c.name == col_name)
        .unwrap_or(0);
    model
        .register_rows(reg)
        .iter()
        .find(|r| r.line == line)
        .and_then(|r| r.cols.get(idx).copied())
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(1)
}

fn claim_location(model: &Model, claim_id: &ClaimId, col_name: &str) -> (usize, usize) {
    if let Some(located) = model
        .claims()
        .iter()
        .find(|r| &r.value.claim_id == claim_id)
    {
        let line = usize::try_from(located.line).unwrap_or(1);
        let col = col_1based(model, Register::Claims, located.line, col_name);
        (line, col)
    } else {
        let rows = model.register_rows(Register::Claims);
        let fallback_line = rows.last().map_or(2, |r| r.line.saturating_add(1));
        let line = usize::try_from(fallback_line).unwrap_or(2);
        (line, 1)
    }
}

/// The one diagnostic the pass raises for `claim`, or None.
#[must_use]
pub fn match_claim(model: &Model, claim: &ClaimRow) -> Option<Diagnostic> {
    let sid = claim.source_id.as_ref()?;
    let ver = claim.version?;
    let locator = claim.locator.as_deref().filter(|s| !s.is_empty())?;
    if matches!(
        claim.locator_kind,
        Some(LocatorKind::NoFineLocator | LocatorKind::RecordField)
    ) {
        return None;
    }
    let vid = VersionId::parse(&format!("{sid}-v{ver}")).ok()?;

    let extraction = read_extraction(model.tree(), &vid)?;

    let range = match quote::locator_resolves(&extraction, locator) {
        Err(code) => {
            let reason = match Locator::parse(locator) {
                Ok(Locator::Page(_) | Locator::PageRange { .. }) => {
                    format!("the page is beyond its {} pages", extraction.page_count)
                }
                _ => "the label is not in its [locators]".to_string(),
            };
            let message = format!(
                "locator `{locator}` does not resolve in the extraction of `{vid}`: {reason}"
            );
            let (line, col) = claim_location(model, &claim.claim_id, "locator");
            return Some(Diagnostic::new(
                code,
                false,
                register_path(Register::Claims),
                line,
                col,
                message,
            ));
        }
        Ok(None) => return None,
        Ok(Some(r)) => r,
    };

    let quote_str = match claim.quote.as_deref() {
        Some(q) if !quote::normalise(q).is_empty() => q,
        _ => return None,
    };

    let pages = read_pages(model.tree(), &vid, range.clone())?;

    if quote::find(quote_str, &pages, range.clone()).is_some() {
        return None;
    }

    if range.clone().all(|p| extraction.thin_pages.contains(&p)) {
        return None;
    }

    let (line, col) = claim_location(model, &claim.claim_id, "quote");
    let message = format!(
        "the quote of claim `{}` is not found in the extracted text of `{vid}` at `{locator}`",
        claim.claim_id
    );
    Some(Diagnostic::new(
        Code::Fb0308,
        false,
        register_path(Register::Claims),
        line,
        col,
        message,
    ))
}

/// The pass.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    for claim in model.claims() {
        if let Some(diag) = match_claim(model, &claim.value) {
            report.push(diag);
        }
    }

    let mut extractions: BTreeMap<VersionId, Option<ExtractionToml>> = BTreeMap::new();
    for page in model.pages() {
        for cite in model.citations(&page.id) {
            if cite.key_kind != KeyKind::Version {
                continue;
            }
            let Some(loc) = &cite.locator else {
                continue;
            };
            let Ok(vid) = VersionId::parse(&cite.key) else {
                continue;
            };
            let extraction = extractions
                .entry(vid.clone())
                .or_insert_with(|| read_extraction(model.tree(), &vid));
            let Some(extraction) = extraction.as_ref() else {
                continue;
            };
            if let Err(code) = quote::locator_resolves(extraction, &loc.text) {
                let reason = match Locator::parse(&loc.text) {
                    Ok(Locator::Page(_) | Locator::PageRange { .. }) => {
                        format!("the page is beyond its {} pages", extraction.page_count)
                    }
                    _ => "the label is not in its [locators]".to_string(),
                };
                let message = format!(
                    "citation `{}` does not resolve in the extraction of `{vid}`: {reason}",
                    cite.to_canonical_bracketed_string()
                );
                report.push(Diagnostic::new(
                    code,
                    options.strict,
                    &page.path,
                    cite.span.line,
                    cite.span.col,
                    message,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::check::{Code, Host, Options, Severity, Tree, load};
    use crate::date::CivilDate;
    use crate::extraction::{Extraction, LocatorPages, PageText, build_record};
    use crate::ids::VersionId;
    use crate::schema::locator::ExtensionResolution;

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const MINI_ASSISTANT_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const MINI_CHANGES_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/changes.md");
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
    const MINI_S0002_V2_TXT: &[u8] = include_bytes!(
        "../../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt"
    );
    const MINI_S0001_V1_TXT: &[u8] = include_bytes!(
        "../../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt"
    );
    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const PATH_CMP_SYSTEM_REVIEW: &str =
        "corpus/pages/compliance/business-systems/cmp-system-review.md";

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", MINI_S0002_V2_TXT);
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", MINI_S0001_V1_TXT);
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, PAGE_CMP_SYSTEM_REVIEW.as_bytes());
        tree
    }

    fn tree_with_extraction(pages: &[&str], locators: &[(&str, u32, u32)]) -> Tree {
        let mut tree = mini_tree();
        tree.insert_dir("corpus/sources/text");
        tree.insert_dir("corpus/sources/text/S-0001-v1");

        let page_texts: Vec<PageText> = pages
            .iter()
            .enumerate()
            .map(|(idx, &text)| PageText {
                number: u32::try_from(idx + 1).expect("valid page number"),
                text: text.to_owned(),
            })
            .collect();
        let extraction = Extraction {
            method: "plain",
            pages: page_texts,
        };
        let mut kept_locators = BTreeMap::new();
        for &(label, first, last) in locators {
            kept_locators.insert(label.to_owned(), LocatorPages { first, last });
        }
        let record = build_record(
            &extraction,
            "plain",
            "1",
            "0fba4a465337f7383e693c1348ff428c4ce68dbfc6f6a3b5b64014d35d0eb73d",
            &kept_locators,
        );
        let rendered = record.render();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        for (idx, &text) in pages.iter().enumerate() {
            let page_num = u32::try_from(idx + 1).expect("valid page number");
            let file_name = crate::extraction::page_file_name(page_num);
            let path = format!("corpus/sources/text/S-0001-v1/{file_name}");
            tree.insert_bytes(path, text.as_bytes());
        }
        tree
    }

    fn tree_with_both_sections() -> Tree {
        tree_with_extraction(
            &[
                "Section 1: Synthetic compliance reviews occur every 30 days.\nSection 2: Synthetic withholding threshold is 5 percent.",
            ],
            &[("1", 1, 1), ("2", 1, 1)],
        )
    }

    fn claims_csv_with_cl_cmp_0001(
        locator_kind: &str,
        locator: &str,
        quote: &str,
        consequential: bool,
    ) -> String {
        let quote_cell = if quote.contains('"') {
            format!("\"{}\"", quote.replace('"', "\"\""))
        } else {
            quote.to_owned()
        };
        MINI_CLAIMS_CSV
            .replace(
                ",text_section,sec. 1,Synthetic compliance reviews occur every 30 days.,true,",
                &format!(",{locator_kind},{locator},{quote_cell},{consequential},"),
            )
            .replace(
                "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,text_section,sec. 2,",
                "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0002,2,text_section,sec. 2,",
            )
    }

    fn claims_csv_with_locator_and_quote(locator_kind: &str, locator: &str, quote: &str) -> String {
        claims_csv_with_cl_cmp_0001(locator_kind, locator, quote, true)
    }

    #[test]
    fn locator_beyond_page_count_is_fb0304() {
        // (1) Page locator beyond page_count
        let mut tree = tree_with_extraction(&["p1", "p2", "p3"], &[]);
        let claims = claims_csv_with_locator_and_quote(
            "pdf_page_box",
            "p. 4",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, Code::Fb0304);
        assert_eq!(diags[0].severity, Severity::Error);
        assert_eq!(diags[0].path, "corpus/registers/claims.csv");
        assert_eq!(diags[0].line, 2);
        let expected_col = col_1based(&model, Register::Claims, 2, "locator");
        assert_eq!(diags[0].col, expected_col);
        assert!(
            diags[0]
                .message
                .contains("locator `p. 4` does not resolve in the extraction of `S-0001-v1`: the page is beyond its 3 pages")
        );

        // (2) Section label not in [locators]
        let mut tree2 = tree_with_extraction(&["p1", "p2", "p3"], &[("1", 1, 1)]);
        let claims2 = claims_csv_with_locator_and_quote(
            "text_section",
            "sec. 99",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree2.insert_bytes("corpus/registers/claims.csv", claims2.as_bytes());
        let (model2, report2) = load(&Host::new(tree2, sample_today()), &Options::new(false));
        let diags2: Vec<_> = report2
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert_eq!(diags2.len(), 1);
        assert_eq!(diags2[0].code, Code::Fb0304);
        assert_eq!(diags2[0].severity, Severity::Error);
        assert_eq!(diags2[0].line, 2);
        let expected_col2 = col_1based(&model2, Register::Claims, 2, "locator");
        assert_eq!(diags2[0].col, expected_col2);
        assert!(
            diags2[0]
                .message
                .contains("locator `sec. 99` does not resolve in the extraction of `S-0001-v1`: the label is not in its [locators]")
        );
    }

    #[test]
    fn quote_not_found_is_fb0308_warning() {
        let mut tree = tree_with_extraction(
            &["Synthetic compliance reviews occur every 30 days."],
            &[("1", 1, 1)],
        );
        let claims = claims_csv_with_locator_and_quote(
            "text_section",
            "sec. 1",
            "Synthetic compliance reviews occur every 45 days.",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::new(true));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, Code::Fb0308);
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(diags[0].path, "corpus/registers/claims.csv");
        assert_eq!(diags[0].line, 2);
        let expected_col = col_1based(&model, Register::Claims, 2, "quote");
        assert_eq!(diags[0].col, expected_col);
        assert_eq!(
            diags[0].message,
            "the quote of claim `CL-CMP-0001` is not found in the extracted text of `S-0001-v1` at `sec. 1`"
        );
    }

    #[test]
    fn matching_quote_is_clean() {
        let mut tree = tree_with_extraction(
            &["Synthetic \u{201C}compliance\u{201D} reviews occur every 30 days."],
            &[("1", 1, 1)],
        );
        let claims = claims_csv_with_locator_and_quote(
            "text_section",
            "sec. 1",
            "Synthetic \"compliance\" reviews occur every 30 days.",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let (_, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert!(diags.is_empty(), "expected clean, got: {diags:?}");
    }

    #[test]
    fn version_without_extraction_is_skipped() {
        let mut tree = tree_with_extraction(&["Text for S-0001-v1"], &[("1", 1, 1)]);
        // Claim on S-0002-v2 (which has no extraction) with locator p. 400
        let claims =
            claims_csv_with_cl_cmp_0001("text_section", "sec. 1", "Text for S-0001-v1", false)
                .replace(
                    "S-0002,2,text_section,sec. 2,",
                    "S-0002,2,pdf_page_box,p. 400,",
                );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let (_, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert!(diags.is_empty(), "expected clean, got: {diags:?}");

        // Table locators on extracted version
        for (kind, loc) in [("record_key", "rec 7"), ("sheet_cells", "sheet Data!A1")] {
            let mut t = tree_with_extraction(&["Some text"], &[]);
            let c = claims_csv_with_locator_and_quote(
                kind,
                loc,
                "Synthetic compliance reviews occur every 30 days.",
            );
            t.insert_bytes("corpus/registers/claims.csv", c.as_bytes());
            let (_, rep) = load(&Host::new(t, sample_today()), &Options::new(false));
            let d: Vec<_> = rep
                .diagnostics
                .iter()
                .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
                .collect();
            assert!(
                d.is_empty(),
                "expected clean for {kind} / {loc}, got: {d:?}"
            );
        }
    }

    #[test]
    fn quote_only_outside_locator_is_fb0308() {
        let mut tree = tree_with_extraction(
            &[
                "Synthetic compliance reviews occur every 30 days.",
                "Page 2 without the target quote, containing sufficient text so this page is not thin.",
                "Page 3 without the target quote, containing sufficient text so this page is not thin.",
            ],
            &[],
        );
        let claims = claims_csv_with_locator_and_quote(
            "pdf_page_box",
            "p. 2",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let (_, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, Code::Fb0308);
    }

    #[test]
    fn quote_repeated_inside_and_outside_matches_inside() {
        // (1) Same quote on page 1 and page 3, locator p. 3 -> matches inside
        let mut tree1 = tree_with_extraction(
            &[
                "Synthetic compliance reviews occur every 30 days.",
                "Page 2 filler text, long enough so that this page is definitely not classified as thin.",
                "Synthetic compliance reviews occur every 30 days. Plenty of extra characters.",
            ],
            &[],
        );
        let claims1 = claims_csv_with_locator_and_quote(
            "pdf_page_box",
            "p. 3",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree1.insert_bytes("corpus/registers/claims.csv", claims1.as_bytes());
        let (_, report1) = load(&Host::new(tree1, sample_today()), &Options::new(false));
        let diags1: Vec<_> = report1
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert!(diags1.is_empty(), "expected clean match on p.3: {diags1:?}");

        // (2) Page 3 text is changed -> Fb0308
        let mut tree2 = tree_with_extraction(
            &[
                "Synthetic compliance reviews occur every 30 days.",
                "Page 2 filler text, long enough so that this page is definitely not classified as thin.",
                "Changed text on page 3, long enough so that this page is definitely not classified as thin.",
            ],
            &[],
        );
        let claims2 = claims_csv_with_locator_and_quote(
            "pdf_page_box",
            "p. 3",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree2.insert_bytes("corpus/registers/claims.csv", claims2.as_bytes());
        let (_, report2) = load(&Host::new(tree2, sample_today()), &Options::new(false));
        let diags2: Vec<_> = report2
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert_eq!(diags2.len(), 1);
        assert_eq!(diags2[0].code, Code::Fb0308);
    }

    #[test]
    fn section_range_limits_the_match() {
        // label `15.404-1` mapped to pages [2, 3]
        let locators = [("15.404-1", 2, 3)];

        // (1) Quote on page 3 only -> clean
        let mut tree1 = tree_with_extraction(
            &[
                "Page 1 filler text, long enough so that this page is definitely not classified as thin.",
                "Page 2 filler text, long enough so that this page is definitely not classified as thin.",
                "Synthetic compliance reviews occur every 30 days. Plenty of extra characters.",
            ],
            &locators,
        );
        let claims1 = claims_csv_with_locator_and_quote(
            "text_section",
            "sec. 15.404-1",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree1.insert_bytes("corpus/registers/claims.csv", claims1.as_bytes());
        let (_, report1) = load(&Host::new(tree1, sample_today()), &Options::new(false));
        let diags1: Vec<_> = report1
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert!(
            diags1.is_empty(),
            "expected clean match on sec. 15.404-1: {diags1:?}"
        );

        // (2) Quote on page 1 only -> Fb0308
        let mut tree2 = tree_with_extraction(
            &[
                "Synthetic compliance reviews occur every 30 days. Plenty of extra characters.",
                "Page 2 filler text, long enough so that this page is definitely not classified as thin.",
                "Page 3 filler text, long enough so that this page is definitely not classified as thin.",
            ],
            &locators,
        );
        let claims2 = claims_csv_with_locator_and_quote(
            "text_section",
            "sec. 15.404-1",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree2.insert_bytes("corpus/registers/claims.csv", claims2.as_bytes());
        let (_, report2) = load(&Host::new(tree2, sample_today()), &Options::new(false));
        let diags2: Vec<_> = report2
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert_eq!(diags2.len(), 1);
        assert_eq!(diags2[0].code, Code::Fb0308);
    }

    #[test]
    fn empty_optional_quote_is_not_a_mismatch() {
        // An empty cell and a quote that normalises to nothing (a lone no-break space) both
        // carry no text to match, so neither is a mismatch.
        for quote in ["", "\u{a0}"] {
            let mut tree = tree_with_extraction(
                &["Synthetic page text with enough characters to not be thin."],
                &[("1", 1, 1)],
            );
            let claims = claims_csv_with_cl_cmp_0001("text_section", "sec. 1", quote, false);
            tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
            let (_, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
            let diags: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
                .collect();
            assert!(
                diags.is_empty(),
                "expected no mismatch for empty quote {quote:?}: {diags:?}"
            );
        }
    }

    #[test]
    fn page_citation_locator_resolves() {
        let mut tree = tree_with_both_sections();
        let page = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Synthetic reviewers check control operation and record open items before billing runs.",
            "Synthetic reviewers check control operation and record open items before billing runs. [@S-0001-v1, p. 1]",
        );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, page.as_bytes());
        let (_, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert!(diags.is_empty(), "expected clean page citation: {diags:?}");
    }

    #[test]
    fn page_citation_out_of_range_is_fb0304() {
        let mut tree = tree_with_both_sections();
        let orig_line = "Synthetic reviewers check control operation and record open items before billing runs.";
        let appended = " [@S-0001-v1, p. 4]";
        let page = PAGE_CMP_SYSTEM_REVIEW.replace(orig_line, &format!("{orig_line}{appended}"));
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, page.as_bytes());
        let (_, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, Code::Fb0304);
        assert_eq!(diags[0].path, PATH_CMP_SYSTEM_REVIEW);
        assert_eq!(diags[0].line, 28);
        // Column of `[`: orig_line chars (85) + 1 space = 86 chars before `[`, so col = 87
        let expected_col = orig_line.chars().count() + 2;
        assert_eq!(diags[0].col, expected_col);
    }

    #[test]
    fn page_citation_without_extraction_is_unavailable() {
        let mut tree = tree_with_both_sections();
        let page = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Synthetic reviewers check control operation and record open items before billing runs.",
            "Synthetic reviewers check control operation and record open items before billing runs. [@S-0002-v2, p. 400]",
        );
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, page.as_bytes());
        let (_, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308)
            .collect();
        assert!(
            diags.is_empty(),
            "expected unavailable citation to be skipped: {diags:?}"
        );
    }

    #[test]
    fn unsupported_extension_is_unavailable() {
        // (a) claim row with ext:structured_path and ext structured_path %2Froot%2Fitem on S-0001-v1
        let mut tree = tree_with_both_sections();
        let ext_unchecked = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,text_section,sec. 2,",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,ext:structured_path,ext structured_path %2Froot%2Fitem,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", ext_unchecked.as_bytes());
        let (model, report) = load(&Host::new(tree, sample_today()), &Options::new(false));
        let diags: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308 || d.code == Code::Fb0300)
            .collect();
        assert!(
            diags.is_empty(),
            "expected no FB0304/FB0308/FB0300: {diags:?}"
        );

        let claim_u = &model
            .claims()
            .iter()
            .find(|c| c.value.claim_id.as_str() == "CL-CMP-0002")
            .expect("claim CL-CMP-0002")
            .value;
        assert!(match_claim(&model, claim_u).is_none());

        let vid = VersionId::parse("S-0001-v1").expect("valid vid");
        let extraction = read_extraction(model.tree(), &vid).expect("extraction");
        assert_eq!(
            quote::locator_resolves(&extraction, "ext structured_path %2Froot%2Fitem"),
            Ok(None)
        );
        assert_eq!(
            quote::unsupported_extension("ext structured_path %2Froot%2Fitem"),
            Some(ExtensionResolution::Unsupported {
                kind: "structured_path".to_owned(),
                payload: "/root/item".to_owned(),
            })
        );

        // (b) the same locator in a page-body citation [@S-0001-v1, ext structured_path %2Froot%2Fitem]
        let mut tree_b = tree_with_both_sections();
        let page_b = PAGE_CMP_SYSTEM_REVIEW.replace(
            "Synthetic reviewers check control operation and record open items before billing runs.",
            "Synthetic reviewers check control operation and record open items before billing runs. [@S-0001-v1, ext structured_path %2Froot%2Fitem]",
        );
        tree_b.insert_bytes(PATH_CMP_SYSTEM_REVIEW, page_b.as_bytes());
        let (_, report_b) = load(&Host::new(tree_b, sample_today()), &Options::new(false));
        let diags_b: Vec<_> = report_b
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0304 || d.code == Code::Fb0308 || d.code == Code::Fb0300)
            .collect();
        assert!(
            diags_b.is_empty(),
            "expected no FB0304/FB0308/FB0300 in citation: {diags_b:?}"
        );

        // (c) a SUPPORTED claim with that locator fails the promotion requirement: FB0209 is raised
        let mut tree_c = tree_with_both_sections();
        let ext_supported = MINI_CLAIMS_CSV.replace(
            "S-0001,1,text_section,sec. 1,",
            "S-0001,1,ext:structured_path,ext structured_path %2Froot%2Fitem,",
        );
        tree_c.insert_bytes("corpus/registers/claims.csv", ext_supported.as_bytes());
        let (_, report_c) = load(&Host::new(tree_c, sample_today()), &Options::new(false));
        let codes: Vec<_> = report_c.diagnostics.iter().map(|d| d.code).collect();
        assert!(
            codes.contains(&Code::Fb0209),
            "expected FB0209 in {codes:?}"
        );
    }

    #[test]
    fn match_claim_returns_the_one_diagnostic() {
        // Bad locator
        let mut tree_loc = tree_with_extraction(&["p1", "p2"], &[]);
        let claims_loc = claims_csv_with_locator_and_quote(
            "pdf_page_box",
            "p. 4",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree_loc.insert_bytes("corpus/registers/claims.csv", claims_loc.as_bytes());
        let (model_loc, report_loc) =
            load(&Host::new(tree_loc, sample_today()), &Options::new(false));
        let claim_loc = &model_loc.claims()[0].value;
        let d_direct_loc = match_claim(&model_loc, claim_loc).expect("diagnostic from match_claim");
        let d_from_check_loc = report_loc
            .diagnostics
            .iter()
            .find(|d| d.code == Code::Fb0304)
            .expect("check reported FB0304");
        assert_eq!(d_direct_loc.code, d_from_check_loc.code);
        assert_eq!(d_direct_loc.path, d_from_check_loc.path);
        assert_eq!(d_direct_loc.line, d_from_check_loc.line);
        assert_eq!(d_direct_loc.col, d_from_check_loc.col);
        assert_eq!(d_direct_loc.message, d_from_check_loc.message);

        // Bad quote
        let mut tree_quote = tree_with_extraction(
            &["Synthetic compliance reviews occur every 30 days."],
            &[("1", 1, 1)],
        );
        let claims_quote = claims_csv_with_locator_and_quote(
            "text_section",
            "sec. 1",
            "Synthetic compliance reviews occur every 99 days.",
        );
        tree_quote.insert_bytes("corpus/registers/claims.csv", claims_quote.as_bytes());
        let (model_quote, report_quote) =
            load(&Host::new(tree_quote, sample_today()), &Options::new(false));
        let claim_quote = &model_quote.claims()[0].value;
        let d_direct_quote =
            match_claim(&model_quote, claim_quote).expect("diagnostic from match_claim");
        let d_from_check_quote = report_quote
            .diagnostics
            .iter()
            .find(|d| d.code == Code::Fb0308)
            .expect("check reported FB0308");
        assert_eq!(d_direct_quote.code, d_from_check_quote.code);
        assert_eq!(d_direct_quote.path, d_from_check_quote.path);
        assert_eq!(d_direct_quote.line, d_from_check_quote.line);
        assert_eq!(d_direct_quote.col, d_from_check_quote.col);
        assert_eq!(d_direct_quote.message, d_from_check_quote.message);

        // Clean claim
        let mut tree_clean = tree_with_extraction(
            &["Synthetic compliance reviews occur every 30 days."],
            &[("1", 1, 1)],
        );
        let claims_clean = claims_csv_with_locator_and_quote(
            "text_section",
            "sec. 1",
            "Synthetic compliance reviews occur every 30 days.",
        );
        tree_clean.insert_bytes("corpus/registers/claims.csv", claims_clean.as_bytes());
        let (model_clean, report_clean) =
            load(&Host::new(tree_clean, sample_today()), &Options::new(false));
        let claim_clean = &model_clean.claims()[0].value;
        assert!(match_claim(&model_clean, claim_clean).is_none());
        assert!(
            !report_clean
                .diagnostics
                .iter()
                .any(|d| (d.code == Code::Fb0304 || d.code == Code::Fb0308) && d.line == 2)
        );
    }
}
