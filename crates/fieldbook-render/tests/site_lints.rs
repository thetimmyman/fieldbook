//! Unit and property tests for `fieldbook_render::lint` (`build-site-lints`).

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::collections::BTreeSet;

use fieldbook_core::check::Code;
use fieldbook_render::csp::CSP_STATIC;
use fieldbook_render::lint::{
    APP_JS, ASSIST_JS, DATA_FILE, DAY_JS, OriginalNames, SCRIPTS_TOTAL, SEARCH_INDEX, SEARCH_JS,
    lint, scan_html,
};
use fieldbook_render::site::Site;

const CORPUS_FORMAT_MD: &str = include_str!("../../../docs/reference/corpus-format.md");

fn valid_page_with_prefix(prefix: &str, body_inner: &str) -> Vec<u8> {
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">\n<title>Fieldbook</title>\n<link rel=\"stylesheet\" href=\"{prefix}assets/app.css\">\n</head>\n<body>\n<main>{body_inner}</main>\n<script src=\"{prefix}assets/strings.js\"></script>\n</body>\n</html>\n"
    )
    .into_bytes()
}

fn valid_page(body_inner: &str) -> Vec<u8> {
    valid_page_with_prefix("", body_inner)
}

fn clean_site() -> Site {
    let mut site = Site::new();
    site.insert("index.html", valid_page("<h1 id=\"top\">Fieldbook</h1>"));
    site.insert(
        "assets/app.css",
        b"body { color: var(--color-fg); }\n".to_vec(),
    );
    site.insert("assets/strings.js", b"var FB_STRINGS = {};\n".to_vec());
    site
}

mod site_lints {
    use super::*;

    #[test]
    fn clean_site_passes() {
        let site = clean_site();
        let diags = lint(&site, &OriginalNames::default());
        assert!(diags.is_empty(), "expected clean site, got {diags:?}");
    }

    #[test]
    fn dead_link_is_fb0603() {
        let mut site = clean_site();
        site.insert(
            "page.html",
            valid_page("<a href=\"missing.html\">Missing</a>"),
        );
        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0603);
        assert_eq!(diags[0].path, "page.html");
        assert!(diags[0].message.contains("missing.html"));
    }

    #[test]
    fn style_attribute_is_fb0604() {
        let mut site = clean_site();
        site.insert(
            "page.html",
            valid_page("<p style=\"color: red\">Styled</p>"),
        );
        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0604);
        assert_eq!(diags[0].path, "page.html");
    }

    // control: WB-03
    #[test]
    fn script_over_budget_is_fb0605() {
        let mut site = clean_site();
        let oversized = usize::try_from(APP_JS).unwrap_or(32_000) + 1;
        site.insert("assets/app.js", vec![b'a'; oversized]);
        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0605);
        assert_eq!(diags[0].path, "assets/app.js");
    }

    #[test]
    fn fragment_must_exist() {
        let mut site = clean_site();
        site.insert(
            "page.html",
            valid_page(
                "<a href=\"index.html#top\">Ok</a><a href=\"index.html#missing-anchor\">Bad</a>",
            ),
        );
        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0603);
        assert!(diags[0].message.contains("missing-anchor"));
    }

    #[test]
    fn directory_link_needs_index_html() {
        let mut site = clean_site();
        site.insert(
            "a/index.html",
            valid_page_with_prefix("../", "<h1>Area</h1>"),
        );
        site.insert("page.html", valid_page("<a href=\"a/\">Area dir</a>"));
        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0603);
        assert!(diags[0].message.contains("index.html"));
    }

    #[test]
    fn originals_link_resolves_by_name() {
        let mut site = clean_site();
        site.insert(
            "s/S-0001-v1.html",
            format!(
                "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">\n<title>Fieldbook</title>\n<link rel=\"stylesheet\" href=\"../assets/app.css\">\n</head>\n<body>\n<main><a href=\"../originals/S-0001-v1.html.txt\">HTML original</a><a href=\"../originals/S-0001-v1.html\">Unrenamed</a></main>\n</body>\n</html>\n"
            )
            .into_bytes(),
        );
        let originals = OriginalNames(BTreeSet::from(["S-0001-v1.html.txt".to_owned()]));
        let diags = lint(&site, &originals);
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0603);
        assert!(diags[0].message.contains("S-0001-v1.html"));
    }

    #[test]
    fn budget_counts_all_scripts() {
        let mut site = clean_site();
        // Each individual script is within its own limit, and strings.js + extra.js bring total > 64_000.
        site.insert("assets/app.js", vec![b'a'; 25_000]);
        site.insert("assets/search.js", vec![b'b'; 9_000]);
        site.insert("assets/assist.js", vec![b'c'; 19_000]);
        site.insert("assets/extra.js", vec![b'd'; 12_000]);
        // Also add search-index.js and assist-data.js well over 64_000 but under 3_000_000 to verify they are data, not scripts.
        site.insert("assets/search-index.js", vec![b'e'; 100_000]);
        site.insert("assets/assist-data.js", vec![b'f'; 100_000]);

        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(
            diags.len(),
            1,
            "expected 1 total-script FB0605 diagnostic, got {diags:?}"
        );
        assert_eq!(diags[0].code, Code::Fb0605);
        assert!(diags[0].message.contains("total script size"));

        // A .js file under data/ is still counted toward the script budget.
        let mut site_data_js = clean_site();
        site_data_js.insert("data/extra.js", vec![b'x'; 65_000]);
        let diags_data_js = lint(&site_data_js, &OriginalNames::default());
        assert_eq!(diags_data_js.len(), 1);
        assert_eq!(diags_data_js[0].code, Code::Fb0605);
    }

    // control: WB-02
    #[test]
    fn remote_script_is_fb0604() {
        let mut site = clean_site();
        site.insert(
            "page.html",
            valid_page("<script src=\"https://example.invalid/x.js\"></script>"),
        );
        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0604);
        assert_eq!(diags[0].path, "page.html");

        // Unclosed inline script at EOF is also caught as FB0604.
        let mut site_unclosed = clean_site();
        site_unclosed.insert("page.html", valid_page("<script>var x = 1;"));
        let diags_unclosed = lint(&site_unclosed, &OriginalNames::default());
        assert!(
            diags_unclosed.iter().any(|d| d.code == Code::Fb0604),
            "expected FB0604 for unclosed inline script"
        );
    }

    // control: WB-02
    #[test]
    fn external_href_is_fb0604() {
        let mut site = clean_site();
        site.insert(
            "page.html",
            valid_page(
                "<a href=\"https://example.invalid/doc\">External</a><a href=\"HTTPS://example.invalid/upper\">Upper</a>",
            ),
        );
        let diags = lint(&site, &OriginalNames::default());
        assert_eq!(diags.len(), 2, "expected 2 diagnostics, got {diags:?}");
        assert!(diags.iter().all(|d| d.code == Code::Fb0604));
        assert_eq!(diags[0].path, "page.html");
    }

    // control: WB-02
    #[test]
    fn fb0604_still_fails_outside_source_pages() {
        let ecfr_link = "<a class=\"ext-link\" href=\"https://www.ecfr.gov/on/2026-10-27/title-48/section-31.205-1\" target=\"_blank\" rel=\"noopener noreferrer\">ecfr.gov</a>";

        // On s/, a listed official publisher over https: is allowed.
        let mut s_ok = clean_site();
        s_ok.insert("s/S-0001-v1.html", valid_page_with_prefix("../", ecfr_link));
        let d_ok = lint(&s_ok, &OriginalNames::default());
        assert!(
            d_ok.is_empty(),
            "listed publisher https: link on s/ must pass FB0604, got {d_ok:?}"
        );

        // On s/, an unlisted host or http: link still fails FB0604.
        let mut s_bad = clean_site();
        s_bad.insert(
            "s/S-0001-v1.html",
            valid_page_with_prefix(
                "../",
                "<a class=\"ext-link\" href=\"https://unlisted.example.org/doc\" target=\"_blank\" rel=\"noopener noreferrer\">unlisted</a><a class=\"ext-link\" href=\"http://www.ecfr.gov/current/title-48\" target=\"_blank\" rel=\"noopener noreferrer\">http</a>",
            ),
        );
        let d_bad = lint(&s_bad, &OriginalNames::default());
        assert_eq!(
            d_bad.len(),
            2,
            "expected 2 FB0604 diagnostics on s/, got {d_bad:?}"
        );
        assert!(d_bad.iter().all(|d| d.code == Code::Fb0604));

        // On s/, a listed publisher link missing any required safety attribute still fails FB0604.
        for bad_attrs_link in [
            "<a href=\"https://www.ecfr.gov/\">bare</a>",
            "<a href=\"https://www.ecfr.gov/\" target=\"_blank\" rel=\"noopener noreferrer\">no-class</a>",
            "<a class=\"other\" href=\"https://www.ecfr.gov/\" target=\"_blank\" rel=\"noopener noreferrer\">wrong-class</a>",
            "<a class=\"ext-link\" href=\"https://www.ecfr.gov/\" rel=\"noopener noreferrer\">no-target</a>",
            "<a class=\"ext-link\" href=\"https://www.ecfr.gov/\" target=\"_self\" rel=\"noopener noreferrer\">wrong-target</a>",
            "<a class=\"ext-link\" href=\"https://www.ecfr.gov/\" target=\"_blank\">no-rel</a>",
            "<a class=\"ext-link\" href=\"https://www.ecfr.gov/\" target=\"_blank\" rel=\"noopener\">partial-rel</a>",
            "<a class=\"ext-link\" href=\"https://www.ecfr.gov/\" target=\"_blank\" target=\"_self\" rel=\"noopener noreferrer\">dup-target</a>",
        ] {
            let mut site = clean_site();
            site.insert(
                "s/S-0001-v1.html",
                valid_page_with_prefix("../", bad_attrs_link),
            );
            let diags = lint(&site, &OriginalNames::default());
            assert_eq!(
                diags.len(),
                1,
                "s/ link `{bad_attrs_link}` missing required attributes must fail FB0604, got {diags:?}"
            );
            assert_eq!(diags[0].code, Code::Fb0604);
            assert_eq!(diags[0].path, "s/S-0001-v1.html");
        }

        // An extra attribute that can fetch (`ping`) is refused.
        let mut ping = clean_site();
        ping.insert(
            "s/S-0001-v1.html",
            valid_page_with_prefix(
                "../",
                "<a class=\"ext-link\" href=\"https://www.ecfr.gov/\" target=\"_blank\" rel=\"noopener noreferrer\" ping=\"https://unlisted.example.org/p\">ping</a>",
            ),
        );
        let d_ping = lint(&ping, &OriginalNames::default());
        assert_eq!(
            d_ping.len(),
            1,
            "extra `ping` must give FB0604, got {d_ping:?}"
        );
        assert_eq!(d_ping[0].code, Code::Fb0604);

        // A listed link on a nested path under s/ is refused.
        let mut nested = clean_site();
        nested.insert("s/x/y.html", valid_page_with_prefix("../../", ecfr_link));
        let d_nested = lint(&nested, &OriginalNames::default());
        assert_eq!(
            d_nested.len(),
            1,
            "nested s/ link must give FB0604, got {d_nested:?}"
        );
        assert_eq!(d_nested[0].code, Code::Fb0604);

        // Outside s/ (p/, search.html, sources.html, claims/), even a listed publisher link fails FB0604.
        for (path, prefix) in [
            ("p/cmp-system-review.html", "../"),
            ("search.html", ""),
            ("sources.html", ""),
            ("claims/CMP-1.html", "../"),
        ] {
            let mut site = clean_site();
            site.insert(path, valid_page_with_prefix(prefix, ecfr_link));
            let diags = lint(&site, &OriginalNames::default());
            assert_eq!(
                diags.len(),
                1,
                "listed publisher link on `{path}` must still fail FB0604, got {diags:?}"
            );
            assert_eq!(diags[0].code, Code::Fb0604);
            assert_eq!(diags[0].path, path);
        }
    }

    #[test]
    fn budgets_match_the_reference() {
        let fb0605_line = CORPUS_FORMAT_MD
            .lines()
            .find(|l| l.starts_with("| FB0605 |"))
            .unwrap_or_else(|| panic!("FB0605 row missing from corpus-format.md"));

        assert!(
            fb0605_line.contains("`app.js` 32 KB") && APP_JS == 32_000,
            "APP_JS must match `app.js` 32 KB (32,000 bytes)"
        );
        assert!(
            fb0605_line.contains("`search.js` 11 KB") && SEARCH_JS == 11_000,
            "SEARCH_JS must match `search.js` 11 KB (11,000 bytes)"
        );
        assert!(
            fb0605_line.contains("`assist.js` 20 KB") && ASSIST_JS == 20_000,
            "ASSIST_JS must match `assist.js` 20 KB (20,000 bytes)"
        );
        assert!(
            fb0605_line.contains("64 KB of scripts in all") && SCRIPTS_TOTAL == 64_000,
            "SCRIPTS_TOTAL must match 64 KB (64,000 bytes)"
        );
        assert!(
            fb0605_line.contains("3 MB for any other data file of the edition")
                && DATA_FILE == 3_000_000,
            "DATA_FILE must match 3 MB (3,000,000 bytes)"
        );
        assert!(
            fb0605_line.contains("15 MB for `search-index.js`") && SEARCH_INDEX == 15_000_000,
            "SEARCH_INDEX must match 15 MB (15,000,000 bytes)"
        );
        assert!(
            fb0605_line.contains("A KB is 1,000 bytes"),
            "FB0605 row must state decimal KB"
        );
        assert!(
            fb0605_line.contains("Scripts are app.js, search.js, strings.js and assist.js; strings.js counts only in the 64 KB total."),
            "FB0605 row must state script list and strings.js rule"
        );
        assert!(
            fb0605_line.contains("search-index.js and assist-data.js are data, not scripts. Other .js files count as scripts."),
            "FB0605 row must state data vs script classification"
        );

        // Verify runtime classification of search-index.js and assist-data.js vs other .js files:
        // 1. A 70 KB search-index.js and 70 KB assist-data.js pass (they are data files, <= DATA_FILE, not scripts).
        let mut site_data = clean_site();
        site_data.insert("assets/search-index.js", vec![b'x'; 70_000]);
        site_data.insert("assets/assist-data.js", vec![b'y'; 70_000]);
        assert!(
            lint(&site_data, &OriginalNames::default()).is_empty(),
            "70 KB search-index.js and assist-data.js must be classified as data and pass"
        );

        // 2. A data file other than the search index exceeding DATA_FILE raises FB0605.
        let mut site_data_over = clean_site();
        let over_data = usize::try_from(DATA_FILE).unwrap_or(3_000_000) + 1;
        site_data_over.insert("assets/assist-data.js", vec![b'x'; over_data]);
        let over_diags = lint(&site_data_over, &OriginalNames::default());
        assert_eq!(over_diags.len(), 1);
        assert_eq!(over_diags[0].code, Code::Fb0605);

        // 3. A 40 KB strings.js passes on its own (no individual limit, under 64 KB total).
        let mut site_strings = Site::new();
        site_strings.insert("index.html", valid_page("<h1 id=\"top\">Fieldbook</h1>"));
        site_strings.insert(
            "assets/app.css",
            b"body { color: var(--color-fg); }\n".to_vec(),
        );
        site_strings.insert("assets/strings.js", vec![b's'; 40_000]);
        assert!(
            lint(&site_strings, &OriginalNames::default()).is_empty(),
            "40 KB strings.js has no individual limit and fits in 64 KB total"
        );

        // 4. An unnamed other .js file of 65 KB raises FB0605 against SCRIPTS_TOTAL.
        let mut site_other_js = clean_site();
        site_other_js.insert("assets/other.js", vec![b'o'; 65_000]);
        let other_diags = lint(&site_other_js, &OriginalNames::default());
        assert_eq!(other_diags.len(), 1);
        assert_eq!(other_diags[0].code, Code::Fb0605);
    }

    #[test]
    fn day_js_passes_at_its_cap_and_fails_one_byte_over() {
        assert_eq!(DAY_JS, 4_000);
        let doc = CORPUS_FORMAT_MD
            .lines()
            .find(|l| l.starts_with("| FB0605 |"))
            .unwrap_or_else(|| panic!("FB0605 row missing from corpus-format.md"));
        assert!(
            doc.contains("day.js, is a script of its own with a cap of 4,000 bytes"),
            "FB0605 row must name day.js and its cap"
        );
        let cap = usize::try_from(DAY_JS).unwrap_or(4_000);

        let mut at_cap = clean_site();
        at_cap.insert("assets/day.js", vec![b'x'; cap]);
        let diags = lint(&at_cap, &OriginalNames::default());
        assert!(
            diags.is_empty(),
            "day.js at its cap must pass, got {diags:?}"
        );

        // Negative control: one byte over the cap fails with FB0605 on day.js.
        let mut over = clean_site();
        over.insert("assets/day.js", vec![b'x'; cap + 1]);
        let diags = lint(&over, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, Code::Fb0605);
        assert!(diags[0].message.contains("assets/day.js"));
    }

    #[test]
    fn search_index_passes_at_its_cap_and_fails_one_byte_over() {
        let cap = usize::try_from(SEARCH_INDEX).unwrap_or(15_000_000);

        let mut at_cap = clean_site();
        at_cap.insert("assets/search-index.js", vec![b'x'; cap]);
        let diags = lint(&at_cap, &OriginalNames::default());
        assert!(
            diags.is_empty(),
            "search-index.js of exactly {SEARCH_INDEX} bytes must pass, got {diags:?}"
        );

        let mut over = clean_site();
        over.insert("assets/search-index.js", vec![b'x'; cap + 1]);
        let diags = lint(&over, &OriginalNames::default());
        assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {diags:?}");
        assert_eq!(diags[0].code, Code::Fb0605);
        assert_eq!(diags[0].path, "assets/search-index.js");
        assert!(
            diags[0].message.contains("15000000"),
            "the message must name the 15 MB cap: {}",
            diags[0].message
        );
    }

    #[test]
    fn register_copies_take_the_hosted_file_limit() {
        use fieldbook_render::lint::REGISTER_COPY;
        assert_eq!(REGISTER_COPY, 25 * 1024 * 1024);
        let cap = usize::try_from(REGISTER_COPY).unwrap_or(26_214_400);

        for path in ["data/claims.csv", fieldbook_core::strings::PATHS[0]] {
            let mut above_data_budget = clean_site();
            above_data_budget.insert(path, vec![b'x'; 20_000_000]);
            let diags = lint(&above_data_budget, &OriginalNames::default());
            assert!(diags.is_empty(), "{path} at 20 MB must pass: {diags:?}");

            let mut over = clean_site();
            over.insert(path, vec![b'x'; cap + 1]);
            let diags = lint(&over, &OriginalNames::default());
            assert!(
                diags.iter().any(|d| d.code == Code::Fb0605
                    && d.path == path
                    && d.message.contains("26214400")),
                "{path} one byte over the limit must raise FB0605: {diags:?}"
            );
        }

        // Any other file under data/ keeps the 3 MB budget.
        let mut other = clean_site();
        other.insert("data/other.csv.txt", vec![b'x'; 3_000_001]);
        let diags = lint(&other, &OriginalNames::default());
        assert!(
            diags
                .iter()
                .any(|d| d.code == Code::Fb0605 && d.message.contains("3000000")),
            "{diags:?}"
        );
    }

    #[test]
    fn other_data_files_keep_the_3_mb_cap() {
        let cap = usize::try_from(DATA_FILE).unwrap_or(3_000_000);
        assert_eq!(cap, 3_000_000);

        for path in ["assets/assist-data.js", "data/units.json", "data/units.js"] {
            let mut at_cap = clean_site();
            at_cap.insert(path, vec![b'x'; cap]);
            let diags = lint(&at_cap, &OriginalNames::default());
            // A `.js` file under `data/` is not exempt from the script total, so judge
            // only the data-file finding.
            assert!(
                !diags.iter().any(|d| d.message.contains("data file")),
                "{path} of exactly {DATA_FILE} bytes must not be over the data-file cap, got {diags:?}"
            );

            let mut over = clean_site();
            over.insert(path, vec![b'x'; cap + 1]);
            let diags = lint(&over, &OriginalNames::default());
            assert!(
                diags.iter().any(|d| d.code == Code::Fb0605
                    && d.path == path
                    && d.message.contains("data file")
                    && d.message.contains("3000000")),
                "{path} of {} bytes must fail the 3 MB cap, got {diags:?}",
                cap + 1
            );
        }
    }

    #[test]
    fn html_scanner_never_panics_on_200_seeded_inputs() {
        let mut prng = rng::Rng::new(1);
        for _ in 0..200 {
            let len = usize::try_from(prng.next_u64() % 256).unwrap_or(64);
            let buf = prng.bytes(len);
            if let Err(err) = scan_html(&buf) {
                assert!(err.byte_offset <= buf.len());
                assert!(err.line >= 1);
                assert!(err.col >= 1);
            }
        }

        let sample = "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\" content=\"§14 — 252.242‑7005 • café\"></head><body><a href='p/cmp-test.html#sec-1' data-label=\"quote\">Text</a><!-- comment --></body></html>".as_bytes();
        for idx in 0..sample.len() {
            // Flip byte
            let mut flipped = sample.to_vec();
            flipped[idx] ^= 0xff;
            if let Err(err) = scan_html(&flipped) {
                assert!(err.byte_offset <= flipped.len());
                assert!(err.line >= 1);
                assert!(err.col >= 1);
            }

            // Delete byte
            let mut deleted = sample.to_vec();
            deleted.remove(idx);
            if let Err(err) = scan_html(&deleted) {
                assert!(err.byte_offset <= deleted.len());
                assert!(err.line >= 1);
                assert!(err.col >= 1);
            }
        }
    }

    #[test]
    fn pdf_page_and_search_query_fragments_are_valid() {
        let mut site = clean_site();
        site.insert("search.html", valid_page("<h1>Search</h1>"));
        site.insert(
            "p/test.html",
            format!(
                "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">\n<title>Fieldbook</title>\n<link rel=\"stylesheet\" href=\"../assets/app.css\">\n</head>\n<body>\n<main><a href=\"../../originals/S-0042-v2.pdf#page=1\">PDF p.1</a><a href=\"../search.html#q=252.242-7005%20clause\">Search</a></main>\n</body>\n</html>\n"
            )
            .into_bytes(),
        );
        let originals = OriginalNames(BTreeSet::from(["S-0042-v2.pdf".to_owned()]));
        let diags = lint(&site, &originals);
        assert!(
            diags.is_empty(),
            "valid PDF #page=1 and search.html#q=... fragments must pass, got {diags:?}"
        );
    }

    #[test]
    fn fragment_exceptions_still_require_targets() {
        let originals = OriginalNames(BTreeSet::from([
            "S-0042-v2.pdf".to_owned(),
            "S-0001-v1.html.txt".to_owned(),
        ]));

        // 1. #page=0 on a shipped PDF raises FB0603
        let mut s1 = Site::new();
        s1.insert("assets/app.css", b"body {}\n".to_vec());
        s1.insert("assets/strings.js", b"var FB_STRINGS = {};\n".to_vec());
        s1.insert(
            "index.html",
            valid_page("<a href=\"../originals/S-0042-v2.pdf#page=0\">p0</a>"),
        );
        let d1 = lint(&s1, &originals);
        assert_eq!(d1.len(), 1, "expected FB0603 for #page=0, got {d1:?}");
        assert_eq!(d1[0].code, Code::Fb0603);

        // 2. Non-integer page selector (#page=abc) on a shipped PDF raises FB0603
        let mut s2 = Site::new();
        s2.insert("assets/app.css", b"body {}\n".to_vec());
        s2.insert("assets/strings.js", b"var FB_STRINGS = {};\n".to_vec());
        s2.insert(
            "index.html",
            valid_page("<a href=\"../originals/S-0042-v2.pdf#page=abc\">pabc</a>"),
        );
        let d2 = lint(&s2, &originals);
        assert_eq!(d2.len(), 1, "expected FB0603 for #page=abc, got {d2:?}");
        assert_eq!(d2[0].code, Code::Fb0603);

        // 3. Valid #page=1 on a missing original raises FB0603
        let mut s3 = Site::new();
        s3.insert("assets/app.css", b"body {}\n".to_vec());
        s3.insert("assets/strings.js", b"var FB_STRINGS = {};\n".to_vec());
        s3.insert(
            "index.html",
            valid_page("<a href=\"../originals/S-9999-v1.pdf#page=1\">missing pdf</a>"),
        );
        let d3 = lint(&s3, &originals);
        assert_eq!(
            d3.len(),
            1,
            "expected FB0603 for missing PDF original, got {d3:?}"
        );
        assert_eq!(d3[0].code, Code::Fb0603);

        // 4. Valid #q=foo on a missing search.html raises FB0603
        let mut s4 = Site::new();
        s4.insert("assets/app.css", b"body {}\n".to_vec());
        s4.insert("assets/strings.js", b"var FB_STRINGS = {};\n".to_vec());
        s4.insert(
            "index.html",
            valid_page("<a href=\"search.html#q=foo\">missing search.html</a>"),
        );
        let d4 = lint(&s4, &originals);
        assert_eq!(
            d4.len(),
            1,
            "expected FB0603 when search.html is absent, got {d4:?}"
        );
        assert_eq!(d4[0].code, Code::Fb0603);

        // 5. Missing ordinary HTML anchor on search.html raises FB0603
        let mut s5 = Site::new();
        s5.insert("assets/app.css", b"body {}\n".to_vec());
        s5.insert("assets/strings.js", b"var FB_STRINGS = {};\n".to_vec());
        s5.insert("search.html", valid_page("<h1 id=\"top\">Search</h1>"));
        s5.insert(
            "index.html",
            valid_page("<a href=\"search.html#missing-id\">bad anchor</a>"),
        );
        let d5 = lint(&s5, &originals);
        assert_eq!(
            d5.len(),
            1,
            "expected FB0603 for missing ordinary HTML anchor on search.html, got {d5:?}"
        );
        assert_eq!(d5[0].code, Code::Fb0603);
    }
}
