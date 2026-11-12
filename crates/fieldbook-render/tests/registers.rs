//! Integration and snapshot tests for `fieldbook_render::{register, ids, status_chip, links}` (`build-registers`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod registers {
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::ids::{
        AreaCode, ClaimId, ConflictId, QuestionId, SourceId, VersionId, claim_page_route,
        claim_part,
    };
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::schema::{
        Adoption, Applicability, ConflictRow, ConflictStatus, RefId, Register, Verification,
    };
    use fieldbook_render::ids;
    use fieldbook_render::links::{
        first_eligible_version, render_citation_chip, render_source_id_chip, render_version_chip,
        source_id_href, source_version_href,
    };
    use fieldbook_render::lint::lint;
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::nav;
    use fieldbook_render::page;
    use fieldbook_render::register::{
        QUESTIONS_CORPUS_COLS, RegisterRowView, build_claims_part_view,
        build_register_conflict_box, build_register_view, claims_part_paths, claims_parts,
        output_path, render_claims_part_main, render_claims_part_page, render_main, render_page,
    };
    use fieldbook_render::site::{self, PlannedPaths};
    use fieldbook_render::source;
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
        "claims/CMP-1.html",
        "claims/EST-1.html",
        "sources.html",
        "questions.html",
        "conflicts.html",
        "glossary.html",
        "ids.html",
        "ids/pages.html",
        "ids/sources.html",
        "ids/versions.html",
        "ids/questions.html",
        "ids/conflicts.html",
        "ids/terms.html",
        "ids/claims-CMP-1.html",
        "ids/claims-EST-1.html",
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

    fn claims_part_mains(
        view: &LabelledView,
        planned: &PlannedPaths,
    ) -> Vec<(String /*route*/, String /*main html*/)> {
        let parts = claims_parts(view);
        parts
            .iter()
            .enumerate()
            .map(|(idx, part)| {
                (
                    part.route.clone(),
                    render_claims_part_main(&parts, idx, view, planned),
                )
            })
            .collect()
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
        assert!(
            claims_html.contains("class=\"register-view\""),
            "register `claims.html` must render through `class=\"register-view\"`"
        );
        let part_mains = claims_part_mains(&view, &planned);
        for c in view.claims() {
            let needle = format!("<tr id=\"{}\"", c.value.claim_id);
            let route = fieldbook_core::ids::claim_page_route(c.value.claim_id.as_str())
                .expect("valid claim id has route");
            let (_, part_html) = part_mains
                .iter()
                .find(|(r, _)| r == &route)
                .unwrap_or_else(|| panic!("missing part main for `{route}`"));
            assert!(
                part_html.contains(&needle),
                "claims part page `{route}` missing row anchor `{needle}`"
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
        let diags = lint(&rendered, &source::original_names(&view));
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
    fn ids_index_lists_groups_with_counts() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ids_view = ids::build_ids_view(&view, &planned);
        let html = ids::render_main(&view, &planned);

        let expected_slugs = [
            "pages",
            "sources",
            "versions",
            "claims",
            "questions",
            "conflicts",
            "terms",
        ];

        let mut last_pos = 0;
        for slug in expected_slugs {
            let marker = format!("data-id-kind=\"{slug}\"");
            let pos = html
                .find(&marker)
                .unwrap_or_else(|| panic!("group section `{slug}` must be present in ids.html"));
            assert!(
                pos >= last_pos,
                "group section `{slug}` appeared out of order in ids.html"
            );
            last_pos = pos;

            let grp = ids_view
                .groups
                .iter()
                .find(|g| g.slug == slug)
                .unwrap_or_else(|| panic!("group `{slug}` missing in ids_view"));
            let expected_count = if grp.items.is_empty() {
                "None recorded".to_owned()
            } else {
                grp.items.len().to_string()
            };
            let count_needle = format!("<span class=\"panel-sub mono\">{expected_count}</span>");
            assert!(
                html[pos..].contains(&count_needle),
                "group `{slug}` count `{expected_count}` missing in panel-sub span"
            );
        }

        for grp in &ids_view.groups {
            if grp.slug != "claims" {
                let heading_link = format!("<a href=\"ids/{}.html\">{}</a>", grp.slug, grp.title);
                assert!(
                    html.contains(&heading_link),
                    "non-claims group `{}` heading must link to ids/{}.html",
                    grp.slug,
                    grp.slug
                );
            }
        }

        let parts = claims_parts(&view);
        assert!(!parts.is_empty(), "claims parts must be present in fixture");
        for part in &parts {
            let part_href = format!("ids/claims-{}-{}.html", part.area_code, part.part);
            let href_needle = format!("href=\"{part_href}\"");
            assert_eq!(
                html.matches(&href_needle).count(),
                1,
                "claims section must list exactly one link with href `{part_href}`"
            );
        }

        assert!(
            !html.contains("data-id="),
            "no data-id= row of any id appears on the index"
        );
    }

    #[test]
    fn ids_group_pages_hold_every_id() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let list_pages = ids::list_pages(&view, &planned);

        let mut expected_ids = BTreeSet::new();
        for page in view.pages() {
            expected_ids.insert(page.id.as_str().to_owned());
        }
        let distinct_sids: BTreeSet<&SourceId> =
            view.sources().iter().map(|s| &s.value.source_id).collect();
        for sid in distinct_sids {
            expected_ids.insert(sid.as_str().to_owned());
        }
        for s in view.sources() {
            expected_ids.insert(format!("{}-v{}", s.value.source_id, s.value.version));
        }
        for c in view.claims() {
            expected_ids.insert(c.value.claim_id.as_str().to_owned());
        }
        for q in view.questions() {
            expected_ids.insert(q.value.question_id.as_str().to_owned());
        }
        for xc in view.conflicts() {
            expected_ids.insert(xc.value.conflict_id.as_str().to_owned());
        }
        for t in view.glossary() {
            expected_ids.insert(t.value.term_id.as_str().to_owned());
        }

        let rendered: Vec<(String, String)> = list_pages
            .iter()
            .map(|p| (p.path.clone(), ids::render_list_main(p)))
            .collect();

        let mut id_occurrences: BTreeMap<String, usize> = BTreeMap::new();
        for (_path, html) in &rendered {
            let mut cursor = 0;
            while let Some(start) = html[cursor..].find("data-id=\"") {
                let id_start = cursor + start + "data-id=\"".len();
                let end = html[id_start..].find('"').unwrap();
                let id = &html[id_start..id_start + end];
                *id_occurrences.entry(id.to_owned()).or_default() += 1;
                cursor = id_start + end + 1;
            }
        }

        for (id, count) in &id_occurrences {
            assert_eq!(
                *count, 1,
                "id `{id}` must appear on exactly one page, found {count}"
            );
        }

        let union_ids: BTreeSet<String> = id_occurrences.into_keys().collect();
        assert_eq!(
            union_ids, expected_ids,
            "union of data-id values across list pages must equal independent view ids"
        );

        let all_html = rendered
            .iter()
            .map(|(_, h)| h.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        for page in view.pages() {
            assert!(
                all_html.contains(&page.meta.title),
                "page title `{}` must be present in html",
                page.meta.title
            );
        }
        for c in view.claims() {
            assert!(
                all_html.contains(&c.value.statement),
                "claim statement `{}` must be present in html",
                c.value.statement
            );
        }
        for q in view.questions() {
            assert!(
                all_html.contains(&q.value.question),
                "question `{}` must be present in html",
                q.value.question
            );
        }
        for xc in view.conflicts() {
            assert!(
                all_html.contains(&xc.value.topic),
                "conflict topic `{}` must be present in html",
                xc.value.topic
            );
        }
        for t in view.glossary() {
            assert!(
                all_html.contains(&t.value.label),
                "glossary label `{}` must be present in html",
                t.value.label
            );
        }
    }

    #[test]
    fn ids_claims_split_by_area() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let list_pages = ids::list_pages(&view, &planned);

        let group_page_slugs: Vec<&str> = list_pages
            .iter()
            .filter(|p| p.slug != "claims")
            .map(|p| p.slug)
            .collect();
        assert_eq!(
            group_page_slugs,
            vec![
                "pages",
                "sources",
                "versions",
                "questions",
                "conflicts",
                "terms"
            ],
            "the six group pages' slugs must be exactly pages, sources, versions, questions, conflicts, terms"
        );

        let rendered_pages: Vec<(String, &ids::IdListPage, String)> = list_pages
            .iter()
            .map(|p| (p.path.clone(), p, ids::render_list_main(p)))
            .collect();

        for (_path, page, _html) in &rendered_pages {
            if page.slug == "claims" {
                assert!(
                    page.items.len() <= fieldbook_core::ids::CLAIMS_PER_PART as usize,
                    "claims part page `{}` holds {} ids, exceeding CLAIMS_PER_PART {}",
                    page.path,
                    page.items.len(),
                    fieldbook_core::ids::CLAIMS_PER_PART
                );
            }
        }

        for c in view.claims() {
            let (area, part) = claim_part(c.value.claim_id.as_str())
                .unwrap_or_else(|| panic!("claim {} must have area and part", c.value.claim_id));
            let expected_page = format!("ids/claims-{area}-{part}.html");
            let needle = format!("data-id=\"{}\"", c.value.claim_id);

            for (path, _page, html) in &rendered_pages {
                let count = html.matches(&needle).count();
                if path == &expected_page {
                    assert_eq!(
                        count, 1,
                        "claim `{}` must appear exactly once on expected page `{expected_page}`",
                        c.value.claim_id
                    );
                } else {
                    assert_eq!(
                        count, 0,
                        "claim `{}` must not appear on page `{path}`",
                        c.value.claim_id
                    );
                }
            }
        }

        let (area500, part500) = claim_part("CL-CMP-0500").unwrap();
        let path500 = format!("ids/claims-{area500}-{part500}.html");
        assert_eq!(path500, "ids/claims-CMP-1.html");

        let (area501, part501) = claim_part("CL-CMP-0501").unwrap();
        let path501 = format!("ids/claims-{area501}-{part501}.html");
        assert_eq!(path501, "ids/claims-CMP-2.html");
    }

    #[test]
    fn ids_list_pages_link_with_the_ids_directory_prefix() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let navigation = nav::build(&view, &planned);
        let list_pages = ids::list_pages(&view, &planned);

        let pages_page = list_pages
            .iter()
            .find(|p| p.slug == "pages")
            .unwrap_or_else(|| panic!("pages list page must exist"));
        let html = ids::render_list_page(pages_page, &ctx, &navigation, &planned);

        assert!(
            html.contains("href=\"../p/"),
            "page chip href in ids list page must start with `../p/`: {html}"
        );
        assert!(
            html.contains("href=\"../ids.html\""),
            "crumb link to ids.html in ids list page must be `../ids.html`: {html}"
        );
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
            html_with.contains("href=\"../claims/CMP-1.html#CL-CMP-0001\""),
            "when `claims/CMP-1.html` is in PlannedPaths, page claim chips must link to `../claims/CMP-1.html#CL-CMP-0001`, got:\n{html_with}"
        );

        let without_claims_vec: Vec<&str> = FIXED_PLANNED_PATHS
            .iter()
            .copied()
            .filter(|&p| p != "claims/CMP-1.html")
            .collect();
        let without_claims = PlannedPaths::of(&without_claims_vec);
        let html_without = page::render_main(page_entry, &view, &without_claims);
        assert!(
            !html_without.contains("claims/CMP-1.html#CL-CMP-0001")
                && html_without.contains("<span class=\"claim-mark"),
            "when `claims/CMP-1.html` is omitted from PlannedPaths, page claim chips must stay plain text `<span>`, got:\n{html_without}"
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
            claims_html.contains("class=\"register-view\""),
            "claims.html must render through class=\"register-view\""
        );
        let claims_parts_pdf = claims_part_mains(&view_pdf, &planned);
        assert!(
            claims_parts_pdf
                .iter()
                .any(|(_, html)| html.contains("<td>pdf_page_box</td>")),
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
            let rows: Vec<_> = if reg == Register::Claims {
                let parts = claims_parts(&view_pdf);
                (0..parts.len())
                    .flat_map(|idx| build_claims_part_view(&parts, idx, &view_pdf, &planned).rows)
                    .collect()
            } else {
                build_register_view(reg, &view_pdf, &planned).rows
            };
            assert!(!rows.is_empty(), "must have rows to check in {page_name}");
            for row in &rows {
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
            let rows: Vec<_> = if reg == Register::Claims {
                let parts = claims_parts(&view);
                (0..parts.len())
                    .flat_map(|idx| build_claims_part_view(&parts, idx, &view, &planned).rows)
                    .collect()
            } else {
                build_register_view(reg, &view, &planned).rows
            };
            assert!(!rows.is_empty(), "must have rows to check in {page_name}");
            for row in &rows {
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

        // 2. Active version S-0002-v2 DOES link, to its source page now that the build plans one.
        let active_vid = VersionId::parse("S-0002-v2").unwrap();
        let active_row = view
            .sources()
            .iter()
            .find(|s| s.value.source_id.as_str() == "S-0002" && s.value.version == 2)
            .map(|s| &s.value);
        assert_eq!(
            source_version_href("claims.html", &active_vid, active_row, &planned),
            Some("s/S-0002-v2.html".to_owned())
        );

        // 3. In sources.html and ids/versions.html, S-0004-v1 register metadata row exists (<tr id="S-0004-v1">),
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
            sources_html.contains("href=\"s/S-0002-v2.html\""),
            "sources.html must link active version S-0002-v2"
        );

        let list_pages = ids::list_pages(&view, &planned);
        let versions_page = list_pages.iter().find(|p| p.slug == "versions").unwrap();
        let ids_html = ids::render_list_main(versions_page);
        assert!(
            ids_html.contains("data-id=\"S-0004-v1\""),
            "ids/versions.html must list S-0004-v1"
        );
        assert!(
            !ids_html.contains("href=\"sources.html#S-0004-v1\"")
                && !ids_html.contains("s/S-0004-v1.html"),
            "ids/versions.html must never link to quarantined version S-0004-v1:\n{ids_html}"
        );
        assert!(
            ids_html.contains("href=\"../s/S-0002-v2.html\""),
            "ids/versions.html must link active version S-0002-v2"
        );

        // 4. Every emitted link across the rendered site passes FB0603.
        let rendered = site::render(&view, &BuildInfo::default()).unwrap();
        let diags = lint(&rendered, &source::original_names(&view));
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
            if reg == Register::Claims {
                assert!(
                    html.contains("class=\"register-view\""),
                    "claims.html must render through class=\"register-view\""
                );
                let part_mains = claims_part_mains(&view, &planned);
                for (route, part_html) in &part_mains {
                    assert!(
                        !part_html.contains("th-sort")
                            && !part_html.contains("<button type=\"button\" data-sort-col"),
                        "`{route}` ships no sort buttons: the script adds them"
                    );
                    assert_eq!(
                        part_html.matches("aria-sort=").count(),
                        1,
                        "`{route}` marks only the id column, which the page is sorted by"
                    );
                }
            } else {
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
        }
        let part_mains = claims_part_mains(&view, &planned);
        for (route, part_html) in &part_mains {
            assert!(
                part_html.contains("id=\"fb:claimsFilter\"") && part_html.contains(" hidden>"),
                "the `{route}` claims filter stays hidden until the script reveals it"
            );
        }
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
            if reg == Register::Claims {
                assert!(
                    html.contains("class=\"register-view\""),
                    "claims.html must render through class=\"register-view\""
                );
                let part_mains = claims_part_mains(&view, &planned);
                for (route, part_html) in &part_mains {
                    assert!(
                        part_html.contains("<div class=\"table-wrap\">"),
                        "`{route}` must wrap its table in `.table-wrap`"
                    );
                    assert!(
                        part_html.contains("<caption>"),
                        "`{route}` table must include a `<caption>`"
                    );
                    let th_count =
                        part_html.matches("<th ").count() + part_html.matches("<th>").count();
                    let scope_col_count = part_html.matches("<th scope=\"col\"").count();
                    assert!(
                        th_count > 0 && th_count == scope_col_count,
                        "`{route}` must put `scope=\"col\"` on every `<th>` (found {scope_col_count} of {th_count})"
                    );
                    assert!(
                        part_html.contains("aria-sort=\"ascending\""),
                        "`{route}` must mark the initial sorted column header with `aria-sort=\"ascending\"`"
                    );
                }
            } else {
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

            let rows_to_check: Vec<(String, RegisterRowView)> = if reg == Register::Claims {
                let parts = claims_parts(&view);
                (0..parts.len())
                    .flat_map(|idx| {
                        let pv = build_claims_part_view(&parts, idx, &view, &planned);
                        assert_eq!(pv.columns.len(), expected_cols);
                        let route = parts[idx].route.clone();
                        pv.rows.into_iter().map(move |r| (route.clone(), r))
                    })
                    .collect()
            } else {
                reg_view
                    .rows
                    .into_iter()
                    .map(|r| (output_path(reg).to_owned(), r))
                    .collect()
            };

            for (path, row) in &rows_to_check {
                assert_eq!(row.cells.len(), expected_cols);
                for cell in &row.cells {
                    let trimmed = cell.html.trim();
                    // The format reference leaves "Empty means" blank for these columns, so an
                    // empty value is an empty cell and nothing is invented for it.
                    if trimmed.is_empty()
                        && matches!(
                            cell.col_name,
                            "unit" | "citation_codified" | "citation_rfo" | "note"
                        )
                    {
                        continue;
                    }
                    assert!(
                        !trimmed.is_empty(),
                        "register `{path}` row `{}` column `{}` must not be blank",
                        row.id,
                        cell.col_name
                    );
                    assert!(
                        !trimmed.contains("<span></span>"),
                        "register `{path}` row `{}` column `{}` has empty `<span>` in `{trimmed}`",
                        row.id,
                        cell.col_name
                    );
                }
            }
        }

        let claims_parts_html: String = claims_part_mains(&view, &planned)
            .into_iter()
            .map(|(_, h)| h)
            .collect();
        for expected_meaning in [
            "<span class=\"muted\">An assumption has no source</span>",
            "<span class=\"muted\">No fine locator</span>",
            "<span class=\"muted\">No quote</span>",
            "<span class=\"muted\">Still in force at as_of</span>",
            "<span class=\"muted\">Not a regulatory claim</span>",
            "<span class=\"muted\">No such test</span>",
            "<span class=\"muted\">The claim is not an input to a calculation</span>",
            "<span class=\"unver\">Never verified</span>",
            "<span class=\"muted\">Not checked</span>",
            "<span class=\"muted\">Replaces nothing</span>",
            "<span class=\"muted\">Not replaced</span>",
            "<span class=\"muted\">In no conflict</span>",
            "<span class=\"muted\">No question</span>",
        ] {
            assert!(
                claims_parts_html.contains(expected_meaning),
                "claims part pages must render empty-cell meaning `{expected_meaning}`"
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

    /// The first `n` comma-separated fields of every data line of a fixture CSV (ids hold no commas).
    fn csv_lead_fields(csv: &str, n: usize) -> Vec<String> {
        csv.lines()
            .skip(1)
            .filter(|l| !l.is_empty())
            .map(|l| l.split(',').take(n).collect::<Vec<_>>().join("-v"))
            .collect()
    }

    fn row_ids(reg: Register, view: &LabelledView, planned: &PlannedPaths) -> Vec<String> {
        if reg == Register::Claims {
            let parts = claims_parts(view);
            (0..parts.len())
                .flat_map(|idx| {
                    build_claims_part_view(&parts, idx, view, planned)
                        .rows
                        .into_iter()
                        .map(|r| r.id)
                })
                .collect()
        } else {
            build_register_view(reg, view, planned)
                .rows
                .into_iter()
                .map(|r| r.id)
                .collect()
        }
    }

    #[test]
    fn rows_follow_the_format_order() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        for (reg, csv, n) in [
            (Register::Sources, SOURCES_CSV, 2),
            (Register::Claims, CLAIMS_CSV, 1),
            (Register::Questions, QUESTIONS_CSV, 1),
            (Register::Conflicts, CONFLICTS_CSV, 1),
            (Register::Glossary, GLOSSARY_CSV, 1),
        ] {
            let expected = csv_lead_fields(csv, n);
            assert!(
                !expected.is_empty(),
                "fixture `{}` has rows",
                reg.file_name()
            );
            assert_eq!(
                row_ids(reg, &view, &planned),
                expected,
                "`{}` rows follow the register's sort order",
                output_path(reg)
            );
        }
    }

    fn conflicts_tree() -> Tree {
        let mut tree = fixture_tree();
        let mut csv = CONFLICTS_CSV.to_owned();
        csv.push_str(
            "XC-99,cmp,Synthetic resolved topic,Synthetic position C|Synthetic position D,Synthetic adopted rule,Synthetic rationale text,CL-CMP-0001,2026-02-15,resolved,CL-CMP-0001|CL-CMP-0002,\n",
        );
        csv.push_str(
            "XC-100,cmp,Synthetic third topic,Synthetic position A|Synthetic position B,,,,,unresolved,CL-CMP-0001|CL-CMP-0002,Q-001\n",
        );
        tree.insert_bytes("corpus/registers/conflicts.csv", csv.into_bytes());
        tree
    }

    #[test]
    fn conflicts_sort_by_number_and_resolved_box_shows_adoption() {
        let (_model, view) = load_view(conflicts_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);

        assert_eq!(
            row_ids(Register::Conflicts, &view, &planned),
            ["XC-01", "XC-99", "XC-100"],
            "XC-99 sorts before XC-100"
        );
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let navigation = nav::build(&view, &planned);
        let list_pages = ids::list_pages(&view, &planned);
        let conflicts_page = list_pages.iter().find(|p| p.slug == "conflicts").unwrap();
        let ids_html = ids::render_list_page(conflicts_page, &ctx, &navigation, &planned);
        let at = |id: &str| ids_html.find(&format!(">{id}<")).unwrap();
        assert!(
            at("XC-01") < at("XC-99") && at("XC-99") < at("XC-100"),
            "ids/conflicts.html lists conflicts in numeric order"
        );

        let html = render_main(Register::Conflicts, &view, &planned);
        let resolved = html
            .split("<div class=\"conflict-box")
            .find(|b| b.contains("data-conflict=\"XC-99\""))
            .and_then(|b| b.split("</section>").next())
            .unwrap();
        assert!(resolved.contains("Resolved · never averaged"), "{resolved}");
        for needle in [
            "Synthetic adopted rule",
            "Synthetic rationale text",
            "2026-02-15",
        ] {
            assert!(resolved.contains(needle), "resolved box shows `{needle}`");
        }
        assert!(
            resolved.contains("Synthetic position C") && resolved.contains("Synthetic position D"),
            "a resolved conflict still keeps every position"
        );
        let unresolved = html
            .split("<div class=\"conflict-box")
            .find(|b| b.contains("data-conflict=\"XC-100\""))
            .and_then(|b| b.split("</section>").next())
            .unwrap();
        for needle in [
            "Synthetic adopted rule",
            "Synthetic rationale text",
            "Resolved on",
        ] {
            assert!(
                !unresolved.contains(needle),
                "an unresolved box never shows `{needle}`"
            );
        }
    }

    #[test]
    fn empty_ids_page_shows_no_bare_zero() {
        let (_model, view) = load_view(empty_registers_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let navigation = nav::build(&view, &planned);
        let html = ids::render_page(&view, &ctx, &navigation, &planned);
        assert!(html.contains("None recorded"));
        assert!(!html.contains("id-row"), "an empty edition lists no id");
        let main = html.split("class=\"ids-view\"").nth(1).unwrap();
        assert!(!main.contains(">0<") && !main.contains("0 IDs"));
    }

    #[test]
    fn registers_page_in_fifties_and_numbers_sort_numerically() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        for reg in Register::ALL {
            let html = render_main(reg, &view, &planned);
            if reg == Register::Claims {
                assert!(
                    html.contains("class=\"register-view\""),
                    "claims.html must render through class=\"register-view\""
                );
                let part_mains = claims_part_mains(&view, &planned);
                for (route, part_html) in &part_mains {
                    assert!(
                        part_html.contains("id=\"fb:registerPager\"")
                            && part_html.contains("hidden>"),
                        "`{route}` ships the pager hidden: the script shows it past 50 rows"
                    );
                    assert!(
                        part_html.matches("<tr id=").count() > 0,
                        "`{route}` ships every row, so the table is complete without the script"
                    );
                }
            } else {
                assert!(
                    html.contains("id=\"fb:registerPager\"") && html.contains("hidden>"),
                    "`{}` ships the pager hidden: the script shows it past 50 rows",
                    output_path(reg)
                );
                assert!(
                    html.matches("<tr id=").count() > 0,
                    "`{}` ships every row, so the table is complete without the script",
                    output_path(reg)
                );
            }
        }
        for needle in [
            "PAGE_SIZE = 50",
            "const NUMBER = /^-?\\d+(\\.\\d+)?$/",
            "compareDecimal(x, y)",
            "window.addEventListener('hashchange', revealHash)",
        ] {
            assert!(APP_JS.contains(needle), "app.js must contain `{needle}`");
        }
    }

    /// Every `id="..."` attribute of a complete document, in order. A `data-id=` attribute is not one.
    fn id_attributes(html: &str) -> Vec<String> {
        html.split(" id=\"")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap().to_owned())
            .collect()
    }

    const FRAME_NAMED_TERMS: [&str; 9] = [
        "main", "sidebar", "crumbs", "banners", "scrim", "toast", "sr", "reader", "popover",
    ];

    /// A tree whose glossary holds terms named like the ids the page frame and its scripts use.
    fn frame_named_terms_tree() -> Tree {
        let mut tree = fixture_tree();
        let (header, existing) = GLOSSARY_CSV.split_once('\n').unwrap();
        let mut rows: Vec<String> = existing.lines().map(str::to_owned).collect();
        for term in FRAME_NAMED_TERMS {
            rows.push(format!(
                "{term},{term},Synthetic term named like a frame element.,Synthetic boundary.,cmp,,general_practice,,"
            ));
        }
        rows.sort();
        let csv = format!("{header}\n{}\n", rows.join("\n"));
        tree.insert_bytes("corpus/registers/glossary.csv", csv.into_bytes());
        tree
    }

    #[test]
    fn complete_documents_have_unique_ids() {
        let (_model, view) = load_view(frame_named_terms_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let navigation = nav::build(&view, &planned);

        let mut docs: Vec<(String, String)> = Register::ALL
            .iter()
            .map(|&reg| {
                (
                    output_path(reg).to_owned(),
                    render_page(reg, &view, &ctx, &navigation, &planned),
                )
            })
            .collect();
        docs.push((
            "ids.html".to_owned(),
            ids::render_page(&view, &ctx, &navigation, &planned),
        ));
        for page_entry in view.pages() {
            docs.push((
                format!("p/{}.html", page_entry.id),
                page::render_page(page_entry, &view, &ctx, &navigation, &planned),
            ));
        }

        for (name, html) in &docs {
            let ids = id_attributes(html);
            let unique: BTreeSet<&String> = ids.iter().collect();
            assert_eq!(
                unique.len(),
                ids.len(),
                "`{name}` repeats an element id: {ids:?}"
            );
        }

        // The glossary keeps one canonical anchor per term, named by the term id, and the frame
        // elements sit in a namespace no row id can take.
        let glossary = docs.iter().find(|(n, _)| n == "glossary.html").unwrap();
        let ids = id_attributes(&glossary.1);
        for term in [
            "main", "sidebar", "crumbs", "banners", "scrim", "toast", "sr", "reader", "popover",
        ] {
            assert_eq!(
                ids.iter().filter(|i| i.as_str() == term).count(),
                1,
                "`glossary.html#{term}` must be the term row alone"
            );
            assert!(
                glossary.1.contains(&format!("<tr id=\"{term}\"")),
                "term `{term}` keeps its row anchor"
            );
        }
        assert!(
            glossary.1.contains("<main id=\"fb:main\"") && glossary.1.contains("href=\"#fb:main\""),
            "the frame's main landmark and its skip link use the frame namespace"
        );
        // Register and id pages emit only row anchors and namespaced control ids; a content page
        // also carries its part anchors, which sit on pages that have no register rows.
        for (name, html) in docs.iter().filter(|(n, _)| !n.starts_with("p/")) {
            for id in id_attributes(html) {
                let is_row = html.contains(&format!("<tr id=\"{id}\""));
                assert!(
                    is_row || id.starts_with("fb:"),
                    "`{name}` has the unnamespaced control id `{id}`"
                );
            }
        }
        for (name, html) in docs.iter().filter(|(n, _)| n.starts_with("p/")) {
            for id in [
                "main", "sidebar", "reader", "popover", "toast", "scrim", "sr", "crumbs", "banners",
            ] {
                assert!(
                    !id_attributes(html).iter().any(|i| i == id),
                    "`{name}` emits the bare frame id `{id}`"
                );
            }
        }
    }

    /// A Rust port of app.js `parseDecimal`, `compareMagnitude` and `compareDecimal`, step for step.
    fn compare_decimal(x: &str, y: &str) -> i32 {
        struct Dec {
            negative: bool,
            whole: String,
            fraction: String,
        }
        fn parse(text: &str) -> Dec {
            let mut negative = text.starts_with('-');
            let body = text.strip_prefix('-').unwrap_or(text);
            let (whole, fraction) = body.split_once('.').unwrap_or((body, ""));
            let whole = whole.trim_start_matches('0').to_owned();
            let fraction = fraction.trim_end_matches('0').to_owned();
            if whole.is_empty() && fraction.is_empty() {
                negative = false;
            }
            Dec {
                negative,
                whole,
                fraction,
            }
        }
        fn sign(o: std::cmp::Ordering) -> i32 {
            match o {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            }
        }
        fn magnitude(a: &Dec, b: &Dec) -> i32 {
            if a.whole.len() != b.whole.len() {
                return sign(a.whole.len().cmp(&b.whole.len()));
            }
            if a.whole != b.whole {
                return sign(a.whole.cmp(&b.whole));
            }
            let width = a.fraction.len().max(b.fraction.len());
            let fa = format!("{:0<width$}", a.fraction);
            let fb = format!("{:0<width$}", b.fraction);
            sign(fa.cmp(&fb))
        }
        let (a, b) = (parse(x), parse(y));
        if a.negative != b.negative {
            return if a.negative { -1 } else { 1 };
        }
        let m = magnitude(&a, &b);
        if a.negative && m != 0 { -m } else { m }
    }

    /// The `COMPARE-CASES` table embedded in app.js: `x | y | expected`.
    fn compare_cases() -> Vec<(String, String, i32)> {
        let table = APP_JS
            .split("COMPARE-CASES\n")
            .nth(1)
            .and_then(|t| t.split("END-COMPARE-CASES").next())
            .unwrap();
        table
            .lines()
            .filter_map(|l| l.trim().strip_prefix("//"))
            .filter(|l| l.contains('|'))
            .map(|l| {
                let f: Vec<&str> = l.split('|').map(str::trim).collect();
                assert_eq!(f.len(), 3, "case row `{l}`");
                (f[0].to_owned(), f[1].to_owned(), f[2].parse().unwrap())
            })
            .collect()
    }

    #[test]
    fn numeric_sort_compares_decimal_strings_exactly() {
        let cases = compare_cases();
        assert!(cases.len() >= 15, "the case table is the shared evidence");
        for (x, y, expected) in &cases {
            assert_eq!(
                compare_decimal(x, y),
                *expected,
                "app.js case `{x}` vs `{y}`"
            );
            assert_eq!(
                compare_decimal(y, x),
                -*expected,
                "app.js case is antisymmetric: `{y}` vs `{x}`"
            );
        }
        // The point of the exact comparator: these pairs are one and the same double.
        let collapsing: Vec<_> = cases
            .iter()
            .filter(|(x, y, e)| {
                *e != 0
                    && x.parse::<f64>().unwrap().to_bits() == y.parse::<f64>().unwrap().to_bits()
            })
            .collect();
        assert!(
            collapsing.len() >= 3,
            "the table holds distinct decimals that collapse to the same f64, got {collapsing:?}"
        );

        // The port agrees with exact integer arithmetic on scaled decimals.
        let scaled = |n: i128| {
            let sign = if n < 0 { "-" } else { "" };
            let a = n.abs();
            format!("{sign}{}.{:06}", a / 1_000_000, a % 1_000_000)
        };
        let samples: [i128; 12] = [
            0,
            1,
            -1,
            999_999,
            1_000_000,
            -1_000_001,
            123_456_789,
            -123_456_789,
            10_000_000,
            -10_000_000,
            5,
            -5,
        ];
        for &a in &samples {
            for &b in &samples {
                assert_eq!(
                    compare_decimal(&scaled(a), &scaled(b)),
                    match a.cmp(&b) {
                        std::cmp::Ordering::Less => -1,
                        std::cmp::Ordering::Equal => 0,
                        std::cmp::Ordering::Greater => 1,
                    },
                    "{} vs {}",
                    scaled(a),
                    scaled(b)
                );
            }
        }

        // app.js really does what the port does, and no longer converts to Number.
        for needle in [
            "const compareDecimal = function (x, y)",
            "replace(/^0+/, '')",
            "replace(/0+$/, '')",
            "a.whole.length !== b.whole.length",
            "padEnd(width, '0')",
            "return compareDecimal(x, y);",
        ] {
            assert!(APP_JS.contains(needle), "app.js must contain `{needle}`");
        }
        assert!(
            !APP_JS.contains("Number(x)") && !APP_JS.contains("Number(y)"),
            "numeric sort must not convert cells to doubles"
        );
    }

    /// The shipped comparator, pinned verbatim (whitespace aside). `compare_decimal` above is its
    /// port; when this changes, re-check the port and the case table against it.
    const PINNED_COMPARATOR: &str = r#"const parseDecimal = function (text) {
  let negative = text.charAt(0) === '-';
  const body = negative ? text.slice(1) : text;
  const dot = body.indexOf('.');
  const whole = (dot < 0 ? body : body.slice(0, dot)).replace(/^0+/, '');
  const fraction = dot < 0 ? '' : body.slice(dot + 1).replace(/0+$/, '');
  if (whole === '' && fraction === '') {
    negative = false;
  }
  return { negative: negative, whole: whole, fraction: fraction };
};
const compareMagnitude = function (a, b) {
  if (a.whole.length !== b.whole.length) {
    return a.whole.length < b.whole.length ? -1 : 1;
  }
  if (a.whole !== b.whole) {
    return a.whole < b.whole ? -1 : 1;
  }
  const width = Math.max(a.fraction.length, b.fraction.length);
  const fa = a.fraction.padEnd(width, '0');
  const fb = b.fraction.padEnd(width, '0');
  return fa === fb ? 0 : fa < fb ? -1 : 1;
};
const compareDecimal = function (x, y) {
  const a = parseDecimal(x);
  const b = parseDecimal(y);
  if (a.negative !== b.negative) {
    return a.negative ? -1 : 1;
  }
  const magnitude = compareMagnitude(a, b);
  return a.negative && magnitude !== 0 ? -magnitude : magnitude;
};"#;

    #[test]
    fn numeric_comparator_is_pinned() {
        let squash = |t: &str| t.split_whitespace().collect::<Vec<_>>().join(" ");
        let start = APP_JS.find("const parseDecimal = function").unwrap();
        let end = APP_JS.find("// Other cells compare as text").unwrap();
        assert_eq!(
            squash(&APP_JS[start..end]),
            squash(PINNED_COMPARATOR),
            "app.js comparator changed: update the Rust port `compare_decimal`, the case table \
             and this pin together"
        );
    }

    #[test]
    fn script_never_looks_up_a_bare_frame_id() {
        // Every id the script reads is in the frame namespace or is a content anchor prefix.
        for part in APP_JS.split("getElementById(").skip(1) {
            let arg = part.split(')').next().unwrap();
            assert!(
                arg.starts_with("'fb:") || arg.starts_with("'cite-") || arg.starts_with("'ev-"),
                "app.js looks up an id outside the frame namespace: getElementById({arg})"
            );
        }
        assert!(
            !APP_JS.contains("document.getElementById(id)"),
            "revealHash must look up the register row, not any element with that id"
        );
        assert!(
            APP_JS.contains("rowById"),
            "revealHash finds the row among the table's own rows"
        );
    }

    /// The text of `name`'s `const name = function` body in app.js, up to its closing `  };`.
    fn script_function(name: &str) -> &'static str {
        let head = format!("const {name} = function");
        let start = APP_JS
            .find(&head)
            .unwrap_or_else(|| panic!("app.js defines {name}"));
        let rest = &APP_JS[start..];
        let end = rest
            .find("\n  };\n")
            .unwrap_or_else(|| panic!("{name} ends"));
        &rest[..end]
    }

    #[test]
    fn reveal_hash_resolves_only_register_rows() {
        let body = script_function("revealHash");
        assert!(
            body.contains("rowById(id)"),
            "revealHash finds the row among the table's rows"
        );
        for lookup in [
            "querySelector",
            "getElementById",
            "document.all",
            "closest(",
            "getElementsBy",
            "elementFromPoint",
        ] {
            assert!(
                !body.contains(lookup),
                "revealHash must not look an id up outside the rows: `{lookup}`"
            );
        }
        // A second line of defence: a row that is not among the matching rows never sets a page.
        assert!(
            body.contains("const at = matching.indexOf(row);")
                && body.contains("if (at < 0) {")
                && body.contains("page = Math.floor(at / PAGE_SIZE);"),
            "revealHash returns when the row is not in the matching rows after the filter reset"
        );
        let render = script_function("render");
        assert!(
            render.contains("if (page < 0) {"),
            "render clamps the page from below as well as above"
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
    fn claims_part_page_renders() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let navigation = nav::build(&view, &planned);
        let parts = claims_parts(&view);
        let cmp_index = parts
            .iter()
            .position(|p| p.route == "claims/CMP-1.html")
            .expect("claims/CMP-1.html part present");
        let html = render_claims_part_page(&parts, cmp_index, &view, &ctx, &navigation, &planned);
        insta::assert_snapshot!("claims_part_page_renders", html);
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

    fn resolve_relative_site_path(base_file: &str, file_part: &str) -> String {
        if file_part.is_empty() {
            return base_file.to_owned();
        }
        let mut parts: Vec<&str> = Vec::new();
        if let Some(parent) = Path::new(base_file).parent() {
            for comp in parent.components() {
                if let std::path::Component::Normal(c) = comp
                    && let Some(s) = c.to_str()
                {
                    parts.push(s);
                }
            }
        }
        for comp in Path::new(file_part).components() {
            match comp {
                std::path::Component::ParentDir => {
                    parts.pop();
                }
                std::path::Component::Normal(c) => {
                    if let Some(s) = c.to_str() {
                        parts.push(s);
                    }
                }
                _ => {}
            }
        }
        parts.join("/")
    }

    #[test]
    fn every_claim_is_on_exactly_one_part_page() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let part_mains = claims_part_mains(&view, &planned);
        let index_main = render_main(Register::Claims, &view, &planned);

        assert!(!view.claims().is_empty(), "fixture view must have claims");
        for c in view.claims() {
            let id = c.value.claim_id.as_str();
            let needle = format!("<tr id=\"{id}\"");

            assert!(
                !index_main.contains(&needle),
                "claim {id} must not appear in the claims index main"
            );

            let matching_parts: Vec<&str> = part_mains
                .iter()
                .filter(|(_, html)| html.contains(&needle))
                .map(|(route, _)| route.as_str())
                .collect();

            assert_eq!(
                matching_parts.len(),
                1,
                "claim {id} must appear on exactly one part page, found on {matching_parts:?}"
            );

            for other_reg in [
                Register::Sources,
                Register::Questions,
                Register::Conflicts,
                Register::Glossary,
            ] {
                let other_main = render_main(other_reg, &view, &planned);
                assert!(
                    !other_main.contains(&needle),
                    "claim {id} must not appear in {}",
                    output_path(other_reg)
                );
            }
        }
    }

    #[test]
    fn the_claims_index_links_every_part_page() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let index_main = render_main(Register::Claims, &view, &planned);

        assert!(
            index_main.contains("href=\"claims/CMP-1.html\""),
            "index main must link claims/CMP-1.html"
        );
        assert!(
            index_main.contains("href=\"claims/EST-1.html\""),
            "index main must link claims/EST-1.html"
        );

        for route in claims_part_paths(&view) {
            let expected_href = format!("href=\"{route}\"");
            assert!(
                index_main.contains(&expected_href),
                "index main must link part route `{route}`"
            );
        }

        assert!(
            index_main.contains("2 claims"),
            "index main must list CMP part count `2 claims`"
        );
        assert!(
            index_main.contains("3 claims"),
            "index main must list EST part count `3 claims`"
        );

        let empty_planned = PlannedPaths::default();
        let unlinked_index_main = render_main(Register::Claims, &view, &empty_planned);
        assert!(
            !unlinked_index_main.contains("<a"),
            "with empty planned set, labels must render as plain text without `<a`"
        );
        for part in claims_parts(&view) {
            let expected_label = format!("{} to {}", part.first_id, part.last_id);
            assert!(
                unlinked_index_main.contains(&expected_label),
                "unlinked index main must still contain part label `{expected_label}`"
            );
        }
    }

    fn assert_claim_links_land_on_their_part(site: &site::Site) -> usize {
        let mut claim_links_found = 0usize;

        for (from_path, bytes) in site.files() {
            if !from_path.ends_with(".html") {
                continue;
            }
            let html = std::str::from_utf8(bytes).unwrap();
            for part in html.split("href=\"").skip(1) {
                let Some(href) = part.split('"').next() else {
                    continue;
                };
                if let Some((file_part, anchor)) = href.split_once('#')
                    && anchor.starts_with("CL-")
                {
                    claim_links_found += 1;
                    let target_path = resolve_relative_site_path(from_path, file_part);
                    let target_bytes = site.files().get(&target_path).unwrap_or_else(|| {
                        panic!(
                            "claim link in `{from_path}` to `{href}` resolves to missing file `{target_path}`"
                        )
                    });
                    let target_html = std::str::from_utf8(target_bytes).unwrap();
                    let anchor_needle = format!("id=\"{anchor}\"");
                    assert!(
                        target_html.contains(&anchor_needle),
                        "target file `{target_path}` missing anchor `{anchor_needle}` (linked from `{from_path}` via `{href}`)"
                    );
                    let expected_route = claim_page_route(anchor).unwrap_or_else(|| {
                        panic!("claim id `{anchor}` must have a claim page route")
                    });
                    assert_eq!(
                        target_path, expected_route,
                        "claim link in `{from_path}` to `{href}` landed on `{target_path}`, expected `{expected_route}`"
                    );
                }
            }
        }

        claim_links_found
    }

    #[test]
    fn every_claim_link_resolves_to_an_existing_page_and_anchor() {
        let (_model, view) = load_view(fixture_tree());
        let site = site::render(&view, &BuildInfo::default()).unwrap();
        let claim_links_found = assert_claim_links_land_on_their_part(&site);
        assert!(
            claim_links_found > 0,
            "must find at least one claim link, found {claim_links_found}"
        );
    }

    #[test]
    fn the_part_of_a_claim_equals_claim_part() {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(FIXED_PLANNED_PATHS);
        let parts = claims_parts(&view);
        assert!(!parts.is_empty(), "fixture must have claims parts");

        for (idx, part) in parts.iter().enumerate() {
            let main_html = render_claims_part_main(&parts, idx, &view, &planned);
            let mut row_ids = Vec::new();
            for piece in main_html.split("<tr id=\"").skip(1) {
                if let Some(id) = piece.split('"').next() {
                    row_ids.push(id);
                }
            }
            assert!(
                !row_ids.is_empty(),
                "part page `{}` must have rendered claim rows",
                part.route
            );
            for id in row_ids {
                assert_eq!(
                    claim_part(id),
                    Some((part.area_code.clone(), part.part)),
                    "claim_part({id}) must match ({}, {})",
                    part.area_code,
                    part.part
                );
                assert_eq!(
                    Some(part.route.clone()),
                    claim_page_route(id),
                    "part.route must match claim_page_route({id})"
                );
            }
        }
    }

    const CONTINUOUS_CLAIM_IDS: [&str; 6] = [
        "CL-CMP-0001",
        "CL-CMP-0500",
        "CL-CMP-0501",
        "CL-CMP-1000",
        "CL-CMP-1001",
        "CL-CMP-1501",
    ];

    fn make_claim_row(id: &str) -> String {
        let first_row = CLAIMS_CSV.lines().nth(1).unwrap();
        let mut cols: Vec<&str> = first_row.split(',').collect();
        cols[0] = id;
        cols.join(",")
    }

    fn continuous_view() -> (Model, LabelledView) {
        let mut tree_continuous = empty_registers_tree();
        tree_continuous.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree_continuous.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);

        let claims_header = CLAIMS_CSV.lines().next().unwrap();
        let mut continuous_csv = format!("{claims_header}\n");
        for id in CONTINUOUS_CLAIM_IDS {
            continuous_csv.push_str(&make_claim_row(id));
            continuous_csv.push('\n');
        }
        tree_continuous.insert_bytes("corpus/registers/claims.csv", continuous_csv.into_bytes());
        load_view(tree_continuous)
    }

    #[test]
    fn many_claims_split_into_parts_by_number() {
        let claims_header = CLAIMS_CSV.lines().next().unwrap();

        // Continuous variant: 0001, 0500, 0501, 1000, 1001, 1501 giving parts 1, 2, 3, 4
        let (_model_c, view_c) = continuous_view();
        let parts_c = claims_parts(&view_c);
        let routes_c: Vec<&str> = parts_c.iter().map(|p| p.route.as_str()).collect();
        assert_eq!(
            routes_c,
            vec![
                "claims/CMP-1.html",
                "claims/CMP-2.html",
                "claims/CMP-3.html",
                "claims/CMP-4.html",
            ]
        );
        assert_eq!((parts_c[0].ordinal, parts_c[0].of), (1, 4));
        assert_eq!((parts_c[1].ordinal, parts_c[1].of), (2, 4));
        assert_eq!((parts_c[2].ordinal, parts_c[2].of), (3, 4));
        assert_eq!((parts_c[3].ordinal, parts_c[3].of), (4, 4));

        // Variant with a gap: 0001, 0501, 1501 giving parts 1, 2, 4 with part 3 missing
        let mut tree_gap = empty_registers_tree();
        tree_gap.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree_gap.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);

        let gap_ids = ["CL-CMP-0001", "CL-CMP-0501", "CL-CMP-1501"];
        let mut gap_csv = format!("{claims_header}\n");
        for id in gap_ids {
            gap_csv.push_str(&make_claim_row(id));
            gap_csv.push('\n');
        }
        tree_gap.insert_bytes("corpus/registers/claims.csv", gap_csv.into_bytes());
        let (_model_gap, view_gap) = load_view(tree_gap);
        let parts_gap = claims_parts(&view_gap);
        let routes_gap: Vec<&str> = parts_gap.iter().map(|p| p.route.as_str()).collect();
        assert_eq!(
            routes_gap,
            vec![
                "claims/CMP-1.html",
                "claims/CMP-2.html",
                "claims/CMP-4.html",
            ]
        );
        assert_eq!((parts_gap[0].ordinal, parts_gap[0].of), (1, 3));
        assert_eq!((parts_gap[1].ordinal, parts_gap[1].of), (2, 3));
        assert_eq!((parts_gap[2].ordinal, parts_gap[2].of), (3, 3));

        assert!(
            parts_gap.iter().all(|p| p.route != "claims/CMP-3.html"),
            "there must be no claims/CMP-3.html route in parts"
        );
        assert!(
            !claims_part_paths(&view_gap).contains(&"claims/CMP-3.html".to_owned()),
            "claims_part_paths must not contain claims/CMP-3.html"
        );

        let planned_gap = site::planned_paths(&view_gap);
        let ctx_gap = site::render_context(&view_gap, &BuildInfo::default()).unwrap();
        let nav_gap = nav::build(&view_gap, &planned_gap);

        // part page titles read `Claims: <area name>, part 3 of 3` for route claims/CMP-4.html
        let part4_view = build_claims_part_view(&parts_gap, 2, &view_gap, &planned_gap);
        assert_eq!(part4_view.title, "Claims: Compliance, part 3 of 3");
        let part4_page =
            render_claims_part_page(&parts_gap, 2, &view_gap, &ctx_gap, &nav_gap, &planned_gap);
        assert!(
            part4_page.contains("<title>Claims: Compliance, part 3 of 3</title>"),
            "part 4 page <title> must read `Claims: Compliance, part 3 of 3`"
        );
        assert!(
            part4_page.contains("<h1>Claims: Compliance, part 3 of 3</h1>"),
            "part 4 page <h1> must read `Claims: Compliance, part 3 of 3`"
        );

        // prev/next links on claims/CMP-2.html point to `CMP-1.html` and `CMP-4.html` (same folder relative)
        let part2_view = build_claims_part_view(&parts_gap, 1, &view_gap, &planned_gap);
        let part2_nav = part2_view.part_nav.as_ref().expect("part_nav present");
        let prev = part2_nav.prev.as_ref().expect("prev link present");
        assert_eq!(prev.0, "CMP-1.html");
        let next = part2_nav.next.as_ref().expect("next link present");
        assert_eq!(next.0, "CMP-4.html");

        let part2_main = render_claims_part_main(&parts_gap, 1, &view_gap, &planned_gap);
        assert!(
            part2_main.contains("href=\"CMP-1.html\""),
            "part 2 page must link prev to CMP-1.html"
        );
        assert!(
            part2_main.contains("href=\"CMP-4.html\""),
            "part 2 page must link next to CMP-4.html"
        );

        // the index lists the three id ranges (e.g. `CL-CMP-0001 to CL-CMP-0001`)
        let index_main = render_main(Register::Claims, &view_gap, &planned_gap);
        assert!(
            index_main.contains("CL-CMP-0001 to CL-CMP-0001"),
            "index must list first id range `CL-CMP-0001 to CL-CMP-0001`"
        );
        assert!(
            index_main.contains("CL-CMP-0501 to CL-CMP-0501"),
            "index must list second id range `CL-CMP-0501 to CL-CMP-0501`"
        );
        assert!(
            index_main.contains("CL-CMP-1501 to CL-CMP-1501"),
            "index must list third id range `CL-CMP-1501 to CL-CMP-1501`"
        );

        // and the part page's asset links start with `../assets/`
        let mut asset_link_count = 0usize;
        for attr in ["href=\"", "src=\""] {
            for piece in part4_page.split(attr).skip(1) {
                let val = piece.split('"').next().unwrap();
                if val.contains("assets/") {
                    asset_link_count += 1;
                    assert!(
                        val.starts_with("../assets/"),
                        "asset link `{val}` in claims part page must start with `../assets/`"
                    );
                }
            }
        }
        assert!(
            asset_link_count > 0,
            "part page must have at least one asset link"
        );
    }

    #[test]
    fn ids_one_id_part_page_reads_one_id() {
        let mut tree = empty_registers_tree();
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);

        let claims_header = CLAIMS_CSV.lines().next().unwrap();
        let mut claims_csv = format!("{claims_header}\n");
        claims_csv.push_str(&make_claim_row("CL-CMP-0501"));
        claims_csv.push('\n');
        tree.insert_bytes("corpus/registers/claims.csv", claims_csv.into_bytes());

        let (_model, view) = load_view(tree);
        let planned = site::planned_paths(&view);
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let nav = nav::build(&view, &planned);

        let list_pages = ids::list_pages(&view, &planned);
        let page = list_pages
            .iter()
            .find(|p| p.path == "ids/claims-CMP-2.html")
            .unwrap();
        let html = ids::render_list_page(page, &ctx, &nav, &planned);
        assert!(html.contains("<span class=\"chip mono\">1 ID</span>"));
        assert!(!html.contains("1 IDs"));
        assert!(!html.contains("1 claims"));
        assert!(!html.contains("class=\"muted\">The "));

        let pages_page = list_pages
            .iter()
            .find(|p| p.path == "ids/pages.html")
            .unwrap();
        let pages_html = ids::render_list_page(pages_page, &ctx, &nav, &planned);
        assert!(!pages_html.contains("<p class=\"muted\">All "));
    }

    #[test]
    fn parts_hold_exactly_their_own_claim_rows() {
        let (_model, view) = continuous_view();
        let planned = site::planned_paths(&view);
        let part_mains = claims_part_mains(&view, &planned);

        let mut page_rows = BTreeMap::new();
        for (route, html) in &part_mains {
            let mut ids = BTreeSet::new();
            for piece in html.split("<tr id=\"").skip(1) {
                if let Some(id) = piece.split('"').next() {
                    ids.insert(id.to_owned());
                }
            }
            page_rows.insert(route.as_str(), ids);
        }

        assert_eq!(
            page_rows.len(),
            4,
            "must render exactly 4 claims part pages"
        );
        assert_eq!(
            page_rows.get("claims/CMP-1.html"),
            Some(&BTreeSet::from([
                "CL-CMP-0001".to_owned(),
                "CL-CMP-0500".to_owned(),
            ]))
        );
        assert_eq!(
            page_rows.get("claims/CMP-2.html"),
            Some(&BTreeSet::from([
                "CL-CMP-0501".to_owned(),
                "CL-CMP-1000".to_owned(),
            ]))
        );
        assert_eq!(
            page_rows.get("claims/CMP-3.html"),
            Some(&BTreeSet::from(["CL-CMP-1001".to_owned()]))
        );
        assert_eq!(
            page_rows.get("claims/CMP-4.html"),
            Some(&BTreeSet::from(["CL-CMP-1501".to_owned()]))
        );

        let sum_of_sizes: usize = page_rows.values().map(BTreeSet::len).sum();
        let union: BTreeSet<&String> = page_rows.values().flat_map(|s| s.iter()).collect();
        assert_eq!(sum_of_sizes, 6, "sum of set sizes must equal 6");
        assert_eq!(union.len(), 6, "union must have 6 elements");
    }

    #[test]
    fn multi_part_site_places_every_claim_and_link_on_the_right_part() {
        let (_model, view) = continuous_view();
        let site = site::render(&view, &BuildInfo::default()).unwrap();

        let part_pages: Vec<(&str, &str)> = site
            .files()
            .iter()
            .filter(|(path, _)| path.starts_with("claims/") && path.ends_with(".html"))
            .map(|(path, bytes)| (path.as_str(), std::str::from_utf8(bytes).unwrap()))
            .collect();

        for id in CONTINUOUS_CLAIM_IDS {
            let needle = format!("<tr id=\"{id}\"");
            let matching: Vec<&str> = part_pages
                .iter()
                .filter(|(_, html)| html.contains(&needle))
                .map(|(path, _)| *path)
                .collect();
            assert_eq!(
                matching.len(),
                1,
                "claim `{id}` must appear on exactly one claims part page, found on {matching:?}"
            );
            let found_path = matching[0];
            let expected_route = claim_page_route(id).expect("claim has page route");
            assert_eq!(
                found_path, expected_route,
                "claim `{id}` found on `{found_path}`, expected route `{expected_route}`"
            );
            let (expected_area, expected_part) = claim_part(id).expect("claim has part");
            let expected_part_route = format!("claims/{expected_area}-{expected_part}.html");
            assert_eq!(
                found_path, expected_part_route,
                "claim `{id}` found on `{found_path}`, expected part ({expected_area}, {expected_part})"
            );
        }

        for (path, bytes) in site.files() {
            if !path.ends_with(".html") || (path.starts_with("claims/") && path.ends_with(".html"))
            {
                continue;
            }
            let html = std::str::from_utf8(bytes).unwrap();
            assert!(
                !html.contains("<tr id=\"CL-"),
                "file `{path}` must not contain `<tr id=\"CL-` row"
            );
        }

        for (part, id) in [
            (1, "CL-CMP-0001"),
            (2, "CL-CMP-0501"),
            (3, "CL-CMP-1001"),
            (4, "CL-CMP-1501"),
        ] {
            let part_path = format!("ids/claims-CMP-{part}.html");
            let part_bytes = site
                .files()
                .get(&part_path)
                .unwrap_or_else(|| panic!("{part_path} exists"));
            let part_html = std::str::from_utf8(part_bytes).unwrap();
            let expected_link = format!("href=\"../claims/CMP-{part}.html#{id}\"");
            assert!(
                part_html.contains(&expected_link),
                "{part_path} must contain link to ../claims/CMP-{part}.html#{id}"
            );
        }

        let claim_links_found = assert_claim_links_land_on_their_part(&site);
        assert!(
            claim_links_found >= 6,
            "must find at least 6 claim links, found {claim_links_found}"
        );
    }

    #[test]
    fn rendering_is_deterministic() {
        let (_model, view) = load_view(fixture_tree());
        let site1 = site::render(&view, &BuildInfo::default()).unwrap();
        let site2 = site::render(&view, &BuildInfo::default()).unwrap();

        assert_eq!(
            site1.files().get("claims.html"),
            site2.files().get("claims.html"),
            "claims.html must render deterministically"
        );

        let mut claims_parts_found = 0usize;
        for (path, bytes1) in site1.files() {
            if path.starts_with("claims/") {
                claims_parts_found += 1;
                let bytes2 = site2.files().get(path).unwrap_or_else(|| {
                    panic!("path `{path}` missing in second rendered site");
                });
                assert_eq!(bytes1, bytes2, "byte mismatch on part page `{path}`");
            }
        }
        assert!(
            claims_parts_found > 0,
            "must have rendered at least one claims/ part page, found {claims_parts_found}"
        );
        assert_eq!(
            site1.files(),
            site2.files(),
            "full site files must be identical"
        );
    }
}
