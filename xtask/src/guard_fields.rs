//! The content rules of GS-02: what an agent commit may not certify.
//!
//! The registers are read with a small RFC 4180 reader until `core-csv` gives the
//! guard `table::read`; the page rule reads only the front-matter `status` line.

use crate::proc;
use std::collections::BTreeMap;
use std::path::Path;

/// The claims register and the regression set, by path.
const CLAIMS: &str = "corpus/registers/claims.csv";
pub(super) const REGRESSION: &str = "corpus/eval/regression.toml";

/// The three graded fields and their entry values, then the two verification
/// fields, then the eight evidence fields. Order matters to `claim_problems`.
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
const ENTRY: [&str; 3] = ["unchecked", "pending_company_confirmation", "proposed"];

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
        let parsed = claims(&text(repo, after)?).and_then(|after| {
            let parents = parents
                .iter()
                .map(|old| old.as_deref().map(claims).transpose())
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
            if parents.iter().any(|rows| rows.get(id) == Some(row)) {
                continue;
            }
            for rows in &parents {
                for (field, reason) in claim_problems(rows.get(id).map(Vec::as_slice), row) {
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
        match status(&text(repo, after)?) {
            Ok(Some(value)) if value == "stable" => {
                let mut stable_before = false;
                for old in before {
                    if let Some(old) = read(old)? {
                        stable_before |= status(&old).ok().flatten().as_deref() == Some("stable");
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

/// Every line of `before` stays, in order, and no added line sets `critical`.
fn regression_problem(before: &str, after: &str) -> Option<&'static str> {
    let mut kept = before.lines().peekable();
    let mut added = Vec::new();
    for line in after.lines() {
        if kept.peek() == Some(&line) {
            kept.next();
        } else {
            added.push(line);
        }
    }
    if kept.next().is_some() {
        return Some("an agent may only add regression cases, never edit or remove one");
    }
    added
        .iter()
        .any(|line| line.trim_start().starts_with("critical"))
        .then_some("only a person marks a regression case critical")
}

/// What an agent's change to one claim certifies, as (field, reason) pairs.
fn claim_problems(before: Option<&[String]>, after: &[String]) -> Vec<(&'static str, String)> {
    let mut found = Vec::new();
    let old = |index: usize| before.and_then(|row| row.get(index)).map(String::as_str);
    for (index, entry) in ENTRY.iter().enumerate() {
        let value = after[index].as_str();
        if !value.is_empty() && value != *entry && old(index) != Some(value) {
            found.push((
                FIELDS[index],
                format!("raised to `{value}`; only the owner certifies"),
            ));
        }
    }
    for index in [3, 4] {
        let value = after[index].as_str();
        if !value.is_empty() && old(index) != Some(value) {
            found.push((FIELDS[index], "set; only the owner certifies".to_owned()));
        }
    }
    let reset = after[0] == ENTRY[0] && after[3].is_empty() && after[4].is_empty();
    if let Some(row) = before
        && !reset
        && let Some(index) = (5..FIELDS.len()).find(|&index| row[index] != after[index])
    {
        found.push((
            FIELDS[index],
            "edited on a checked claim; reset verification to `unchecked` and clear the verified fields in the same commit".to_owned(),
        ));
    }
    found
}

/// The claims register as `claim_id` -> the values of `FIELDS`.
fn claims(text: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let mut records = csv(text)?.into_iter();
    let header = records.next().ok_or("no header row")?;
    let column = |name: &str| {
        header
            .iter()
            .position(|cell| cell == name)
            .ok_or_else(|| format!("no `{name}` column"))
    };
    let id = column("claim_id")?;
    let columns = FIELDS
        .iter()
        .map(|name| column(name))
        .collect::<Result<Vec<_>, _>>()?;
    let mut rows = BTreeMap::new();
    for record in records {
        if record.len() != header.len() {
            return Err(format!(
                "a row has {} cells, the header {}",
                record.len(),
                header.len()
            ));
        }
        let values = columns.iter().map(|&index| record[index].clone()).collect();
        if rows.insert(record[id].clone(), values).is_some() {
            return Err(format!("claim id `{}` appears twice", record[id]));
        }
    }
    Ok(rows)
}

/// RFC 4180 records: commas, double-quoted cells with doubled quotes, CRLF or LF.
fn csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut cell = String::new();
    let mut chars = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .chars()
        .peekable();
    let mut quoted = false;
    let mut started = false;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    cell.push('"');
                }
                '"' => {
                    quoted = false;
                    if !matches!(chars.peek(), None | Some(',' | '\r' | '\n')) {
                        return Err("text after a closing quote".to_owned());
                    }
                }
                _ => cell.push(c),
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() => quoted = true,
            '"' => return Err("a quote inside an unquoted cell".to_owned()),
            ',' => record.push(std::mem::take(&mut cell)),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => {
                record.push(std::mem::take(&mut cell));
                records.push(std::mem::take(&mut record));
            }
            _ => cell.push(c),
        }
        started = true;
    }
    if quoted {
        return Err("an unclosed quote".to_owned());
    }
    if started && (!cell.is_empty() || !record.is_empty()) {
        record.push(cell);
        records.push(record);
    }
    Ok(records)
}

/// The front-matter `status` of a page, if it has one.
fn status(page: &str) -> Result<Option<String>, String> {
    let mut lines = page.lines();
    if lines.next().map(str::trim_end) != Some("+++") {
        return Err("no front matter".to_owned());
    }
    for line in lines {
        let line = line.trim();
        if line == "+++" {
            return Ok(None);
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim() == "status"
        {
            return Ok(Some(value.trim().trim_matches('"').to_owned()));
        }
    }
    Err("unclosed front matter".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{claims, csv, regression_problem, status};

    #[test]
    fn the_reader_handles_quotes_commas_and_line_ends() {
        let rows = csv("a,b\r\n\"x, \"\"y\"\"\",\"two\nlines\"\nlast,").unwrap();
        assert_eq!(rows[1], ["x, \"y\"", "two\nlines"]);
        assert_eq!(rows[2], ["last", ""]);
        assert!(csv("a,\"open").is_err());
        assert!(csv("a\"b,c").is_err());
        assert!(csv("\"a\"b,c").is_err());
    }

    #[test]
    fn a_register_without_a_protected_column_or_with_a_repeated_id_cannot_be_judged() {
        assert!(claims("claim_id,statement\nCL-1,text\n").is_err());
        let header = "claim_id,verification,applicability,adoption,verified_on,verified_by_role,statement,value,unit,source_id,version,locator_kind,locator,quote";
        let row = "CL-1,unchecked,pending_company_confirmation,proposed,,,s,,,S-1,1,text_section,sec. 1,q";
        assert!(claims(&format!("{header}\n{row}\n{row}\n")).is_err());
        assert!(claims(&format!("{header}\n{row},extra\n")).is_err());
        assert_eq!(claims(&format!("{header}\n{row}\n")).unwrap().len(), 1);
    }

    #[test]
    fn regression_cases_may_only_be_added() {
        let before = "[[case]]\nid = \"R-1\"\n";
        assert_eq!(
            regression_problem(before, &format!("{before}[[case]]\nid = \"R-2\"\n")),
            None
        );
        assert!(regression_problem(before, "[[case]]\nid = \"R-9\"\n").is_some());
        assert!(regression_problem(before, &format!("{before}critical = true\n")).is_some());
    }

    #[test]
    fn the_status_line_is_read_from_the_front_matter_only() {
        assert_eq!(
            status("+++\nid = \"a\"\nstatus = \"stable\"\n+++\nstatus = \"draft\"\n").unwrap(),
            Some("stable".to_owned())
        );
        assert_eq!(
            status("+++\nid = \"a\"\n+++\nstatus = \"stable\"\n").unwrap(),
            None
        );
        assert!(status("no front matter").is_err());
        assert!(status("+++\nid = \"a\"\n").is_err());
    }
}
