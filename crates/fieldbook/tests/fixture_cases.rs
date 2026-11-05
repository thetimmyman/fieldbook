//! Verify that every `Fixture:` line in every `calc` page of `tests/fixtures/corpus-mini`
//! (and `corpus/` when it holds a `calc` page) names an id in `fieldbook_core::cases::ids()`.

mod fixture_cases {
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    use fieldbook::io;
    use fieldbook_core::cases;
    use fieldbook_core::check::{Entry, Tree};
    use fieldbook_core::schema::{PageType, parse_meta, split};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn verify_calc_page_fixtures(tree: &Tree) -> Result<usize, String> {
        let valid_ids: BTreeSet<String> = cases::ids().into_iter().collect();
        let mut calc_pages = 0usize;
        let mut failures = Vec::new();

        for (path, entry) in tree.iter() {
            if !path.starts_with("corpus/pages/")
                || !std::path::Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
            {
                continue;
            }
            let Entry::Bytes(bytes) = entry else {
                continue;
            };
            let Ok(raw) = std::str::from_utf8(bytes) else {
                continue;
            };
            let Ok(sp) = split(raw) else {
                continue;
            };
            let Ok(meta) = parse_meta(&sp.front_matter) else {
                continue;
            };
            if meta.r#type != PageType::Calc {
                continue;
            }
            calc_pages += 1;
            let mut found_fixture = false;
            for line in sp.body.lines() {
                if let Some(rest) = line.trim().strip_prefix("Fixture:") {
                    found_fixture = true;
                    let fixture_id = rest.trim();
                    if !valid_ids.contains(fixture_id) {
                        failures.push(format!(
                            "{path} ({}): Fixture `{fixture_id}` is not in fieldbook_core::cases::ids()",
                            meta.id
                        ));
                    }
                }
            }
            if !found_fixture {
                failures.push(format!(
                    "{path} ({}): calc page has no `Fixture:` line",
                    meta.id
                ));
            }
        }

        if failures.is_empty() {
            Ok(calc_pages)
        } else {
            Err(failures.join("\n"))
        }
    }

    #[test]
    fn every_calc_page_fixture_exists_in_cases() {
        let root = repo_root();
        let mini_root = root.join("tests/fixtures/corpus-mini");
        let mini_tree =
            io::read_tree(&mini_root).unwrap_or_else(|e| panic!("read corpus-mini tree: {e}"));
        let mini_count = verify_calc_page_fixtures(&mini_tree)
            .unwrap_or_else(|err| panic!("corpus-mini fixture_cases failed:\n{err}"));
        assert!(
            mini_count >= 1,
            "expected at least 1 calc page in corpus-mini, found {mini_count}"
        );

        let real_tree =
            io::read_tree(&root).unwrap_or_else(|e| panic!("read repository corpus tree: {e}"));
        let _real_count = verify_calc_page_fixtures(&real_tree)
            .unwrap_or_else(|err| panic!("repository corpus fixture_cases failed:\n{err}"));

        // Direct negative check: a calc page naming CALC-99 fails and names the page and CALC-99.
        let mut mutated = mini_tree;
        let calc_path = "corpus/pages/estimating/pricing/est-calc-01.md";
        let orig = match mutated
            .get(calc_path)
            .unwrap_or_else(|| panic!("est-calc-01 in tree"))
        {
            Entry::Bytes(b) => std::str::from_utf8(b)
                .unwrap_or_else(|e| panic!("utf8: {e}"))
                .to_owned(),
            _ => panic!("expected file"),
        };
        let bad = orig.replace("Fixture: CALC-01", "Fixture: CALC-99");
        assert_ne!(bad, orig);
        mutated.insert_bytes(calc_path, bad.into_bytes());
        let err = verify_calc_page_fixtures(&mutated).expect_err("CALC-99 must fail");
        assert!(
            err.contains("est-calc-01") && err.contains("CALC-99"),
            "unexpected error: {err}"
        );
    }
}
