//! Integration tests for `fieldbook_render::source` and `fieldbook_render::cite` (`build-source-pages`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod source {
    use std::collections::BTreeSet;

    use fieldbook_core::audit::{Now, Sha256};
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Code, Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::ids::VersionId;
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::cite::{OriginalAvailability, first_pdf_page, located_original_href};
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::links::{render_version_chip, source_version_href};
    use fieldbook_render::lint::{OriginalNames, lint};
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::nav::format_not_available;
    use fieldbook_render::site::{self, PlannedPaths, Site};
    use fieldbook_render::source::{
        held_versions, original_names, page_path, page_versions, shipped_name, shipped_originals,
    };

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

    const PDF_BYTES: &[u8] = b"%PDF-1.4\nsynthetic original, made up for tests\n";
    const HTML_BYTES: &[u8] = b"<html><body>synthetic original, made up for tests</body></html>\n";

    /// One synthetic source-version row of the sources register (28 columns).
    fn row(
        id: &str,
        version: u32,
        location: &str,
        sha256: &str,
        lifecycle: &str,
        supersedes: &str,
        note: &str,
    ) -> String {
        format!(
            "S-{id},{version},synthetic-{id},\"Synthetic Source {id} v{version}, made up for tests\",Synthetic Board,primary_law,primary_evidence,A,\"Synthetic, no restrictions\",,,{location},{sha256},2026-01-01,,2026-02-01,2026-02-01,{lifecycle},{supersedes},V2,public,{note},Synthetic test,,,,,\n"
        )
    }

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes).as_str().to_owned()
    }

    /// The fixture tree with `extra_rows` appended to the sources register and `extra_files`
    /// stored under `corpus/sources/public/`.
    fn tree_with(extra_rows: &str, extra_files: &[(&str, &[u8])]) -> Tree {
        let mut tree = Tree::new();
        for dir in [
            "corpus",
            "corpus/eval",
            "corpus/registers",
            "corpus/sources",
            "corpus/sources/public",
            "corpus/pages",
            "corpus/pages/compliance",
            "corpus/pages/compliance/business-systems",
        ] {
            tree.insert_dir(dir);
        }
        tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", CHANGES_MD.as_bytes());
        tree.insert_bytes(
            "corpus/registers/sources.csv",
            format!("{SOURCES_CSV}{extra_rows}").as_bytes(),
        );
        tree.insert_bytes("corpus/registers/claims.csv", CLAIMS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            CMP_SYSTEM_REVIEW_MD.as_bytes(),
        );
        for (name, bytes) in extra_files {
            tree.insert_bytes(format!("corpus/sources/public/{name}"), *bytes);
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

    fn render_site(view: &LabelledView) -> Site {
        site::render(view, &BuildInfo::default()).unwrap()
    }

    fn page_text(site: &Site, vid: &str) -> String {
        let bytes = site
            .files()
            .get(&page_path(vid))
            .unwrap_or_else(|| panic!("no page for {vid}"));
        String::from_utf8(bytes.clone()).unwrap()
    }

    fn s_pages(site: &Site) -> Vec<String> {
        site.files()
            .keys()
            .filter(|k| k.starts_with("s/"))
            .cloned()
            .collect()
    }

    #[test]
    fn one_page_per_version() {
        let (_model, view) = load_view(tree_with("", &[]));
        let site = render_site(&view);
        let mut expected: Vec<String> = view
            .sources()
            .iter()
            .map(|s| format!("s/{}-v{}.html", s.value.source_id, s.value.version))
            .collect();
        expected.sort();
        assert_eq!(expected.len(), 4, "the fixture has four version rows");
        assert_eq!(s_pages(&site), expected);
        let planned = site::planned_paths(&view);
        for path in &expected {
            assert!(planned.contains(path), "{path} must be planned");
        }
    }

    #[test]
    fn renderable_original_ships_as_txt() {
        for ext in ["html", "htm", "svg", "xhtml", "HTML", "Svg"] {
            assert_eq!(
                shipped_name("S-0042-v2", ext),
                format!("S-0042-v2.{ext}.txt"),
                "the .{ext} case must gain .txt"
            );
        }
        for ext in ["pdf", "txt", "csv", "xlsx", "tar.gz"] {
            assert_eq!(
                shipped_name("S-0042-v2", ext),
                format!("S-0042-v2.{ext}"),
                "the .{ext} case must keep its name"
            );
        }

        let rows = row(
            "0004",
            1,
            "S-0004-v1.html",
            &sha(HTML_BYTES),
            "active",
            "",
            "",
        ) + &row(
            "0005",
            1,
            "S-0005-v1.pdf",
            &sha(PDF_BYTES),
            "active",
            "",
            "",
        );
        let (_model, view) = load_view(tree_with(
            &rows,
            &[("S-0004-v1.html", HTML_BYTES), ("S-0005-v1.pdf", PDF_BYTES)],
        ));
        let names = original_names(&view);
        assert!(names.0.contains("S-0004-v1.html.txt"));
        assert!(names.0.contains("S-0005-v1.pdf"));
        assert!(names.0.contains("S-0002-v2.txt"));
        assert_eq!(names.0.len(), 3);

        let site = render_site(&view);
        assert!(page_text(&site, "S-0004-v1").contains("../../originals/S-0004-v1.html.txt"));
        assert!(page_text(&site, "S-0005-v1").contains("../../originals/S-0005-v1.pdf"));
        let diags = lint(&site, &names);
        assert!(diags.is_empty(), "no lint finding expected: {diags:?}");
    }

    #[test]
    fn metadata_only_has_no_link() {
        let (_model, view) = load_view(tree_with("", &[]));
        let site = render_site(&view);
        let html = page_text(&site, "S-0001-v1");
        assert!(
            !html.contains("originals/"),
            "a metadata-only page must not link an original"
        );
        assert!(html.contains("Metadata only"), "the lock chip must show");
        assert!(html.contains("class=\"ic\""), "the lock icon must show");
        assert!(
            shipped_originals(&view)
                .iter()
                .all(|o| o.version_id() != "S-0001-v1"),
            "a metadata-only version ships no original"
        );
    }

    #[test]
    fn superseded_version_links_forward() {
        let (_model, view) = load_view(tree_with("", &[]));
        let site = render_site(&view);
        let old = page_text(&site, "S-0002-v1");
        assert!(old.contains("class=\"banner warn\""));
        assert!(
            old.contains("href=\"S-0002-v2.html\""),
            "the stale page must link to the version that supersedes it:\n{old}"
        );
        let current = page_text(&site, "S-0002-v2");
        assert!(
            !current.contains("class=\"banner warn\""),
            "the current version carries no stale banner"
        );
    }

    #[test]
    fn missing_parts_show_unavailable() {
        let (_model, view) = load_view(tree_with("", &[]));
        let site = render_site(&view);
        let excerpt = format_not_available("Excerpt", "this version is extracted");
        let locator = format_not_available("Locator view", "this version is extracted");
        for vid in ["S-0001-v1", "S-0002-v1", "S-0002-v2", "S-0003-v1"] {
            let html = page_text(&site, vid);
            assert!(html.contains("<section class=\"unavailable\""), "{vid}");
            assert!(
                html.contains(&excerpt),
                "{vid} must say the excerpt is not available"
            );
            assert!(
                html.contains(&locator),
                "{vid} must say the locator view is not available"
            );
        }
        let answer = format_not_available("Answer metadata", "a claim cites this version");
        assert!(
            page_text(&site, "S-0003-v1").contains(&answer),
            "a version no claim cites has no answer metadata"
        );
        assert!(
            !page_text(&site, "S-0001-v1").contains(&answer),
            "a version a claim cites shows that claim instead"
        );
    }

    #[test]
    fn every_page_has_the_csp_meta_and_no_style_attribute() {
        let (_model, view) = load_view(tree_with("", &[]));
        let site = render_site(&view);
        let pages = s_pages(&site);
        assert!(!pages.is_empty());
        for path in pages {
            let html = String::from_utf8(site.files()[&path].clone()).unwrap();
            assert_eq!(
                html.matches("http-equiv=\"Content-Security-Policy\"")
                    .count(),
                1,
                "{path} must have one CSP meta"
            );
            assert!(html.contains(CSP_STATIC), "{path}");
            assert!(
                !html.contains("style=\""),
                "{path} must have no style attribute"
            );
        }
    }

    #[test]
    fn held_version_has_no_page() {
        // A source with an active version 1 and a held version 2 that supersedes it, and a
        // source whose only version is held.
        let rows = row(
            "0004",
            1,
            "metadata-only: made up for tests",
            &sha(b"a"),
            "active",
            "",
            "",
        ) + &row(
            "0004",
            2,
            "metadata-only: made up for tests",
            &sha(b"b"),
            "quarantined",
            "S-0004-v1",
            "Held for test.",
        ) + &row(
            "0005",
            1,
            "metadata-only: made up for tests",
            &sha(b"c"),
            "quarantined",
            "",
            "Wholly held for test.",
        );
        let (_model, view) = load_view(tree_with(&rows, &[]));
        let held: Vec<String> = held_versions(&view)
            .iter()
            .map(|v| v.as_str().to_owned())
            .collect();
        assert_eq!(held, ["S-0004-v2", "S-0005-v1"]);
        let pages: Vec<String> = page_versions(&view)
            .iter()
            .map(|v| v.as_str().to_owned())
            .collect();
        assert!(pages.contains(&"S-0004-v1".to_owned()));
        assert!(!pages.contains(&"S-0004-v2".to_owned()));
        assert!(!pages.contains(&"S-0005-v1".to_owned()));

        let planned = site::planned_paths(&view);
        assert!(planned.contains("s/S-0004-v1.html"));
        assert!(!planned.contains("s/S-0004-v2.html"));
        assert!(!planned.contains("s/S-0005-v1.html"));

        let site = render_site(&view);
        assert!(site.files().contains_key("s/S-0004-v1.html"));
        assert!(!site.files().contains_key("s/S-0004-v2.html"));
        assert!(!site.files().contains_key("s/S-0005-v1.html"));
        for path in s_pages(&site) {
            let html = String::from_utf8(site.files()[&path].clone()).unwrap();
            assert!(
                !html.contains("S-0004-v2.html") && !html.contains("S-0005-v1.html"),
                "{path} must not link a held version"
            );
        }
        let active = page_text(&site, "S-0004-v1");
        assert!(
            !active.contains("href=\"S-0004-v2.html\""),
            "the active version must not link forward to the held one"
        );
        let diags = lint(&site, &original_names(&view));
        assert!(diags.is_empty(), "no link may dangle: {diags:?}");
        assert!(diags.iter().all(|d| d.code != Code::Fb0603));
    }

    #[test]
    fn version_links_point_at_s_pages_once_planned() {
        let (_model, view) = load_view(tree_with("", &[]));
        let vid = VersionId::parse("S-0002-v2").unwrap();
        let row = view
            .sources()
            .iter()
            .find(|s| s.value.source_id.as_str() == "S-0002" && s.value.version == 2)
            .map(|s| &s.value);
        assert!(row.is_some());

        let with_page = PlannedPaths::of(&["sources.html", "s/S-0002-v2.html"]);
        assert_eq!(
            source_version_href("claims.html", &vid, row, &with_page),
            Some("s/S-0002-v2.html".to_owned())
        );
        assert_eq!(
            source_version_href("s/S-0001-v1.html", &vid, row, &with_page),
            Some("S-0002-v2.html".to_owned())
        );
        let chip = render_version_chip("claims.html", &vid, row, &with_page);
        assert!(chip.contains("href=\"s/S-0002-v2.html\""), "{chip}");

        let without_page = PlannedPaths::of(&["sources.html"]);
        assert_eq!(
            source_version_href("claims.html", &vid, row, &without_page),
            Some("sources.html#S-0002-v2".to_owned())
        );
        let chip = render_version_chip("claims.html", &vid, row, &without_page);
        assert!(chip.contains("href=\"sources.html#S-0002-v2\""), "{chip}");
    }

    fn site_with_original_link(target: &str) -> Site {
        let mut site = Site::new();
        site.insert(
            "s/S-0001-v1.html",
            format!(
                "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">\n<title>Fieldbook</title>\n<link rel=\"stylesheet\" href=\"../assets/app.css\">\n</head>\n<body>\n<main><a href=\"{target}\">Original</a></main>\n</body>\n</html>\n"
            )
            .into_bytes(),
        );
        site.insert("assets/app.css", b"body { color: var(--ink); }\n".to_vec());
        site
    }

    #[test]
    fn a_renderable_original_link_resolves_to_its_txt_name() {
        let site = site_with_original_link("../../originals/S-0001-v1.html.txt");
        let shipped = OriginalNames(BTreeSet::from([shipped_name("S-0001-v1", "html")]));
        let diags = lint(&site, &shipped);
        assert!(diags.is_empty(), "the .txt name resolves: {diags:?}");

        let unrenamed = OriginalNames(BTreeSet::from(["S-0001-v1.html".to_owned()]));
        let diags = lint(&site, &unrenamed);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, Code::Fb0603);
    }

    #[test]
    fn a_page_range_or_page_of_a_shipped_pdf_links_to_its_first_page() {
        let avail = OriginalAvailability::empty()
            .with("S-0005-v1", "S-0005-v1.pdf")
            .with("S-0006-v1", "S-0006-v1.html.txt");
        assert_eq!(first_pdf_page("p. 14"), Some(14));
        assert_eq!(first_pdf_page("pp. 14-15"), Some(14));
        assert_eq!(first_pdf_page("sec. 1"), None);
        assert_eq!(first_pdf_page("sheet Rates!B2:D9"), None);
        assert_eq!(first_pdf_page("rec 48213"), None);

        assert_eq!(
            located_original_href("p/x.html", "S-0005-v1", Some("p. 14"), &avail),
            Some("../../originals/S-0005-v1.pdf#page=14".to_owned())
        );
        assert_eq!(
            located_original_href("s/S-0001-v1.html", "S-0005-v1", Some("pp. 14-15"), &avail),
            Some("../../originals/S-0005-v1.pdf#page=14".to_owned())
        );
        assert_eq!(
            located_original_href("p/x.html", "S-0005-v1", Some("sec. 1"), &avail),
            None
        );
        assert_eq!(
            located_original_href("p/x.html", "S-0005-v1", None, &avail),
            None
        );
        assert_eq!(
            located_original_href("p/x.html", "S-0006-v1", Some("p. 14"), &avail),
            None,
            "only a PDF original takes a page anchor"
        );
        assert_eq!(
            located_original_href("p/x.html", "S-0007-v1", Some("p. 14"), &avail),
            None,
            "a version with no shipped original keeps its source page"
        );
    }

    #[test]
    fn shipped_availability_follows_the_selection() {
        let rows = row(
            "0004",
            1,
            "S-0004-v1.pdf",
            &sha(PDF_BYTES),
            "active",
            "",
            "",
        ) + &row(
            "0005",
            1,
            "S-0005-v1.pdf",
            &sha(PDF_BYTES),
            "quarantined",
            "",
            "Held for test.",
        );
        let (_model, view) = load_view(tree_with(
            &rows,
            &[("S-0004-v1.pdf", PDF_BYTES), ("S-0005-v1.pdf", PDF_BYTES)],
        ));
        let avail = OriginalAvailability::from_view(&view);
        assert_eq!(avail.shipped_name("S-0004-v1"), Some("S-0004-v1.pdf"));
        assert_eq!(
            avail.shipped_name("S-0005-v1"),
            None,
            "a held original ships nothing"
        );
        assert_eq!(
            avail.shipped_name("S-0001-v1"),
            None,
            "metadata-only ships nothing"
        );
        assert!(!avail.is_empty());
        assert!(OriginalAvailability::empty().is_empty());
    }
}
