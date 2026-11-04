//! `guard-statuses`: the content rule (GS-02, AD-21).
//!
//! An agent may change the Corpus: add originals, sources, claims, questions and
//! pages. It may not certify its own work: raise a claim's `verification`,
//! `applicability` or `adoption`, set `verified_on` or `verified_by_role`, keep a
//! check through an evidence edit, set a page `stable`, change an existing seed
//! golden, add a seed golden beside the code it checks, or edit, remove or mark
//! critical a regression case. Nor may it rewrite or delete a byte-exact original.
//! Merges are judged against every parent.

use crate::guard_fields::{REGRESSION, entry, git, is_seed, judge};
use std::path::Path;

/// A commit is the agent's when its author name ends in "(agent)" or it carries a
/// `Co-authored-by` trailer, which is how an assistant marks a commit it made in
/// the owner's checkout. This stops mistakes, not a determined actor.
fn is_agent(author: &str, coauthors: &str) -> bool {
    author.ends_with("(agent)") || !coauthors.trim().is_empty()
}

/// An agent commit and the paths it changes against any parent.
struct Commit {
    sha: String,
    paths: Vec<String>,
    parents: Vec<String>,
}

/// The commits in `base..head` that are the agent's. An empty or all-zero `base`
/// (the first push of a branch) means every commit reachable from `head`.
fn agent_commits(repo: &Path, range: &str) -> Result<(usize, Vec<Commit>), String> {
    let log = git(
        repo,
        &[
            "log",
            "-z",
            "--format=%H%x1f%an%x1f%P%x1f%(trailers:key=Co-authored-by,valueonly,separator=%x1e)",
            range,
            "--",
        ],
    )?;
    let mut total = 0;
    let mut found = Vec::new();
    for record in log.split('\0').filter(|record| !record.is_empty()) {
        total += 1;
        let fields: Vec<_> = record.split('\u{1f}').collect();
        let [sha, author, parents, coauthors] = fields.as_slice() else {
            return Err("unreadable git log output".to_owned());
        };
        if !is_agent(author, coauthors) {
            continue;
        }
        let sha = sha.trim();
        let parents: Vec<String> = parents.split_whitespace().map(str::to_owned).collect();
        let mut paths = std::collections::BTreeSet::new();
        let comparisons: Vec<Vec<&str>> = if parents.is_empty() {
            vec![vec!["--root", sha]]
        } else {
            parents
                .iter()
                .map(|parent| vec![parent.as_str(), sha])
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
            paths.extend(
                git(repo, &args)?
                    .split('\0')
                    .filter(|path| !path.is_empty())
                    .map(str::to_owned),
            );
        }
        found.push(Commit {
            sha: sha.to_owned(),
            paths: paths.into_iter().collect(),
            parents,
        });
    }
    Ok((total, found))
}

/// The findings for the agent's commits. A new seed golden must arrive in a range
/// that changes nothing outside `tests/golden/`, so that it is written before, and
/// apart from, the code it checks.
fn findings(
    repo: &Path,
    range_paths: &[String],
    commits: &[Commit],
) -> Result<Vec<String>, String> {
    let mut problems = Vec::new();
    for commit in commits {
        let sha = commit.sha.get(..12).unwrap_or(&commit.sha);
        for path in &commit.paths {
            let after = entry(repo, &commit.sha, path)?;
            let mut before = Vec::new();
            for parent in &commit.parents {
                before.push(entry(repo, parent, path)?);
            }
            if before.is_empty() {
                before.push(None);
            }
            if before.contains(&after) {
                continue;
            }
            let verdict = judge(repo, sha, path, &before, after.as_ref())?;
            problems.extend(verdict.problems);
            if verdict.new_seed
                && let Some(other) = range_paths
                    .iter()
                    .find(|other| !is_seed(other) && !other.starts_with("tests/golden/"))
            {
                problems.push(format!(
                    "{sha} {path}: a new seed golden lands in a pull request of its own, not beside {other}"
                ));
            }
        }
    }
    Ok(problems)
}

/// Check `base..head` in `repo`. Prints the findings; true when there are none.
pub fn command(repo: &Path, base: &str, head: &str) -> Result<bool, String> {
    for rev in [base, head] {
        if rev.starts_with('-') {
            return Err(format!("`{rev}` is not a revision"));
        }
    }
    let first_push = base.chars().all(|c| c == '0');
    let range = if first_push {
        head.to_owned()
    } else {
        format!("{base}..{head}")
    };
    let (total, commits) = agent_commits(repo, &range)?;
    let listing = if first_push {
        git(repo, &["ls-tree", "-r", "-z", "--name-only", head, "--"])?
    } else {
        git(
            repo,
            &[
                "diff",
                "--name-only",
                "--no-renames",
                "-z",
                base,
                head,
                "--",
            ],
        )?
    };
    let range_paths: Vec<String> = listing
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect();
    let problems = findings(repo, &range_paths, &commits)?;
    for problem in &problems {
        println!("guard-statuses: {problem}");
    }
    if problems.is_empty() {
        println!(
            "guard-statuses: ok ({total} commits checked, {} by an agent)",
            commits.len()
        );
        return Ok(true);
    }
    println!(
        "help: only the owner certifies a claim or a stable page; an agent never weakens a seed golden or {REGRESSION}"
    );
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::{command, is_agent};
    use crate::guard_fields::git;
    use std::path::{Path, PathBuf};

    /// The author name Codex commits under.
    const AGENT_AUTHOR: &str = "Codex (agent)";

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
            git(&dir, &["config", "user.name", "Synthetic role"]).expect("local name");
            git(&dir, &["config", "user.email", "agent@noreply.invalid"]).expect("local email");
            Self(dir)
        }

        fn write(&self, files: &[(&str, &str)]) {
            for (path, text) in files {
                let full = self.0.join(path);
                std::fs::create_dir_all(full.parent().expect("a parent")).expect("directories");
                std::fs::write(full, text).expect("file");
            }
        }

        /// Commit `files` (path, text) as `author` with `message`; returns the commit id.
        fn commit_with(&self, author: &str, message: &str, files: &[(&str, &str)]) -> String {
            self.write(files);
            let identity = format!("user.name={author}");
            let mut args = vec![
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
            args.extend(["commit", "--quiet", "--allow-empty", "--message", message]);
            git(&self.0, &args).expect("git commit");
            git(&self.0, &["rev-parse", "HEAD"])
                .expect("rev-parse")
                .trim()
                .to_owned()
        }

        fn commit(&self, author: &str, files: &[(&str, &str)]) -> String {
            self.commit_with(author, "change", files)
        }
    }

    fn passes(repo: &Path, base: &str, head: &str) -> bool {
        command(repo, base, head).expect("the guard runs")
    }

    const CLAIMS: &str = "corpus/registers/claims.csv";
    const HEADER: &str = "claim_id,statement,value,unit,source_id,version,locator_kind,locator,quote,drafted_by,verification,applicability,adoption,verified_on,verified_by_role,note";
    const CHECKED: [(&str, &str); 3] = [
        ("verification", "primary_source_supported"),
        ("verified_on", "2026-01-05"),
        ("verified_by_role", "owner"),
    ];
    const ASSISTED: &str =
        "change\n\nCo-Authored-By: Synthetic Assistant <assistant@noreply.invalid>";

    /// One synthetic claim row: entry values unless `changes` names a column.
    fn claim(id: &str, changes: &[(&str, &str)]) -> String {
        let defaults = [
            ("claim_id", id),
            ("statement", "A synthetic rule applies above 10 units."),
            ("value", "10"),
            ("unit", "USD"),
            ("source_id", "S-0001"),
            ("version", "1"),
            ("locator_kind", "text_section"),
            ("locator", "sec. 1"),
            ("quote", "applies above 10 units, as \"stated\""),
            ("drafted_by", "codex"),
            ("verification", "unchecked"),
            ("applicability", "pending_company_confirmation"),
            ("adoption", "proposed"),
            ("verified_on", ""),
            ("verified_by_role", ""),
            ("note", ""),
        ];
        defaults
            .iter()
            .map(|(column, default)| {
                let value = changes
                    .iter()
                    .find(|(name, _)| name == column)
                    .map_or(*default, |(_, value)| *value);
                if value.contains([',', '"']) {
                    format!("\"{}\"", value.replace('"', "\"\""))
                } else {
                    value.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    fn register(rows: &[String]) -> String {
        format!("{HEADER}\n{}\n", rows.join("\n"))
    }

    fn page(status: &str) -> String {
        format!("+++\nid = \"syn-a\"\nstatus = \"{status}\"\n+++\n\n## Purpose\n\nSynthetic.\n")
    }

    /// A repository whose owner commit holds one checked claim and one entry claim.
    fn checked_base(label: &str) -> (Scratch, String) {
        let repo = Scratch::new(label);
        let rows = [claim("CL-SYN-0001", &CHECKED), claim("CL-SYN-0002", &[])];
        let base = repo.commit("Owner", &[(CLAIMS, &register(&rows))]);
        (repo, base)
    }

    #[test]
    fn an_agent_is_the_agent_author_or_a_commit_with_a_co_author() {
        assert!(is_agent(AGENT_AUTHOR, ""));
        assert!(is_agent("Another (agent)", ""));
        assert!(is_agent(
            "Owner",
            "Synthetic Assistant <assistant@noreply.invalid>"
        ));
        assert!(!is_agent("Owner", "\n"));
    }

    #[test]
    fn an_agent_may_add_originals_sources_claims_and_pages() {
        let repo = Scratch::new("upkeep");
        let base = repo.commit("Owner", &[("README.md", "start")]);
        let head = repo.commit(
            AGENT_AUTHOR,
            &[
                ("corpus/sources/public/S-0001-v1.txt", "synthetic original"),
                (
                    "corpus/registers/sources.csv",
                    "source_id,version\nS-0001,1\n",
                ),
                (CLAIMS, &register(&[claim("CL-SYN-0001", &[])])),
                ("corpus/pages/syn/syn-a.md", &page("draft")),
                ("corpus/AGENTS.md", "rules"),
            ],
        );
        assert!(passes(&repo.0, &base, &head));
        let edit = repo.commit_with(
            "Owner",
            ASSISTED,
            &[("corpus/pages/syn/syn-a.md", &page("reviewed"))],
        );
        assert!(passes(&repo.0, &head, &edit));
    }

    // control: GS-02
    #[test]
    fn an_agent_setting_verified_by_role_fails() {
        let (repo, base) = checked_base("verified-by-role");
        let rows = [
            claim("CL-SYN-0001", &CHECKED),
            claim("CL-SYN-0002", &[("verified_by_role", "owner")]),
        ];
        let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
        assert!(!passes(&repo.0, &base, &head));
    }

    #[test]
    fn an_agent_raising_any_graded_field_or_the_check_date_fails() {
        for change in [
            ("verification", "primary_source_supported"),
            ("applicability", "general_reference"),
            ("adoption", "prototype_reviewed"),
            ("verified_on", "2026-01-06"),
        ] {
            let (repo, base) = checked_base("raise");
            let rows = [
                claim("CL-SYN-0001", &CHECKED),
                claim("CL-SYN-0002", &[change]),
            ];
            let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
            assert!(!passes(&repo.0, &base, &head), "{change:?}");
        }
    }

    #[test]
    fn an_agent_adding_a_raised_row_fails() {
        for change in [
            ("verification", "primary_source_supported"),
            ("applicability", "general_reference"),
            ("adoption", "prototype_reviewed"),
            ("verified_on", "2026-01-06"),
            ("verified_by_role", "lead"),
        ] {
            let (repo, base) = checked_base("raised-row");
            let rows = [
                claim("CL-SYN-0001", &CHECKED),
                claim("CL-SYN-0002", &[]),
                claim("CL-SYN-0003", &[change]),
            ];
            let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
            assert!(!passes(&repo.0, &base, &head), "{change:?}");
        }
    }

    #[test]
    fn an_assistant_commit_under_the_owner_name_cannot_certify() {
        let (repo, base) = checked_base("assisted");
        let rows = [
            claim("CL-SYN-0001", &CHECKED),
            claim("CL-SYN-0002", &CHECKED),
        ];
        let head = repo.commit_with("Owner", ASSISTED, &[(CLAIMS, &register(&rows))]);
        assert!(!passes(&repo.0, &base, &head));
    }

    #[test]
    fn the_owner_may_certify_and_an_agent_may_reset() {
        let (repo, base) = checked_base("owner-raise");
        let rows = [
            claim("CL-SYN-0001", &CHECKED),
            claim("CL-SYN-0002", &CHECKED),
        ];
        let owner = repo.commit("Owner", &[(CLAIMS, &register(&rows))]);
        assert!(passes(&repo.0, &base, &owner));
        let rows = [claim("CL-SYN-0001", &[]), claim("CL-SYN-0002", &CHECKED)];
        let reset = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
        assert!(passes(&repo.0, &owner, &reset));
        let rows = [claim("CL-SYN-0002", &CHECKED)];
        let deleted = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
        assert!(passes(&repo.0, &reset, &deleted), "a deleted row passes");
    }

    // control: GS-02
    #[test]
    fn statement_edit_on_verified_claim_fails() {
        for (field, value) in [
            ("statement", "A synthetic rule applies above 20 units."),
            ("value", "20"),
            ("unit", "EUR"),
            ("source_id", "S-0002"),
            ("version", "2"),
            ("locator_kind", "pdf_page_box"),
            ("locator", "sec. 2"),
            ("quote", "applies above 20 units"),
        ] {
            let (repo, base) = checked_base("evidence");
            let mut edited = CHECKED.to_vec();
            edited.push((field, value));
            let rows = [claim("CL-SYN-0001", &edited), claim("CL-SYN-0002", &[])];
            let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
            assert!(!passes(&repo.0, &base, &head), "{field}");
        }
    }

    #[test]
    fn the_same_edit_with_a_reset_passes_and_an_unchecked_claim_is_free() {
        let (repo, base) = checked_base("evidence-reset");
        let statement = ("statement", "A synthetic rule applies above 20 units.");
        let rows = [
            claim("CL-SYN-0001", &[statement]),
            claim("CL-SYN-0002", &[statement]),
        ];
        let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
        assert!(passes(&repo.0, &base, &head));
        let (repo, base) = checked_base("evidence-half-reset");
        let half = [("verified_on", "2026-01-05"), statement];
        let rows = [claim("CL-SYN-0001", &half), claim("CL-SYN-0002", &[])];
        let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
        assert!(
            !passes(&repo.0, &base, &head),
            "the verified fields stay set"
        );
    }

    #[test]
    fn a_register_that_does_not_parse_cannot_be_judged() {
        let (repo, base) = checked_base("unparsed");
        let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, "claim_id,statement\n\"open\n")]);
        assert!(!passes(&repo.0, &base, &head));
        let gone = repo.commit(
            "Owner",
            &[(CLAIMS, &register(&[claim("CL-SYN-0001", &[])]))],
        );
        git(&repo.0, &["rm", "-q", CLAIMS]).unwrap();
        let deleted = repo.commit(AGENT_AUTHOR, &[]);
        assert!(
            passes(&repo.0, &gone, &deleted),
            "a deleted register passes"
        );
    }

    // control: GS-02
    #[test]
    fn an_agent_setting_a_page_stable_fails() {
        let repo = Scratch::new("stable");
        let path = "corpus/pages/syn/syn-a.md";
        let base = repo.commit("Owner", &[(path, &page("reviewed"))]);
        let head = repo.commit(AGENT_AUTHOR, &[(path, &page("stable"))]);
        assert!(!passes(&repo.0, &base, &head));
        let owner = repo.commit("Owner", &[(path, &page("stable"))]);
        let edit = repo.commit(
            AGENT_AUTHOR,
            &[(path, &format!("{}A typo fixed.\n", page("stable")))],
        );
        assert!(passes(&repo.0, &owner, &edit), "a page already stable");
    }

    #[test]
    fn an_agent_cannot_raise_status_with_valid_toml_spellings() {
        let mut allowed = Vec::new();
        for (index, line) in [
            "status = 'stable'",
            "status = \"stable\" # synthetic comment",
            "\"status\" = \"stable\"",
            "'status' = 'stable'",
            r#"status = "\u0073table""#,
        ]
        .iter()
        .enumerate()
        {
            let repo = Scratch::new(&format!("toml-status-{index}"));
            let path = "corpus/pages/syn/syn-a.md";
            let base = repo.commit("Owner", &[(path, &page("reviewed"))]);
            let changed = page("stable").replace("status = \"stable\"", line);
            let head = repo.commit(AGENT_AUTHOR, &[(path, &changed)]);
            if passes(&repo.0, &base, &head) {
                allowed.push(*line);
            }
        }
        assert!(
            allowed.is_empty(),
            "guard allowed stable promotion: {allowed:?}"
        );
    }

    #[test]
    fn a_duplicate_protected_header_cannot_be_judged() {
        let repo = Scratch::new("duplicate-protected-header");
        let base = repo.commit(
            "Owner",
            &[(CLAIMS, &register(&[claim("CL-SYN-0001", &[])]))],
        );
        let changed = format!(
            "{HEADER},verification\n{},verified\n",
            claim("CL-SYN-0001", &[])
        );
        let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &changed)]);
        assert!(!passes(&repo.0, &base, &head));
    }

    #[test]
    fn an_existing_seed_golden_is_never_changed_by_an_agent() {
        let seed = "tests/golden/search/seed-rank.json";
        for remove in [false, true] {
            let repo = Scratch::new("seed");
            let base = repo.commit("Owner", &[(seed, "[]")]);
            if remove {
                git(&repo.0, &["rm", "-q", seed]).unwrap();
            }
            let files: &[(&str, &str)] = if remove { &[] } else { &[(seed, "[1]")] };
            let head = repo.commit(AGENT_AUTHOR, files);
            assert!(!passes(&repo.0, &base, &head), "remove: {remove}");
        }
    }

    #[test]
    fn a_new_seed_golden_lands_apart_from_code() {
        let repo = Scratch::new("new-seed");
        let base = repo.commit("Owner", &[("README.md", "start")]);
        let seed = repo.commit(
            AGENT_AUTHOR,
            &[
                ("tests/golden/search/seed-ranked.json", "[]"),
                ("tests/golden/search/rank-001.json", "[]"),
            ],
        );
        assert!(passes(&repo.0, &base, &seed));
        let code = repo.commit(AGENT_AUTHOR, &[("crates/a.rs", "fn a() {}")]);
        assert!(!passes(&repo.0, &base, &code), "seed and code in one range");
        assert!(passes(&repo.0, &seed, &code));
    }

    #[test]
    fn an_original_is_added_but_never_rewritten_or_deleted() {
        let path = "corpus/sources/public/S-0001-v1.pdf";
        for remove in [false, true] {
            let repo = Scratch::new("original");
            let base = repo.commit("Owner", &[(path, "synthetic original")]);
            if remove {
                git(&repo.0, &["rm", "-q", path]).unwrap();
            }
            let files: &[(&str, &str)] = if remove { &[] } else { &[(path, "rewritten")] };
            let head = repo.commit(AGENT_AUTHOR, files);
            assert!(!passes(&repo.0, &base, &head), "remove: {remove}");
            let next = repo.commit(
                AGENT_AUTHOR,
                &[("corpus/sources/public/S-0001-v2.pdf", "next version")],
            );
            assert!(passes(&repo.0, &head, &next));
        }
    }

    #[test]
    fn regression_cases_are_only_appended_and_never_marked_critical() {
        let path = "corpus/eval/regression.toml";
        let case = "[[case]]\nid = \"RG-001\"\nquery = \"synthetic\"\n";
        let more = format!("{case}\n[[case]]\nid = \"RG-002\"\nquery = \"other\"\n");
        let marked = format!("{more}critical = true\n");
        let widened = format!("{case}top = 1000000\n");
        for (after, allowed) in [
            (more.as_str(), true),
            ("[[case]]\nid = \"RG-002\"\n", false),
            (marked.as_str(), false),
            (widened.as_str(), false),
        ] {
            let repo = Scratch::new("regression");
            let base = repo.commit("Owner", &[(path, case)]);
            let head = repo.commit(AGENT_AUTHOR, &[(path, after)]);
            assert_eq!(passes(&repo.0, &base, &head), allowed, "{after}");
        }
        let repo = Scratch::new("regression-deleted");
        let base = repo.commit("Owner", &[(path, case)]);
        git(&repo.0, &["rm", "-q", path]).unwrap();
        let head = repo.commit(AGENT_AUTHOR, &[]);
        assert!(!passes(&repo.0, &base, &head));
    }

    #[test]
    fn agent_merge_keeps_owner_status_and_refuses_a_resolution_that_raises() {
        for (resolution, allowed) in [("owner", true), ("raised", false)] {
            let (repo, base) = checked_base("merge");
            git(&repo.0, &["checkout", "-q", "-b", "agent"]).unwrap();
            let rows = [
                claim("CL-SYN-0001", &CHECKED),
                claim("CL-SYN-0002", &[]),
                claim("CL-SYN-0003", &[]),
            ];
            repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
            git(&repo.0, &["checkout", "-q", "main"]).unwrap();
            let rows = [
                claim("CL-SYN-0001", &CHECKED),
                claim("CL-SYN-0002", &CHECKED),
            ];
            repo.commit("Owner", &[(CLAIMS, &register(&rows))]);
            git(&repo.0, &["checkout", "-q", "agent"]).unwrap();
            let _ = git(&repo.0, &["merge", "--no-ff", "--no-commit", "main"]);
            let third = if resolution == "owner" {
                &[][..]
            } else {
                &CHECKED[..]
            };
            let rows = [
                claim("CL-SYN-0001", &CHECKED),
                claim("CL-SYN-0002", &CHECKED),
                claim("CL-SYN-0003", third),
            ];
            let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
            assert_eq!(passes(&repo.0, &base, &head), allowed, "{resolution}");
        }
    }

    #[test]
    fn agent_merge_preserves_owner_verification_when_merging_independent_claim_reset() {
        let repo = Scratch::new("merge-independent-claim");
        let base_row = claim("CL-SYN-0001", &[("adoption", "company_standard")]);
        let base = repo.commit("Owner", &[(CLAIMS, &register(&[base_row]))]);
        git(&repo.0, &["checkout", "-q", "-b", "agent"]).unwrap();
        let agent_row = claim("CL-SYN-0001", &[("adoption", "proposed")]);
        repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&[agent_row]))]);
        git(&repo.0, &["checkout", "-q", "main"]).unwrap();
        let mut owner_fields = CHECKED.to_vec();
        owner_fields.push(("adoption", "company_standard"));
        let owner_row = claim("CL-SYN-0001", &owner_fields);
        repo.commit("Owner", &[(CLAIMS, &register(&[owner_row]))]);
        git(&repo.0, &["checkout", "-q", "agent"]).unwrap();
        let _ = git(&repo.0, &["merge", "--no-ff", "--no-commit", "main"]);
        let merged_row = claim("CL-SYN-0001", &CHECKED);
        let head = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&[merged_row]))]);
        assert!(passes(&repo.0, &base, &head));

        let mut recombined = CHECKED.to_vec();
        recombined.push(("statement", "A synthetic rule applies above 20 units."));
        let bad_head = repo.commit(
            AGENT_AUTHOR,
            &[(CLAIMS, &register(&[claim("CL-SYN-0001", &recombined)]))],
        );
        assert!(!passes(&repo.0, &base, &bad_head));
    }

    #[test]
    fn a_first_push_checks_every_commit() {
        let repo = Scratch::new("guard-first");
        repo.commit(
            AGENT_AUTHOR,
            &[(CLAIMS, &register(&[claim("CL-SYN-0001", &CHECKED)]))],
        );
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

    #[test]
    fn replacement_refs_cannot_hide_a_raise() {
        let (repo, base) = checked_base("replace");
        let rows = [
            claim("CL-SYN-0001", &CHECKED),
            claim("CL-SYN-0002", &CHECKED),
        ];
        let bad = repo.commit(AGENT_AUTHOR, &[(CLAIMS, &register(&rows))]);
        git(&repo.0, &["checkout", "-q", "-b", "replacement", &base]).unwrap();
        let clean = repo.commit("Owner", &[("docs/a.md", "unprotected")]);
        git(&repo.0, &["replace", &bad, &clean]).unwrap();
        assert!(!passes(&repo.0, &base, &bad));
    }
}
