//! `denylist`: the personal denylist, run locally before a push (D-11).
//!
//! The file of terms lives outside the repository and is never read by CI. A term
//! matches without regard to case. Terms are never printed: the report names object ids only.

use crate::proc::{git, output};
use std::path::Path;

/// The terms in `text`: one per line; blank lines and `#` lines are ignored.
fn terms(text: &str) -> Vec<&str> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

fn contains_term(bytes: &[u8], terms: &[&str]) -> bool {
    let lowered = String::from_utf8_lossy(bytes).to_lowercase();
    terms
        .iter()
        .any(|term| lowered.contains(&term.to_lowercase()))
}

fn object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parse the pre-push protocol. Ref names and denied text never enter reports.
fn push_range(text: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 4 || !object_id(fields[1]) || !object_id(fields[3]) {
            return Err("denylist: malformed push update".into());
        }
        if fields[1].bytes().all(|b| b == b'0') {
            continue;
        }
        include.push(fields[1].to_owned());
        if !fields[3].bytes().all(|b| b == b'0') {
            exclude.push(fields[3].to_owned());
        }
    }
    Ok((include, exclude))
}

/// Scan bytes and paths in the working index, then every object the push adds.
/// Without hook input, scan every local branch/tag conservatively.
fn hits(repo: &Path, terms: &[&str], updates: Option<&str>) -> Result<Vec<String>, String> {
    let mut found = std::collections::BTreeSet::new();
    let index = git(repo, &["ls-files", "--stage", "-z"])
        .map_err(|_| "denylist: cannot read index".to_owned())?;
    for entry in index.split('\0').filter(|s| !s.is_empty()) {
        let (meta, path) = entry.split_once('\t').ok_or("denylist: malformed index")?;
        let oid = meta
            .split_whitespace()
            .nth(1)
            .ok_or("denylist: missing index object")?;
        let blob = output("git", &["cat-file", "blob", oid], repo)
            .map_err(|_| "denylist: cannot read index object".to_owned())?;
        if !blob.status.success() {
            return Err("denylist: cannot read index object".into());
        }
        let working = match std::fs::read(repo.join(path)) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(_) => return Err("denylist: cannot read tracked file".into()),
        };
        if contains_term(path.as_bytes(), terms)
            || contains_term(&blob.stdout, terms)
            || contains_term(&working, terms)
        {
            found.insert(format!("index object {oid}"));
        }
    }
    let (include, exclude) = match updates {
        Some(text) => push_range(text)?,
        None => {
            let refs = git(
                repo,
                &[
                    "for-each-ref",
                    "--format=%(objectname)",
                    "refs/heads",
                    "refs/tags",
                ],
            )
            .map_err(|_| "denylist: cannot enumerate local refs".to_owned())?;
            (refs.lines().map(str::to_owned).collect(), Vec::new())
        }
    };
    if include.is_empty() {
        return Ok(found.into_iter().collect());
    }
    let mut args = vec!["rev-list", "--objects", "--no-object-names"];
    args.extend(include.iter().map(String::as_str));
    // An unavailable remote tip cannot justify excluding any local history.
    let known: Vec<_> = exclude
        .iter()
        .filter(|oid| git(repo, &["cat-file", "-e", oid]).is_ok())
        .collect();
    if !known.is_empty() {
        args.push("--not");
        args.extend(known.iter().map(|s| s.as_str()));
    }
    args.push("--");
    let objects =
        git(repo, &args).map_err(|_| "denylist: cannot walk pushed history".to_owned())?;
    for oid in objects.lines() {
        if !object_id(oid) {
            return Err("denylist: malformed object walk".into());
        }
        let object = output("git", &["cat-file", "-p", oid], repo)
            .map_err(|_| "denylist: cannot read pushed object".to_owned())?;
        if !object.status.success() {
            return Err("denylist: cannot read pushed object".into());
        }
        if contains_term(&object.stdout, terms) {
            found.insert(format!("history object {oid}"));
        }
        let kind = git(repo, &["cat-file", "-t", oid])
            .map_err(|_| "denylist: cannot read object type".to_owned())?;
        if kind.trim() == "commit" {
            let paths = git(repo, &["ls-tree", "-r", "--name-only", "-z", oid])
                .map_err(|_| "denylist: cannot read historical paths".to_owned())?;
            if contains_term(paths.as_bytes(), terms) {
                found.insert(format!("history paths at {oid}"));
            }
        }
    }
    Ok(found.into_iter().collect())
}

/// Check `repo` against the terms in `file`. Prints the findings; true when clean.
pub fn command(repo: &Path, file: &Path) -> Result<bool, String> {
    check(repo, file, None)
}

/// Check exactly the refs supplied by the pre-push hook.
pub fn pushed(repo: &Path, file: &Path, updates: &Path) -> Result<bool, String> {
    let updates = std::fs::read_to_string(updates)
        .map_err(|_| "denylist: cannot read push updates".to_owned())?;
    check(repo, file, Some(&updates))
}

fn check(repo: &Path, file: &Path, updates: Option<&str>) -> Result<bool, String> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("cannot read the denylist: {error}"))?;
    let terms = terms(&text);
    if terms.is_empty() {
        println!("denylist: the file has no terms, nothing to check");
        return Ok(true);
    }
    let found = hits(repo, &terms, updates)?;
    for place in &found {
        println!("denylist: {place} contains a denied term");
    }
    if found.is_empty() {
        println!("denylist: ok ({} terms)", terms.len());
    }
    Ok(found.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{command, hits, push_range, terms};
    use crate::proc::git;
    use std::path::PathBuf;

    #[test]
    fn blank_lines_and_comments_are_not_terms() {
        assert_eq!(
            terms("# private\n\n  Zebra Corp \nplain\n"),
            vec!["Zebra Corp", "plain"]
        );
    }

    // control: SC-04
    #[test]
    fn a_term_in_a_tracked_file_or_an_unpushed_commit_fails() {
        let dir: PathBuf =
            std::env::temp_dir().join(format!("fieldbook-xtask-denylist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).expect("directories");
        let list = dir.join("denylist.txt");
        std::fs::write(&list, "# terms\n\nzebra corp\n").expect("denylist");
        git(&repo, &["init", "--quiet", "-b", "main"]).expect("git init");
        std::fs::write(repo.join("notes.md"), "Reference text about ZEBRA CORP.").expect("file");
        git(&repo, &["add", "--all"]).expect("git add");
        assert!(
            !command(&repo, &list).expect("runs"),
            "a tracked file with a term fails"
        );

        std::fs::write(repo.join("notes.md"), "Reference text.").expect("file");
        git(&repo, &["add", "--all"]).expect("git add");
        assert!(command(&repo, &list).expect("runs"), "a clean tree passes");

        let commit = [
            "-c",
            "user.name=Owner",
            "-c",
            "user.email=owner@noreply.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--quiet",
            "-m",
            "Notes on Zebra Corp",
        ];
        git(&repo, &commit).expect("git commit");
        assert!(
            !command(&repo, &list).expect("runs"),
            "a term in an unpushed message fails"
        );
        // Use independent terms so the earlier message cannot satisfy these checks.
        let base = git(&repo, &["rev-parse", "HEAD"]).expect("base id");
        let mut clean_commit = commit;
        clean_commit[11] = "Synthetic change";
        std::fs::write(repo.join("history.txt"), "HISTORICAL-PAYLOAD-CANARY")
            .expect("historic file");
        std::fs::write(repo.join("path-only-canary.txt"), "Reference text.")
            .expect("historic path");
        git(&repo, &["add", "--all"]).expect("stage history");
        git(&repo, &clean_commit).expect("history commit");
        let historic = git(&repo, &["rev-parse", "HEAD"]).expect("history id");
        let blob = git(&repo, &["rev-parse", "HEAD:history.txt"]).expect("historic blob");
        std::fs::remove_file(repo.join("history.txt")).expect("remove history");
        std::fs::remove_file(repo.join("path-only-canary.txt")).expect("remove path");
        git(&repo, &["add", "--all"]).expect("stage deletion");
        git(&repo, &clean_commit).expect("deletion commit");
        let tip = git(&repo, &["rev-parse", "HEAD"]).expect("tip");
        let zero = "0".repeat(40);
        let updates = format!(
            "refs/heads/other {} refs/heads/other {}\n",
            tip.trim(),
            base.trim()
        );
        let result = hits(&repo, &["historical-payload-canary"], Some(&updates))
            .expect("scan deleted bytes");
        assert!(result.contains(&format!("history object {}", blob.trim())));
        assert!(result.iter().all(|s| !s.contains("canary")));
        assert!(
            hits(&repo, &["path-only-canary"], Some(&updates))
                .expect("scan deleted path")
                .iter()
                .any(|s| s.starts_with("history paths at"))
        );
        // Move HEAD before the planted history: only the supplied ref reaches it.
        git(&repo, &["tag", "synthetic-history", historic.trim()]).expect("tag");
        git(&repo, &["checkout", "--quiet", "--detach", base.trim()]).expect("clean detached head");
        let tagged = format!(
            "refs/tags/synthetic-history {} refs/tags/synthetic-history {zero}\n",
            historic.trim()
        );
        assert!(
            hits(&repo, &["historical-payload-canary"], Some(&tagged))
                .expect("non-HEAD tag scan")
                .contains(&format!("history object {}", blob.trim()))
        );
        let branch = format!("refs/heads/other {} refs/heads/other {zero}\n", tip.trim());
        assert!(
            hits(&repo, &["historical-payload-canary"], Some(&branch))
                .expect("non-HEAD branch scan")
                .contains(&format!("history object {}", blob.trim()))
        );
        let mut annotation = commit[..8].to_vec();
        annotation.extend([
            "-c",
            "tag.gpgsign=false",
            "tag",
            "-a",
            "synthetic-note",
            "-m",
            "ANNOTATION-ONLY-CANARY",
        ]);
        git(&repo, &annotation).expect("annotated tag");
        let tag = git(&repo, &["rev-parse", "refs/tags/synthetic-note"]).expect("tag object");
        let annotated = format!(
            "refs/tags/synthetic-note {} refs/tags/synthetic-note {zero}\n",
            tag.trim()
        );
        assert!(
            hits(&repo, &["annotation-only-canary"], Some(&annotated))
                .expect("tag annotation scan")
                .contains(&format!("history object {}", tag.trim()))
        );
        let deletion = format!("(delete) {zero} refs/heads/other {}\n", tip.trim());
        assert!(
            hits(&repo, &["historical-payload-canary"], Some(&deletion))
                .expect("deletion scan")
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_push_updates_are_refused() {
        assert!(push_range("refs/heads/main bad refs/heads/main bad").is_err());
        assert!(push_range("").expect("empty").0.is_empty());
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_pass() {
        let repo = std::env::temp_dir();
        assert!(command(&repo, &repo.join("fieldbook-no-such-denylist.txt")).is_err());
    }
}
