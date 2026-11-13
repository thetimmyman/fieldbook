//! Determinism and verb classification integration tests (`ai-kernel`, AD-01, AD-17).

#![expect(
    clippy::disallowed_methods,
    reason = "determinism tests create temporary fixture directories for Ctx validation"
)]

mod support;

use std::collections::BTreeSet;
use std::path::Path;

use clap::CommandFactory;
use fieldbook::World;
use fieldbook::ctx::{AiCtx, Ctx};
use fieldbook::io::journal::Journal;
use fieldbook_core::audit::Via;

use support::faulty::{Fault, FaultyWorld};
use support::world::TestWorld;

const ADR_0001: &str = include_str!("../../../docs/decisions/0001-foundation.md");

fn parse_ad17_verbs(doc: &str) -> Vec<&str> {
    let mut in_block = false;
    let mut out = Vec::new();
    for line in doc.lines() {
        let trimmed = line.trim();
        if !in_block {
            if trimmed == "```verbs" {
                in_block = true;
            }
        } else if trimmed == "```" {
            break;
        } else if !trimmed.is_empty() {
            out.push(trimmed);
        }
    }
    out
}

fn collect_clap_leaves(cmd: &clap::Command, prefix: &str, out: &mut Vec<String>) {
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }
        let path = if prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{prefix} {name}")
        };
        if sub.has_subcommands() {
            if !sub.is_subcommand_required_set() {
                out.push(path.clone());
            }
            collect_clap_leaves(sub, &path, out);
        } else {
            out.push(path);
        }
    }
}

fn classify(leaves: &[String], ai: &[&str], det: &[&str]) -> Result<(), String> {
    // Err when add --propose's leaf is not in the deterministic list
    for &entry in ai {
        if let Some((base, _flag)) = entry.split_once(" --")
            && !det.contains(&base)
        {
            return Err(format!(
                "flagged AI verb `{entry}` leaf `{base}` is not in deterministic list"
            ));
        }
    }

    // Err when an entry of one list is also an entry of the other
    // (compare the flagged entry's leaf, ie add --propose has leaf add, which
    // is allowed in DETERMINISTIC_VERBS and must NOT be treated as a clash)
    for &entry in ai {
        if !entry.contains(" --") && det.contains(&entry) {
            return Err(format!(
                "verb `{entry}` is in both AI_VERBS and DETERMINISTIC_VERBS"
            ));
        }
    }
    for &entry in det {
        if ai.contains(&entry) {
            return Err(format!(
                "verb `{entry}` is in both DETERMINISTIC_VERBS and AI_VERBS"
            ));
        }
    }

    // Err naming the leaf when a clap leaf is in neither list or in both
    for leaf in leaves {
        let in_ai = ai.contains(&leaf.as_str());
        let in_det = det.contains(&leaf.as_str());
        if in_ai && in_det {
            return Err(format!("clap leaf `{leaf}` is in both lists"));
        }
        if !in_ai && !in_det {
            return Err(format!("clap leaf `{leaf}` is in neither list"));
        }
    }

    Ok(())
}

#[test]
fn every_verb_is_in_exactly_one_list() {
    let mut leaves = Vec::new();
    collect_clap_leaves(&fieldbook::Cli::command(), "", &mut leaves);

    assert!(
        classify(
            &leaves,
            fieldbook::verbs::AI_VERBS,
            fieldbook::verbs::DETERMINISTIC_VERBS
        )
        .is_ok()
    );

    // Synthetic extra leaf "hello" in neither list is an Err naming it:
    let mut extra_leaves = leaves.clone();
    extra_leaves.push("hello".to_string());
    let err = classify(
        &extra_leaves,
        fieldbook::verbs::AI_VERBS,
        fieldbook::verbs::DETERMINISTIC_VERBS,
    )
    .unwrap_err();
    assert!(err.contains("hello"), "expected err naming hello: {err}");

    // Synthetic: a leaf in both lists is an Err:
    let mut synthetic_ai = fieldbook::verbs::AI_VERBS.to_vec();
    synthetic_ai.push("check");
    let err = classify(
        &leaves,
        &synthetic_ai,
        fieldbook::verbs::DETERMINISTIC_VERBS,
    )
    .unwrap_err();
    assert!(err.contains("check"), "expected err naming check: {err}");
}

#[test]
fn lists_name_only_ad17_verbs() {
    let ad17 = parse_ad17_verbs(ADR_0001);
    assert!(
        !ad17.is_empty(),
        "failed to find ```verbs block in ADR 0001"
    );
    let ad17_set: BTreeSet<&str> = ad17.iter().copied().collect();

    for &ai_verb in fieldbook::verbs::AI_VERBS {
        let leaf = ai_verb.split_once(" --").map_or(ai_verb, |(base, _)| base);
        assert!(
            ad17_set.contains(leaf),
            "AI verb `{ai_verb}` (leaf `{leaf}`) is not in the AD-17 verbs block"
        );
    }

    for &det_verb in fieldbook::verbs::DETERMINISTIC_VERBS {
        assert!(
            ad17_set.contains(det_verb),
            "deterministic verb `{det_verb}` is not in the AD-17 verbs block"
        );
    }
}

#[test]
fn ai_ctx_wraps_ctx_and_ctx_has_no_model() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let corpus = tmp.path().join("corpus");
    let state = tmp.path().join("state");
    std::fs::create_dir_all(&corpus).expect("create corpus");
    std::fs::create_dir_all(&state).expect("create state");

    let world = TestWorld::new(&corpus);
    let journal = Journal::open(&world, &state, Via::Cli).expect("open journal");

    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini");
    let tree = fieldbook::io::read_tree(&fixture_path).expect("read fixture tree");

    let ctx = Ctx::new(&world, journal, &tree);

    struct NoSend;

    trait Sink {
        fn send(&self);
    }

    impl Sink for NoSend {
        fn send(&self) {}
    }

    let model = NoSend;
    let sink: &dyn Sink = &model;
    let ai_ctx: AiCtx<'_, dyn Sink> = AiCtx::new(ctx, sink);

    assert!(std::ptr::eq(ai_ctx.ctx.corpus, &tree));
    assert_eq!(ai_ctx.ctx.io.store_id(), world.store_id());
    assert_eq!(ai_ctx.ctx.io.store_id(), "memory@0");
    ai_ctx.model.send();

    let ctx_source = include_str!("../src/ctx.rs");
    let start = ctx_source
        .find("pub struct Ctx<'a> {")
        .expect("find pub struct Ctx<'a> {");
    let end = ctx_source[start..]
        .find('}')
        .expect("find closing brace for Ctx");
    let ctx_def = &ctx_source[start..start + end];
    assert!(
        !ctx_def.to_lowercase().contains("model"),
        "Ctx struct body must not contain the word 'model'"
    );

    let def_line = ctx_source[start..]
        .lines()
        .next()
        .expect("first line of Ctx definition")
        .trim();
    assert_eq!(
        def_line, "pub struct Ctx<'a> {",
        "Ctx definition line must have no type parameter besides the lifetime"
    );
}

#[test]
fn world_store_ids() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let test_world = TestWorld::new(tmp.path());
    assert_eq!(test_world.store_id(), "memory@0");

    let disk = fieldbook::io::Disk::with_state(tmp.path(), tmp.path().join("state"));
    assert_eq!(disk.store_id(), "folder@1");

    let faulty = FaultyWorld::new(TestWorld::new(tmp.path()), Fault::BrokenPrompt);
    assert_eq!(faulty.store_id(), "memory@0");
}
