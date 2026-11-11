//! Integration tests for `fieldbook_render::{charts, svg}` (`build-charts`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod charts {
    use std::path::Path;

    use fieldbook_calc::Value::Known;
    use fieldbook_calc::{Dec, Value, sub_dec};
    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::cases::{self, run};
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_render::charts::{
        WATERFALL_CASE, area_charts, format_amount, rate_build_up_panel, waterfall_bars,
    };
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::site::{Site, render};
    use fieldbook_render::svg::{CoverageCounts, coverage_bar, line_plot};

    const RTE_AREA: &str = "\n[[area]]\ncode = \"rte\"\nslug = \"rates\"\nname = \"Indirect rates\"\nsummary = \"Synthetic rates area for fixture tests\"\nsubareas = [\"rates\"]\n";

    fn insert_all(tree: &mut Tree, dir: &Path, rel: &str) {
        tree.insert_dir(rel);
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let child = format!("{rel}/{}", entry.file_name().to_string_lossy());
            if entry.file_type().unwrap().is_dir() {
                insert_all(tree, &entry.path(), &child);
            } else {
                tree.insert_bytes(child, std::fs::read(entry.path()).unwrap());
            }
        }
    }

    /// The `corpus-mini` fixture, with `edit` applied to `(corpus.toml, claims.csv)`.
    fn tree_with(edit: impl Fn(&str, &str) -> (String, String)) -> Tree {
        let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
        let corpus = base.join("corpus");
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
        tree_with(|toml, claims| (format!("{toml}{RTE_AREA}"), claims.to_owned()))
    }

    fn load_view(tree: Tree) -> (Model, LabelledView) {
        let today = CivilDate::parse("2026-03-01").unwrap();
        let (model, _report) = load(&Host::new(tree, today), &Options::new(false));
        let now = Now::new(today, 0).unwrap();
        let Decision::Allowed(allowed) = check(
            &EffectivePolicy::floor(false),
            &Dest::Edition,
            None,
            None,
            now,
            Shipped::Never,
        ) else {
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
        String::from_utf8(site.files().get(path).unwrap().clone()).unwrap()
    }

    /// The text of the `<section id="id">` of an area page, up to the next workspace section.
    fn section<'a>(html: &'a str, id: &str) -> &'a str {
        let m = format!("<section class=\"ws-section\" id=\"{id}\">");
        let start = html
            .find(&m)
            .unwrap_or_else(|| panic!("section {id} must exist"))
            + m.len();
        let rest = &html[start..];
        rest.find("<section class=\"ws-section\"")
            .map_or(rest, |e| &rest[..e])
    }

    fn dec(out: &fieldbook_calc::Outcome, name: &str) -> Dec {
        match out.outputs.iter().find(|o| o.name == name).unwrap().value {
            Known(d) => d,
            Value::Unknown(_) => panic!("expected known {name}"),
        }
    }

    #[test]
    fn waterfall_rows_are_differences_of_outcome_outputs() {
        let outcome = cases::run(WATERFALL_CASE).unwrap().unwrap();
        let (l, c, p) = (
            dec(&outcome, "landed"),
            dec(&outcome, "cost"),
            dec(&outcome, "price"),
        );

        let bars = waterfall_bars(&outcome).unwrap();
        assert_eq!(bars.len(), 3);
        assert_eq!(bars[0].output, "landed");
        assert_eq!(bars[1].output, "cost");
        assert_eq!(bars[2].output, "price");

        let sub = |a, b| sub_dec(Known(a), Known(b)).unwrap().known().unwrap();
        let exp = [(Dec::ZERO, l, l), (l, c, sub(c, l)), (c, p, sub(p, c))];
        for (b, (f, t, s)) in bars.iter().zip(exp) {
            let o = b.output;
            assert_eq!(b.from, f, "bar `{o}`: expected from {f}, found {}", b.from);
            assert_eq!(b.to, t, "bar `{o}`: expected to {t}, found {}", b.to);
            assert_eq!(b.step, s, "bar `{o}`: expected step {s}, found {}", b.step);
        }
        assert!(!bars[0].total && !bars[1].total && bars[2].total);
        assert!(bars.iter().all(|b| b.output != "unit"));
    }

    #[test]
    fn svg_has_role_label_and_no_text() {
        let rate = rate_build_up_panel();
        let est = area_charts("est");
        let rte = area_charts("rte");
        let pts = vec![("p1".into(), 10), ("p2".into(), 20), ("p3".into(), 15)];
        let line = line_plot(&pts, 0, 30, "sample plot");
        let counts = CoverageCounts {
            supported: 3,
            unchecked: 1,
            ..Default::default()
        };
        let cov = coverage_bar(&counts);

        for s in [&rate, &est, &rte, &line, &cov] {
            assert!(!s.contains("<text") && !s.contains("<script") && !s.contains(" style="));
        }
        assert!(line.contains("role=\"img\"") && line.contains("aria-label=\""));
        assert!(cov.contains("role=\"img\"") && cov.contains("aria-label=\""));

        let m = "<svg class=\"track\"";
        assert!(rate.contains(m));
        for chunk in rate.split(m).skip(1) {
            let tag = chunk.split('>').next().unwrap();
            assert!(
                tag.contains("aria-hidden=\"true\""),
                "track svg missing aria-hidden: {tag}"
            );
        }
        assert!(rate.contains("<svg") && rate.matches("bar1 tot").count() == 1);
    }

    #[test]
    fn marks_never_use_the_accent() {
        let css = include_str!("../assets/app.css");
        let targets = ".chart|.hb-|.hbars|svg.plot|svg.track|.bar1|svg .line|svg .dot|svg .ring|svg .grid|svg .hit";
        let mut count = 0;
        for rule in css.split('}') {
            if let Some((sel, _)) = rule.split_once('{')
                && targets.split('|').any(|t| sel.contains(t))
            {
                count += 1;
                assert!(
                    !rule.contains("--color-accent"),
                    "rule `{sel}` has --color-accent"
                );
            }
        }
        assert!(
            count >= 8,
            "expected at least 8 matching rules, found {count}"
        );
        assert!(!area_charts("rte").contains("accent"));
    }

    #[test]
    fn est_shows_unavailable() {
        let est = area_charts("est");
        assert_eq!(est.matches("class=\"unavailable\"").count(), 2);
        assert!(!est.contains("<svg class=\"plot\"") && !est.contains("<svg class=\"track\""));
        assert!(!est.contains("bar1"));
        assert!(est.contains("The escalation index is not available until"));
        assert!(est.contains("Bid history is not available until Release 2."));
        assert_eq!(area_charts("EST"), est);
    }

    #[test]
    fn rte_has_one_waterfall_and_unavailable_history() {
        let rte = area_charts("rte");
        assert!(
            rte.contains("Price build-up") && rte.matches("class=\"unavailable\"").count() == 1
        );
        assert!(rte.contains("Rate history is not available until Release 2."));
        for tag in ["<details class=\"fold\">", "<caption>", "<table>"] {
            assert!(rte.contains(tag));
        }
        assert!(rte.contains("Source: CALC-02"));

        let m = "As of <time datetime=\"";
        let date = &rte[rte.find(m).unwrap() + m.len()..][..10];
        assert!(date.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        }));
        assert_eq!(area_charts("cmp"), "");
    }

    #[test]
    fn chart_figures_come_from_the_outcome() {
        let outcome = run(WATERFALL_CASE).unwrap().unwrap();
        let panel = rate_build_up_panel();
        assert!(panel.contains(&format_amount(dec(&outcome, "price"))));
        assert!(panel.contains(&format_amount(dec(&outcome, "unit"))));
        assert_eq!(
            format_amount(Dec::parse("1234567.80").unwrap()),
            "1,234,567.80"
        );
        assert_eq!(format_amount(Dec::parse("999.99").unwrap()), "999.99");
    }

    #[test]
    fn negative_amount_in_parentheses() {
        assert_eq!(format_amount(Dec::parse("-12.50").unwrap()), "(12.50)");
        assert_eq!(
            format_amount(Dec::parse("-1234567.80").unwrap()),
            "(1,234,567.80)"
        );
        assert_eq!(format_amount(Dec::parse("12.50").unwrap()), "12.50");
        assert_eq!(format_amount(Dec::ZERO), "0");
    }

    /// The value cell text of the output row `name` in a CalcTable, if the row exists.
    fn calc_cell<'a>(html: &'a str, name: &str) -> Option<&'a str> {
        let row = format!("<td>{name}</td>");
        let rest = &html[html.find(&row)? + row.len()..];
        let rest = &rest[rest.find("<td class=\"num")?..];
        let rest = &rest[rest.find('>')? + 1..];
        Some(&rest[..rest.find("</td>")?])
    }

    #[test]
    fn calc_table_shows_fixture_result() {
        let site = built(fixture_tree());
        let html = page(&site, "p/est-calc-01.html");
        let outcome = cases::run("CALC-01").unwrap().unwrap();
        assert!(!outcome.outputs.is_empty());
        let mut rows = 0;
        for out in &outcome.outputs {
            let Known(d) = out.value else {
                panic!("CALC-01 output `{}` must be known", out.name)
            };
            let expected = format_amount(d);
            let found = calc_cell(&html, &out.name).unwrap_or("(no row)");
            assert_eq!(
                found, expected,
                "output `{}`: expected {expected}, found {found}",
                out.name
            );
            rows += 1;
        }
        assert_eq!(html.matches("<td class=\"num").count(), rows);
        assert!(html.contains("Computed by the calc core, not a model"));
        assert!(html.contains(fieldbook_core::strings::rounding()));
        assert!(html.contains("Fixture CALC-01: pass"));
        assert!(!html.contains("class=\"num neg\""));
    }

    #[test]
    fn calc_table_is_unavailable_for_a_refused_case() {
        let tree = {
            let mut tree = fixture_tree();
            let path = "corpus/pages/estimating/pricing/est-calc-01.md";
            let md = String::from_utf8(tree.get_bytes(path).unwrap().to_vec()).unwrap();
            tree.insert_bytes(path, md.replace("CALC-01", "CALC-90").into_bytes());
            tree
        };
        let html = page(&built(tree), "p/est-calc-01.html");
        assert!(html.contains("class=\"unavailable\"") && html.contains("The calculation table"));
        assert!(!html.contains("class=\"panel calc\""));
    }

    #[test]
    fn area_overview_holds_the_charts() {
        let site = built(fixture_tree());
        let (p_rte, p_est, p_cmp) = (
            page(&site, "a/rte.html"),
            page(&site, "a/est.html"),
            page(&site, "a/cmp.html"),
        );
        let rte = section(&p_rte, "overview");
        assert!(rte.contains("class=\"chart-row\"") && rte.contains("Price build-up"));
        let est = section(&p_est, "overview");
        assert!(est.contains("class=\"chart-row\"") && est.contains("Escalation index"));
        assert!(!section(&p_cmp, "overview").contains("chart-row"));

        for (path, bytes) in site.files() {
            if path.starts_with("a/") && path.ends_with(".html") {
                let html = std::str::from_utf8(bytes).unwrap();
                for bad in ["<text", "<script>", " style=\""] {
                    assert!(!html.contains(bad), "{path} has {bad}");
                }
            }
        }
    }
}
