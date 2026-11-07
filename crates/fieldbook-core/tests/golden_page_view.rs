//! Golden vector tests for the page view above `query`
//! (`tests/golden/search/page-view.json`, `docs/reference/search.md#the-page-view`).

mod golden_page_view {
    use std::collections::BTreeSet;

    use fieldbook_core::search::{
        Glossary, GoldenUnit, Index, Unit, UnitFields, ViewLimits, index_tokens, lookup_unit_kind,
        page_view, query,
    };
    use serde::Deserialize;

    const PAGE_VIEW_JSON: &str = include_str!("../../../tests/golden/search/page-view.json");

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PageViewFile {
        kind: String,
        version: u32,
        units: Vec<GoldenUnit>,
        #[serde(default)]
        glossary: Glossary,
        cases: Vec<PageViewCase>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct PageViewCase {
        id: String,
        query: String,
        /// The flat ranked list the view starts from.
        hits: Vec<String>,
        exact: Vec<String>,
        pages: Vec<ExpectedPage>,
        other: Vec<String>,
        absorbed: usize,
        shown_pages: usize,
        shown_other: usize,
        /// Free text for readers; the loader ignores it.
        #[serde(default, rename = "note")]
        _note: Option<String>,
    }

    #[derive(Debug, PartialEq, Eq, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ExpectedPage {
        page: String,
        has_lead: bool,
        kind: String,
        title: String,
        parent: String,
        status: String,
        as_of: String,
        effective_from: String,
        verified_on: String,
        best_unit: String,
        snippet: String,
        matched_units: usize,
    }

    fn parse(raw: &str) -> Result<PageViewFile, String> {
        let file: PageViewFile =
            serde_json::from_str(raw).map_err(|e| format!("JSON parse error: {e}"))?;
        if file.kind != "page-view" {
            return Err(format!("unknown golden kind {:?}", file.kind));
        }
        if file.version != 1 {
            return Err(format!("unsupported golden version {}", file.version));
        }
        for u in &file.units {
            if lookup_unit_kind(&u.kind).is_none() {
                return Err(format!("unit {:?} has unknown kind {:?}", u.id, u.kind));
            }
        }
        Ok(file)
    }

    fn file() -> PageViewFile {
        parse(PAGE_VIEW_JSON).unwrap_or_else(|e| panic!("page-view.json: {e}"))
    }

    fn run_case(index: &Index, case: &PageViewCase) -> Vec<String> {
        let results = query(index, &case.query, index.docs.len());
        let view = page_view(index, &results);
        let shown = view.shown(ViewLimits::CLI);
        let mut problems = Vec::new();

        let hits: Vec<&str> = results.hits.iter().map(|h| h.id.as_str()).collect();
        if hits != case.hits {
            problems.push(format!("hits: expected {:?}, got {hits:?}", case.hits));
        }
        let exact: Vec<&str> = view.exact.iter().map(|h| h.id.as_str()).collect();
        if exact != case.exact {
            problems.push(format!("exact: expected {:?}, got {exact:?}", case.exact));
        }
        let pages: Vec<ExpectedPage> = view
            .pages
            .iter()
            .map(|row| ExpectedPage {
                page: row.page.to_owned(),
                has_lead: row.has_lead,
                kind: row.display.kind.clone(),
                title: row.display.title.clone(),
                parent: row.display.parent.clone(),
                status: row.display.status.clone(),
                as_of: row.display.as_of.clone(),
                effective_from: row.display.effective_from.clone(),
                verified_on: row.display.verified_on.clone(),
                best_unit: row.best.id.clone(),
                snippet: row.best_doc.snippet.clone(),
                matched_units: row.matched_units,
            })
            .collect();
        if pages != case.pages {
            problems.push(format!("pages: expected {:?}, got {pages:?}", case.pages));
        }
        let other: Vec<&str> = view.other.iter().map(|h| h.id.as_str()).collect();
        if other != case.other {
            problems.push(format!("other: expected {:?}, got {other:?}", case.other));
        }
        if view.absorbed != case.absorbed {
            problems.push(format!(
                "absorbed: expected {}, got {}",
                case.absorbed, view.absorbed
            ));
        }

        // Every exact row is shown, and the limits only ever take the head of a list.
        if shown.exact.len() != view.exact.len() {
            problems.push(format!(
                "shown exact: expected {}, got {}",
                view.exact.len(),
                shown.exact.len()
            ));
        }
        if shown.pages.len() != case.shown_pages {
            problems.push(format!(
                "shown pages: expected {}, got {}",
                case.shown_pages,
                shown.pages.len()
            ));
        }
        if shown.other.len() != case.shown_other {
            problems.push(format!(
                "shown other: expected {}, got {}",
                case.shown_other,
                shown.other.len()
            ));
        }

        // Nothing is dropped from the totals.
        let accounted = view.exact.len()
            + view.pages.iter().map(|p| p.matched_units).sum::<usize>()
            + view.absorbed
            + view.other.len();
        if accounted != results.total || view.total != results.total {
            problems.push(format!(
                "accounting: {accounted} units in the view, view total {}, {} matched",
                view.total, results.total
            ));
        }
        problems
    }

    #[test]
    fn all_page_view_golden_cases_pass() {
        let file = file();
        let index = Index::from_golden(&file.units, &file.glossary);
        let mut seen = BTreeSet::new();
        let mut failures = Vec::new();
        for case in &file.cases {
            if !seen.insert(case.id.clone()) {
                failures.push(format!("{}: duplicate case id", case.id));
            }
            let problems = run_case(&index, case);
            if !problems.is_empty() {
                failures.push(format!(
                    "{} `{}`: {}",
                    case.id,
                    case.query,
                    problems.join("; ")
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "page-view golden failures ({}):\n{}",
            failures.len(),
            failures.join("\n")
        );
        assert!(
            file.cases.len() >= 8,
            "expected at least 8 page-view cases, found {}",
            file.cases.len()
        );
    }

    #[test]
    fn loader_rejects_an_unknown_kind_version_and_key() {
        let valid = serde_json::json!({
            "kind": "page-view", "version": 1, "units": [],
            "cases": [{
                "id": "pv-x", "query": "q", "hits": [], "exact": [], "pages": [], "other": [],
                "absorbed": 0, "shown_pages": 0, "shown_other": 0, "note": "readers only"
            }]
        });
        parse(&valid.to_string()).expect("the control file loads");

        for (pointer, key, value) in [
            ("/kind", "", serde_json::json!("ranked")),
            ("/version", "", serde_json::json!(2)),
            ("", "planted_key", serde_json::json!(1)),
            ("/cases/0", "planted_key", serde_json::json!(1)),
        ] {
            let mut planted = valid.clone();
            if key.is_empty() {
                *planted.pointer_mut(pointer).expect("field") = value;
            } else {
                planted
                    .pointer_mut(pointer)
                    .and_then(serde_json::Value::as_object_mut)
                    .expect("object")
                    .insert(key.to_owned(), value);
            }
            assert!(parse(&planted.to_string()).is_err(), "{pointer} {key}");
        }
    }

    fn unit(id: &str, kind: &str, body: &str) -> Unit {
        Unit::new(
            id,
            kind,
            UnitFields {
                id: Vec::new(),
                title: Vec::new(),
                headings: Vec::new(),
                claim: Vec::new(),
                body: index_tokens(body),
            },
        )
    }

    #[test]
    fn a_kind_the_view_has_never_heard_of_is_an_other_row() {
        // A later kind must fall into the other rows rather than vanish, even with a `#` in its
        // id: only the kinds `page` and `clause` group.
        let index = Index::build(
            vec![
                unit("later-kind-one", "later", "ledger"),
                unit("later-kind#two", "later", "ledger"),
                unit("a-page", "page", "ledger"),
            ],
            &Glossary::default(),
        );
        let results = query(&index, "ledger", index.docs.len());
        let view = page_view(&index, &results);
        let other: Vec<&str> = view.other.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(other, ["later-kind#two", "later-kind-one"]);
        assert_eq!(view.pages.len(), 1);
        assert_eq!(view.pages[0].page, "a-page");
    }

    #[test]
    fn a_tighter_limit_takes_the_head_of_each_list() {
        let file = file();
        let index = Index::from_golden(&file.units, &file.glossary);
        let results = query(&index, "dingo", index.docs.len());
        let view = page_view(&index, &results);
        let shown = view.shown(ViewLimits {
            pages: 2,
            other: 1,
            other_alone: 4,
        });
        let pages: Vec<&str> = shown.pages.iter().map(|p| p.page).collect();
        assert_eq!(pages, ["pg-01", "pg-02"]);
        let other: Vec<&str> = shown.other.iter().map(|h| h.id.as_str()).collect();
        assert_eq!(other, ["CL-DNG-0001"]);
        assert!(view.is_cut_by(&shown));

        // A limit at or above the list size cuts nothing.
        let all = view.shown(ViewLimits {
            pages: 12,
            other: 7,
            other_alone: 7,
        });
        assert!(!view.is_cut_by(&all));
    }

    #[test]
    fn the_view_is_the_same_whichever_order_the_units_were_given_in() {
        let file = file();
        let mut reversed = file.units.clone();
        reversed.reverse();
        let forward = Index::from_golden(&file.units, &file.glossary);
        let backward = Index::from_golden(&reversed, &file.glossary);
        for case in &file.cases {
            let problems = run_case(&backward, case);
            assert!(problems.is_empty(), "{}: {problems:?}", case.id);
            assert!(run_case(&forward, case).is_empty(), "{}", case.id);
        }
    }
}
