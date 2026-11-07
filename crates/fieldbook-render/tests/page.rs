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
    use fieldbook_core::review::page_review_by;
    use fieldbook_core::schema::ClaimRow;
    use fieldbook_core::strings;
    use fieldbook_render::inline::first_sentence_of_purpose;
    use fieldbook_render::links::{
        render_area_chip, render_citation_chip, render_claim_chip, render_conflict_chip,
        render_page_link, render_question_chip, render_term_link,
    };
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::nav;
    use fieldbook_render::page::{render_main, render_page};
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
    fn markers_number_in_reading_order_and_match_sources_rows() {
        let mut tree = fixture_tree();
        let custom_md = format!(
            "+++\nid = \"cmp-system-review\"\ntitle = \"System review concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 2\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nterms = [\"system_review\"]\nquestions = [\"Q-001\"]\nconflicts = [\"XC-01\"]\nvolatility = \"V2\"\n+++\n\n## {}\n\nDefine a contractor business system review [@CL-CMP-0001].\n\n## {}\n\nCheck controls against the standard [@S-0001-v1, sec. 1].\n\n## {}\n\nRepeat the supported claim [@CL-CMP-0001] and note [@Q-001; @XC-01].\n\n## {}\n\nSynthetic example. The numbers are made up.\n\nA synthetic review takes 30 days.\n\n## {}\n\nDo not confuse with contract closeout.\n\n## {}\n\nConfirm the approving role [@Q-001].\n\n## {}\n\n## {}\n\n- NEW_KNOWLEDGE, 2026-02-01: Initial fixture page.\n",
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

        let m1 = html.find("href=\"#cite-1\"").expect("marker 1");
        let m2 = html.find("href=\"#cite-2\"").expect("marker 2");
        let m3 = html.find("href=\"#cite-3\"").expect("marker 3");
        assert!(
            m1 < m2 && m2 < m3,
            "markers 1, 2, 3 must appear in reading order"
        );
        assert!(html.contains("aria-label=\"Source 1\">1</a>"));
        assert!(html.contains("aria-label=\"Source 2\">2</a>"));
        assert!(html.contains("aria-label=\"Source 3\">3</a>"));
        assert!(
            !html.contains("href=\"#cite-4\""),
            "Q-001 and XC-01 must be RefChips, not numbered markers"
        );

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
        assert!(row1_slice.contains("CL-CMP-0001"));
        assert!(row2_slice.contains("S-0001-v1"));
        assert!(
            !row2_slice.contains("data-claim="),
            "direct source-version row must not invent a claim"
        );
        assert!(row3_slice.contains("CL-CMP-0001"));
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
        assert!(
            !html.contains("Second sentence stays out"),
            "second sentence of Purpose must not appear in PageLink popover summary"
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
        let expected_date = page_review_by(&page.meta, &all_claims).to_string();

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
        assert!(
            html.contains("<code>fieldbook verify CL-CMP-0001 --on YYYY-MM-DD</code>"),
            "the chip keeps the literal date placeholder:\n{html}"
        );
        assert!(html.contains("role=\"group\""));
        assert!(html.contains(&format!(
            "<span class=\"cmd-tag\">{}</span>",
            strings::tag_at_home()
        )));
        assert!(html.contains("<div class=\"due-claims\" hidden>"));
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
            !unknown.contains(row) && !unknown.contains("due-claims"),
            "an unknown review date is never overdue:\n{unknown}"
        );

        // The viewer's own date decides, in the page script; the build reads no clock.
        assert!(APP_JS.contains("rows[j].getAttribute('data-claim-review-by')"));
        assert!(APP_JS.contains("claimBy && today > claimBy"));
        assert!(APP_JS.contains("rows[j].hidden = false"));
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
}
