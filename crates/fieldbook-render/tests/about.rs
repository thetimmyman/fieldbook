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
    use fieldbook_core::schema::{PageStatus, Verification};
    use fieldbook_core::strings;
    use fieldbook_render::about::{self, CLAIM_LIST_SHOWN, NETWORK_POSTURE_SHORT};
    use fieldbook_render::chip::{self, ChipError};
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::frame::root_prefix_for_path;
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::site::{self, Site};
    use fieldbook_render::source;

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

    /// The fixture with `edit` applied to the text of one register.
    fn tree_with_register(name: &str, edit: impl Fn(&str) -> String) -> Tree {
        let mut tree = fixture_tree();
        let path = format!("corpus/registers/{name}.csv");
        let csv = std::str::from_utf8(tree.get_bytes(&path).unwrap())
            .unwrap()
            .to_owned();
        let edited = edit(&csv);
        assert_ne!(edited, csv, "the plant must change {name}.csv");
        tree.insert_bytes(path, edited.into_bytes());
        tree
    }

    /// One claim of the fixture, moved from `unchecked` to `stale`.
    fn tree_with_stale_claim() -> Tree {
        tree_with_register("claims", |csv| {
            csv.lines()
                .map(|line| {
                    if line.starts_with("CL-CMP-0002,") {
                        line.replace(",unchecked,", ",stale,")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        })
    }

    /// The fixture with `extra` more consequential, unchecked claims in the register.
    fn tree_with_unreviewed_claims(extra: usize) -> Tree {
        tree_with_register("claims", |csv| {
            let model = csv
                .lines()
                .find(|line| line.starts_with("CL-CMP-0002,"))
                .unwrap();
            let mut lines: Vec<String> = Vec::new();
            for line in csv.lines() {
                lines.push(line.to_owned());
                if line == model {
                    for n in 0..extra {
                        lines
                            .push(model.replace("CL-CMP-0002", &format!("CL-CMP-{:04}", 1000 + n)));
                    }
                }
            }
            lines.join("\n") + "\n"
        })
    }

    /// The rows of one panel of the About page.
    fn panel<'a>(main: &'a str, id: &str) -> &'a str {
        let body = main.split(&format!("id=\"{id}\"")).nth(1).unwrap();
        body.split("</section>").next().unwrap()
    }

    /// The Short form as the page writes it: its code spans in `<code>`, no backticks.
    fn short_form_on_the_page() -> String {
        readme_short()
            .split('`')
            .enumerate()
            .map(|(index, part)| {
                if index % 2 == 1 {
                    format!("<code>{part}</code>")
                } else {
                    part.to_owned()
                }
            })
            .collect()
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
            APP_JS.contains("const refused = function () {\n    btn.focus();"),
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
            main.contains(&short_form_on_the_page()),
            "the page quotes the Short form:\n{main}"
        );
        assert!(
            !panel(main, "about-profile").contains('`'),
            "a code span shows as code, not as backticks"
        );
        assert!(main.contains("The silent build is the only build in this edition."));
        assert!(main.contains("The AI build is not in this edition."));
        assert!(!html.contains("Connected"), "About reports no live state");

        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let ai = about::render_main(&view, &ctx, BuildProfile::Ai, &site::planned_paths(&view));
        assert!(ai.contains("<span class=\"panel-sub mono\">AI build</span>"));
        assert!(!ai.contains("only build in this edition"));
        assert!(
            !ai.contains("The AI build is not in this edition."),
            "an AI build's page must not deny the AI build:\n{ai}"
        );
        assert!(panel(&ai, "about-ai").contains("This is the AI build."));
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
    fn about_status_counts_match_the_view() {
        let (_model, view) = load_view(fixture_tree());
        let site = built(fixture_tree(), &BuildInfo::default());
        let html = text(&site, about::OUTPUT_PATH);
        let main = about_main(&html);
        for status in PageStatus::ALL {
            let n = view
                .pages()
                .iter()
                .filter(|page| page.meta.status == status)
                .count();
            let row = format!(
                "<dt>Pages {}</dt><dd class=\"mono\">{n}</dd>",
                status.as_str()
            );
            assert!(main.contains(&row), "{row}\n{main}");
        }
        for (verification, word) in [
            (Verification::PrimarySourceSupported, "supported"),
            (Verification::Contradicted, "contradicted"),
            (Verification::Stale, "stale"),
            (Verification::Unresolved, "unresolved"),
            (Verification::Unchecked, "unchecked"),
        ] {
            let n = view
                .claims()
                .iter()
                .filter(|claim| claim.value.verification == verification)
                .count();
            let row = format!("<dt>Claims {word}</dt><dd class=\"mono\">{n}</dd>");
            assert!(main.contains(&row), "{row}\n{main}");
        }
        assert!(
            !main.contains("primary_source_supported"),
            "a status count reads as a word, not a raw vocabulary value"
        );
    }

    #[test]
    fn about_lists_a_stale_claim() {
        let site = built(tree_with_stale_claim(), &BuildInfo::default());
        let html = text(&site, about::OUTPUT_PATH);
        let main = about_main(&html);
        let stale = panel(main, "tu-stale-items");
        assert!(stale.contains("CL-CMP-0002"), "{stale}");
        assert!(
            stale.contains("<span class=\"panel-sub mono\">1</span>"),
            "{stale}"
        );
        assert!(!stale.contains("None recorded"));
        assert!(!stale.contains("CL-CMP-0001"));
        let unreviewed = panel(main, "tu-unreviewed-consequential-claims");
        assert!(
            !unreviewed.contains("CL-CMP-0002"),
            "a stale claim is not an unchecked one: {unreviewed}"
        );
        assert!(main.contains("<dt>Claims stale</dt><dd class=\"mono\">1</dd>"));
    }

    /// The panels write a bounded list and the true count, so the page does not grow with the
    /// Corpus; the claims register holds the rest.
    #[test]
    fn about_claim_lists_are_bounded_and_count_every_claim() {
        let small = text(
            &built(tree_with_unreviewed_claims(100), &BuildInfo::default()),
            about::OUTPUT_PATH,
        );
        let large = text(
            &built(tree_with_unreviewed_claims(900), &BuildInfo::default()),
            about::OUTPUT_PATH,
        );
        let (_model, view) = load_view(tree_with_unreviewed_claims(900));
        let total = view
            .claims()
            .iter()
            .filter(|c| c.value.consequential && c.value.verification == Verification::Unchecked)
            .count();
        assert!(total > 900);
        let main = about_main(&large);
        let unreviewed = panel(main, "tu-unreviewed-consequential-claims");
        assert_eq!(
            unreviewed.matches("class=\"tu-item\"").count(),
            CLAIM_LIST_SHOWN
        );
        assert!(unreviewed.contains(&format!("<span class=\"panel-sub mono\">{total}</span>")));
        assert!(
            unreviewed.contains(&format!(
                "<a href=\"claims.html\">{} more in the claims register</a>",
                total - CLAIM_LIST_SHOWN
            )),
            "{unreviewed}"
        );
        // Only the digits of the counts and the register's own sidebar count differ.
        assert!(
            large.len().abs_diff(small.len()) < 64,
            "the page grew with the Corpus: {} and {} bytes",
            small.len(),
            large.len()
        );
    }

    /// An original is counted by its version id, as the build copies it; `location` is usually a
    /// web address. A held version ships no original.
    #[test]
    fn originals_count_is_the_shipped_selection() {
        let web_address = |csv: &str| {
            csv.replace(
                ",S-0002-v2.txt,",
                ",https://example.invalid/synthetic-pricing-guide,",
            )
        };
        let (_model, view) = load_view(tree_with_register("sources", web_address));
        assert_eq!(source::shipped_originals(&view).len(), 1);
        let site = built(
            tree_with_register("sources", web_address),
            &BuildInfo::default(),
        );
        let html = text(&site, about::OUTPUT_PATH);
        assert!(
            about_main(&html).contains("<dt>Originals</dt><dd class=\"mono\">1</dd>"),
            "{html}"
        );
        assert!(text(&site, "edition.toml").contains("\noriginals = 1\n"));

        let held = tree_with_register("sources", |csv| {
            csv.lines()
                .map(|line| {
                    if line.starts_with("S-0002,2,") {
                        line.replace(",active,", ",quarantined,")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        });
        let site = built(held, &BuildInfo::default());
        assert!(text(&site, "edition.toml").contains("\noriginals = 0\n"));
    }

    /// The build-date element is the way to About from every page, in both of its forms.
    #[test]
    fn every_page_links_to_about() {
        for (tree, info) in [
            (fixture_tree(), BuildInfo::default()),
            (released_tree(), released_info()),
        ] {
            let site = built(tree, &info);
            let mut pages = 0;
            for (path, bytes) in site.files() {
                if !path.ends_with(".html") {
                    continue;
                }
                pages += 1;
                let html = std::str::from_utf8(bytes).unwrap();
                let link = format!(
                    "<a class=\"build-date-link\" href=\"{}{}\">",
                    root_prefix_for_path(path),
                    about::OUTPUT_PATH
                );
                assert!(html.contains(&link), "{path} has no link to About");
            }
            assert!(pages > 1);
        }
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
