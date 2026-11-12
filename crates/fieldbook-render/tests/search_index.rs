//! Integration tests for `fieldbook_render::search_index` (`search-site-index`).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod search_index {
    use std::path::Path;

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::labels::{LabelId, LabelSet, PUBLIC};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::search::{Index, Unit, UnitFields, labels_of, make_snippet, utf16_cmp};
    use fieldbook_render::meta::BuildInfo;
    use fieldbook_render::search_index::{self, OUTPUT_PATH};
    use fieldbook_render::site::{Site, planned_paths, render};

    const PREFIX: &str = "window.FB_INDEX = ";

    /// A page whose own handling carries `np-0007`, which the edition selection leaves out.
    const NONPUBLIC_PAGE: &str = "+++\nid = \"est-nonpublic-canary\"\ntitle = \"Nonpublic pricing canary\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nNonpublic zetanonpubliccanary text.\n\n## The rule\n\nSecret.\n\n## How it works\n\nSecret.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSecret.\n\n## Never confuse\n\nSecret.\n\n## Company calibration\n\nSecret.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-03-01: Secret.\n";

    /// The held version's canaries, in its title and its note.
    const HELD_CANARIES: [&str; 2] = ["heldtitlecanary", "heldnotecanary"];

    /// A long two-sentence Purpose for `cmp-system-review`, longer than 160 code points.
    const LONG_PURPOSE: &str = "Explain how a synthetic system review interacts with a synthetic billing withhold across every synthetic billing cycle of the year. A second synthetic sentence follows so the paragraph runs past the snippet limit.";

    /// A `Never confuse` first paragraph ending that quotes two remote URLs.
    const URL_SENTENCE: &str = "with a final contract closeout audit, listed at https://example.org/closeout and HTTP://example.org/x.";

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

    fn fixture_corpus() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini/corpus")
    }

    /// One `sources.csv` row of `header` with `cells` and every other column empty.
    fn csv_row(header: &str, cells: &[(&str, &str)]) -> String {
        header
            .split(',')
            .map(|col| {
                let value = cells
                    .iter()
                    .find(|(c, _)| *c == col)
                    .map_or("", |(_, v)| *v);
                if value.contains(',') {
                    format!("\"{value}\"")
                } else {
                    value.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// `corpus-mini` with a nonpublic page, a held (`quarantined`) source version carrying
    /// canaries, a long two-sentence Purpose and a few non-ASCII words.
    fn fixture_tree() -> Tree {
        let corpus = fixture_corpus();
        let mut tree = Tree::new();
        insert_all(&mut tree, &corpus, "corpus");

        tree.insert_bytes(
            "corpus/pages/estimating/pricing/est-nonpublic-canary.md",
            NONPUBLIC_PAGE.as_bytes().to_vec(),
        );

        let sources = std::fs::read_to_string(corpus.join("registers/sources.csv")).unwrap();
        let header = sources.lines().next().unwrap().to_owned();
        let held = csv_row(
            &header,
            &[
                ("source_id", "S-0005"),
                ("version", "1"),
                ("tag", "synthetic-held"),
                (
                    "title",
                    "Synthetic held source heldtitlecanary, made up for tests",
                ),
                ("publisher", "Synthetic Board"),
                ("source_kind", "primary_law"),
                ("source_role", "primary_evidence"),
                ("authority_tier", "A"),
                ("licence", "Synthetic, no restrictions"),
                ("location", "metadata-only: made up for tests"),
                ("sha256", &"5".repeat(64)),
                ("as_of", "2026-02-01"),
                ("retrieved", "2026-02-01"),
                ("lifecycle", "quarantined"),
                ("volatility", "V2"),
                ("handling", "public"),
                ("note", "Held for test heldnotecanary."),
                ("purpose", "Synthetic purpose for tests"),
            ],
        );
        tree.insert_bytes(
            "corpus/registers/sources.csv",
            format!("{}\n{held}\n", sources.trim_end_matches('\n')).into_bytes(),
        );

        let review_path = "corpus/pages/compliance/business-systems/cmp-system-review.md";
        let review = std::fs::read_to_string(corpus.join(&review_path["corpus/".len()..]))
            .unwrap()
            .replace(
                "Explain how a synthetic system review interacts with a synthetic billing withhold.",
                LONG_PURPOSE,
            )
            .replace(
                "with a final contract closeout audit.",
                URL_SENTENCE,
            )
            .replace(
                "Synthetic reviewers check control operation",
                "Synthetic reviewers \u{fa0e} \u{10000} check control operation",
            );
        tree.insert_bytes(review_path, review.into_bytes());
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

    fn built() -> (Model, LabelledView, Site) {
        let (model, view) = load_view(fixture_tree());
        let site = render(&view, &BuildInfo::default()).unwrap();
        (model, view, site)
    }

    fn delivered(site: &Site) -> String {
        String::from_utf8(
            site.files()
                .get(OUTPUT_PATH)
                .unwrap_or_else(|| panic!("{OUTPUT_PATH} must be built"))
                .clone(),
        )
        .unwrap()
    }

    fn parsed(text: &str) -> serde_json::Value {
        let json = text
            .strip_prefix(PREFIX)
            .and_then(|s| s.strip_suffix(';'))
            .unwrap_or_else(|| panic!("the index must be `{PREFIX}<json>;`"));
        serde_json::from_str(json).unwrap()
    }

    fn doc_ids(index: &serde_json::Value) -> Vec<String> {
        index["docs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d[0].as_str().unwrap().to_owned())
            .collect()
    }

    fn unit_text(u: &Unit) -> String {
        let mut text = format!(
            "{} {} {} {}",
            u.id, u.display.title, u.display.parent, u.display.snippet
        );
        for field in u.fields.as_array() {
            for tok in field {
                text.push(' ');
                text.push_str(tok);
            }
        }
        text
    }

    #[test]
    fn edition_index_holds_public_units_only() {
        let (model, view, site) = built();
        assert!(
            model
                .pages()
                .iter()
                .any(|p| p.id.as_str() == "est-nonpublic-canary"),
            "the model holds the nonpublic page the view must leave out"
        );

        // The unit list the builder returns, before the kind filter.
        let (units, _glossary) = search_index::view_units(&view);
        assert!(units.iter().any(|u| u.id == "cmp-system-review"));
        for u in &units {
            assert!(labels_of(u).is_public(), "unit {} is not public", u.id);
            assert!(!u.id.starts_with("est-nonpublic-canary"), "unit {}", u.id);
            assert!(
                !unit_text(u).contains("zetanonpubliccanary"),
                "unit {}",
                u.id
            );
        }

        // A nonpublic page unit offered to the edition selection is left out.
        let np7 = LabelId::parse("np-0007").unwrap();
        let planted = Unit::new("est-planted", "page", UnitFields::default())
            .with_labels(LabelSet::from_ids([PUBLIC, np7]));
        let public = Unit::new("est-kept", "page", UnitFields::default());
        let kept =
            search_index::edition_units(vec![planted, public.with_labels(LabelSet::public())]);
        let kept_ids: Vec<&str> = kept.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(
            kept_ids,
            ["est-kept"],
            "the edition keeps public units only"
        );

        // The delivered index: permitted pages present, the nonpublic page and every label id absent.
        let text = delivered(&site);
        let index = parsed(&text);
        let ids = doc_ids(&index);
        assert!(ids.iter().any(|id| id == "cmp-system-review"));
        assert!(ids.iter().any(|id| id == "est-rate-window-brief"));
        for needle in [
            "est-nonpublic-canary",
            "Nonpublic pricing canary",
            "zetanonpubliccanary",
            "np-0007",
        ] {
            assert!(!text.contains(needle), "the index holds `{needle}`");
        }
    }

    #[test]
    fn held_version_not_in_index() {
        let (_model, view, site) = built();
        assert!(
            view.sources()
                .iter()
                .any(|s| s.value.source_id.as_str() == "S-0005"),
            "the view keeps the held row, so only the hold rule keeps it out"
        );

        // Both checks run before either fails, so a failure names every stage that let the
        // held version through.
        let mut leaks: Vec<String> = Vec::new();

        // Before the kind filter: a held version has no unit and no canary.
        let (units, _glossary) = search_index::view_units(&view);
        assert!(
            units.iter().any(|u| u.id == "S-0002-v2"),
            "an active version has a unit before the kind filter"
        );
        for u in &units {
            if u.id == "S-0005-v1" {
                leaks.push("unit list: the held version S-0005-v1 has a unit".to_owned());
            }
            for canary in HELD_CANARIES {
                if unit_text(u).contains(canary) {
                    leaks.push(format!(
                        "unit list: held canary `{canary}` in unit {}",
                        u.id
                    ));
                }
            }
        }

        // The delivered index holds neither the id nor the canaries.
        let text = delivered(&site);
        let index = parsed(&text);
        if doc_ids(&index).iter().any(|id| id.starts_with("S-0005")) {
            leaks.push("delivered index: a unit of the held version S-0005".to_owned());
        }
        for needle in ["S-0005", "s-0005"].into_iter().chain(HELD_CANARIES) {
            if text.contains(needle) {
                leaks.push(format!("delivered index: holds `{needle}`"));
            }
        }
        assert!(
            leaks.is_empty(),
            "the held version leaked:\n{}",
            leaks.join("\n")
        );
    }

    #[test]
    fn the_edition_index_holds_no_claim_units() {
        let (_model, view, site) = built();
        let text = delivered(&site);
        let index = parsed(&text);
        let docs = index["docs"].as_array().unwrap();
        let kinds: std::collections::BTreeSet<&str> =
            docs.iter().map(|d| d[1].as_str().unwrap()).collect();
        assert_eq!(
            kinds.into_iter().collect::<Vec<_>>(),
            ["clause", "page", "term"],
            "every kind in the delivered docs is `page`, `clause` or `term`, and all three occur"
        );

        // One record of each other kind: a unit before the filter, absent after it, and still
        // reachable through its register page and its id list page.
        let (units, _glossary) = search_index::view_units(&view);
        let ids = doc_ids(&index);
        for (id, kind, register, id_list_page) in [
            (
                "CL-CMP-0001",
                "claim",
                "claims/CMP-1.html",
                "ids/claims-CMP-1.html",
            ),
            ("S-0002-v2", "source", "sources.html", "ids/versions.html"),
            ("Q-001", "question", "questions.html", "ids/questions.html"),
            ("XC-01", "conflict", "conflicts.html", "ids/conflicts.html"),
        ] {
            assert!(
                units.iter().any(|u| u.id == id && u.kind == kind),
                "{id} is a {kind} unit before the kind filter"
            );
            assert!(
                !ids.iter().any(|d| d == id),
                "{id} must not be in the index"
            );
            let reg_html = String::from_utf8(site.files()[register].clone()).unwrap();
            assert!(
                reg_html.contains(&format!("<tr id=\"{id}\"")),
                "{id} must have its row in {register}"
            );
            let id_page_html = String::from_utf8(site.files()[id_list_page].clone()).unwrap();
            assert!(
                id_page_html.contains(&format!("data-id=\"{id}\"")),
                "{id} must be listed in {id_list_page}"
            );
        }
        // The register text of those records is not in the index either. (A page that cites a
        // claim keeps the citation in its text, so a claim id can still be a term.)
        for needle in [
            "Synthetic owner-verified fact for fixture tests.",
            "Synthetic fixture second version superseding S-0002-v1.",
            "Which synthetic business-system control owner approves the monthly compliance review?",
            "Synthetic review interval versus withholding check timing",
        ] {
            assert!(!text.contains(needle), "the index holds `{needle}`");
        }

        // N is that of the page, section and term units alone.
        let edition_units = units
            .iter()
            .filter(|u| u.kind == "page" || u.kind == "clause" || u.kind == "term")
            .count();
        assert_eq!(docs.len(), edition_units);
        for list in index["post"].as_array().unwrap() {
            for pair in list.as_array().unwrap().chunks(2) {
                assert!(pair[0].as_u64().unwrap() < docs.len() as u64);
            }
        }
    }

    #[test]
    fn the_edition_index_holds_term_units_with_aliases() {
        let (_model, _view, site) = built();
        let index = parsed(&delivered(&site));
        let docs = index["docs"].as_array().unwrap();

        let (bw_pos, bw) = docs
            .iter()
            .enumerate()
            .find(|(_i, d)| d[0] == "billing_withhold")
            .unwrap_or_else(|| panic!("billing_withhold not found in docs"));
        let bw_unit = u64::try_from(bw_pos).unwrap();

        assert_eq!(bw[1].as_str().unwrap(), "term");
        assert_eq!(bw[2].as_str().unwrap(), "billing withhold");
        assert_eq!(bw[3].as_str().unwrap(), "withholding holdback");
        assert_eq!(
            bw[4].as_str().unwrap(),
            "Synthetic retention of a percentage of interim payments pending system correction."
        );
        assert_eq!(bw[5].as_str().unwrap(), "public_reference");
        for i in 6..=9 {
            assert_eq!(bw[i].as_str().unwrap(), "", "field {i} must be empty");
        }

        for other in ["cost_baseline", "rate_window", "system_review"] {
            let doc = docs
                .iter()
                .find(|d| d[0] == other)
                .unwrap_or_else(|| panic!("missing {other}"));
            assert_eq!(doc[1].as_str().unwrap(), "term");
            assert_eq!(doc[3].as_str().unwrap(), "");
        }

        let terms: Vec<&str> = index["terms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        let posts = index["post"].as_array().unwrap();
        for tok in ["holdback", "billing"] {
            let term_idx = terms
                .iter()
                .position(|t| *t == tok)
                .unwrap_or_else(|| panic!("token `{tok}` not found in index terms"));
            let post = posts[term_idx].as_array().unwrap();
            let contains_bw = post
                .chunks(2)
                .any(|pair| pair[0].as_u64().unwrap() == bw_unit);
            assert!(
                contains_bw,
                "posting list for token `{tok}` must contain unit number {bw_unit}"
            );
        }

        let ids = doc_ids(&index);
        assert_eq!(
            ids.iter().filter(|id| *id == "billing_withhold").count(),
            1,
            "billing_withhold appears as a docs id exactly once"
        );
        for term_id in ["cost_baseline", "rate_window", "system_review"] {
            assert_eq!(
                ids.iter().filter(|id| *id == term_id).count(),
                1,
                "{term_id} appears as a docs id exactly once"
            );
        }
    }

    #[test]
    fn the_edition_index_holds_no_nonpublic_terms() {
        let np7 = LabelId::parse("np-0007").unwrap();
        let planted = Unit::new("t_planted", "term", UnitFields::default())
            .with_labels(LabelSet::from_ids([PUBLIC, np7]));
        let kept =
            Unit::new("t_kept", "term", UnitFields::default()).with_labels(LabelSet::public());
        let filtered = search_index::edition_units(vec![planted, kept]);
        let kept_ids: Vec<&str> = filtered.iter().map(|u| u.id.as_str()).collect();
        assert_eq!(kept_ids, ["t_kept"], "the edition keeps public terms only");
    }

    #[test]
    fn index_has_its_prefix_version_and_key_order() {
        let (_model, _view, site) = built();
        let text = delivered(&site);
        assert!(
            text.starts_with("window.FB_INDEX = {\"v\":1,\"docs\":[["),
            "{}",
            &text[..60]
        );
        assert!(text.ends_with("]]};"));
        let terms_at = text.find("],\"terms\":[").unwrap();
        let post_at = text.find("],\"post\":[").unwrap();
        assert!(terms_at < post_at);
        assert!(
            !text.contains("\"v\": ")
                && !text.contains("\"docs\": ")
                && !text.contains("\"terms\": "),
            "compact JSON"
        );
        let index = parsed(&text);
        assert_eq!(index["v"], 1);
        let keys: Vec<&String> = index.as_object().unwrap().keys().collect();
        assert_eq!(keys.len(), 4);
        for doc in index["docs"].as_array().unwrap() {
            assert_eq!(doc.as_array().unwrap().len(), 10);
            assert!(
                doc.as_array()
                    .unwrap()
                    .iter()
                    .all(serde_json::Value::is_string)
            );
        }
        assert_eq!(
            index["terms"].as_array().unwrap().len(),
            index["post"].as_array().unwrap().len()
        );
    }

    #[test]
    fn snippets_hold_at_most_160_code_points() {
        let (_model, _view, site) = built();
        let index = parsed(&delivered(&site));
        for doc in index["docs"].as_array().unwrap() {
            let snippet = doc[4].as_str().unwrap();
            assert!(snippet.chars().count() <= 160, "{snippet}");
        }
        // The page lead's snippet is the first paragraph of Purpose, cut under the rule.
        let lead = index["docs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d[0] == "cmp-system-review")
            .unwrap();
        let snippet = lead[4].as_str().unwrap();
        assert_eq!(snippet, make_snippet(LONG_PURPOSE));
        assert!(snippet.ends_with('\u{2026}'), "{snippet}");
        assert!(snippet.chars().count() <= 160);
    }

    #[test]
    fn terms_are_in_utf16_order() {
        let (_model, _view, site) = built();
        let index = parsed(&delivered(&site));
        let terms: Vec<&str> = index["terms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap())
            .collect();
        for pair in terms.windows(2) {
            assert_eq!(
                utf16_cmp(pair[0], pair[1]),
                std::cmp::Ordering::Less,
                "{} before {}",
                pair[0],
                pair[1]
            );
        }
        // U+10000 is a surrogate pair starting 0xD800, so it sorts before U+FA0E.
        let astral = terms.iter().position(|t| *t == "\u{10000}");
        let compat = terms.iter().position(|t| *t == "\u{fa0e}");
        assert!(astral.is_some() && compat.is_some(), "both words are terms");
        assert!(astral < compat);
    }

    #[test]
    fn equal_views_give_equal_bytes() {
        let (_m1, _v1, first) = built();
        let (_m2, _v2, second) = built();
        assert_eq!(delivered(&first), delivered(&second));
    }

    #[test]
    fn the_index_path_is_planned() {
        let (_model, view, site) = built();
        assert!(planned_paths(&view).contains(OUTPUT_PATH));
        assert!(site.files().contains_key(OUTPUT_PATH));
    }

    /// The impact of every (unit id, term) posting of a parsed delivered index.
    fn delivered_impacts(
        index: &serde_json::Value,
    ) -> std::collections::BTreeMap<(String, String), u64> {
        let ids = doc_ids(index);
        let mut out = std::collections::BTreeMap::new();
        let terms = index["terms"].as_array().unwrap();
        for (term, list) in terms.iter().zip(index["post"].as_array().unwrap()) {
            let list = list.as_array().unwrap();
            assert!(!list.is_empty(), "term {term} has an empty posting list");
            for pair in list.chunks(2) {
                let unit = usize::try_from(pair[0].as_u64().unwrap()).unwrap();
                out.insert(
                    (ids[unit].clone(), term.as_str().unwrap().to_owned()),
                    pair[1].as_u64().unwrap(),
                );
            }
        }
        out
    }

    fn index_impacts(index: &Index) -> std::collections::BTreeMap<(String, String), u64> {
        let mut out = std::collections::BTreeMap::new();
        for (term, list) in index.terms.iter().zip(&index.post) {
            for pair in list.chunks(2) {
                out.insert(
                    (index.docs[pair[0] as usize].id.clone(), term.clone()),
                    u64::from(pair[1]),
                );
            }
        }
        out
    }

    #[test]
    fn statistics_come_from_the_edition_units_alone() {
        let (_model, view, site) = built();
        let delivered = delivered_impacts(&parsed(&delivered(&site)));
        let (units, glossary) = search_index::view_units(&view);
        let edition = Index::build(search_index::edition_units(units.clone()), &glossary);
        let full = Index::build(units, &glossary);

        // Every delivered term and impact is that of the index over the page, section and term units.
        assert_eq!(delivered, index_impacts(&edition));

        // The full index gives other impacts for the same postings, so the check above tells
        // the two apart: a full index filtered afterwards would fail it.
        let full = index_impacts(&full);
        let differ = delivered
            .iter()
            .filter(|(key, impact)| full.get(*key).is_some_and(|f| f != *impact))
            .count();
        assert!(differ > 0, "the full index must give other impacts");
    }

    #[test]
    fn delivered_index_holds_no_remote_url() {
        let (_model, view, site) = built();
        let (units, _glossary) = search_index::view_units(&view);
        let section = units
            .iter()
            .find(|u| u.id == "cmp-system-review#never-confuse")
            .unwrap();
        assert!(
            section.display.snippet.contains("https://"),
            "the fixture section quotes a URL: {}",
            section.display.snippet
        );

        let text = delivered(&site);
        assert!(!text.contains("http://"), "the index holds `http://`");
        assert!(!text.contains("https://"), "the index holds `https://`");
        let lower = text.to_ascii_lowercase();
        assert!(!lower.contains("http://") && !lower.contains("https://"));

        // The snippet still reads as the paragraph, with the address kept and its scheme removed.
        let index = parsed(&text);
        let doc = index["docs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d[0] == "cmp-system-review#never-confuse")
            .unwrap();
        let snippet = doc[4].as_str().unwrap();
        assert_eq!(
            snippet,
            search_index::strip_url_schemes(&section.display.snippet)
        );
        assert!(
            snippet.contains("closeout audit, listed at example.org/closeout and"),
            "{snippet}"
        );
    }
}
