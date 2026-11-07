//! Integration tests for `fieldbook_render::{about, chip, edition}` (`build-about`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod about {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::strings;
    use fieldbook_render::about::{self, NETWORK_POSTURE_SHORT};
    use fieldbook_render::chip::{self, ChipError};
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::site::{self, Site};

    const CARDS_README: &str = include_str!("../../../docs/cards/README.md");
    const APP_JS: &str = include_str!("../assets/app.js");
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

    fn fixture_tree() -> Tree {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
        let mut tree = Tree::new();
        insert_all(&mut tree, &root.join("corpus"), "corpus");
        tree
    }

    /// The fixture with an edition heading, so that a released build can name its edition.
    fn released_tree() -> Tree {
        let mut tree = fixture_tree();
        let changes = std::str::from_utf8(tree.get_bytes("corpus/changes.md").unwrap())
            .unwrap()
            .to_owned();
        tree.insert_bytes(
            "corpus/changes.md",
            format!("{changes}\n## edition-007\n\n- Added: synthetic.\n").into_bytes(),
        );
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

    fn built(tree: Tree, info: &BuildInfo) -> Site {
        let (_model, view) = load_view(tree);
        site::render(&view, info).unwrap()
    }

    fn released_info() -> BuildInfo {
        let date = CivilDate::parse("2026-03-01").unwrap();
        BuildInfo::new(
            Some(date),
            Some(COMMIT.to_owned()),
            BuildProfile::Silent,
            Some(1_772_323_200),
        )
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

    fn about_main(html: &str) -> &str {
        let start = html.find("<main").unwrap();
        let end = html[start..].find("</main>").unwrap();
        &html[start..start + end]
    }

    fn readme_short() -> &'static str {
        let line = CARDS_README
            .lines()
            .find_map(|line| line.strip_prefix("> **Short.** "))
            .unwrap_or_else(|| panic!("the README block has a Short paragraph"));
        line.trim_end()
    }

    #[test]
    fn unreleased_without_source_date_epoch() {
        let site = built(fixture_tree(), &BuildInfo::default());
        let html = text(&site, about::OUTPUT_PATH);
        assert!(about_main(&html).contains(strings::unreleased_build()));
        assert!(html.contains(&format!(
            "<span class=\"chip\">{}</span>",
            strings::unreleased_build()
        )));
        let toml = text(&site, "edition.toml");
        assert!(toml.starts_with("edition = \"unreleased\"\nnumber = 0\nbuilt = \"\"\n"));
        assert!(toml.contains("commit = \"\"\n"));
    }

    #[test]
    fn edition_toml_keys_in_fixed_order() {
        let site = built(released_tree(), &released_info());
        let toml = text(&site, "edition.toml");
        let keys: Vec<&str> = toml
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| line.split(" = ").next().unwrap())
            .collect();
        assert_eq!(
            keys,
            [
                "edition",
                "number",
                "built",
                "tool_version",
                "min_tool_version",
                "schema_version",
                "commit",
                "fingerprint",
                "[counts]",
                "pages",
                "source_versions",
                "claims",
                "questions",
                "conflicts",
                "terms",
                "originals",
                "[ids]",
                "pages",
                "source_versions",
                "claims",
                "questions",
                "conflicts",
                "terms",
            ]
        );
        assert!(
            toml.starts_with("edition = \"edition-007\"\nnumber = 7\nbuilt = \"2026-03-01\"\n")
        );
        assert!(toml.contains(&format!("commit = \"{COMMIT}\"\n")));
    }

    #[test]
    fn two_builds_give_the_same_edition_toml() {
        for info in [BuildInfo::default(), released_info()] {
            let first = built(released_tree(), &info);
            let second = built(released_tree(), &info);
            assert_eq!(text(&first, "edition.toml"), text(&second, "edition.toml"));
            assert_eq!(
                text(&first, about::OUTPUT_PATH),
                text(&second, about::OUTPUT_PATH)
            );
        }
    }

    #[test]
    fn about_short_form_matches_the_readme() {
        assert_eq!(NETWORK_POSTURE_SHORT, readme_short());
    }

    #[test]
    fn chip_refuses_free_text_in_an_id_placeholder() {
        let free_text = "CL-CMP-0001 --on 2026-01-01 & del /q *";
        assert_eq!(
            chip::quote(chip::KEY_VERIFY, &[("claim-id", free_text)]),
            Err(ChipError::InvalidValue {
                placeholder: "claim-id".to_owned(),
                value: free_text.to_owned(),
            })
        );
        let filled = chip::quote(chip::KEY_VERIFY, &[("claim-id", "CL-CMP-0001")]).unwrap();
        assert_eq!(
            filled.command,
            format!("{} verify CL-CMP-0001 --on YYYY-MM-DD", strings::binary())
        );

        // An area code is lowercase, a page type is a page type.
        let start_page = "Start a page";
        assert!(chip::quote(start_page, &[("area", "CMP")]).is_err());
        assert!(chip::quote(start_page, &[("area", "cmp")]).is_ok());
        assert!(chip::quote(start_page, &[("type", "concept")]).is_ok());
        assert!(chip::quote(start_page, &[("type", "concept --force")]).is_err());

        // A text placeholder stays for the owner: it takes no value and is left as written.
        let question = "Add a question";
        assert_eq!(
            chip::quote(question, &[("text", "Anything at all")]),
            Err(ChipError::TextPlaceholder("text".to_owned()))
        );
        let written = chip::quote(question, &[("a", "cmp")]).unwrap();
        assert!(written.command.ends_with("--area cmp \"<text>\""));

        assert!(matches!(
            chip::quote("Not a chip", &[]),
            Err(ChipError::UnknownChip(_))
        ));
        assert!(matches!(
            chip::quote(chip::KEY_VERIFY, &[("area", "cmp")]),
            Err(ChipError::UnknownPlaceholder(_))
        ));
    }

    #[test]
    fn chip_keys_are_unique() {
        let mut keys: Vec<&str> = strings::chips().iter().map(|row| row.intent).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), strings::chips().len());
    }

    #[test]
    fn r2_chips_carry_the_r2_tag() {
        let mut r2 = 0;
        for row in strings::chips() {
            let built = chip::quote(row.intent, &[]).unwrap();
            let is_r2 = row.from == strings::ReleaseFrom::R2;
            let expected = if is_r2 {
                r2 += 1;
                strings::tag_r2_laptop()
            } else {
                strings::tag_at_home()
            };
            assert_eq!(built.tag, expected, "{}", row.chip);
            assert_eq!(built.command, row.chip);
            assert!(
                chip::render(&built)
                    .contains(&format!("<span class=\"cmd-tag\">{expected}</span>"))
            );
        }
        assert!(r2 >= 2, "the R2 chips include the overlay build and sync");
    }

    #[test]
    fn chip_part_has_the_group_the_mono_command_and_copy() {
        let html = chip::render(&chip::quote(chip::KEY_VERIFY, &[]).unwrap());
        assert!(html.contains("role=\"group\""));
        assert!(html.contains("<code>"));
        assert!(html.contains("data-copy-chip=\"command\""));
        assert!(html.contains(&format!(
            "aria-label=\"Copy command: {} verify",
            strings::binary()
        )));

        let landing = chip::render_or_landing(None, "The queue", "R2");
        assert!(landing.contains("The queue is not available until R2."));
        assert!(!landing.contains("<code>"));

        let address = chip::render_address("Source address", "https://example.invalid/a?b=1&c=2");
        assert!(address.contains("data-copy-chip=\"address\""));
        assert!(address.contains("https://example.invalid/a?b=1&#38;c=2"));
        assert!(
            !address.contains("cmd-tag"),
            "an address has no command tag"
        );
        assert!(!address.contains("href="), "an address is text, not a link");
    }

    /// The copy script writes on a click, falls back to selecting the text, and opens no dialog.
    #[test]
    fn copy_only_writes_and_opens_no_dialog() {
        assert!(APP_JS.contains("clip.writeText(code.textContent)"));
        assert!(APP_JS.contains("selection.addRange(range)"));
        assert!(
            APP_JS.contains("const refused = function () {\n      btn.focus();"),
            "a refused copy must return focus to the control that asked for it"
        );
        for forbidden in [
            "readText",
            "clipboard.read",
            "alert(",
            "confirm(",
            "prompt(",
            "window.print",
        ] {
            assert!(
                !APP_JS.contains(forbidden),
                "app.js must not use {forbidden}"
            );
        }
    }

    #[test]
    fn about_names_the_build_profile() {
        let site = built(fixture_tree(), &BuildInfo::default());
        let html = text(&site, about::OUTPUT_PATH);
        let main = about_main(&html);
        assert!(NETWORK_POSTURE_SHORT.contains("silent build"));
        assert!(main.contains("<span class=\"panel-sub mono\">silent build</span>"));
        assert!(
            main.contains(readme_short()),
            "the page quotes the Short form:\n{main}"
        );
        assert!(main.contains("The silent build is the only build in this edition."));
        assert!(main.contains("The AI build is not in this edition."));
        assert!(!html.contains("Connected"), "About reports no live state");

        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let ai = about::render_main(&view, &ctx, BuildProfile::Ai, &site::planned_paths(&view));
        assert!(ai.contains("<span class=\"panel-sub mono\">AI build</span>"));
        assert!(!ai.contains("only build in this edition"));
    }

    #[test]
    fn about_states_public_only_and_has_no_company_control() {
        let site = built(fixture_tree(), &BuildInfo::default());
        let html = text(&site, about::OUTPUT_PATH);
        let main = about_main(&html);
        assert!(main.contains("This edition holds public only."));
        for control in [
            "policy show",
            "sync --dry-run",
            "<button",
            "<form",
            "<input",
        ] {
            assert!(!main.contains(control), "About must not hold `{control}`");
        }
        // The AI position is text: the only links in the main part are claim chips.
        let section = main.split("id=\"about-ai\"").nth(1).unwrap();
        let section = section.split("</section>").next().unwrap();
        assert!(!section.contains("<a "));
    }

    #[test]
    fn about_lists_show_counts_items_and_empty_lines() {
        let site = built(fixture_tree(), &BuildInfo::default());
        let html = text(&site, about::OUTPUT_PATH);
        let main = about_main(&html);
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let counts = ctx.edition_info().counts;
        assert!(counts.pages > 0 && counts.claims > 0);
        assert!(main.contains(&format!(
            "<dt>Claims</dt><dd class=\"mono\">{}</dd>",
            counts.claims
        )));
        let unchecked = view
            .claims()
            .iter()
            .filter(|c| c.value.verification == fieldbook_core::schema::Verification::Unchecked)
            .count();
        assert!(main.contains(&format!(
            "<dt>Claims unchecked</dt><dd class=\"mono\">{unchecked}</dd>"
        )));
        for claim in view.claims() {
            let c = &claim.value;
            let expected = c.consequential
                && c.verification == fieldbook_core::schema::Verification::Unchecked;
            let panel = main
                .split("id=\"tu-unreviewed-consequential-claims\"")
                .nth(1)
                .unwrap();
            assert_eq!(
                panel.contains(c.claim_id.as_str()),
                expected,
                "{}",
                c.claim_id
            );
        }
        let stale = main.split("id=\"tu-stale-items\"").nth(1).unwrap();
        let stale = stale.split("</section>").next().unwrap();
        assert!(
            stale.contains("<p class=\"muted\">None recorded</p>"),
            "an empty list shows its empty line: {stale}"
        );
    }

    #[test]
    fn header_shows_edition_and_build_date_without_scripts() {
        let site = built(released_tree(), &released_info());
        let html = text(&site, "index.html");
        let element = "<span class=\"chip build-date\">edition 007 · <time datetime=\"2026-03-01\">2026-03-01</time></span>";
        assert!(html.contains(element), "{html}");
        assert!(text(&site, about::OUTPUT_PATH).contains(element));
        let main = text(&site, about::OUTPUT_PATH);
        assert!(about_main(&main).contains("edition-007"));
        assert!(about_main(&main).contains(COMMIT));
    }

    /// `app.js` writes the age inside a `try`, whose `catch` writes nothing.
    #[test]
    fn header_age_script_keeps_the_date_text_on_error() {
        let start = APP_JS.find("function initBuildAge()").unwrap();
        let body = &APP_JS[start..];
        let body = &body[..body.find("function selectChipText").unwrap()];
        let try_at = body.find("try {").unwrap();
        let write_at = body.find("stamp.textContent =").unwrap();
        let catch_at = body.find("} catch (_err) {").unwrap();
        assert!(try_at < write_at && write_at < catch_at);
        assert_eq!(body.matches("textContent").count(), 1);
        assert_eq!(
            APP_JS.matches("stamp.textContent").count(),
            1,
            "no other code writes the date text"
        );
        assert!(
            !body[catch_at..].contains('='),
            "the catch leaves the text as the build wrote it"
        );
    }

    #[test]
    fn every_page_has_the_csp_meta_and_no_style_attribute() {
        let site = built(fixture_tree(), &BuildInfo::default());
        let mut pages = 0;
        for (path, bytes) in site.files() {
            if !path.ends_with(".html") {
                continue;
            }
            pages += 1;
            let html = std::str::from_utf8(bytes).unwrap();
            assert!(
                html.contains(&format!(
                    "<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">"
                )),
                "{path} lacks the CSP meta"
            );
            assert!(!html.contains("style=\""), "{path} has a style attribute");
        }
        assert!(pages > 1 && site.files().contains_key(about::OUTPUT_PATH));
    }
}
