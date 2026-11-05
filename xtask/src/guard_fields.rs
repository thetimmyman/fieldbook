//! The content rules of GS-02: what an agent commit may not certify.
//!
//! The claims register is parsed with `fieldbook_core::schema::table::read` and
//! `ClaimRow`; the page rule reads `status` via `page::split` and `parse_meta`.

use crate::proc;
use fieldbook_core::ClaimId;
use fieldbook_core::schema::page::{self, MetaErrorKind};
use fieldbook_core::schema::table::{self, Row};
use fieldbook_core::schema::{
    Adoption, Applicability, ClaimRow, DataOrigin, PageStatus, Register, Verification, is_raise,
};
use std::collections::BTreeMap;
use std::path::Path;

/// The claims register and the regression set, by path.
const CLAIMS: &str = "corpus/registers/claims.csv";
pub(super) const REGRESSION: &str = "corpus/eval/regression.toml";

/// The five certification columns and the eight evidence columns guarded by GS-02.
const FIELDS: [&str; 13] = [
    "verification",
    "applicability",
    "adoption",
    "verified_on",
    "verified_by_role",
    "statement",
    "value",
    "unit",
    "source_id",
    "version",
    "locator_kind",
    "locator",
    "quote",
];

/// A tree entry: its mode and object id. Equal entries are the same bytes.
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Entry {
    mode: String,
    oid: String,
}

/// What judging one changed path found.
#[derive(Default)]
pub(super) struct Verdict {
    pub problems: Vec<String>,
    /// The path is a seed golden that no parent holds.
    pub new_seed: bool,
}

pub(super) fn is_seed(path: &str) -> bool {
    path.rsplit('/').next().unwrap_or(path).starts_with("seed-")
}

/// Originals are byte-exact: a new version is a new file.
fn is_original(path: &str) -> bool {
    path.starts_with("corpus/sources/public/")
}

/// Read raw objects from the explicit repository, unaffected by hook Git context.
pub(super) fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let root = repo.to_str().ok_or("repository path is not UTF-8")?;
    let mut argv = Vec::new();
    for name in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_COMMON_DIR",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
        "GIT_CONFIG",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
    ] {
        argv.extend(["-u", name]);
    }
    argv.extend([
        "git",
        "--no-replace-objects",
        "--literal-pathspecs",
        "-C",
        root,
    ]);
    argv.extend_from_slice(args);
    proc::capture("env", &argv, repo)
}

/// The entry of `path` at `revision`, or `None` when the tree has no such file.
pub(super) fn entry(repo: &Path, revision: &str, path: &str) -> Result<Option<Entry>, String> {
    let line = git(repo, &["ls-tree", "-z", revision, "--", path])?;
    let Some(record) = line.split('\0').find(|record| !record.is_empty()) else {
        return Ok(None);
    };
    let mut words = record.split(['\t', ' ']);
    match (words.next(), words.next(), words.next()) {
        (Some(mode), Some(_kind), Some(oid)) => Ok(Some(Entry {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
        })),
        _ => Err(format!("unreadable tree entry for {path}")),
    }
}

fn text(repo: &Path, entry: &Entry) -> Result<String, String> {
    git(repo, &["cat-file", "blob", &entry.oid])
}

/// Judge one path an agent commit changed, against the entry in each parent
/// (`None` for a parent without the file, or for a root commit).
pub(super) fn judge(
    repo: &Path,
    sha: &str,
    path: &str,
    before: &[Option<Entry>],
    after: Option<&Entry>,
) -> Result<Verdict, String> {
    let mut verdict = Verdict::default();
    let problem = |reason: &str| format!("{sha} {path}: {reason}");
    if is_seed(path) {
        if after.is_some() && before.iter().all(Option::is_none) {
            verdict.new_seed = true;
        } else {
            verdict
                .problems
                .push(problem("an agent may not change an existing seed golden"));
        }
        return Ok(verdict);
    }
    if is_original(path) {
        if before.iter().any(Option::is_some) {
            verdict.problems.push(problem(
                "an original is byte-exact; add the next version as a new file",
            ));
        }
        return Ok(verdict);
    }
    let read = |entry: &Option<Entry>| -> Result<Option<String>, String> {
        entry.as_ref().map(|entry| text(repo, entry)).transpose()
    };
    if path == REGRESSION {
        let Some(after) = after else {
            verdict
                .problems
                .push(problem("an agent may not delete the regression set"));
            return Ok(verdict);
        };
        let after = text(repo, after)?;
        for old in before {
            if let Some(reason) = regression_problem(read(old)?.as_deref().unwrap_or(""), &after) {
                verdict.problems.push(problem(reason));
            }
        }
    } else if path == CLAIMS {
        let Some(after) = after else {
            return Ok(verdict);
        };
        let mut parents = Vec::new();
        for old in before {
            parents.push(read(old)?);
        }
        let parsed = parse_claims(&text(repo, after)?).and_then(|after| {
            let parents = parents
                .iter()
                .map(|old| old.as_deref().map(parse_claims).transpose())
                .collect::<Result<Vec<_>, _>>()?;
            Ok((after, parents))
        });
        let (after, parents) = match parsed {
            Ok((after, parents)) => (after, parents),
            Err(error) => {
                verdict
                    .problems
                    .push(format!("{}; cannot judge", problem(&error)));
                return Ok(verdict);
            }
        };
        let parents: Vec<_> = parents.into_iter().map(Option::unwrap_or_default).collect();
        for (id, row) in &after {
            if parents
                .iter()
                .any(|rows| row_problems(rows.get(id), row).is_empty())
            {
                continue;
            }
            for rows in &parents {
                for (field, reason) in row_problems(rows.get(id), row) {
                    let line = format!("{sha} {id} {field}: {reason}");
                    if !verdict.problems.contains(&line) {
                        verdict.problems.push(line);
                    }
                }
            }
        }
    } else if path.starts_with("corpus/pages/") && path.ends_with(".md") {
        let Some(after) = after else {
            return Ok(verdict);
        };
        match page_status(&text(repo, after)?) {
            Ok(PageStatus::Stable) => {
                let mut stable_before = false;
                for old in before {
                    if let Some(old) = read(old)? {
                        stable_before |= page_status(&old).ok() == Some(PageStatus::Stable);
                    }
                }
                if !stable_before {
                    verdict.problems.push(problem(
                        "status stable: only the owner accepts a page as stable",
                    ));
                }
            }
            Ok(_) => {}
            Err(error) => verdict
                .problems
                .push(format!("{}; cannot judge", problem(&error))),
        }
    }
    Ok(verdict)
}

/// Every case of `before` stays unchanged, in order, and no added case sets `critical`.
fn regression_problem(before: &str, after: &str) -> Option<&'static str> {
    let before = match regression_cases(before) {
        Ok(cases) => cases,
        Err(reason) => return Some(reason),
    };
    let after = match regression_cases(after) {
        Ok(cases) => cases,
        Err(reason) => return Some(reason),
    };
    if after.get(..before.len()) != Some(before.as_slice()) {
        return Some("an agent may only add regression cases, never edit or remove one");
    }
    after[before.len()..]
        .iter()
        .any(|(_, case)| {
            case.get("critical")
                .is_some_and(|value| value.as_bool() != Some(false))
        })
        .then_some("only a person marks a regression case critical")
}

fn regression_cases(text: &str) -> Result<Vec<(String, toml::Table)>, &'static str> {
    let mut root = text
        .parse::<toml::Table>()
        .map_err(|_| "regression.toml is not valid TOML")?;
    let cases = match root.remove("case") {
        None => Vec::new(),
        Some(toml::Value::Array(cases)) => cases,
        Some(_) => return Err("`case` in regression.toml is not an array of tables"),
    };
    if !root.is_empty() {
        return Err("an agent may only add regression cases, never edit or remove one");
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut parsed = Vec::with_capacity(cases.len());
    for value in cases {
        let toml::Value::Table(case) = value else {
            return Err("a regression case is not a table");
        };
        let Some(id) = case
            .get("id")
            .and_then(toml::Value::as_str)
            .filter(|id| !id.is_empty())
        else {
            return Err("a regression case has no string `id`");
        };
        if !ids.insert(id.to_owned()) {
            return Err("duplicate regression case `id`");
        }
        parsed.push((id.to_owned(), case));
    }
    Ok(parsed)
}

/// What an agent's change to one claim certifies against `before`, as `(field, reason)` pairs.
fn row_problems(before: Option<&ClaimRow>, after: &ClaimRow) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    if is_raise(before, after) {
        if after.verification != Verification::Unchecked
            && before.map(|row| row.verification) != Some(after.verification)
        {
            found.push((
                "verification",
                format!(
                    "raised to `{}`; only the owner certifies",
                    after.verification
                ),
            ));
        }
        if after.applicability != Applicability::PendingCompanyConfirmation
            && before.map(|row| row.applicability) != Some(after.applicability)
        {
            found.push((
                "applicability",
                format!(
                    "raised to `{}`; only the owner certifies",
                    after.applicability
                ),
            ));
        }
        if after.adoption != Adoption::Proposed
            && before.map(|row| row.adoption) != Some(after.adoption)
        {
            found.push((
                "adoption",
                format!("raised to `{}`; only the owner certifies", after.adoption),
            ));
        }
        if after.verified_on.is_some()
            && before.and_then(|row| row.verified_on) != after.verified_on
        {
            found.push(("verified_on", "set; only the owner certifies".to_owned()));
        }
        if after.verified_by_role.is_some()
            && before.and_then(|row| row.verified_by_role) != after.verified_by_role
        {
            found.push((
                "verified_by_role",
                "set; only the owner certifies".to_owned(),
            ));
        }
    }
    let reset = after.verification == Verification::Unchecked
        && after.verified_on.is_none()
        && after.verified_by_role.is_none();
    if !reset
        && let Some(prev) = before
        && let Some(field) = [
            ("statement", prev.statement != after.statement),
            ("value", prev.value != after.value),
            ("unit", prev.unit != after.unit),
            ("source_id", prev.source_id != after.source_id),
            ("version", prev.version != after.version),
            ("locator_kind", prev.locator_kind != after.locator_kind),
            ("locator", prev.locator != after.locator),
            ("quote", prev.quote != after.quote),
        ]
        .into_iter()
        .find_map(|(field, changed)| changed.then_some(field))
    {
        found.push((
            field,
            "edited on a checked claim; reset verification to `unchecked` and clear the verified fields in the same commit".to_owned(),
        ));
    }
    found
}

/// Parses `claims.csv` with `table::read` and `ClaimRow::from_row`, pairing rows by `claim_id`.
fn parse_claims(text: &str) -> Result<BTreeMap<ClaimId, ClaimRow>, String> {
    let table = table::read(text.as_bytes()).map_err(|err| err.to_string())?;
    let header = &table.header.cells;
    let unique: std::collections::BTreeSet<_> = header.iter().collect();
    if unique.len() != header.len() {
        return Err("duplicate column in the claims header".to_owned());
    }
    for required in std::iter::once("claim_id").chain(FIELDS) {
        if !header.iter().any(|cell| cell == required) {
            return Err(format!("no `{required}` column"));
        }
    }
    let schema_cols = Register::Claims.columns();
    let indices: Vec<Option<usize>> = schema_cols
        .iter()
        .map(|col| header.iter().position(|cell| cell == col.name))
        .collect();
    let mut rows = BTreeMap::new();
    for row in &table.rows {
        let mut cells = Vec::with_capacity(schema_cols.len());
        let mut cols = Vec::with_capacity(schema_cols.len());
        for (i, col) in schema_cols.iter().enumerate() {
            let value = match indices[i] {
                Some(idx) => {
                    let cell = row.cells[idx].as_str();
                    if col.name == "adoption" && cell == "company_standard" {
                        "company_approved"
                    } else {
                        cell
                    }
                }
                None => match col.name {
                    "area" => "syn",
                    "kind" => "fact",
                    "consequential" => "false",
                    "data_origin" => "unknown",
                    "volatility" => "V1",
                    _ => "",
                },
            };
            cells.push(value.to_owned());
            cols.push(
                indices[i]
                    .and_then(|idx| row.cols.get(idx).copied())
                    .unwrap_or(1),
            );
        }
        let canonical = Row {
            line: row.line,
            cols,
            cells,
        };
        let mut claim = ClaimRow::from_row(&canonical)
            .map_err(|problems| format!("row {}: {}", row.line, problems[0]))?;
        claim.data_origin = DataOrigin::Unknown;
        if rows.insert(claim.claim_id.clone(), claim).is_some() {
            return Err(format!("row {}: duplicate claim_id", row.line));
        }
    }
    Ok(rows)
}

/// Reads a page's `status` with `page::split` and `page::parse_meta`.
fn page_status(page: &str) -> Result<PageStatus, String> {
    let split = page::split(page).map_err(|err| err.to_string())?;
    let mut front_matter = split.front_matter;
    for _ in 0..8 {
        match page::parse_meta(&front_matter) {
            Ok(meta) => return Ok(meta.status),
            Err(err) if err.kind() == MetaErrorKind::MissingField => {
                let fallback = match err.field() {
                    Some("title") => "\ntitle = \"Synthetic\"\n",
                    Some("area") => "\narea = \"syn\"\n",
                    Some("subarea") => "\nsubarea = \"syn-a\"\n",
                    Some("type") => "\ntype = \"concept\"\n",
                    Some("stage") => "\nstage = 1\n",
                    Some("readiness") => "\nreadiness = \"PUBLIC-NOW\"\n",
                    Some("as_of") => "\nas_of = 2026-01-01\n",
                    Some("handling") => "\nhandling = [\"public\"]\n",
                    _ => return Err(err.to_string()),
                };
                front_matter.push_str(fallback);
            }
            Err(err) => return Err(err.to_string()),
        }
    }
    page::parse_meta(&front_matter)
        .map(|meta| meta.status)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::regression_problem;

    #[test]
    fn regression_cases_may_only_be_added() {
        let before = "[[case]]\nid = \"R-1\"\n";
        assert_eq!(
            regression_problem(before, &format!("{before}[[case]]\nid = \"R-2\"\n")),
            None
        );
        assert!(regression_problem(before, "[[case]]\nid = \"R-9\"\n").is_some());
        assert!(regression_problem(before, &format!("{before}critical = true\n")).is_some());
        assert!(regression_problem(before, &format!("{before}top = 1000000\n")).is_some());
    }
}
