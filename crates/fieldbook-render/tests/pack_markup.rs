//! The page context pack markup (`page-context-packs` step 5).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod pack_markup {
    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::page::render_main;
    use fieldbook_render::site::PlannedPaths;

    const CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const ASSISTANT_MD: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const CHANGES_MD: &str = include_str!("../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const SOURCES_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const CLAIMS_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const QUESTIONS_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const CONFLICTS_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const GLOSSARY_CSV: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
    const S_0002_V2_TXT: &[u8] =
        include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt");
    const CMP_SYSTEM_REVIEW_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const CMP_REVIEW_STEPS_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const EST_CALC_01_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-calc-01.md"
    );
    const EST_RATE_WINDOW_BRIEF_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );

    const SNAPSHOT_PLANNED: &[&str] = &[
        "p/cmp-review-steps.html",
        "p/cmp-system-review.html",
        "p/est-calc-01.html",
        "p/est-rate-window-brief.html",
    ];

    fn fixture_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/eval");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating");
        tree.insert_dir("corpus/pages/estimating/pricing");

        tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", CLAIMS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            CMP_SYSTEM_REVIEW_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            CMP_REVIEW_STEPS_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-calc-01.md",
            EST_CALC_01_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-window-brief.md",
            EST_RATE_WINDOW_BRIEF_MD.as_bytes(),
        );
        tree
    }

    fn load_view(tree: Tree) -> (Model, LabelledView) {
        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, report) = load(&Host::new(tree, today), &Options::new(false));
        assert!(
            report.is_ok(),
            "fixture tree must load cleanly:\n{}",
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
        (model, view)
    }

    /// The quote of `CL-CMP-0001` as the fixture writes it, and a hostile one.
    const QUOTE: &str = "Synthetic compliance reviews occur every 30 days.";
    const HOSTILE_QUOTE: &str =
        "Synthetic <script>x</script> & compliance reviews occur every 30 days.";

    const PACK_OPEN: &str = "<details class=\"context-pack\">";

    /// The page `cmp-system-review` rendered over a fixture whose claim quote is hostile.
    fn rendered() -> String {
        let mut tree = fixture_tree();
        let claims = CLAIMS_CSV.replace(QUOTE, HOSTILE_QUOTE);
        assert_ne!(claims, CLAIMS_CSV, "the fixture quote is replaced");
        tree.insert_bytes("corpus/registers/claims.csv", claims.into_bytes());
        let (_model, view) = load_view(tree);
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        render_main(page, &view, &planned)
    }

    /// The pack part of `html`: from its `<details>` to its `</details>`.
    fn pack_part(html: &str) -> &str {
        let start = html.find(PACK_OPEN).expect("the page has a context pack");
        let rest = &html[start..];
        let end = rest.find("</details>").expect("the pack part is closed") + "</details>".len();
        &rest[..end]
    }

    // askama's escaper writes numeric references (`&#60;` for `<`, `&#38;` for `&`), which a
    // browser reads exactly as `&lt;` and `&amp;`.
    #[test]
    fn pack_markup_is_escaped() {
        let html = rendered();
        let part = pack_part(&html);
        let pre = part
            .split_once("<pre>")
            .and_then(|(_, rest)| rest.split_once("</pre>"))
            .map(|(text, _)| text)
            .expect("the pack has a <pre>");

        assert!(
            pre.contains("&#60;script&#62;x&#60;/script&#62;"),
            "the quote's markup is escaped in the pack:\n{pre}"
        );
        assert!(
            pre.contains(" &#38; "),
            "an ampersand is escaped in the pack:\n{pre}"
        );
        assert!(!pre.contains('<'), "no raw `<` inside the <pre>:\n{pre}");
        assert!(
            !html.contains("<script"),
            "the page holds no `<script` from the quote"
        );
    }

    #[test]
    fn pack_has_no_script() {
        let html = fixture_html();
        let part = pack_part(&html);

        assert!(part.contains("<summary>context pack</summary>"));
        assert!(!part.contains("<script"), "no script in the pack part");
        assert!(
            !part.contains("style="),
            "no style attribute in the pack part"
        );
        // Every `<` of the part opens a tag (the text is escaped), so check each tag's attributes.
        let mut rest = part;
        while let Some(open) = rest.find('<') {
            let close = rest[open..].find('>').expect("a tag is closed") + open;
            let tag = &rest[open + 1..close];
            for attribute in tag.split_whitespace().skip(1) {
                let name = attribute.split('=').next().unwrap();
                assert!(
                    !name.starts_with("on"),
                    "an event handler attribute `{name}` in the pack part"
                );
            }
            rest = &rest[close + 1..];
        }
    }

    #[test]
    fn the_pack_is_the_last_child_of_the_article() {
        let html = fixture_html();
        let part = pack_part(&html);
        let after = &html[html.find(part).unwrap() + part.len()..];
        assert!(
            after.trim_start().starts_with("</article>"),
            "the pack part closes the article:\n{after}"
        );
    }

    fn fixture_html() -> String {
        let (_model, view) = load_view(fixture_tree());
        let planned = PlannedPaths::of(SNAPSHOT_PLANNED);
        let page = view
            .pages()
            .iter()
            .find(|p| p.id.as_str() == "cmp-system-review")
            .unwrap();
        render_main(page, &view, &planned)
    }
}
