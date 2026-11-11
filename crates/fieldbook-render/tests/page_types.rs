//! Integration and snapshot tests for `fieldbook_render::types`, `plan`, and `stages` (`build-page-types`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod page_types {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::schema::PageType;
    use fieldbook_core::strings;
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::nav;
    use fieldbook_render::page::{render_main, render_page};
    use fieldbook_render::plan::{build_plan_timeline, parse_window_heading};
    use fieldbook_render::site::{self, PlannedPaths};
    use fieldbook_render::stages::{STAGE_COUNT, build_stage_columns, render_stage_track};

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
    const CMP_ONBOARDING_CHECKLIST_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md"
    );
    const CMP_WITHHOLD_CASE_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-withhold-case.md"
    );
    const CMP_CLAUSE_9900_101_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-clause-9900-101.md"
    );
    const EST_CALC_01_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-calc-01.md"
    );
    const EST_RATE_WINDOW_BRIEF_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );
    const EST_RATE_REFERENCE_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-reference.md"
    );
    const EST_BASELINE_TEMPLATE_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-baseline-template.md"
    );

    const SNAPSHOT_PLANNED: &[&str] = &[
        "p/cmp-clause-9900-101.html",
        "p/cmp-onboarding-checklist.html",
        "p/cmp-review-steps.html",
        "p/cmp-system-review.html",
        "p/cmp-withhold-case.html",
        "p/est-baseline-template.html",
        "p/est-calc-01.html",
        "p/est-rate-reference.html",
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
            "corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md",
            CMP_ONBOARDING_CHECKLIST_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-withhold-case.md",
            CMP_WITHHOLD_CASE_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-clause-9900-101.md",
            CMP_CLAUSE_9900_101_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-calc-01.md",
            EST_CALC_01_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-window-brief.md",
            EST_RATE_WINDOW_BRIEF_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-reference.md",
            EST_RATE_REFERENCE_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-baseline-template.md",
            EST_BASELINE_TEMPLATE_MD.as_bytes(),
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
        while let Some(sec_idx) = rest.find("<section class=\"part") {
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
    fn brief_parts_in_order_then_sources() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-window-brief")
            .unwrap();
        let html = render_main(page, &view, &planned);
        assert!(
            html.contains("<div class=\"sheet\">"),
            "brief must render inside a BriefSheet (.sheet)"
        );
        let rendered_headings = extract_part_headings(&html);
        let mut expected: Vec<String> = strings::brief_parts()
            .iter()
            .map(|&s| s.to_owned())
            .collect();
        expected.push(strings::parts()[6].to_owned());

        for exp in &expected {
            assert!(
                rendered_headings.contains(exp),
                "brief is missing required part `{exp}` (rendered: {rendered_headings:?})"
            );
        }
        assert_eq!(
            rendered_headings, expected,
            "brief parts must appear in exact brief_parts order followed by Sources"
        );
    }

    #[test]
    fn unfilled_brief_part_in_warn() {
        let mut tree = fixture_tree();
        let with_unfilled = EST_RATE_WINDOW_BRIEF_MD.replace(
            "- Multi-year synthetic escalation index selection remains open [@Q-002].",
            &format!("- {}", strings::empty_part()),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-window-brief.md",
            with_unfilled.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-window-brief")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let expected_warn = format!("<p class=\"empty-part warn\">{}</p>", strings::empty_part());
        assert!(
            html.contains(&expected_warn),
            "unfilled brief part must render in italic warn (`{expected_warn}`), got:\n{html}"
        );
    }

    #[test]
    fn checklist_windows_on_one_axis() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-onboarding-checklist")
            .unwrap();
        let tl = build_plan_timeline(page);
        assert_eq!(tl.axis_max, 60);
        assert_eq!(tl.windows.len(), 2);
        assert_eq!(tl.windows[0].label, "Day 1 to Day 30");
        assert_eq!(tl.windows[0].x_attr, "0.00");
        assert_eq!(tl.windows[0].width_attr, "50.00");
        assert_eq!(tl.windows[1].label, "Day 31 to Day 60");
        assert_eq!(tl.windows[1].x_attr, "50.00");
        assert_eq!(tl.windows[1].width_attr, "50.00");

        let html = render_main(page, &view, &planned);
        assert_eq!(
            html.matches("class=\"tl90\"").count(),
            1,
            "checklist windows must share a single PlanTimeline axis"
        );
        assert_eq!(
            html.matches("<rect class=\"bar1\"").count(),
            2,
            "both windows must plot on the shared SVG track"
        );
        assert!(
            html.contains("<ol class=\"steps\">"),
            "checklist task items under ### headings must render as numbered steps (<ol class=\"steps\">)"
        );
    }

    #[test]
    fn window_grammar_accepts_day_1_to_day_30() {
        assert_eq!(parse_window_heading("Day 1 to Day 30"), Some((1, 30)));
        assert_eq!(parse_window_heading("Day 1 to Day 1"), Some((1, 1)));
        assert_eq!(parse_window_heading("Day 31 to Day 60"), Some((31, 60)));
        assert_eq!(parse_window_heading("Day 61 to Day 90"), Some((61, 90)));
    }

    #[test]
    fn window_grammar_refuses_reversed_and_other_forms() {
        for bad in [
            "Day 30 to Day 1",
            "Day 1-30",
            "Days 1 to 30",
            "Day 0 to Day 30",
            "day 1 to day 30",
            "Day 1 to Day 30 ",
            " Day 1 to Day 30",
            "Phase 1",
        ] {
            assert_eq!(
                parse_window_heading(bad),
                None,
                "heading `{bad}` must be refused by window grammar"
            );
        }

        let mut tree = fixture_tree();
        let bad_headings_md = CMP_ONBOARDING_CHECKLIST_MD
            .replace("### Day 1 to Day 30", "### Day 30 to Day 1")
            .replace(
                "### Day 31 to Day 60",
                "### Day 1-30\n\n- [ ] Check hyphen form.\n\n### Days 1 to 30",
            );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md",
            bad_headings_md.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-onboarding-checklist")
            .unwrap();
        let tl = build_plan_timeline(page);
        assert!(
            tl.windows.is_empty(),
            "non-matching headings must add no window rows, got {:?}",
            tl.windows
        );
    }

    #[test]
    fn timeline_svg_has_no_text_or_style() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-onboarding-checklist")
            .unwrap();
        let html = render_main(page, &view, &planned);
        let svg_start = html
            .find("<svg class=\"track tl-track\"")
            .expect("timeline <svg> present");
        let svg_end = html[svg_start..]
            .find("</svg>")
            .expect("closing </svg> present")
            + svg_start
            + 6;
        let svg = &html[svg_start..svg_end];
        assert!(
            svg.contains("role=\"img\""),
            "timeline SVG must carry role=\"img\": {svg}"
        );
        assert!(
            svg.contains("aria-label=\"Plan timeline across 60 days:"),
            "timeline SVG must carry an accessible aria-label: {svg}"
        );
        assert!(
            !svg.contains("<text"),
            "timeline SVG must not contain <text>: {svg}"
        );
        assert!(
            !svg.contains("style="),
            "timeline SVG must not contain a style attribute: {svg}"
        );
    }

    #[test]
    fn reference_rows_newest_first() {
        let mut tree = fixture_tree();
        let with_older_first = EST_RATE_REFERENCE_MD.replace(
            "| Rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |\n| Escalation placeholder | 3 percent [@CL-EST-0001] | Synthetic multi-year assumption [@CL-EST-0001] |",
            "| Prior rate review window | 15 days [@CL-EST-0002] | Superseded synthetic guidance [@CL-EST-0002] |\n| Current rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |",
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-reference.md",
            with_older_first.into_bytes(),
        );
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-reference")
            .unwrap();
        let html = render_main(page, &view, &planned);

        let pos_2026_02 = html
            .find("data-as-of=\"2026-02-01\"")
            .expect("2026-02-01 row present");
        let pos_2026_01 = html
            .find("data-as-of=\"2026-01-15\"")
            .expect("2026-01-15 row present");
        assert!(
            pos_2026_02 < pos_2026_01,
            "newer reference row (2026-02-01) must sort before older row (2026-01-15)"
        );
        assert!(
            html.contains(
                "class=\"row-superseded\" data-as-of=\"2026-01-15\" data-superseded-by=\"CL-EST-0003\""
            ),
            "superseded reference row must carry row-superseded class and data-superseded-by"
        );
        assert!(
            html.contains("Superseded by CL-EST-0003"),
            "superseded reference row must be labelled with its replacing claim"
        );
    }

    #[test]
    fn reference_rows_follow_claim_facts() {
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);

        // 1. Repeated citation of the same claim in one row still counts as 1 unique claim,
        //    and partial dates sort by first_day while keeping authored order on ties and
        //    displaying at original precision.
        let mut tree1 = fixture_tree();
        let extra_claims = format!(
            "{CLAIMS_CSV}CL-EST-0004,est,fact,Synthetic 2026-01 partial date rate is 6 percent.,2026-01,S-0002,2,text_section,sec. 5,Synthetic Jan 2026 rate is 6 percent.,true,codex,unchecked,pending_company_confirmation,proposed,observed,V2,6,percent,,,,,,,,,,,,,,,Q-002,Synthetic partial-date claim.\nCL-EST-0005,est,fact,Synthetic 2026-01-01 full date rate is 7 percent.,2026-01-01,S-0002,2,text_section,sec. 6,Synthetic Jan 1 2026 rate is 7 percent.,true,codex,unchecked,pending_company_confirmation,proposed,observed,V2,7,percent,,,,,,,,,,,,,,,Q-002,Synthetic full-date tie claim.\nCL-EST-0006,est,fact,Synthetic 2026-02 partial date rate is 8 percent.,2026-02,S-0002,2,text_section,sec. 7,Synthetic Feb 2026 rate is 8 percent.,true,codex,unchecked,pending_company_confirmation,proposed,observed,V2,8,percent,,,,,,,,,,,,,,,Q-002,Synthetic Feb partial-date claim.\nCL-EST-0007,est,fact,Synthetic undated fact rate is 9 percent.,,S-0002,2,text_section,sec. 8,Synthetic undated rate is 9 percent.,true,codex,stale,pending_company_confirmation,proposed,observed,V2,9,percent,,,,,,,,,,,,,,,Q-002,as_of unknown: synthetic undated claim for reference table test.\n"
        );
        tree1.insert_bytes(
            "corpus/registers/claims.csv",
            extra_claims.as_bytes().to_vec(),
        );

        let dated_ties_md = EST_RATE_REFERENCE_MD.replace(
            "| Rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |\n| Escalation placeholder | 3 percent [@CL-EST-0001] | Synthetic multi-year assumption [@CL-EST-0001] |",
            "| Prior window | 15 days [@CL-EST-0002] | Repeat same claim [@CL-EST-0002] |\n| Jan partial | 6 percent [@CL-EST-0004] | First tie row |\n| Jan full | 7 percent [@CL-EST-0005] | Second tie row |\n| Feb partial | 8 percent [@CL-EST-0006] | Newest partial |",
        );
        tree1.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-reference.md",
            dated_ties_md.into_bytes(),
        );
        let (_m1, view1) = load_view(tree1);
        let page1 = view1
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-reference")
            .unwrap();
        let html1 = render_main(page1, &view1, &planned);

        let pos_feb = html1.find("Feb partial").unwrap();
        let pos_prior = html1.find("Prior window").unwrap();
        let pos_jan_partial = html1.find("Jan partial").unwrap();
        let pos_jan_full = html1.find("Jan full").unwrap();
        assert!(
            pos_feb < pos_prior && pos_prior < pos_jan_partial && pos_jan_partial < pos_jan_full,
            "rows must sort newest-first by first_day (2026-02-01 > 2026-01-15 > 2026-01-01) and keep authored order on ties (2026-01 vs 2026-01-01)"
        );
        assert!(
            html1.contains("<time datetime=\"2026-02\">2026-02</time>"),
            "partial date 2026-02 must display at its original precision"
        );
        assert!(
            html1.contains("<time datetime=\"2026-01\">2026-01</time>"),
            "partial date 2026-01 must display at its original precision"
        );
        assert!(
            html1.contains("data-superseded-by=\"CL-EST-0003\""),
            "row citing CL-EST-0002 twice must still count as 1 distinct claim and infer supersession"
        );

        // 2. Undated row leaves the entire table in authored order and infers nothing from cell text.
        let mut tree2 = fixture_tree();
        tree2.insert_bytes(
            "corpus/registers/claims.csv",
            extra_claims.as_bytes().to_vec(),
        );
        let undated_md = EST_RATE_REFERENCE_MD.replace(
            "| Rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |\n| Escalation placeholder | 3 percent [@CL-EST-0001] | Synthetic multi-year assumption [@CL-EST-0001] |",
            "| Undated first row | 9 percent [@CL-EST-0007] | Cell text 2099-12-31 superseded |\n| Newer second row | 10 days [@CL-EST-0003] | Active window |",
        );
        tree2.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-reference.md",
            undated_md.into_bytes(),
        );
        let (_m2, view2) = load_view(tree2);
        let page2 = view2
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-reference")
            .unwrap();
        let html2 = render_main(page2, &view2, &planned);
        let pos_undated = html2.find("Undated first row").unwrap();
        let pos_newer = html2.find("Newer second row").unwrap();
        assert!(
            pos_undated < pos_newer,
            "an undated row must leave the entire table in authored order"
        );
        assert!(
            !html2.contains("data-as-of=\"2099-12-31\""),
            "dates must never be inferred from arbitrary cell text"
        );

        // 3. Ambiguous row (2 distinct claims) receives no inferred date or supersession and
        //    leaves the entire table in authored order.
        let mut tree3 = fixture_tree();
        let ambiguous_md = EST_RATE_REFERENCE_MD.replace(
            "| Rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |\n| Escalation placeholder | 3 percent [@CL-EST-0001] | Synthetic multi-year assumption [@CL-EST-0001] |",
            "| Ambiguous first row | 15 days [@CL-EST-0002] to 10 days [@CL-EST-0003] | Two distinct claims |\n| Single-claim second row | 10 days [@CL-EST-0003] | Active window |",
        );
        tree3.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-reference.md",
            ambiguous_md.into_bytes(),
        );
        let (_m3, view3) = load_view(tree3);
        let page3 = view3
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-reference")
            .unwrap();
        let html3 = render_main(page3, &view3, &planned);
        let pos_ambig = html3.find("Ambiguous first row").unwrap();
        let pos_single = html3.find("Single-claim second row").unwrap();
        assert!(
            pos_ambig < pos_single,
            "an ambiguous multi-claim row must leave the entire table in authored order"
        );
        assert!(
            !html3.contains("row-superseded"),
            "an ambiguous row citing both a superseded and an active claim must not infer supersession"
        );

        // 4. Reference table inside a `> Precise:` blockquote keeps `.precise` container balanced
        //    so following ordinary prose does not remain inside `.precise`.
        let mut tree4 = fixture_tree();
        let precise_table_md = EST_RATE_REFERENCE_MD.replace(
            "| Parameter | Value | Basis |\n|---|---|---|\n| Rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |\n| Escalation placeholder | 3 percent [@CL-EST-0001] | Synthetic multi-year assumption [@CL-EST-0001] |",
            "> Precise: Synthetic verbatim schedule [@CL-EST-0003].\n>\n> | Parameter | Value | Basis |\n> |---|---|---|\n> | Rate review window | 10 days [@CL-EST-0003] | Current synthetic pricing guidance [@CL-EST-0003] |\n\nPlain prose after the precise table [@CL-EST-0001].",
        );
        tree4.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-reference.md",
            precise_table_md.into_bytes(),
        );
        let (_m4, view4) = load_view(tree4);
        let page4 = view4
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "est-rate-reference")
            .unwrap();
        let html4 = render_main(page4, &view4, &planned);
        assert!(
            html4.contains("<div class=\"precise\"><span class=\"eyebrow\">Precise</span>"),
            "precise container must open before quoted reference table"
        );
        let close_precise = html4
            .find("</table></div>\n</div>\n<p>Plain prose after")
            .unwrap_or_else(|| {
                panic!(
                    "precise container must close with </div> before following plain prose:\n{html4}"
                )
            });
        assert!(close_precise > 0);
    }

    #[test]
    fn case_keeps_record_and_inference_apart() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-withhold-case")
            .unwrap();
        let html = render_main(page, &view, &planned);

        let rec_idx = html
            .find("data-case-section=\"public-record\"")
            .expect("public-record section present");
        let inf_idx = html
            .find("data-case-section=\"inference\"")
            .expect("inference section present");
        assert!(
            rec_idx < inf_idx,
            "Public record block must precede Inference block"
        );
        let rec_slice = &html[rec_idx..inf_idx];
        let inf_slice = &html[inf_idx..];
        assert!(
            rec_slice.contains(
                "<span class=\"ev-label\" title=\"Public record\">PF · Public record</span>"
            ),
            "Public record block must carry its evidence label"
        );
        assert!(
            rec_slice.contains("5 percent billing withhold"),
            "Public record prose must stay inside the Public record block"
        );
        assert!(
            !rec_slice.contains("Early pre-review checks reduce"),
            "Inference prose must not leak into Public record block"
        );
        assert!(
            inf_slice
                .contains("<span class=\"ev-label\" title=\"Inference\">INF · Inference</span>"),
            "Inference block must carry its evidence label"
        );
        assert!(
            inf_slice.contains("Early pre-review checks reduce"),
            "Inference prose must stay inside the Inference block"
        );

        // Introductory prose between `## How it works` and `### Public record` is preserved
        // alongside its citation markers.
        let mut tree_intro = fixture_tree();
        let with_intro = CMP_WITHHOLD_CASE_MD.replace(
            "## How it works\n\n### Public record",
            "## How it works\n\nIntroductory case context before subsections [@CL-CMP-0001].\n\n### Public record",
        );
        tree_intro.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-withhold-case.md",
            with_intro.into_bytes(),
        );
        let (_m_intro, view_intro) = load_view(tree_intro);
        let page_intro = view_intro
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-withhold-case")
            .unwrap();
        let html_intro = render_main(page_intro, &view_intro, &planned);
        let intro_pos = html_intro
            .find("Introductory case context before subsections")
            .expect("introductory prose in How it works must be preserved");
        let split_pos = html_intro
            .find("<div class=\"case-split\">")
            .expect("case-split container present");
        assert!(
            intro_pos < split_pos,
            "introductory prose must render before .case-split"
        );
    }

    #[test]
    fn stage_track_has_eight_columns_and_no_progress() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let cols = build_stage_columns("p/cmp-system-review.html", Some(2), &view, &planned);
        assert_eq!(cols.len(), usize::from(STAGE_COUNT));
        assert_eq!(cols.len(), 8);
        assert!(cols[1].is_current);
        assert!(
            cols[1]
                .pages
                .iter()
                .any(|p| p.page_id == "cmp-system-review"),
            "Stage 2 column must include cmp-system-review"
        );

        let html = render_stage_track("p/cmp-system-review.html", Some(2), &view, &planned);
        for s in 1..=8 {
            assert!(
                html.contains(&format!("data-stage=\"{s}\"")),
                "StageTrack must contain column for Stage {s}"
            );
        }
        assert!(
            html.contains("aria-current=\"step\""),
            "current stage column must carry aria-current=\"step\""
        );
        let lower = html.to_ascii_lowercase();
        assert!(
            !lower.contains("<progress") && !lower.contains("done") && !lower.contains('%'),
            "StageTrack must not show reading progress or done marks: {html}"
        );
    }

    #[test]
    fn mkt_page_renders_as_a_page() {
        let mut tree = fixture_tree();
        let corpus_with_mkt = format!(
            "{CORPUS_TOML}\n[[area]]\ncode = \"mkt\"\nslug = \"markets\"\nname = \"Markets\"\nsummary = \"Synthetic market and peer intelligence.\"\nsubareas = [\"company\", \"peers\", \"programs\"]\n"
        );
        tree.insert_bytes("corpus/corpus.toml", corpus_with_mkt.into_bytes());
        tree.insert_dir("corpus/pages/markets");
        tree.insert_dir("corpus/pages/markets/peers");
        let mkt_peer_md = "+++\nid = \"mkt-peer-alpha\"\ntitle = \"Synthetic peer alpha profile\"\narea = \"mkt\"\nsubarea = \"peers\"\ntype = \"concept\"\nstage = 2\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nProfile synthetic competitor Peer Alpha in the peers subarea.\n\n## The rule\n\nPeer Alpha reports a 30 days review cycle in public filings [@CL-CMP-0001].\n\n## How it works\n\nAnalysts compare public segment disclosures across periods.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nPeer Alpha discloses 2 synthetic segment awards.\n\n## Never confuse\n\nDo not confuse a public award announcement with funded backlog.\n\n## Company calibration\n\nNot established.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Added synthetic MKT peers page.\n";
        tree.insert_bytes(
            "corpus/pages/markets/peers/mkt-peer-alpha.md",
            mkt_peer_md.as_bytes(),
        );

        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(&["p/mkt-peer-alpha.html"]);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let nav = nav::build(&view, &planned);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "mkt-peer-alpha")
            .unwrap();
        let html = render_page(page, &view, &ctx, &nav, &planned);
        assert!(html.contains("<h1>Synthetic peer alpha profile</h1>"));
        assert!(html.contains("<span class=\"chip\">peers</span>"));
        assert!(html.contains("MKT"));
    }

    fn render_fixture_golden(page_id: &str) -> String {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == page_id)
            .unwrap_or_else(|| panic!("fixture page {page_id} not found"));
        let nav = nav::build(&view, &planned);
        render_page(page, &view, &ctx, &nav, &planned)
    }

    #[test]
    fn brief_page_renders() {
        let html = render_fixture_golden("est-rate-window-brief");
        insta::assert_snapshot!("brief_page_renders", html);
    }

    #[test]
    fn checklist_page_renders() {
        let html = render_fixture_golden("cmp-onboarding-checklist");
        insta::assert_snapshot!("checklist_page_renders", html);
    }

    #[test]
    fn reference_page_renders() {
        let html = render_fixture_golden("est-rate-reference");
        insta::assert_snapshot!("reference_page_renders", html);
    }

    #[test]
    fn case_page_renders() {
        let html = render_fixture_golden("cmp-withhold-case");
        insta::assert_snapshot!("case_page_renders", html);
    }

    #[test]
    fn clause_card_page_renders() {
        let html = render_fixture_golden("cmp-clause-9900-101");
        insta::assert_snapshot!("clause_card_page_renders", html);
    }

    #[test]
    fn calc_page_renders() {
        let html = render_fixture_golden("est-calc-01");
        insta::assert_snapshot!("calc_page_renders", html);
    }

    #[test]
    fn template_page_renders() {
        let html = render_fixture_golden("est-baseline-template");
        insta::assert_snapshot!("template_page_renders", html);
    }

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "test verifies snapshot files on disk"
    )]
    fn every_page_type_has_a_render_golden() {
        let snap_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
        let entries: Vec<String> = std::fs::read_dir(&snap_dir)
            .expect("tests/snapshots directory must exist")
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                if path.extension().and_then(|s| s.to_str()) != Some("snap") {
                    return None;
                }
                let meta = std::fs::metadata(&path).ok()?;
                if meta.len() == 0 {
                    return None;
                }
                path.file_name()?.to_str().map(ToOwned::to_owned)
            })
            .collect();

        for pt in PageType::ALL {
            let type_slug = pt.as_str().replace('-', "_");
            let needle = format!("__{type_slug}_page_renders.snap");
            let found = entries.iter().any(|name| name.ends_with(&needle));
            assert!(
                found,
                "missing render golden snapshot for page type `{}` (expected snapshot ending with `{needle}` in {})",
                pt.as_str(),
                snap_dir.display()
            );
        }
    }
}
