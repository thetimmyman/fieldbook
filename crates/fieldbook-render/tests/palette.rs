//! Integration tests for the SearchTrigger, the CommandPalette and the ShortcutsDialog on every
//! built page (`search-palette`, `docs/reference/search.md#the-site-index`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod palette {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::lint::APP_JS;
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::site::{self, Site};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const SCRIPT: &str = include_str!("../assets/app.js");
    const BASE_HTML: &str = include_str!("../templates/base.html");
    const HARNESS_HTML: &str = include_str!("../../../xtask/harness/shortcuts.html");
    const CSP_META: &str = "http-equiv=\"Content-Security-Policy\"";

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

    fn built() -> Site {
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
        let info = BuildInfo::new(None, Some(COMMIT.to_owned()), BuildProfile::Silent, None);
        site::render(&view, &info).unwrap()
    }

    /// Every built HTML page, with its path, in path order.
    fn pages(site: &Site) -> Vec<(String, String)> {
        let pages: Vec<(String, String)> = site
            .files()
            .iter()
            .filter(|(path, _)| path.ends_with(".html"))
            .map(|(path, bytes)| (path.clone(), String::from_utf8(bytes.clone()).unwrap()))
            .collect();
        assert!(pages.len() > 10, "the fixture builds the frame pages");
        pages
    }

    /// `../` once per directory level of `path`.
    fn prefix(path: &str) -> String {
        "../".repeat(path.matches('/').count())
    }

    /// The opening tag that contains `needle`.
    fn tag_with<'a>(html: &'a str, needle: &str) -> &'a str {
        let at = html.find(needle).unwrap_or_else(|| panic!("no `{needle}`"));
        let start = html[..at].rfind('<').unwrap();
        let end = at + html[at..].find('>').unwrap();
        &html[start..=end]
    }

    #[test]
    fn every_page_has_the_trigger_as_a_link_to_the_id_list() {
        let site = built();
        assert!(
            site.files().contains_key("ids.html"),
            "the trigger's target is built"
        );
        for (path, html) in pages(&site) {
            assert_eq!(html.matches("id=\"fb:searchBtn\"").count(), 1, "{path}");
            let tag = tag_with(&html, "id=\"fb:searchBtn\"");
            let href = format!("href=\"{}ids.html\"", prefix(&path));
            assert!(
                tag.starts_with("<a ") && tag.contains(&href),
                "{path}: without scripts the trigger is a plain link, {href}: {tag}"
            );
        }
    }

    #[test]
    fn every_page_has_both_dialogs_empty_until_script_fills_them() {
        for (path, html) in pages(&built()) {
            for (id, attr) in [
                ("fb:palette", "aria-label=\"Search the library\""),
                ("fb:keys", "aria-labelledby=\"fb:keysTitle\""),
            ] {
                let open = format!("id=\"{id}\" {attr}></dialog>");
                assert_eq!(html.matches(&open).count(), 1, "{path}: dialog {id}");
                assert!(
                    tag_with(&html, &format!("id=\"{id}\"")).starts_with("<dialog "),
                    "{path}: {id} is a native dialog"
                );
            }
            let keys = tag_with(&html, "id=\"fb:keysBtn\"");
            assert!(
                keys.starts_with("<button ") && keys.ends_with(" hidden>"),
                "{path}: the Keyboard shortcuts link waits for the script: {keys}"
            );
        }
    }

    #[test]
    fn every_page_loads_search_js_before_app_js() {
        for (path, html) in pages(&built()) {
            let p = prefix(&path);
            let strings = html
                .find(&format!("src=\"{p}assets/strings.js\" defer"))
                .unwrap();
            let search = format!("<script src=\"{p}assets/search.js\" defer></script>");
            assert_eq!(html.matches(&search).count(), 1, "{path}: search.js once");
            let search_at = html.find(&search).unwrap();
            let app = html
                .find(&format!("src=\"{p}assets/app.js\" defer"))
                .unwrap();
            assert!(
                strings < search_at && search_at < app,
                "{path}: script order"
            );
        }
    }

    #[test]
    fn every_page_has_no_style_attribute_and_the_csp_meta_once() {
        for (path, html) in pages(&built()) {
            assert!(!html.contains("style=\""), "{path} has a style attribute");
            assert_eq!(html.matches(CSP_META).count(), 1, "{path}: CSP meta");
        }
    }

    /// `/`, `?` and Ctrl K are read in one handler; the palette and the dialog open modally;
    /// the single-key choice is read and written only inside `try`.
    #[test]
    fn the_script_opens_dialogs_modally_and_keeps_its_choice_in_try() {
        assert!(SCRIPT.contains("dlg.showModal()"));
        assert!(!SCRIPT.contains(".show()"), "never a non-modal dialog");
        assert!(
            !SCRIPT.contains("aria-modal"),
            "never a div posing as a modal"
        );
        assert!(!SCRIPT.contains("innerHTML"), "rows are text, never markup");
        assert!(SCRIPT.contains("const KEYS_KEY = 'fieldbook.singleKeys';"));
        assert!(SCRIPT.contains("singleKeys ? 'on' : 'off'"));
        for read in [
            "window.localStorage.getItem(KEYS_KEY)",
            "window.localStorage.setItem(KEYS_KEY",
        ] {
            let at = SCRIPT.find(read).unwrap();
            let before = &SCRIPT[..at];
            assert!(
                before.rfind("try {").unwrap() > before.rfind("} catch").unwrap(),
                "{read} sits inside try"
            );
        }
    }

    #[test]
    fn app_js_stays_within_its_budget() {
        let size = u64::try_from(SCRIPT.len()).unwrap();
        assert!(
            size <= APP_JS,
            "app.js is {size} bytes, budget {APP_JS} (FB0605)"
        );
    }

    #[test]
    fn the_script_names_the_shortcuts_dialog_by_its_h2_and_quotes_the_search_row() {
        assert!(SCRIPT.contains("{ id: 'fb:keysTitle' }"));
        assert!(SCRIPT.contains("placeholder: 'Pages, clauses and terms\\u2026'"));
        assert!(SCRIPT.contains("'Search for \\u201c' + q + '\\u201d'"));
    }

    #[test]
    fn harness_dialogs_match_base() {
        for id in ["fb:palette", "fb:keys"] {
            assert_eq!(
                tag_with(BASE_HTML, &format!("id=\"{id}\"")),
                tag_with(HARNESS_HTML, &format!("id=\"{id}\"")),
                "dialog {id} does not match between base.html and shortcuts.html"
            );
            assert!(
                tag_with(BASE_HTML, &format!("id=\"{id}\"")).starts_with("<dialog "),
                "dialog {id} does not start with '<dialog '"
            );
        }
    }

    #[test]
    fn the_search_page_has_static_listbox_roles() {
        let site = built();
        let html = String::from_utf8(site.files().get("search.html").unwrap().clone()).unwrap();
        assert_eq!(
            html.matches("role=\"listbox\"").count(),
            2,
            "search.html must contain exactly two role=\"listbox\" occurrences"
        );
        let latest_tag = tag_with(&html, "aria-label=\"Latest pages\"");
        let results_tag = tag_with(&html, "id=\"fb:search-results\"");
        assert!(
            latest_tag.contains("role=\"listbox\""),
            "tag with aria-label=\"Latest pages\" must contain role=\"listbox\""
        );
        assert!(
            results_tag.contains("role=\"listbox\""),
            "tag with id=\"fb:search-results\" must contain role=\"listbox\""
        );
        assert!(
            results_tag.contains("aria-label=\"Results\""),
            "tag with id=\"fb:search-results\" must contain aria-label=\"Results\""
        );
        let sr_row_count = html.matches("<li class=\"sr-row\"").count();
        let sr_option_count = html.matches("<li class=\"sr-row\" role=\"option\"").count();
        assert!(
            sr_row_count > 0,
            "expected at least one sr-row in latest list"
        );
        assert_eq!(
            sr_row_count, sr_option_count,
            "every <li class=\"sr-row\" row in the latest list must have role=\"option\""
        );
        for (path, page_html) in pages(&site) {
            assert!(
                !page_html.contains("role=\"combobox\""),
                "{path} must not contain role=\"combobox\""
            );
        }
    }
}
