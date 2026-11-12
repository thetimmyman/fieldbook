//! Integration and snapshot tests for `fieldbook_render::page` (`build-page`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod page {
    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::ids::{AreaCode, ClaimId, ConflictId, QuestionId, TermId};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::review::{newest_change_note_date, page_review_by};
    use fieldbook_core::schema::ClaimRow;
    use fieldbook_core::strings;
    use fieldbook_render::chip;
    use fieldbook_render::inline::first_sentence_of_purpose;
    use fieldbook_render::links::{
        render_area_chip, render_citation_chip, render_claim_chip, render_conflict_chip,
        render_page_link, render_question_chip, render_term_link,
    };
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::nav;
    use fieldbook_render::page::{render_due_claims, render_main, render_page};
    use fieldbook_render::site::{self, PlannedPaths};

    const APP_CSS: &str = include_str!("../assets/app.css");
    const CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const ASSISTANT_MD: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const CHANGES_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const SOURCES_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const CLAIMS_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const QUESTIONS_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const CONFLICTS_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const GLOSSARY_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
    const S_0002_V2_TXT: &[u8] =
        include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt");
    const S_0001_V1_TXT: &[u8] =
        include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt");
    const CMP_SYSTEM_REVIEW_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const CMP_REVIEW_STEPS_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const EST_CALC_01_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-calc-01.md"
    );
    const EST_RATE_WINDOW_BRIEF_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    const SNAPSHOT_PLANNED: &[&str] = &[
        "p/cmp-review-steps.html",
        "p/cmp-system-review.html",
        "p/est-calc-01.html",
        "p/est-rate-window-brief.html",
    ];

    fn fixture_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/eval");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating");
        tree.insert_dir("corpus/pages/estimating/pricing");

        tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", CLAIMS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            CMP_SYSTEM_REVIEW_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            CMP_REVIEW_STEPS_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-calc-01.md",
            EST_CALC_01_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-window-brief.md",
            EST_RATE_WINDOW_BRIEF_MD.as_bytes(),
        );
        tree
    }

    fn load_view(tree: Tree) -> (Model, LabelledView) {
        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, report) = load(&Host::new(tree, today), &Options::new(false));
        assert!(
            report.is_ok(),
            "fixture tree must load cleanly:\n{}",
            report.render_text()
        );
        let eff = EffectivePolicy::floor(false);
        let now = Now::new(today, 0).unwrap();
        let Decision::Allowed(allowed) =
            check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
        else {
            panic!("edition selection must succeed");
        };
        let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
        (model, view)
    }

    fn extract_part_headings(html: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = html;
        while let Some(sec_idx) = rest.find("<section class=\"part\"") {
            let after_sec = &rest[sec_idx..];
            let Some(h2_start) = after_sec.find("<h2") else {
                break;
            };
            let after_h2 = &after_sec[h2_start..];
            let Some(gt_idx) = after_h2.find('>') else {
                break;
            };
            let after_gt = &after_h2[gt_idx + 1..];
            let Some(end_h2) = after_gt.find("</h2>") else {
                break;
            };
            out.push(after_gt[..end_h2].to_owned());
            rest = &after_gt[end_h2 + 5..];
        }
        out
    }

    #[test]
    fn parts_in_frozen_order() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let rendered_headings = extract_part_headings(&html);
        let expected: Vec<String> = strings::parts().iter().map(|&s| s.to_owned()).collect();
        if rendered_headings != expected {
            for (idx, (got, exp)) in rendered_headings.iter().zip(expected.iter()).enumerate() {
                if got != exp {
                    panic!(
                        "part {idx} out of frozen order: expected `{exp}`, got `{got}` (full order: {rendered_headings:?})"
                    );
                }
            }
            panic!(
                "parts do not match frozen order: expected {expected:?}, got {rendered_headings:?}"
            );
        }
    }

    #[test]
    fn empty_part_shows_the_string() {
        let mut tree = fixture_tree();
        let with_empty_part = CMP_SYSTEM_REVIEW_MD.replace(
            "Do not confuse a synthetic system review or a synthetic billing withhold with a final contract closeout audit.\n",
            "",
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            with_empty_part.into_bytes(),
        );
        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, _report) = load(&Host::new(tree, today), &Options::new(false));
        let eff = EffectivePolicy::floor(false);
        let now = Now::new(today, 0).unwrap();
        let Decision::Allowed(allowed) =
            check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
        else {
            panic!("edition selection must succeed");
        };
        let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let empty_marker = format!("<p class=\"empty-part\">{}</p>", strings::empty_part());
        assert!(
            html.contains(&empty_marker),
            "empty parts must render strings::empty_part(), got:\n{html}"
        );
        assert_eq!(
            html.matches(&empty_marker).count(),
            1,
            "cleared Never confuse part must render strings::empty_part() once"
        );
    }

    #[test]
    fn repeated_claim_has_one_row() {
        let mut tree = fixture_tree();
        let custom_md = format!(
            "+++\nid = \"cmp-system-review\"\ntitle = \"System review concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 2\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nterms = [\"system_review\"]\nquestions = [\"Q-001\"]\nconflicts = [\"XC-01\"]\nvolatility = \"V2\"\n+++\n\n## {}\n\nDefine a contractor business system review [@CL-CMP-0001].\n\n## {}\n\nCheck controls against the standard [@CL-CMP-0001].\n\n## {}\n\nRepeat the supported claim [@CL-CMP-0001] and note [@Q-001; @XC-01].\n\n## {}\n\nSynthetic example. The numbers are made up.\n\nA synthetic review takes 30 days.\n\n## {}\n\nDo not confuse with contract closeout.\n\n## {}\n\nConfirm the approving role [@Q-001].\n\n## {}\n\n## {}\n\n- NEW_KNOWLEDGE, 2026-02-01: Initial fixture page.\n",
            strings::parts()[0],
            strings::parts()[1],
            strings::parts()[2],
            strings::parts()[3],
            strings::parts()[4],
            strings::parts()[5],
            strings::parts()[6],
            strings::parts()[7],
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            custom_md.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);

        assert_eq!(
            html.matches("aria-label=\"Source 1\">1</a>").count(),
            3,
            "exactly three markers for Source 1 must exist"
        );
        assert_eq!(
            html.matches("id=\"cite-1\"").count(),
            1,
            "exactly one cite-1 row must exist"
        );
        assert!(!html.contains("id=\"cite-2\""), "no cite-2 row must exist");
        assert!(
            !html.contains("href=\"#cite-2\""),
            "no href to cite-2 must exist"
        );

        assert_eq!(
            html.matches("id=\"cite-ref-1-1\"").count(),
            1,
            "cite-ref-1-1 marker id must appear once"
        );
        assert_eq!(
            html.matches("id=\"cite-ref-1-2\"").count(),
            1,
            "cite-ref-1-2 marker id must appear once"
        );
        assert_eq!(
            html.matches("id=\"cite-ref-1-3\"").count(),
            1,
            "cite-ref-1-3 marker id must appear once"
        );

        let m1 = html.find("id=\"cite-ref-1-1\"").expect("marker 1-1");
        let m2 = html.find("id=\"cite-ref-1-2\"").expect("marker 1-2");
        let m3 = html.find("id=\"cite-ref-1-3\"").expect("marker 1-3");
        assert!(
            m1 < m2 && m2 < m3,
            "marker ids must appear in reading order"
        );
    }

    #[test]
    fn back_references_link_each_marker() {
        let mut tree = fixture_tree();
        let custom_md = format!(
            "+++\nid = \"cmp-system-review\"\ntitle = \"System review concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 2\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nterms = [\"system_review\"]\nquestions = [\"Q-001\"]\nconflicts = [\"XC-01\"]\nvolatility = \"V2\"\n+++\n\n## {}\n\nDefine a contractor business system review [@CL-CMP-0001].\n\n## {}\n\nCheck controls against the standard [@CL-CMP-0001] and standard version [@S-0001-v1, sec. 1].\n\n## {}\n\nRepeat the supported claim [@CL-CMP-0001] and note [@Q-001; @XC-01].\n\n## {}\n\nSynthetic example. The numbers are made up.\n\nA synthetic review takes 30 days.\n\n## {}\n\nDo not confuse with contract closeout.\n\n## {}\n\nConfirm the approving role [@Q-001].\n\n## {}\n\n## {}\n\n- NEW_KNOWLEDGE, 2026-02-01: Initial fixture page.\n",
            strings::parts()[0],
            strings::parts()[1],
            strings::parts()[2],
            strings::parts()[3],
            strings::parts()[4],
            strings::parts()[5],
            strings::parts()[6],
            strings::parts()[7],
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            custom_md.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);

        let r1 = html.find("id=\"cite-1\"").expect("row 1");
        let r2 = html.find("id=\"cite-2\"").expect("row 2");
        assert!(r1 < r2, "Sources rows 1, 2 must appear in order");

        let row1_slice = &html[r1..r2];
        let row2_slice = &html[r2..];

        assert!(
            row1_slice.contains("Back to text:"),
            "row 1 footer must contain `Back to text:`"
        );
        assert!(
            row1_slice.contains("href=\"#cite-ref-1-1\""),
            "row 1 footer must link to marker 1-1"
        );
        assert!(
            row1_slice.contains("href=\"#cite-ref-1-2\""),
            "row 1 footer must link to marker 1-2"
        );
        assert!(
            row1_slice.contains("href=\"#cite-ref-1-3\""),
            "row 1 footer must link to marker 1-3"
        );
        assert!(
            row1_slice.contains("aria-label=\"Back to citation a of 3 in the text\""),
            "row 1 link a aria-label"
        );
        assert!(
            row1_slice.contains("aria-label=\"Back to citation b of 3 in the text\""),
            "row 1 link b aria-label"
        );
        assert!(
            row1_slice.contains("aria-label=\"Back to citation c of 3 in the text\""),
            "row 1 link c aria-label"
        );

        assert!(
            row2_slice.contains("href=\"#cite-ref-2-1\""),
            "row 2 footer must link to marker 2-1"
        );
        assert!(
            row2_slice.contains("aria-label=\"Back to the citation in the text\""),
            "row 2 single link aria-label"
        );
        assert!(
            row2_slice.contains("Back to text"),
            "row 2 footer must contain `Back to text`"
        );

        let prefix = "href=\"#cite-ref-";
        let mut rest = html.as_str();
        let mut back_ref_targets = Vec::new();
        while let Some(idx) = rest.find(prefix) {
            let after = &rest[idx + prefix.len()..];
            let quote_end = after.find('"').expect("closing quote after href");
            let marker_id = format!("cite-ref-{}", &after[..quote_end]);
            back_ref_targets.push(marker_id);
            rest = &after[quote_end..];
        }
        assert_eq!(
            back_ref_targets.len(),
            4,
            "must have exactly 4 back-reference links"
        );
        for target in &back_ref_targets {
            let expected_id = format!("id=\"{target}\"");
            assert!(
                html.contains(&expected_id),
                "target id `{expected_id}` must exist in html"
            );
        }

        assert!(
            html.contains("aria-label=\"Sources cited on this page\""),
            "sources list must have accessible label"
        );
    }

    #[test]
    fn claim_and_version_rows_stay_separate() {
        let mut tree = fixture_tree();
        let custom_md = format!(
            "+++\nid = \"cmp-system-review\"\ntitle = \"System review concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 2\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nterms = [\"system_review\"]\nquestions = [\"Q-001\"]\nconflicts = [\"XC-01\"]\nvolatility = \"V2\"\n+++\n\n## {}\n\nDefine a contractor business system review [@CL-CMP-0001].\n\n## {}\n\nCheck controls against the standard [@S-0001-v1, sec. 1] and verify again [@S-0001-v1, sec. 1].\n\n## {}\n\nReview subsection [@S-0001-v1, sec. 2], repeat claim [@CL-CMP-0001] and note [@Q-001; @XC-01].\n\n## {}\n\nSynthetic example. The numbers are made up.\n\nA synthetic review takes 30 days.\n\n## {}\n\nDo not confuse with contract closeout.\n\n## {}\n\nConfirm the approving role [@Q-001].\n\n## {}\n\n## {}\n\n- NEW_KNOWLEDGE, 2026-02-01: Initial fixture page.\n",
            strings::parts()[0],
            strings::parts()[1],
            strings::parts()[2],
            strings::parts()[3],
            strings::parts()[4],
            strings::parts()[5],
            strings::parts()[6],
            strings::parts()[7],
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            custom_md.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);

        let r1 = html.find("id=\"cite-1\"").expect("row 1");
        let r2 = html.find("id=\"cite-2\"").expect("row 2");
        let r3 = html.find("id=\"cite-3\"").expect("row 3");
        assert!(
            r1 < r2 && r2 < r3,
            "Sources rows 1, 2, 3 must match marker order"
        );

        let row1_slice = &html[r1..r2];
        let row2_slice = &html[r2..r3];
        let row3_slice = &html[r3..];

        // Row 1 is the claim: contains CL-CMP-0001, has data-claim=.
        assert!(
            row1_slice.contains("CL-CMP-0001"),
            "row 1 must contain CL-CMP-0001"
        );
        assert!(
            row1_slice.contains("data-claim="),
            "row 1 claim row must have data-claim="
        );

        // Row 2 is the version with locator sec. 1: no data-claim=, back to text lists two links a and b.
        assert!(
            row2_slice.contains("S-0001-v1"),
            "row 2 must contain version S-0001-v1"
        );
        assert!(
            row2_slice.contains("sec. 1"),
            "row 2 must contain locator sec. 1"
        );
        assert!(
            !row2_slice.contains("data-claim="),
            "direct source-version row must not invent a claim"
        );
        assert!(
            row2_slice.contains("Back to text:"),
            "row 2 footer must contain `Back to text:`"
        );
        assert!(
            row2_slice.contains("href=\"#cite-ref-2-1\""),
            "row 2 footer must link to marker 2-1"
        );
        assert!(
            row2_slice.contains("href=\"#cite-ref-2-2\""),
            "row 2 footer must link to marker 2-2"
        );
        assert!(
            row2_slice.contains(">a</a>"),
            "row 2 footer must have link a"
        );
        assert!(
            row2_slice.contains(">b</a>"),
            "row 2 footer must have link b"
        );

        // Row 3 is the version with the other locator: one link.
        assert!(
            row3_slice.contains("S-0001-v1"),
            "row 3 must contain version S-0001-v1"
        );
        assert!(
            row3_slice.contains("sec. 2"),
            "row 3 must contain locator sec. 2"
        );
        assert!(
            row3_slice.contains("href=\"#cite-ref-3-1\""),
            "row 3 footer must link to marker 3-1"
        );
        assert!(
            row3_slice.contains("aria-label=\"Back to the citation in the text\""),
            "row 3 footer must have single back-to-text aria-label"
        );
        assert!(
            row3_slice.contains("Back to text"),
            "row 3 footer must contain `Back to text`"
        );
        assert!(
            !row3_slice.contains("href=\"#cite-ref-3-2\""),
            "row 3 must not have a second link"
        );

        // There is no id="cite-4".
        assert!(!html.contains("id=\"cite-4\""), "no cite-4 row must exist");
        assert!(
            !html.contains("href=\"#cite-4\""),
            "no href to cite-4 must exist"
        );

        // The second claim marker shows number 1 (id="cite-ref-1-2").
        assert!(
            html.contains("id=\"cite-ref-1-2\""),
            "second claim marker must have id cite-ref-1-2"
        );
        let m_claim2 = html
            .find("id=\"cite-ref-1-2\"")
            .expect("second claim marker");
        let m_claim2_end = html[m_claim2..]
            .find("</a>")
            .expect("second claim marker closing tag");
        let m_claim2_slice = &html[m_claim2..m_claim2 + m_claim2_end + 4];
        assert!(
            m_claim2_slice.contains("href=\"#cite-1\""),
            "second claim marker must link to #cite-1"
        );
        assert!(
            m_claim2_slice.contains("aria-label=\"Source 1\">1</a>"),
            "second claim marker must show number 1"
        );
    }

    #[test]
    fn sources_built_only_from_citations() {
        let mut tree = fixture_tree();
        let planted_prose = "PLANTED_SOURCES_PROSE_SHOULD_NEVER_RENDER";
        let with_planted = CMP_SYSTEM_REVIEW_MD.replace(
            &format!("## {}\n", strings::parts()[6]),
            &format!("## {}\n\n{planted_prose}\n", strings::parts()[6]),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            with_planted.into_bytes(),
        );

        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, _report) = load(&Host::new(tree, today), &Options::new(false));
        let eff = EffectivePolicy::floor(false);
        let now = Now::new(today, 0).unwrap();
        let Decision::Allowed(allowed) =
            check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
        else {
            panic!("edition selection must succeed");
        };
        let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        assert!(
            !html.contains(planted_prose),
            "prose under Sources in .md must not appear in rendered Sources"
        );
        assert!(html.contains("id=\"cite-1\""));
    }

    #[test]
    fn unbuilt_targets_are_text() {
        let (_model, view) = load_view(fixture_tree());
        let empty_planned = PlannedPaths::default();
        let from_path = "p/cmp-system-review.html";

        let area_html = render_area_chip(
            from_path,
            &AreaCode::parse("cmp").unwrap(),
            Some("Compliance"),
            &empty_planned,
        );
        assert!(
            !area_html.contains("<a "),
            "unbuilt area must not render <a>: {area_html}"
        );

        let ver_html = render_citation_chip(
            from_path,
            "S-0001-v1",
            Some("p. 12"),
            view.sources().first().map(|s| &s.value),
            &empty_planned,
        );
        assert!(
            !ver_html.contains("<a "),
            "unbuilt version must not render <a>: {ver_html}"
        );

        let claim_html = render_claim_chip(
            from_path,
            &ClaimId::parse("CL-CMP-0001").unwrap(),
            &empty_planned,
        );
        assert!(
            !claim_html.contains("<a "),
            "unbuilt claim must not render <a>: {claim_html}"
        );

        let q_html = render_question_chip(
            from_path,
            &QuestionId::parse("Q-001").unwrap(),
            Some("Sample question"),
            &empty_planned,
        );
        assert!(
            !q_html.contains("<a "),
            "unbuilt question must not render <a>: {q_html}"
        );

        let xc_html = render_conflict_chip(
            from_path,
            &ConflictId::parse("XC-01").unwrap(),
            Some("Sample conflict"),
            &empty_planned,
        );
        assert!(
            !xc_html.contains("<a "),
            "unbuilt conflict must not render <a>: {xc_html}"
        );

        let term_html = render_term_link(
            from_path,
            &TermId::parse("system_review").unwrap(),
            "system review",
            view.glossary().first().map(|g| &g.value),
            &empty_planned,
        );
        assert!(
            !term_html.contains("<a "),
            "unbuilt term must not render <a>: {term_html}"
        );

        let page_html = render_page_link(
            from_path,
            "cmp-review-steps",
            None,
            "System review steps",
            None,
            None,
            &empty_planned,
        );
        assert!(
            !page_html.contains("<a "),
            "unbuilt page must not render <a>: {page_html}"
        );

        // Also verify full page render with empty PlannedPaths has no external/unbuilt <a href="...html">
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-review-steps")
            .unwrap();
        let full_main = render_main(page, &view, &empty_planned);
        assert!(
            !full_main.contains("href=\"cmp-system-review.html\""),
            "unbuilt page link in prose must be plain text when PlannedPaths is empty"
        );
    }

    fn rail_section(html: &str, label: &str) -> String {
        let start_tag = format!("<section class=\"rail-sec rail-list\" aria-label=\"{label}\">");
        let Some(start_idx) = html.find(&start_tag) else {
            panic!("missing rail section with label `{label}` in html:\n{html}");
        };
        let rest = &html[start_idx..];
        let end_tag = "</section>";
        let Some(end_idx) = rest.find(end_tag) else {
            panic!(
                "missing closing tag `{end_tag}` for rail section with label `{label}` in html:\n{html}"
            );
        };
        rest[..end_idx + end_tag.len()].to_owned()
    }

    #[test]
    fn related_lists_links_with_titles() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-window-brief")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let sec = rail_section(&html, "Related");
        assert!(
            sec.contains("<a class=\"plink\" href=\"cmp-system-review.html\""),
            "expected link to cmp-system-review.html in Related section, got:\n{sec}"
        );
        let expected_title = "Synthetic business-system review and withholding overview";
        assert!(
            sec.contains(&format!(">{expected_title}</a>")),
            "expected link text to be full title of cmp-system-review, got:\n{sec}"
        );
        assert!(
            sec.contains("(1)"),
            "expected count (1) in Related section, got:\n{sec}"
        );
    }

    #[test]
    fn linked_from_lists_inbound_pages() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let sec = rail_section(&html, "Linked from");

        let brief_title = "Synthetic decision brief on forward pricing rate review window";
        let steps_title = "Synthetic procedure for running a business-system review";

        let brief_link = "<a class=\"plink\" href=\"est-rate-window-brief.html\"";
        let steps_link = "<a class=\"plink\" href=\"cmp-review-steps.html\"";

        assert!(
            sec.contains(brief_link),
            "expected link to est-rate-window-brief in Linked from, got:\n{sec}"
        );
        assert!(
            sec.contains(&format!(">{brief_title}</a>")),
            "expected est-rate-window-brief title in Linked from, got:\n{sec}"
        );
        assert!(
            sec.contains(steps_link),
            "expected link to cmp-review-steps in Linked from, got:\n{sec}"
        );
        assert!(
            sec.contains(&format!(">{steps_title}</a>")),
            "expected cmp-review-steps title in Linked from, got:\n{sec}"
        );

        let pos_brief = sec.find(brief_link).unwrap();
        let pos_steps = sec.find(steps_link).unwrap();
        assert!(
            pos_brief < pos_steps,
            "decision brief must precede procedure in title order"
        );

        assert!(
            sec.contains("(2)"),
            "expected count (2) in Linked from, got:\n{sec}"
        );

        assert!(
            !sec.contains("cmp-system-review.html"),
            "Linked from must not list cmp-system-review itself"
        );
        assert!(
            !sec.contains("Synthetic business-system review and withholding overview"),
            "Linked from must not contain page title itself"
        );
    }

    #[test]
    fn linked_from_is_capped() {
        let mut tree = fixture_tree();
        let mut planned_paths: Vec<String> =
            SNAPSHOT_PLANNED.iter().map(|&s| s.to_owned()).collect();

        for i in 1..=12 {
            let id = format!("est-fan-{i:02}");
            let title = format!("Synthetic fan-in page {i:02}");
            let content = EST_RATE_WINDOW_BRIEF_MD
                .replace("id = \"est-rate-window-brief\"", &format!("id = \"{id}\""))
                .replace(
                    "title = \"Synthetic decision brief on forward pricing rate review window\"",
                    &format!("title = \"{title}\""),
                );
            tree.insert_bytes(
                format!("corpus/pages/estimating/pricing/{id}.md"),
                content.into_bytes(),
            );
            planned_paths.push(format!("p/{id}.html"));
        }

        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::from_owned(planned_paths);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let sec = rail_section(&html, "Linked from");

        assert!(
            sec.contains("(14)"),
            "expected count (14) in Linked from, got:\n{sec}"
        );

        let fold_split: Vec<&str> = sec.split("<details class=\"fold\">").collect();
        assert_eq!(
            fold_split.len(),
            2,
            "expected exactly one <details class=\"fold\"> in Linked from section, got:\n{sec}"
        );
        let outside = fold_split[0];
        let inside = fold_split[1];

        assert_eq!(
            outside.matches("<li>").count(),
            10,
            "expected exactly 10 <li> rows outside the fold, got:\n{outside}"
        );

        assert!(
            inside.starts_with("<summary><b>4 more</b></summary>"),
            "expected summary <b>4 more</b> at start of fold, got:\n{inside}"
        );
        assert_eq!(
            inside.matches("<li>").count(),
            4,
            "expected exactly 4 <li> rows inside the fold, got:\n{inside}"
        );

        let details_start = sec.find("<details").expect("<details element");
        let details_end = sec[details_start..].find('>').expect("details tag end");
        let details_tag = &sec[details_start..details_start + details_end + 1];
        assert_eq!(details_tag, "<details class=\"fold\">");
        assert!(
            !details_tag.contains("open"),
            "<details> must not have an open attribute, got: {details_tag}"
        );
    }

    #[test]
    fn related_target_not_planned_is_text() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(&[
            "p/cmp-review-steps.html",
            "p/est-calc-01.html",
            "p/est-rate-window-brief.html",
        ]);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-window-brief")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let sec = rail_section(&html, "Related");

        let expected_title = "Synthetic business-system review and withholding overview";
        assert!(
            sec.contains("<span class=\"plink\""),
            "expected unbuilt target to render as <span class=\"plink\", got:\n{sec}"
        );
        assert!(
            sec.contains(expected_title),
            "expected title text in unbuilt target span, got:\n{sec}"
        );
        assert!(
            !sec.contains("<a "),
            "Related section must contain no <a> element when target is unbuilt, got:\n{sec}"
        );
    }

    #[test]
    fn related_lists_are_empty_state_when_none() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-calc-01")
            .unwrap();
        let html = render_main(page, &view, &planned);

        let related_sec = rail_section(&html, "Related");
        assert!(
            related_sec.contains("No related pages listed."),
            "Related empty state text missing, got:\n{related_sec}"
        );
        assert!(
            !related_sec.contains("<ul>"),
            "Related empty state must not contain <ul>, got:\n{related_sec}"
        );
        assert!(
            !related_sec.contains('('),
            "Related empty state must not contain '(' count, got:\n{related_sec}"
        );

        let linked_from_sec = rail_section(&html, "Linked from");
        assert!(
            linked_from_sec.contains("No page links here."),
            "Linked from empty state text missing, got:\n{linked_from_sec}"
        );
        assert!(
            !linked_from_sec.contains("<ul>"),
            "Linked from empty state must not contain <ul>, got:\n{linked_from_sec}"
        );
    }

    #[test]
    fn purpose_summary_drops_citation_markers() {
        let plain =
            "Explain how a synthetic system review interacts with a synthetic billing withhold";
        for (purpose, want) in [
            (format!("{plain} [@CL-CMP-0001]."), format!("{plain}.")),
            (
                format!("{plain} [@CL-CMP-0001; @CL-CMP-0002]."),
                format!("{plain}."),
            ),
            (
                format!("{plain} [@CL-CMP-0001] [@CL-CMP-0002]."),
                format!("{plain}."),
            ),
            (
                format!("{plain} [@S-0001-v1, sec. 1]. Second [@CL-CMP-0002]."),
                format!("{plain}."),
            ),
        ] {
            let mut tree = fixture_tree();
            tree.insert_bytes(
                "corpus/pages/compliance/business-systems/cmp-system-review.md",
                CMP_SYSTEM_REVIEW_MD
                    .replace(&format!("{plain}."), &purpose)
                    .into_bytes(),
            );
            let (_model, view) = load_view(tree);
            let concept = view
                .pages()
                .iter()
                .find(|p| p.id.as_str() == "cmp-system-review")
                .unwrap();
            assert_eq!(first_sentence_of_purpose(concept), want, "from: {purpose}");
        }
    }

    #[test]
    fn md_links_become_html() {
        let mut tree = fixture_tree();
        let two_sentence_concept = CMP_SYSTEM_REVIEW_MD.replace(
            "Explain how a synthetic system review interacts with a synthetic billing withhold.",
            "Explain how a synthetic system review interacts with a synthetic billing withhold. Second sentence stays out of the first-sentence popover summary.",
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            two_sentence_concept.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let concept = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        assert_eq!(
            first_sentence_of_purpose(concept),
            "Explain how a synthetic system review interacts with a synthetic billing withhold."
        );
        let steps = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-review-steps")
            .unwrap();
        let html = render_main(steps, &view, &planned);
        assert!(
            html.contains("href=\"cmp-system-review.html\""),
            "expected [review concept](cmp-system-review.md) to render href=\"cmp-system-review.html\", got:\n{html}"
        );
        assert!(
            html.contains(
                "data-pv-body=\"Explain how a synthetic system review interacts with a synthetic billing withhold.\""
            ),
            "expected first-sentence summary in PageLink popover data-pv-body, got:\n{html}"
        );
        // The context pack lists related pages with their full lead; the popover summary is the
        // rest of the page.
        let outside_pack = html
            .split("<details class=\"context-pack\">")
            .next()
            .unwrap();
        assert!(
            !outside_pack.contains("Second sentence stays out"),
            "second sentence of Purpose must not appear in PageLink popover summary"
        );
    }

    #[test]
    fn review_by_counts_from_newest_change_note() {
        // The fixture page is `as_of = 2026-02-01`, `volatility = "V2"` (180 days). A newer
        // Change note entry moves the base: 2026-09-01 + 180 days = 2027-02-28.
        let mut tree = fixture_tree();
        let md = CMP_SYSTEM_REVIEW_MD.replace(
            "- NEW_KNOWLEDGE, 2026-02-01:",
            "- COMPATIBLE_EXTENSION, 2026-09-01: Reworded the synthetic summary.\n- NEW_KNOWLEDGE, 2026-02-01:",
        );
        assert_ne!(
            md, CMP_SYSTEM_REVIEW_MD,
            "the fixture Change note must be found"
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            md.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        assert!(
            html.contains("Review by <time datetime=\"2027-02-28\">2027-02-28</time>"),
            "expected review_by counted from the newest Change note entry, got:\n{html}"
        );
    }

    #[test]
    fn review_by_is_computed() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let all_claims: Vec<ClaimRow> = view.claims().iter().map(|c| c.value.clone()).collect();
        let expected_date =
            page_review_by(&page.meta, newest_change_note_date(&page.body), &all_claims)
                .to_string();

        let html = render_main(page, &view, &planned);
        let expected_tag =
            format!("Review by <time datetime=\"{expected_date}\">{expected_date}</time>");
        assert!(
            html.contains(&expected_tag),
            "expected computed review_by `{expected_tag}`, got:\n{html}"
        );
        assert!(
            html.contains(&format!("data-review-by=\"{expected_date}\" hidden")),
            "expected overdue review banner with data-review-by=\"{expected_date}\" and hidden"
        );
    }

    /// Every cited claim with a computed review date gets a hidden row with the verify chip;
    /// a claim with no known date gets none, and only the viewer's script can reveal a row.
    #[test]
    fn overdue_claim_uses_verify_command_chip() {
        const APP_JS: &str = include_str!("../assets/app.js");
        let row = "data-claim-review-by=";
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let claim = view
            .claims()
            .iter()
            .find(|c| c.value.claim_id.as_str() == "CL-CMP-0001")
            .unwrap();
        let base = claim
            .value
            .as_of
            .map(|d| d.first_day())
            .max(claim.value.verified_on)
            .unwrap();
        let date = fieldbook_core::date::review_by(base, claim.value.volatility);
        assert!(
            html.contains(&format!("{row}\"{date}\" hidden")),
            "the row must carry the computed date and start hidden:\n{html}"
        );

        // Neither branch changes the rows: the due ids, their dates and the hidden start.
        let from_path = "p/cmp-system-review.html";
        let ids = [
            ClaimId::parse("CL-CMP-0001").unwrap(),
            ClaimId::parse("CL-CMP-0002").unwrap(),
        ];
        let landing = "The verify command is not available until Day 30.";
        let not_landed = render_due_claims(from_path, &ids, &view, &planned, false);
        let landed = render_due_claims(from_path, &ids, &view, &planned, true);
        for fold in [&not_landed, &landed] {
            assert_eq!(fold.matches(row).count(), 2, "{fold}");
            assert!(fold.contains("data-claim=\"CL-CMP-0001\""), "{fold}");
            assert!(fold.contains("data-claim=\"CL-CMP-0002\""), "{fold}");
            assert!(fold.contains(&format!("{row}\"{date}\" hidden")), "{fold}");
        }

        // Not landed: one landing line for the fold, and nothing to copy or to run.
        assert_eq!(not_landed.matches(landing).count(), 1, "{not_landed}");
        for absent in [
            "role=\"group\"",
            "class=\"cmd",
            "<button",
            "data-copy-chip",
            "Copy",
            "fieldbook verify",
            "YYYY-MM-DD",
        ] {
            assert!(!not_landed.contains(absent), "{absent}: {not_landed}");
        }

        // Landed: the chip on each row, with its literal placeholder and tag, and no landing line.
        assert!(!landed.contains(landing), "{landed}");
        assert!(
            landed.contains("<code>fieldbook verify CL-CMP-0001 --on YYYY-MM-DD</code>"),
            "the chip keeps the literal date placeholder:\n{landed}"
        );
        assert!(landed.contains("<code>fieldbook verify CL-CMP-0002 --on YYYY-MM-DD</code>"));
        assert_eq!(landed.matches("role=\"group\"").count(), 2, "{landed}");
        assert_eq!(landed.matches("data-copy-chip=").count(), 2, "{landed}");
        assert_eq!(
            landed
                .matches(&format!(
                    "<span class=\"cmd-tag\">{}</span>",
                    strings::tag_at_home()
                ))
                .count(),
            2,
            "{landed}"
        );

        // The page takes its branch from the one value.
        assert_eq!(html.contains("fieldbook verify"), chip::VERIFY_LANDED);
        assert_eq!(html.contains(landing), !chip::VERIFY_LANDED);

        assert!(html.contains("<div class=\"due-claims\" hidden><details class=\"fold\"><summary><b>Claims due for a check</b> <span class=\"due-count mono\"></span></summary>"));
        assert!(
            html.find("<details class=\"fold\"><summary><b>Claims due")
                .unwrap()
                < html.find(row).unwrap()
                && html.find(row).unwrap() < html.find("</details></div>").unwrap(),
            "the fold holds the rows, closed by default"
        );
        let banner_end = html.find("<header class=\"page-head\">").unwrap();
        assert!(
            html.find(row).unwrap() < banner_end,
            "the rows sit in the overdue banner"
        );

        // A claim with neither `as_of` nor `verified_on` has an unknown date: no row, no chip.
        let mut tree = fixture_tree();
        let claims = tree.get_bytes("corpus/registers/claims.csv").unwrap();
        let claims = std::str::from_utf8(claims).unwrap();
        let mut rows = csv_rows(claims);
        let header = rows[0].clone();
        let at = |name: &str| header.iter().position(|c| c == name).unwrap();
        for record in rows.iter_mut().skip(1) {
            if record[at("kind")] != "fact" {
                continue;
            }
            record[at("as_of")].clear();
            record[at("verified_on")].clear();
            record[at("verified_by_role")].clear();
            record[at("verification")] = "stale".to_owned();
            record[at("note")] = "as_of unknown".to_owned();
        }
        tree.insert_bytes("corpus/registers/claims.csv", csv_text(&rows).into_bytes());
        let (_model, view) = load_view(tree);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let unknown = render_main(page, &view, &planned);
        assert!(
            !unknown.contains(row) && !unknown.contains("due-claims") && !unknown.contains(landing),
            "an unknown review date is never overdue:\n{unknown}"
        );

        // The viewer's own date decides, in the page script; the build reads no clock.
        assert!(APP_JS.contains("rows[j].getAttribute('data-claim-review-by')"));
        assert!(APP_JS.contains("claimBy && today > claimBy"));
        assert!(APP_JS.contains("rows[j].hidden = false"));
        assert!(APP_JS.contains("dueCount.textContent = String(dueRows)"));
    }

    fn csv_rows(text: &str) -> Vec<Vec<String>> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(text.as_bytes());
        reader
            .records()
            .map(|r| r.unwrap().iter().map(str::to_owned).collect())
            .collect()
    }

    fn csv_text(rows: &[Vec<String>]) -> String {
        let mut writer = csv::Writer::from_writer(Vec::new());
        for row in rows {
            writer.write_record(row).unwrap();
        }
        String::from_utf8(writer.into_inner().unwrap()).unwrap()
    }

    #[test]
    fn never_verified_in_warn() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        assert!(
            html.contains("<span class=\"unver\">Never verified</span>"),
            "unverified claim must show `<span class=\"unver\">Never verified</span>`, got:\n{html}"
        );
        assert!(
            APP_CSS.contains(".unver") && APP_CSS.contains("color: var(--color-warn);"),
            ".unver in app.css must be styled with var(--color-warn)"
        );
    }

    #[test]
    fn change_note_renders_chip_date_and_sentence() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        assert!(
            html.contains(
                "<p class=\"change\"><span class=\"chip\">NEW_KNOWLEDGE</span> <time datetime=\"2026-02-01\">2026-02-01</time> Added the synthetic business-system review concept page.</p>"
            ),
            "expected structured change note chip, <time>, and sentence, got:\n{html}"
        );
    }

    #[test]
    fn change_note_is_the_last_part() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let headings = extract_part_headings(&html);
        assert_eq!(headings.len(), 8);
        assert_eq!(
            headings.last().map(String::as_str),
            Some(strings::parts()[7])
        );
    }

    #[test]
    fn no_style_attribute_or_handler() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let navigation = nav::build(&view, &planned);

        for page in view.pages() {
            let html = render_page(page, &view, &ctx, &navigation, &planned);
            assert!(
                !html.contains("style=\""),
                "page `{}` must not contain style=\" attribute",
                page.id
            );
            for attr in [
                " onclick=",
                " onload=",
                " onerror=",
                " onmouseover=",
                " onfocus=",
            ] {
                assert!(
                    !html.contains(attr),
                    "page `{}` must not contain inline handler `{attr}`",
                    page.id
                );
            }
            assert!(
                !first_sentence_of_purpose(page).is_empty(),
                "first_sentence_of_purpose must be non-empty for `{}`",
                page.id
            );
        }
    }

    #[test]
    fn concept_page_renders() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let navigation = nav::build(&view, &planned);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_page(page, &view, &ctx, &navigation, &planned);
        insta::assert_snapshot!("concept_page_renders", html);
    }

    #[test]
    fn procedure_page_renders() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let navigation = nav::build(&view, &planned);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-review-steps")
            .unwrap();
        let html = render_page(page, &view, &ctx, &navigation, &planned);
        insta::assert_snapshot!("procedure_page_renders", html);
    }

    #[test]
    fn decoded_entities_and_task_list_markers_render_cleanly() {
        let mut tree = fixture_tree();
        let updated = CMP_REVIEW_STEPS_MD.replace(
            "## How it works\n",
            "## How it works\n\n- [ ] Check R&amp;D &#37; rate\n- [x] Verify Done\n\n",
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            updated.as_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-review-steps")
            .unwrap();
        let html = render_main(page, &view, &planned);
        assert!(
            html.contains("Check R&amp;D % rate"),
            "HTML entities must decode once without double-escaping: {html}"
        );
        assert!(
            !html.contains("&amp;amp;") && !html.contains("&amp;#37;"),
            "HTML entities must not be double-escaped: {html}"
        );
        assert!(
            html.contains("<input type=\"checkbox\" disabled> Check"),
            "unchecked task list item must render disabled checkbox: {html}"
        );
        assert!(
            html.contains("<input type=\"checkbox\" checked disabled> Verify Done"),
            "checked task list item must render checked disabled checkbox: {html}"
        );
    }

    #[test]
    fn held_citation_shows_reason_without_link() {
        let mut tree = fixture_tree();
        let held_row = "S-0004,1,held-source,\"Synthetic Held Source, made up for tests\",Synthetic Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",,,metadata-only: made up for tests,5555555555555555555555555555555555555555555555555555555555555555,2026-01-01,,2026-02-01,2026-02-01,quarantined,,V2,public,Held <for> review & test.,Synthetic test,,,,,\n";
        tree.insert_bytes(
            "corpus/registers/sources.csv",
            format!("{SOURCES_CSV}{held_row}").into_bytes(),
        );
        let updated = CMP_REVIEW_STEPS_MD.replace(
            "## How it works\n",
            "## How it works\n\nThe held source says so [@S-0004-v1, sec. 1] and the active source says so [@S-0002-v2, sec. 1].\n\n",
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            updated.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = site::planned_paths(&view);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-review-steps")
            .unwrap();
        let html = render_main(page, &view, &planned);

        assert!(
            html.contains("Held: Held &lt;for&gt; review &amp; test."),
            "the escaped hold reason must show beside the held citation: {html}"
        );
        assert!(
            !html.contains("Held <for>"),
            "the hold reason must be escaped: {html}"
        );
        assert!(
            !html.contains("S-0004-v1.html") && !html.contains("sources.html#S-0004-v1"),
            "a held version has neither a source page link nor a register fallback: {html}"
        );
        assert!(
            html.contains("href=\"../s/S-0002-v2.html\""),
            "the active comparison version keeps its link: {html}"
        );
    }

    #[test]
    fn unreleased_page_has_no_overdue_banner() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let navigation = nav::build(&view, &planned);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_page(page, &view, &ctx, &navigation, &planned);

        assert!(!html.contains("fb:overdueBanner"));
        assert!(!html.contains("Review overdue"));
        assert!(!html.contains("due-claims"));
        assert!(!html.contains("data-claim-review-by"));
        assert!(html.contains("Review by <time datetime="));
    }

    #[test]
    fn released_page_keeps_overdue_banner() {
        let mut tree = fixture_tree();
        tree.insert_bytes(
            "corpus/changes.md",
            b"# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n",
        );
        let (_model, view) = load_view(tree);
        let date = CivilDate::parse("2026-03-01").unwrap();
        let sha = "0123456789abcdef0123456789abcdef01234567".to_owned();
        let released_info = BuildInfo::new(Some(date), Some(sha), BuildProfile::Silent, None);
        let ctx = site::render_context(&view, &released_info).unwrap();
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let navigation = nav::build(&view, &planned);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        let html = render_page(page, &view, &ctx, &navigation, &planned);

        assert!(html.contains("id=\"fb:overdueBanner\""));
        assert!(html.contains("data-review-by=\""));
        let banner_start = html.find("id=\"fb:overdueBanner\"").unwrap();
        let banner_tag_end = html[banner_start..].find('>').unwrap();
        let banner_tag = &html[banner_start..banner_start + banner_tag_end];
        assert!(banner_tag.contains("hidden"));
        assert!(html.contains("Review overdue."));
        assert!(!html.contains("This is an unreleased build"));
    }
}
