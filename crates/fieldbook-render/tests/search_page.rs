//! Integration tests for `fieldbook_render::search_page` (`search-js`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod search_page {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::search::{Index, unit_href, utf16_cmp};
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::search_index;
    use fieldbook_render::search_page::{self, LATEST_PAGES, OUTPUT_PATH, SCRIPT, SCRIPT_PATH};
    use fieldbook_render::site::{self, Site, planned_paths};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn insert_all(tree: &mut Tree, dir: &Path, rel: &str) {
        tree.insert_dir(rel);
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let child = format!("{rel}/{name}");
            if entry.file_type().unwrap().is_dir() {
                insert_all(tree, &entry.path(), &child);
            } else {
                tree.insert_bytes(child, std::fs::read(entry.path()).unwrap());
            }
        }
    }

    fn with_view<T>(f: impl FnOnce(&LabelledView) -> T) -> T {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
        let mut tree = Tree::new();
        insert_all(&mut tree, &root.join("corpus"), "corpus");
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
        f(&view)
    }

    fn info() -> BuildInfo {
        BuildInfo::new(None, Some(COMMIT.to_owned()), BuildProfile::Silent, None)
    }

    fn built() -> Site {
        with_view(|view| site::render(view, &info()).unwrap())
    }

    fn text(site: &Site, path: &str) -> String {
        String::from_utf8(
            site.files()
                .get(path)
                .unwrap_or_else(|| panic!("{path} must be built"))
                .clone(),
        )
        .unwrap()
    }

    fn page() -> String {
        text(&built(), OUTPUT_PATH)
    }

    #[test]
    fn the_page_and_its_script_are_planned_and_built() {
        let site = built();
        let planned = with_view(planned_paths);
        assert!(planned.contains(OUTPUT_PATH));
        assert!(planned.contains(SCRIPT_PATH));
        assert_eq!(text(&site, SCRIPT_PATH), SCRIPT);
        assert!(text(&site, search_index::OUTPUT_PATH).starts_with("window.FB_INDEX = {\"v\":1,"));
    }

    #[test]
    fn the_csp_meta_appears_once() {
        let html = page();
        let meta =
            format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">");
        assert_eq!(html.matches(&meta).count(), 1);
        assert_eq!(html.matches("Content-Security-Policy").count(), 1);
    }

    #[test]
    fn the_page_has_no_style_attribute() {
        let html = page();
        assert!(!html.contains("style="), "no style attribute");
        assert!(!html.contains("<style"), "no style element");
    }

    #[test]
    fn the_page_links_the_id_list_for_blocked_scripts_and_other_versions() {
        let html = page();
        assert!(html.contains("href=\"ids.html\""));
        let blocked = html
            .split("id=\"fb:search-blocked\"")
            .nth(1)
            .and_then(|rest| rest.split("</p>").next())
            .unwrap();
        assert!(blocked.contains("Search needs scripts, which this browser blocks."));
        assert!(blocked.contains("href=\"ids.html\""));
        assert!(
            !html.contains("id=\"fb:search-blocked\" hidden"),
            "the blocked line shows until search.js hides it"
        );
        let version = html
            .split("id=\"fb:search-version\"")
            .nth(1)
            .and_then(|rest| rest.split("</p>").next())
            .unwrap();
        assert!(version.contains("Search is unavailable for this index version. Use the"));
        assert!(version.contains("<a href=\"ids.html\">ID list</a>."));
    }

    #[test]
    fn the_page_loads_search_js_by_src() {
        let html = page();
        assert!(html.contains("<script src=\"assets/search.js\" defer></script>"));
        // The index is loaded on demand by search.js, never by the page.
        assert!(!html.contains("search-index.js"));
        // No inline script: every <script> carries a src.
        for (at, _) in html.match_indices("<script") {
            let tag = &html[at..at + html[at..].find('>').unwrap()];
            assert!(tag.contains(" src="), "inline script: {tag}");
        }
    }

    #[test]
    fn the_page_has_no_static_skeleton() {
        let html = page();
        assert!(
            !html.contains("skeleton"),
            "the Skeleton is added by search.js after 300 ms"
        );
    }

    #[test]
    fn the_page_has_the_result_list_and_input() {
        let html = page();
        for id in [
            "fb:search-q",
            "fb:search-out",
            "fb:search-results",
            "fb:search-failed",
            "fb:search-version",
            "fb:search-more",
            "fb:search-total",
            "fb:search-none",
            "fb:search-latest",
            "fb:search-blocked",
        ] {
            assert_eq!(html.matches(&format!("id=\"{id}\"")).count(), 1, "{id}");
        }
        assert!(html.contains("Showing 50 of"));
        assert!(html.contains("role=\"listbox\""));
    }

    #[test]
    fn latest_pages_are_page_leads_by_as_of_with_links() {
        let index: Index = with_view(search_index::build);
        let rows = search_page::latest_pages(&index);
        assert!(!rows.is_empty(), "corpus-mini has pages");
        assert!(rows.len() <= LATEST_PAGES);
        let dates: Vec<String> = rows
            .iter()
            .map(|r| {
                index
                    .docs
                    .iter()
                    .find(|d| d.id == r.id)
                    .unwrap()
                    .as_of
                    .clone()
            })
            .collect();
        for pair in dates.windows(2) {
            assert_ne!(
                utf16_cmp(&pair[0], &pair[1]),
                std::cmp::Ordering::Less,
                "latest first: {dates:?}"
            );
        }
        for row in &rows {
            assert!(!row.id.contains('#'), "a lead, not a section: {}", row.id);
            assert_eq!(Some(row.href.clone()), unit_href(&row.kind, &row.id));
            assert!(row.meta.starts_with(&row.kind));
        }
        let html = page();
        for row in &rows {
            assert!(
                html.contains(&format!("data-id=\"{}\"", row.id)),
                "{}",
                row.id
            );
        }
    }

    // control: WB-03
    #[test]
    fn search_js_is_a_classic_script_within_budget() {
        let script = include_str!("../assets/search.js");
        assert!(
            script.len() <= 10_000,
            "search.js is {} bytes",
            script.len()
        );
        for banned in [
            "fetch(",
            "eval(",
            "new Function",
            "XMLHttpRequest",
            "localStorage",
            // Text goes in with textContent only.
            "innerHTML",
            "outerHTML",
            "insertAdjacentHTML",
            "document.write",
            "DOMParser",
        ] {
            assert!(!script.contains(banned), "search.js uses {banned}");
        }
        for line in script.lines() {
            let line = line.trim_start();
            assert!(
                !line.starts_with("import") && !line.starts_with("export"),
                "search.js is a classic script: {line}"
            );
        }
        assert!(script.contains("SKELETON_DELAY_MS = 300"));
        assert!(script.contains("window.FB_SEARCH"));
    }
}
