//! Integration tests for `fieldbook_render::changes`.

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod changes_tests {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::edition::{EditionCounts, EditionIds, EditionInfo};
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::changes::{self, GROUP_IDS_SHOWN, NOTES_SHOWN};
    use fieldbook_render::csp::CSP_STATIC;
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::nav;
    use fieldbook_render::site::{self, PlannedPaths, Site};

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
    const FINGERPRINT: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

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

    fn main_slice(html: &str) -> &str {
        let start = html.find("<main").unwrap();
        let end = html[start..].find("</main>").unwrap();
        &html[start..start + end]
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

    fn sample_edition_info(edition: &str, number: u32, ids: EditionIds) -> EditionInfo {
        let is_unreleased = edition == "unreleased";
        EditionInfo {
            edition: edition.to_owned(),
            number: if is_unreleased { 0 } else { number },
            built: if is_unreleased {
                String::new()
            } else {
                "2026-02-01".to_owned()
            },
            tool_version: "0.1.0".to_owned(),
            min_tool_version: "0.1.0".to_owned(),
            schema_version: 1,
            commit: if is_unreleased {
                String::new()
            } else {
                COMMIT.to_owned()
            },
            fingerprint: FINGERPRINT.to_owned(),
            counts: EditionCounts {
                pages: ids.pages.len(),
                source_versions: ids.source_versions.len(),
                claims: ids.claims.len(),
                questions: ids.questions.len(),
                conflicts: ids.conflicts.len(),
                terms: ids.terms.len(),
                originals: 0,
            },
            ids,
        }
    }

    #[test]
    fn diff_lists_added_and_removed_per_group() {
        let previous = EditionIds {
            pages: vec![
                "cmp-beta".to_owned(),
                "cmp-common".to_owned(),
                "cmp-alpha".to_owned(),
            ],
            source_versions: vec![
                "S-0001-v10".to_owned(),
                "S-0001-v1".to_owned(),
                "S-0001-v2".to_owned(),
            ],
            claims: vec![
                "CL-CMP-10000".to_owned(),
                "CL-CMP-0001".to_owned(),
                "CL-CMP-9999".to_owned(),
            ],
            questions: vec!["Q-010".to_owned(), "Q-001".to_owned(), "Q-002".to_owned()],
            conflicts: vec!["XC-100".to_owned(), "XC-01".to_owned(), "XC-14".to_owned()],
            terms: vec![
                "term_zeta".to_owned(),
                "term_shared".to_owned(),
                "term_alpha".to_owned(),
            ],
        };
        let current = EditionIds {
            pages: vec![
                "cmp-common".to_owned(),
                "cmp-gamma".to_owned(),
                "cmp-delta".to_owned(),
            ],
            source_versions: vec![
                "S-0001-v1".to_owned(),
                "S-0002-v10".to_owned(),
                "S-0002-v2".to_owned(),
            ],
            claims: vec![
                "CL-CMP-0001".to_owned(),
                "CL-CMP-10001".to_owned(),
                "CL-CMP-0002".to_owned(),
            ],
            questions: vec!["Q-001".to_owned(), "Q-025".to_owned(), "Q-003".to_owned()],
            conflicts: vec!["XC-01".to_owned(), "XC-101".to_owned(), "XC-09".to_owned()],
            terms: vec![
                "term_shared".to_owned(),
                "term_omega".to_owned(),
                "term_beta".to_owned(),
            ],
        };

        let diff = changes::diff_ids(&previous, &current);
        assert!(!diff.is_empty());
        assert_eq!(diff.hidden, 0);

        assert_eq!(diff.pages.added, ["cmp-delta", "cmp-gamma"]);
        assert_eq!(diff.pages.removed, ["cmp-alpha", "cmp-beta"]);

        assert_eq!(diff.source_versions.added, ["S-0002-v2", "S-0002-v10"]);
        assert_eq!(diff.source_versions.removed, ["S-0001-v2", "S-0001-v10"]);

        assert_eq!(diff.claims.added, ["CL-CMP-0002", "CL-CMP-10001"]);
        assert_eq!(diff.claims.removed, ["CL-CMP-9999", "CL-CMP-10000"]);

        assert_eq!(diff.questions.added, ["Q-003", "Q-025"]);
        assert_eq!(diff.questions.removed, ["Q-002", "Q-010"]);

        assert_eq!(diff.conflicts.added, ["XC-09", "XC-101"]);
        assert_eq!(diff.conflicts.removed, ["XC-14", "XC-100"]);

        assert_eq!(diff.terms.added, ["term_beta", "term_omega"]);
        assert_eq!(diff.terms.removed, ["term_alpha", "term_zeta"]);
    }

    #[test]
    fn diff_is_empty_for_equal_ids() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let ids = ctx.edition_info().ids.clone();

        let diff = changes::diff_ids(&ids, &ids);
        assert!(diff.is_empty());
        assert_eq!(diff.hidden, 0);
        assert!(diff.pages.is_empty());
        assert!(diff.source_versions.is_empty());
        assert!(diff.claims.is_empty());
        assert!(diff.questions.is_empty());
        assert!(diff.conflicts.is_empty());
        assert!(diff.terms.is_empty());

        let prev = sample_edition_info("edition-001", 1, ids);
        let site =
            site::render_with_previous(&view, &ctx, &BuildInfo::default(), Some(&prev)).unwrap();
        let html = text(&site, changes::OUTPUT_PATH);
        let main = main_slice(&html);
        assert!(main.contains("Ids since edition-001"), "{main}");
        assert!(
            main.contains("No ids were added or removed since edition-001."),
            "{main}"
        );
        assert!(!main.contains("<table"), "{main}");
    }

    #[test]
    fn removed_ids_without_the_public_form_are_counted_not_printed() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let current_ids = ctx.edition_info().ids.clone();

        let mut previous_ids = current_ids.clone();
        previous_ids.pages.push("xes-vendor-quotes".to_owned());
        previous_ids.pages.push("not a page".to_owned());
        previous_ids.source_versions.push("S-00012-v1".to_owned());
        previous_ids.claims.push("CL-XES-0012".to_owned());
        previous_ids.claims.push("CL-XES-000012".to_owned());
        previous_ids.questions.push("Q-10000".to_owned());
        previous_ids.conflicts.push("XC-14".to_owned());
        previous_ids.conflicts.push("XC-bad".to_owned());
        previous_ids.terms.push("x_vendor_tier".to_owned());

        let diff = changes::diff_ids(&previous_ids, &current_ids);
        assert_eq!(diff.hidden, 8);
        assert!(diff.pages.removed.is_empty());
        assert!(diff.source_versions.removed.is_empty());
        assert!(diff.claims.removed.is_empty());
        assert!(diff.questions.removed.is_empty());
        assert_eq!(diff.conflicts.removed, ["XC-14"]);
        assert!(diff.terms.removed.is_empty());

        let prev = sample_edition_info("edition-001", 1, previous_ids);
        let site =
            site::render_with_previous(&view, &ctx, &BuildInfo::default(), Some(&prev)).unwrap();
        let html = text(&site, changes::OUTPUT_PATH);
        let main = main_slice(&html);

        for forbidden in [
            "xes-vendor-quotes",
            "not a page",
            "S-00012-v1",
            "CL-XES-0012",
            "CL-XES-000012",
            "Q-10000",
            "XC-bad",
            "x_vendor_tier",
        ] {
            assert!(
                !html.contains(forbidden),
                "non-public removed id `{forbidden}` must not appear in changes.html"
            );
        }

        assert!(
            main.contains("8 removed ids are not listed. They are not in the public id form."),
            "{main}"
        );
        assert!(
            main.contains(">XC-14</span>"),
            "XC-14 stays public and plain text:\n{main}"
        );
        assert!(
            !main.contains("#XC-14"),
            "removed XC-14 must not link:\n{main}"
        );

        // Singular form when exactly 1 removed id is hidden.
        let mut one_hidden = current_ids;
        one_hidden.pages.push("xes-vendor-quotes".to_owned());
        let prev_one = sample_edition_info("edition-001", 1, one_hidden);
        let site_one =
            site::render_with_previous(&view, &ctx, &BuildInfo::default(), Some(&prev_one))
                .unwrap();
        let html_one = text(&site_one, changes::OUTPUT_PATH);
        assert!(!html_one.contains("xes-vendor-quotes"));
        assert!(
            main_slice(&html_one)
                .contains("1 removed id is not listed. It is not in the public id form."),
            "{html_one}"
        );
    }

    #[test]
    fn page_lists_notes_newest_first() {
        // Empty changes.md (no ## edition-NNN sections) shows empty state.
        let empty_site = site::render(&load_view(fixture_tree()).1, &BuildInfo::default()).unwrap();
        let empty_main = main_slice(&text(&empty_site, changes::OUTPUT_PATH)).to_owned();
        assert!(empty_main.contains("Edition notes"), "{empty_main}");
        assert!(
            empty_main.contains("No edition notes recorded."),
            "{empty_main}"
        );

        // Five sections: newest 3 shown, oldest 2 in closed Fold "2 more" / "older editions",
        // missing bullets omitted, and markup characters escaped as plain text.
        let mut tree = fixture_tree();
        let header = std::str::from_utf8(tree.get_bytes("corpus/changes.md").unwrap())
            .unwrap()
            .to_owned();
        let notes = concat!(
            "\n## edition-005\n\n",
            "- Added: fifth <script>alert(1)</script> & note.\n",
            "- Changed: fifth change.\n",
            "- Verified: fifth check.\n",
            "\n## edition-004\n\n",
            "- Added: fourth note only.\n",
            "\n## edition-003\n\n",
            "- Changed: third change only.\n",
            "\n## edition-002\n\n",
            "- Verified: second verification.\n",
            "\n## edition-001\n\n",
            "- Added: first edition.\n",
        );
        tree.insert_bytes("corpus/changes.md", format!("{header}{notes}").into_bytes());

        let (_model, view) = load_view(tree);
        let date = CivilDate::parse("2026-03-01").unwrap();
        let info = BuildInfo::new(
            Some(date),
            Some(COMMIT.to_owned()),
            BuildProfile::Silent,
            Some(1_772_323_200),
        );
        let site = site::render(&view, &info).unwrap();
        let html = text(&site, changes::OUTPUT_PATH);
        let main = main_slice(&html);

        assert_eq!(NOTES_SHOWN, 3);
        let pos5 = main.find("edition-005").unwrap();
        let pos4 = main.find("edition-004").unwrap();
        let pos3 = main.find("edition-003").unwrap();
        let fold_pos = main.find("<details").unwrap();
        let pos2 = main.find("edition-002").unwrap();
        let pos1 = main.find("edition-001").unwrap();
        assert!(pos5 < pos4 && pos4 < pos3 && pos3 < fold_pos && fold_pos < pos2 && pos2 < pos1);

        // Closed fold (no `open` attribute), summary has "2 more" and "older editions".
        let fold_tag = &main[fold_pos..main[fold_pos..].find('>').unwrap() + fold_pos + 1];
        assert!(
            !fold_tag.contains("open"),
            "fold must render closed: {fold_tag}"
        );
        assert!(main.contains("<b>2 more</b>"), "{main}");
        assert!(main.contains("older editions"), "{main}");

        // Escaped plain text, never raw markup.
        assert!(
            main.contains(
                "<b>Added</b> fifth &#60;script&#62;alert(1)&#60;/script&#62; &#38; note."
            ) || main
                .contains("<b>Added</b> fifth &lt;script&gt;alert(1)&lt;/script&gt; &amp; note."),
            "{main}"
        );
        assert!(!main.contains("<script>alert(1)</script>"));
        assert!(main.contains("<b>Changed</b> fifth change."), "{main}");
        assert!(main.contains("<b>Verified</b> fifth check."), "{main}");

        // Missing bullets are omitted from edition-004 (only Added).
        let ed4_slice = &main[pos4..pos3];
        assert!(ed4_slice.contains("<b>Added</b> fourth note only."));
        assert!(!ed4_slice.contains("<b>Changed</b>"));
        assert!(!ed4_slice.contains("<b>Verified</b>"));
    }

    #[test]
    fn page_without_previous_shows_empty_state() {
        let (_model, view) = load_view(fixture_tree());
        let site = site::render(&view, &BuildInfo::default()).unwrap();
        let html = text(&site, changes::OUTPUT_PATH);
        let main = main_slice(&html);

        assert!(
            html.contains("<title>Changes · Fieldbook</title>"),
            "{html}"
        );
        assert!(
            html.contains(&format!(
                "<meta http-equiv=\"Content-Security-Policy\" content=\"{CSP_STATIC}\">"
            )),
            "{html}"
        );
        assert!(!html.contains("style=\""), "{html}");
        assert!(
            html.contains(">Today</a>"),
            "Today crumb must link:\n{html}"
        );
        assert!(
            html.contains("aria-current=\"page\">Changes</span>"),
            "Changes crumb must be current:\n{html}"
        );
        assert!(main.contains("<h1>Changes</h1>"), "{main}");
        assert!(
            main.contains("What each edition added, changed and verified."),
            "{main}"
        );
        assert!(
            !main.contains("eyebrow"),
            "PageHeader has no eyebrow:\n{main}"
        );
        assert!(main.contains("Ids since an earlier edition"), "{main}");
        assert!(
            main.contains(
                "No earlier edition was named for this build, so the ids added and removed are not listed."
            ),
            "{main}"
        );
        assert!(!main.contains("<table"), "{main}");
    }

    #[test]
    fn page_with_previous_links_planned_ids() {
        let (_model, view) = load_view(fixture_tree());
        let ctx = site::render_context(&view, &BuildInfo::default()).unwrap();
        let current_ids = ctx.edition_info().ids.clone();

        // Construct an earlier edition that:
        // - lacks one current page, one source version, one claim, one question, one conflict, one term (so each is Added),
        // - holds one removed claim `CL-CMP-0499` whose part page `claims/CMP-1.html` IS planned,
        // - plus >20 removed public pages so the Fold cap (20 + "N more") is exercised.
        let added_page = current_ids.pages[0].clone();
        let added_version = current_ids.source_versions[0].clone();
        let added_claim = current_ids.claims[0].clone();
        let added_question = current_ids.questions[0].clone();
        let added_conflict = current_ids.conflicts[0].clone();
        let added_term = current_ids.terms[0].clone();

        let mut prev_ids = EditionIds {
            pages: current_ids.pages[1..].to_vec(),
            source_versions: current_ids.source_versions[1..].to_vec(),
            claims: current_ids.claims[1..].to_vec(),
            questions: current_ids.questions[1..].to_vec(),
            conflicts: current_ids.conflicts[1..].to_vec(),
            terms: current_ids.terms[1..].to_vec(),
        };
        prev_ids.claims.push("CL-CMP-0499".to_owned());
        for i in 1..=23 {
            prev_ids.pages.push(format!("cmp-removed-page-{i:02}"));
        }

        let full_planned = site::planned_paths(&view);
        assert!(
            full_planned.contains("claims/CMP-1.html"),
            "claims/CMP-1.html must be planned so removed CL-CMP-0499 tests the never-link rule"
        );
        let nav_model = nav::build(&view, &full_planned);
        let prev = sample_edition_info("edition-001", 1, prev_ids.clone());
        let html = changes::render_page(&view, &ctx, &nav_model, &full_planned, Some(&prev));
        let main = main_slice(&html);

        assert!(main.contains("Ids since edition-001"), "{main}");
        assert!(
            main.contains("<caption>Ids added and removed since edition-001</caption>"),
            "{main}"
        );
        assert!(
            main.contains(
                "This compares ids only. An item that kept its id may still have changed."
            ),
            "{main}"
        );
        assert!(
            !main.contains("removed id is not listed")
                && !main.contains("removed ids are not listed"),
            "no hidden ids means no not-listed sentence:\n{main}"
        );

        // Summary table lists all 6 rows in order with exact counts.
        for expected_row in [
            "<td>Pages</td><td class=\"mono\">1</td><td class=\"mono\">23</td>",
            "<td>Source versions</td><td class=\"mono\">1</td><td class=\"mono\">0</td>",
            "<td>Claims</td><td class=\"mono\">1</td><td class=\"mono\">1</td>",
            "<td>Questions</td><td class=\"mono\">1</td><td class=\"mono\">0</td>",
            "<td>Conflicts</td><td class=\"mono\">1</td><td class=\"mono\">0</td>",
            "<td>Terms</td><td class=\"mono\">1</td><td class=\"mono\">0</td>",
        ] {
            let normalized: String = main.lines().map(str::trim).collect();
            assert!(
                normalized.contains(expected_row),
                "missing summary row `{expected_row}` in:\n{main}"
            );
        }

        // Added IDs link when planned.
        assert!(
            main.contains(&format!("href=\"p/{added_page}.html\">{added_page}</a>")),
            "{main}"
        );
        assert!(
            main.contains(&format!(
                "href=\"s/{added_version}.html\">{added_version}</a>"
            )),
            "{main}"
        );
        assert!(
            main.contains(&format!(
                "href=\"claims/CMP-1.html#{added_claim}\">{added_claim}</a>"
            )),
            "{main}"
        );
        assert!(
            main.contains(&format!(
                "href=\"questions.html#{added_question}\">{added_question}</a>"
            )),
            "{main}"
        );
        assert!(
            main.contains(&format!(
                "href=\"conflicts.html#{added_conflict}\">{added_conflict}</a>"
            )),
            "{main}"
        );
        assert!(
            main.contains(&format!(
                "href=\"glossary.html#{added_term}\">{added_term}</a>"
            )),
            "{main}"
        );

        // Removed claim stays plain text even though `claims/CMP-1.html` IS planned!
        assert!(main.contains(">CL-CMP-0499</span>"), "{main}");
        assert!(
            !main.contains("#CL-CMP-0499"),
            "removed claim must never link even when its claims part page is planned:\n{main}"
        );

        // Group overflow cap: 23 removed pages -> first 20 shown, remaining 3 in closed Fold "3 more".
        assert_eq!(GROUP_IDS_SHOWN, 20);
        assert!(main.contains("<b>3 more</b>"), "{main}");

        // When planned paths are empty, added IDs degrade to plain text spans (no broken links).
        let empty_planned = PlannedPaths::of(&[]);
        let nav_empty = nav::build(&view, &empty_planned);
        let html_unplanned =
            changes::render_page(&view, &ctx, &nav_empty, &empty_planned, Some(&prev));
        let main_unplanned = main_slice(&html_unplanned);
        assert!(
            !main_unplanned.contains("<a "),
            "no target planned means zero links inside <main>:\n{main_unplanned}"
        );
        assert!(main_unplanned.contains(&format!(">{added_page}</span>")));
        assert!(main_unplanned.contains(&format!(">{added_claim}</span>")));

        // When previous edition was unreleased, heading and caption say "an unreleased build".
        let prev_unreleased = sample_edition_info("unreleased", 0, prev_ids);
        let html_unrel = changes::render_page(
            &view,
            &ctx,
            &nav_model,
            &full_planned,
            Some(&prev_unreleased),
        );
        let main_unrel = main_slice(&html_unrel);
        assert!(
            main_unrel.contains("Ids since an unreleased build"),
            "{main_unrel}"
        );
        assert!(
            main_unrel
                .contains("<caption>Ids added and removed since an unreleased build</caption>"),
            "{main_unrel}"
        );
    }
}
