//! Integration tests for `fieldbook_render::site`, `csp`, and `meta` (`build-v0` step 6).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod site {
    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::schema::Register;
    use fieldbook_core::strings;
    use fieldbook_render::csp::{CSP_BRIDGE, CSP_STATIC};
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::site::render;

    const CORPUS_FORMAT_MD: &str = include_str!("../../../docs/reference/corpus-format.md");
    const MINI_CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    fn build_public_view(changes_md: &str) -> (Model, LabelledView) {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_dir("corpus/registers");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes(
            format!("corpus/{}", strings::paths()[6]),
            b"rules_version = 1\n",
        );
        tree.insert_bytes("corpus/changes.md", changes_md.as_bytes());
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");
        for reg in Register::ALL {
            let mut header = String::new();
            for (idx, col) in reg.columns().iter().enumerate() {
                if idx > 0 {
                    header.push(',');
                }
                header.push_str(col.name);
            }
            header.push('\n');
            tree.insert_bytes(
                format!("corpus/registers/{}", reg.file_name()),
                header.into_bytes(),
            );
        }
        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, report) = load(&Host::new(tree, today), &Options::new(false));
        assert_eq!(report.errors(), 0, "{:?}", report.diagnostics);

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
    fn csp_matches_the_reference() {
        let expected_code_span = format!("`{CSP_STATIC}`");
        assert!(
            CORPUS_FORMAT_MD.contains(&expected_code_span),
            "docs/reference/corpus-format.md must contain exact CSP_STATIC constant: {expected_code_span}"
        );
    }

    #[test]
    fn csp_constants_differ_only_in_connect_src() {
        let static_parts: Vec<&str> = CSP_STATIC.split("; ").collect();
        let bridge_parts: Vec<&str> = CSP_BRIDGE.split("; ").collect();
        assert_eq!(static_parts.len(), bridge_parts.len());

        let mut diffs = Vec::new();
        for (s, b) in static_parts.iter().zip(&bridge_parts) {
            if s != b {
                diffs.push((*s, *b));
            }
        }
        assert_eq!(
            diffs,
            vec![("connect-src 'none'", "connect-src 'self'")],
            "CSP_STATIC and CSP_BRIDGE must differ only in connect-src"
        );
    }

    #[test]
    fn every_html_has_one_csp_meta() {
        let (_model, view) = build_public_view(
            "# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n",
        );
        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = render(&view, &info).unwrap();

        let expected_tag =
            format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">");
        let mut html_count = 0usize;
        for (path, bytes) in site.files() {
            if std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
            {
                html_count += 1;
                let text = std::str::from_utf8(bytes).unwrap();
                let count = text
                    .matches("http-equiv=\"Content-Security-Policy\"")
                    .count();
                assert_eq!(count, 1, "{path} must contain exactly one CSP meta tag");
                assert!(
                    text.contains(&expected_tag),
                    "{path} must contain exact CSP_STATIC meta tag"
                );
            }
        }
        assert!(html_count >= 1, "site must contain at least index.html");
    }

    #[test]
    fn no_style_attribute() {
        let (_model, view) = build_public_view(
            "# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n",
        );
        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = render(&view, &info).unwrap();

        for (path, bytes) in site.files() {
            let text = std::str::from_utf8(bytes).unwrap();
            assert!(
                !text.contains("style=\""),
                "{path} must not contain inline style attribute"
            );
        }
    }

    #[test]
    fn unreleased_without_date() {
        let (_model, view) = build_public_view("# Changes\n\nSynthetic changes log.\n");
        let sha = "0123456789abcdef0123456789abcdef01234567".to_owned();
        let info = BuildInfo::new(None, Some(sha), BuildProfile::Silent, None);
        assert!(!info.is_released());

        let site = render(&view, &info).unwrap();
        let index = std::str::from_utf8(&site.files()["index.html"]).unwrap();
        assert!(
            index.contains(strings::unreleased_build()),
            "missing date must render unreleased_build in header: {index}"
        );
        assert!(
            !index.contains("<time"),
            "missing date must not render <time> element in header: {index}"
        );
    }

    #[test]
    fn unreleased_without_commit() {
        let (_model, view) = build_public_view("# Changes\n\nSynthetic changes log.\n");
        let date = CivilDate::parse("2026-03-01").unwrap();

        let info_missing = BuildInfo::new(Some(date), None, BuildProfile::Silent, None);
        assert!(!info_missing.is_released());
        let site_missing = render(&view, &info_missing).unwrap();
        let index_missing = std::str::from_utf8(&site_missing.files()["index.html"]).unwrap();
        assert!(
            index_missing.contains(strings::unreleased_build()),
            "missing commit must render unreleased_build in header: {index_missing}"
        );
        assert!(!index_missing.contains("<time"));
        assert!(!index_missing.contains("2026-03-01"));

        let info_invalid = BuildInfo::new(
            Some(date),
            Some("not-a-40-char-sha".to_owned()),
            BuildProfile::Silent,
            None,
        );
        assert!(!info_invalid.is_released());
        let site_invalid = render(&view, &info_invalid).unwrap();
        let index_invalid = std::str::from_utf8(&site_invalid.files()["index.html"]).unwrap();
        assert!(
            index_invalid.contains(strings::unreleased_build()),
            "invalid commit must render unreleased_build in header: {index_invalid}"
        );
        assert!(!index_invalid.contains("<time"));
    }

    #[test]
    fn released_with_date_and_commit() {
        let (_model, view) = build_public_view(
            "# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n",
        );
        let date = CivilDate::parse("2026-03-01").unwrap();
        let sha = "0123456789abcdef0123456789abcdef01234567".to_owned();
        let info = BuildInfo::new(Some(date), Some(sha), BuildProfile::Silent, None);
        assert!(info.is_released());

        let site = render(&view, &info).unwrap();
        let index = std::str::from_utf8(&site.files()["index.html"]).unwrap();
        assert!(
            index.contains("datetime=\"2026-03-01\">2026-03-01</time>"),
            "released build must render <time> element in header: {index}"
        );
        assert!(!index.contains(strings::unreleased_build()));
    }

    #[test]
    fn strings_js_is_strings_js() {
        let (_model, view) = build_public_view("# Changes\n\nSynthetic changes log.\n");
        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = render(&view, &info).unwrap();
        let js = std::str::from_utf8(&site.files()["assets/strings.js"]).unwrap();
        assert_eq!(js, strings::js());
    }
}
