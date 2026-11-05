//! Verifies that the pattern lists in `fieldbook_core::scan` stay in lockstep
//! with the `Scans` tables in `docs/reference/corpus-format.md`, and that
//! tracker-key exemptions cover every `ids::FAMILIES` prefix while refusing
//! synthetic tracker prefixes.

mod scan_lists {
    use fieldbook_core::ids::FAMILIES;
    use fieldbook_core::scan::{claim_language, formula, identity, marking};

    const CORPUS_FORMAT: &str = include_str!("../../../docs/reference/corpus-format.md");

    fn extract_scans_section(doc: &str) -> &str {
        let Some(start) = doc.find("\n### Scans\n") else {
            panic!("docs/reference/corpus-format.md must contain `### Scans`");
        };
        let after = &doc[start + "\n### Scans\n".len()..];
        let end = after.find("\n## ").unwrap_or(after.len());
        &after[..end]
    }

    fn parse_markdown_table<'a>(section: &'a str, header_prefix: &str) -> Vec<Vec<&'a str>> {
        let mut lines = section.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim_start().starts_with(header_prefix) {
                // Skip separator line `|---|...`
                let Some(sep) = lines.next() else {
                    panic!("markdown table header must be followed by separator row");
                };
                assert!(
                    sep.trim_start().starts_with("|---"),
                    "expected table separator after {header_prefix}, got {sep:?}"
                );
                let mut rows = Vec::new();
                while let Some(&row_line) = lines.peek() {
                    let trimmed = row_line.trim();
                    if !trimmed.starts_with('|') {
                        break;
                    }
                    let _ = lines.next();
                    let inner = trimmed.strip_prefix('|').unwrap_or(trimmed);
                    let inner = inner.strip_suffix('|').unwrap_or(inner);
                    let cells: Vec<&str> = inner.split('|').map(str::trim).collect();
                    rows.push(cells);
                }
                return rows;
            }
        }
        panic!("table starting with {header_prefix:?} not found in Scans section");
    }

    fn extract_backticked(cell: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut rest = cell;
        while let Some(open) = rest.find('`') {
            let after_open = &rest[open + 1..];
            let Some(close) = after_open.find('`') else {
                panic!("unbalanced backtick in Scans table cell");
            };
            out.push(&after_open[..close]);
            rest = &after_open[close + 1..];
        }
        out
    }

    #[test]
    fn scan_tables_match_corpus_format() {
        let section = extract_scans_section(CORPUS_FORMAT);

        // 1. Main Scans summary table: Claim language and Formula triggers rows.
        let main_rows =
            parse_markdown_table(section, "| Scan | Code | Reads | Catches | Allowance |");
        let claim_row = main_rows
            .iter()
            .find(|r| r.first().copied() == Some("Claim language"))
            .expect("Scans table must contain row 'Claim language'");
        let doc_phrases = extract_backticked(claim_row[3]);
        assert_eq!(
            doc_phrases.as_slice(),
            claim_language::PHRASES,
            "Scans table row 'Claim language' Catches cell differs from claim_language::PHRASES"
        );

        let formula_row = main_rows
            .iter()
            .find(|r| r.first().copied() == Some("Formula triggers"))
            .expect("Scans table must contain row 'Formula triggers'");
        let mut doc_triggers: Vec<&str> = extract_backticked(formula_row[3]);
        if formula_row[3].contains("TAB") {
            doc_triggers.push("TAB");
        }
        if formula_row[3].contains("CR") {
            doc_triggers.push("CR");
        }
        assert_eq!(
            doc_triggers.as_slice(),
            formula::TRIGGER_NAMES,
            "Scans table row 'Formula triggers' Catches cell differs from formula::TRIGGER_NAMES"
        );
        let mapped_chars: Vec<char> = doc_triggers
            .iter()
            .map(|&name| match name {
                "=" => '=',
                "+" => '+',
                "-" => '-',
                "@" => '@',
                "TAB" => '\t',
                "CR" => '\r',
                other => {
                    panic!("Scans table row 'Formula triggers' has unknown trigger token {other:?}")
                }
            })
            .collect();
        assert_eq!(
            mapped_chars.as_slice(),
            formula::TRIGGERS,
            "Scans table row 'Formula triggers' Catches cell differs from formula::TRIGGERS"
        );

        // 2. Banner shapes table.
        let banner_rows = parse_markdown_table(section, "| Shape | Matches |");
        assert_eq!(
            banner_rows.len(),
            marking::SHAPES.len(),
            "Banner shapes table row count differs from marking::SHAPES"
        );
        let mut expected_all_patterns: Vec<marking::Pattern> = Vec::new();
        for (row, code_row) in banner_rows.iter().zip(marking::SHAPES.iter()) {
            let shape_name = row[0];
            let matches_cell = row[1];
            assert_eq!(
                shape_name, code_row.shape,
                "Banner shapes row {shape_name:?}: shape name mismatch in marking::SHAPES"
            );

            let ticks = extract_backticked(matches_cell);
            let mut row_patterns: Vec<marking::Pattern> = Vec::new();
            if shape_name == "Distribution statements" {
                assert_eq!(
                    ticks,
                    vec!["DISTRIBUTION STATEMENT B", "DISTRIBUTION STATEMENT F"],
                    "Banner shapes row 'Distribution statements' unexpected range ticks"
                );
                for letter in b'B'..=b'F' {
                    let s = format!("DISTRIBUTION STATEMENT {}", char::from(letter));
                    let leaked: &'static str = Box::leak(s.into_boxed_str());
                    row_patterns.push(marking::Pattern::exact(leaked));
                }
            } else {
                let any_case_tail = matches_cell.ends_with("in any case");
                for (idx, &tick) in ticks.iter().enumerate() {
                    let any_case = any_case_tail && idx + 1 == ticks.len();
                    row_patterns.push(marking::Pattern {
                        text: tick,
                        any_case,
                    });
                }
            }
            assert_eq!(
                row_patterns.as_slice(),
                code_row.patterns,
                "Banner shapes row {shape_name:?}: patterns differ from marking::SHAPES"
            );
            expected_all_patterns.extend(row_patterns);
        }
        assert_eq!(
            expected_all_patterns.as_slice(),
            marking::PATTERNS,
            "Banner shapes table patterns differ from marking::PATTERNS"
        );

        // 3. Identity shapes table.
        let id_rows = parse_markdown_table(section, "| Shape | Matches | Does not match |");
        assert_eq!(
            id_rows.len(),
            identity::SHAPES.len(),
            "Identity shapes table row count differs from identity::SHAPES"
        );
        for (row, code_row) in id_rows.iter().zip(identity::SHAPES.iter()) {
            let shape_name = row[0];
            let matches_cell = row[1];
            let dnm_cell = row[2];
            assert_eq!(
                shape_name, code_row.shape,
                "Identity shapes row {shape_name:?}: shape name mismatch in identity::SHAPES"
            );
            assert_eq!(
                matches_cell, code_row.matches,
                "Identity shapes row {shape_name:?}: Matches cell mismatch in identity::SHAPES"
            );
            assert_eq!(
                dnm_cell, code_row.does_not_match,
                "Identity shapes row {shape_name:?}: Does not match cell mismatch in identity::SHAPES"
            );

            match shape_name {
                "Internal host name" => {
                    let suffixes = extract_backticked(matches_cell);
                    assert_eq!(
                        suffixes.as_slice(),
                        identity::INTERNAL_HOST_SUFFIXES,
                        "Identity shapes row 'Internal host name' differs from identity::INTERNAL_HOST_SUFFIXES"
                    );
                }
                "Home-directory path" => {
                    let prefixes = extract_backticked(matches_cell);
                    assert_eq!(
                        prefixes.as_slice(),
                        identity::HOME_PATH_PREFIXES,
                        "Identity shapes row 'Home-directory path' differs from identity::HOME_PATH_PREFIXES"
                    );
                }
                "Tracker key" => {
                    // Split `Does not match` by `;` into its three clauses:
                    // 1) Every prefix in the const `FAMILIES` table (...)
                    // 2) the control families `AC AT ... SR`
                    // 3) and `DD`, `SF`, `ISO`, `AS`, `FIPS`, `RFC`. No tracker key prefix is exempt
                    let clauses: Vec<&str> = dnm_cell.split(';').collect();
                    assert_eq!(
                        clauses.len(),
                        3,
                        "Identity shapes row 'Tracker key' Does not match cell must have 3 semicolon-separated clauses"
                    );

                    // Clause 1: FAMILIES prefixes + explicitly named prefixes in the prose cell.
                    let prose_family_ticks: Vec<&str> = extract_backticked(clauses[0])
                        .into_iter()
                        .filter(|&t| t != "FAMILIES")
                        .collect();
                    for &p in &prose_family_ticks {
                        assert!(
                            identity::FAMILY_PREFIXES.contains(&p),
                            "Identity shapes row 'Tracker key': prefix {p:?} named in Does not match cell is missing from identity::FAMILY_PREFIXES"
                        );
                    }
                    let mut all_family_prefixes: Vec<&str> = Vec::new();
                    for fam in &FAMILIES {
                        for &p in fam.prefixes {
                            if !all_family_prefixes.contains(&p) {
                                all_family_prefixes.push(p);
                            }
                        }
                        if let Some(cp) = fam.company_prefix
                            && !all_family_prefixes.contains(&cp)
                        {
                            all_family_prefixes.push(cp);
                        }
                    }
                    assert_eq!(
                        identity::FAMILY_PREFIXES,
                        all_family_prefixes.as_slice(),
                        "Identity shapes row 'Tracker key': identity::FAMILY_PREFIXES differs from FAMILIES prefixes"
                    );

                    // Clause 2: control families
                    let ctrl_ticks = extract_backticked(clauses[1]);
                    assert_eq!(
                        ctrl_ticks.len(),
                        1,
                        "Identity shapes row 'Tracker key': control families clause must have one backticked space-separated list"
                    );
                    let doc_ctrl: Vec<&str> = ctrl_ticks[0].split_whitespace().collect();
                    assert_eq!(
                        doc_ctrl.as_slice(),
                        identity::CONTROL_FAMILY_PREFIXES,
                        "Identity shapes row 'Tracker key': control families differ from identity::CONTROL_FAMILY_PREFIXES"
                    );

                    // Clause 3: standard prefixes
                    let doc_std = extract_backticked(clauses[2]);
                    assert_eq!(
                        doc_std.as_slice(),
                        identity::STANDARD_PREFIXES,
                        "Identity shapes row 'Tracker key': standard prefixes differ from identity::STANDARD_PREFIXES"
                    );
                }
                _ => {}
            }
        }
    }

    #[test]
    fn tracker_shaped_key_fails_on_the_exempt_list() {
        // Build a synthetic tracker prefix at runtime without writing any real tracker prefix.
        let prefix: String = ['T', 'K', 'R'].into_iter().collect();
        let key = format!("{prefix}-1042");

        // Even alongside exempt family, control, and standard keys in an exempt-context line,
        // the synthetic tracker key is caught by the identity scan.
        let sample = format!("References Q-014, CL-CMP-0001, AC-2, NIST FIPS-199, and {key}.");
        let findings = identity::scan(&sample);
        assert!(
            findings.iter().any(|f| f.matched == key),
            "synthetic tracker key {key} must fail the identity scan, got {findings:?}"
        );
    }

    #[test]
    fn tracker_exemption_lists_every_family_prefix() {
        for family in &FAMILIES {
            for &prefix in family.prefixes {
                assert!(
                    identity::FAMILY_PREFIXES.contains(&prefix),
                    "family {:?} prefix {prefix:?} is missing from identity::FAMILY_PREFIXES",
                    family.name
                );
            }
            if let Some(cp) = family.company_prefix {
                assert!(
                    identity::FAMILY_PREFIXES.contains(&cp),
                    "family {:?} company_prefix {cp:?} is missing from identity::FAMILY_PREFIXES",
                    family.name
                );
            }
        }
    }
}
