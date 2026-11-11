//! Card lifecycle integration tests (`docs-checks`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "card lifecycle integration tests start processes and run git in a tempdir"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn extract_readme_template(real_readme: &str) -> String {
    let b_marker = "<!-- card-template:begin -->";
    let e_marker = "<!-- card-template:end -->";
    let b_idx = real_readme
        .find(b_marker)
        .unwrap_or_else(|| panic!("missing card-template:begin in README.md"));
    let e_idx = real_readme
        .find(e_marker)
        .unwrap_or_else(|| panic!("missing card-template:end in README.md"))
        + e_marker.len();
    let template_block = &real_readme[b_idx..e_idx];

    let h_marker = "## The three header fields";
    let h_idx = real_readme
        .find(h_marker)
        .unwrap_or_else(|| panic!("missing The three header fields in README.md"));
    let rev_idx = real_readme[h_idx..]
        .find("| Review |")
        .unwrap_or_else(|| panic!("missing Review row in three header fields table"))
        + h_idx;
    let eol = real_readme[rev_idx..]
        .find('\n')
        .map_or(real_readme.len(), |pos| rev_idx + pos);
    let table_block = &real_readme[h_idx..=eol];

    format!(
        "# Cards\n\n## Recipes for recurring changes\n\n## The template\n\n{template_block}\n\n{table_block}\n"
    )
}

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn run_git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(self.path())
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("failed to run git {args:?}: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write_file(&self, rel_path: &str, content: &str) {
        let dest = self.path().join(rel_path);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("mkdir {}: {e}", parent.display()));
        }
        std::fs::write(&dest, content).unwrap_or_else(|e| panic!("write {rel_path}: {e}"));
    }

    fn commit(&self, msg: &str) {
        self.run_git(&["add", "."]);
        self.run_git(&["commit", "-q", "-m", msg]);
    }

    fn delete_card(&self, slug: &str) {
        let path = format!("docs/cards/{slug}.md");
        self.run_git(&["rm", "-q", &path]);
        self.commit(&format!("complete {slug}"));
    }

    fn card(
        &self,
        slug: &str,
        files_line: &str,
        merged_bullets: &[&str],
        steps: &[&str],
        acceptance: &[&str],
    ) {
        let mut text = String::new();
        text.push_str(&format!("# {slug}\n\n"));
        text.push_str("- **Runs where:** home, CI Linux\n");
        text.push_str("- **Schema impact:** none\n");
        text.push_str("- **Review:** auto\n\n");
        text.push_str("## Outcome\n\n");
        text.push_str(&format!("Outcome for {slug}.\n\n"));
        text.push_str("## Why\n\n");
        text.push_str(&format!("Why description for {slug}.\n\n"));
        text.push_str("## Scope\n\n");
        let files_fmt = if files_line.starts_with("- Files:") {
            files_line.trim_end_matches('.').to_owned() + "."
        } else if files_line.starts_with("Files:") {
            format!("- {}.", files_line.trim_end_matches('.'))
        } else {
            format!("- Files: {}.", files_line.trim_end_matches('.'))
        };
        text.push_str(&format!("{files_fmt}\n"));
        text.push_str("- Docs to update: `README.md`.\n\n");
        text.push_str("## Out of scope\n\n");
        text.push_str("Nothing.\n\n");
        text.push_str("## Preconditions\n\n");
        if merged_bullets.is_empty() {
            text.push_str("- Cards merged: none.\n\n");
        } else {
            for b in merged_bullets {
                if b.starts_with("- ") {
                    text.push_str(b);
                } else if b.starts_with("Cards merged:") {
                    text.push_str(&format!("- {b}"));
                } else {
                    let clean = b.trim_end_matches('.');
                    text.push_str(&format!("- Cards merged: {clean}."));
                }
                text.push('\n');
            }
            text.push('\n');
        }
        text.push_str("## Steps\n\n");
        if steps.is_empty() {
            text.push_str("1. Step one.\n\n");
        } else {
            for (idx, step) in steps.iter().enumerate() {
                if step.starts_with(|c: char| c.is_ascii_digit()) {
                    text.push_str(step);
                } else {
                    text.push_str(&format!("{}. {step}", idx + 1));
                }
                text.push('\n');
            }
            text.push('\n');
        }
        text.push_str("## Acceptance\n\n```sh\n");
        if acceptance.is_empty() {
            text.push_str("just check\n");
        } else {
            for cmd in acceptance {
                text.push_str(cmd);
                text.push('\n');
            }
            if !acceptance.iter().any(|cmd| cmd.trim() == "just check") {
                text.push_str("just check\n");
            }
        }
        text.push_str("```\n\n");
        text.push_str("Expected: clean check.\n\n");
        text.push_str("## Negative control\n\n");
        text.push_str("Negative control.\n\n");
        text.push_str("## Rollback\n\n");
        text.push_str("Rollback.\n\n");
        text.push_str("## BLOCKED\n\n");
        text.push_str("Blocked.\n");

        self.write_file(&format!("docs/cards/{slug}.md"), &text);
    }

    fn plan(&self, phases: &[(&str, &[&str])]) {
        let mut text = String::from("# Plan\n\n");
        for (phase, items) in phases {
            text.push_str(&format!("## {phase}\n\n"));
            for item in *items {
                text.push_str(&format!("- {item}\n"));
            }
            text.push('\n');
        }
        self.write_file("docs/plan/PLAN.md", &text);
    }

    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let exe = env!("CARGO_BIN_EXE_xtask");
        let out = Command::new(exe)
            .current_dir(self.path())
            .env("XTASK_REPO_ROOT", self.path())
            .args(args)
            .output()
            .unwrap_or_else(|e| panic!("failed to run {exe} {args:?}: {e}"));
        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        (code, stdout, stderr)
    }
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let fix = Fixture { dir };

    fix.run_git(&["init", "-q", "-b", "main"]);
    fix.run_git(&["config", "user.name", "t"]);
    fix.run_git(&["config", "user.email", "t@example.invalid"]);
    fix.run_git(&["config", "commit.gpgsign", "false"]);

    // Extract template from real cards README
    let real_readme_path = repo_root().join("docs/cards/README.md");
    let real_readme = std::fs::read_to_string(&real_readme_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", real_readme_path.display()));
    let extracted_readme = extract_readme_template(&real_readme);
    fix.write_file("docs/cards/README.md", &extracted_readme);

    // Minimal reference docs to satisfy generator sync and link checking:
    // Real corpus-format strings block with cross-doc links turned into plain text
    let real_cf_path = repo_root().join("docs/reference/corpus-format.md");
    let real_cf = std::fs::read_to_string(&real_cf_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", real_cf_path.display()));
    let s_start = real_cf
        .find("<!-- strings:begin -->")
        .unwrap_or_else(|| panic!("strings:begin in corpus-format.md"));
    let s_end = real_cf
        .find("<!-- strings:end -->")
        .unwrap_or_else(|| panic!("strings:end in corpus-format.md"))
        + "<!-- strings:end -->".len();
    let strings_block = &real_cf[s_start..s_end]
        .replace(
            "[ADR 0001](../decisions/0001-foundation.md#ad-17-authoring-experience)",
            "ADR 0001",
        )
        .replace("[ADR 0001](../decisions/0001-foundation.md)", "ADR 0001");

    let minimal_cf = format!("# Corpus format\n\n## Scope and versioning\n\n{strings_block}\n");
    fix.write_file("docs/reference/corpus-format.md", &minimal_cf);

    // Other reference docs copied on disk but excluded from snapshot via .gitignore
    fix.write_file(
        ".gitignore",
        "docs/reference/errors.md\ndocs/reference/cli.md\ndocs/reference/audit.md\n",
    );
    for name in &["errors.md", "cli.md", "audit.md"] {
        let src = repo_root().join("docs/reference").join(name);
        let content =
            std::fs::read_to_string(&src).unwrap_or_else(|e| panic!("read {}: {e}", src.display()));
        fix.write_file(&format!("docs/reference/{name}"), &content);
    }

    // Baseline empty-safe AGENTS.md and initial PLAN.md
    fix.write_file("AGENTS.md", "Fieldbook dev.\n");
    fix.plan(&[("P1 Read", &[])]);

    fix.commit("initial fixture setup");
    fix
}

fn card_slug(text: &str) -> Result<String, String> {
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(slug) = trimmed.strip_prefix("# ") {
            return Ok(slug.trim().to_owned());
        }
    }
    Err("missing card title".to_owned())
}

fn obligation_ids_in_steps(text: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut in_steps = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "## Steps" {
            in_steps = true;
            continue;
        }
        if in_steps && trimmed.starts_with("## ") {
            break;
        }
        if in_steps && trimmed.starts_with(|c: char| c.is_ascii_digit()) {
            let rest = trimmed
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
                .trim_start();
            if let Some(bracketed) = rest.strip_prefix('[')
                && let Some((id, _)) = bracketed.split_once(']')
                && !id.is_empty()
            {
                ids.push(id.to_owned());
            }
        }
    }
    ids
}

fn acceptance_commands(text: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_acceptance = false;
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "## Acceptance" {
            in_acceptance = true;
            continue;
        }
        if in_acceptance && !in_fence && trimmed.starts_with("## ") {
            break;
        }
        if in_acceptance {
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
                continue;
            }
            if in_fence
                && !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && trimmed != "just check"
            {
                commands.push(trimmed.to_owned());
            }
        }
    }
    commands
}

fn obligation_refs_in_steps(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut in_steps = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "## Steps" {
            in_steps = true;
            continue;
        }
        if in_steps && trimmed.starts_with("## ") {
            break;
        }
        if in_steps {
            for (i, c) in trimmed.char_indices() {
                if c == '['
                    && let Some(end) = trimmed[i + 1..].find(']')
                {
                    let id = &trimmed[i + 1..i + 1 + end];
                    if !id.is_empty()
                        && id
                            .chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                        && !refs.contains(&id.to_owned())
                    {
                        refs.push(id.to_owned());
                    }
                }
            }
        }
    }
    refs
}

fn cards_merged(text: &str) -> Vec<String> {
    let mut slugs = Vec::new();
    let mut in_precond = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "## Preconditions" {
            in_precond = true;
            continue;
        }
        if in_precond && trimmed.starts_with("## ") {
            break;
        }
        if in_precond && trimmed.starts_with("- Cards merged:") {
            let rest = trimmed.strip_prefix("- Cards merged:").unwrap_or("");
            let mut in_backticks = false;
            let mut current = String::new();
            for c in rest.chars() {
                if c == '`' {
                    if in_backticks {
                        let clean = current.trim();
                        if !clean.is_empty()
                            && clean != "none"
                            && !slugs.contains(&clean.to_owned())
                        {
                            slugs.push(clean.to_owned());
                        }
                        current.clear();
                    }
                    in_backticks = !in_backticks;
                } else if in_backticks {
                    current.push(c);
                }
            }
            if slugs.is_empty() {
                for word in rest.split_whitespace() {
                    let clean = word.trim_matches(|c: char| c == '`' || c == '.' || c == ',');
                    if !clean.is_empty() && clean != "none" && !slugs.contains(&clean.to_owned()) {
                        slugs.push(clean.to_owned());
                    }
                }
            }
        }
    }
    slugs
}

fn split_released(before: &str, parts: &[&str], consumer: &str) -> Result<(), String> {
    // (a) every obligation id of before appears in exactly one of parts
    let before_obs = obligation_ids_in_steps(before);
    for ob in &before_obs {
        let count = parts
            .iter()
            .filter(|p| obligation_ids_in_steps(p).contains(ob))
            .count();
        if count == 0 {
            return Err(format!("appended obligation `{ob}` appears in no part"));
        }
        if count > 1 {
            return Err(format!(
                "obligation `{ob}` appears in multiple parts ({count})"
            ));
        }
    }

    // (b) every Acceptance command of before appears in exactly one part
    let before_cmds = acceptance_commands(before);
    for cmd in &before_cmds {
        let count = parts
            .iter()
            .filter(|p| acceptance_commands(p).contains(cmd))
            .count();
        if count == 0 {
            return Err(format!("acceptance command `{cmd}` appears in no part"));
        }
        if count > 1 {
            return Err(format!(
                "acceptance command `{cmd}` appears in multiple parts ({count})"
            ));
        }
    }

    // (c) for each obligation id the consumer card names in its Steps with a [ob-id] reference,
    // the part that holds that id must be one of the slugs in the consumer's Cards merged: bullet
    let consumer_slug = card_slug(consumer)?;
    let consumer_refs = obligation_refs_in_steps(consumer);
    let consumer_prereqs = cards_merged(consumer);

    for ob_id in &consumer_refs {
        let mut provider_slug = None;
        let mut provider_idx = None;
        for (idx, part) in parts.iter().enumerate() {
            if obligation_ids_in_steps(part).contains(ob_id) {
                provider_slug = Some(card_slug(part)?);
                provider_idx = Some(idx);
                break;
            }
        }

        if let Some(ref slug) = provider_slug
            && !consumer_prereqs.contains(slug)
        {
            return Err(format!(
                "consumer `{consumer_slug}` references `{ob_id}` but is missing prerequisite `{slug}`"
            ));
        }

        // (d) a required invariant or test (id starts inv- or test-) must not sit in a part that
        // comes after consumer's first prerequisite part unless consumer lists that part
        if (ob_id.starts_with("inv-") || ob_id.starts_with("test-"))
            && let Some(idx) = provider_idx
            && let Some(ref slug) = provider_slug
            && idx > 0
            && !consumer_prereqs.contains(slug)
        {
            return Err(format!(
                "consumer `{consumer_slug}` references required invariant/test `{ob_id}` in deferred part `{slug}` without prerequisite"
            ));
        }
    }

    Ok(())
}

#[test]
fn closing_card_keeps_permanent_links_valid() {
    let fix = fixture();
    fix.card(
        "demo",
        "`xtask/src/cards.rs`",
        &["none."],
        &["1. Step."],
        &["just check"],
    );
    fix.plan(&[("P1 Read", &["demo: one line"])]);
    fix.write_file(
        "docs/guide.md",
        "# Guide\n\nA permanent guide linking to [Cards](cards/README.md).\n",
    );
    fix.commit("add demo and guide");

    let (code, stdout, stderr) = fix.run(&["cards", "--check", "--closing", "demo"]);
    assert_eq!(
        code, 0,
        "expected exit 0, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("cards: ok"),
        "expected 'cards: ok', got: {stdout}"
    );

    let card_path = fix.path().join("docs/cards/demo.md");
    assert!(card_path.is_file(), "card file must still exist on disk");
}

#[test]
fn closing_with_inbound_card_link_fails() {
    let fix = fixture();
    fix.card(
        "demo",
        "`xtask/src/cards.rs`",
        &["none."],
        &["1. Step."],
        &["just check"],
    );
    fix.plan(&[("P1 Read", &["demo: one line"])]);
    fix.write_file(
        "docs/guide.md",
        "# Guide\n\nA permanent guide linking to [Demo](cards/demo.md).\n",
    );
    fix.commit("add demo and guide");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 0,
        "expected exit 0 while card exists, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));

    let (code, stdout, stderr) = fix.run(&["cards", "--check", "--closing", "demo"]);
    assert_eq!(
        code, 1,
        "expected exit 1, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("docs/guide.md"),
        "stderr must name docs/guide.md: {stderr}"
    );
    assert!(
        stderr.contains("docs/cards/demo.md") || stderr.contains("cards/demo.md"),
        "stderr must name target demo: {stderr}"
    );
}

#[test]
fn prerequisite_cycle_is_rejected() {
    let fix = fixture();
    fix.card(
        "demo-a",
        "`xtask/src/cards.rs`",
        &["`demo-b`."],
        &["1. Step."],
        &["just check"],
    );
    fix.card(
        "demo-b",
        "`xtask/src/scope.rs`",
        &["`demo-a`."],
        &["1. Step."],
        &["just check"],
    );
    fix.plan(&[("P1 Read", &["demo-a: first", "demo-b: second"])]);
    fix.commit("add cyclic cards");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 1,
        "expected exit 1 for cycle, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("demo-a -> demo-b -> demo-a")
            || stderr.contains("demo-b -> demo-a -> demo-b"),
        "stderr must report cycle: {stderr}"
    );

    fix.card(
        "demo-a",
        "`xtask/src/cards.rs`",
        &[
            "- Cards merged: none.",
            "- Safe to run beside `demo-b`: their paths are disjoint.",
        ],
        &["1. Step."],
        &["just check"],
    );
    fix.card(
        "demo-b",
        "`xtask/src/scope.rs`",
        &[
            "- Cards merged: none.",
            "- Safe to run beside `demo-a`: their paths are disjoint.",
        ],
        &["1. Step."],
        &["just check"],
    );
    fix.commit("use parallel notes instead of merged bullets");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 0,
        "expected exit 0 with parallel note, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));
}

#[test]
fn parallel_note_is_not_a_dependency() {
    let fix = fixture();
    fix.card(
        "demo-a",
        "`xtask/src/cards.rs`",
        &[
            "- Cards merged: none.",
            "- Safe to run beside `demo-b`: their paths are disjoint.",
        ],
        &["1. Step."],
        &["just check"],
    );
    fix.card(
        "demo-b",
        "`xtask/src/scope.rs`",
        &[
            "- Cards merged: none.",
            "- Safe to run beside `demo-a`: their paths are disjoint.",
        ],
        &["1. Step."],
        &["just check"],
    );
    fix.plan(&[("P1 Read", &["demo-a: first", "demo-b: second"])]);
    fix.commit("cards with parallel notes");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 0,
        "expected exit 0, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));
    assert!(
        !stderr.contains("cycle"),
        "stderr must not report any cycle: {stderr}"
    );
}

#[test]
fn prerequisite_precedes_consumer() {
    let fix = fixture();
    fix.card(
        "demo-a",
        "`xtask/src/cards.rs`",
        &["none."],
        &["1. Step."],
        &["just check"],
    );
    fix.card(
        "demo-b",
        "`xtask/src/scope.rs`",
        &["`demo-a`."],
        &["1. Step."],
        &["just check"],
    );

    // PLAN lists prerequisite before consumer: passes
    fix.plan(&[("P1 Read", &["demo-a: first", "demo-b: second"])]);
    fix.commit("prereq precedes consumer");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 0,
        "expected exit 0, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));

    // PLAN lists consumer before prerequisite: exit 1 and message names both slugs
    fix.plan(&[("P1 Read", &["demo-b: second", "demo-a: first"])]);
    fix.commit("consumer precedes prereq in plan");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 1,
        "expected exit 1, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("demo-a"),
        "stderr must name demo-a: {stderr}"
    );
    assert!(
        stderr.contains("demo-b"),
        "stderr must name demo-b: {stderr}"
    );
}

#[test]
fn split_preserves_prerequisite_providers() {
    let before = r#"# demo

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome
Outcome.

## Why
Why.

## Scope
- Files: `xtask/src/cards.rs`.
- Docs to update: `README.md`.

## Out of scope
Nothing.

## Preconditions
- Cards merged: none.

## Steps
1. [contract] define the trait interface
2. [inv-state] verify state invariant
3. [test-pos] positive unit test
4. [test-neg] negative unit test
5. [ob-deferred] deferred extra feature

## Acceptance
```sh
cargo test --test part1
cargo test --test part2
just check
```

Expected: check passes.

## Negative control
Control.

## Rollback
Rollback.

## BLOCKED
Blocked.
"#;

    let part1 = r#"# demo

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome
Outcome.

## Why
Why.

## Scope
- Files: `xtask/src/cards.rs`.
- Docs to update: `README.md`.

## Out of scope
Nothing.

## Preconditions
- Cards merged: none.

## Steps
1. [contract] define the trait interface
2. [inv-state] verify state invariant
3. [test-pos] positive unit test
4. [test-neg] negative unit test

## Acceptance
```sh
cargo test --test part1
just check
```

Expected: check passes.

## Negative control
Control.

## Rollback
Rollback.

## BLOCKED
Blocked.
"#;

    let part2 = r#"# demo-2

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome
Outcome.

## Why
Why.

## Scope
- Files: `xtask/src/scope.rs`.
- Docs to update: `README.md`.

## Out of scope
Nothing.

## Preconditions
- Cards merged: `demo`.

## Steps
1. [ob-deferred] deferred extra feature

## Acceptance
```sh
cargo test --test part2
just check
```

Expected: check passes.

## Negative control
Control.

## Rollback
Rollback.

## BLOCKED
Blocked.
"#;

    let consumer = r#"# demo-consumer

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome
Outcome.

## Why
Why.

## Scope
- Files: `xtask/src/main.rs`.
- Docs to update: `README.md`.

## Out of scope
Nothing.

## Preconditions
- Cards merged: `demo`, `demo-2`.

## Steps
1. Integrate [contract], [inv-state], [test-pos], [test-neg] and [ob-deferred].

## Acceptance
```sh
cargo test --test consumer
just check
```

Expected: check passes.

## Negative control
Control.

## Rollback
Rollback.

## BLOCKED
Blocked.
"#;

    // Passing split check
    assert!(split_released(before, &[part1, part2], consumer).is_ok());

    // Failing fixture 1: appended obligation in neither part
    let before_with_appended = before.replace(
        "5. [ob-deferred] deferred extra feature",
        "5. [ob-deferred] deferred extra feature\n6. [ob-appended] extra obligation",
    );
    let err1 = split_released(&before_with_appended, &[part1, part2], consumer).unwrap_err();
    assert!(
        err1.contains("ob-appended"),
        "error must name appended obligation: {err1}"
    );

    // Failing fixture 2: required invariant deferred to part two while consumer lists only part one
    let consumer_only_part1 = consumer.replace(
        "- Cards merged: `demo`, `demo-2`.",
        "- Cards merged: `demo`.",
    );
    let part1_without_inv = part1.replace("2. [inv-state] verify state invariant\n", "");
    let part2_with_inv = part2.replace(
        "1. [ob-deferred] deferred extra feature",
        "1. [inv-state] verify state invariant\n2. [ob-deferred] deferred extra feature",
    );
    let err2 = split_released(
        before,
        &[&part1_without_inv, &part2_with_inv],
        &consumer_only_part1,
    )
    .unwrap_err();
    assert!(
        err2.contains("inv-state"),
        "error must name inv-state: {err2}"
    );
    assert!(
        err2.contains("demo-consumer"),
        "error must name consumer: {err2}"
    );
    assert!(err2.contains("demo-2"), "error must name demo-2: {err2}");

    // Failing fixture 3: consumer's part-two prerequisite dropped
    let err3 = split_released(before, &[part1, part2], &consumer_only_part1).unwrap_err();
    assert!(
        err3.contains("demo-consumer"),
        "error must name consumer: {err3}"
    );
    assert!(
        err3.contains("ob-deferred"),
        "error must name obligation id: {err3}"
    );
    assert!(
        err3.contains("demo-2"),
        "error must name missing slug: {err3}"
    );

    // Also run cards --check on the passing split tree and assert exit 0
    let fix = fixture();
    fix.write_file("docs/cards/demo.md", part1);
    fix.write_file("docs/cards/demo-2.md", part2);
    fix.write_file("docs/cards/demo-consumer.md", consumer);
    fix.plan(&[(
        "P1 Read",
        &[
            "demo: part one",
            "demo-2: part two",
            "demo-consumer: consumer",
        ],
    )]);
    fix.commit("passing split tree");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 0,
        "passing split tree must pass cards --check, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));

    // with the consumer's demo-2 prerequisite dropped from its Cards merged: bullet and the consumer placed before demo-2 in PLAN, the helper fails (the program need not)
    assert!(split_released(before, &[part1, part2], &consumer_only_part1).is_err());
}

#[test]
fn fresh_fixture_passes() {
    let fix = fixture();
    fix.card(
        "demo-a",
        "`xtask/src/cards.rs`",
        &["none."],
        &["1. Step."],
        &["just check"],
    );
    fix.card(
        "demo-b",
        "`xtask/src/scope.rs`",
        &["`demo-a`."],
        &["1. Step."],
        &["just check"],
    );
    fix.plan(&[("P1 Read", &["demo-a: first", "demo-b: second"])]);
    fix.commit("baseline cards and plan");

    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 0,
        "fresh fixture must pass, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));
}

#[test]
fn closing_an_unknown_slug_simulates() {
    let fix = fixture();
    fix.card(
        "demo",
        "`xtask/src/cards.rs`",
        &["none."],
        &["1. Step."],
        &["just check"],
    );
    fix.plan(&[("P1 Read", &["demo: valid card"])]);
    fix.commit("valid card");

    // A slug that never existed passes with --closing
    let (code, stdout, stderr) = fix.run(&["cards", "--check", "--closing", "unknown-slug"]);
    assert_eq!(
        code, 0,
        "unknown slug with --closing must simulate pass, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));

    // But another card whose file was removed by hand without a deleting commit still fails the plan lint when its slug is in PLAN.md
    fix.plan(&[(
        "P1 Read",
        &["demo: valid card", "removed-slug: missing file"],
    )]);
    fix.commit("add removed-slug to plan without card file or deleting commit");

    let (code, stdout, stderr) = fix.run(&["cards", "--check", "--closing", "unknown-slug"]);
    assert_eq!(
        code, 1,
        "missing card must fail plan lint, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("removed-slug"),
        "stderr must mention removed-slug: {stderr}"
    );

    // Verify delete_card records completion in history so the completed card passes
    fix.plan(&[("P1 Read", &["demo: valid card"])]);
    fix.delete_card("demo");
    let (code, stdout, stderr) = fix.run(&["cards", "--check"]);
    assert_eq!(
        code, 0,
        "completed card in plan must pass, got {code}:\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cards: ok"));
}
