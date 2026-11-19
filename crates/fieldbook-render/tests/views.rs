//! Integration tests for the view pages `a/<code>.html` of `fieldbook_render::area`
//! (`views-and-day-n`). Every page and view added here is synthetic.

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod views {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::site::{Site, render};

    /// The default workspace (the areas, PGM and any other view) in HANDOFF order.
    const DEFAULT: [(&str, &str); 8] = [
        ("overview", "Overview"),
        ("processes", "Processes"),
        ("procedures", "Procedures"),
        ("templates", "Templates"),
        ("module", "Module"),
        ("library-pages", "Library pages"),
        ("history", "History"),
        ("questions", "Questions"),
    ];

    /// The CPY Company profile view in HANDOFF order.
    const CPY: [(&str, &str); 8] = [
        ("background", "Background"),
        ("organization", "Organization"),
        ("processes", "Processes"),
        ("procedures", "Procedures"),
        ("templates", "Templates"),
        ("systems", "Systems"),
        ("history", "History"),
        ("questions", "Questions"),
    ];

    /// The CPT Competitors view in HANDOFF order; its Sources section takes the frozen part name.
    fn cpt() -> [(&'static str, &'static str); 8] {
        [
            ("compare", "Compare"),
            ("profiles", "Profiles"),
            ("moves", "Moves"),
            ("watch-process", "Watch process"),
            ("templates", "Templates"),
            ("library-pages", "Library pages"),
            ("sources", fieldbook_core::strings::parts()[6]),
            ("questions", "Questions"),
        ]
    }

    /// A synthetic markets area with the three views of the design over it.
    const MKT: &str = "\n[[area]]\ncode = \"mkt\"\nslug = \"markets\"\nname = \"Markets\"\nsummary = \"Synthetic markets area for fixture tests\"\nsubareas = [\"company\", \"peers\", \"programs\"]\n\n[[view]]\ncode = \"CPY\"\nname = \"Company profile\"\nfilter = { area = \"mkt\", subarea = \"company\" }\n\n[[view]]\ncode = \"CPT\"\nname = \"Competitors\"\nfilter = { area = \"mkt\", subarea = \"peers\" }\n\n[[view]]\ncode = \"PGM\"\nname = \"Programs\"\nfilter = { area = \"mkt\", subarea = \"programs\" }\n";

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

    /// The `corpus-mini` fixture, with `edit` applied to `corpus.toml`.
    fn tree_with(edit: impl Fn(&str) -> String) -> Tree {
        let corpus =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini/corpus");
        let toml = edit(&std::fs::read_to_string(corpus.join("corpus.toml")).unwrap());
        let mut tree = Tree::new();
        insert_all(&mut tree, &corpus, "corpus");
        tree.insert_bytes("corpus/corpus.toml", toml.into_bytes());
        tree
    }

    /// A synthetic page of `kind` in `area`/`subarea` whose rule holds `rule`.
    fn synthetic_page(
        id: &str,
        title: &str,
        area: &str,
        subarea: &str,
        kind: &str,
        stage: &str,
        rule: &str,
    ) -> String {
        format!(
            "+++\nid = \"{id}\"\ntitle = \"{title}\"\narea = \"{area}\"\nsubarea = \"{subarea}\"\ntype = \"{kind}\"\nstage = {stage}\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic purpose of {id}.\n\n## The rule\n\n{rule}\n\n## How it works\n\nSynthetic body.\n\n## Worked example\n\nSynthetic example.\n\n## Never confuse\n\nSynthetic distinction.\n\n## Company calibration\n\nSynthetic calibration.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-03-01: Added synthetic page.\n"
        )
    }

    fn insert_page(tree: &mut Tree, folder: &str, id: &str, text: String) {
        tree.insert_dir(folder);
        tree.insert_bytes(format!("{folder}/{id}.md"), text.into_bytes());
    }

    /// The fixture with the synthetic markets area, its three views and four synthetic pages.
    fn mkt_tree() -> Tree {
        let mut tree = tree_with(|toml| format!("{toml}{MKT}"));
        tree.insert_dir("corpus/pages/markets");
        for [subarea, id, kind, stage, rule] in [
            [
                "peers",
                "mkt-peer-alpha",
                "reference",
                "2",
                "Synthetic peer rule [@CL-EST-0003].",
            ],
            [
                "peers",
                "mkt-peer-moves",
                "case",
                "4",
                "Synthetic case rule.",
            ],
            [
                "programs",
                "mkt-program-one",
                "concept",
                "1",
                "Synthetic program rule.",
            ],
            [
                "company",
                "mkt-company-one",
                "concept",
                "1",
                "Synthetic company rule.",
            ],
        ] {
            let title = format!("Synthetic page {id}");
            let text = synthetic_page(id, &title, "mkt", subarea, kind, stage, rule);
            insert_page(
                &mut tree,
                &format!("corpus/pages/markets/{subarea}"),
                id,
                text,
            );
        }
        tree
    }

    fn built(tree: Tree) -> Site {
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

    /// The text of the `<section id="id">` of a workspace page, up to the next workspace section.
    fn section<'a>(html: &'a str, id: &str) -> &'a str {
        let marker = format!("<section class=\"ws-section\" id=\"{id}\">");
        let start = html
            .find(&marker)
            .unwrap_or_else(|| panic!("section {id} must exist"));
        let rest = &html[start + marker.len()..];
        rest.find("<section class=\"ws-section")
            .map_or(rest, |end| &rest[..end])
    }

    /// The ids of the workspace sections of `main`, in document order.
    fn section_ids(main: &str) -> Vec<&str> {
        main.split("<section class=\"ws-section\" id=\"")
            .skip(1)
            .map(|chunk| chunk.split('"').next().unwrap())
            .collect()
    }

    fn a_pages(site: &Site) -> Vec<&str> {
        site.files()
            .keys()
            .filter(|key| key.starts_with("a/"))
            .map(String::as_str)
            .collect()
    }

    #[test]
    fn one_page_per_view() {
        let site = built(tree_with(str::to_owned));
        assert_eq!(a_pages(&site), ["a/cmp.html", "a/est.html", "a/prc.html"]);
        let main = page(&site, "a/prc.html");
        assert!(main_of(&main).contains("<h1>Pricing</h1>"));

        let site = built(mkt_tree());
        assert_eq!(
            a_pages(&site),
            [
                "a/cmp.html",
                "a/cpt.html",
                "a/cpy.html",
                "a/est.html",
                "a/mkt.html",
                "a/pgm.html",
                "a/prc.html"
            ]
        );
        for (path, name) in [
            ("a/cpy.html", "Company profile"),
            ("a/cpt.html", "Competitors"),
            ("a/pgm.html", "Programs"),
        ] {
            let html = page(&site, path);
            assert!(
                main_of(&html).contains(&format!("<h1>{name}</h1>")),
                "{path}"
            );
            assert!(
                html.contains(&format!("<span aria-current=\"page\">{name}</span>")),
                "{path} crumbs end at the view name"
            );
        }
    }

    #[test]
    fn view_eyebrow_and_note_follow_the_filter() {
        let site = built(mkt_tree());
        for (path, area, subarea) in [
            ("a/prc.html", "EST", "pricing"),
            ("a/cpy.html", "MKT", "company"),
            ("a/cpt.html", "MKT", "peers"),
            ("a/pgm.html", "MKT", "programs"),
        ] {
            let html = page(&site, path);
            let main = main_of(&html);
            assert!(
                main.contains(&format!("<div class=\"eyebrow\">View over {area}</div>")),
                "{path} eyebrow"
            );
            assert!(
                main.contains(&format!(
                    "<p class=\"view-note\">{area} pages in subarea {subarea}</p>"
                )),
                "{path} note"
            );
            assert!(!main.contains("· workspace"), "{path} is not an area");
        }
        let est = page(&site, "a/est.html");
        let est_main = main_of(&est);
        assert!(est_main.contains("<div class=\"eyebrow\">EST · workspace</div>"));
        assert!(!est_main.contains("view-note"));
        assert!(!est_main.contains("View over"));
    }

    /// The ids of the sections folded into the "Not built yet" disclosure of `main`, in order.
    fn folded_ids(main: &str) -> Vec<&str> {
        main.split("<p class=\"ws-sub\" id=\"")
            .skip(1)
            .map(|chunk| chunk.split('"').next().unwrap())
            .collect()
    }

    #[test]
    fn view_sections_follow_the_design_order() {
        let site = built(mkt_tree());
        for (path, sections) in [
            ("a/prc.html", DEFAULT),
            ("a/pgm.html", DEFAULT),
            ("a/cpy.html", CPY),
            ("a/cpt.html", cpt()),
            ("a/est.html", DEFAULT),
        ] {
            let html = page(&site, path);
            let main = main_of(&html);
            let table: Vec<&str> = sections.iter().map(|(id, _)| *id).collect();
            // The built sections keep the design order, and the unbuilt ones fold in that order.
            let built = section_ids(main);
            let folded = folded_ids(main);
            for (group, ids) in [("built", &built), ("folded", &folded)] {
                let in_order: Vec<&str> = table
                    .iter()
                    .copied()
                    .filter(|id| ids.contains(id))
                    .collect();
                assert_eq!(**ids, in_order, "{path} {group} sections");
            }
            assert_eq!(
                built.len() + folded.len(),
                table.len(),
                "{path}: every section of the design is built or folded once"
            );
            for id in &table {
                assert_ne!(
                    built.contains(id),
                    folded.contains(id),
                    "{path}: {id} is built or folded, not both"
                );
            }
            let jump_start = main.find("<nav class=\"tabs\"").unwrap();
            let jump = &main[jump_start..main[jump_start..].find("</nav>").unwrap() + jump_start];
            let mut last = 0;
            for (id, label) in sections {
                if !built.contains(&id) {
                    continue;
                }
                assert!(
                    section(main, id).contains(&format!("<h2>{label}</h2>")),
                    "{path} section {id} is headed {label}"
                );
                let at = jump
                    .find(&format!("<a href=\"#{id}\">{label}"))
                    .unwrap_or_else(|| panic!("{path} jump list lacks {id}"));
                assert!(at > last, "{path} jump link {id} is out of order");
                last = at;
            }
            assert_eq!(
                jump.matches("<a href=\"#").count(),
                built.len() + 1,
                "{path}: the built sections and the Not built yet link"
            );
        }
    }

    #[test]
    fn view_lists_only_its_own_pages() {
        let mut tree = tree_with(|toml| {
            toml.replace(
                "subareas = [\"pricing\"]",
                "subareas = [\"pricing\", \"other\"]",
            )
        });
        insert_page(
            &mut tree,
            "corpus/pages/estimating/other",
            "est-other-one",
            synthetic_page(
                "est-other-one",
                "Synthetic page of another subarea",
                "est",
                "other",
                "concept",
                "1",
                "Synthetic rule.",
            ),
        );
        let site = built(tree);
        assert!(
            main_of(&page(&site, "a/est.html")).contains("<tr id=\"est-other-one\">"),
            "the area page lists the page of the other subarea"
        );
        let html = page(&site, "a/prc.html");
        let library = section(main_of(&html), "library-pages");
        let rows: Vec<&str> = library
            .split("<tr id=\"")
            .skip(1)
            .map(|chunk| chunk.split('"').next().unwrap())
            .collect();
        assert!(
            !main_of(&html).contains("est-other-one"),
            "a/prc.html shows est-other-one, a page outside subarea pricing; rows: {rows:?}"
        );
        // Stage, then id.
        assert_eq!(
            rows,
            [
                "est-rate-reference",
                "est-calc-01",
                "est-rate-window-brief",
                "est-baseline-template"
            ]
        );
        assert!(library.contains("<caption>Pages in Pricing by stage</caption>"));
    }

    #[test]
    fn view_sections_list_the_views_own_rows() {
        let site = built(mkt_tree());

        // Templates and Questions of the fixture's view come from its four pages.
        let prc = page(&site, "a/prc.html");
        let prc_main = main_of(&prc);
        let templates = section(prc_main, "templates");
        assert!(
            templates.contains("est-baseline-template.html"),
            "{templates}"
        );
        assert!(!templates.contains("est-calc-01.html"), "{templates}");
        let questions = section(prc_main, "questions");
        assert!(questions.contains("Q-002"), "{questions}");
        assert!(!questions.contains("Q-001"), "{questions}");

        // Profiles are the view's reference and concept pages; Sources are what its pages cite.
        let cpt = page(&site, "a/cpt.html");
        let cpt_main = main_of(&cpt);
        let profiles = section(cpt_main, "profiles");
        assert!(profiles.contains("mkt-peer-alpha.html"), "{profiles}");
        assert!(!profiles.contains("mkt-peer-moves.html"), "{profiles}");
        assert!(!profiles.contains("mkt-program-one"), "{profiles}");
        let sources = section(cpt_main, "sources");
        assert!(sources.contains("S-0002"), "{sources}");
        assert!(sources.contains("Synthetic Pricing Guide"), "{sources}");
        assert!(!sources.contains("S-0001"), "{sources}");
        let library = section(cpt_main, "library-pages");
        assert!(library.contains("<tr id=\"mkt-peer-alpha\">"));
        assert!(library.contains("<tr id=\"mkt-peer-moves\">"));
        assert!(!library.contains("mkt-company-one"));

        // A section with no rows says so and invents none.
        assert!(section(cpt_main, "questions").contains(
            "No questions for Competitors yet. That means none recorded here, not that none exist."
        ));
        let pgm = page(&site, "a/pgm.html");
        assert!(section(main_of(&pgm), "library-pages").contains("<tr id=\"mkt-program-one\">"));
        let cpy = page(&site, "a/cpy.html");
        assert!(!main_of(&cpy).contains("mkt-peer-alpha"));
    }

    #[test]
    fn unbuilt_tables_show_unavailable_not_rows() {
        let site = built(mkt_tree());
        let later = "is not available until a later release.";
        for (path, id, thing) in [
            ("a/pgm.html", "overview", "The programs table"),
            ("a/cpt.html", "compare", "The comparison matrix"),
            ("a/cpt.html", "moves", "The moves list"),
            ("a/cpt.html", "watch-process", "The watch process"),
            ("a/cpy.html", "background", "The company background"),
            ("a/cpy.html", "organization", "The organization chart"),
            ("a/cpy.html", "systems", "The systems list"),
        ] {
            let html = page(&site, path);
            let main = main_of(&html);
            // The landing line sits in the disclosure, under its old id, and holds no table.
            let at = main
                .find(&format!("<p class=\"ws-sub\" id=\"{id}\">"))
                .unwrap_or_else(|| panic!("{path}#{id} must be listed in the disclosure"));
            let line = &main[at..];
            let line = &line[..line.find("</section>").unwrap()];
            assert!(
                line.contains(&format!("<p class=\"ws-line\">{thing} {later}</p>")),
                "{path}#{id}: {line}"
            );
            assert!(!main.contains(&format!("<section class=\"ws-section\" id=\"{id}\">")));
            assert!(
                !main.contains("<section class=\"unavailable\""),
                "{path}#{id}"
            );
            assert!(
                !line.contains("<table"),
                "{path}#{id} holds no made-up table"
            );
            assert!(!line.contains("<tr"), "{path}#{id} holds no made-up row");
        }
        for path in ["a/prc.html", "a/pgm.html", "a/cpy.html", "a/cpt.html"] {
            let html = page(&site, path);
            let main = main_of(&html);
            for id in ["processes", "procedures", "module", "history"] {
                if section_ids(main).contains(&id) {
                    let sec = section(main, id);
                    assert!(!sec.contains("<table"), "{path}#{id}");
                    assert!(sec.contains("Nothing recorded yet."), "{path}#{id}: {sec}");
                } else if let Some(at) = main.find(&format!("<p class=\"ws-sub\" id=\"{id}\">")) {
                    // The Company profile has no Module section; the others fold it.
                    assert!(main[at..].contains("is not available until"), "{path}#{id}");
                }
            }
            for unbuilt in [
                "programs-table",
                "compare-matrix",
                "cap-ladder",
                "Budget line",
                "Budget stage",
                "Linked work",
            ] {
                assert!(!main.contains(unbuilt), "{path} must not draw {unbuilt}");
            }
        }
    }

    #[test]
    fn sidebar_view_entry_links_and_is_current() {
        let site = built(tree_with(str::to_owned));
        let prc = page(&site, "a/prc.html");
        assert!(
            prc.contains("<a href=\"../a/prc.html\" data-item=\"view-prc\" aria-current=\"page\">"),
            "the view entry links to its page and is current on it"
        );
        assert!(prc.contains("data-section=\"areas\" data-item=\"view-prc\""));
        assert!(!prc.contains("data-item=\"area-prc\""));
        assert_eq!(
            prc.matches("aria-current=\"page\">").count(),
            2,
            "entry and crumb"
        );
        assert!(prc.contains("<div class=\"nav-sub\">Views over EST</div>"));
        assert!(!prc.contains("Pricing is not available until"));

        let est = page(&site, "a/est.html");
        assert!(
            est.contains("<a href=\"../a/prc.html\" data-item=\"view-prc\">"),
            "the view entry links from an area page and is not current there"
        );
        assert!(est.contains("data-item=\"area-est\" aria-current=\"page\""));
        assert!(est.contains("data-section=\"areas\" data-item=\"area-est\""));

        let index = page(&site, "index.html");
        assert!(index.contains("<a href=\"a/prc.html\" data-item=\"view-prc\">"));
    }

    #[test]
    fn view_page_has_the_csp_meta_and_no_style_attribute() {
        let site = built(mkt_tree());
        for path in ["a/prc.html", "a/cpy.html", "a/cpt.html", "a/pgm.html"] {
            let html = page(&site, path);
            assert!(
                html.contains(&format!(
                    "http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\""
                )),
                "{path} must carry the static CSP"
            );
            assert!(!html.contains("style=\""), "{path} must have no style attr");
            assert!(
                !html.contains("<script>"),
                "{path} must have no inline script"
            );
        }
    }
    /// Unbuilt workspace sections fold into one closed "Not built yet" disclosure after the last
    /// built section, on the area pages and the view pages alike.
    mod unbuilt_fold {
        use super::{built, main_of, mkt_tree, page, section, section_ids};

        /// An unbuilt section: its id, tab label and landing line.
        type Unbuilt = (&'static str, &'static str, &'static str);

        const PROCESSES: Unbuilt = (
            "processes",
            "Processes",
            "The process list is not available until Release 3.",
        );
        const PROCEDURES: Unbuilt = (
            "procedures",
            "Procedures",
            "The procedure list is not available until Release 3.",
        );
        const TEMPLATES: Unbuilt = (
            "templates",
            "Templates",
            "The template list is not available until E1.",
        );
        const MODULE: Unbuilt = (
            "module",
            "Module",
            "The module card is not available until Release 3.",
        );
        const OVERVIEW_TABLE: Unbuilt = (
            "overview",
            "Overview",
            "The overview table is not available until a later release.",
        );
        const PROGRAMS_TABLE: Unbuilt = (
            "overview",
            "Overview",
            "The programs table is not available until a later release.",
        );
        const BACKGROUND: Unbuilt = (
            "background",
            "Background",
            "The company background is not available until a later release.",
        );
        const ORGANIZATION: Unbuilt = (
            "organization",
            "Organization",
            "The organization chart is not available until a later release.",
        );
        const SYSTEMS: Unbuilt = (
            "systems",
            "Systems",
            "The systems list is not available until a later release.",
        );
        const COMPARE: Unbuilt = (
            "compare",
            "Compare",
            "The comparison matrix is not available until a later release.",
        );
        const MOVES: Unbuilt = (
            "moves",
            "Moves",
            "The moves list is not available until a later release.",
        );
        const WATCH: Unbuilt = (
            "watch-process",
            "Watch process",
            "The watch process is not available until a later release.",
        );

        /// Per workspace page: the built section ids and the unbuilt sections, each in the table
        /// order of HANDOFF section 2. The fixture's est area and Pricing view hold a template
        /// page, so their Templates section is built; no other page here does.
        fn workspaces() -> Vec<(&'static str, Vec<&'static str>, Vec<Unbuilt>)> {
            vec![
                (
                    "a/est.html",
                    vec![
                        "overview",
                        "templates",
                        "library-pages",
                        "history",
                        "questions",
                    ],
                    vec![PROCESSES, PROCEDURES, MODULE],
                ),
                (
                    "a/mkt.html",
                    vec!["overview", "library-pages", "history", "questions"],
                    vec![PROCESSES, PROCEDURES, TEMPLATES, MODULE],
                ),
                (
                    "a/prc.html",
                    vec!["templates", "library-pages", "history", "questions"],
                    vec![OVERVIEW_TABLE, PROCESSES, PROCEDURES, MODULE],
                ),
                (
                    "a/pgm.html",
                    vec!["library-pages", "history", "questions"],
                    vec![PROGRAMS_TABLE, PROCESSES, PROCEDURES, TEMPLATES, MODULE],
                ),
                (
                    "a/cpy.html",
                    vec!["history", "questions"],
                    vec![
                        BACKGROUND,
                        ORGANIZATION,
                        PROCESSES,
                        PROCEDURES,
                        TEMPLATES,
                        SYSTEMS,
                    ],
                ),
                (
                    "a/cpt.html",
                    vec!["profiles", "library-pages", "sources", "questions"],
                    vec![COMPARE, MOVES, WATCH, TEMPLATES],
                ),
            ]
        }

        const DISCLOSURE: &str = "<section class=\"ws-section ws-unbuilt\" id=\"not-built\" aria-label=\"Not built yet\">";

        /// The jump list, "On this page".
        fn jump_list(main: &str) -> &str {
            let start = main.find("<nav class=\"tabs\"").unwrap();
            &main[start..main[start..].find("</nav>").unwrap() + start]
        }

        /// The "Not built yet" section of `main`, one tag per line trimmed and joined.
        fn disclosure(main: &str) -> String {
            let start = main
                .find(DISCLOSURE)
                .unwrap_or_else(|| panic!("the page must carry the Not built yet section"));
            let rest = &main[start..];
            rest[..rest.find("</section>").unwrap() + "</section>".len()]
                .lines()
                .map(str::trim)
                .collect()
        }

        #[test]
        fn unbuilt_sections_leave_the_sequence_and_the_jump_list() {
            let site = built(mkt_tree());
            for (path, built_ids, unbuilt) in workspaces() {
                let html = page(&site, path);
                let main = main_of(&html);
                assert_eq!(section_ids(main), built_ids, "{path}: the built sections");
                let jump = jump_list(main);
                let mut last = 0;
                for id in &built_ids {
                    let at = jump
                        .find(&format!("<a href=\"#{id}\">"))
                        .unwrap_or_else(|| panic!("{path}: the jump list lacks {id}"));
                    assert!(at > last, "{path}: jump link {id} is out of order");
                    last = at;
                }
                for (id, _, _) in &unbuilt {
                    assert!(
                        !jump.contains(&format!("href=\"#{id}\"")),
                        "{path}: the jump list links the unbuilt section {id}: {jump}"
                    );
                }
                let count = unbuilt.len();
                let link = format!(
                    "<a href=\"#not-built\">Not built yet <span class=\"count\">{count}</span></a>"
                );
                assert!(
                    jump.find(&link).is_some_and(|at| at > last),
                    "{path}: the jump list must end with {link}: {jump}"
                );
                assert_eq!(
                    jump.matches("<a href=\"#").count(),
                    built_ids.len() + 1,
                    "{path}: {jump}"
                );
                assert!(
                    jump.trim_end()
                        .trim_end_matches("</nav>")
                        .trim_end()
                        .ends_with(&link),
                    "{path}: the link is the last entry: {jump}"
                );
            }
        }

        #[test]
        fn the_disclosure_follows_the_last_built_section_closed_with_each_landing_line() {
            let site = built(mkt_tree());
            for (path, built_ids, unbuilt) in workspaces() {
                let html = page(&site, path);
                let main = main_of(&html);
                let last_built = main
                    .find(&format!(
                        "<section class=\"ws-section\" id=\"{}\">",
                        built_ids.last().unwrap()
                    ))
                    .unwrap();
                let at = main.find(DISCLOSURE).unwrap_or_else(|| {
                    panic!("{path}: the page must carry the Not built yet section")
                });
                assert!(
                    at > last_built,
                    "{path}: the disclosure comes after the last built section"
                );
                assert_eq!(main.matches("id=\"not-built\"").count(), 1, "{path}");
                let body: String = unbuilt
                    .iter()
                    .map(|(id, label, line)| {
                        format!(
                            "<p class=\"ws-sub\" id=\"{id}\">{label}</p><p class=\"ws-line\">{line}</p>"
                        )
                    })
                    .collect();
                let expected = format!(
                    "{DISCLOSURE}<details class=\"fold\"><summary><b>Not built yet</b> <span class=\"meta\">· {}</span></summary><div class=\"fold-b\">{body}</div></details></section>",
                    unbuilt.len()
                );
                assert_eq!(disclosure(main), expected, "{path}");
                assert!(
                    !main.contains("<details class=\"fold\" open"),
                    "{path}: closed by default"
                );
            }
        }

        #[test]
        fn a_landing_line_is_in_the_disclosure_and_not_in_the_section_sequence() {
            let site = built(mkt_tree());
            for (path, built_ids, unbuilt) in workspaces() {
                let html = page(&site, path);
                let main = main_of(&html);
                let fold = disclosure(main);
                for (id, _, line) in &unbuilt {
                    assert_eq!(fold.matches(line).count(), 1, "{path}: {line}");
                    assert_eq!(
                        main.matches(&format!("id=\"{id}\"")).count(),
                        1,
                        "{path}: the old id {id} resolves to one element, inside the disclosure"
                    );
                    assert!(fold.contains(&format!("id=\"{id}\"")), "{path}: {id}");
                }
                for id in &built_ids {
                    // The Overview of an area holds its own panels, with their own notes.
                    if *id != "overview" {
                        assert!(
                            !section(main, id).contains("is not available until"),
                            "{path}: section {id} holds an unavailable note"
                        );
                    }
                }
            }
        }

        #[test]
        fn a_built_empty_section_stays_in_place() {
            let site = built(mkt_tree());
            for path in ["a/est.html", "a/mkt.html", "a/cpy.html", "a/prc.html"] {
                let html = page(&site, path);
                let history = section(main_of(&html), "history");
                assert!(
                    history.contains("Nothing recorded yet."),
                    "{path}: {history}"
                );
            }
        }

        #[test]
        fn a_template_page_lands_the_templates_section_in_its_place() {
            let site = built(mkt_tree());
            let est = page(&site, "a/est.html");
            let main = main_of(&est);
            assert!(section(main, "templates").contains("est-baseline-template.html"));
            assert!(!disclosure(main).contains("id=\"templates\""));
            let mkt = page(&site, "a/mkt.html");
            let main = main_of(&mkt);
            assert!(!section_ids(main).contains(&"templates"));
            assert!(disclosure(main).contains("id=\"templates\""));
        }
    }
}
