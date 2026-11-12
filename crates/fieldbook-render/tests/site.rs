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
    const SOURCES_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const CLAIMS_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const S_0002_V2_TXT: &[u8] =
        include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt");
    const S_0001_V1_TXT: &[u8] =
        include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt");

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

    fn make_claim_row(id: &str, drafted_by: &str, verification: &str) -> String {
        let template = if verification == "primary_source_supported" {
            CLAIMS_CSV.lines().nth(1).unwrap()
        } else {
            CLAIMS_CSV.lines().nth(2).unwrap()
        };
        let mut cols: Vec<&str> = template.split(',').collect();
        cols[0] = id;
        cols[11] = drafted_by;
        cols[12] = verification;
        if verification == "unchecked" {
            cols[28] = "";
            cols[29] = "";
        } else if verification == "primary_source_supported" {
            cols[28] = "2026-02-10";
            cols[29] = "owner";
        }
        cols[33] = "";
        cols.join(",")
    }

    fn build_public_view_with_claims(changes_md: &str, claims_csv: &str) -> (Model, LabelledView) {
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
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);
        tree.insert_bytes("corpus/registers/claims.csv", claims_csv.as_bytes());

        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, report) = load(&Host::new(tree, today), &Options::new(false));
        assert_eq!(
            report.errors(),
            0,
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

    fn recompute_claims_counts_sentence(claims_csv: &str) -> (String, Option<String>) {
        let mut total = 0usize;
        let mut ai_drafted = 0usize;
        let mut unchecked = 0usize;
        let mut supported = 0usize;

        for line in claims_csv.lines().skip(1) {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let cols: Vec<&str> = trimmed.split(',').collect();
            total += 1;
            let drafted_by = cols[11];
            let verification = cols[12];
            if matches!(drafted_by, "codex" | "ai") {
                ai_drafted += 1;
            }
            if verification == "unchecked" {
                unchecked += 1;
            } else if verification == "primary_source_supported" {
                supported += 1;
            }
        }

        let count_sentence = if total == 0 {
            "No claims recorded.".to_owned()
        } else {
            let noun = if total == 1 { "claim" } else { "claims" };
            let verb_a = if ai_drafted == 1 { "is" } else { "are" };
            let verb_u = if unchecked == 1 { "is" } else { "are" };
            format!(
                "{ai_drafted} of {total} {noun} {verb_a} AI-drafted; {unchecked} of {total} {verb_u} unchecked."
            )
        };

        let supported_sentence = if total > 0 && unchecked < total {
            let has = if supported == 1 { "has" } else { "have" };
            Some(format!(
                "{supported} of {total} {has} been checked and found supported."
            ))
        } else {
            None
        };

        (count_sentence, supported_sentence)
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

    #[test]
    fn unreleased_and_released_build_status_across_all_html_files() {
        let claims_header = CLAIMS_CSV.lines().next().unwrap();
        let claim_rows = [
            make_claim_row("CL-CMP-0001", "codex", "unchecked"),
            make_claim_row("CL-CMP-0002", "ai", "unchecked"),
            make_claim_row("CL-CMP-0003", "lead", "primary_source_supported"),
            make_claim_row("CL-CMP-0004", "owner", "unchecked"),
        ];
        let claims_csv = format!("{claims_header}\n{}\n", claim_rows.join("\n"));

        let changes_md =
            "# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n";
        let (_model, view) = build_public_view_with_claims(changes_md, &claims_csv);
        assert_eq!(view.claims().len(), 4);

        // 1. Unreleased build: every HTML file carries the unreleased status banner.
        let unreleased_info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        assert!(!unreleased_info.is_released());
        let site = render(&view, &unreleased_info).unwrap();

        let (expected_counts, expected_supported) = recompute_claims_counts_sentence(&claims_csv);
        assert_eq!(
            expected_counts,
            "2 of 4 claims are AI-drafted; 3 of 4 are unchecked."
        );
        assert_eq!(
            expected_supported,
            Some("1 of 4 has been checked and found supported.".to_owned())
        );

        let mut html_count = 0usize;
        for (path, bytes) in site.files() {
            if std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
            {
                html_count += 1;
                let text = std::str::from_utf8(bytes).unwrap();
                assert_eq!(
                    text.matches("aria-label=\"Build status\"").count(),
                    1,
                    "{path} must contain exactly one aria-label=\"Build status\""
                );
                assert_eq!(
                    text.matches("This is an unreleased build, not an edition.")
                        .count(),
                    1,
                    "{path} must contain exactly one occurrence of unreleased build status banner"
                );
                assert!(
                    text.contains(&expected_counts),
                    "{path} must contain recomputed counts sentence `{expected_counts}`"
                );
                if let Some(ref supp) = expected_supported {
                    assert!(
                        text.contains(supp),
                        "{path} must contain recomputed supported sentence `{supp}`"
                    );
                }
            }
        }
        assert!(html_count >= 1, "site must contain at least one html file");

        // 2. Released build: no HTML file contains "Build status".
        let date = CivilDate::parse("2026-03-01").unwrap();
        let sha = "0123456789abcdef0123456789abcdef01234567".to_owned();
        let released_info = BuildInfo::new(Some(date), Some(sha), BuildProfile::Silent, None);
        assert!(released_info.is_released());

        let released_site = render(&view, &released_info).unwrap();
        let mut released_html_count = 0usize;
        for (path, bytes) in released_site.files() {
            if std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
            {
                released_html_count += 1;
                let text = std::str::from_utf8(bytes).unwrap();
                assert!(
                    !text.contains("Build status"),
                    "{path} in released build must not contain \"Build status\""
                );
            }
        }
        assert!(
            released_html_count >= 1,
            "released site must contain at least one html file"
        );
    }

    #[test]
    fn unreleased_banner_counts_vary_with_fixture_claims() {
        // Negative control against hard-coded counts: modifying the fixture claims
        // must change the rendered counts and supported sentences accordingly.
        let claims_header = CLAIMS_CSV.lines().next().unwrap();
        let claim_rows = [
            make_claim_row("CL-CMP-0001", "codex", "unchecked"),
            make_claim_row("CL-CMP-0002", "codex", "primary_source_supported"),
            make_claim_row("CL-CMP-0003", "lead", "primary_source_supported"),
        ];
        let claims_csv = format!("{claims_header}\n{}\n", claim_rows.join("\n"));

        let changes_md =
            "# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n";
        let (_model, view) = build_public_view_with_claims(changes_md, &claims_csv);
        assert_eq!(view.claims().len(), 3);

        let unreleased_info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = render(&view, &unreleased_info).unwrap();

        let (expected_counts, expected_supported) = recompute_claims_counts_sentence(&claims_csv);
        assert_eq!(
            expected_counts,
            "2 of 3 claims are AI-drafted; 1 of 3 is unchecked."
        );
        assert_eq!(
            expected_supported,
            Some("2 of 3 have been checked and found supported.".to_owned())
        );

        let previous_counts = "2 of 4 claims are AI-drafted; 3 of 4 are unchecked.";

        let mut html_count = 0usize;
        for (path, bytes) in site.files() {
            if std::path::Path::new(path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
            {
                html_count += 1;
                let text = std::str::from_utf8(bytes).unwrap();
                assert!(
                    text.contains(&expected_counts),
                    "{path} must contain modified counts sentence `{expected_counts}`"
                );
                assert!(
                    !text.contains(previous_counts),
                    "{path} must NOT contain previous counts sentence `{previous_counts}`"
                );
                if let Some(ref supp) = expected_supported {
                    assert!(
                        text.contains(supp),
                        "{path} must contain modified supported sentence `{supp}`"
                    );
                }
            }
        }
        assert!(html_count >= 1);
    }
}
