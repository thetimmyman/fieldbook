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
        ];
        for (id, line) in unavailable {
            let body = section(&html, id);
            assert!(body.contains("class=\"unavailable\""), "{id}: {body}");
            assert!(body.contains(line), "{id} must say {line}: {body}");
        }
        let history = section(&html, "history");
        assert!(
            history.contains("Nothing recorded yet. History fills as pages and registers change."),
            "{history}"
        );
        assert!(history.contains("class=\"muted\""), "{history}");
        assert!(!history.contains("History is not available"), "{history}");
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
        assert!(!main.contains("Next up is not available"));
        assert!(!main.contains("ADR 0001"));
        assert!(!main.contains("id=\"next-up\""));
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

    /// The `ChangeItem` rows of kind "Source update" in a built page.
    fn source_updates(html: &str) -> Vec<&str> {
        html.split("<li class=\"tu-item chg\">")
            .skip(1)
            .map(|chunk| chunk.split("</li>").next().unwrap())
            .filter(|row| row.contains("<span class=\"k\">Source update</span>"))
            .collect()
    }

    /// `fixture_tree` with the lifecycle of one version in `sources.csv` set to quarantined.
    fn with_quarantined(marker: &str) -> Tree {
        let mut tree = fixture_tree();
        let path = "corpus/registers/sources.csv";
        let sources = String::from_utf8(tree.get_bytes(path).unwrap().to_vec()).unwrap();
        let edited: Vec<String> = sources
            .lines()
            .map(|line| {
                if line.starts_with(marker) {
                    line.replace(",active,", ",quarantined,")
                } else {
                    line.to_owned()
                }
            })
            .collect();
        assert_ne!(edited.join("\n") + "\n", sources, "{marker} must be edited");
        tree.insert_bytes(path, edited.join("\n") + "\n");
        tree
    }

    #[test]
    fn superseded_source_version_is_a_change_on_the_home_page() {
        let site = built(fixture_tree());
        let html = page(&site, "index.html");
        // S-0002-v2 supersedes S-0002-v1 in the fixture; only the older version is listed.
        let rows = source_updates(main_of(&html));
        assert_eq!(rows.len(), 1, "{rows:?}");
        let row = rows[0];
        assert!(row.contains("Synthetic Pricing Guide v1"), "{row}");
        assert!(row.contains("v1 superseded"), "{row}");
        assert!(
            row.contains("Re-check<span class=\"sr-only\"> S-0002-v1</span>"),
            "the button is named with its item: {row}"
        );
        assert!(row.contains("href=\"s/S-0002-v1.html\""), "{row}");
        assert!(!row.contains("S-0002-v2"), "{row}");
    }

    #[test]
    fn source_update_stays_with_the_area_that_cites_it() {
        // No claim in the fixture cites S-0002-v1, so no area page lists it.
        let site = built(fixture_tree());
        for path in ["a/cmp.html", "a/est.html", "a/mna.html"] {
            let html = page(&site, path);
            assert_eq!(source_updates(main_of(&html)), Vec::<&str>::new(), "{path}");
        }
        // One est claim citing the older version brings it to est, and only to est.
        let site = built(tree_with(|toml, claims| {
            let cited = claims.replace(
                "S-0002,2,text_section,sec. 4",
                "S-0002,1,text_section,sec. 4",
            );
            assert_ne!(cited, claims, "the est claim must be edited");
            (format!("{toml}{NEW_AREA}"), cited)
        }));
        let est = page(&site, "a/est.html");
        let rows = source_updates(main_of(&est));
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(rows[0].contains("S-0002-v1"), "{}", rows[0]);
        for path in ["a/cmp.html", "a/mna.html"] {
            let html = page(&site, path);
            assert_eq!(source_updates(main_of(&html)), Vec::<&str>::new(), "{path}");
        }
    }

    #[test]
    fn held_source_versions_make_no_source_update() {
        // A held older version is not listed, and a held newer version supersedes nothing.
        for marker in ["S-0002,1,", "S-0002,2,"] {
            let site = built(with_quarantined(marker));
            let html = page(&site, "index.html");
            assert_eq!(
                source_updates(main_of(&html)),
                Vec::<&str>::new(),
                "{marker}"
            );
        }
    }

    #[test]
    fn a_rare_state_stays_visible_in_the_bar() {
        // One contradicted claim among thousands: the size of the real areas.
        let segs = segments(&coverage_bar(&counts(0, 9_484, 1, 0, 0)));
        let bad = segs.iter().find(|s| s.0 == "s-bad").unwrap();
        assert!(bad.2 >= 200, "a lone state is drawn too thin: {}", bad.2);
        assert_eq!(segs.iter().map(|s| s.2).sum::<u32>(), 10_000);
        // A larger state keeps a larger share.
        let segs = segments(&coverage_bar(&counts(1, 3, 0, 0, 0)));
        assert!(segs[0].2 < segs[1].2, "{segs:?}");
    }

    #[test]
    fn three_up_sections_are_named_by_their_headings() {
        let site = built(fixture_tree());
        for (path, heading) in [("index.html", "h2"), ("a/cmp.html", "h3")] {
            let html = page(&site, path);
            for (id, title) in [
                ("tu-continue-working", "Continue working"),
                ("tu-ready-to-reuse", "Ready to reuse"),
                ("tu-changes-to-review", "Changes to review"),
            ] {
                assert!(
                    html.contains(&format!(
                        "<section class=\"panel\" aria-labelledby=\"{id}\"><div class=\"panel-head\"><{heading} class=\"panel-title\" id=\"{id}\">{title}</{heading}>"
                    )),
                    "{path}: the {title} section must be named by its heading"
                );
            }
        }
    }

    #[test]
    fn withdrawn_questions_are_not_asked() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
        let csv = std::fs::read_to_string(root.join("corpus/registers/questions.csv")).unwrap();
        let withdrawn: String = csv
            .lines()
            .map(|l| {
                if l.starts_with("Q-001,") {
                    l.replace(",open,", ",withdrawn,")
                } else {
                    l.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert_ne!(csv.trim_end(), withdrawn.trim_end(), "the row must change");
        let mut tree = fixture_tree();
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            format!("{withdrawn}\n").into_bytes(),
        );
        let site = built(tree);
        assert!(
            !main_of(&page(&site, "index.html")).contains("Q-001"),
            "a withdrawn question is no longer asked on the home page"
        );
        assert!(
            !section(&page(&site, "a/cmp.html"), "questions").contains("Q-001"),
            "a withdrawn question is no longer asked on its area page"
        );
    }

    #[test]
    fn changes_list_contradicted_then_stale_and_consequential_first() {
        let site = built(tree_with(|toml, claims| {
            let row = claims
                .lines()
                .find(|l| l.starts_with("CL-CMP-0002,"))
                .unwrap();
            assert!(
                row.contains(",true,codex,"),
                "the row must be consequential"
            );
            let mut out = claims.to_owned();
            for (id, state, consequential) in [
                ("0091", "stale", "true"),
                ("0092", "contradicted", "false"),
                ("0093", "contradicted", "true"),
            ] {
                let copy = row
                    .replace("CL-CMP-0002", &format!("CL-CMP-{id}"))
                    .replace(",unchecked,", &format!(",{state},"))
                    .replace(",true,codex,", &format!(",{consequential},codex,"));
                out.push_str(&copy);
                out.push('\n');
            }
            (format!("{toml}{NEW_AREA}"), out)
        }));
        let index = page(&site, "index.html");
        let at = |id: &str| {
            index
                .find(id)
                .unwrap_or_else(|| panic!("{id} must be listed"))
        };
        assert!(at("CL-CMP-0093") < at("CL-CMP-0092"), "consequential first");
        assert!(at("CL-CMP-0092") < at("CL-CMP-0091"), "contradicted first");
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

    #[test]
    fn no_page_names_adr_0001() {
        let site = built(fixture_tree());
        for (path, bytes) in site.files() {
            if path.ends_with(".html") {
                let text = String::from_utf8_lossy(bytes);
                assert!(!text.contains("ADR 0001"), "{path}");
            }
        }
    }
}
