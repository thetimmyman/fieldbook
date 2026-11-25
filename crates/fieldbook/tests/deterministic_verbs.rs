//! Mechanism M4: deterministic verbs never call a model (`model-gate` step 11, `ADR 0001 AD-01`).
//!
//! Every built deterministic leaf verb runs in process on a temporary copy of its fixture, twice,
//! once without a settings file and once with the AI settings fixture and its policy. Neither run
//! may append an `ai.begin`, `ai.call` or `ai.paste` event.

#![cfg(feature = "test-hooks")]
#![allow(
    clippy::err_expect,
    clippy::manual_let_else,
    clippy::needless_borrow,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "determinism tests copy fixtures and inspect their journals"
)]

mod support;

#[path = "support/policy.rs"]
mod policy_support;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use clap::CommandFactory;
use fieldbook::cli::Cli;
use fieldbook::verbs::DETERMINISTIC_VERBS;
use fieldbook_core::audit::{Event, EventPayload};
use policy_support::write_policy;
use support::world::TestWorld;

const POLICY_GATE: &str = include_str!("../../../tests/fixtures/policy/gate.toml");
const POLICY_NEXT: &str = include_str!("../../../tests/fixtures/policy/policy-next.toml");

/// One deterministic leaf verb the test runs.
struct Entry {
    /// Full leaf verb path (`policy show`).
    verb: &'static str,
    /// Arguments the call needs; `@TODAY`, `@ROOT`, `@OUT` and `policy-next` are replaced.
    args: &'static [&'static str],
    /// Fixture folder under `tests/fixtures/` (`corpus-mini` unless the row says otherwise).
    fixture: &'static str,
    /// Whether the output tree is compared between the two runs.
    builds: bool,
}

/// The built deterministic leaves this card runs.
const ENTRIES: &[Entry] = &[
    Entry {
        verb: "check",
        args: &["--today", "@TODAY"],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "build",
        args: &["--out", "@OUT"],
        fixture: "corpus-mini",
        builds: true,
    },
    Entry {
        verb: "fmt",
        args: &["--check"],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "status",
        args: &["--today", "@TODAY"],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "search",
        args: &["rate"],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "policy show",
        args: &[],
        fixture: "state-policy",
        builds: false,
    },
    Entry {
        verb: "policy check",
        args: &["policy-next"],
        fixture: "state-policy",
        builds: false,
    },
    Entry {
        verb: "ai status",
        args: &[],
        fixture: "state-policy",
        builds: false,
    },
    Entry {
        verb: "audit verify",
        args: &[],
        fixture: "state-mini",
        builds: false,
    },
    Entry {
        verb: "audit show",
        args: &["--head"],
        fixture: "state-mini",
        builds: false,
    },
    Entry {
        verb: "new page",
        args: &[
            "--confirm-public",
            "--type",
            "concept",
            "--subarea",
            "fnd",
            "fnd-test-page",
        ],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "new claim",
        args: &[
            "--confirm-public",
            "--area",
            "cmp",
            "--source",
            "S-0001-v1",
            "--at",
            "sec. 1",
            "--quote",
            "Synthetic quote.",
        ],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "new question",
        args: &["--confirm-public", "--area", "cmp", "Synthetic question?"],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "new conflict",
        args: &["--confirm-public", "--area", "cmp"],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "new term",
        args: &[
            "--confirm-public",
            "--area",
            "cmp",
            "--id",
            "synthetic_term",
        ],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "add",
        args: &["--confirm-public", "@ROOT/corpus/pages/fnd-overview.md"],
        fixture: "corpus-mini",
        builds: false,
    },
    Entry {
        verb: "context",
        args: &["synthetic"],
        fixture: "corpus-mini",
        builds: false,
    },
];

/// Built deterministic verbs with no runnable row, and the one-line reason.
const NO_ENTRY: &[(&str, &str)] = &[
    ("verify", "it asks for confirmation on a terminal"),
    (
        "backup",
        "it needs a target folder a fixed argument list cannot name",
    ),
    (
        "backup verify",
        "it needs a target folder a fixed argument list cannot name",
    ),
    (
        "backup list",
        "it needs a target folder a fixed argument list cannot name",
    ),
    (
        "restore",
        "it needs a target folder a fixed argument list cannot name",
    ),
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture(name: &str) -> PathBuf {
    repo_root().join("tests/fixtures").join(name)
}

fn today(fixture: &Path) -> String {
    let root = if fixture.join("TODAY").is_file() {
        fixture.to_path_buf()
    } else {
        repo_root().join("tests/fixtures/corpus-mini")
    };
    std::fs::read_to_string(root.join("TODAY"))
        .expect("TODAY")
        .trim()
        .to_owned()
}

struct RunResult {
    state_dir: PathBuf,
    out_dir: PathBuf,
}

fn run_entry(entry: &Entry, with_ai: bool) -> RunResult {
    let fixture_path = fixture(entry.fixture);
    let copy = if entry.fixture == "corpus-mini" {
        support::temp_copy(&fixture_path)
    } else {
        support::temp_copy(&fixture("corpus-mini"))
    };
    let root = copy.path().to_path_buf();
    let (state_dir, policy) = if entry.fixture == "corpus-mini" && with_ai {
        let state = root.join("state-ai");
        support::copy_dir_recursive(&fixture("state-ai-gate"), &state).expect("copy state");
        let policy_dir = root.join("outside-state");
        let path = write_policy(&policy_dir, "gate.toml", POLICY_GATE);
        (state, Some(path))
    } else if entry.fixture == "corpus-mini" {
        (root.join(".fieldbook"), None)
    } else {
        let state = root.join(entry.fixture);
        support::copy_dir_recursive(&fixture_path, &state).expect("copy state fixture");
        (state, None)
    };

    let out = root.join("out");
    let mut argv: Vec<String> = vec!["--state".to_owned(), state_dir.to_string_lossy().into()];
    if let Some(policy) = &policy {
        argv.push("--policy".to_owned());
        argv.push(policy.to_string_lossy().into());
    }
    argv.extend(entry.verb.split(' ').map(str::to_owned));
    for arg in entry.args {
        let value = match *arg {
            "@TODAY" => today(&fixture_path),
            "@ROOT" => root.to_string_lossy().into(),
            "@OUT" => out.to_string_lossy().into(),
            "policy-next" => {
                let dir = root.join("outside-state");
                write_policy(&dir, "policy-next.toml", POLICY_NEXT)
                    .to_string_lossy()
                    .into()
            }
            other => other.to_owned(),
        };
        argv.push(value);
    }

    let world = TestWorld::with_state(&root, &state_dir);
    let _ = fieldbook::run(&argv, &world);
    RunResult {
        state_dir,
        out_dir: out,
    }
}

fn ai_events(state_dir: &Path) -> Vec<String> {
    let audit = state_dir.join("audit");
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&audit) else {
        return found;
    };
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok((event, _)) = Event::parse_line(line)
                && matches!(
                    event.payload,
                    EventPayload::AiBegin { .. }
                        | EventPayload::AiCall { .. }
                        | EventPayload::AiPaste { .. }
                )
            {
                found.push(event.kind().as_str().to_owned());
            }
        }
    }
    found
}

fn tree_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, current: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(current) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let rel = path
                    .strip_prefix(root)
                    .expect("relative")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, bytes);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn deterministic_verbs_never_call_a_model() {
    for entry in ENTRIES {
        let plain = run_entry(entry, false);
        let with_ai = run_entry(entry, true);
        assert!(
            ai_events(&plain.state_dir).is_empty(),
            "{} appended an AI event without settings",
            entry.verb
        );
        assert!(
            ai_events(&with_ai.state_dir).is_empty(),
            "{} appended an AI event with settings",
            entry.verb
        );
        if entry.builds {
            assert_eq!(
                tree_files(&plain.out_dir),
                tree_files(&with_ai.out_dir),
                "{} produced a different tree in the two runs",
                entry.verb
            );
        }
    }
}

fn leaf_verbs() -> Vec<String> {
    fn collect(command: &clap::Command, prefix: &str, out: &mut Vec<String>) {
        for sub in command.get_subcommands() {
            let name = sub.get_name();
            if name == "help" {
                continue;
            }
            let path = format!("{prefix}{name}");
            if sub.has_subcommands() {
                collect(sub, &format!("{path} "), out);
            } else {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    collect(&Cli::command(), "", &mut out);
    out
}

#[test]
fn every_built_deterministic_verb_has_a_row_or_a_reason() {
    let entries: Vec<&str> = ENTRIES.iter().map(|entry| entry.verb).collect();
    let reasons: Vec<&str> = NO_ENTRY.iter().map(|(verb, _)| *verb).collect();
    for leaf in leaf_verbs() {
        if !DETERMINISTIC_VERBS.contains(&leaf.as_str()) {
            continue;
        }
        assert!(
            entries.contains(&leaf.as_str()) || reasons.contains(&leaf.as_str()),
            "built deterministic verb `{leaf}` has neither an ENTRIES row nor a NO_ENTRY reason"
        );
    }
}
