//! Pure confidentiality, identity, claim-language, formula and allowance scans
//! (`docs/reference/corpus-format.md#scans`).
//!
//! **Responsibility.** Provide the pure scan functions (`marking`, `identity`,
//! `claim_language`, `formula`) and the `scan-allow.toml` parser (`allow`) used
//! by `check`, `add` and `new note`, plus the pattern lists pinned against the
//! reference tables in `docs/reference/corpus-format.md#scans`.
//!
//! **Rules.** Hand-written matchers only; no regex, no I/O, no clock, no environment.

use std::ops::Range;

pub mod allow;
pub mod claim_language;
pub mod formula;
pub mod identity;
pub mod marking;

/// A single match reported by a scan over a text slice or cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// The matched substring as written in the input text.
    pub matched: String,
    /// Byte span (`start..end`) of `matched` in the input text.
    pub span: Range<usize>,
}

impl Finding {
    /// Create a new [`Finding`] with `matched` and `span`.
    #[must_use]
    pub fn new(matched: impl Into<String>, span: Range<usize>) -> Self {
        Self {
            matched: matched.into(),
            span,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::hash::sha256_hex;

    use super::{allow, claim_language, formula, identity, marking};

    #[test]
    fn marking_controlled_information_matches() {
        let hits = marking::scan("Header CUI// and CONTROLLED UNCLASSIFIED INFORMATION footer");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].matched, "CUI//");
        assert_eq!(hits[1].matched, "CONTROLLED UNCLASSIFIED INFORMATION");
    }

    #[test]
    fn marking_dissemination_matches() {
        let hits = marking::scan("Banner NOFORN and FOUO here.");
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].matched, "NOFORN");
        assert_eq!(hits[1].matched, "FOUO");
    }

    #[test]
    fn marking_distribution_statements_b_to_f_match() {
        for letter in ['B', 'C', 'D', 'E', 'F'] {
            let banner = format!("DISTRIBUTION STATEMENT {letter}");
            let text = format!("Notice: {banner} applies.");
            let hits = marking::scan(&text);
            assert_eq!(hits.len(), 1, "expected {banner} to match");
            assert_eq!(hits[0].matched, banner);
        }
    }

    #[test]
    fn marking_distribution_statement_a_does_not_match() {
        let hits = marking::scan("DISTRIBUTION STATEMENT A. Approved for public release.");
        assert!(hits.is_empty(), "Statement A must not match, got: {hits:?}");
    }

    #[test]
    fn marking_export_control_matches() {
        for banner in [
            "EXPORT CONTROLLED",
            "EXPORT-CONTROLLED",
            "ITAR CONTROLLED",
            "technical data whose export is restricted",
            "Technical Data Whose Export Is Restricted",
            "TECHNICAL DATA WHOSE EXPORT IS RESTRICTED",
        ] {
            let hits = marking::scan(banner);
            assert_eq!(hits.len(), 1, "expected {banner:?} to match");
            assert_eq!(hits[0].matched, banner);
        }
    }

    #[test]
    fn marking_company_and_procurement_matches() {
        for banner in [
            "PROPRIETARY",
            "COMPANY CONFIDENTIAL",
            "SOURCE SELECTION INFORMATION",
            "CONTRACTOR BID OR PROPOSAL INFORMATION",
        ] {
            let hits = marking::scan(banner);
            assert_eq!(hits.len(), 1, "expected {banner:?} to match");
            assert_eq!(hits[0].matched, banner);
        }
    }

    #[test]
    fn lower_case_running_text_does_not_match() {
        let text = "\
            The contractor protects proprietary rates, company confidential memos, \
            source selection information, contractor bid or proposal information, \
            controlled unclassified information, export controlled drawings, \
            export-controlled items, itar controlled parts, noforn notes, fouo drafts, \
            and distribution statement b markings in lower-case running prose.";
        let hits = marking::scan(text);
        assert!(
            hits.is_empty(),
            "lower-case running text must not match banner shapes, got: {hits:?}"
        );
    }

    #[test]
    fn identity_email_address_matches() {
        let text = "Contact jane.doe@example.org for details.";
        let hits = identity::scan(text);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].matched, "jane.doe@example.org");
    }

    #[test]
    fn identity_email_noreply_and_github_noreply_do_not_match() {
        let text = "\
            Commit by noreply@example.com, no-reply@agency.gov, \
            and 205793494+thetimmyman@users.noreply.github.com.";
        let hits = identity::scan(text);
        assert!(
            hits.is_empty(),
            "noreply local parts and users.noreply.github.com must not match: {hits:?}"
        );
    }

    #[test]
    fn identity_phone_number_matches() {
        for sample in [
            "+12025550143",
            "(202) 555-0143",
            "(202)5550143",
            "202-555-0143",
        ] {
            let text = format!("Call {sample} today.");
            let hits = identity::scan(&text);
            assert_eq!(hits.len(), 1, "expected phone {sample} to match: {hits:?}");
            assert_eq!(hits[0].matched, sample);
        }
    }

    #[test]
    fn identity_phone_dotted_clause_number_does_not_match() {
        let text = "DFARS 252.204-7012 and 252.215-7009 and FAR 52.215-2.";
        let hits = identity::scan(text);
        assert!(
            hits.is_empty(),
            "dotted clause numbers must not match phone scan: {hits:?}"
        );
    }

    #[test]
    fn identity_private_ipv4_matches() {
        for ip in [
            "10.0.0.1",
            "10.255.255.254",
            "172.16.0.1",
            "172.31.255.1",
            "192.168.1.100",
            "127.0.0.1",
            "169.254.169.254",
        ] {
            let text = format!("Server at {ip} responded.");
            let hits = identity::scan(&text);
            assert_eq!(hits.len(), 1, "expected private IPv4 {ip} to match");
            assert_eq!(hits[0].matched, ip);
        }
    }

    #[test]
    fn identity_public_ipv4_does_not_match() {
        for ip in [
            "8.8.8.8",
            "1.1.1.1",
            "172.15.0.1",
            "172.32.0.1",
            "192.169.1.1",
            "169.253.1.1",
        ] {
            let text = format!("Public address {ip} is outside RFC 1918.");
            let hits = identity::scan(&text);
            assert!(hits.is_empty(), "public IPv4 {ip} must not match: {hits:?}");
        }
    }

    #[test]
    fn identity_internal_host_name_matches() {
        for host in [
            "erp.lan",
            "printer.local",
            " build-box.internal ",
            "nas.home.",
            "sub.corp.internal",
        ] {
            let hits = identity::scan(host);
            assert_eq!(hits.len(), 1, "expected internal host in {host:?} to match");
        }
    }

    #[test]
    fn identity_home_directory_path_matches() {
        for path in [
            r"C:\Users\alice\Documents",
            "/home/alice/work",
            "/Users/bob/Desktop",
        ] {
            let hits = identity::scan(path);
            assert_eq!(hits.len(), 1, "expected home path in {path:?} to match");
        }
    }

    #[test]
    fn identity_home_directory_placeholder_does_not_match() {
        for placeholder in [
            r"C:\Users\<user>\AppData",
            r"C:\Users\%USERNAME%\AppData",
            "/home/<user>/.config",
            "/Users/<owner>/Library",
            "/home/%USER%/work",
        ] {
            let hits = identity::scan(placeholder);
            assert!(
                hits.is_empty(),
                "placeholder {placeholder:?} must not match: {hits:?}"
            );
        }
    }

    #[test]
    fn identity_tracker_key_matches() {
        for key in ["PROJ-123", "ABCDE-1", "TICKET-999999"] {
            let text = format!("See {key} for details.");
            let hits = identity::scan(&text);
            assert_eq!(hits.len(), 1, "expected tracker key {key} to match");
            assert_eq!(hits[0].matched, key);
        }
    }

    #[test]
    fn identity_tracker_key_exempt_prefixes_do_not_match() {
        for exempt in [
            "Q-025",
            "XC-14",
            "CL-CMP-0011",
            "CALC-01",
            "QA-01",
            "FC-01",
            "EST-01",
            "QTE-01",
            "SCH-01",
            "RTE-01",
            "EAC-01",
            "EVM-01",
            "CTF-01",
            "AC-2",
            "AT-1",
            "AU-6",
            "CA-7",
            "CM-2",
            "CP-9",
            "IA-5",
            "IR-4",
            "MA-2",
            "MP-4",
            "PE-3",
            "PL-2",
            "PM-1",
            "PS-3",
            "PT-2",
            "RA-5",
            "SA-8",
            "SC-7",
            "SI-4",
            "SR-3",
            "DD-254",
            "SF-1408",
            "ISO-9001",
            "AS-9100",
            "FIPS-140",
            "RFC-4180",
        ] {
            let hits = identity::scan(exempt);
            assert!(
                hits.is_empty(),
                "exempt prefix in {exempt} must not match: {hits:?}"
            );
        }
    }

    #[test]
    fn claim_language_phrases_match_in_any_case() {
        for phrase in claim_language::PHRASES {
            let upper = phrase.to_uppercase();
            let hits = claim_language::scan(&upper);
            assert_eq!(hits.len(), 1, "expected {upper} to match");
            assert_eq!(hits[0].matched, upper);
        }
    }

    #[test]
    fn precise_and_code_are_not_read_for_claim_language() {
        let body = "\
## Purpose

Explain the boundary.

> Precise: The vendor brochure says \"CMMC compliant\" and \"NIST certified\".

```text
System status: EVMS compliant, audit-ready, DCAA approved.
```

The system is `FIPS compliant` in code only.
";
        let hits = crate::check::scans::scan_page_body_claim_language(body, 1);
        assert!(
            hits.is_empty(),
            "Precise: blockquotes and code blocks must not be read for claim language, got: {hits:?}"
        );

        let body_with_prose_hit = "\
## Purpose

The system is CMMC compliant.
";
        let prose_hits = crate::check::scans::scan_page_body_claim_language(body_with_prose_hit, 1);
        assert_eq!(prose_hits.len(), 1);
        assert_eq!(prose_hits[0].0.matched, "CMMC compliant");
    }

    #[test]
    fn formula_triggers_match() {
        for cell in ["=1+1", "+SUM(A1:A2)", "-A1", "@SUM(A1)", "\ttext", "\rtext"] {
            let hit = formula::scan_cell(cell);
            assert!(hit.is_some(), "expected {cell:?} to trigger FB0403");
        }
    }

    #[test]
    fn minus_five_is_not_a_formula() {
        assert_eq!(formula::scan_cell("-5"), None);
        assert_eq!(formula::scan_cell("-0.25"), None);
        assert_eq!(formula::scan_cell("42.5"), None);
    }

    #[test]
    fn an_edit_ends_the_allowance() {
        let original = b"Public regulation quoting PROPRIETARY banner.\n";
        let hash = sha256_hex(original);
        let toml_text = format!(
            "[[allow]]\nsha256 = \"{hash}\"\nmatch = \"PROPRIETARY\"\nreason = \"Quoted in regulation.\"\n"
        );
        let mut allowances = allow::parse(&toml_text).expect("valid scan-allow.toml");
        assert!(allowances.allows(&hash, "PROPRIETARY"));
        assert!(allowances.unused().is_empty());

        let edited = b"Public regulation quoting PROPRIETARY banner (edited).\n";
        let edited_hash = sha256_hex(edited);
        let mut allowances_after_edit = allow::parse(&toml_text).expect("valid scan-allow.toml");
        assert!(
            !allowances_after_edit.allows(&edited_hash, "PROPRIETARY"),
            "an edit to the file changes its sha256 and must end the allowance"
        );
    }

    #[test]
    fn an_unused_allowance_is_fb0405() {
        let hash = "a".repeat(64);
        let toml_text = format!(
            "[[allow]]\nsha256 = \"{hash}\"\nmatch = \"FOUO\"\nreason = \"Historical citation.\"\n"
        );
        let allowances = allow::parse(&toml_text).expect("valid scan-allow.toml");
        let unused = allowances.unused();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].matched, "FOUO");
    }
}
