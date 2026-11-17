//! Integration tests for `fieldbook_render::map` (`app-map`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod map {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::about::NETWORK_POSTURE_SHORT;
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::map;
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::site::{self, PlannedPaths, Site};

    const APP_CSS: &str = include_str!("../assets/app.css");
    const CARDS_README: &str = include_str!("../../../docs/cards/README.md");

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

    fn fixture_tree() -> Tree {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
        let mut tree = Tree::new();
        insert_all(&mut tree, &root.join("corpus"), "corpus");
        tree
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

    fn built(tree: Tree, profile: BuildProfile) -> Site {
        let (_model, view) = load_view(tree);
        let info = BuildInfo::new(None, None, profile, None);
        site::render(&view, &info).unwrap()
    }

    fn map_html(site: &Site) -> String {
        String::from_utf8(
            site.files()
                .get(map::OUTPUT_PATH)
                .unwrap_or_else(|| panic!("{} must be built", map::OUTPUT_PATH))
                .clone(),
        )
        .unwrap()
    }

    fn main_of(html: &str) -> &str {
        let start = html.find("<main").unwrap();
        let end = html[start..].find("</main>").unwrap();
        &html[start..start + end]
    }

    fn band_sections(main: &str) -> Vec<&str> {
        let mut rest = main;
        let mut out = Vec::new();
        while let Some(pos) = rest.find("<section class=\"layer") {
            let from = &rest[pos..];
            let end = from.find("</section>").unwrap() + "</section>".len();
            // Include any nested `<section class="unavailable"` inside the Modules band.
            let slice = if from[1..end].contains("<section") {
                let second = from[end..].find("</section>").unwrap() + end + "</section>".len();
                &from[..second]
            } else {
                &from[..end]
            };
            out.push(slice);
            rest = &from[slice.len()..];
        }
        out
    }

    fn extract_anchor_hrefs(fragment: &str) -> Vec<String> {
        let mut hrefs = Vec::new();
        let mut rest = fragment;
        while let Some(a_pos) = rest.find("<a ") {
            let after_a = &rest[a_pos + 3..];
            let close = after_a.find('>').unwrap();
            let tag = &after_a[..close];
            if let Some(h_pos) = tag.find("href=\"") {
                let after_h = &tag[h_pos + 6..];
                let end_q = after_h.find('"').unwrap();
                hrefs.push(after_h[..end_q].to_owned());
            }
            rest = &after_a[close + 1..];
        }
        hrefs
    }

    fn short_form_on_the_page() -> String {
        let mut out = String::new();
        for (i, part) in NETWORK_POSTURE_SHORT.split('`').enumerate() {
            if i % 2 == 1 {
                out.push_str("<code>");
                out.push_str(part);
                out.push_str("</code>");
            } else {
                out.push_str(part);
            }
        }
        out
    }

    #[test]
    fn map_page_has_five_bands_in_order() {
        let site = built(fixture_tree(), BuildProfile::Silent);
        let html = map_html(&site);
        let main = main_of(&html);

        assert!(html.contains("<title>App map</title>"));
        assert!(main.contains("<h1>How Fieldbook fits together</h1>"));
        assert!(main.contains(
            "The library is the foundation, not the point. The point is the decision: \
             every figure in a package traces to its assumption, source, owner and version."
        ));

        let bands = band_sections(main);
        assert_eq!(bands.len(), 5, "expected 5 section.layer bands");
        assert!(bands[0].contains("<h2>Outputs<small>"));
        assert!(
            bands[1].starts_with("<section class=\"layer core\">")
                && bands[1].contains("<h2>Work core<small>")
        );
        assert!(bands[2].contains("<h2>Workspaces<small>"));
        assert!(bands[3].contains("<h2>Modules<small>"));
        assert!(
            bands[4].starts_with("<section class=\"layer found\">")
                && bands[4].contains("<h2>Library<small>")
        );

        let pos_bands = main.find("<div class=\"layers\">").unwrap();
        let pos_asst = main.find("<aside class=\"asst-col\"").unwrap();
        let pos_releases = main.find(">Releases</h2>").unwrap();
        let pos_labels = main.find(">Labels and stores</h2>").unwrap();
        assert!(pos_bands < pos_asst, "layers precede Assistant column");
        assert!(pos_asst < pos_releases, "Assistant precedes Releases panel");
        assert!(
            pos_releases < pos_labels,
            "Releases precedes Labels and stores panel"
        );
    }

    #[test]
    fn bands_have_headings_and_pills() {
        let site = built(fixture_tree(), BuildProfile::Silent);
        let html = map_html(&site);
        let main = main_of(&html);
        let bands = band_sections(main);

        let expected_headings = [
            ("Outputs", "What you hand on"),
            (
                "Work core",
                "Bids, estimates, decisions and projects on one assumption register, inside the 90-day plan",
            ),
            (
                "Workspaces",
                "Processes, procedures, templates, module and history per area",
            ),
            ("Modules", "FM0 to FM14, each gated on a named question"),
            (
                "Library",
                "Foundation layer: cited rules, market evidence, definitions",
            ),
        ];
        for (idx, (heading, subtitle)) in expected_headings.iter().enumerate() {
            let expected = format!("<h2>{heading}<small>{subtitle}</small></h2>");
            assert!(
                bands[idx].contains(&expected),
                "band {idx} missing `{expected}`, got:\n{}",
                bands[idx]
            );
        }

        assert!(bands[0].contains(
            "Outputs that use company data are made from R2, in company storage, and leave it \
             only where policy allows. Planned pricing modules use the same evidence and approval recipes."
        ));

        // Outputs, Work core, Workspaces, and Library render pills inside `ul.links`.
        for idx in [0, 1, 2, 4] {
            assert!(
                bands[idx].contains("<ul class=\"links\">")
                    && bands[idx].contains("class=\"pill\""),
                "band {idx} must render link pills in ul.links:\n{}",
                bands[idx]
            );
        }

        // Workspaces has CMP and EST from corpus-mini, and omits PRC (not one of CPY/CPT/PGM).
        assert!(
            bands[2].contains("<span class=\"code\">CMP</span>Compliance")
                && bands[2].contains("href=\"a/cmp.html\"")
        );
        assert!(
            bands[2].contains("<span class=\"code\">EST</span>Estimating")
                && bands[2].contains("href=\"a/est.html\"")
        );
        assert!(!bands[2].contains(">PRC<"));

        // Without planned view paths, CPY, CPT and PGM stay out of Workspaces.
        let mut tree = fixture_tree();
        let toml_text = std::str::from_utf8(tree.get_bytes("corpus/corpus.toml").unwrap())
            .unwrap()
            .replace(
                "[[view]]",
                "[[area]]\ncode = \"mkt\"\nslug = \"markets\"\nname = \"Markets\"\nsummary = \"Synthetic markets\"\nsubareas = [\"company\", \"peers\", \"programs\"]\n\n[[view]]\ncode = \"CPY\"\nname = \"Company profile\"\nfilter = { area = \"mkt\", subarea = \"company\" }\n\n[[view]]\ncode = \"CPT\"\nname = \"Competitors\"\nfilter = { area = \"mkt\", subarea = \"peers\" }\n\n[[view]]\ncode = \"PGM\"\nname = \"Programs\"\nfilter = { area = \"mkt\", subarea = \"programs\" }\n\n[[view]]",
            );
        tree.insert_bytes("corpus/corpus.toml", toml_text.into_bytes());
        let (_m, view_with_mkt) = load_view(tree);
        let ctx = site::render_context(&view_with_mkt, &BuildInfo::default()).unwrap();
        let unbuilt_views = PlannedPaths::of(&["a/cmp.html", "a/est.html", "a/mkt.html"]);
        let main_unbuilt_views =
            map::render_main(&view_with_mkt, &ctx, BuildProfile::Silent, &unbuilt_views);
        let ws_unbuilt = band_sections(&main_unbuilt_views)[2];
        assert!(
            !ws_unbuilt.contains(">CPY<")
                && !ws_unbuilt.contains(">CPT<")
                && !ws_unbuilt.contains(">PGM<"),
            "CPY, CPT and PGM appear only if their page paths are planned"
        );

        let planned_with_views = site::planned_paths(&view_with_mkt);
        let main_built_views = map::render_main(
            &view_with_mkt,
            &ctx,
            BuildProfile::Silent,
            &planned_with_views,
        );
        let ws_built = band_sections(&main_built_views)[2];
        for (code, href) in [
            ("CPY", "a/cpy.html"),
            ("CPT", "a/cpt.html"),
            ("PGM", "a/pgm.html"),
        ] {
            assert!(
                ws_built.contains(&format!("<span class=\"code\">{code}</span>"))
                    && ws_built.contains(&format!("href=\"{href}\"")),
                "planned view {code} must appear as a pill in Workspaces:\n{ws_built}"
            );
        }
    }

    #[test]
    fn assistant_column_names_the_adapter_states() {
        let site = built(fixture_tree(), BuildProfile::Ai);
        let html = map_html(&site);
        let main = main_of(&html);

        let asst_start = main.find("<aside class=\"asst-col\"").unwrap();
        let asst_end = main[asst_start..].find("</aside>").unwrap() + asst_start + "</aside>".len();
        let asst = &main[asst_start..asst_end];

        assert!(asst.contains("aria-label=\"Assistant\""));
        assert!(asst.contains(">Assistant</h2>"));
        assert!(asst.contains(
            "A chat in every layer. Fieldbook picks the context from the page you are on and \
             shows it. You can pin, remove or add items."
        ));

        let rows = [
            (
                "Handoff",
                "Copy the context pack, paste the answer back, and its citations are checked.",
            ),
            ("Off", "No model connected here"),
            ("Connected", "Where a model adapter exists."),
        ];
        let mut last_pos = 0;
        for (label, line) in rows {
            let needle = format!("<dt>{label}</dt><dd>{line}</dd>");
            let pos = asst[last_pos..]
                .find(&needle)
                .unwrap_or_else(|| panic!("missing adapter row `{needle}` in:\n{asst}"));
            last_pos += pos + needle.len();
        }

        assert!(
            !asst.contains("<button"),
            "Assistant column holds no button"
        );
        assert!(!asst.contains("<a "), "Assistant column holds no link");
        let lower = asst.to_ascii_lowercase();
        for forbidden in [
            "text only",
            "built",
            "http://",
            "https://",
            "api_key",
            "localhost",
        ] {
            assert!(
                !lower.contains(forbidden),
                "Assistant column must not contain `{forbidden}`:\n{asst}"
            );
        }
    }

    #[test]
    fn silent_profile_quotes_the_short_posture_sentence() {
        let readme_short = CARDS_README
            .lines()
            .find_map(|line| line.strip_prefix("> **Short.** "))
            .unwrap()
            .trim_end();
        assert_eq!(NETWORK_POSTURE_SHORT, readme_short);

        let silent_site = built(fixture_tree(), BuildProfile::Silent);
        let silent_main = main_of(&map_html(&silent_site)).to_owned();
        let expected_html = short_form_on_the_page();
        assert!(
            silent_main.contains(&expected_html),
            "Silent build must quote NETWORK_POSTURE_SHORT verbatim:\n{silent_main}"
        );
        assert!(
            !silent_main.contains('`'),
            "code spans in NETWORK_POSTURE_SHORT must render as <code>, not raw backticks"
        );

        let ai_site = built(fixture_tree(), BuildProfile::Ai);
        let ai_main = main_of(&map_html(&ai_site)).to_owned();
        assert!(
            !ai_main.contains(&expected_html),
            "Ai build must omit the Silent network-posture sentence"
        );
    }

    #[test]
    fn pills_link_only_to_planned_paths() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let full_planned = site::planned_paths(&view);
        let full_main = map::render_main(&view, &ctx, BuildProfile::Silent, &full_planned);

        let hrefs = extract_anchor_hrefs(&full_main);
        assert!(!hrefs.is_empty(), "map must emit planned pill links");
        for href in &hrefs {
            let target = href.split('#').next().unwrap_or(href);
            assert!(
                full_planned.contains(target),
                "map link href `{href}` points to unplanned path `{target}`"
            );
        }

        let partial = PlannedPaths::of(&["index.html", "a/cmp.html", "claims.html"]);
        let partial_main = map::render_main(&view, &ctx, BuildProfile::Silent, &partial);
        for href in extract_anchor_hrefs(&partial_main) {
            let target = href.split('#').next().unwrap_or(&href);
            assert!(
                partial.contains(target),
                "narrowed map link href `{href}` points to unplanned path `{target}`"
            );
        }
    }

    #[test]
    fn an_unplanned_pill_is_in_the_not_built_fold() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let partial = PlannedPaths::of(&["index.html", "a/cmp.html", "claims.html"]);
        let main = map::render_main(&view, &ctx, BuildProfile::Silent, &partial);
        let bands = band_sections(&main);

        // Outputs has `day-one.html` unplanned -> closed Not built yet fold with landing line.
        assert!(!bands[0].contains("href=\"day-one.html\""));
        assert!(bands[0].contains("<details class=\"fold nav-more\">"));
        assert!(bands[0].contains("<summary><b>Not built yet</b></summary>"));
        assert!(bands[0].contains("The day-one binder is not available until Day 1."));

        // Workspaces has CMP planned (`a/cmp.html`) and EST unplanned (`a/est.html`).
        assert!(bands[2].contains("href=\"a/cmp.html\""));
        assert!(!bands[2].contains("href=\"a/est.html\""));
        let fold_pos = bands[2].find("<details class=\"fold nav-more\">").unwrap();
        let fold = &bands[2][fold_pos..];
        let open_tag = &fold[..fold.find('>').unwrap() + 1];
        assert!(
            !open_tag.contains(" open") && !open_tag.contains("open="),
            "Not built yet fold must be closed by default: `{open_tag}`"
        );
        assert!(!fold.contains("<a "), "Not built yet fold holds no link");
        assert!(
            fold.contains("<code>EST</code> Estimating is not available until Day 1."),
            "unplanned area pill must appear with its landing line in fold:\n{fold}"
        );
    }

    #[test]
    fn modules_band_is_unavailable() {
        let site = built(fixture_tree(), BuildProfile::Silent);
        let html = map_html(&site);
        let main = main_of(&html);
        let modules = band_sections(main)[3];

        assert!(modules.contains(
            "<h2>Modules<small>FM0 to FM14, each gated on a named question</small></h2>"
        ));
        assert!(modules.contains("class=\"unavailable\""));
        assert!(modules.contains("<b>Modules are not available until R3.</b>"));
        assert!(!modules.contains("class=\"pill\""));
        assert!(!modules.contains("<a "));
    }

    #[test]
    fn map_has_the_static_csp_and_no_inline_style() {
        let site = built(fixture_tree(), BuildProfile::Silent);
        let html = map_html(&site);
        let main = main_of(&html);

        let expected_csp =
            format!("<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">");
        assert_eq!(
            html.matches(&expected_csp).count(),
            1,
            "map.html must carry the exact static CSP once"
        );
        assert!(
            !html.contains("style=\""),
            "map.html must contain no inline style attribute"
        );
        assert!(
            !main.contains("<script"),
            "map.html main region must contain no <script>"
        );
    }

    #[test]
    fn map_has_no_external_href() {
        let site = built(fixture_tree(), BuildProfile::Silent);
        let html = map_html(&site);

        for forbidden in ["href=\"http", "href=\"//", "src=\"http", "src=\"//"] {
            assert!(
                !html.contains(forbidden),
                "map.html must not contain `{forbidden}`"
            );
        }
    }

    #[test]
    fn map_stylesheet_stacks_at_900px() {
        for selector in [".appmap", ".layers", ".layer", ".pill", ".asst-col"] {
            assert!(
                APP_CSS.contains(selector),
                "assets/app.css must define `{selector}`"
            );
        }
        let compact: String = APP_CSS.chars().filter(|c| !c.is_whitespace()).collect();
        let marker = "@container(max-width:900px){";
        let mut bodies = Vec::new();
        for (start, _) in compact.match_indices(marker) {
            let after = &compact[start + marker.len()..];
            let mut depth = 1usize;
            let mut end = 0usize;
            for (idx, ch) in after.char_indices() {
                match ch {
                    '{' => depth += 1,
                    '}' => {
                        depth -= 1;
                        if depth == 0 {
                            end = idx;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            bodies.push(&after[..end]);
        }
        assert!(
            !bodies.is_empty(),
            "assets/app.css must contain `@container (max-width: 900px)`"
        );
        let joined = bodies.join("");
        assert!(
            joined.contains(".appmap{grid-template-columns:minmax(0,1fr)")
                || joined.contains(".appmap{grid-template-columns:1fr"),
            "900px container query must stack .appmap to one column, got: `{joined}`"
        );
        assert!(
            joined.contains(".layer{grid-template-columns:minmax(0,1fr)")
                || joined.contains(".layer{grid-template-columns:1fr"),
            "900px container query must stack .layer to one column, got: `{joined}`"
        );
    }

    #[test]
    fn releases_panel_has_four_rows_in_order() {
        let site = built(fixture_tree(), BuildProfile::Silent);
        let html = map_html(&site);
        let main = main_of(&html);

        let start = main.find("aria-labelledby=\"map-releases\"").unwrap();
        let panel = &main[start..start + main[start..].find("</section>").unwrap()];

        let rows = [
            ("R1", "Day-one Corpus: edition-001 and tool v0.1.0."),
            (
                "E1",
                "The first edition, moved to the laptop over the approved channel.",
            ),
            (
                "R2",
                "Signed Toolkit: v1.0.0. A signed command-line tool, installed by IT.",
            ),
            ("R3", "Modules FM0 to FM14, each gated on a named question."),
        ];
        assert_eq!(
            panel.matches("<dt>").count(),
            4,
            "Releases must have 4 rows"
        );
        let mut last = 0;
        for (name, line) in rows {
            let needle = format!("<dt>{name}</dt><dd>{line}</dd>");
            let pos = panel[last..]
                .find(&needle)
                .unwrap_or_else(|| panic!("missing Releases row `{needle}` in:\n{panel}"));
            last += pos + needle.len();
        }
        assert!(
            panel.contains(
                "This build&#39;s edition and tool version are on <a href=\"about.html\">About</a>."
            ) || panel.contains(
                "This build's edition and tool version are on <a href=\"about.html\">About</a>."
            ),
            "Releases note must link About to about.html:\n{panel}"
        );
    }

    #[test]
    fn labels_and_stores_panel_has_four_labels_and_the_store_table() {
        let site = built(fixture_tree(), BuildProfile::Silent);
        let html = map_html(&site);
        let main = main_of(&html);

        let start = main.find("aria-labelledby=\"map-labels-stores\"").unwrap();
        let panel = &main[start..start + main[start..].find("</section>").unwrap()];

        assert!(panel.contains(
            "Every page, source and record carries labels. A record goes only where its labels allow."
        ));

        let labels = [
            ("public", "A public source or a synthetic example."),
            (
                "unlabelled",
                "No label could be found. It stays local and goes out only in a company-storage backup.",
            ),
            (
                "marked",
                "A marking was found and no rule maps it. It is held locally and never leaves.",
            ),
            (
                "owner-supplied",
                "Text you typed or pasted for one request. Only request logs hold it.",
            ),
        ];
        assert_eq!(panel.matches("<dt>").count(), 4, "4 built-in labels");
        let mut last = 0;
        for (label, line) in labels {
            let needle =
                format!("<dt><span class=\"chip mono\">{label}</span></dt><dd>{line}</dd>");
            let pos = panel[last..]
                .find(&needle)
                .unwrap_or_else(|| panic!("missing label row `{needle}` in:\n{panel}"));
            last += pos + needle.len();
        }
        for tone in ["chip ok", "chip warn", "chip bad", "chip info"] {
            assert!(
                !panel.contains(tone),
                "a label is not a status, so no status colour (`{tone}`)"
            );
        }

        assert!(panel.contains("<caption>Stores</caption>"));
        assert!(panel.contains("<th scope=\"col\">Store</th><th scope=\"col\">Holds</th>"));
        let store_rows = [
            "<tr><td><code>corpus</code></td><td><code>public</code></td></tr>",
            "<tr><td><code>overlay</code></td><td><code>public</code>, <code>unlabelled</code>, <code>marked</code></td></tr>",
            "<tr><td><code>working</code></td><td><code>public</code>, <code>unlabelled</code>, <code>marked</code></td></tr>",
            "<tr><td><code>state</code></td><td><code>public</code>, <code>unlabelled</code>, <code>owner-supplied</code>, <code>marked</code></td></tr>",
        ];
        let mut store_last = 0;
        for row in store_rows {
            let pos = panel[store_last..]
                .find(row)
                .unwrap_or_else(|| panic!("missing store row `{row}` in:\n{panel}"));
            store_last += pos + row.len();
        }

        assert!(
            panel.contains(
                "<code>marked</code> is held only on a machine that has a company store. \
             A company&#39;s own labels are not listed here."
            ) || panel.contains(
                "<code>marked</code> is held only on a machine that has a company store. \
             A company's own labels are not listed here."
            )
        );
    }
}
