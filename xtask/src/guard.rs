//! `guard-statuses`, first form: the path rule (AD-21).
//!
//! GS-01 keeps protected paths blocked except the exact reviewed foundation
//! transitions. Agent protected merges fail closed against every parent. General
//! field-level upkeep belongs to the later `guard-statuses` card (GS-02).

use crate::guard_foundation::{allows, git, is_protected};
use std::path::Path;

/// The author name the agent commits under.
pub const AGENT_AUTHOR: &str = "Codex (agent)";

/// A commit by the agent author and the paths it changes.
struct Commit {
    sha: String,
    paths: Vec<String>,
    parents: Vec<String>,
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
        &["log", "-z", "--format=%H%x1f%an%x1f%P", &range, "--"],
    )?;
    let mut total = 0;
    let mut found = Vec::new();
    for record in log.split('\0').filter(|record| !record.is_empty()) {
        total += 1;
        let fields: Vec<_> = record.split('\u{1f}').collect();
        let [sha, author, parents] = fields.as_slice() else {
            return Err("unreadable git log output".to_owned());
        };
        if *author != AGENT_AUTHOR {
            continue;
        }
        let parents: Vec<String> = parents.split_whitespace().map(str::to_owned).collect();
        let mut paths = std::collections::BTreeSet::new();
        let comparisons: Vec<Vec<&str>> = if parents.is_empty() {
            vec![vec!["--root", sha]]
        } else {
            parents
                .iter()
                .map(|parent| vec![parent.as_str(), *sha])
                .collect()
        };
        for comparison in comparisons {
            let mut args = vec![
                "diff-tree",
                "-r",
                "--no-commit-id",
                "--name-only",
                "--no-renames",
                "-z",
            ];
            args.extend(comparison);
            args.push("--");
            let files = git(repo, &args)?;
            paths.extend(
                files
                    .split('\0')
                    .filter(|path| !path.is_empty())
                    .map(str::to_owned),
            );
        }
        found.push(Commit {
            sha: (*sha).to_owned(),
            paths: paths.into_iter().collect(),
            parents,
        });
    }
    Ok((total, found))
}

/// One finding per protected path that an agent commit touches.
fn findings(repo: &Path, commits: &[Commit]) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for commit in commits {
        if commit.parents.len() == 1
            && allows(repo, &commit.parents[0], &commit.sha, &commit.paths)?
        {
            continue;
        }
        for path in commit.paths.iter().filter(|path| is_protected(path)) {
            problems.push(format!(
                "commit {} by \"{AGENT_AUTHOR}\" touches {path}",
                commit.sha.get(..12).unwrap_or(&commit.sha)
            ));
        }
    }
    Ok(problems)
}

/// Check `base..head` in `repo`. Prints the findings; true when there are none.
pub fn command(repo: &Path, base: &str, head: &str) -> Result<bool, String> {
    let (total, commits) = agent_commits(repo, base, head)?;
    let problems = findings(repo, &commits)?;
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
    use crate::guard_foundation::git;
    use std::path::{Path, PathBuf};

    /// A git repository in an atomically claimed directory; never pre-deleted.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = loop {
                let candidate = std::env::temp_dir().join(format!(
                    "fieldbook-xtask-{label}-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&candidate) {
                    Ok(()) => break candidate,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("temporary directory: {error}"),
                }
            };
            git(&dir, &["init", "--quiet", "-b", "main"]).expect("git init");
            Self(dir)
        }

        /// Commit `files` (path, text) as `author`; returns the new commit's id.
        fn write(&self, files: &[(&str, &str)]) {
            for (path, text) in files {
                let full = self.0.join(path);
                std::fs::create_dir_all(full.parent().expect("a parent")).expect("directories");
                std::fs::write(full, text).expect("file");
            }
        }

        fn commit(&self, author: &str, files: &[(&str, &str)]) -> String {
            self.write(files);
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

    const FOUNDATION: [(&str, &str); 3] = [
        (
            "corpus/registers/questions.csv",
            include_str!("../../tests/fixtures/foundation/questions.csv"),
        ),
        (
            "corpus/registers/glossary.csv",
            include_str!("../../tests/fixtures/foundation/glossary.csv"),
        ),
        (
            "corpus/pages/foundation/onboarding/fnd-onboarding-plan.md",
            include_str!("../../tests/fixtures/foundation/onboarding.md"),
        ),
    ];

    fn foundation_base() -> (Scratch, String) {
        let repo = Scratch::new("foundation");
        let questions = FOUNDATION[0].1.split_inclusive('\n').next().unwrap();
        let glossary = FOUNDATION[1].1.split_inclusive('\n').next().unwrap();
        let base = repo.commit(
            "Owner",
            &[(FOUNDATION[0].0, questions), (FOUNDATION[1].0, glossary)],
        );
        (repo, base)
    }

    #[test]
    fn exact_foundation_transition_passes() {
        let (repo, base) = foundation_base();
        let head = repo.commit(AGENT_AUTHOR, &FOUNDATION);
        assert!(passes(&repo.0, &base, &head));
    }

    #[test]
    fn exact_authority_bootstrap_passes() {
        let repo = Scratch::new("authority");
        let path = "corpus/AGENTS.md";
        let base = repo.commit(
            "Owner",
            &[(
                path,
                include_str!("../../tests/fixtures/foundation/authority-before.md"),
            )],
        );
        let head = repo.commit(
            AGENT_AUTHOR,
            &[
                (
                    path,
                    include_str!("../../tests/fixtures/foundation/authority-after.md"),
                ),
                ("AGENTS.md", "explicit temporary authority"),
            ],
        );
        assert!(passes(&repo.0, &base, &head));
    }

    #[test]
    fn agent_merge_checks_protected_differences_against_every_parent() {
        for (path, allowed) in [("corpus/a.md", false), ("docs/a.md", true)] {
            let repo = Scratch::new("merge");
            let base = repo.commit("Owner", &[("README.md", "start")]);
            git(&repo.0, &["checkout", "-q", "-b", "other"]).unwrap();
            repo.commit("Owner", &[("docs/other.md", "other")]);
            git(&repo.0, &["checkout", "-q", "main"]).unwrap();
            repo.commit("Lead", &[(path, "inherited from first parent")]);
            git(&repo.0, &["merge", "--no-ff", "--no-commit", "other"]).unwrap();
            let head = repo.commit(AGENT_AUTHOR, &[]);
            assert_eq!(passes(&repo.0, &base, &head), allowed, "{path}");
        }
    }

    #[test]
    fn changed_fields_bytes_and_partial_foundation_are_refused() {
        for (index, changed) in [
            (0, FOUNDATION[0].1.replace(",open,,,", ",open,answer,,")),
            (
                1,
                FOUNDATION[1].1.replace(
                    ",,pending_company_calibration,",
                    ",S-0010-v1,pending_company_calibration,",
                ),
            ),
            (
                2,
                FOUNDATION[2]
                    .1
                    .replace("status = \"draft\"", "status = \"stable\""),
            ),
            (2, format!("{} ", FOUNDATION[2].1)),
        ] {
            assert_ne!(changed, FOUNDATION[index].1);
            let (repo, base) = foundation_base();
            let mut files = FOUNDATION;
            files[index].1 = &changed;
            let head = repo.commit(AGENT_AUTHOR, &files);
            assert!(!passes(&repo.0, &base, &head));
        }
        let (repo, base) = foundation_base();
        let head = repo.commit(AGENT_AUTHOR, &FOUNDATION[..2]);
        assert!(!passes(&repo.0, &base, &head));
    }

    #[test]
    fn drift_later_edits_and_extra_paths_are_refused() {
        let (repo, _) = foundation_base();
        let drift = format!(
            "{}\n",
            FOUNDATION[0].1.split_inclusive('\n').next().unwrap()
        );
        let base = repo.commit("Owner", &[(FOUNDATION[0].0, &drift)]);
        let head = repo.commit(AGENT_AUTHOR, &FOUNDATION);
        assert!(!passes(&repo.0, &base, &head));
        let (repo, base) = foundation_base();
        let first = repo.commit(AGENT_AUTHOR, &FOUNDATION);
        assert!(passes(&repo.0, &base, &first));
        let later = repo.commit(AGENT_AUTHOR, &[(FOUNDATION[2].0, "later draft")]);
        assert!(!passes(&repo.0, &first, &later));
        let (repo, base) = foundation_base();
        let mut files = FOUNDATION.to_vec();
        files.push(("docs/extra.md", "not the three-file transition"));
        let head = repo.commit(AGENT_AUTHOR, &files);
        assert!(!passes(&repo.0, &base, &head));
    }

    #[test]
    fn protected_additions_deletions_and_renames_stay_blocked() {
        for path in [
            "corpus/registers/claims.csv",
            "corpus/registers/sources.csv",
            "corpus/sources/public/S-0001-v1.txt",
            "corpus/eval/regression.toml",
            "tests/golden/seed-new.json",
        ] {
            let (repo, base) = foundation_base();
            let head = repo.commit(AGENT_AUTHOR, &[(path, "synthetic refused change")]);
            assert!(!passes(&repo.0, &base, &head), "{path}");
        }
        for operation in ["rm", "mv"] {
            let (repo, base) = foundation_base();
            let mut args = vec![operation, FOUNDATION[0].0];
            if operation == "mv" {
                args.push("questions.csv");
            }
            git(&repo.0, &args).unwrap();
            let head = repo.commit(AGENT_AUTHOR, &[]);
            assert!(!passes(&repo.0, &base, &head));
        }
    }

    #[cfg(unix)]
    #[test]
    fn regular_modes_are_required_on_both_sides() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        for variant in ["before", "after", "symlink"] {
            let (repo, mut base) = foundation_base();
            if variant == "before" {
                std::fs::set_permissions(
                    repo.0.join(FOUNDATION[0].0),
                    std::fs::Permissions::from_mode(0o755),
                )
                .unwrap();
                base = repo.commit("Owner", &[]);
            }
            repo.write(&FOUNDATION);
            if variant == "before" {
                std::fs::set_permissions(
                    repo.0.join(FOUNDATION[0].0),
                    std::fs::Permissions::from_mode(0o644),
                )
                .unwrap();
            } else if variant == "after" {
                std::fs::set_permissions(
                    repo.0.join(FOUNDATION[2].0),
                    std::fs::Permissions::from_mode(0o755),
                )
                .unwrap();
            } else {
                std::fs::remove_file(repo.0.join(FOUNDATION[2].0)).unwrap();
                symlink(FOUNDATION[2].1, repo.0.join(FOUNDATION[2].0)).unwrap();
            }
            let head = repo.commit(AGENT_AUTHOR, &[]);
            assert!(!passes(&repo.0, &base, &head), "{variant}");
        }
    }

    #[test]
    fn authority_amendment_is_exact_and_the_only_protected_change() {
        let path = "corpus/AGENTS.md";
        let before = include_str!("../../tests/fixtures/foundation/authority-before.md");
        let after = include_str!("../../tests/fixtures/foundation/authority-after.md");
        for (old, new, extra) in [
            (before.to_owned(), format!("{after} "), false),
            (format!("{before} "), after.to_owned(), false),
            (before.to_owned(), after.to_owned(), true),
        ] {
            let repo = Scratch::new("authority-refused");
            let base = repo.commit("Owner", &[(path, &old)]);
            let mut files = vec![(path, new.as_str())];
            if extra {
                files.push(("corpus/other.md", "not authorized"));
            }
            let head = repo.commit(AGENT_AUTHOR, &files);
            assert!(!passes(&repo.0, &base, &head));
        }
    }

    #[test]
    fn replacement_refs_cannot_hide_a_protected_commit() {
        let repo = Scratch::new("replace");
        let base = repo.commit("Owner", &[("README.md", "start")]);
        let bad = repo.commit(AGENT_AUTHOR, &[("corpus/a.md", "blocked")]);
        git(&repo.0, &["checkout", "-q", "-b", "replacement", &base]).unwrap();
        let clean = repo.commit("Owner", &[("docs/a.md", "unprotected")]);
        git(&repo.0, &["replace", &bad, &clean]).unwrap();
        assert!(!passes(&repo.0, &base, &bad));
    }
}
