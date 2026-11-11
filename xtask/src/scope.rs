//! `scope BASE HEAD`: a pull request that deletes a card changes only the paths the card lists (`docs-checks`).

use crate::cards::{Card, ScopePath, covers, parse_scope};
use std::path::Path;

/// The outcome of checking the pull request scope.
#[derive(Debug, PartialEq, Eq)]
pub enum ScopeOutcome {
    /// No card file was deleted, or the base was empty or all zeros.
    Skipped,
    /// Scope check passed for the given card slug and number of changed paths.
    Ok {
        /// The slug of the deleted card.
        slug: String,
        /// The number of changed paths in the range.
        paths: usize,
    },
    /// Scope check failed with diagnostic error messages.
    Fail(Vec<String>),
}

fn is_empty_or_zeros(s: &str) -> bool {
    s.is_empty() || s.chars().all(|c| c == '0')
}

fn is_card_file(path: &str) -> bool {
    let clean = path.replace('\\', "/");
    let Some(rest) = clean.strip_prefix("docs/cards/") else {
        return false;
    };
    if rest == "README.md" || rest.starts_with("drafts/") || rest.contains('/') {
        return false;
    }
    rest.ends_with(".md")
}

fn is_exempt(path: &str, card_file: &str) -> bool {
    if path == "Cargo.lock" || path == "CHANGELOG.md" || path == card_file {
        return true;
    }
    crate::docs::GENERATORS.iter().any(|g| g.file == path)
}

fn find_line_for_error(card_text: &str, err: &str, default_line: usize) -> usize {
    if let Some(open) = err.find('`')
        && let Some(close) = err[open + 1..].find('`')
    {
        let token = &err[open + 1..open + 1 + close];
        let needle = format!("`{token}`");
        for (idx, line) in card_text.lines().enumerate() {
            if line.contains(&needle) || line.contains(token) {
                return idx + 1;
            }
        }
    }
    default_line
}

fn find_bullet<'a>(text: &'a str, label: &str) -> Option<(usize, &'a str)> {
    let prefix = format!("- {label}:");
    let mut start_line = 0;
    let mut first_line: Option<&str> = None;
    let mut last_line: Option<&str> = None;

    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if first_line.is_none() {
            if trimmed.starts_with(&prefix) {
                first_line = Some(line);
                last_line = Some(line);
                start_line = idx + 1;
            }
        } else if trimmed.starts_with("- ") || trimmed.starts_with('#') {
            break;
        } else if line.starts_with(' ') || line.starts_with('\t') {
            last_line = Some(line);
        } else {
            break;
        }
    }

    let first = first_line?;
    let last = last_line?;
    let start_offset = first.as_ptr() as usize - text.as_ptr() as usize;
    let end_offset = (last.as_ptr() as usize + last.len()) - text.as_ptr() as usize;
    Some((start_line, &text[start_offset..end_offset]))
}

/// Check changed paths against declared scope paths and exemptions.
///
/// Returns the list of changed paths that are not covered by any declared scope path
/// and are not exempt.
pub fn check_paths(declared: &[ScopePath], changed: &[String], card_file: &str) -> Vec<String> {
    let mut undeclared = Vec::new();

    for path in changed {
        if is_exempt(path, card_file) {
            continue;
        }

        let is_covered = declared.iter().any(|scope| covers(scope, path));
        if !is_covered && !undeclared.contains(path) {
            undeclared.push(path.clone());
        }
    }

    undeclared
}

/// Parse declared scope paths from the `Files:` and `Registration lines:` bullets of card text.
///
/// Returns the list of declared `ScopePath`s and any syntax or wildcard/placeholder errors.
pub fn declared_paths(card_text: &str) -> (Vec<ScopePath>, Vec<String>) {
    let card = Card {
        slug: String::new(),
        path: String::new(),
        draft: false,
        text: card_text.to_owned(),
    };

    let mut paths = Vec::new();
    let mut errors = Vec::new();

    let files_bullet = card
        .bullet("Scope", "Files")
        .or_else(|| find_bullet(card_text, "Files"));

    if let Some((files_line, bullet_text)) = files_bullet {
        let (f_paths, f_errors) = parse_scope(bullet_text);
        paths.extend(f_paths);
        for err in f_errors {
            let line = find_line_for_error(card_text, &err, files_line);
            errors.push(format!("{line}: {err}"));
        }
    }

    let reg_bullet = card
        .bullet("Scope", "Registration lines")
        .or_else(|| find_bullet(card_text, "Registration lines"));

    if let Some((reg_line, bullet_text)) = reg_bullet {
        let (r_paths, r_errors) = parse_scope(bullet_text);
        paths.extend(r_paths);
        for err in r_errors {
            let line = find_line_for_error(card_text, &err, reg_line);
            errors.push(format!("{line}: {err}"));
        }
    }

    let mut unique_paths = Vec::new();
    for p in paths {
        if !unique_paths.contains(&p) {
            unique_paths.push(p);
        }
    }

    (unique_paths, errors)
}

/// Check the scope of changes in the git revision range `base..head`.
pub fn run(root: &Path, base: &str, head: &str) -> Result<ScopeOutcome, String> {
    if is_empty_or_zeros(base) {
        return Ok(ScopeOutcome::Skipped);
    }
    for rev in [base, head] {
        if rev.starts_with('-') {
            return Err(format!("`{rev}` is not a revision"));
        }
    }

    let diff_d = crate::proc::git(
        root,
        &[
            "diff",
            "--no-renames",
            "--name-only",
            "--diff-filter=D",
            base,
            head,
        ],
    )?;

    let mut deleted_cards: Vec<String> = diff_d
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && is_card_file(line))
        .map(str::to_owned)
        .collect();

    deleted_cards.sort();
    deleted_cards.dedup();

    if deleted_cards.is_empty() {
        return Ok(ScopeOutcome::Skipped);
    }

    if deleted_cards.len() > 1 {
        let names: Vec<&str> = deleted_cards
            .iter()
            .map(|p| p.strip_prefix("docs/cards/").unwrap_or(p))
            .collect();
        let msg = format!(
            "scope: one pull request closes one card, but the range deletes: {}",
            names.join(", ")
        );
        return Ok(ScopeOutcome::Fail(vec![msg]));
    }

    let card_file = &deleted_cards[0];
    let slug = card_file
        .strip_prefix("docs/cards/")
        .unwrap_or(card_file)
        .strip_suffix(".md")
        .unwrap_or(card_file)
        .to_owned();

    let range = format!("{base}..{head}");
    let log_out = crate::proc::git(
        root,
        &[
            "log",
            "--diff-filter=D",
            "--format=%H",
            &range,
            "--",
            card_file,
        ],
    )?;

    let deleting_commit = log_out
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| format!("no deleting commit found for {card_file} in {range}"))?;

    let parent_spec = format!("{deleting_commit}^1:{card_file}");
    let card_text = crate::proc::git(root, &["show", &parent_spec])
        .map_err(|e| format!("cannot read {parent_spec}: {e}"))?;

    let (declared, parse_errors) = declared_paths(&card_text);

    let mut messages = Vec::new();
    for err in parse_errors {
        if err.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            messages.push(format!("{card_file}:{err}"));
        } else {
            messages.push(format!("{card_file}: {err}"));
        }
    }

    let diff_all = crate::proc::git(root, &["diff", "--no-renames", "--name-only", base, head])?;

    let mut changed: Vec<String> = diff_all
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect();
    changed.sort();
    changed.dedup();

    let undeclared = check_paths(&declared, &changed, card_file);
    for path in undeclared {
        messages.push(format!(
            "scope: {path} is not in the Files or Registration lines of {card_file}"
        ));
    }

    if !messages.is_empty() {
        return Ok(ScopeOutcome::Fail(messages));
    }

    Ok(ScopeOutcome::Ok {
        slug,
        paths: changed.len(),
    })
}

/// The `scope BASE HEAD` command entry point.
pub fn command(root: &Path, base: &str, head: &str) -> Result<bool, String> {
    match run(root, base, head)? {
        ScopeOutcome::Skipped => {
            println!("scope: skipped (no card file deleted)");
            Ok(true)
        }
        ScopeOutcome::Ok { slug, paths } => {
            println!("scope: ok ({slug}, {paths} paths)");
            Ok(true)
        }
        ScopeOutcome::Fail(messages) => {
            for message in messages {
                eprintln!("{message}");
            }
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    struct TestRepo {
        dir: tempfile::TempDir,
    }

    impl TestRepo {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path();
            Self::git_cmd(path, &["init", "-b", "main"]);
            Self::git_cmd(path, &["config", "user.name", "t"]);
            Self::git_cmd(path, &["config", "user.email", "t@example.invalid"]);
            Self::git_cmd(path, &["config", "commit.gpgsign", "false"]);
            Self { dir }
        }

        fn path(&self) -> &Path {
            self.dir.path()
        }

        fn git_cmd(dir: &Path, args: &[&str]) -> String {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
            assert!(
                out.status.success(),
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        }

        fn git(&self, args: &[&str]) -> String {
            Self::git_cmd(self.path(), args)
        }

        fn write(&self, rel_path: &str, content: &str) {
            let full = self.path().join(rel_path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("create_dir_all");
            }
            std::fs::write(&full, content).expect("write file");
        }

        fn remove(&self, rel_path: &str) {
            let full = self.path().join(rel_path);
            let _ = std::fs::remove_file(full);
        }

        fn commit(&self, msg: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-m",
                msg,
            ]);
            self.git(&["rev-parse", "HEAD"])
        }
    }

    #[test]
    fn passes_when_every_path_is_listed() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`, `src/b.rs`.\n",
        );
        repo.write("src/a.rs", "fn a() {}\n");
        repo.write("src/b.rs", "fn b() {}\n");
        let base = repo.commit("initial");

        repo.write("src/a.rs", "fn a_updated() {}\n");
        repo.remove("docs/cards/demo.md");
        let head = repo.commit("delete card and modify a");

        let outcome = run(repo.path(), &base, &head).expect("run");
        assert_eq!(
            outcome,
            ScopeOutcome::Ok {
                slug: "demo".to_owned(),
                paths: 2,
            }
        );
        assert!(command(repo.path(), &base, &head).expect("command"));
    }

    #[test]
    fn fails_on_an_unlisted_path_and_names_it() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`.\n",
        );
        repo.write("src/a.rs", "fn a() {}\n");
        repo.write("crates/fieldbook/src/cli.rs", "fn cli() {}\n");
        let base = repo.commit("initial");

        // Case 1: unlisted path crates/fieldbook/src/cli.rs fails and is named
        repo.write("crates/fieldbook/src/cli.rs", "fn cli_changed() {}\n");
        repo.remove("docs/cards/demo.md");
        let head = repo.commit("unlisted change and delete card");

        let outcome = run(repo.path(), &base, &head).expect("run");
        match outcome {
            ScopeOutcome::Fail(messages) => {
                assert!(
                    messages
                        .iter()
                        .any(|m| m.contains("crates/fieldbook/src/cli.rs")
                            && m.contains("docs/cards/demo.md")),
                    "expected failure message naming path and card, got: {messages:?}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
        assert!(!command(repo.path(), &base, &head).expect("command"));

        // Case 2: same path declared under Registration lines passes
        let repo2 = TestRepo::new();
        repo2.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`.\n- Registration lines: `crates/fieldbook/src/cli.rs`.\n",
        );
        repo2.write("src/a.rs", "fn a() {}\n");
        repo2.write("crates/fieldbook/src/cli.rs", "fn cli() {}\n");
        let base2 = repo2.commit("initial");

        repo2.write("crates/fieldbook/src/cli.rs", "fn cli_changed() {}\n");
        repo2.remove("docs/cards/demo.md");
        let head2 = repo2.commit("modify registered and delete card");

        let outcome2 = run(repo2.path(), &base2, &head2).expect("run");
        assert_eq!(
            outcome2,
            ScopeOutcome::Ok {
                slug: "demo".to_owned(),
                paths: 2,
            }
        );
        assert!(command(repo2.path(), &base2, &head2).expect("command"));
    }

    #[test]
    fn reads_the_card_before_the_deleting_commit() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`.\n",
        );
        repo.write("src/a.rs", "fn a() {}\n");
        repo.write("src/b.rs", "fn b() {}\n");
        let base = repo.commit("base");

        // Commit 1: card extended in an earlier commit of the range
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`, `src/b.rs`.\n",
        );
        repo.write("src/b.rs", "fn b_modified() {}\n");
        let _c1 = repo.commit("extend card in earlier commit");

        // Commit 2: delete card
        repo.remove("docs/cards/demo.md");
        let head = repo.commit("delete card");

        let outcome = run(repo.path(), &base, &head).expect("run");
        assert_eq!(
            outcome,
            ScopeOutcome::Ok {
                slug: "demo".to_owned(),
                paths: 2,
            }
        );
        assert!(command(repo.path(), &base, &head).expect("command"));

        // Case 3b: card not extended before deleting commit -> fails
        let repo2 = TestRepo::new();
        repo2.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`.\n",
        );
        repo2.write("src/a.rs", "fn a() {}\n");
        repo2.write("src/b.rs", "fn b() {}\n");
        let base2 = repo2.commit("base");

        // Commit 1: modifies src/b.rs WITHOUT extending demo.md
        repo2.write("src/b.rs", "fn b_modified() {}\n");
        let _c1_2 = repo2.commit("modify b without extending card");

        // Commit 2: deletes demo.md
        repo2.remove("docs/cards/demo.md");
        let head2 = repo2.commit("delete card");

        let outcome2 = run(repo2.path(), &base2, &head2).expect("run");
        match outcome2 {
            ScopeOutcome::Fail(messages) => {
                assert!(messages.iter().any(|m| m.contains("src/b.rs")));
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn skips_a_range_with_no_card_deleted() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`.\n",
        );
        repo.write("src/a.rs", "fn a() {}\n");
        let base = repo.commit("base");

        repo.write("src/a.rs", "fn a_updated() {}\n");
        let head = repo.commit("modify without deleting card");

        let outcome = run(repo.path(), &base, &head).expect("run");
        assert_eq!(outcome, ScopeOutcome::Skipped);
        assert!(command(repo.path(), &base, &head).expect("command"));
    }

    #[test]
    fn skips_an_empty_or_zero_base() {
        let repo = TestRepo::new();
        repo.write("src/a.rs", "fn a() {}\n");
        let head = repo.commit("initial");

        let zeros = "0".repeat(40);
        assert_eq!(
            run(repo.path(), "", &head).expect("run"),
            ScopeOutcome::Skipped
        );
        assert_eq!(
            run(repo.path(), &zeros, &head).expect("run"),
            ScopeOutcome::Skipped
        );
        assert!(command(repo.path(), "", &head).expect("command"));
        assert!(command(repo.path(), &zeros, &head).expect("command"));
    }

    #[test]
    fn fails_on_two_deleted_cards() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/a.md",
            "# a\n\n## Scope\n\n- Files: `src/a.rs`.\n",
        );
        repo.write(
            "docs/cards/b.md",
            "# b\n\n## Scope\n\n- Files: `src/b.rs`.\n",
        );
        let base = repo.commit("base");

        repo.remove("docs/cards/a.md");
        repo.remove("docs/cards/b.md");
        let head = repo.commit("delete both cards");

        let outcome = run(repo.path(), &base, &head).expect("run");
        match outcome {
            ScopeOutcome::Fail(messages) => {
                assert_eq!(messages.len(), 1);
                let msg = &messages[0];
                assert!(msg.starts_with(
                    "scope: one pull request closes one card, but the range deletes:"
                ));
                assert!(msg.contains("a.md"));
                assert!(msg.contains("b.md"));
            }
            other => panic!("expected Fail, got {other:?}"),
        }
        assert!(!command(repo.path(), &base, &head).expect("command"));
    }

    #[test]
    fn allows_the_lock_the_changelog_and_generated_files() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`.\n",
        );
        repo.write("src/a.rs", "fn a() {}\n");
        repo.write("Cargo.lock", "# lock\n");
        repo.write("CHANGELOG.md", "# changelog\n");
        for generator in crate::docs::GENERATORS {
            repo.write(generator.file, "# generated\n");
        }
        let base = repo.commit("base");

        repo.write("src/a.rs", "fn a2() {}\n");
        repo.write("Cargo.lock", "# lock changed\n");
        repo.write("CHANGELOG.md", "# changelog changed\n");
        for generator in crate::docs::GENERATORS {
            repo.write(generator.file, "# generated changed\n");
        }
        repo.remove("docs/cards/demo.md");
        let head = repo.commit("delete card and modify exempt files");

        let outcome = run(repo.path(), &base, &head).expect("run");
        match outcome {
            ScopeOutcome::Ok { slug, paths } => {
                assert_eq!(slug, "demo");
                assert_eq!(paths, 4 + crate::docs::GENERATORS.len());
            }
            other => panic!("expected Ok, got {other:?}"),
        }
        assert!(command(repo.path(), &base, &head).expect("command"));
    }

    #[test]
    fn expands_registration_braces() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/a.rs`.\n- Registration lines: `tests/support/{world.rs,faulty.rs}`.\n",
        );
        repo.write("src/a.rs", "fn a() {}\n");
        repo.write("tests/support/world.rs", "fn world() {}\n");
        repo.write("tests/support/faulty.rs", "fn faulty() {}\n");
        repo.write("tests/support/other.rs", "fn other() {}\n");
        let base = repo.commit("base");

        // Case 1: changing declared files passes
        repo.write("tests/support/world.rs", "fn world2() {}\n");
        repo.write("tests/support/faulty.rs", "fn faulty2() {}\n");
        repo.remove("docs/cards/demo.md");
        let head = repo.commit("change both declared files and delete card");

        let outcome = run(repo.path(), &base, &head).expect("run");
        assert!(matches!(outcome, ScopeOutcome::Ok { .. }));

        // Case 2: undeclared sibling fails
        repo.write("tests/support/other.rs", "fn other2() {}\n");
        let head_sibling = repo.commit("also change undeclared sibling");

        let outcome_fail = run(repo.path(), &base, &head_sibling).expect("run");
        match outcome_fail {
            ScopeOutcome::Fail(messages) => {
                assert!(
                    messages
                        .iter()
                        .any(|m| m.contains("tests/support/other.rs")),
                    "expected failure naming sibling, got: {messages:?}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn rename_reports_old_and_new_path() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `src/declared.rs`.\n",
        );
        repo.write("src/declared.rs", "fn dec() {}\n");
        repo.write("src/old_name.rs", "fn old() {}\n");
        let base = repo.commit("base");

        repo.git(&["mv", "src/old_name.rs", "src/new_name.rs"]);
        repo.remove("docs/cards/demo.md");
        let head = repo.commit("rename and delete card");

        let outcome = run(repo.path(), &base, &head).expect("run");
        match outcome {
            ScopeOutcome::Fail(messages) => {
                assert!(
                    messages.iter().any(|m| m.contains("src/old_name.rs")),
                    "missing old path in: {messages:?}"
                );
                assert!(
                    messages.iter().any(|m| m.contains("src/new_name.rs")),
                    "missing new path in: {messages:?}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn directory_prefix_is_not_a_text_prefix_match() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `tests/fix/`.\n",
        );
        repo.write("tests/fixtures/x.rs", "fn x() {}\n");
        repo.write("tests/fix/valid.rs", "fn v() {}\n");
        let base = repo.commit("base");

        // Case 1: modifying tests/fixtures/x.rs is NOT covered by tests/fix/
        repo.write("tests/fixtures/x.rs", "fn x2() {}\n");
        repo.remove("docs/cards/demo.md");
        let head = repo.commit("modify fixture and delete card");

        let outcome = run(repo.path(), &base, &head).expect("run");
        match outcome {
            ScopeOutcome::Fail(messages) => {
                assert!(
                    messages.iter().any(|m| m.contains("tests/fixtures/x.rs")),
                    "expected failure naming tests/fixtures/x.rs, got: {messages:?}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }

        // Case 2: modifying tests/fix/valid.rs IS covered by tests/fix/
        let repo2 = TestRepo::new();
        repo2.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `tests/fix/`.\n",
        );
        repo2.write("tests/fix/valid.rs", "fn v() {}\n");
        let base2 = repo2.commit("base");

        repo2.write("tests/fix/valid.rs", "fn v2() {}\n");
        repo2.remove("docs/cards/demo.md");
        let head2 = repo2.commit("modify inside fix dir and delete card");

        let outcome2 = run(repo2.path(), &base2, &head2).expect("run");
        assert!(matches!(outcome2, ScopeOutcome::Ok { .. }));
    }

    #[test]
    fn wildcard_in_files_is_reported() {
        let repo = TestRepo::new();
        repo.write(
            "docs/cards/demo.md",
            "# demo\n\n## Scope\n\n- Files: `tests/*.rs`.\n",
        );
        let base = repo.commit("base");

        repo.remove("docs/cards/demo.md");
        let head = repo.commit("delete card with wildcard");

        let outcome = run(repo.path(), &base, &head).expect("run");
        match outcome {
            ScopeOutcome::Fail(messages) => {
                assert!(
                    messages.iter().any(|m| m.contains("docs/cards/demo.md")
                        && m.contains('5')
                        && m.contains("unsupported wildcard")),
                    "expected failure with card file, line and wildcard, got: {messages:?}"
                );
            }
            other => panic!("expected Fail, got {other:?}"),
        }
    }

    #[test]
    fn pure_declared_paths_and_check_paths() {
        let card_text = r#"# demo

## Scope

- Files: `src/{a.rs,b.rs}`, `docs/`.
- Registration lines: `tests/support/helper.rs`, `SomeStruct`, `method_name()`.
"#;
        let (declared, errors) = declared_paths(card_text);
        assert!(errors.is_empty(), "unexpected errors: {errors:?}");
        assert!(declared.contains(&ScopePath::File("src/a.rs".to_owned())));
        assert!(declared.contains(&ScopePath::File("src/b.rs".to_owned())));
        assert!(declared.contains(&ScopePath::Dir("docs".to_owned())));
        assert!(declared.contains(&ScopePath::File("tests/support/helper.rs".to_owned())));
        assert_eq!(declared.len(), 4);

        let changed = vec![
            "src/a.rs".to_owned(),
            "docs/guide.md".to_owned(),
            "tests/support/helper.rs".to_owned(),
            "Cargo.lock".to_owned(),
            "CHANGELOG.md".to_owned(),
            "docs/cards/demo.md".to_owned(),
            "docs/reference/errors.md".to_owned(),
            "unlisted.rs".to_owned(),
        ];
        let undeclared = check_paths(&declared, &changed, "docs/cards/demo.md");
        assert_eq!(undeclared, vec!["unlisted.rs".to_owned()]);
    }
}
