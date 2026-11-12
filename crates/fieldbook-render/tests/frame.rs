//! Integration tests for `fieldbook_render::frame` and `fieldbook_render::nav` (`build-frame`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod frame {
    use std::collections::{BTreeMap, BTreeSet};

    use fieldbook_core::audit::{LabelId, LabelSet, Now};
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::schema::Register;
    use fieldbook_render::edition::{EditionCounts, EditionIds, EditionInfo};
    use fieldbook_render::frame::{Crumb, FrameInput, PRODUCT_TITLE, StoreContext, render_page};
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::nav::{self, NavTarget, SectionId};
    use fieldbook_render::site::{self, PlannedPaths};

    const APP_JS: &str = include_str!("../assets/app.js");
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

    fn edition_info(built: &str, number: u32) -> EditionInfo {
        EditionInfo {
            edition: "unreleased".to_owned(),
            number,
            built: built.to_owned(),
            tool_version: "0.1.0".to_owned(),
            min_tool_version: "0.1.0".to_owned(),
            schema_version: 1,
            commit: String::new(),
            fingerprint: String::new(),
            counts: EditionCounts::default(),
            ids: EditionIds::default(),
        }
    }

    fn insert_register_headers(tree: &mut Tree) {
        tree.insert_dir("corpus/registers");
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
    }

    fn base_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating");
        tree.insert_dir("corpus/pages/estimating/pricing");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
        tree.insert_bytes(
            "corpus/changes.md",
            b"# Changes\n\n## edition-001\n\n- Added: initial.\n",
        );
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");
        insert_register_headers(&mut tree);
        tree
    }

    fn sample_page_md(id: &str, title: &str, area: &str, subarea: &str, handling: &str) -> String {
        format!(
            "+++\nid = \"{id}\"\ntitle = \"{title}\"\narea = \"{area}\"\nsubarea = \"{subarea}\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [{handling}]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic purpose.\n\n## The rule\n\nSynthetic rule.\n\n## How it works\n\nSynthetic body.\n\n## Worked example\n\nSynthetic example.\n\n## Never confuse\n\nSynthetic distinction.\n\n## Company calibration\n\nSynthetic calibration.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-03-01: Added synthetic page.\n"
        )
    }

    fn build_public_view_from_tree(tree: Tree) -> (Model, LabelledView) {
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

    fn build_public_view() -> (Model, LabelledView) {
        build_public_view_from_tree(base_tree())
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

    fn build_public_view_with_claims(claim_rows: &[String]) -> (Model, LabelledView) {
        let mut tree = base_tree();
        tree.insert_bytes(
            "corpus/changes.md",
            b"# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n",
        );
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);

        let claims_header = CLAIMS_CSV.lines().next().unwrap();
        let mut claims_content = format!("{claims_header}\n");
        for row in claim_rows {
            claims_content.push_str(row);
            claims_content.push('\n');
        }
        tree.insert_bytes("corpus/registers/claims.csv", claims_content.into_bytes());

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
        assert_eq!(
            view.claims().len(),
            claim_rows.len(),
            "view claims count must match fixture rows"
        );
        (model, view)
    }

    fn parse_js_route_sections(js: &str) -> BTreeMap<String, (String, String)> {
        let marker = "const ROUTE_SECTIONS = {";
        let Some(start) = js.find(marker) else {
            panic!("missing `const ROUTE_SECTIONS = {{` in app.js");
        };
        let after = &js[start + marker.len()..];
        let Some(end) = after.find("};") else {
            panic!("missing closing `}};` for ROUTE_SECTIONS in app.js");
        };
        let body = &after[..end];
        let mut out = BTreeMap::new();
        for raw_line in body.lines() {
            let line = raw_line.trim().trim_end_matches(',');
            if line.is_empty() || line.starts_with("//") {
                continue;
            }
            let Some((key_part, val_part)) = line.split_once(':') else {
                continue;
            };
            let route = key_part
                .trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_owned();
            let mut section = None;
            let mut item = None;
            let inner = val_part
                .trim()
                .trim_start_matches('{')
                .trim_end_matches('}');
            for field in inner.split(',') {
                if let Some((fk, fv)) = field.split_once(':') {
                    let k = fk.trim().trim_matches(|c| c == '\'' || c == '"');
                    let v = fv.trim().trim_matches(|c| c == '\'' || c == '"').to_owned();
                    match k {
                        "section" => section = Some(v),
                        "item" => item = Some(v),
                        _ => {}
                    }
                }
            }
            let sec = section.unwrap_or_else(|| panic!("missing section for route `{route}`"));
            let it = item.unwrap_or_else(|| panic!("missing item for route `{route}`"));
            out.insert(route, (sec, it));
        }
        out
    }

    fn extract_anchor_hrefs(html: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = html;
        while let Some(a_idx) = rest.find("<a ") {
            let after_a = &rest[a_idx + 3..];
            let Some(close_idx) = after_a.find('>') else {
                break;
            };
            let tag = &after_a[..close_idx];
            if let Some(h_idx) = tag.find("href=\"") {
                let after_h = &tag[h_idx + 6..];
                if let Some(end_q) = after_h.find('"') {
                    out.push(after_h[..end_q].to_owned());
                }
            }
            rest = &after_a[close_idx + 1..];
        }
        out
    }

    /// The opening `<a ...>` tag that carries `marker`, if the marker sits inside an anchor.
    fn anchor_tag_of<'a>(html: &'a str, marker: &str) -> Option<&'a str> {
        let at = html.find(marker)?;
        let start = html[..at].rfind('<')?;
        let tag = &html[start..];
        let end = tag.find('>')?;
        let tag = &tag[..=end];
        tag.starts_with("<a ").then_some(tag)
    }

    fn split_disclosure(html: &str) -> (&str, &str, &str) {
        let marker = "<details class=\"fold nav-more\"";
        let start = html.find(marker).unwrap();
        let before = &html[..start];
        let rest = &html[start..];
        let end_marker = "</details>";
        let end = rest.find(end_marker).unwrap();
        let disclosure = &rest[..end + end_marker.len()];
        let after = &rest[end + end_marker.len()..];
        (before, disclosure, after)
    }

    fn disclosure(html: &str) -> &str {
        split_disclosure(html).1
    }

    #[test]
    fn section_map_matches_nav_rs() {
        let js_map = parse_js_route_sections(APP_JS);
        let rust_rows = nav::route_sections();

        let mut rust_map = BTreeMap::new();
        for row in &rust_rows {
            rust_map.insert(
                row.route.to_owned(),
                (row.section.key().to_owned(), row.item.to_owned()),
            );
        }

        for (route, expected) in &rust_map {
            let actual = js_map.get(route).unwrap_or_else(|| {
                panic!("route `{route}` present in nav.rs is missing from ROUTE_SECTIONS in app.js")
            });
            assert_eq!(
                actual, expected,
                "route `{route}` in app.js has {actual:?}, expected {expected:?} from nav.rs"
            );
        }
        for route in js_map.keys() {
            assert!(
                rust_map.contains_key(route),
                "route `{route}` in app.js is missing from nav.rs"
            );
        }
    }

    #[test]
    fn planned_paths_equal_the_site_keys() {
        let (_model, view) = build_public_view();
        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let rendered = site::render(&view, &info).unwrap();
        let planned = site::planned_paths(&view);

        for key in rendered.files().keys() {
            assert!(
                planned.contains(key),
                "site key `{key}` is missing from site::planned_paths"
            );
        }
        for path in planned.iter() {
            assert!(
                rendered.files().contains_key(path),
                "planned path `{path}` is not a key in rendered Site"
            );
        }
    }

    #[test]
    fn links_only_to_built_paths() {
        let (_model, view) = build_public_view();
        let planned = site::planned_paths(&view);
        let navigation = nav::build(&view, &planned);
        let html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[
                Crumb::linked("Today", "index.html", &planned),
                Crumb::linked("Unbuilt map", fieldbook_core::strings::paths()[5], &planned),
                Crumb::current("Current"),
            ],
            main_html: "",
            nav: &navigation,
        });

        for href in extract_anchor_hrefs(&html) {
            if href.starts_with('#') {
                continue;
            }
            let target = href.split('#').next().unwrap_or(&href);
            assert!(
                planned.contains(target),
                "rendered link href `{href}` points to unbuilt path `{target}` not in PlannedPaths"
            );
        }
    }

    #[test]
    fn unbuilt_entries_follow_revision_22() {
        let (_model, view) = build_public_view();
        let planned = site::planned_paths(&view);
        let navigation = nav::build(&view, &planned);
        let html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &navigation,
        });

        assert!(
            !html.contains("class=\"not-available\""),
            "page must not contain class=\"not-available\""
        );

        for chunk in html.split("title=\"").skip(1) {
            let title_val = chunk.split('"').next().unwrap();
            assert!(
                !title_val.contains("is not available until"),
                "title attribute must not carry a landing line: `{title_val}`"
            );
        }

        let (before, disc, after) = split_disclosure(&html);
        assert!(
            !before.contains("is not available until"),
            "substring 'is not available until' must not appear before the disclosure"
        );
        assert!(
            !after.contains("is not available until"),
            "substring 'is not available until' must not appear after the disclosure"
        );
        assert!(
            disc.contains("is not available until"),
            "substring 'is not available until' must appear inside the disclosure"
        );

        let open_tag = &disc[..disc.find('>').unwrap() + 1];
        assert!(
            !open_tag.contains(" open") && !open_tag.contains("open="),
            "disclosure opening tag must not have open attribute: `{open_tag}`"
        );
        assert!(
            disc.contains("<summary><b>Not built yet</b>"),
            "disclosure must contain `<summary><b>Not built yet</b>`"
        );
        assert!(
            !disc.contains("<a "),
            "disclosure must contain no `<a>` tag"
        );

        let summary_start = disc.find("<summary>").unwrap();
        let summary_end = disc.find("</summary>").unwrap();
        let summary_content = &disc[summary_start..summary_end];
        let badge_prefix = "<span class=\"fold-d\">";
        let badge_start = summary_content.find(badge_prefix).unwrap() + badge_prefix.len();
        let badge_end = summary_content[badge_start..].find("</span>").unwrap();
        let summary_count: usize = summary_content[badge_start..badge_start + badge_end]
            .trim()
            .parse()
            .unwrap();
        let expected_unbuilt = navigation.unbuilt_count();
        let nav_line_count = disc.matches("<p class=\"nav-line\">").count();
        assert_eq!(
            summary_count, expected_unbuilt,
            "summary count must equal navigation.unbuilt_count()"
        );
        assert_eq!(
            summary_count, nav_line_count,
            "summary count must equal number of `<p class=\"nav-line\">` in disclosure"
        );

        for tab in &navigation.tabs {
            if let NavTarget::Unbuilt { landing_line } = &tab.target {
                let amp = landing_line.replace('&', "&amp;");
                let num = landing_line.replace('&', "&#38;");
                assert!(
                    disc.contains(landing_line) || disc.contains(&amp) || disc.contains(&num),
                    "unbuilt tab `{}` landing line `{landing_line}` must appear inside disclosure",
                    tab.label
                );
            }
        }
        for group in &navigation.groups {
            for entry in &group.entries {
                if let NavTarget::Unbuilt { landing_line } = &entry.target {
                    let amp = landing_line.replace('&', "&amp;");
                    let num = landing_line.replace('&', "&#38;");
                    assert!(
                        disc.contains(landing_line) || disc.contains(&amp) || disc.contains(&num),
                        "unbuilt entry `{}` landing line `{landing_line}` must appear inside disclosure",
                        entry.id
                    );
                }
            }
        }

        let map_path = fieldbook_core::strings::paths()[5];
        for href in extract_anchor_hrefs(&html) {
            let clean = href.split('#').next().unwrap_or(&href);
            assert!(
                !clean.ends_with(map_path) && clean != map_path,
                "map.html must not be a link target anywhere, found `{href}`"
            );
        }
        assert!(
            disc.contains("The app map is not available until Day 30."),
            "app map landing line must be inside disclosure"
        );

        let planned_with_map =
            PlannedPaths::of(&["index.html", fieldbook_core::strings::paths()[5]]);
        let nav_with_map = nav::build(&view, &planned_with_map);
        let html_with_map = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &nav_with_map,
        });

        let map_link_exists = extract_anchor_hrefs(&html_with_map)
            .iter()
            .any(|href| href.ends_with(map_path));
        assert!(
            map_link_exists,
            "App map entry must appear as an `<a ` link whose href ends with `{map_path}`"
        );
        assert!(
            !html_with_map.contains("The app map is not available until Day 30."),
            "app map landing line must not appear anywhere in html when map is built"
        );
    }

    #[test]
    fn built_entries_still_link() {
        let (_model, view) = build_public_view();
        let planned = site::planned_paths(&view);
        let navigation = nav::build(&view, &planned);
        let html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &navigation,
        });

        let anchor_hrefs = extract_anchor_hrefs(&html);

        let top_bar = &html[html.find("<nav aria-label=\"Sections\">").unwrap()..];
        for tab in &navigation.tabs {
            if let NavTarget::Built { href } = &tab.target {
                assert!(
                    anchor_hrefs.contains(href),
                    "built tab `{}` href `{href}` must appear in anchor hrefs",
                    tab.label
                );
                let marker = format!("data-sec=\"{}\"", tab.key);
                assert!(
                    anchor_tag_of(top_bar, &marker)
                        .is_some_and(|tag| tag.contains(&format!("href=\"{href}\""))),
                    "built tab `{}` must be an <a> carrying href `{href}`",
                    tab.label
                );
            }
        }

        let side_bar = &html[html.find("<aside class=\"sidebar\"").unwrap()..];
        for group in &navigation.groups {
            for entry in &group.entries {
                if let NavTarget::Built { href } = &entry.target {
                    assert!(
                        anchor_hrefs.contains(href),
                        "built entry `{}` href `{href}` must appear in anchor hrefs",
                        entry.id
                    );
                    let expected_data_item = format!("data-item=\"{}\"", entry.id);
                    assert!(
                        anchor_tag_of(side_bar, &expected_data_item)
                            .is_some_and(|tag| tag.contains(&format!("href=\"{href}\""))),
                        "built entry `{}` must be an <a> carrying href `{href}`",
                        entry.id
                    );
                    assert!(
                        html.contains(&entry.label.replace('&', "&#38;")),
                        "built entry `{}` label `{}` must appear in html",
                        entry.id,
                        entry.label
                    );
                }
            }
        }

        for sec in [SectionId::Today, SectionId::Areas, SectionId::Library] {
            let group = navigation
                .group(sec)
                .unwrap_or_else(|| panic!("group `{sec:?}` must exist"));
            let built_count = group
                .entries
                .iter()
                .filter(|e| matches!(e.target, NavTarget::Built { .. }))
                .count();
            assert!(
                built_count >= 1,
                "tested group `{sec:?}` must have at least one built entry, found {built_count}"
            );
        }
    }

    #[test]
    fn unbuilt_tab_follows_revision_22() {
        let (_model, view) = build_public_view();
        let planned = PlannedPaths::of(&["index.html", "a/cmp.html", "claims.html"]);
        let navigation = nav::build(&view, &planned);
        let html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &navigation,
        });

        let topbar_start = html
            .find("<nav aria-label=\"Sections\">")
            .expect("top bar sections nav must exist");
        let topbar_end = html[topbar_start..]
            .find("</nav>")
            .expect("closing top bar nav must exist");
        let topbar_nav = &html[topbar_start..topbar_start + topbar_end + "</nav>".len()];

        assert!(
            topbar_nav.contains("data-sec=\"today\""),
            "top bar sections nav must contain data-sec=\"today\""
        );
        assert!(
            !topbar_nav.contains("data-sec=\"assistant\""),
            "top bar sections nav must not contain unbuilt data-sec=\"assistant\""
        );
        assert!(
            !topbar_nav.contains("data-sec=\"work\""),
            "top bar sections nav must not contain unbuilt data-sec=\"work\""
        );
        assert!(
            !topbar_nav.contains("data-sec=\"plan\""),
            "top bar sections nav must not contain unbuilt data-sec=\"plan\""
        );

        let switch_start = html
            .find("id=\"fb:secSwitch\"")
            .expect("secSwitch element must exist");
        let switch_end = html[switch_start..]
            .find("</div>")
            .expect("closing secSwitch div must exist");
        let sec_switch = &html[switch_start..switch_start + switch_end + "</div>".len()];

        assert!(
            sec_switch.contains("data-secpick=\"today\""),
            "#fb:secSwitch must contain data-secpick=\"today\""
        );
        assert!(
            !sec_switch.contains("data-secpick=\"assistant\""),
            "#fb:secSwitch must not contain data-secpick=\"assistant\""
        );
        assert!(
            !sec_switch.contains("data-secpick=\"work\""),
            "#fb:secSwitch must not contain data-secpick=\"work\""
        );
        assert!(
            !sec_switch.contains("data-secpick=\"plan\""),
            "#fb:secSwitch must not contain data-secpick=\"plan\""
        );

        let disc = disclosure(&html);
        assert!(
            disc.contains("The assistant is not available until Day 30."),
            "assistant tab landing line must appear inside disclosure"
        );
        assert!(
            disc.contains("The 90-day plan is not available until Day 1."),
            "plan tab landing line must appear inside disclosure"
        );

        assert!(
            !html.contains("<div class=\"nav-group\" data-sec=\"work\""),
            "no nav-group with data-sec=\"work\" must be rendered"
        );
    }

    #[test]
    fn current_tab_and_item_are_marked() {
        let (_model, view) = build_public_view();
        let planned = PlannedPaths::of(&["index.html", "a/cmp.html", "claims.html"]);
        let navigation = nav::build(&view, &planned);

        // 1. Canonical Today state on index.html (Step 8) + Public StoreContext (Step 9).
        assert_eq!(
            nav::resolve_route("index.html", None),
            Some((SectionId::Today, "today".to_owned()))
        );
        // All areas and Browse are aliases to the anchor build-home-and-areas creates; the alias
        // destination keeps the canonical Today frame, in Rust and in the app.js map.
        assert_eq!(
            nav::resolve_route("index.html#areas", None),
            Some((SectionId::Today, "today".to_owned()))
        );
        let alias_rows: Vec<_> = nav::route_sections()
            .into_iter()
            .filter(|row| row.route == "index.html#areas")
            .collect();
        assert_eq!(alias_rows.len(), 1, "one canonical section for the alias");
        assert_eq!(alias_rows[0].section, SectionId::Today);
        assert_eq!(alias_rows[0].item, "today");
        let js_map = parse_js_route_sections(APP_JS);
        assert_eq!(
            js_map.get("index.html#areas"),
            Some(&("today".to_owned(), "today".to_owned())),
            "app.js must map the alias to Today"
        );
        for id in ["areas", "browse"] {
            let target = &navigation.entry(id).unwrap().target;
            assert!(
                matches!(target, NavTarget::Built { href } if href == "index.html#areas"),
                "{id} must be an alias to index.html#areas, got {target:?}"
            );
        }
        // Fragments never enter PlannedPaths: without index.html the alias is suppressed.
        let without_index = PlannedPaths::of(&["a/cmp.html"]);
        let suppressed = nav::build(&view, &without_index);
        assert!(
            matches!(
                suppressed.entry("areas").unwrap().target,
                NavTarget::Unbuilt { .. }
            ),
            "the alias keeps the index.html PlannedPaths check"
        );

        let index_html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &navigation,
        });
        assert!(
            index_html.contains("data-sec=\"today\" aria-current=\"true\""),
            "Today tab must have aria-current=\"true\" on index.html"
        );
        assert!(
            index_html.contains("<div class=\"nav-group\" data-sec=\"today\">"),
            "Today nav-group must be visible without scripts on index.html"
        );
        assert!(
            index_html.contains("<div class=\"nav-group\" data-sec=\"areas\" hidden>"),
            "non-current nav-groups must carry `hidden` on index.html"
        );
        assert!(
            index_html.contains("data-item=\"today\" aria-current=\"page\""),
            "Today sidebar item must have aria-current=\"page\" on index.html"
        );
        assert!(
            index_html.contains("href=\"index.html#areas\""),
            "the aliases must link to the #areas anchor without scripts"
        );
        assert!(
            !index_html.contains("data-item=\"areas\" aria-current"),
            "All areas is an alias: Today stays the current item on index.html"
        );
        assert!(
            !index_html.contains("data-item=\"browse\" aria-current"),
            "Browse is an alias: Today stays the current item on index.html"
        );
        assert!(index_html.contains("Public data only"));
        assert!(index_html.contains("Company data is not connected."));
        assert!(!index_html.contains("aria-label=\"Handling notice\""));

        // 2. Area page (a/cmp.html) + Company StoreContext (Step 9).
        let bid3 = LabelId::parse("bid-0003").unwrap();
        let np7 = LabelId::parse("np-0007").unwrap();
        let company_store = StoreContext::Company {
            effective_labels: LabelSet::from_ids([bid3, np7]),
        };
        let area_html = render_page(&FrameInput {
            output_path: "a/cmp.html",
            title: "Compliance",
            page_area: None,
            edition: &edition_info("2026-03-01", 1),
            store: &company_store,
            crumbs: &[],
            main_html: "",
            nav: &navigation,
        });
        assert!(
            area_html.contains("data-sec=\"areas\" aria-current=\"true\""),
            "Areas tab must have aria-current=\"true\" on a/cmp.html"
        );
        assert!(
            area_html.contains("<div class=\"nav-group\" data-sec=\"areas\">"),
            "Areas nav-group must be visible without scripts on a/cmp.html"
        );
        assert!(
            area_html.contains("<div class=\"nav-group\" data-sec=\"today\" hidden>"),
            "Today nav-group must carry `hidden` on a/cmp.html"
        );
        assert!(
            area_html.contains("data-item=\"area-cmp\" aria-current=\"page\""),
            "area-cmp sidebar item must have aria-current=\"page\" on a/cmp.html"
        );
        assert!(area_html.contains("Company storage"));
        assert!(area_html.contains("Handling: bid-0003, np-0007."));
        assert!(area_html.contains("Local view; no outward permission is implied."));
    }

    #[test]
    fn crumbs_only_with_two_levels() {
        let (_model, view) = build_public_view();
        let planned = PlannedPaths::of(&["index.html", "a/cmp.html"]);
        let navigation = nav::build(&view, &planned);

        let zero_html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &navigation,
        });
        assert!(
            !zero_html.contains("aria-label=\"Breadcrumb\""),
            "0 crumbs must not render CrumbsRow"
        );

        let one_html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[Crumb::current("Today")],
            main_html: "",
            nav: &navigation,
        });
        assert!(
            !one_html.contains("aria-label=\"Breadcrumb\""),
            "1 crumb must not render CrumbsRow"
        );

        let two_html = render_page(&FrameInput {
            output_path: "a/cmp.html",
            title: "Compliance",
            page_area: None,
            edition: &edition_info("", 0),
            store: &StoreContext::Public,
            crumbs: &[
                Crumb::linked("Today", "index.html", &planned),
                Crumb::current("Compliance"),
            ],
            main_html: "",
            nav: &navigation,
        });
        assert!(
            two_html.contains("<nav class=\"crumbs\" id=\"fb:crumbs\" aria-label=\"Breadcrumb\">"),
            "2 crumbs must render CrumbsRow"
        );
        assert!(two_html.contains("<a href=\"../index.html\">Today</a>"));
        assert!(two_html.contains("<span aria-current=\"page\">Compliance</span>"));
    }

    #[test]
    fn relative_paths_at_each_depth() {
        let (_model, view) = build_public_view();
        let planned = PlannedPaths::of(&["index.html", "a/cmp.html"]);
        let navigation = nav::build(&view, &planned);

        for (path, expected_prefix) in [
            ("index.html", ""),
            ("a/cmp.html", "../"),
            ("p/sub/deep.html", "../../"),
        ] {
            let html = render_page(&FrameInput {
                output_path: path,
                title: PRODUCT_TITLE,
                page_area: Some("cmp"),
                edition: &edition_info("", 0),
                store: &StoreContext::Public,
                crumbs: &[
                    Crumb::linked("Today", "index.html", &planned),
                    Crumb::current("Here"),
                ],
                main_html: "",
                nav: &navigation,
            });
            assert!(
                html.contains(&format!("href=\"{expected_prefix}assets/app.css\"")),
                "missing relative app.css link at `{path}`"
            );
            assert!(
                html.contains(&format!("src=\"{expected_prefix}assets/strings.js\"")),
                "missing relative strings.js link at `{path}`"
            );
            assert!(
                html.contains(&format!("src=\"{expected_prefix}assets/app.js\"")),
                "missing relative app.js link at `{path}`"
            );
            assert!(
                html.contains(&format!("href=\"{expected_prefix}index.html\"")),
                "missing relative index.html link at `{path}`"
            );
        }
    }

    #[test]
    fn no_inline_handler_or_style_attribute() {
        let (_model, view) = build_public_view();
        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = site::render(&view, &info).unwrap();

        for (path, bytes) in site.files() {
            if !path.ends_with(".html") {
                continue;
            }
            let text = std::str::from_utf8(bytes).unwrap();
            assert!(
                !text.contains("style=\""),
                "{path} must not contain inline style attribute"
            );
            let lower = text.to_ascii_lowercase();
            for attr in [
                " onclick=",
                " onload=",
                " onchange=",
                " oninput=",
                " onsubmit=",
                " onkeydown=",
                " onkeyup=",
                " onfocus=",
                " onblur=",
            ] {
                assert!(
                    !lower.contains(attr),
                    "{path} must not contain inline handler `{attr}`"
                );
            }
        }
    }

    #[test]
    fn storage_only_inside_try() {
        let mut search_from = 0usize;
        let mut count = 0usize;
        while let Some(rel) = APP_JS[search_from..].find("localStorage") {
            let idx = search_from + rel;
            count += 1;

            // Walk backwards from `idx` to verify `localStorage` is enclosed in an active `try { ... }` block.
            let prefix = &APP_JS[..idx];
            let mut open_braces = Vec::new();
            let tokens: Vec<&str> = prefix
                .split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ';'))
                .filter(|t| !t.is_empty())
                .collect();
            let mut prev_try = false;
            for ch in prefix.chars() {
                if ch == '{' {
                    open_braces.push(prev_try);
                    prev_try = false;
                } else if ch == '}' {
                    open_braces.pop();
                    prev_try = false;
                }
                // Track `try` keyword immediately preceding `{`.
                let _ = &tokens;
            }

            // Precise lexical scan of `prefix` for `try { ... }` containment.
            let mut in_try_stack = Vec::new();
            let bytes = prefix.as_bytes();
            let mut i = 0usize;
            let mut pending_try = false;
            while i < bytes.len() {
                if bytes[i..].starts_with(b"try")
                    && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
                    && (i + 3 >= bytes.len() || !bytes[i + 3].is_ascii_alphanumeric())
                {
                    pending_try = true;
                    i += 3;
                    continue;
                }
                if bytes[i] == b'{' {
                    in_try_stack.push(pending_try);
                    pending_try = false;
                } else if bytes[i] == b'}' {
                    in_try_stack.pop();
                    pending_try = false;
                } else if !bytes[i].is_ascii_whitespace() {
                    pending_try = false;
                }
                i += 1;
            }

            assert!(
                in_try_stack.iter().any(|&is_try| is_try),
                "localStorage at byte offset {idx} in app.js must sit inside a `try` block"
            );
            search_from = idx + "localStorage".len();
        }
        assert!(
            count >= 2,
            "expected at least 2 localStorage references in app.js (getItem and setItem), found {count}"
        );
    }

    #[test]
    fn navigation_counts_use_the_filtered_view() {
        // 1. Permitted-only tree with known public pages and register rows.
        let mut permitted_tree = base_tree();
        permitted_tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-public-one.md",
            sample_page_md(
                "cmp-public-one",
                "Public compliance concept",
                "cmp",
                "business-systems",
                "\"public\"",
            )
            .into_bytes(),
        );
        permitted_tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-public-one.md",
            sample_page_md(
                "est-public-one",
                "Public pricing concept",
                "est",
                "pricing",
                "\"public\"",
            )
            .into_bytes(),
        );
        permitted_tree.insert_bytes(
            "corpus/registers/questions.csv",
            include_bytes!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv")
                .to_vec(),
        );

        let (_model_perm, view_perm) = build_public_view_from_tree(permitted_tree.clone());
        let planned_perm = site::planned_paths(&view_perm);
        let nav_perm = nav::build(&view_perm, &planned_perm);

        assert_eq!(nav_perm.entry("area-cmp").unwrap().count, Some(1));
        assert_eq!(nav_perm.entry("area-est").unwrap().count, Some(1));
        assert_eq!(nav_perm.entry("view-prc").unwrap().count, Some(1));
        assert_eq!(nav_perm.entry("questions").unwrap().count, Some(2));
        assert_eq!(nav_perm.entry("claims").unwrap().count, Some(0));

        // 2. Add forbidden (`np-0007`) pages and relabelled records to the unrestricted model.
        let mut mixed_tree = permitted_tree;
        mixed_tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-secret-two.md",
            sample_page_md(
                "est-secret-two",
                "Forbidden pricing concept",
                "est",
                "pricing",
                "\"public\", \"np-0007\"",
            )
            .into_bytes(),
        );
        mixed_tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-secret-two.md",
            sample_page_md(
                "cmp-secret-two",
                "Forbidden compliance concept",
                "cmp",
                "business-systems",
                "\"public\", \"np-0007\"",
            )
            .into_bytes(),
        );

        let (model_mixed, view_mixed) = build_public_view_from_tree(mixed_tree);
        assert_eq!(
            model_mixed.pages().len(),
            4,
            "unrestricted model must hold all 4 pages"
        );
        assert_eq!(
            view_mixed.pages().len(),
            2,
            "filtered view must retain only the 2 public pages"
        );
        assert!(view_mixed.left_out() >= 2);

        let planned_mixed = site::planned_paths(&view_mixed);
        let nav_mixed = nav::build(&view_mixed, &planned_mixed);

        // Forbidden pages change neither counts nor available destinations.
        assert_eq!(nav_mixed, nav_perm);
        let built_dests_perm: BTreeSet<String> = nav_perm
            .groups
            .iter()
            .flat_map(|g| &g.entries)
            .filter_map(|e| match &e.target {
                NavTarget::Built { href } => Some(href.clone()),
                NavTarget::Unbuilt { .. } => None,
            })
            .collect();
        let built_dests_mixed: BTreeSet<String> = nav_mixed
            .groups
            .iter()
            .flat_map(|g| &g.entries)
            .filter_map(|e| match &e.target {
                NavTarget::Built { href } => Some(href.clone()),
                NavTarget::Unbuilt { .. } => None,
            })
            .collect();
        assert_eq!(built_dests_mixed, built_dests_perm);
    }

    #[test]
    fn unreleased_banner_ai_drafted_and_unchecked_counts() {
        // 1. fixture with 4 claims: 2 `ai`, 1 `codex`, 1 `lead`, all unchecked
        // => html contains "This is an unreleased build, not an edition.",
        //    "3 of 4 claims are AI-drafted", "4 of 4 are unchecked",
        //    and does NOT contain "checked and found supported".
        let rows = vec![
            make_claim_row("CL-CMP-0001", "ai", "unchecked"),
            make_claim_row("CL-CMP-0002", "ai", "unchecked"),
            make_claim_row("CL-CMP-0003", "codex", "unchecked"),
            make_claim_row("CL-CMP-0004", "lead", "unchecked"),
        ];
        let (_model, view) = build_public_view_with_claims(&rows);
        assert_eq!(view.claims().len(), 4);

        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = site::render(&view, &info).unwrap();
        let html = std::str::from_utf8(&site.files()["index.html"]).unwrap();

        assert!(html.contains("This is an unreleased build, not an edition."));
        assert!(html.contains("3 of 4 claims are AI-drafted"));
        assert!(html.contains("4 of 4 are unchecked"));
        assert!(!html.contains("checked and found supported"));
    }

    #[test]
    fn unreleased_banner_supported_count_rendered_when_some_checked() {
        // 2. fixture where exactly one claim is `primary_source_supported` and the others unchecked
        //    (e.g. 3 claims: 1 supported, 2 unchecked)
        // => contains "1 of 3 has been checked and found supported." and "2 of 3 are unchecked".
        let rows = vec![
            make_claim_row("CL-CMP-0001", "codex", "primary_source_supported"),
            make_claim_row("CL-CMP-0002", "codex", "unchecked"),
            make_claim_row("CL-CMP-0003", "ai", "unchecked"),
        ];
        let (_model, view) = build_public_view_with_claims(&rows);
        assert_eq!(view.claims().len(), 3);

        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = site::render(&view, &info).unwrap();
        let html = std::str::from_utf8(&site.files()["index.html"]).unwrap();

        assert!(html.contains("1 of 3 has been checked and found supported."));
        assert!(html.contains("2 of 3 are unchecked"));
    }

    #[test]
    fn unreleased_banner_no_claims_recorded() {
        // 3. fixture with no claims => contains "No claims recorded." and not "found supported" and not " of 0".
        let (_model, view) = build_public_view_with_claims(&[]);
        assert_eq!(view.claims().len(), 0);

        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = site::render(&view, &info).unwrap();
        let html = std::str::from_utf8(&site.files()["index.html"]).unwrap();

        assert!(html.contains("No claims recorded."));
        assert!(!html.contains("found supported"));
        assert!(!html.contains(" of 0"));
    }

    #[test]
    fn unreleased_banner_singular_noun_for_single_claim() {
        // 4. fixture with exactly one claim (unchecked, codex)
        // => contains "1 of 1 claim is AI-drafted; 1 of 1 is unchecked." (singular noun).
        let rows = vec![make_claim_row("CL-CMP-0001", "codex", "unchecked")];
        let (_model, view) = build_public_view_with_claims(&rows);
        assert_eq!(view.claims().len(), 1);

        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = site::render(&view, &info).unwrap();
        let html = std::str::from_utf8(&site.files()["index.html"]).unwrap();

        assert!(html.contains("1 of 1 claim is AI-drafted; 1 of 1 is unchecked."));
    }

    #[test]
    fn released_build_does_not_render_build_status_banner() {
        // 5. a released build (edition_info with a built date) via render_page
        // => the html does NOT contain `Build status`, `build-status` or "This is an unreleased build".
        let rows = vec![make_claim_row("CL-CMP-0001", "codex", "unchecked")];
        let (_model, view) = build_public_view_with_claims(&rows);
        assert_eq!(view.claims().len(), 1);

        let planned = PlannedPaths::of(&["index.html", "claims.html"]);
        let nav = nav::build(&view, &planned);
        let released = edition_info("2026-03-01", 1);
        let html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &released,
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &nav,
        });

        assert!(!html.contains("Build status"));
        assert!(!html.contains("build-status"));
        assert!(!html.contains("This is an unreleased build"));
    }

    #[test]
    fn build_status_banner_slot_position_and_company_ordering() {
        // 6. the banner is the first item of the banners slot and a region named "Build status":
        // in the unreleased html, `html.find("aria-label=\"Build status\"")` is Some
        // and comes after `id="fb:banners"` and before `<main id="fb:main"`;
        // and for a StoreContext::Company view, assert the "Handling notice" region comes AFTER the "Build status" region.
        let rows = vec![make_claim_row("CL-CMP-0001", "codex", "unchecked")];
        let (_model, view) = build_public_view_with_claims(&rows);
        assert_eq!(view.claims().len(), 1);

        let planned = PlannedPaths::of(&["index.html", "claims.html"]);
        let nav = nav::build(&view, &planned);
        let unreleased = edition_info("", 0);

        let html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &unreleased,
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &nav,
        });

        let banner_pos = html
            .find("aria-label=\"Build status\"")
            .expect("Build status region must exist");
        let banners_pos = html
            .find("id=\"fb:banners\"")
            .expect("banners slot must exist");
        let main_pos = html
            .find("<main id=\"fb:main\"")
            .expect("main element must exist");
        assert!(
            banner_pos > banners_pos,
            "Build status region must come after id=\"fb:banners\""
        );
        assert!(
            banner_pos < main_pos,
            "Build status region must come before main element"
        );

        let bid3 = LabelId::parse("bid-0003").unwrap();
        let np7 = LabelId::parse("np-0007").unwrap();
        let company_store = StoreContext::Company {
            effective_labels: LabelSet::from_ids([bid3, np7]),
        };
        let company_html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &unreleased,
            store: &company_store,
            crumbs: &[],
            main_html: "",
            nav: &nav,
        });
        let company_banner_pos = company_html
            .find("aria-label=\"Build status\"")
            .expect("Build status region must exist in company view");
        let handling_pos = company_html
            .find("aria-label=\"Handling notice\"")
            .expect("Handling notice region must exist in company view");
        assert!(
            handling_pos > company_banner_pos,
            "Handling notice region must come after Build status region"
        );
    }

    #[test]
    fn unreleased_banner_claims_link_relative_prefix_at_depths() {
        // 7. the banner link is `<a href="claims.html">` at the root and `href="../claims.html"`
        //    for a page one level down (e.g. render_page with output_path "p/fnd-0001.html").
        let rows = vec![make_claim_row("CL-CMP-0001", "codex", "unchecked")];
        let (_model, view) = build_public_view_with_claims(&rows);
        assert_eq!(view.claims().len(), 1);

        let planned = PlannedPaths::of(&["index.html", "claims.html", "p/fnd-0001.html"]);
        let nav = nav::build(&view, &planned);
        let unreleased = edition_info("", 0);

        let root_html = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &unreleased,
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &nav,
        });
        assert!(
            root_html.contains("<a href=\"claims.html\">the claims register</a>"),
            "root page must link to claims.html directly"
        );

        let nested_html = render_page(&FrameInput {
            output_path: "p/fnd-0001.html",
            title: PRODUCT_TITLE,
            page_area: Some("fnd"),
            edition: &unreleased,
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &nav,
        });
        assert!(
            nested_html.contains("<a href=\"../claims.html\">the claims register</a>"),
            "nested page must link to ../claims.html"
        );
    }

    #[test]
    fn build_status_banner_states_review_rule() {
        const REVIEW_RULE: &str = "Each page shows its review date; none is flagged as overdue.";

        // Unreleased build with checked claims:
        let rows = vec![
            make_claim_row("CL-CMP-0001", "codex", "primary_source_supported"),
            make_claim_row("CL-CMP-0002", "codex", "primary_source_supported"),
            make_claim_row("CL-CMP-0003", "ai", "unchecked"),
        ];
        let (_model, view) = build_public_view_with_claims(&rows);
        assert_eq!(view.claims().len(), 3);

        let info = BuildInfo::new(None, None, BuildProfile::Silent, None);
        let site = site::render(&view, &info).unwrap();
        let html = std::str::from_utf8(&site.files()["index.html"]).unwrap();

        assert_eq!(html.matches(REVIEW_RULE).count(), 1);
        let supported_pos = html
            .find("have been checked and found supported")
            .expect("must contain have been checked and found supported");
        let rule_pos = html.find(REVIEW_RULE).expect("must contain review rule");
        let see_pos = html
            .find("the claims register")
            .expect("must contain the claims register");
        assert!(
            supported_pos < rule_pos,
            "review rule must appear after 'have been checked and found supported'"
        );
        assert!(
            rule_pos < see_pos,
            "review rule must appear before the claims register link"
        );

        // Unreleased build with no claims recorded:
        let (_model, view_empty) = build_public_view_with_claims(&[]);
        assert_eq!(view_empty.claims().len(), 0);

        let site_empty = site::render(&view_empty, &info).unwrap();
        let html_empty = std::str::from_utf8(&site_empty.files()["index.html"]).unwrap();

        assert!(html_empty.contains("No claims recorded."));
        assert_eq!(html_empty.matches(REVIEW_RULE).count(), 1);
        let no_claims_pos = html_empty
            .find("No claims recorded.")
            .expect("must contain No claims recorded.");
        let rule_empty_pos = html_empty
            .find(REVIEW_RULE)
            .expect("must contain review rule");
        let see_empty_pos = html_empty
            .find("the claims register")
            .expect("must contain the claims register");
        assert!(no_claims_pos < rule_empty_pos);
        assert!(rule_empty_pos < see_empty_pos);

        // Released build: review rule is absent:
        let planned = PlannedPaths::of(&["index.html", "claims.html"]);
        let nav = nav::build(&view, &planned);
        let released = edition_info("2026-03-01", 1);
        let html_released = render_page(&FrameInput {
            output_path: "index.html",
            title: PRODUCT_TITLE,
            page_area: None,
            edition: &released,
            store: &StoreContext::Public,
            crumbs: &[],
            main_html: "",
            nav: &nav,
        });
        assert!(!html_released.contains(REVIEW_RULE));
    }
}
