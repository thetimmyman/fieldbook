//! Integration tests for `fieldbook_render::{home, area, svg}` (`build-home-and-areas`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod home {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::site::{Site, render};
    use fieldbook_render::svg::{CoverageCounts, coverage_bar};

    /// The default workspace in HANDOFF order.
    const SECTIONS: [&str; 8] = [
        "overview",
        "processes",
        "procedures",
        "templates",
        "module",
        "library-pages",
        "history",
        "questions",
    ];

    const NEW_AREA: &str = "\n[[area]]\ncode = \"mna\"\nslug = \"mergers\"\nname = \"M&A\"\nsummary = \"Synthetic empty area for fixture tests\"\nsubareas = [\"deals\"]\n";

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

    /// The `corpus-mini` fixture, with `edit` applied to `(corpus.toml, claims.csv)`.
    fn tree_with(edit: impl Fn(&str, &str) -> (String, String)) -> Tree {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
        let corpus = root.join("corpus");
        let (toml, claims) = edit(
            &std::fs::read_to_string(corpus.join("corpus.toml")).unwrap(),
            &std::fs::read_to_string(corpus.join("registers/claims.csv")).unwrap(),
        );
        let mut tree = Tree::new();
        insert_all(&mut tree, &corpus, "corpus");
        tree.insert_bytes("corpus/corpus.toml", toml.into_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", claims.into_bytes());
        tree
    }

    fn fixture_tree() -> Tree {
        tree_with(|toml, claims| (format!("{toml}{NEW_AREA}"), claims.to_owned()))
    }

    fn load_view(tree: Tree) -> (Model, LabelledView) {
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
        (model, view)
    }

    fn built(tree: Tree) -> Site {
        let (_model, view) = load_view(tree);
        render(&view, &BuildInfo::default()).unwrap()
    }

    fn page(site: &Site, path: &str) -> String {
        String::from_utf8(
            site.files()
                .get(path)
                .unwrap_or_else(|| panic!("{path} must be built"))
                .clone(),
        )
        .unwrap()
    }

    fn main_of(html: &str) -> &str {
        let start = html.find("<main").unwrap();
        let end = html.rfind("</main>").unwrap();
        &html[start..end]
    }

    /// The text of the `<section id="id">` of an area page, up to the next workspace section.
    fn section<'a>(html: &'a str, id: &str) -> &'a str {
        let marker = format!("<section class=\"ws-section\" id=\"{id}\">");
        let start = html
            .find(&marker)
            .unwrap_or_else(|| panic!("section {id} must exist"));
        let rest = &html[start + marker.len()..];
        rest.find("<section class=\"ws-section\"")
            .map_or(rest, |end| &rest[..end])
    }

    /// Every `(class, x, width)` segment of a bar, in hundredths of a viewBox unit.
    fn segments(svg: &str) -> Vec<(String, u32, u32)> {
        svg.split("<rect class=\"seg ")
            .skip(1)
            .map(|chunk| {
                let class = chunk.split('"').next().unwrap().to_owned();
                (
                    class,
                    hundredths(attr(chunk, "x")),
                    hundredths(attr(chunk, "width")),
                )
            })
            .collect()
    }

    fn attr<'a>(chunk: &'a str, name: &str) -> &'a str {
        let marker = format!(" {name}=\"");
        let start = chunk.find(&marker).unwrap() + marker.len();
        chunk[start..].split('"').next().unwrap()
    }

    fn hundredths(value: &str) -> u32 {
        let (whole, frac) = value.split_once('.').unwrap();
        whole.parse::<u32>().unwrap() * 100 + frac.parse::<u32>().unwrap()
    }

    const fn counts(
        supported: usize,
        unchecked: usize,
        contradicted: usize,
        stale: usize,
        unresolved: usize,
    ) -> CoverageCounts {
        CoverageCounts {
            supported,
            unchecked,
            contradicted,
            stale,
            unresolved,
        }
    }

    #[test]
    fn one_area_page_per_area() {
        let (_model, view) = load_view(fixture_tree());
        let site = render(&view, &BuildInfo::default()).unwrap();
        let areas = &view.corpus().unwrap().area;
        assert_eq!(areas.len(), 3);
        let built: Vec<&str> = site
            .files()
            .keys()
            .filter(|key| key.starts_with("a/"))
            .map(String::as_str)
            .collect();
        assert_eq!(built, ["a/cmp.html", "a/est.html", "a/mna.html"]);
        for area in areas {
            let html = page(&site, &format!("a/{}.html", area.code));
            let main = main_of(&html);
            assert!(main.contains(&format!("<h1>{}</h1>", area.name.replace('&', "&#38;"))));
            assert!(main.contains(&area.summary), "{} summary", area.code);
            let mut last = 0;
            for id in SECTIONS {
                let at = main
                    .find(&format!("<section class=\"ws-section\" id=\"{id}\">"))
                    .unwrap_or_else(|| panic!("{} lacks section {id}", area.code));
                assert!(at > last, "{} section {id} is out of order", area.code);
                last = at;
                assert!(
                    main.contains(&format!("<a href=\"#{id}\">")),
                    "{} jump list lacks {id}",
                    area.code
                );
            }
        }
    }

    #[test]
    fn coverage_segments_sum_to_track() {
        for set in [
            counts(1, 1, 1, 1, 1),
            counts(3, 0, 0, 0, 0),
            counts(0, 0, 7, 0, 0),
            counts(0, 0, 0, 2, 0),
            counts(0, 0, 0, 0, 9),
            counts(1, 2, 3, 4, 5),
            counts(1, 0, 1, 0, 1),
            counts(999, 1, 0, 0, 1),
            counts(1, 1, 1, 0, 0),
            counts(7, 11, 13, 17, 19),
        ] {
            let svg = coverage_bar(&set);
            let segs = segments(&svg);
            let width: u32 = segs.iter().map(|s| s.2).sum();
            assert_eq!(width, 10_000, "the widths of {set:?} do not fill the track");
            let mut at = 0;
            for (class, x, seg_width) in &segs {
                assert_eq!(*x, at, "segment {class} of {set:?} leaves a gap");
                assert!(*seg_width > 0, "segment {class} of {set:?} is empty");
                at += seg_width;
            }
            let label = format!(
                "{} of {} supported: {} supported, {} unchecked, {} contradicted, {} stale, {} unresolved",
                set.supported,
                set.total(),
                set.supported,
                set.unchecked,
                set.contradicted,
                set.stale,
                set.unresolved
            );
            assert!(
                svg.contains(&format!("aria-label=\"{label}\"")),
                "{svg} must repeat all five counts of {set:?}"
            );
        }

        let empty = coverage_bar(&CoverageCounts::default());
        assert!(segments(&empty).is_empty(), "zero claims fill nothing");
        assert!(empty.contains("<rect class=\"track\""), "the track stays");
        assert!(empty.contains("aria-label=\"No claims\""));
    }

    #[test]
    fn coverage_counts_each_state_once() {
        // One claim of each state in one area, in a real permitted view.
        let (_model, view) = load_view(tree_with(|toml, claims| {
            let row = claims
                .lines()
                .find(|l| l.starts_with("CL-CMP-0002,"))
                .unwrap();
            let mut out = claims.to_owned();
            for (id, state) in [
                ("0091", "contradicted"),
                ("0092", "stale"),
                ("0093", "unresolved"),
            ] {
                let copy = row
                    .replace("CL-CMP-0002", &format!("CL-CMP-{id}"))
                    .replace(",unchecked,", &format!(",{state},"));
                out.push_str(&copy);
                out.push('\n');
            }
            (toml.to_owned(), out)
        }));
        let site = render(&view, &BuildInfo::default()).unwrap();
        let cmp = page(&site, "a/cmp.html");
        assert!(
            cmp.contains(
                "aria-label=\"1 of 5 supported: 1 supported, 1 unchecked, 1 contradicted, 1 stale, 1 unresolved\""
            ),
            "five claims must be accounted for"
        );
        assert_eq!(
            segments(section(&cmp, "overview"))
                .iter()
                .map(|s| s.0.as_str())
                .collect::<Vec<_>>(),
            ["s-ok", "s-un", "s-bad", "s-stale", "s-unres"]
        );
        let index = page(&site, "index.html");
        assert!(index.contains("1 of 5 supported"), "the tile repeats it");
        assert!(index.contains("0 of 3 supported"), "est is counted alone");
        // Flags by exception: unchecked claims are not changes; stale and contradicted ones are.
        let changes = section(&cmp, "overview");
        assert!(changes.contains("CL-CMP-0091"));
        assert!(changes.contains("CL-CMP-0092"));
        assert!(!changes.contains("CL-CMP-0093"));
        assert!(!changes.contains("CL-CMP-0002"));
    }

    #[test]
    fn coverage_svg_has_no_text_and_no_style() {
        for set in [counts(1, 2, 3, 4, 5), CoverageCounts::default()] {
            let svg = coverage_bar(&set);
            assert!(svg.starts_with("<svg class=\"cover\" viewBox=\"0 0 100 10\""));
            assert!(svg.contains("preserveAspectRatio=\"none\""));
            assert!(svg.contains("role=\"img\""));
            assert!(!svg.contains("style"), "{svg}");
            assert!(!svg.contains("<text"), "{svg}");
            assert!(!svg.contains("<title"), "{svg}");
            assert!(!svg.contains("fill="), "colour comes from classes: {svg}");
            let without_tags: String = svg
                .split('<')
                .filter_map(|part| part.split_once('>').map(|(_, text)| text))
                .collect();
            assert_eq!(without_tags, "", "no text inside the SVG");
        }
    }

    #[test]
    fn empty_sections_show_their_line() {
        let site = built(fixture_tree());
        let html = page(&site, "a/mna.html");
        let unavailable = [
            (
                "processes",
                "The process list is not available until Release 3.",
            ),
            (
                "procedures",
                "The procedure list is not available until Release 3.",
            ),
            ("templates", "The template list is not available until E1."),
            (
                "module",
                "The module card is not available until Release 3.",
            ),
            ("history", "History is not available until a later release."),
        ];
        for (id, line) in unavailable {
            let body = section(&html, id);
            assert!(body.contains("class=\"unavailable\""), "{id}: {body}");
            assert!(body.contains(line), "{id} must say {line}: {body}");
        }
        let library = section(&html, "library-pages");
        assert!(library.contains("No pages in M&#38;A yet."), "{library}");
        assert!(library.contains("sources.html"), "{library}");
        assert!(library.contains("Learn path"), "{library}");
        let questions = section(&html, "questions");
        assert!(
            questions.contains(
                "No questions for M&#38;A yet. That means none recorded here, not that none exist."
            ),
            "{questions}"
        );
        let overview = section(&html, "overview");
        assert!(overview.contains("Nothing approved or proven here yet."));
        assert!(overview.contains("Nothing upstream has moved."));
        assert!(overview.contains("Continue working is not available until"));
        assert!(overview.contains("aria-label=\"No claims\""));
    }

    #[test]
    fn index_lists_day_one_questions_and_briefs() {
        let site = built(fixture_tree());
        let html = page(&site, "index.html");
        let main = main_of(&html);
        assert_eq!(main.matches("<h1").count(), 1);
        assert!(main.contains("Q-001"), "the day-one question");
        assert!(!main.contains("Q-002"), "a question not marked day one");
        assert!(
            main.contains("p/est-rate-window-brief.html"),
            "the fixture's brief"
        );
        assert!(main.contains("id=\"areas\""));
        assert_eq!(main.matches("class=\"area-tile\"").count(), 3);
        assert_eq!(main.matches("role=\"img\"").count(), 3, "one bar per tile");
        for code in ["cmp", "est", "mna"] {
            assert!(main.contains(&format!("href=\"a/{code}.html\"")), "{code}");
        }
        assert!(main.contains("Next up is not available until"));
        assert!(main.contains("XC-01"), "an open conflict is a change");
        assert!(
            main.contains("p/est-baseline-template.html"),
            "a template page is ready to reuse"
        );
        assert!(
            html.contains("href=\"index.html#areas\""),
            "the aliases reach the anchor"
        );
    }

    #[test]
    fn purpose_lines_drop_citation_markers() {
        let mut tree = fixture_tree();
        let path = "corpus/pages/compliance/business-systems/cmp-system-review.md";
        let source = String::from_utf8(tree.get_bytes(path).unwrap().to_vec()).unwrap();
        tree.insert_bytes(
            path,
            source.replace(
                "billing withhold.\n",
                "billing withhold [@CL-CMP-0001; @CL-CMP-0002].\n",
            ),
        );
        let site = built(tree);
        let html = page(&site, "a/cmp.html");
        let library = section(&html, "library-pages");
        assert!(
            library.contains("a synthetic billing withhold.</span>"),
            "{library}"
        );
        assert!(!library.contains("[@"), "{library}");
    }

    #[test]
    fn flags_are_by_exception() {
        let site = built(fixture_tree());
        for path in ["index.html", "a/cmp.html"] {
            let html = page(&site, path);
            let main = main_of(&html);
            assert!(!main.contains("Primary source supported"), "{path}");
            assert!(!main.contains("chip ok"), "{path}");
        }
    }

    #[test]
    fn every_page_has_the_csp_meta_and_no_style_attribute() {
        let site = built(fixture_tree());
        for path in ["index.html", "a/cmp.html", "a/est.html", "a/mna.html"] {
            let html = page(&site, path);
            assert!(
                html.contains(&format!(
                    "http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\""
                )),
                "{path} must carry the CSP meta"
            );
            assert!(!html.contains("style=\""), "{path} must have no style attr");
            assert!(
                !html.contains("<style"),
                "{path} must have no style element"
            );
        }
    }
}
