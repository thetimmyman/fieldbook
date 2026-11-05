//! Integration and snapshot tests for `fieldbook_render::{register, ids, status_chip, links}` (`build-registers`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod registers {
    use std::collections::BTreeSet;
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::ids::{AreaCode, ClaimId, ConflictId, QuestionId, SourceId, VersionId};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::schema::{
        Adoption, Applicability, ConflictRow, ConflictStatus, RefId, Register, Verification,
    };
    use fieldbook_render::ids;
    use fieldbook_render::links::{
        first_eligible_version, render_citation_chip, render_source_id_chip, render_version_chip,
        source_id_href, source_version_href,
    };
    use fieldbook_render::lint::{OriginalNames, lint};
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::nav;
    use fieldbook_render::page;
    use fieldbook_render::register::{
        QUESTIONS_CORPUS_COLS, build_register_conflict_box, build_register_view, output_path,
        render_main, render_page,
    };
    use fieldbook_render::site::{self, PlannedPaths};
    use fieldbook_render::status_chip::{
        render_adoption_chip, render_applicability_chip, render_handling_chips,
        render_verification_chip,
    };

    const APP_CSS: &str = include_str!("../assets/app.css");
    const APP_JS: &str = include_str!("../assets/app.js");
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

    const FIXED_PLANNED_PATHS: &[&str] = &[
        "claims.html",
        "sources.html",
        "questions.html",
        "conflicts.html",
        "glossary.html",
        "ids.html",
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
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");
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

    fn empty_registers_tree() -> Tree {
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
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");
        for reg in Register::ALL {
            let header = reg
                .columns()
                .iter()
                .map(|c| c.name)
                .collect::<Vec<_>>()
                .join(",")
                + "\n";
            tree.insert_bytes(
                format!("corpus/registers/{}", reg.file_name()),
                header.into_bytes(),
            );
        }
        tree
    }

    fn load_view(tree: Tree) -> (Model, LabelledView) {
        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, report) = load(&Host::new(tree, today), &Options::new(false));
        assert!(
            report.is_ok(),
            "tree must load cleanly:\n{}",
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

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "test inspects template files on disk"
    )]
    fn one_template_for_five_registers() {
        let templates_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("templates");
        let mut html_templates = Vec::new();
        for entry in std::fs::read_dir(&templates_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("html") {
                html_templates.push(path.file_name().unwrap().to_str().unwrap().to_owned());
            }
        }
        html_templates.sort();
        assert!(
            html_templates.contains(&"register.html".to_owned()),
            "templates/register.html must exist"
        );
        for forbidden in [
            "claims.html",
            "sources.html",
            "questions.html",
            "conflicts.html",
            "glossary.html",
            "register2.html",
            "register_claims.html",
            "register_sources.html",
            "register_questions.html",
            "register_conflicts.html",
            "register_glossary.html",
        ] {
            assert!(
                !html_templates.contains(&forbidden.to_owned()),
                "must not create a second register template `{forbidden}` in templates/"
            );
        }
        let reg_templates: Vec<&String> = html_templates
            .iter()
            .filter(|name| name.starts_with("register"))
            .collect();
        assert_eq!(
            reg_templates,
            vec![&"register.html".to_owned()],
            "exactly one register template (`register.html`) may exist in templates/"
        );

        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        for reg in Register::ALL {
            let html = render_main(reg, &view, &planned);
            assert!(
                html.contains("class=\"register-view\""),
                "`{}` must render through `templates/register.html`",
                output_path(reg)
            );
        }
    }

    #[test]
    fn row_anchor_is_the_id() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        let claims_html = render_main(Register::Claims, &view, &planned);
        for c in view.claims() {
            let needle = format!("<tr id=\"{}\"", c.value.claim_id);
            assert!(
                claims_html.contains(&needle),
                "register `claims.html` missing row anchor `{needle}`"
            );
        }

        let sources_html = render_main(Register::Sources, &view, &planned);
        for s in view.sources() {
            let vid = format!("{}-v{}", s.value.source_id, s.value.version);
            let needle = format!("<tr id=\"{vid}\"");
            assert!(
                sources_html.contains(&needle),
                "register `sources.html` missing row anchor `{needle}`"
            );
        }

        let questions_html = render_main(Register::Questions, &view, &planned);
        for q in view.questions() {
            let needle = format!("<tr id=\"{}\"", q.value.question_id);
            assert!(
                questions_html.contains(&needle),
                "register `questions.html` missing row anchor `{needle}`"
            );
        }

        let conflicts_html = render_main(Register::Conflicts, &view, &planned);
        for xc in view.conflicts() {
            let needle = format!("<tr id=\"{}\"", xc.value.conflict_id);
            assert!(
                conflicts_html.contains(&needle),
                "register `conflicts.html` missing row anchor `{needle}`"
            );
        }

        let glossary_html = render_main(Register::Glossary, &view, &planned);
        for g in view.glossary() {
            let needle = format!("<tr id=\"{}\"", g.value.term_id);
            assert!(
                glossary_html.contains(&needle),
                "register `glossary.html` missing row anchor `{needle}`"
            );
        }

        // Also verify full site build passes FB0603 anchor resolution lint.
        let rendered = site::render(&view, &BuildInfo::default()).unwrap();
        let diags = lint(&rendered, &OriginalNames::default());
        assert!(
            diags.is_empty(),
            "rendered site must have 0 FB0603/FB0604/FB0605 lint errors, got: {diags:?}"
        );
    }

    #[test]
    fn anchors_are_unique_per_page() {
        // Synthetic register of three rows (plus full fixture registers).
        let mut tree = empty_registers_tree();
        let three_questions = concat!(
            "question_id,area,question,owner_role,blocks,why_it_matters,default_assumption,work_can_continue,day_one,status,answer,answered_on,adoption\n",
            "Q-001,cmp,First synthetic question?,,,,,,false,open,,,\n",
            "Q-002,cmp,Second synthetic question?,,,,,,false,open,,,\n",
            "Q-003,est,Third synthetic question?,,,,,,false,open,,,\n"
        );
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            three_questions.as_bytes().to_vec(),
        );
        let (_m3, view3) = load_view(tree);
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let q3_html = render_main(Register::Questions, &view3, &planned);

        let mut q3_ids = Vec::new();
        for part in q3_html.split("<tr id=\"").skip(1) {
            let id = part.split('"').next().unwrap();
            q3_ids.push(id.to_owned());
        }
        assert_eq!(
            q3_ids.len(),
            3,
            "register `questions.html` must render 3 `<tr id=...>` rows"
        );
        let unique_q3: BTreeSet<&String> = q3_ids.iter().collect();
        assert_eq!(
            unique_q3.len(),
            q3_ids.len(),
            "register `questions.html` has duplicate row anchors: {q3_ids:?}"
        );

        // Also check every register on the fixture tree.
        let (_model, view) = load_view(fixture_tree());
        for reg in Register::ALL {
            let page_name = output_path(reg);
            let html = render_main(reg, &view, &planned);
            let mut row_ids = Vec::new();
            for part in html.split("<tr id=\"").skip(1) {
                let id = part.split('"').next().unwrap();
                row_ids.push(id.to_owned());
            }
            let unique: BTreeSet<&String> = row_ids.iter().collect();
            assert_eq!(
                unique.len(),
                row_ids.len(),
                "register `{page_name}` has duplicate row anchors: {row_ids:?}"
            );
        }
    }

    #[test]
    fn version_links_to_sources_row_until_s_page_is_built() {
        let (_model, view) = load_view(fixture_tree());
        let active_vid = VersionId::parse("S-0002-v2").unwrap();
        let active_row = view
            .sources()
            .iter()
            .find(|s| s.value.source_id.as_str() == "S-0002" && s.value.version == 2)
            .map(|s| &s.value);
        assert!(active_row.is_some());

        let without_s_page = PlannedPaths::of(&["sources.html", "claims.html"]);
        assert_eq!(
            source_version_href("claims.html", &active_vid, active_row, &without_s_page),
            Some("sources.html#S-0002-v2".to_owned()),
            "before `s/S-0002-v2.html` is planned, version must link to `sources.html#S-0002-v2`"
        );
        let chip_before =
            render_version_chip("claims.html", &active_vid, active_row, &without_s_page);
        assert!(
            chip_before.contains("href=\"sources.html#S-0002-v2\""),
            "version chip must link to `sources.html#S-0002-v2` before s/ is planned: `{chip_before}`"
        );

        let with_s_page = PlannedPaths::of(&["sources.html", "claims.html", "s/S-0002-v2.html"]);
        assert_eq!(
            source_version_href("claims.html", &active_vid, active_row, &with_s_page),
            Some("s/S-0002-v2.html".to_owned()),
            "once `s/S-0002-v2.html` is planned, version must link to `s/S-0002-v2.html`"
        );
        let chip_after = render_version_chip("claims.html", &active_vid, active_row, &with_s_page);
        assert!(
            chip_after.contains("href=\"s/S-0002-v2.html\""),
            "version chip must link to `s/S-0002-v2.html` once s/ is planned: `{chip_after}`"
        );
    }

    #[test]
    fn empty_register_shows_its_line() {
        let (_model, view) = load_view(empty_registers_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        for reg in Register::ALL {
            let html = render_main(reg, &view, &planned);
            assert!(
                html.contains("data-empty-register"),
                "empty `{}` must render the empty-register state panel",
                output_path(reg)
            );
            assert!(
                html.contains("None recorded"),
                "empty `{}` must show `None recorded`",
                output_path(reg)
            );
            assert_eq!(
                html.matches("None recorded").count(),
                1,
                "empty `{}` shows exactly one `None recorded` line",
                output_path(reg)
            );
            assert!(
                !html.contains("0 rows") && !html.contains(">0<"),
                "empty `{}` must never show a bare `0` or `0 rows`:\n{html}",
                output_path(reg)
            );
        }
    }

    #[test]
    fn unresolved_conflict_keeps_every_position() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        let html = render_main(Register::Conflicts, &view, &planned);
        assert!(
            html.contains("data-conflict=\"XC-01\""),
            "XC-01 ConflictBox must be present on conflicts.html"
        );
        assert!(
            html.contains("Synthetic review runs every 30 days"),
            "unresolved conflict XC-01 must keep Position 1"
        );
        assert!(
            html.contains("Synthetic withholding check runs at each billing cycle"),
            "unresolved conflict XC-01 must keep Position 2"
        );
        assert!(
            html.contains("Settled by") && html.contains("Q-001"),
            "unresolved conflict XC-01 must name the settling question Q-001"
        );
        assert!(
            !html.contains("Library adopts")
                && !html.contains("winner")
                && !html.contains("average"),
            "unresolved conflict must not show a winner, resolution, or average:\n{html}"
        );

        // Even if an unresolved ConflictRow in memory has resolution/rationale/evidence/resolved_on
        // populated, build_register_conflict_box must refuse to expose a winner or resolution.
        let synthetic_unresolved = ConflictRow {
            conflict_id: ConflictId::parse("XC-99").unwrap(),
            area: AreaCode::parse("cmp").unwrap(),
            topic: "Synthetic unresolved dispute".to_owned(),
            positions: vec!["First position".to_owned(), "Second position".to_owned()],
            resolution: Some("Forbidden winner resolution".to_owned()),
            rationale: Some("Forbidden rationale".to_owned()),
            evidence: vec![RefId::Claim(ClaimId::parse("CL-CMP-0001").unwrap())],
            resolved_on: Some(CivilDate::parse("2026-02-01").unwrap()),
            status: ConflictStatus::Unresolved,
            claim_ids: vec![ClaimId::parse("CL-CMP-0001").unwrap()],
            question_id: Some(QuestionId::parse("Q-001").unwrap()),
        };
        let box_view =
            build_register_conflict_box("conflicts.html", &synthetic_unresolved, &view, &planned);
        assert!(!box_view.is_resolved);
        assert_eq!(box_view.positions.len(), 2);
        assert_eq!(
            box_view.resolution, None,
            "unresolved conflict must never expose a resolution"
        );
        assert_eq!(
            box_view.rationale, None,
            "unresolved conflict must never expose a rationale"
        );
        assert_eq!(
            box_view.evidence_html, None,
            "unresolved conflict must never expose resolution evidence"
        );
        assert_eq!(
            box_view.resolved_on, None,
            "unresolved conflict must never expose a resolved_on date"
        );
    }

    #[test]
    fn ids_lists_every_id_once() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ids_view = ids::build_ids_view(&view, &planned);
        let html = ids::render_main(&view, &planned);

        let slugs: Vec<&str> = ids_view.groups.iter().map(|g| g.slug).collect();
        assert_eq!(
            slugs,
            vec![
                "pages",
                "sources",
                "versions",
                "claims",
                "questions",
                "conflicts",
                "terms"
            ],
            "ids.html must include all 7 ID groups in order"
        );

        let mut seen = BTreeSet::new();
        for grp in &ids_view.groups {
            for item in &grp.items {
                assert!(
                    seen.insert(item.id.clone()),
                    "id `{}` appeared more than once on ids.html",
                    item.id
                );
            }
        }

        for page in view.pages() {
            let needle = format!("data-id=\"{}\"", page.id);
            assert_eq!(
                html.matches(&needle).count(),
                1,
                "ids.html must list page `{}` exactly once",
                page.id
            );
            assert!(html.contains(&page.meta.title));
        }
        let distinct_sids: BTreeSet<&SourceId> =
            view.sources().iter().map(|s| &s.value.source_id).collect();
        for sid in distinct_sids {
            let sid_needle = format!("data-id=\"{sid}\"");
            assert_eq!(
                html.matches(&sid_needle).count(),
                1,
                "ids.html must list source `{sid}` exactly once"
            );
        }
        for s in view.sources() {
            let vid_needle = format!("data-id=\"{}-v{}\"", s.value.source_id, s.value.version);
            assert_eq!(
                html.matches(&vid_needle).count(),
                1,
                "ids.html must list source version `{}-v{}` exactly once",
                s.value.source_id,
                s.value.version
            );
        }
        for c in view.claims() {
            let needle = format!("data-id=\"{}\"", c.value.claim_id);
            assert_eq!(
                html.matches(&needle).count(),
                1,
                "ids.html must list claim `{}` exactly once",
                c.value.claim_id
            );
            assert!(html.contains(&c.value.statement));
        }
        for q in view.questions() {
            let needle = format!("data-id=\"{}\"", q.value.question_id);
            assert_eq!(
                html.matches(&needle).count(),
                1,
                "ids.html must list question `{}` exactly once",
                q.value.question_id
            );
            assert!(html.contains(&q.value.question));
        }
        for xc in view.conflicts() {
            let needle = format!("data-id=\"{}\"", xc.value.conflict_id);
            assert_eq!(
                html.matches(&needle).count(),
                1,
                "ids.html must list conflict `{}` exactly once",
                xc.value.conflict_id
            );
            assert!(html.contains(&xc.value.topic));
        }
        for t in view.glossary() {
            let needle = format!("data-id=\"{}\"", t.value.term_id);
            assert_eq!(
                html.matches(&needle).count(),
                1,
                "ids.html must list glossary term `{}` exactly once",
                t.value.term_id
            );
            assert!(html.contains(&t.value.label));
        }
    }

    #[test]
    fn status_chip_has_icon_and_word() {
        for v in Verification::ALL {
            let chip = render_verification_chip(v);
            assert!(
                chip.contains("<svg") && chip.contains("aria-hidden=\"true\""),
                "verification chip for `{v}` must include an aria-hidden SVG icon: `{chip}`"
            );
            assert!(
                chip.contains("<span>") && !chip.contains("<span></span>"),
                "verification chip for `{v}` must include a visible text word: `{chip}`"
            );
        }
        for a in Applicability::ALL {
            let chip = render_applicability_chip(a);
            assert!(
                chip.starts_with("<span class=\"chip\" title=\"") && !chip.contains("<svg"),
                "applicability chip for `{a}` is neutral words only: `{chip}`"
            );
            assert!(
                chip.contains(&format!("title=\"{a}\"")),
                "raw value in title: `{chip}`"
            );
        }
        for ad in Adoption::ALL {
            let chip = render_adoption_chip(ad);
            assert!(
                chip.starts_with("<span class=\"chip\" title=\"") && !chip.contains("<svg"),
                "adoption chip for `{ad}` is neutral words only: `{chip}`"
            );
        }
        let h_chip = render_handling_chips(&["public".to_owned(), "np-0007".to_owned()]);
        assert!(h_chip.contains("<span class=\"chip\">public</span>"));
        assert!(h_chip.contains("<span class=\"chip\">np-0007</span>"));
    }

    #[test]
    fn chips_link_once_their_targets_are_planned() {
        let (_model, view) = load_view(fixture_tree());
        let page_entry = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .expect("cmp-system-review page in fixture");

        let with_claims = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let html_with = page::render_main(page_entry, &view, &with_claims);
        assert!(
            html_with.contains("href=\"../claims.html#CL-CMP-0001\""),
            "when `claims.html` is in PlannedPaths, page claim chips must link to `../claims.html#CL-CMP-0001`, got:\n{html_with}"
        );

        let without_claims_vec: Vec<&str> = FIXED_PLANNED_PATHS
            .iter()
            .copied()
            .filter(|&p| p != "claims.html")
            .collect();
        let without_claims = PlannedPaths::of(&without_claims_vec);
        let html_without = page::render_main(page_entry, &view, &without_claims);
        assert!(
            !html_without.contains("claims.html#CL-CMP-0001")
                && html_without.contains("<span class=\"claim-mark"),
            "when `claims.html` is omitted from PlannedPaths, page claim chips must stay plain text `<span>`, got:\n{html_without}"
        );
    }

    #[test]
    fn enum_presentations_are_explicit() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        // Ensure `pdf_page_box` is also exercised in claims.csv alongside `primary_law` and `primary_evidence` in sources.csv.
        let mut tree_pdf = fixture_tree();
        let claims_header = CLAIMS_CSV.lines().next().unwrap();
        let pdf_row = claims_header
            .split(',')
            .map(|col| match col {
                "claim_id" => "CL-CMP-0003",
                "area" => "cmp",
                "kind" => "fact",
                "statement" => "Synthetic PDF box locator claim.",
                "as_of" => "2026-02-01",
                "source_id" => "S-0002",
                "version" => "2",
                "locator_kind" => "pdf_page_box",
                "locator" => "p. 1",
                "consequential" => "false",
                "drafted_by" => "codex",
                "verification" => "unchecked",
                "applicability" => "general_reference",
                "adoption" => "proposed",
                "data_origin" => "observed",
                "volatility" => "V2",
                "note" => "Synthetic pdf_page_box test row.",
                _ => "",
            })
            .collect::<Vec<_>>()
            .join(",");
        let claims_with_pdf =
            CLAIMS_CSV.replacen("CL-EST-0001", &format!("{pdf_row}\nCL-EST-0001"), 1);
        tree_pdf.insert_bytes("corpus/registers/claims.csv", claims_with_pdf.into_bytes());
        let (_m_pdf, view_pdf) = load_view(tree_pdf);

        let sources_html = render_main(Register::Sources, &view_pdf, &planned);
        let claims_html = render_main(Register::Claims, &view_pdf, &planned);
        assert!(
            sources_html.contains("<td>primary_law</td>"),
            "unmapped enum `primary_law` must render as plain canonical text"
        );
        assert!(
            sources_html.contains("<td>primary_evidence</td>"),
            "unmapped enum `primary_evidence` must render as plain canonical text"
        );
        assert!(
            claims_html.contains("<td>pdf_page_box</td>"),
            "unmapped enum `pdf_page_box` must render as plain canonical text"
        );

        let mapped_cols: &[(&str, &[&str])] = &[
            ("sources.html", &["handling"]),
            (
                "claims.html",
                &["verification", "applicability", "adoption"],
            ),
            ("questions.html", &[]),
            ("conflicts.html", &[]),
            ("glossary.html", &[]),
        ];

        let unmapped_enum_cols: &[(&str, Register, &[&str])] = &[
            (
                "sources.html",
                Register::Sources,
                &[
                    "source_kind",
                    "source_role",
                    "authority_tier",
                    "family_basis",
                    "lifecycle",
                    "volatility",
                ],
            ),
            (
                "claims.html",
                Register::Claims,
                &[
                    "kind",
                    "locator_kind",
                    "drafted_by",
                    "data_origin",
                    "volatility",
                    "unit",
                    "instrument",
                    "text_status",
                    "evidence_grade",
                    "verified_by_role",
                ],
            ),
            (
                "questions.html",
                Register::Questions,
                &["work_can_continue", "status"],
            ),
            ("conflicts.html", Register::Conflicts, &["status"]),
            ("glossary.html", Register::Glossary, &["definition_status"]),
        ];

        for &(page_name, reg, cols) in unmapped_enum_cols {
            let reg_view = build_register_view(reg, &view_pdf, &planned);
            for row in &reg_view.rows {
                for &col_name in cols {
                    let cell = row
                        .cells
                        .iter()
                        .find(|c| c.col_name == col_name)
                        .unwrap_or_else(|| panic!("missing column `{col_name}` in `{page_name}`"));
                    assert!(
                        !cell.html.contains("class=\"chip") && !cell.html.contains("<svg"),
                        "unmapped enum column `{col_name}` in `{page_name}` row `{}` must render plain canonical text, not a StatusChip: `{}`",
                        row.id,
                        cell.html
                    );
                }
            }
        }

        // Verify mapped status columns DO render StatusChips.
        for &(page_name, cols) in mapped_cols {
            let reg = Register::ALL
                .into_iter()
                .find(|&r| output_path(r) == page_name)
                .unwrap();
            let reg_view = build_register_view(reg, &view, &planned);
            for row in &reg_view.rows {
                for &col_name in cols {
                    let cell = row.cells.iter().find(|c| c.col_name == col_name).unwrap();
                    assert!(
                        cell.html.contains("class=\"chip")
                            && (col_name != "verification" || cell.html.contains("<svg")),
                        "mapped column `{col_name}` in `{page_name}` row `{}` must render its HANDOFF section 7 chip (verification adds an icon): `{}`",
                        row.id,
                        cell.html
                    );
                }
            }
        }
    }

    #[test]
    fn source_ids_choose_first_eligible_version() {
        // Build a tree where:
        // - S-0004 has only v1 (quarantined) -> all held -> stays plain text.
        // - S-0005 has v1 (quarantined), v10 (active), and v2 (active) -> first eligible
        //   version in numeric order is S-0005-v2 (skips held v1 and orders v2 before v10).
        let mut tree = fixture_tree();
        let extra_sources = format!(
            concat!(
                "{}",
                "S-0004,1,held-only,\"Synthetic Held Only Source, made up for tests\",Synthetic Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",,,metadata-only: made up for tests,5555555555555555555555555555555555555555555555555555555555555555,2026-01-01,,2026-02-01,2026-02-01,quarantined,,V2,public,Quarantined only version.,Synthetic test,,,,,",
                "\n",
                "S-0005,1,multi-ver,\"Synthetic Multi v1 Held, made up for tests\",Synthetic Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",synthetic_multi,reviewed,metadata-only: made up for tests,6666666666666666666666666666666666666666666666666666666666666666,2024-01-01,,2026-02-01,2026-02-01,quarantined,,V2,public,Quarantined v1.,Synthetic test,,,,,",
                "\n",
                "S-0005,2,multi-ver,\"Synthetic Multi v2 Active, made up for tests\",Synthetic Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",synthetic_multi,reviewed,metadata-only: made up for tests,7777777777777777777777777777777777777777777777777777777777777777,2025-01-01,,2026-02-01,2026-02-01,active,S-0005-v1,V2,public,Active v2.,Synthetic test,,,,,",
                "\n",
                "S-0005,10,multi-ver,\"Synthetic Multi v10 Active, made up for tests\",Synthetic Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",synthetic_multi,reviewed,metadata-only: made up for tests,8888888888888888888888888888888888888888888888888888888888888888,2026-01-10,,2026-02-01,2026-02-01,active,S-0005-v2,V2,public,Active v10.,Synthetic test,,,,,",
                "\n"
            ),
            SOURCES_CSV
        );
        tree.insert_bytes("corpus/registers/sources.csv", extra_sources.into_bytes());
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        let s4 = SourceId::parse("S-0004").unwrap();
        assert_eq!(
            first_eligible_version(&view, &s4),
            None,
            "S-0004 has only a quarantined version so first_eligible_version must be None"
        );
        assert_eq!(source_id_href("claims.html", &s4, &view, &planned), None);
        let s4_chip = render_source_id_chip("claims.html", &s4, &view, &planned);
        assert_eq!(s4_chip, "<span class=\"cite neutral\">S-0004</span>");

        let s5 = SourceId::parse("S-0005").unwrap();
        assert_eq!(
            first_eligible_version(&view, &s5),
            Some(VersionId::parse("S-0005-v2").unwrap()),
            "S-0005 must skip quarantined v1 and pick v2 before v10 in numeric version order"
        );
        assert_eq!(
            source_id_href("claims.html", &s5, &view, &planned),
            Some("sources.html#S-0005-v2".to_owned())
        );
        let s5_chip = render_source_id_chip("claims.html", &s5, &view, &planned);
        assert_eq!(
            s5_chip,
            "<a class=\"cite neutral\" href=\"sources.html#S-0005-v2\">S-0005</a>"
        );
    }

    #[test]
    fn held_links_have_no_register_fallback() {
        let mut tree = fixture_tree();
        let extra_sources = format!(
            "{}S-0004,1,held-source,\"Synthetic Held Source, made up for tests\",Synthetic Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",,,metadata-only: made up for tests,5555555555555555555555555555555555555555555555555555555555555555,2026-01-01,,2026-02-01,2026-02-01,quarantined,,V2,public,Quarantined for test.,Synthetic test,,,,,\n",
            SOURCES_CSV
        );
        tree.insert_bytes("corpus/registers/sources.csv", extra_sources.into_bytes());
        let (_model, view) = load_view(tree);
        let planned = site::planned_paths(&view);
        assert!(planned.contains("sources.html"));

        let held_vid = VersionId::parse("S-0004-v1").unwrap();
        let held_row = view
            .sources()
            .iter()
            .find(|s| s.value.source_id.as_str() == "S-0004" && s.value.version == 1)
            .map(|s| &s.value);
        assert!(held_row.is_some());

        // 1. Direct link & citation helpers: quarantined version never falls back to sources.html#S-0004-v1.
        assert_eq!(
            source_version_href("claims.html", &held_vid, held_row, &planned),
            None,
            "quarantined version S-0004-v1 must not fall back to sources.html#S-0004-v1"
        );
        let held_cite = render_citation_chip(
            "claims.html",
            "S-0004-v1",
            Some("sec. 1"),
            held_row,
            &planned,
        );
        assert!(
            !held_cite.contains("<a ") && held_cite.contains("S-0004-v1"),
            "quarantined citation chip must stay plain text: `{held_cite}`"
        );
        let held_chip = render_version_chip("claims.html", &held_vid, held_row, &planned);
        assert!(
            !held_chip.contains("<a ") && held_chip.contains("S-0004-v1"),
            "quarantined version chip must stay plain text: `{held_chip}`"
        );

        // 2. Active version S-0002-v2 DOES link to sources.html#S-0002-v2 alongside it.
        let active_vid = VersionId::parse("S-0002-v2").unwrap();
        let active_row = view
            .sources()
            .iter()
            .find(|s| s.value.source_id.as_str() == "S-0002" && s.value.version == 2)
            .map(|s| &s.value);
        assert_eq!(
            source_version_href("claims.html", &active_vid, active_row, &planned),
            Some("sources.html#S-0002-v2".to_owned())
        );

        // 3. In sources.html and ids.html, S-0004-v1 register metadata row exists (<tr id="S-0004-v1">),
        //    but no <a> link targets #S-0004-v1 or s/S-0004-v1.html, while S-0002-v2 is linked.
        let sources_html = render_main(Register::Sources, &view, &planned);
        assert!(
            sources_html.contains("<tr id=\"S-0004-v1\">"),
            "quarantined version metadata row must remain in sources.html"
        );
        assert!(
            !sources_html.contains("href=\"sources.html#S-0004-v1\"")
                && !sources_html.contains("href=\"#S-0004-v1\"")
                && !sources_html.contains("s/S-0004-v1.html"),
            "sources.html must never link to quarantined version S-0004-v1:\n{sources_html}"
        );
        assert!(
            sources_html.contains("href=\"sources.html#S-0002-v2\""),
            "sources.html must link active version S-0002-v2"
        );

        let ids_html = ids::render_main(&view, &planned);
        assert!(
            ids_html.contains("data-id=\"S-0004-v1\""),
            "ids.html must list S-0004-v1"
        );
        assert!(
            !ids_html.contains("href=\"sources.html#S-0004-v1\"")
                && !ids_html.contains("s/S-0004-v1.html"),
            "ids.html must never link to quarantined version S-0004-v1:\n{ids_html}"
        );
        assert!(
            ids_html.contains("href=\"sources.html#S-0002-v2\""),
            "ids.html must link active version S-0002-v2"
        );

        // 4. Every emitted link across the rendered site passes FB0603.
        let rendered = site::render(&view, &BuildInfo::default()).unwrap();
        let diags = lint(&rendered, &OriginalNames::default());
        assert!(
            diags.is_empty(),
            "rendered site must pass FB0603, got: {diags:?}"
        );
    }

    #[test]
    fn sort_buttons_are_added_by_script_and_set_aria_sort() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        for reg in Register::ALL {
            let html = render_main(reg, &view, &planned);
            assert!(
                !html.contains("th-sort")
                    && !html.contains("<button type=\"button\" data-sort-col"),
                "`{}` ships no sort buttons: the script adds them",
                output_path(reg)
            );
            assert_eq!(
                html.matches("aria-sort=").count(),
                1,
                "`{}` marks only the id column, which the page is sorted by",
                output_path(reg)
            );
        }
        let claims = render_main(Register::Claims, &view, &planned);
        assert!(
            claims.contains("id=\"claimsFilter\"") && claims.contains(" hidden>"),
            "the claims filter stays hidden until the script reveals it"
        );
        for needle in [
            "className = 'th-sort'",
            "setAttribute('aria-sort', nextDir)",
            "removeAttribute('aria-sort')",
            "setAttribute('aria-hidden', 'true')",
            "filterBar.hidden = false",
        ] {
            assert!(APP_JS.contains(needle), "app.js must contain `{needle}`");
        }
    }

    #[test]
    fn headers_have_scope_col_and_never_sticky() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        for reg in Register::ALL {
            let html = render_main(reg, &view, &planned);
            assert!(
                html.contains("<div class=\"table-wrap\">"),
                "`{}` must wrap its table in `.table-wrap`",
                output_path(reg)
            );
            assert!(
                html.contains("<caption>"),
                "`{}` table must include a `<caption>`",
                output_path(reg)
            );
            let th_count = html.matches("<th ").count() + html.matches("<th>").count();
            let scope_col_count = html.matches("<th scope=\"col\"").count();
            assert!(
                th_count > 0 && th_count == scope_col_count,
                "`{}` must put `scope=\"col\"` on every `<th>` (found {scope_col_count} of {th_count})",
                output_path(reg)
            );
            assert!(
                html.contains("aria-sort=\"ascending\""),
                "`{}` must mark the initial sorted column header with `aria-sort=\"ascending\"`",
                output_path(reg)
            );
        }

        // Verify app.css never puts position: sticky on th or thead.
        for block in APP_CSS.split('}') {
            let Some((selectors, decls)) = block.split_once('{') else {
                continue;
            };
            let has_sticky = decls.to_ascii_lowercase().contains("position: sticky")
                || decls.to_ascii_lowercase().contains("position:sticky");
            if !has_sticky {
                continue;
            }
            for sel in selectors.split(',') {
                let tokens: Vec<&str> = sel
                    .split(|c: char| {
                        c.is_ascii_whitespace() || matches!(c, '>' | '+' | '~' | '.' | '#' | '[')
                    })
                    .filter(|s| !s.is_empty())
                    .collect();
                assert!(
                    !tokens
                        .iter()
                        .any(|t| t.eq_ignore_ascii_case("th") || t.eq_ignore_ascii_case("thead")),
                    "app.css must never apply `position: sticky` to `th` or `thead` (found in selector `{}`)",
                    sel.trim()
                );
            }
        }
    }

    #[test]
    fn empty_cell_shows_its_meaning() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        for reg in Register::ALL {
            let reg_view = build_register_view(reg, &view, &planned);
            let expected_cols = if reg == Register::Questions {
                QUESTIONS_CORPUS_COLS
            } else {
                reg.columns().len()
            };
            assert_eq!(reg_view.columns.len(), expected_cols);

            for row in &reg_view.rows {
                assert_eq!(row.cells.len(), expected_cols);
                for cell in &row.cells {
                    let trimmed = cell.html.trim();
                    assert!(
                        !trimmed.is_empty(),
                        "register `{}` row `{}` column `{}` must not be blank",
                        output_path(reg),
                        row.id,
                        cell.col_name
                    );
                    assert!(
                        !trimmed.contains("<span></span>"),
                        "register `{}` row `{}` column `{}` has empty `<span>` in `{trimmed}`",
                        output_path(reg),
                        row.id,
                        cell.col_name
                    );
                }
            }
        }

        let claims_html = render_main(Register::Claims, &view, &planned);
        for expected_meaning in [
            "<span class=\"muted\">An assumption has no source</span>",
            "<span class=\"muted\">No fine locator</span>",
            "<span class=\"muted\">No quote</span>",
            "<span class=\"muted\">Still in force at as_of</span>",
            "<span class=\"muted\">Not a regulatory claim</span>",
            "<span class=\"muted\">Not required for this instrument</span>",
            "<span class=\"muted\">Not scoped</span>",
            "<span class=\"muted\">The claim is not an input to a calculation</span>",
            "<span class=\"muted unver\">Never verified</span>",
            "<span class=\"muted\">Not checked</span>",
            "<span class=\"muted\">Replaces nothing</span>",
            "<span class=\"muted\">Not replaced</span>",
            "<span class=\"muted\">In no conflict</span>",
            "<span class=\"muted\">No question</span>",
        ] {
            assert!(
                claims_html.contains(expected_meaning),
                "claims.html must render empty-cell meaning `{expected_meaning}`"
            );
        }

        let sources_html = render_main(Register::Sources, &view, &planned);
        for expected_meaning in [
            "<span class=\"muted\">Replaces nothing</span>",
            "<span class=\"muted\">Added by hand, or not from a company system</span>",
            "<span class=\"muted\">Not part of a series</span>",
        ] {
            assert!(
                sources_html.contains(expected_meaning),
                "sources.html must render empty-cell meaning `{expected_meaning}`"
            );
        }

        let conflicts_html = render_main(Register::Conflicts, &view, &planned);
        assert!(
            conflicts_html.contains("<span class=\"muted\">Unresolved</span>"),
            "conflicts.html must render empty-cell meaning `Unresolved`"
        );

        let glossary_html = render_main(Register::Glossary, &view, &planned);
        assert!(
            glossary_html.contains("<span class=\"muted\">Not replaced</span>"),
            "glossary.html must render empty-cell meaning `Not replaced` for `superseded_by`"
        );
    }

    #[test]
    fn claims_page_renders() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let navigation = nav::build(&view, &planned);
        let html = render_page(Register::Claims, &view, &ctx, &navigation, &planned);
        insta::assert_snapshot!("claims_page_renders", html);
    }

    #[test]
    fn ids_page_renders() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let navigation = nav::build(&view, &planned);
        let html = ids::render_page(&view, &ctx, &navigation, &planned);
        insta::assert_snapshot!("ids_page_renders", html);
    }
}
