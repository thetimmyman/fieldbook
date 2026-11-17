//! Tests for the Day-N chip, the start-date dialog and the plan's today line (`views-and-day-n-2`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod day_n {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::strings;
    use fieldbook_render::lint::{APP_JS, DAY_JS};
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::site::{self, Site, render};

    const PLAN_PART: &str = include_str!("../templates/parts/plan_timeline.html");
    const DAY_SCRIPT: &str = include_str!("../assets/day.js");

    fn insert_all(tree: &mut Tree, dir: &Path, rel: &str) {
        tree.insert_dir(rel);
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let child = format!("{rel}/{}", entry.file_name().into_string().unwrap());
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
        render(&view, &BuildInfo::default()).unwrap()
    }

    /// The text of the `<span class="meta">` that follows each timeline heading.
    fn plan_headers(html: &str) -> Vec<&str> {
        html.split("id=\"fb:plan-timeline-head\">")
            .skip(1)
            .map(|rest| {
                let from = rest.find("<span class=\"meta\">").unwrap() + 19;
                rest[from..].split("</span>").next().unwrap()
            })
            .collect()
    }

    #[test]
    fn plan_timeline_header_text_comes_from_strings() {
        let site = built();
        let mut seen = 0;
        for (path, bytes) in site.files() {
            if !path.ends_with(".html") {
                continue;
            }
            for header in plan_headers(std::str::from_utf8(bytes).unwrap()) {
                assert_eq!(header, strings::set_start_date_plan(), "{path}");
                seen += 1;
            }
        }
        assert!(seen >= 1, "a built page shows a timeline: {seen}");
        // Every include of the part (the day-one page and a checklist page) gets the same text
        // from the one view.
        let view = fieldbook_render::plan::build_from_parsed_windows(&[(
            "Day 1 to Day 30".to_owned(),
            1,
            30,
        )]);
        assert_eq!(view.unset_header, strings::set_start_date_plan());
        assert!(
            !PLAN_PART.contains(strings::set_start_date_plan()),
            "plan_timeline.html holds no literal of the header"
        );
    }

    #[test]
    fn day_js_ships_beside_app_js_and_stays_within_its_cap() {
        let site = built();
        assert_eq!(site.files()["assets/day.js"], site::DAY_JS.as_bytes());
        assert_eq!(site::DAY_JS, DAY_SCRIPT);
        assert!(
            DAY_SCRIPT.len() as u64 <= DAY_JS,
            "{} bytes",
            DAY_SCRIPT.len()
        );
        assert!(site::APP_JS.len() as u64 <= APP_JS);
        for (path, bytes) in site.files() {
            if path.ends_with(".html") {
                let html = std::str::from_utf8(bytes).unwrap();
                let prefix = "../".repeat(path.matches('/').count());
                let app = html
                    .find(&format!("src=\"{prefix}assets/app.js\" defer"))
                    .unwrap();
                let day = html
                    .find(&format!("src=\"{prefix}assets/day.js\" defer"))
                    .unwrap();
                assert!(app < day, "{path}: day.js loads after app.js");
            }
        }
    }

    #[test]
    fn app_css_hides_the_chip_below_1280px_and_a_marked_prompt_from_1280px() {
        let css = include_str!("../assets/app.css");
        let below = css.find("@media (max-width: 1279px) {").unwrap();
        let rule = css[below..].find(".day-chip {").unwrap();
        assert!(css[below + rule..].starts_with(".day-chip {\n    display: none;"));
        let from = css.find("@media (min-width: 1280px) {").unwrap();
        let rest = &css[from..];
        assert!(
            rest[..rest.find("\n}\n").unwrap()]
                .contains(".tu-item[data-day] {\n    display: none;"),
            "a prompt marked with data-day hides only from 1280px"
        );
        assert!(css.contains(".day-chip[hidden],"));
    }

    #[test]
    fn day_js_keeps_storage_inside_try() {
        let mut uses = 0;
        for (idx, _) in DAY_SCRIPT.match_indices("localStorage") {
            uses += 1;
            // A stack of open braces, each noting whether a `try` led into it.
            let mut stack = Vec::new();
            let mut led_by_try = false;
            let prefix = &DAY_SCRIPT[..idx];
            let bytes = prefix.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i..].starts_with(b"try")
                    && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
                    && !bytes.get(i + 3).is_some_and(u8::is_ascii_alphanumeric)
                {
                    led_by_try = true;
                    i += 3;
                    continue;
                }
                match bytes[i] {
                    b'{' => stack.push(std::mem::take(&mut led_by_try)),
                    b'}' => {
                        stack.pop();
                        led_by_try = false;
                    }
                    c if !c.is_ascii_whitespace() => led_by_try = false,
                    _ => {}
                }
                i += 1;
            }
            assert!(
                stack.iter().any(|&t| t),
                "localStorage at byte {idx} of day.js must sit inside `try`"
            );
        }
        assert!(uses >= 3, "getItem, setItem and removeItem: {uses}");
    }
}
