//! `guard-statuses`, first form: the path rule (AD-21).
//!
//! Commits by the agent author may not touch `corpus/` or any file named `seed-*`.
//! Content, statuses and the seed goldens belong to the owner and the lead. The
//! field-level rule (no raised status, no `verified_by_role`) replaces this one after
//! the `search-core-and-cli` card, and only then may the agent do upkeep in `corpus/`.

use crate::proc::git;
use std::path::Path;

/// The author name the agent commits under.
pub const AGENT_AUTHOR: &str = "Codex (agent)";

/// A commit by the agent author and the paths it changes.
struct Commit {
    sha: String,
    paths: Vec<String>,
}

fn is_protected(path: &str) -> bool {
    let file_name = path.rsplit('/').next().unwrap_or(path);
    path.starts_with("corpus/") || file_name.starts_with("seed-")
}

/// The commits in `base..head` by the agent author. An empty or all-zero `base`
/// (the first push of a branch) means every commit reachable from `head`.
fn agent_commits(repo: &Path, base: &str, head: &str) -> Result<(usize, Vec<Commit>), String> {
    for rev in [base, head] {
        if rev.starts_with('-') {
            return Err(format!("`{rev}` is not a revision"));
        }
    }
    let range = if base.chars().all(|c| c == '0') {
        head.to_owned()
    } else {
        format!("{base}..{head}")
    };
    let log = git(
        repo,
        &[
            "log",
            "-z",
            "--no-merges",
            "--format=%H%x1f%an",
            &range,
            "--",
        ],
    )?;
    let mut total = 0;
    let mut found = Vec::new();
    for record in log.split('\0').filter(|record| !record.is_empty()) {
        total += 1;
        let (sha, author) = record
            .split_once('\u{1f}')
            .ok_or("unreadable git log output")?;
        if author != AGENT_AUTHOR {
            continue;
        }
        let files = git(
            repo,
            &[
                "diff-tree",
                "-r",
                "--root",
                "--no-commit-id",
                "--name-only",
                "-z",
                sha,
            ],
        )?;
        let paths = files
            .split('\0')
            .filter(|path| !path.is_empty())
            .map(str::to_owned)
            .collect();
        found.push(Commit {
            sha: sha.to_owned(),
            paths,
        });
    }
    Ok((total, found))
}

/// One finding per protected path that an agent commit touches.
fn findings(commits: &[Commit]) -> Vec<String> {
    commits
        .iter()
        .flat_map(|commit| {
            commit
                .paths
                .iter()
                .filter(|path| is_protected(path))
                .map(move |path| {
                    format!(
                        "commit {} by \"{AGENT_AUTHOR}\" touches {path}",
                        commit.sha.get(..12).unwrap_or(&commit.sha)
                    )
                })
        })
        .collect()
}

/// Check `base..head` in `repo`. Prints the findings; true when there are none.
pub fn command(repo: &Path, base: &str, head: &str) -> Result<bool, String> {
    let (total, commits) = agent_commits(repo, base, head)?;
    let problems = findings(&commits);
    for problem in &problems {
        println!("guard-statuses: {problem}");
    }
    if problems.is_empty() {
        println!(
            "guard-statuses: ok ({total} commits checked, {} by the agent author)",
            commits.len()
        );
        return Ok(true);
    }
    println!(
        "help: content, statuses and seed goldens are changed by the owner or the lead, not by the agent"
    );
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::{AGENT_AUTHOR, command, is_protected};
    use crate::proc::git;
    use std::path::{Path, PathBuf};

    /// A git repository in a fresh temporary directory, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("fieldbook-xtask-{label}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("temporary directory");
            git(&dir, &["init", "--quiet", "-b", "main"]).expect("git init");
            Self(dir)
        }

        /// Commit `files` (path, text) as `author`; returns the new commit's id.
        fn commit(&self, author: &str, files: &[(&str, &str)]) -> String {
            for (path, text) in files {
                let full = self.0.join(path);
                std::fs::create_dir_all(full.parent().expect("a parent")).expect("directories");
                std::fs::write(full, text).expect("file");
            }
            let identity = format!("user.name={author}");
            let base = [
                "-c",
                &identity,
                "-c",
                "user.email=agent@noreply.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ];
            git(&self.0, &["add", "--all"]).expect("git add");
            let mut args = base.to_vec();
            args.extend(["commit", "--quiet", "--message", "change"]);
            git(&self.0, &args).expect("git commit");
            git(&self.0, &["rev-parse", "HEAD"])
                .expect("rev-parse")
                .trim()
                .to_owned()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn passes(repo: &Path, base: &str, head: &str) -> bool {
        command(repo, base, head).expect("the guard runs")
    }

    #[test]
    fn protected_paths_are_corpus_and_seed_files() {
        for path in [
            "corpus/pages/a.md",
            "corpus/registers/claims.csv",
            "tests/golden/search/seed-rank.json",
            "seed-1.json",
        ] {
            assert!(is_protected(path), "{path}");
        }
        for path in [
            "crates/fieldbook/src/main.rs",
            "docs/corpus.md",
            "tests/golden/search/rank-001.json",
            "tests/fixtures/corpus-mini/a.md",
            "xtask/seed.rs",
        ] {
            assert!(!is_protected(path), "{path}");
        }
    }

    // control: GS-01
    #[test]
    fn an_agent_commit_touching_the_corpus_or_a_seed_file_fails() {
        let repo = Scratch::new("guard-fails");
        let base = repo.commit("Owner", &[("README.md", "start")]);
        let code = repo.commit(AGENT_AUTHOR, &[("crates/a.rs", "fn a() {}")]);
        assert!(passes(&repo.0, &base, &code));

        let corpus = repo.commit(AGENT_AUTHOR, &[("corpus/pages/cmp/a.md", "text")]);
        assert!(!passes(&repo.0, &base, &corpus));
        assert!(passes(&repo.0, &corpus, &corpus), "an empty range is clean");

        let seed = repo.commit(
            AGENT_AUTHOR,
            &[("tests/golden/search/seed-rank.json", "[]")],
        );
        assert!(!passes(&repo.0, &corpus, &seed));
    }

    #[test]
    fn other_authors_may_touch_the_corpus() {
        let repo = Scratch::new("guard-others");
        let base = repo.commit("Owner", &[("README.md", "start")]);
        let content = repo.commit(
            "Lead",
            &[
                ("corpus/pages/cmp/a.md", "text"),
                ("tests/golden/seed-a.json", "[]"),
            ],
        );
        assert!(passes(&repo.0, &base, &content));
    }

    #[test]
    fn a_first_push_checks_every_commit() {
        let repo = Scratch::new("guard-first");
        repo.commit(AGENT_AUTHOR, &[("corpus/a.md", "text")]);
        let head = repo.commit("Owner", &[("README.md", "start")]);
        let zeros = "0".repeat(40);
        assert!(!passes(&repo.0, &zeros, &head));
        assert!(!passes(&repo.0, "", &head));
    }

    #[test]
    fn a_revision_that_looks_like_an_option_is_refused() {
        let repo = Scratch::new("guard-option");
        let head = repo.commit("Owner", &[("README.md", "start")]);
        assert!(command(&repo.0, "--all", &head).is_err());
    }
}
