//! Check pass `pages`: `FB0101` to `FB0109`, `FB0114` and `FB0116` on Corpus page front matter
//! (`docs/reference/corpus-format.md#page-files`, `#01-pages-corpustoml-and-files`).
//!
//! **Responsibility.**
//! - `FB0101`: missing or unbalanced `+++` fences, or missing blank line after the closing fence.
//! - `FB0102`: TOML syntax error, unknown front-matter key, or wrong TOML value type.
//! - `FB0103`: missing required front-matter key.
//! - `FB0104`: front-matter value outside its vocabulary or form (other than `id`, `area`,
//!   `subarea`, `type`, `handling`, or calendar dates).
//! - `FB0105`: `id` is not in page-id form, differs from the file stem, or duplicates an earlier
//!   page's `id` (reported at the second page, naming the first).
//! - `FB0106`: `area` is not an area code in `corpus.toml`, or differs from the `id`'s first
//!   segment or the area folder.
//! - `FB0107`: `subarea` is not one of the area's declared `subareas` or differs from the subarea
//!   folder.
//! - `FB0108`: `type` is not a v1 page type.
//! - `FB0109`: computed field (`claims` or `review_by`) is written in front matter.
//! - `FB0114`: a front-matter date (`as_of` or `review_by`) is not a real calendar date or is
//!   partial where a full `YYYY-MM-DD` date is required.
//! - `FB0116`: `as_of` is after [`Model::today`](super::Model::today) (warning).

use std::collections::BTreeMap;

use crate::ids::PageId;
use crate::schema::CorpusToml;
use crate::schema::page::{MetaError, MetaErrorKind, PageMeta};

use super::model::{RawPageOutcome, RawPageRecord};
use super::{Code, Diagnostic, Model, Options, Report};

/// Run the `pages` check pass (`FB0101`..`FB0109`, `FB0114`, `FB0116`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    let Some(corpus) = model.corpus() else {
        return;
    };

    let strict = options.strict;
    let mut seen_ids: BTreeMap<PageId, String> = BTreeMap::new();

    for page in model.raw_pages() {
        match &page.outcome {
            RawPageOutcome::SplitErr(err) => {
                report.push(Diagnostic::new(
                    Code::Fb0101,
                    strict,
                    &page.path,
                    err.line() as usize,
                    err.col() as usize,
                    err.to_string(),
                ));
            }
            RawPageOutcome::MetaErr(err) => {
                let code = map_meta_error_code(err);
                report.push(Diagnostic::new(
                    code,
                    strict,
                    &page.path,
                    err.line() as usize,
                    err.col() as usize,
                    err.message(),
                ));
            }
            RawPageOutcome::Parsed(meta) => {
                check_parsed_page(model, corpus, page, meta, &mut seen_ids, strict, report);
            }
        }
    }
}

fn map_meta_error_code(err: &MetaError) -> Code {
    match err.kind() {
        MetaErrorKind::CalendarDate => Code::Fb0114,
        MetaErrorKind::Syntax | MetaErrorKind::UnknownField | MetaErrorKind::WrongType => {
            Code::Fb0102
        }
        MetaErrorKind::MissingField => Code::Fb0103,
        MetaErrorKind::Value => match err.field() {
            Some("id") => Code::Fb0105,
            Some("area") => Code::Fb0106,
            Some("subarea") => Code::Fb0107,
            Some("type") => Code::Fb0108,
            _ => Code::Fb0104,
        },
    }
}

fn field_loc(meta: &PageMeta, key: &str) -> (usize, usize) {
    meta.field_location(key)
        .map_or((1, 1), |(line, col)| (line as usize, col as usize))
}

fn check_parsed_page(
    model: &Model,
    corpus: &CorpusToml,
    page: &RawPageRecord,
    meta: &PageMeta,
    seen_ids: &mut BTreeMap<PageId, String>,
    strict: bool,
    report: &mut Report,
) {
    // FB0105: `id` differs from the file stem, or duplicates an earlier page's `id`.
    if let Some(first_path) = seen_ids.get(&meta.id) {
        let (line, col) = field_loc(meta, "id");
        report.push(Diagnostic::new(
            Code::Fb0105,
            strict,
            &page.path,
            line,
            col,
            format!(
                "duplicate page id `{}` (first defined in `{first_path}`)",
                meta.id
            ),
        ));
    } else if meta.id.as_str() != page.stem {
        let (line, col) = field_loc(meta, "id");
        report.push(Diagnostic::new(
            Code::Fb0105,
            strict,
            &page.path,
            line,
            col,
            format!(
                "page id `{}` does not match file stem `{}`",
                meta.id, page.stem
            ),
        ));
    } else {
        seen_ids.insert(meta.id.clone(), page.path.clone());
    }

    // FB0106: `area` against `corpus.toml` area codes, `id`'s first segment, and the area folder.
    let declared_area = corpus.areas().iter().find(|a| a.code == meta.area.as_str());
    let id_prefix = meta.id.as_str().split('-').next().unwrap_or("");
    let area_matches_folder = declared_area.is_some_and(|a| a.slug == page.area_folder);
    let area_matches_id = meta.area.as_str() == id_prefix;
    if !area_matches_folder || !area_matches_id {
        let (line, col) = field_loc(meta, "area");
        report.push(Diagnostic::new(
            Code::Fb0106,
            strict,
            &page.path,
            line,
            col,
            format!(
                "page `area = \"{}\"` does not match `corpus.toml`, id `{}` or area folder `{}`",
                meta.area, meta.id, page.area_folder
            ),
        ));
    }

    // FB0107: `subarea` against that area's subareas and the subarea folder.
    let folder_area = corpus
        .areas()
        .iter()
        .find(|a| a.slug == page.area_folder)
        .or(declared_area);
    let subarea_in_area =
        folder_area.is_some_and(|a| a.subareas.iter().any(|s| s == &meta.subarea));
    let subarea_matches_folder = meta.subarea == page.subarea_folder;
    if !subarea_in_area || !subarea_matches_folder {
        let (line, col) = field_loc(meta, "subarea");
        report.push(Diagnostic::new(
            Code::Fb0107,
            strict,
            &page.path,
            line,
            col,
            format!(
                "page `subarea = \"{}\"` does not match declared subarea folder `{}`",
                meta.subarea, page.subarea_folder
            ),
        ));
    }

    // FB0109: computed field (`claims` or `review_by`) is written.
    if meta.claims.is_some() {
        let (line, col) = field_loc(meta, "claims");
        report.push(Diagnostic::new(
            Code::Fb0109,
            strict,
            &page.path,
            line,
            col,
            "computed field `claims` must not be written in front matter",
        ));
    }
    if meta.review_by.is_some() {
        let (line, col) = field_loc(meta, "review_by");
        report.push(Diagnostic::new(
            Code::Fb0109,
            strict,
            &page.path,
            line,
            col,
            "computed field `review_by` must not be written in front matter",
        ));
    }

    // FB0116: `as_of` after `Model::today()` (warning).
    let today = model.today();
    if meta.as_of > today {
        let (line, col) = field_loc(meta, "as_of");
        report.push(Diagnostic::new(
            Code::Fb0116,
            strict,
            &page.path,
            line,
            col,
            format!(
                "page `as_of` ({}) is later than today ({today})",
                meta.as_of
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Severity, Tree, load, run};
    use crate::date::CivilDate;
    use crate::ids::PageId;

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

    const PAGE_CMP_REVIEW_STEPS: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const PAGE_CMP_SYSTEM_REVIEW: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const PAGE_EST_RATE_WINDOW_BRIEF: &str = include_str!(
        "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    const PATH_CMP_REVIEW_STEPS: &str =
        "corpus/pages/compliance/business-systems/cmp-review-steps.md";
    const PATH_CMP_SYSTEM_REVIEW: &str =
        "corpus/pages/compliance/business-systems/cmp-system-review.md";
    const PATH_EST_RATE_WINDOW_BRIEF: &str =
        "corpus/pages/estimating/pricing/est-rate-window-brief.md";

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating");
        tree.insert_dir("corpus/pages/estimating/pricing");
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
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, PAGE_CMP_REVIEW_STEPS.as_bytes());
        tree.insert_bytes(PATH_CMP_SYSTEM_REVIEW, PAGE_CMP_SYSTEM_REVIEW.as_bytes());
        tree.insert_bytes(
            PATH_EST_RATE_WINDOW_BRIEF,
            PAGE_EST_RATE_WINDOW_BRIEF.as_bytes(),
        );
        tree
    }

    #[test]
    fn page_accessor_keeps_body_positions_and_skips_bad_meta() {
        let mut tree = mini_tree();
        // Add a valid page with leading comments so its body starts at line 20
        let custom_valid = PAGE_CMP_SYSTEM_REVIEW.replace(
            "+++\nid = \"cmp-system-review\"",
            "+++\n# comment 1\n# comment 2\nid = \"cmp-extra-valid\"",
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-extra-valid.md",
            custom_valid.as_bytes(),
        );

        // Add a page whose front matter fails `parse_meta` (FB0104)
        let bad_meta = PAGE_CMP_SYSTEM_REVIEW
            .replace("id = \"cmp-system-review\"", "id = \"cmp-bad-meta\"")
            .replace("status = \"draft\"", "status = \"final\"");
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-bad-meta.md",
            bad_meta.as_bytes(),
        );

        // Add a page whose fences fail `split` (FB0101)
        let bad_fence = PAGE_CMP_SYSTEM_REVIEW.replacen("+++\n", "---\n", 1);
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-bad-fence.md",
            bad_fence.as_bytes(),
        );

        let (model, report) = load(&Host::new(tree, sample_today()), &Options::new(false));

        // Only the 4 valid pages are in `model.pages()`, sorted by `id`
        let ids: Vec<&str> = model.pages().iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "cmp-extra-valid",
                "cmp-review-steps",
                "cmp-system-review",
                "est-rate-window-brief",
            ]
        );
        assert_eq!(model.pages()[0].body_line, 20);
        assert!(model.pages()[0].body.starts_with("## Purpose\n"));
        assert_eq!(model.pages()[1].body_line, 18);
        assert_eq!(model.pages()[2].body_line, 18);
        assert_eq!(model.pages()[3].body_line, 18);

        // `Index` holds only the parsed page ids
        assert_eq!(model.index().pages.len(), 4);
        assert!(
            model
                .index()
                .contains_page(&PageId::parse("cmp-extra-valid").unwrap())
        );
        assert!(
            !model
                .index()
                .contains_page(&PageId::parse("cmp-bad-meta").unwrap())
        );

        // The two failed pages remain in `report`
        let codes: Vec<Code> = report.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, vec![Code::Fb0101, Code::Fb0104]);
    }

    #[test]
    fn fb0101_missing_or_unbalanced_fences() {
        let mut tree = mini_tree();
        let bad = PAGE_CMP_REVIEW_STEPS.replacen("+++\n", "---\n", 1);
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0101);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 1);
        assert_eq!(report.diagnostics[0].col, 1);
    }

    #[test]
    fn fb0102_unknown_field_wrong_type_and_syntax() {
        // Unknown field: `titel = "..."`
        let mut tree = mini_tree();
        let bad_unknown = PAGE_CMP_REVIEW_STEPS.replace(
            "volatility = \"V2\"\n+++",
            "volatility = \"V2\"\ntitel = \"typo\"\n+++",
        );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_unknown.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0102);
        assert_eq!(report.diagnostics[0].line, 16);

        // Wrong type: `stage = "3"`
        let mut tree_type = mini_tree();
        let bad_type = PAGE_CMP_REVIEW_STEPS.replace("stage = 3", "stage = \"3\"");
        tree_type.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_type.as_bytes());
        let report_type = run(&Host::new(tree_type, sample_today()), &Options::new(false));
        assert_eq!(report_type.diagnostics.len(), 1);
        assert_eq!(report_type.diagnostics[0].code, Code::Fb0102);
        assert_eq!(report_type.diagnostics[0].line, 7);

        // Wrong type on `handling`: `handling = "public"`
        let mut tree_handling = mini_tree();
        let bad_handling =
            PAGE_CMP_REVIEW_STEPS.replace("handling = [\"public\"]", "handling = \"public\"");
        tree_handling.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_handling.as_bytes());
        let report_handling = run(
            &Host::new(tree_handling, sample_today()),
            &Options::new(false),
        );
        assert_eq!(report_handling.diagnostics.len(), 1);
        assert_eq!(report_handling.diagnostics[0].code, Code::Fb0102);
        assert_eq!(report_handling.diagnostics[0].line, 11);
    }

    #[test]
    fn fb0103_missing_required_field() {
        let mut tree = mini_tree();
        let bad = PAGE_CMP_REVIEW_STEPS.replace("as_of = 2026-02-01\n", "");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0103);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
    }

    #[test]
    fn fb0104_vocabulary_or_form_and_handling_values_deferred_to_fb0410() {
        let mut tree = mini_tree();
        let bad = PAGE_CMP_REVIEW_STEPS.replace("status = \"draft\"", "status = \"final\"");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0104);
        assert_eq!(report.diagnostics[0].line, 9);

        // `handling = ["np-0007"]` parses cleanly in `pages` (FB0410 in `check-scans` judges values)
        let mut tree_handling = mini_tree();
        let non_public =
            PAGE_CMP_REVIEW_STEPS.replace("handling = [\"public\"]", "handling = [\"np-0007\"]");
        tree_handling.insert_bytes(PATH_CMP_REVIEW_STEPS, non_public.as_bytes());
        let report_handling = run(
            &Host::new(tree_handling, sample_today()),
            &Options::new(false),
        );
        assert!(
            report_handling.diagnostics.is_empty(),
            "expected handling values to be deferred to check-scans: {:?}",
            report_handling.diagnostics
        );
    }

    #[test]
    fn fb0105_id_form_stem_mismatch_and_duplicate_id() {
        // Stem mismatch: `cmp-review-steps.md` with `id = "cmp-review-step"`
        let mut tree = mini_tree();
        let bad_stem =
            PAGE_CMP_REVIEW_STEPS.replace("id = \"cmp-review-steps\"", "id = \"cmp-review-step\"");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_stem.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0105);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 2);

        // Invalid `id` form in `parse_meta`
        let mut tree_form = mini_tree();
        let bad_form =
            PAGE_CMP_REVIEW_STEPS.replace("id = \"cmp-review-steps\"", "id = \"CMP_BAD\"");
        tree_form.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_form.as_bytes());
        let report_form = run(&Host::new(tree_form, sample_today()), &Options::new(false));
        assert_eq!(report_form.diagnostics.len(), 1);
        assert_eq!(report_form.diagnostics[0].code, Code::Fb0105);
        assert_eq!(report_form.diagnostics[0].line, 2);

        // Duplicate `id` across two pages: reported at the second, naming the first
        let mut tree_dup = mini_tree();
        let corpus_with_reg_fw = MINI_CORPUS_TOML.replace(
            "subareas = [\"business-systems\"]",
            "subareas = [\"business-systems\", \"regulatory-framework\"]",
        );
        tree_dup.insert_bytes("corpus/corpus.toml", corpus_with_reg_fw.as_bytes());
        tree_dup.insert_dir("corpus/pages/compliance/regulatory-framework");
        let dup_page = PAGE_CMP_SYSTEM_REVIEW.replace(
            "subarea = \"business-systems\"",
            "subarea = \"regulatory-framework\"",
        );
        let second_path = "corpus/pages/compliance/regulatory-framework/cmp-system-review.md";
        tree_dup.insert_bytes(second_path, dup_page.as_bytes());
        let report_dup = run(&Host::new(tree_dup, sample_today()), &Options::new(false));
        assert_eq!(report_dup.diagnostics.len(), 1);
        assert_eq!(report_dup.diagnostics[0].code, Code::Fb0105);
        assert_eq!(report_dup.diagnostics[0].path, second_path);
        assert_eq!(report_dup.diagnostics[0].line, 2);
        assert!(
            report_dup.diagnostics[0]
                .message
                .contains(PATH_CMP_SYSTEM_REVIEW),
            "expected duplicate id diagnostic to name first page: {}",
            report_dup.diagnostics[0].message
        );
    }

    #[test]
    fn fb0106_area_mismatch_and_unknown_area() {
        // `id = "cmp-review-steps"` with `area = "est"`
        let mut tree = mini_tree();
        let bad_area = PAGE_CMP_REVIEW_STEPS.replace("area = \"cmp\"", "area = \"est\"");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_area.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0106);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 4);

        // Area code not in `corpus.toml` (`area = "zzz"`)
        let mut tree_unk = mini_tree();
        let bad_unk = PAGE_CMP_REVIEW_STEPS.replace("area = \"cmp\"", "area = \"zzz\"");
        tree_unk.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_unk.as_bytes());
        let report_unk = run(&Host::new(tree_unk, sample_today()), &Options::new(false));
        assert_eq!(report_unk.diagnostics.len(), 1);
        assert_eq!(report_unk.diagnostics[0].code, Code::Fb0106);
        assert_eq!(report_unk.diagnostics[0].line, 4);
    }

    #[test]
    fn fb0107_subarea_mismatch() {
        // `subarea = "cash"` under `cmp`
        let mut tree = mini_tree();
        let bad_sub =
            PAGE_CMP_REVIEW_STEPS.replace("subarea = \"business-systems\"", "subarea = \"cash\"");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_sub.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0107);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 5);
    }

    #[test]
    fn fb0108_invalid_page_type() {
        // `type = "note"` under schema 1
        let mut tree = mini_tree();
        let bad_type = PAGE_CMP_REVIEW_STEPS.replace("type = \"procedure\"", "type = \"note\"");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_type.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0108);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 6);
    }

    #[test]
    fn fb0109_computed_fields_written_use_parser_field_location() {
        let mut tree = mini_tree();
        // Leading comment containing `review_by = 2026-01-01` and reordered keys:
        // must report at the real key lines (17 and 18), not the comment line!
        let bad_computed = PAGE_CMP_REVIEW_STEPS
            .replace(
                "+++\nid = \"cmp-review-steps\"",
                "+++\n# comment mentioning review_by = 2026-01-01\nid = \"cmp-review-steps\"",
            )
            .replace(
                "volatility = \"V2\"\n+++",
                "volatility = \"V2\"\n  review_by = 2026-01-01\nclaims = [\"CL-CMP-0001\"]\n+++",
            );
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, bad_computed.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert_eq!(report.diagnostics.len(), 2);
        assert_eq!(report.diagnostics[0].code, Code::Fb0109);
        assert_eq!(report.diagnostics[0].line, 17);
        assert_eq!(report.diagnostics[0].col, 3);
        assert_eq!(report.diagnostics[1].code, Code::Fb0109);
        assert_eq!(report.diagnostics[1].line, 18);
        assert_eq!(report.diagnostics[1].col, 1);
    }

    #[test]
    fn fb0114_invalid_or_partial_front_matter_dates() {
        for bad_date in [
            "as_of = 2025-02-30",
            "as_of = 2025-11",
            "as_of = \"2025-11\"",
            "as_of = 2025",
        ] {
            let mut tree = mini_tree();
            let page = PAGE_CMP_REVIEW_STEPS.replace("as_of = 2026-02-01", bad_date);
            tree.insert_bytes(PATH_CMP_REVIEW_STEPS, page.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::new(false));
            assert_eq!(
                report.diagnostics.len(),
                1,
                "expected single diagnostic for `{bad_date}`: {:?}",
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].code, Code::Fb0114);
            assert_eq!(report.diagnostics[0].line, 10);
        }
    }

    #[test]
    fn fb0116_as_of_after_today_and_non_utf8_page_ignored_by_pages_pass() {
        let mut tree = mini_tree();
        let future = PAGE_CMP_REVIEW_STEPS.replace("as_of = 2026-02-01", "as_of = 2027-03-01");
        tree.insert_bytes(PATH_CMP_REVIEW_STEPS, future.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, Code::Fb0116);
        assert_eq!(report.diagnostics[0].severity, Severity::Warning);
        assert_eq!(report.diagnostics[0].path, PATH_CMP_REVIEW_STEPS);
        assert_eq!(report.diagnostics[0].line, 10);

        // Non-UTF-8 page raises only FB0120 from `files::check` and nothing from `pages::check`
        let mut tree_non_utf8 = mini_tree();
        let mut bytes = vec![0xff];
        bytes.extend_from_slice(PAGE_CMP_REVIEW_STEPS.as_bytes());
        tree_non_utf8.insert_bytes(PATH_CMP_REVIEW_STEPS, bytes);
        let report_non_utf8 = run(
            &Host::new(tree_non_utf8, sample_today()),
            &Options::new(false),
        );
        let codes: Vec<Code> = report_non_utf8.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, vec![Code::Fb0120]);
    }
}
