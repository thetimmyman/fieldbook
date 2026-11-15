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
    const S_0001_V1_TXT: &[u8] =
        include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt");
    const S_0001_V1_EXTRACTION_TOML: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/sources/text/S-0001-v1/extraction.toml"
    );
    const S_0001_V1_P0001_TXT: &[u8] = include_bytes!(
        "../../../tests/fixtures/corpus-mini/corpus/sources/text/S-0001-v1/p0001.txt"
    );
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
        tree_full(extra_rows, extra_files, CLAIMS_CSV, CMP_SYSTEM_REVIEW_MD)
    }

    fn tree_with_extraction(
        extra_rows: &str,
        extra_files: &[(&str, &[u8])],
        extraction_toml: &str,
        p0001_bytes: &[u8],
    ) -> Tree {
        let mut tree = tree_with(extra_rows, extra_files);
        for dir in ["corpus/sources/text", "corpus/sources/text/S-0001-v1"] {
            tree.insert_dir(dir);
        }
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            extraction_toml.as_bytes(),
        );
        tree.insert_bytes("corpus/sources/text/S-0001-v1/p0001.txt", p0001_bytes);
        tree
    }

    /// Like [`tree_with`], with the claims register and the one reader page replaced.
    fn tree_full(
        extra_rows: &str,
        extra_files: &[(&str, &[u8])],
        claims_csv: &str,
        page_md: &str,
    ) -> Tree {
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
        tree.insert_bytes("corpus/registers/claims.csv", claims_csv.as_bytes());
        tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            page_md.as_bytes(),
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
        for ext in [
            "html",
            "htm",
            "svg",
            "xhtml",
            "HTML",
            "Svg",
            "x.html",
            "foo.bar.svg",
            "tar.XHTML",
        ] {
            assert_eq!(
                shipped_name("S-0042-v2", ext),
                format!("S-0042-v2.{ext}.txt"),
                "the .{ext} case must gain .txt"
            );
        }
        for ext in ["pdf", "txt", "csv", "xlsx", "tar.gz", "html.bak", "x.pdf"] {
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
        assert!(names.0.contains("S-0001-v1.txt"));
        assert_eq!(names.0.len(), 4);

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
        let html = page_text(&site, "S-0003-v1");
        assert!(
            !html.contains("originals/"),
            "a metadata-only page must not link an original"
        );
        assert!(html.contains("Metadata only"), "the lock chip must show");
        assert!(html.contains("class=\"ic\""), "the lock icon must show");
        assert!(
            shipped_originals(&view)
                .iter()
                .all(|o| o.version_id() != "S-0003-v1"),
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
        let vid = "S-0002-v2";
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
        // A metadata-only version stores no bytes, so nothing can be extracted: its banner
        // stands in for the excerpt and the locator view (HANDOFF sections 5 and 11).
        for vid in ["S-0002-v1", "S-0003-v1"] {
            let html = page_text(&site, vid);
            assert!(html.contains("Metadata only."), "{vid}");
            assert!(!html.contains(&excerpt), "{vid} must show no excerpt line");
            assert!(!html.contains(&locator), "{vid} must show no locator line");
            assert!(!html.contains("until this version is extracted"), "{vid}");
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
            avail.shipped_name("S-0003-v1"),
            None,
            "metadata-only ships nothing"
        );
        assert!(!avail.is_empty());
        assert!(OriginalAvailability::empty().is_empty());
    }

    #[test]
    fn a_double_extension_original_still_ships_as_text() {
        let bytes: &[u8] = b"<!doctype html><script>alert(1)</script> synthetic\n";
        let rows = row(
            "0004",
            1,
            "S-0004-v1.foo.html",
            &sha(bytes),
            "active",
            "",
            "",
        );
        let (_model, view) = load_view(tree_with(&rows, &[("S-0004-v1.foo.html", bytes)]));
        let names = original_names(&view);
        assert!(
            names.0.contains("S-0004-v1.foo.html.txt"),
            "the last extension decides: {names:?}"
        );
        assert!(!names.0.contains("S-0004-v1.foo.html"));
        let site = render_site(&view);
        let html = page_text(&site, "S-0004-v1");
        assert!(html.contains("../../originals/S-0004-v1.foo.html.txt"));
        assert!(!html.contains("originals/S-0004-v1.foo.html\""));
        assert!(lint(&site, &names).is_empty());
    }

    #[test]
    fn a_list_in_supersedes_uses_the_register_separator() {
        let rows = row(
            "0004",
            1,
            "metadata-only: made up for tests",
            &sha(b"a"),
            "active",
            "S-0001-v1|S-0003-v1",
            "",
        );
        let (_model, view) = load_view(tree_with(&rows, &[]));
        let site = render_site(&view);
        let html = page_text(&site, "S-0004-v1");
        assert!(html.contains("<dt>supersedes</dt>"), "{html}");
        assert!(html.contains("href=\"S-0001-v1.html\""));
        assert!(html.contains("href=\"S-0003-v1.html\""));
    }

    #[test]
    fn the_original_panel_names_the_locators_that_cite_it() {
        let rows = row(
            "0005",
            1,
            "S-0005-v1.pdf",
            &sha(PDF_BYTES),
            "active",
            "",
            "",
        );
        let page = CMP_SYSTEM_REVIEW_MD.replace(
            "[@CL-CMP-0001]",
            "[@CL-CMP-0001] [@S-0005-v1, p. 14] [@S-0005-v1, sec. 2]",
        );
        let (_model, view) = load_view(tree_full(
            &rows,
            &[("S-0005-v1.pdf", PDF_BYTES)],
            CLAIMS_CSV,
            &page,
        ));
        let site = render_site(&view);
        let html = page_text(&site, "S-0005-v1");
        let panel_start = html.find("src-original-line").unwrap();
        let panel = &html[panel_start..panel_start + 600];
        assert!(
            panel.contains("href=\"../../originals/S-0005-v1.pdf\""),
            "{panel}"
        );
        assert!(
            panel.contains(
                "<a class=\"loc\" href=\"../../originals/S-0005-v1.pdf#page=14\">p. 14</a>"
            ),
            "{panel}"
        );
        assert!(
            panel.contains("<span class=\"loc\">sec. 2</span>"),
            "{panel}"
        );

        // The citing page row repeats the locator beside its page chip.
        let pages = &html[html.find("src-pages").unwrap()..];
        assert!(pages.contains("p. 14"), "{pages}");
        assert!(pages.contains("sec. 2"), "{pages}");

        // The source of S-0002-v2 shows the locators of the claims that cite it.
        let v2 = page_text(&site, "S-0002-v2");
        let v2_panel = &v2[v2.find("src-original-line").unwrap()..];
        assert!(v2_panel.contains("sec. 3") && v2_panel.contains("sec. 4"));
    }

    #[test]
    fn a_cited_claim_row_carries_the_answer_metadata() {
        let claims: String = CLAIMS_CSV
            .lines()
            .map(|line| {
                if line.starts_with("CL-EST-0003,") {
                    let mut cells: Vec<&str> = line.split(',').collect();
                    cells[12] = "primary_source_supported";
                    cells[19] = "2026-01-15";
                    cells[28] = "2026-02-02";
                    cells[29] = "owner";
                    cells.join(",")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        let (_model, view) = load_view(tree_full("", &[], &claims, CMP_SYSTEM_REVIEW_MD));
        let site = render_site(&view);
        let html = page_text(&site, "S-0002-v2");
        let meta = |claim: &str| -> String {
            let at = html
                .find(&format!("#{claim}"))
                .unwrap_or_else(|| panic!("{claim}"));
            let rest = &html[at..];
            let start = rest.find("class=\"ans-meta\"").unwrap();
            let end = rest[start..].find("</div>").unwrap();
            rest[start..start + end].to_owned()
        };
        let verified = meta("CL-EST-0003");
        assert!(verified.contains("S-0002-v2"), "citation chip: {verified}");
        assert!(verified.contains("sec. 4"), "{verified}");
        assert!(verified.contains("Effective 2026-01-15"), "{verified}");
        assert!(
            verified.contains("Verified <time datetime=\"2026-02-02\">2026-02-02</time> · owner"),
            "the date alone is the datetime, the role stays outside it: {verified}"
        );
        let unverified = meta("CL-EST-0002");
        assert!(unverified.contains("No effective date"), "{unverified}");
        assert!(unverified.contains("Never verified"), "{unverified}");

        let css = include_str!("../assets/app.css");
        assert!(css.contains(".ans-meta {"), "the row class needs a rule");
    }

    #[test]
    fn anchors_and_marks_on_s_page() {
        let tree = tree_with_extraction("", &[], S_0001_V1_EXTRACTION_TOML, S_0001_V1_P0001_TXT);
        let (_model, view) = load_view(tree);
        let site = render_site(&view);

        let html = page_text(&site, "S-0001-v1");
        assert!(html.contains("id=\"sec-1\""), "must contain id=sec-1");
        assert!(html.contains("id=\"sec-2\""), "must contain id=sec-2");
        assert!(
            html.contains("<mark>Synthetic compliance reviews occur every 30 days.</mark>"),
            "must contain marked passage"
        );
        assert!(
            html.contains("<blockquote class=\"excerpt\">"),
            "must contain excerpt blockquote"
        );
        assert!(
            html.contains("<blockquote class=\"excerpt\"><mark>Synthetic compliance reviews occur every 30 days.</mark></blockquote>"),
            "must contain excerpt with passage"
        );
        assert!(!html.contains("style=\""), "must not contain style=");
        assert!(
            html.contains("class=\"locator-view\""),
            "must contain class=locator-view"
        );

        let s0002_html = page_text(&site, "S-0002-v2");
        let excerpt = format_not_available("Excerpt", "this version is extracted");
        let locator = format_not_available("Locator view", "this version is extracted");
        assert!(
            s0002_html.contains(&excerpt),
            "S-0002-v2 must say excerpt is not available"
        );
        assert!(
            s0002_html.contains(&locator),
            "S-0002-v2 must say locator view is not available"
        );

        // Second tree: page text containing markup is escaped
        let script_p0001 = b"Section 1: <script>x</script> Synthetic compliance reviews occur every 30 days.\nSection 2: Synthetic withholding threshold is 5 percent.\n";
        let script_sha = fieldbook_core::hash::sha256_hex(script_p0001);
        let script_toml = S_0001_V1_EXTRACTION_TOML.replace(
            "p0001 = \"0fba4a465337f7383e693c1348ff428c4ce68dbfc6f6a3b5b64014d35d0eb73d\"",
            &format!("p0001 = \"{script_sha}\""),
        );
        let tree2 = tree_with_extraction("", &[], &script_toml, script_p0001);
        let (_model2, view2) = load_view(tree2);
        let site2 = render_site(&view2);
        let html2 = page_text(&site2, "S-0001-v1");
        assert!(html2.contains("&lt;script&gt;"), "must escape script tag");
        assert!(
            !html2.contains("<script>x"),
            "must not contain raw script tag"
        );
    }

    #[test]
    fn ecfr_location_is_a_link() {
        let loc_url = "https://www.ecfr.gov/api/renderer/v1/content/enhanced/2026-10-27/title-48?chapter=1&part=31";
        let rows = row("0004", 1, loc_url, &sha(HTML_BYTES), "active", "", "");
        let (_model, view) = load_view(tree_with(&rows, &[("S-0004-v1.html", HTML_BYTES)]));
        let site = render_site(&view);
        let html = page_text(&site, "S-0004-v1");
        let escaped_url = "https://www.ecfr.gov/api/renderer/v1/content/enhanced/2026-10-27/title-48?chapter=1&amp;part=31";
        assert!(
            html.contains(&format!(
                "<dt>location</dt>\n        <dd class=\"mono\"><a class=\"ext-link\" href=\"{escaped_url}\" target=\"_blank\" rel=\"noopener noreferrer\">{escaped_url}<span class=\"sr-only\">, official publisher, opens in a new tab</span><svg class=\"ic\""
            )),
            "eCFR location must render as an ext-link in the register row:\n{html}"
        );
        let diags = lint(&site, &original_names(&view));
        assert!(
            diags.is_empty(),
            "listed publisher link on s/ must pass lint: {diags:?}"
        );
    }

    #[test]
    fn ecfr_section_locator_links_to_its_section() {
        let loc_url = "https://www.ecfr.gov/api/renderer/v1/content/enhanced/2026-10-27/title-48?chapter=1&part=31";
        let rows = row("0004", 1, loc_url, &sha(PDF_BYTES), "active", "", "");
        let page = CMP_SYSTEM_REVIEW_MD.replace(
            "[@CL-CMP-0001]",
            "[@CL-CMP-0001] [@S-0004-v1, § 31.205-1] [@S-0004-v1, sec. 31.205-6] [@S-0004-v1, p. 14]",
        );
        let (_model, view) = load_view(tree_full(
            &rows,
            &[("S-0004-v1.pdf", PDF_BYTES)],
            CLAIMS_CSV,
            &page,
        ));
        let site = render_site(&view);
        let html = page_text(&site, "S-0004-v1");
        let panel_start = html.find("src-original-line").unwrap();
        let panel_end = html[panel_start..].find("</p>").unwrap() + panel_start;
        let panel = &html[panel_start..panel_end];
        assert!(
            panel.contains(
                "<span class=\"loc\">§ 31.205-1</span> <a class=\"ext-link\" href=\"https://www.ecfr.gov/on/2026-10-27/title-48/section-31.205-1\" target=\"_blank\" rel=\"noopener noreferrer\">ecfr.gov<span class=\"sr-only\">, § 31.205-1 at the official publisher, opens in a new tab</span><svg class=\"ic\""
            ),
            "§ locator must keep its span and have an ecfr.gov section link beside it:\n{panel}"
        );
        assert!(
            panel.contains(
                "<span class=\"loc\">sec. 31.205-6</span> <a class=\"ext-link\" href=\"https://www.ecfr.gov/on/2026-10-27/title-48/section-31.205-6\" target=\"_blank\" rel=\"noopener noreferrer\">ecfr.gov<span class=\"sr-only\">, sec. 31.205-6 at the official publisher, opens in a new tab</span><svg class=\"ic\""
            ),
            "sec. locator must keep its span and have an ecfr.gov section link beside it:\n{panel}"
        );
        assert!(
            panel.contains(
                "<a class=\"loc\" href=\"../../originals/S-0004-v1.pdf#page=14\">p. 14</a>"
            ),
            "page locator must link to the shipped PDF page only:\n{panel}"
        );
        assert!(
            !panel.contains("p. 14 at the official publisher"),
            "page locator must not get a publisher link:\n{panel}"
        );
        let diags = lint(&site, &original_names(&view));
        assert!(
            diags.is_empty(),
            "section links on s/ must pass lint: {diags:?}"
        );
    }

    #[test]
    fn unlisted_host_stays_text() {
        let loc_url = "https://unlisted.example.org/rules/part-31";
        let rows = row("0004", 1, loc_url, &sha(HTML_BYTES), "active", "", "");
        let page = CMP_SYSTEM_REVIEW_MD
            .replace("[@CL-CMP-0001]", "[@CL-CMP-0001] [@S-0004-v1, § 31.205-1]");
        let (_model, view) = load_view(tree_full(
            &rows,
            &[("S-0004-v1.html", HTML_BYTES)],
            CLAIMS_CSV,
            &page,
        ));
        let site = render_site(&view);
        let html = page_text(&site, "S-0004-v1");
        assert!(
            html.contains(&format!(
                "<dt>location</dt>\n        <dd class=\"mono\">{loc_url}</dd>"
            )),
            "unlisted host location must stay plain text:\n{html}"
        );
        assert!(
            !html.contains("class=\"ext-link\""),
            "unlisted host must render no ext-link:\n{html}"
        );
        assert!(
            html.contains("<span class=\"loc\">§ 31.205-1</span>"),
            "locator text must still appear:\n{html}"
        );
        let diags = lint(&site, &original_names(&view));
        assert!(
            diags.is_empty(),
            "unlisted host page must pass lint: {diags:?}"
        );
    }

    #[test]
    fn metadata_only_location_stays_text() {
        let loc_val = "metadata-only: https://www.ecfr.gov/current/title-48/part-31";
        let rows = row("0004", 1, loc_val, &sha(b"meta"), "active", "", "");
        let (_model, view) = load_view(tree_with(&rows, &[]));
        let site = render_site(&view);
        let html = page_text(&site, "S-0004-v1");
        assert!(
            html.contains(&format!(
                "<dt>location</dt>\n        <dd class=\"mono\">{loc_val}</dd>"
            )),
            "metadata-only location must stay plain text even if it mentions a listed host:\n{html}"
        );
        assert!(
            !html.contains("class=\"ext-link\""),
            "metadata-only page must render no ext-link:\n{html}"
        );
        let s3 = page_text(&site, "S-0003-v1");
        assert!(
            !s3.contains("class=\"ext-link\""),
            "S-0003-v1 metadata-only page must render no ext-link:\n{s3}"
        );
    }
}
