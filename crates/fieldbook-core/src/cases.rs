//! Synthetic calculation cases exposed to the core and renderer (`AD-14`, `FM0`).
//!
//! **Responsibility.** Hold the embedded `CASES` table (`CALC-01` to `CALC-04` and `CALC-90`),
//! expose [`ids`], [`run`], [`verify`], [`case_set_sha256_entries`] and [`case_set_sha256`], and prove in
//! `every_case_file_is_listed` that every `*.toml` file in `crates/fieldbook-calc/tests/cases/`
//! is listed in `CASES`.

use fieldbook_calc::{CalcError, Case, Mismatch, Outcome, evaluate, verify_case};

use crate::hash::Sha256Stream;

/// Every synthetic calculation case file in `crates/fieldbook-calc/tests/cases/`, as
/// `(file_name, raw_toml)`.
pub const CASES: &[(&str, &str)] = &[
    (
        "calc-01.toml",
        include_str!("../../fieldbook-calc/tests/cases/calc-01.toml"),
    ),
    (
        "calc-02.toml",
        include_str!("../../fieldbook-calc/tests/cases/calc-02.toml"),
    ),
    (
        "calc-03.toml",
        include_str!("../../fieldbook-calc/tests/cases/calc-03.toml"),
    ),
    (
        "calc-04.toml",
        include_str!("../../fieldbook-calc/tests/cases/calc-04.toml"),
    ),
    (
        "calc-90.toml",
        include_str!("../../fieldbook-calc/tests/cases/calc-90.toml"),
    ),
];

/// Return the case ids (`CALC-01`, `CALC-02`, ...) of every case in [`CASES`].
#[must_use]
pub fn ids() -> Vec<String> {
    let mut out = Vec::with_capacity(CASES.len());
    for &(_name, raw) in CASES {
        if let Ok(case) = toml::from_str::<Case>(raw) {
            out.push(case.id);
        }
    }
    out
}

/// Evaluate the synthetic case whose `id` matches `id`, returning `None` when no case in
/// [`CASES`] has that id.
#[must_use]
pub fn run(id: &str) -> Option<Result<Outcome, CalcError>> {
    for &(_name, raw) in CASES {
        if let Ok(case) = toml::from_str::<Case>(raw)
            && case.id == id
        {
            return Some(evaluate(&case));
        }
    }
    None
}

/// Check the synthetic case whose `id` matches `id` against its expected outputs, returning
/// `None` when no case in [`CASES`] has that id.
///
/// # Errors
///
/// The inner `Err` lists every [`Mismatch`] between the case's expected outputs and what the
/// calc core produced.
#[must_use]
pub fn verify(id: &str) -> Option<Result<(), Vec<Mismatch>>> {
    for &(_name, raw) in CASES {
        if let Ok(case) = toml::from_str::<Case>(raw)
            && case.id == id
        {
            return Some(verify_case(&case));
        }
    }
    None
}

/// Compute the full 64-hex SHA-256 of `entries` (`(file_name, content)`) in file-name order,
/// encoding each entry as `<file_name>\n<bytes>\n` (`AD-14`).
#[must_use]
pub fn case_set_sha256_entries(entries: &[(&str, &str)]) -> String {
    let mut sorted: Vec<(&str, &str)> = entries.to_vec();
    sorted.sort_by_key(|&(name, _)| name);
    let mut stream = Sha256Stream::new();
    for (name, content) in sorted {
        stream.update(name.as_bytes());
        stream.update(b"\n");
        stream.update(content.as_bytes());
        stream.update(b"\n");
    }
    stream.finish()
}

/// Compute the full 64-hex SHA-256 of [`CASES`] in file-name order.
#[must_use]
pub fn case_set_sha256() -> String {
    case_set_sha256_entries(CASES)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::{CASES, ids, run, verify};

    #[test]
    #[expect(
        clippy::disallowed_methods,
        reason = "test reads the repository case directory to verify every file is listed in CASES"
    )]
    fn every_case_file_is_listed() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../fieldbook-calc/tests/cases");
        let entries = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read crates/fieldbook-calc/tests/cases: {e}"));
        let mut on_disk = BTreeMap::new();
        for entry in entries {
            let entry = entry.unwrap_or_else(|e| panic!("dir entry: {e}"));
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("toml") {
                let name = path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or_else(|| panic!("utf-8 file name"))
                    .to_owned();
                let body = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("read case file: {e}"));
                on_disk.insert(name, body);
            }
        }

        let listed: BTreeMap<String, String> = CASES
            .iter()
            .map(|&(name, body)| (name.to_owned(), body.to_owned()))
            .collect();

        for name in on_disk.keys() {
            assert!(
                listed.contains_key(name),
                "case file {name} in crates/fieldbook-calc/tests/cases/ is missing from fieldbook_core::cases::CASES"
            );
        }
        for (name, body) in &listed {
            let disk_body = on_disk.get(name).unwrap_or_else(|| {
                panic!(
                    "CASES lists {name}, which does not exist in crates/fieldbook-calc/tests/cases/"
                )
            });
            assert_eq!(
                body, disk_body,
                "CASES entry for {name} differs from file on disk"
            );
        }
    }

    #[test]
    fn ids_and_run_cover_the_listed_cases() {
        let all_ids = ids();
        assert_eq!(
            all_ids,
            vec!["CALC-01", "CALC-02", "CALC-03", "CALC-04", "CALC-90"]
        );
        let c01 = run("CALC-01")
            .unwrap_or_else(|| panic!("CALC-01 exists"))
            .unwrap_or_else(|e| panic!("CALC-01 evaluates: {e:?}"));
        assert!(!c01.outputs.is_empty());
        let c90 = run("CALC-90").unwrap_or_else(|| panic!("CALC-90 exists"));
        assert!(c90.is_err());
        assert!(run("CALC-99").is_none());
    }

    #[test]
    fn verify_passes_calc_01_and_ignores_unknown_ids() {
        assert_eq!(verify("CALC-01"), Some(Ok(())));
        assert!(verify("CALC-99").is_none());
    }
}
