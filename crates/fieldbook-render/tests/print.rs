//! Integration tests for the day-one binder and print contract.

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod print {
    use std::collections::BTreeSet;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::strings;
    use fieldbook_render::day_one;
    use fieldbook_render::frame::PRODUCT_TITLE;
    use fieldbook_render::meta::{BuildInfo, BuildProfile};
    use fieldbook_render::site;

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
    const S_0001_V1_TXT: &[u8] =
        include_bytes!("../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt");

    const CMP_SYSTEM_REVIEW_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
    );
    const CMP_REVIEW_STEPS_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
    );
    const CMP_ONBOARDING_CHECKLIST_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md"
    );
    const CMP_WITHHOLD_CASE_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-withhold-case.md"
    );
    const CMP_CLAUSE_9900_101_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-clause-9900-101.md"
    );
    const EST_CALC_01_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-calc-01.md"
    );
    const EST_RATE_WINDOW_BRIEF_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
    );
    const EST_RATE_REFERENCE_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-reference.md"
    );
    const EST_BASELINE_TEMPLATE_MD: &str = include_str!(
        "../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-baseline-template.md"
    );

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
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);

        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-system-review.md",
            CMP_SYSTEM_REVIEW_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            CMP_REVIEW_STEPS_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-onboarding-checklist.md",
            CMP_ONBOARDING_CHECKLIST_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-withhold-case.md",
            CMP_WITHHOLD_CASE_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-clause-9900-101.md",
            CMP_CLAUSE_9900_101_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-calc-01.md",
            EST_CALC_01_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-window-brief.md",
            EST_RATE_WINDOW_BRIEF_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-rate-reference.md",
            EST_RATE_REFERENCE_MD.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-baseline-template.md",
            EST_BASELINE_TEMPLATE_MD.as_bytes(),
        );
        tree
    }

    fn stage3_only_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/eval");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");

        tree.insert_bytes("corpus/corpus.toml", CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", CLAIMS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/questions.csv", QUESTIONS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/conflicts.csv", CONFLICTS_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/glossary.csv", GLOSSARY_CSV.as_bytes());
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", S_0002_V2_TXT);
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", S_0001_V1_TXT);

        let steps_md = CMP_REVIEW_STEPS_MD
            .replace("prereq = [\"cmp-system-review\"]\n", "")
            .replace("[review concept](cmp-system-review.md)", "review concept");
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            steps_md.into_bytes(),
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

    fn fixture_view() -> (Model, LabelledView) {
        load_view(fixture_tree())
    }

    #[test]
    fn day_one_lists_briefs_then_stages() {
        let (_model, view) = fixture_view();
        let binder_html = day_one::render(&view, &BuildInfo::default()).unwrap();

        let page_ids: Vec<&str> = binder_html
            .split("data-page-id=\"")
            .skip(1)
            .filter_map(|s| s.split('"').next())
            .collect();

        let expected_ids: Vec<&str> = day_one::binder_pages(&view)
            .iter()
            .map(|p| p.id.as_str())
            .collect();

        assert_eq!(page_ids, expected_ids);
        assert_eq!(page_ids[0], "est-rate-window-brief");
        assert_eq!(page_ids[1], "cmp-onboarding-checklist");
        assert!(!page_ids.contains(&"cmp-review-steps"));
        assert!(!page_ids.contains(&"est-calc-01"));

        let brief_marker = "data-page-id=\"est-rate-window-brief\"";
        let brief_idx = binder_html.find(brief_marker).expect("brief in binder");
        let article_start = binder_html[..brief_idx]
            .rfind("<article")
            .expect("<article before brief data-page-id");
        let tag_close = binder_html[brief_idx..]
            .find('>')
            .expect("> after brief data-page-id")
            + brief_idx;
        let brief_tag = &binder_html[article_start..=tag_close];
        assert!(
            brief_tag.contains("sheet"),
            "brief article tag must carry class `sheet`: {brief_tag}"
        );

        let (_m_empty, view_empty) = load_view(stage3_only_tree());
        let empty_html = day_one::render(&view_empty, &BuildInfo::default()).unwrap();
        assert!(
            empty_html.contains("None recorded"),
            "empty binder must show `None recorded`"
        );
        assert!(
            !empty_html.contains("<article"),
            "empty binder must not contain <article"
        );
    }

    #[test]
    fn one_page_rule_in_print_block() {
        let css = include_str!("../assets/app.css");
        assert_eq!(css.matches("@page").count(), 1);
        let page_rule = "@page { size: Letter; margin: 0; }";
        assert!(css.contains(page_rule));

        let last_print_idx = css
            .rfind("@media print {")
            .expect("last @media print block");
        let page_rule_idx = css.find(page_rule).expect("@page rule index");
        assert!(
            page_rule_idx > last_print_idx,
            "@page rule must sit inside the last @media print block"
        );

        let final_block = &css[last_print_idx..];
        assert!(final_block.contains("break-before: page"));
        assert!(final_block.contains(".cite-back"));
        assert!(final_block.contains(".sidebar"));
        assert!(!final_block.contains("file:"));

        assert!(!css.contains("file:"));
    }

    fn final_print_rules() -> &'static str {
        include_str!("../assets/app.css")
            .rsplit_once("@media print {")
            .unwrap_or_else(|| panic!("last @media print block"))
            .1
    }

    #[test]
    fn brief_one_sheet_rules_keep_status_words_and_locators() {
        let statement_selector = ".sheet .source-body:has(.claim-mark) .source-stmt";
        let mut hidden_statement = false;
        for (selectors, declarations) in final_print_rules()
            .split('}')
            .filter_map(|rule| rule.split_once('{'))
        {
            let hidden = declarations.split(';').any(|declaration| {
                declaration
                    .split_once(':')
                    .is_some_and(|(property, value)| {
                        property.trim() == "display"
                            && value
                                .trim()
                                .split('!')
                                .next()
                                .is_some_and(|v| v.trim() == "none")
                    })
            });
            if !hidden {
                continue;
            }
            for selector in selectors.split(',') {
                let selector = selector
                    .rsplit_once("*/")
                    .map_or(selector, |(_, selector)| selector)
                    .trim();
                match selector.split_whitespace().last() {
                    Some(".source-chips" | ".chip" | ".cite" | ".loc" | ".claim-mark") => {
                        panic!("print must keep the status words and locators: {selector}");
                    }
                    Some(".source-stmt") => {
                        assert_eq!(selector, statement_selector);
                        hidden_statement = true;
                    }
                    _ => {}
                }
            }
        }
        assert!(
            hidden_statement,
            "brief footnotes omit repeated claim statements"
        );
    }

    #[test]
    fn one_sheet_rules_are_under_sheet() {
        let (_, rules) = final_print_rules()
            .split_once("/* Compact brief sheets and their footnotes. */")
            .expect("brief sheet print rules");
        let (rules, _) = rules
            .split_once("\n  .source-row {")
            .expect("ordinary print source rows follow the brief rules");
        for (selectors, declarations) in rules.split('}').filter_map(|rule| rule.split_once('{')) {
            for selector in selectors.split(',').map(str::trim) {
                assert!(
                    selector == ".sheet"
                        || selector.starts_with(".sheet ")
                        || selector == ".binder-head",
                    "brief print rule affects other pages: {selector}"
                );
            }
            for token in
                declarations.split(|c: char| !c.is_ascii_alphanumeric() && c != '.' && c != '-')
            {
                for unit in ["px", "rem"] {
                    assert!(
                        !token
                            .strip_suffix(unit)
                            .is_some_and(|number| number.parse::<f32>().is_ok()),
                        "brief print lengths must use tokens: {token}"
                    );
                }
            }
        }
    }

    #[test]
    fn footer_has_edition_and_page_id() {
        let (_model, view) = fixture_view();
        let binder_html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let site = site::render(&view, &BuildInfo::default()).unwrap();

        let articles: Vec<&str> = binder_html
            .split("<article class=\"binder-page")
            .skip(1)
            .collect();
        assert!(!articles.is_empty(), "binder must contain articles");

        for article in &articles {
            let id_marker = "data-page-id=\"";
            let id_start =
                article.find(id_marker).expect("data-page-id in article") + id_marker.len();
            let id_end = article[id_start..]
                .find('"')
                .expect("closing quote for data-page-id")
                + id_start;
            let page_id = &article[id_start..id_end];

            let foot_marker = "<td class=\"print-foot\">";
            let foot_start =
                article.find(foot_marker).expect("print-foot in article") + foot_marker.len();
            let foot_end = article[foot_start..]
                .find("</td>")
                .expect("closing td for print-foot")
                + foot_start;
            let foot = &article[foot_start..foot_end];

            let p_idx = foot
                .find(PRODUCT_TITLE)
                .expect("footer contains PRODUCT_TITLE");
            let u_idx = foot
                .find(strings::unreleased_build())
                .expect("footer contains unreleased_build");
            let id_idx = foot.find(page_id).expect("footer contains page id");
            let c_idx = foot
                .find(strings::print_check())
                .expect("footer contains print_check");

            assert!(p_idx < u_idx, "PRODUCT_TITLE before unreleased_build");
            assert!(u_idx < id_idx, "unreleased_build before page_id");
            assert!(id_idx < c_idx, "page_id before print_check");
        }

        let css = include_str!("../assets/app.css");
        let last_media_print = css
            .rfind("@media print {")
            .expect("last @media print block");
        let final_block = &css[last_media_print..];
        assert!(
            final_block.contains(strings::print_check()),
            "final print block must hold print_check text"
        );
        assert!(
            final_block.contains("attr(data-route)"),
            "final print block must hold attr(data-route) expression"
        );
        assert!(
            final_block.contains("attr(data-page-id)"),
            "final print block must hold attr(data-page-id) expression"
        );

        let brief_page_bytes = site
            .files()
            .get("p/est-rate-window-brief.html")
            .expect("p/est-rate-window-brief.html in site files");
        let brief_page = std::str::from_utf8(brief_page_bytes).unwrap();
        assert!(
            brief_page.contains("data-route=\"p/est-rate-window-brief.html\""),
            "brief page must contain data-route attribute"
        );
        assert!(
            brief_page.contains("data-page-id=\"est-rate-window-brief\""),
            "brief page must contain data-page-id attribute"
        );
    }

    /// The opening `<div class="app" ...>` tag of a rendered page.
    fn frame_tag(html: &str) -> &str {
        let start = html
            .find("<div class=\"app\"")
            .unwrap_or_else(|| panic!("frame div.app in page"));
        let end = html[start..]
            .find('>')
            .unwrap_or_else(|| panic!("closing > of div.app in page"))
            + start;
        &html[start..=end]
    }

    /// The value of `name` in one start tag, if the tag carries it.
    fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
        let marker = format!("{name}=\"");
        let start = tag.find(&marker)? + marker.len();
        let end = tag[start..].find('"')? + start;
        Some(&tag[start..end])
    }

    #[test]
    fn ordinary_page_footer_is_the_bare_id() {
        let (_model, view) = fixture_view();
        let site = site::render(&view, &BuildInfo::default()).unwrap();

        let css = include_str!("../assets/app.css");
        let last_media_print = css
            .rfind("@media print {")
            .expect("last @media print block");
        let final_block = &css[last_media_print..];
        let route_rule = final_block
            .find(".app::after")
            .expect("route footer rule in the final print block");
        let page_id_rule = final_block
            .find(".app[data-page-id]::after")
            .unwrap_or_else(|| {
                panic!("a reader page must print its page id, not its route: no page id rule")
            });
        assert!(
            page_id_rule > route_rule,
            "the page id rule must follow the route rule so it wins"
        );

        for (path, expected_id) in [
            ("p/est-rate-window-brief.html", "est-rate-window-brief"),
            ("p/cmp-system-review.html", "cmp-system-review"),
        ] {
            let bytes = site
                .files()
                .get(path)
                .unwrap_or_else(|| panic!("{path} in site files"));
            let tag = frame_tag(std::str::from_utf8(bytes).unwrap());
            let page_id = attr_value(tag, "data-page-id")
                .unwrap_or_else(|| panic!("{path} frame must carry data-page-id: {tag}"));
            assert_eq!(page_id, expected_id, "{path} must print its own page id");

            let printed = format!("{page_id}\u{a0}\u{b7}\u{a0}{}", strings::print_check());
            for forbidden in ["/", ".html", "file:"] {
                assert!(
                    !printed.contains(forbidden),
                    "{path} footer text must not hold `{forbidden}`: {printed}"
                );
            }
        }

        for path in ["index.html", "ids.html"] {
            let bytes = site
                .files()
                .get(path)
                .unwrap_or_else(|| panic!("{path} in site files"));
            let tag = frame_tag(std::str::from_utf8(bytes).unwrap());
            assert!(
                !tag.contains("data-page-id"),
                "{path} is not a reader page and must print its route: {tag}"
            );
        }
    }

    #[test]
    fn every_page_has_the_csp_meta_and_no_style_attribute() {
        let (_model, view) = fixture_view();
        let site = site::render(&view, &BuildInfo::default()).unwrap();

        assert!(
            site.files().contains_key(day_one::OUTPUT_PATH),
            "day-one.html must be present in site files"
        );
        assert!(
            site.files().contains_key("index.html"),
            "index.html must be present in site files"
        );

        let mut html_count = 0;
        for (path, bytes) in site.files() {
            if path.ends_with(".html") {
                html_count += 1;
                let text = std::str::from_utf8(bytes).unwrap();
                assert!(
                    text.contains("http-equiv=\"Content-Security-Policy\""),
                    "{path} must contain CSP meta tag"
                );
                assert!(
                    !text.contains(" style=\""),
                    "{path} must not contain inline style attribute"
                );

                for (idx, _) in text.match_indices("<script") {
                    let remainder = &text[idx..];
                    let tag_end = remainder
                        .find('>')
                        .unwrap_or_else(|| panic!("{path} has unclosed <script tag"));
                    let script_tag = &remainder[..tag_end];
                    assert!(
                        script_tag.contains("src="),
                        "{path} has inline script without src attribute: {script_tag}"
                    );
                }

                for event_handler in [
                    " onclick=",
                    " onload=",
                    " onerror=",
                    " onmouseover=",
                    " onfocus=",
                ] {
                    assert!(
                        !text.contains(event_handler),
                        "{path} must not contain inline event handler `{event_handler}`"
                    );
                }
            }
        }
        assert!(html_count > 0, "must have checked at least one html file");
    }

    // control: WB-05
    #[test]
    fn no_file_url_in_built_pages() {
        let (_model, view) = fixture_view();
        let site = site::render(&view, &BuildInfo::default()).unwrap();
        let needle = "file:";

        for (path, bytes) in site.files() {
            if path.ends_with(".html") {
                let text = std::str::from_utf8(bytes).unwrap();
                assert!(!text.contains(needle), "{path} must not contain `{needle}`");
            }
        }

        let app_css_site = site
            .files()
            .get("assets/app.css")
            .expect("assets/app.css in site files");
        let app_css_site_text = std::str::from_utf8(app_css_site).unwrap();
        assert!(
            !app_css_site_text.contains(needle),
            "site assets/app.css must not contain `{needle}`"
        );

        let app_css_inc = include_str!("../assets/app.css");
        assert!(
            !app_css_inc.contains(needle),
            "included app.css must not contain `{needle}`"
        );
    }

    fn article<'a>(html: &'a str, id: &str) -> &'a str {
        let marker = format!("data-page-id=\"{id}\"");
        let at = html.find(&marker).unwrap();
        let start = html[..at].rfind("<article").unwrap();
        let end = html[at..].find("</article>").unwrap() + at;
        &html[start..end]
    }

    fn headings(article: &str) -> Vec<&str> {
        article
            .split("<h3>")
            .skip(1)
            .filter_map(|s| s.split("</h3>").next())
            .collect()
    }

    fn h3(heading: &str) -> String {
        format!("<h3>{heading}</h3>")
    }

    fn the_rule(article: &str) -> &str {
        let at = article.find(&h3(strings::parts()[1])).unwrap();
        let end = article.find(&h3(strings::parts()[2])).unwrap();
        &article[at..end]
    }

    #[test]
    fn binder_articles_carry_the_whole_page() {
        let (_model, view) = fixture_view();
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let a = article(&html, "cmp-system-review");

        assert_eq!(headings(a), strings::parts());
        let calib = a.find(&h3(strings::parts()[5])).unwrap();
        let sources = a.find(&h3(strings::parts()[6])).unwrap();
        assert!(
            a[calib..sources].contains("Q-001"),
            "calibration cites Q-001"
        );
        assert!(a.contains("<span>Draft</span>"), "status word");
        let site = site::render(&view, &BuildInfo::default()).unwrap();
        let page_html = std::str::from_utf8(
            site.files()
                .get("p/cmp-system-review.html")
                .expect("p/cmp-system-review.html in site files"),
        )
        .unwrap();
        let review_by = page_html
            .split("Review by <time datetime=\"")
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("review-by date on the standalone page");
        assert!(
            a.contains(&format!(
                "Review by <time datetime=\"{review_by}\">{review_by}</time>"
            )),
            "binder article prints the review-by date {review_by} the page shows"
        );
        assert!(a.contains("SYNTHETIC-NOW."), "readiness notice");
        let never = a.find(&h3(strings::parts()[4])).unwrap();
        assert!(
            a[never..calib].contains("XC-01"),
            "conflict box sits under Never confuse"
        );
        let change = a.find(&h3(strings::parts()[7])).unwrap();
        assert!(sources < change);
        assert!(
            a[change..].contains("NEW_KNOWLEDGE") && a[change..].contains("Added the synthetic"),
            "change note body"
        );
    }

    #[test]
    fn brief_binder_article_keeps_six_parts_and_status() {
        let (_model, view) = fixture_view();
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let a = article(&html, "est-rate-window-brief");
        let mut expected: Vec<&str> = strings::brief_parts().to_vec();
        expected.push(strings::parts()[6]);
        assert_eq!(headings(a), expected);
        assert!(a.contains("<span>Draft</span>"));
        assert!(!a.contains(&h3(strings::parts()[7])));
    }

    #[test]
    fn reference_rule_renders_as_dated_table() {
        let (_model, view) = fixture_view();
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let rule = the_rule(article(&html, "est-rate-reference"));
        assert!(rule.contains("<table>"), "reference rule is a table");
        assert!(rule.contains("class=\"ref-as-of\""), "row dates");
        assert!(rule.contains("data-as-of=\"2026-02-01\""));
    }

    #[test]
    fn reference_rule_ids_and_links_are_prefixed() {
        let (_model, view) = fixture_view();
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let rule = the_rule(article(&html, "est-rate-reference"));
        assert!(rule.contains("id=\"b-est-rate-reference-cite-ref-1-1\""));
        assert!(rule.contains("href=\"#b-est-rate-reference-cite-1\""));
        assert!(!rule.contains(" id=\"cite-"));
        assert!(!rule.contains("href=\"#cite-"));
    }

    #[test]
    fn checklist_binder_article_has_numbered_steps_and_timeline() {
        let (_model, view) = fixture_view();
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let a = article(&html, "cmp-onboarding-checklist");

        let at = a.find(&h3(strings::parts()[2])).unwrap();
        let end = a.find(&h3(strings::parts()[3])).unwrap();
        let slice = &a[at..end];

        assert!(slice.contains("<ol class=\"steps\">"));
        assert!(!slice.contains("<ul>"));
        assert!(a.contains("class=\"panel plan-panel\""));
        assert!(a.contains("Day 1 to Day 30"));
        assert!(a.contains("Day 31 to Day 60"));

        let panel_idx = slice
            .find("class=\"panel plan-panel\"")
            .expect("timeline panel in How it works slice");
        let steps_idx = slice
            .find("<ol class=\"steps\">")
            .expect("steps list in How it works slice");
        assert!(
            panel_idx < steps_idx,
            "timeline panel appears before the first <ol class=\"steps\"> in How it works"
        );

        let panel_start = slice[..panel_idx].rfind('<').unwrap();
        let panel_end = slice[panel_idx..].find('>').unwrap() + panel_idx;
        let panel_tag = &slice[panel_start..=panel_end];
        assert!(
            panel_tag
                .contains("aria-labelledby=\"b-cmp-onboarding-checklist-fb:plan-timeline-head\"")
        );

        assert!(a.contains("id=\"b-cmp-onboarding-checklist-fb:plan-timeline-head\""));
        assert!(!a.contains(" id=\"fb:plan-timeline-head\""));
        assert!(!a.contains("aria-labelledby=\"fb:plan-timeline-head\""));
    }

    #[test]
    fn released_binder_articles_carry_the_hidden_overdue_banner() {
        let mut tree = fixture_tree();
        tree.insert_bytes(
            "corpus/changes.md",
            b"# Changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: initial.\n",
        );
        let (_model, view) = load_view(tree);
        let date = CivilDate::parse("2026-03-01").unwrap();
        let sha = "0123456789abcdef0123456789abcdef01234567".to_owned();
        let released_info = BuildInfo::new(Some(date), Some(sha), BuildProfile::Silent, None);

        let html = day_one::render(&view, &released_info).unwrap();
        let site = site::render(&view, &released_info).unwrap();

        for id in ["cmp-system-review", "est-rate-window-brief"] {
            let a = article(&html, id);
            let banner_id_attr = format!("id=\"b-{id}-fb:overdueBanner\"");
            assert!(a.contains(&banner_id_attr));
            assert!(a.contains("Review overdue."));
            assert!(a.contains("data-review-by=\""));

            let banner_id_pos = a.find(&banner_id_attr).unwrap();
            let div_start = a[..banner_id_pos].rfind("<div").unwrap();
            let gt_pos = a[banner_id_pos..].find('>').unwrap() + banner_id_pos;
            let banner_tag = &a[div_start..=gt_pos];
            assert!(banner_tag.contains(" hidden"));

            let drb_marker = "data-review-by=\"";
            let drb_start = banner_tag.find(drb_marker).unwrap() + drb_marker.len();
            let drb_end = banner_tag[drb_start..].find('"').unwrap() + drb_start;
            let banner_review_by = &banner_tag[drb_start..drb_end];

            let page_path = format!("p/{id}.html");
            let page_bytes = site
                .files()
                .get(&page_path)
                .unwrap_or_else(|| panic!("{page_path} not found in site files"));
            let page_html = std::str::from_utf8(page_bytes).unwrap();
            let time_marker = "Review by <time datetime=\"";
            let time_start = page_html.find(time_marker).unwrap() + time_marker.len();
            let time_end = page_html[time_start..].find('"').unwrap() + time_start;
            let page_review_by = &page_html[time_start..time_end];

            assert_eq!(banner_review_by, page_review_by);
            assert_eq!(a.contains("due-claims"), page_html.contains("due-claims"));
        }

        let ids: Vec<&str> = html
            .split(" id=\"")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap())
            .collect();
        assert!(!ids.is_empty(), "binder html must contain element ids");
        let mut seen_ids = BTreeSet::new();
        for id in &ids {
            assert!(
                seen_ids.insert(*id),
                "duplicate id found in binder html: {id}"
            );
        }

        let href_targets: Vec<&str> = html
            .split("href=\"#")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap())
            .collect();
        assert!(
            !href_targets.is_empty(),
            "binder html must contain fragment links"
        );
        for target in href_targets {
            assert!(
                seen_ids.contains(target),
                "fragment link target #{target} does not match any element id in binder"
            );
        }

        for rest in html.split("aria-labelledby=\"").skip(1) {
            let target = rest.split('"').next().unwrap();
            assert!(
                seen_ids.contains(target),
                "aria-labelledby target {target} does not match any element id in binder"
            );
        }
    }

    /// The fixture tree with the page at `path` moved to stage 1, so the binder lists it.
    fn tree_with_stage_one(path: &str, stage: u8) -> Tree {
        let mut tree = fixture_tree();
        let page_md = match path {
            "corpus/pages/estimating/pricing/est-calc-01.md" => EST_CALC_01_MD,
            "corpus/pages/compliance/business-systems/cmp-withhold-case.md" => CMP_WITHHOLD_CASE_MD,
            "corpus/pages/compliance/business-systems/cmp-clause-9900-101.md" => {
                CMP_CLAUSE_9900_101_MD
            }
            other => panic!("no fixture page at {other}"),
        };
        let from = format!("\nstage = {stage}\n");
        assert!(page_md.contains(&from), "{path} holds `stage = {stage}`");
        tree.insert_bytes(
            path,
            page_md.replacen(&from, "\nstage = 1\n", 1).into_bytes(),
        );
        tree
    }

    fn site_page(view: &LabelledView, id: &str) -> String {
        let site = site::render(view, &BuildInfo::default()).unwrap();
        let path = format!("p/{id}.html");
        let bytes = site
            .files()
            .get(&path)
            .unwrap_or_else(|| panic!("{path} in site files"));
        std::str::from_utf8(bytes).unwrap().to_owned()
    }

    /// The element that starts at the first `open` in `html`, through its matching `</tag>`.
    fn element<'a>(html: &'a str, open: &str, tag: &str) -> Option<&'a str> {
        let start = html.find(open)?;
        let open_tag = format!("<{tag}");
        let close_tag = format!("</{tag}>");
        let mut depth = 0usize;
        let mut at = start;
        loop {
            let next_open = html[at..].find(&open_tag).map(|i| i + at);
            let next_close = html[at..].find(&close_tag).map(|i| i + at)?;
            match next_open {
                Some(o) if o < next_close => {
                    depth += 1;
                    at = o + open_tag.len();
                }
                _ => {
                    depth -= 1;
                    at = next_close + close_tag.len();
                    if depth == 0 {
                        return Some(&html[start..at]);
                    }
                }
            }
        }
    }

    /// `html` with every `href="..."` value blanked, since the binder and `p/` differ in depth.
    fn without_hrefs(html: &str) -> String {
        let mut out = String::new();
        let mut rest = html;
        while let Some(i) = rest.find("href=\"") {
            out.push_str(&rest[..i + 6]);
            rest = &rest[i + 6..];
            let end = rest.find('"').unwrap();
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    fn assert_unique_ids(html: &str) {
        let mut seen = BTreeSet::new();
        for rest in html.split(" id=\"").skip(1) {
            let id = rest.split('"').next().unwrap();
            assert!(seen.insert(id), "duplicate id in binder html: {id}");
        }
        for rest in html.split("href=\"#").skip(1) {
            let target = rest.split('"').next().unwrap();
            assert!(
                seen.contains(target),
                "fragment link #{target} has no element in the binder"
            );
        }
    }

    #[test]
    fn binder_calc_article_holds_the_calc_table() {
        let (_model, view) = load_view(tree_with_stage_one(
            "corpus/pages/estimating/pricing/est-calc-01.md",
            3,
        ));
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let page = site_page(&view, "est-calc-01");
        let open = "<figure class=\"panel calc\">";
        let on_page =
            element(&page, open, "figure").expect("p/est-calc-01.html holds the calc table");

        let a = article(&html, "est-calc-01");
        let at = a.find(&h3(strings::parts()[3])).unwrap();
        let end = a.find(&h3(strings::parts()[4])).unwrap();
        let worked = &a[at..end];
        let in_binder = element(worked, open, "figure")
            .unwrap_or_else(|| panic!("binder Worked example holds no calc table:\n{worked}"));
        assert_eq!(in_binder.replace("b-est-calc-01-", ""), on_page);

        let caption = on_page
            .split("<caption>")
            .nth(1)
            .and_then(|s| s.split("</caption>").next())
            .unwrap();
        assert!(!caption.is_empty());
        assert!(
            in_binder.contains(caption),
            "binder table holds caption {caption}"
        );
        assert_eq!(headings(a), strings::parts());
        assert_unique_ids(&html);
    }

    #[test]
    fn binder_case_article_holds_the_split() {
        let (_model, view) = load_view(tree_with_stage_one(
            "corpus/pages/compliance/business-systems/cmp-withhold-case.md",
            4,
        ));
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let page = site_page(&view, "cmp-withhold-case");
        assert!(
            page.contains("<div class=\"case-split\">"),
            "standalone split"
        );
        let a = article(&html, "cmp-withhold-case");

        assert_eq!(headings(a), strings::parts());
        let at = a.find(&h3(strings::parts()[2])).unwrap();
        let end = a.find(&h3(strings::parts()[3])).unwrap();
        let how = &a[at..end];
        assert!(
            how.contains("<div class=\"case-split\">"),
            "binder split:\n{how}"
        );
        assert!(how.contains("case-block case-record"));
        assert!(how.contains("case-block case-inference"));

        for heading in ["Public record", "Inference"] {
            let tail = format!("\">{heading}</h3>");
            let page_anchor = page
                .split("<h3 id=\"")
                .skip(1)
                .find(|s| {
                    s.split_once('"')
                        .is_some_and(|(_, r)| r.starts_with(&tail[1..]))
                })
                .and_then(|s| s.split('"').next())
                .unwrap_or_else(|| panic!("standalone {heading} subhead"));
            let binder_head =
                format!("<h4 id=\"b-cmp-withhold-case-{page_anchor}\">{heading}</h4>");
            assert!(how.contains(&binder_head), "binder holds {binder_head}");
        }
        assert!(how.find("case-record") < how.find("case-inference"));
        assert_unique_ids(&html);
    }

    #[test]
    fn binder_clause_article_holds_the_questions() {
        let (_model, view) = load_view(tree_with_stage_one(
            "corpus/pages/compliance/business-systems/cmp-clause-9900-101.md",
            5,
        ));
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        let page = site_page(&view, "cmp-clause-9900-101");
        let open = "<div class=\"chips clause-questions\"";
        let on_page = element(&page, open, "div").expect("standalone clause questions");
        assert!(on_page.contains("Q-001"), "standalone chips name Q-001");

        let a = article(&html, "cmp-clause-9900-101");
        assert_eq!(headings(a), strings::parts());
        let calib = a.find(&h3(strings::parts()[5])).unwrap();
        let sources = a.find(&h3(strings::parts()[6])).unwrap();
        let in_binder = element(&a[calib..sources], open, "div")
            .unwrap_or_else(|| panic!("binder Company calibration holds no clause questions"));
        assert_eq!(
            without_hrefs(&in_binder.replace("b-cmp-clause-9900-101-", "")),
            without_hrefs(on_page)
        );

        let rule = the_rule(a);
        assert!(
            rule.contains("<div class=\"clause-card-rule\">"),
            "binder rule keeps its wrapper:\n{rule}"
        );
        assert!(page.contains("<div class=\"clause-card-rule\">"));
        assert_unique_ids(&html);
    }

    #[test]
    fn unreleased_binder_has_no_review_banner() {
        let (_model, view) = fixture_view();
        let html = day_one::render(&view, &BuildInfo::default()).unwrap();
        assert!(!html.contains("data-review-by"));
        assert!(!html.contains("Review overdue"));
        assert!(!html.contains("fb:overdueBanner"));
    }
}
