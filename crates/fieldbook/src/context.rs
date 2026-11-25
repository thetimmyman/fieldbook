//! Implementation of `fieldbook context` (`handoff-context` steps 6, 7, 9, 13, 14, 15, 16).
//!
//! **Responsibility.** Builds a plain-text (or `--json`) context pack from the `LabelledView`
//! for `Dest::Handoff`, enforces marking and egress checks in exact order, appends `ai.pack`
//! before printing, logs the question via `asked::log_question`, and delivers the final bytes
//! only through a delivery-bound [`Allowed`] token.
//!
//! **Rules.** Deterministic verb running with plain [`Ctx`]: never calls a model, never opens a
//! socket, writes only the audit journal and the question log, and reads the Corpus coherently
//! through [`World::read_tree`].

use std::collections::BTreeSet;

use fieldbook_core::assist::{Budget, ContextPack, Include, Scope, context_pack_for, export_units};
use fieldbook_core::audit::{
    DestKind, EventPayload, Kind, RecordRef, RefuseAbout, Sha256, Via, Word,
};
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Code, Host, Options, load};
use fieldbook_core::egress::{
    self, Allowed, Decision, Delivery, Dest, LabelledView, Operation, Target, Transfer,
};
use fieldbook_core::labels::{self, ItemRef, LabelContext, LabelSet, MARKED, StoreId};
use fieldbook_core::report::ContextReport;

use crate::asked::{Cited, Hits, TOP_IDS, Via as AskedVia};
use crate::cli::{ContextArgs, EXIT_IO, EXIT_OK, EXIT_USAGE};
use crate::ctx::Ctx;
use crate::io::journal::read_valid_events_prefix;
use crate::io::state::{self, read_asked_settings};
use crate::policy::PolicyCtx;
use crate::{Outcome, World};

/// Default character budget when `corpus.toml` does not provide `[assistant] pack_budget_chars`.
const DEFAULT_BUDGET: usize = 12_000;

/// Canonical reply channel target bound into the [`Delivery`] of `fieldbook context`.
pub const HANDOFF_REPLY_TARGET: &str = "handoff:stdout";

/// Delivers the rendered context output only when `allowed` is delivery-bound for `target`
/// (`Operation::Reply`) and the exact `bytes` (`handoff-context` step 13).
///
/// Selection-only tokens (`has_delivery() == false`), modified bytes, or a different reply target
/// are refused with [`Code::Fb0902`].
///
/// # Errors
///
/// Returns [`Code::Fb0902`] when `allowed` does not permit the exact transfer or `bytes` are not
/// valid UTF-8.
pub fn deliver_pack(allowed: &Allowed, target: &str, bytes: &[u8]) -> Result<String, Code> {
    let reply_target = Target::Reply(target.to_owned());
    if !allowed.permits_transfer(&reply_target, &Operation::Reply, bytes) {
        return Err(Code::Fb0902);
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| Code::Fb0902)
}

/// Formats `pack` as the `--json` payload of `fieldbook context` (pretty JSON + trailing LF).
#[must_use]
pub fn render_context_json(pack: &ContextPack) -> String {
    let report = ContextReport::from_pack(pack.clone());
    let mut json = serde_json::to_string_pretty(&report).unwrap_or_default();
    json.push('\n');
    json
}

fn usage_error(msg: impl Into<String>) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{}\n", msg.into()),
        code: EXIT_USAGE,
    }
}

fn io_error(msg: impl Into<String>) -> Outcome {
    Outcome {
        stdout: String::new(),
        stderr: format!("{}\n", msg.into()),
        code: EXIT_IO,
    }
}

fn parse_budget_flag(raw: &str) -> Result<usize, String> {
    let parsed = raw
        .parse::<usize>()
        .map_err(|_| format!("invalid --budget value `{raw}`: expected a positive integer"))?;
    if parsed == 0 {
        return Err("invalid --budget value `0`: expected a positive integer".to_owned());
    }
    Ok(parsed)
}

fn append_refusal(
    world: &dyn World,
    code: Code,
    reason: Word,
    labels: Option<LabelSet>,
    message: &str,
) -> Outcome {
    let journal = match state::open(world, world.state_dir(), Via::Cli) {
        Ok(j) => j,
        Err(err) => {
            return io_error(format!(
                "error[{}]: cannot open audit journal: {err}",
                Code::Fb0903.id()
            ));
        }
    };
    let about = RefuseAbout {
        labels,
        dest: Some(DestKind::Handoff),
        ..RefuseAbout::default()
    };
    if let Err(err) = journal.refuse(Some("context"), code, reason, &about) {
        return io_error(format!(
            "error[{}]: cannot append refusal to audit journal: {err}",
            Code::Fb0903.id()
        ));
    }
    Outcome {
        stdout: String::new(),
        stderr: format!("error[{}]: {message}\n", code.id()),
        code: EXIT_USAGE,
    }
}

fn record_ids_from_pack(pack: &ContextPack) -> Vec<RecordRef> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for item in pack.items() {
        if !item.included {
            continue;
        }
        let base = item
            .unit
            .split_once('#')
            .map_or(item.unit.as_str(), |(b, _)| b);
        let raw = format!("corpus/{base}");
        if seen.insert(raw.clone())
            && let Ok(rec) = RecordRef::parse(&raw)
        {
            out.push(rec);
        }
    }
    out
}

fn scope_exists_in_view(view: &LabelledView, scope: &Scope) -> bool {
    match scope {
        Scope::All => true,
        Scope::Area(code) => {
            view.corpus()
                .is_some_and(|c| c.areas().iter().any(|a| a.code.as_str() == code))
                || export_units(view).iter().any(|u| u.area == *code)
        }
        Scope::Page(id) => view.pages().iter().any(|p| p.id.as_str() == id),
    }
}

/// Executes `fieldbook context` against `world` under `policy_ctx`.
#[must_use]
pub fn run(args: &ContextArgs, policy_ctx: &PolicyCtx, world: &dyn World) -> Outcome {
    // 1. Validate command-line usage before touching disk or opening the journal.
    let question = args.question.as_str();
    if question.trim().is_empty() {
        return usage_error("usage: question must not be empty");
    }

    let scope = match args.scope.as_deref() {
        Some(raw) => match Scope::parse(raw) {
            Ok(s) => s,
            Err(err) => return usage_error(err.to_string()),
        },
        None => Scope::All,
    };

    let include = match args.include.as_deref() {
        Some(raw) => match Include::parse(raw) {
            Ok(inc) => inc,
            Err(err) => return usage_error(err.to_string()),
        },
        None => Include::default(),
    };

    let budget_override = match args.budget.as_deref() {
        Some(raw) => match parse_budget_flag(raw) {
            Ok(n) => Some(n),
            Err(err) => return usage_error(err),
        },
        None => None,
    };

    // 2. Read a coherent Corpus generation under the store lease (refuses pending transactions
    //    and waits for any in-flight two-file writer to finish).
    let tree = match world.read_tree() {
        Ok(tree) => tree,
        Err(err) => return io_error(err.to_string()),
    };

    let (model, _) = load(
        &Host::new(tree.clone(), world.today()),
        &Options::new(false),
    );
    let corpus_fp = crate::asked::fingerprint(&tree).ok();

    let effective = world.effective_policy();
    let now = world.now();
    let last_export = read_valid_events_prefix(world.state_dir())
        .iter()
        .rev()
        .find(|ev| ev.kind() == Kind::AuditExport)
        .map(|ev| ev.time);
    let shipped = egress::shipped(last_export, now);

    // 3. Step 6(a): Label the question with `labels::for_text` and inspect `labels::markings_in`.
    let rules = effective.marking_rules();
    let found = labels::markings_in(question, &rules);
    let text_labels = labels::for_text(question, &rules, &LabelContext::none());
    let question_labels = if found.is_some() {
        LabelSet::owner_supplied().union(&text_labels)
    } else {
        LabelSet::owner_supplied()
    };

    if let Some(ref marked_set) = found {
        if marked_set.contains(&MARKED) {
            return append_refusal(
                world,
                Code::Fb0700,
                Word::new_static("marked"),
                Some(question_labels),
                "question carries an unmapped marking",
            );
        }
        if !effective.state().is_closed()
            && !effective.allow_set(&Dest::Handoff).permits(marked_set)
        {
            return append_refusal(
                world,
                Code::Fb0700,
                Word::new_static("label"),
                Some(question_labels),
                "question carries a marking outside the handoff destination allow set",
            );
        }
    }

    // 4. Step 6(b): Destination-only view token (`labels = None, delivery = None`).
    let view_token = match egress::check(&effective, &Dest::Handoff, None, None, now, shipped) {
        Decision::Allowed(token) => token,
        Decision::Denied(denial) => {
            return append_refusal(
                world,
                denial.code,
                denial.reason.to_word(),
                None,
                &denial.to_string(),
            );
        }
    };

    // 5. Step 6(c): Build the LabelledView, validate `--scope` against the view, and build the pack.
    let view = LabelledView::new(&model, &view_token, &ReadableStores::corpus());
    if !scope_exists_in_view(&view, &scope) {
        return usage_error(format!("unknown scope `{scope}`: not found in the view"));
    }

    let budget_limit = budget_override.unwrap_or_else(|| {
        model
            .corpus()
            .and_then(|c| usize::try_from(c.assistant.pack_budget_chars).ok())
            .filter(|&n| n > 0)
            .unwrap_or(DEFAULT_BUDGET)
    });

    let preamble = view
        .tree()
        .get_bytes("corpus/ASSISTANT.md")
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let budget = Budget::for_question(budget_limit, preamble, question);

    let pack = context_pack_for(
        &view,
        &Dest::Handoff,
        Some(&effective),
        question,
        &question_labels,
        &scope,
        budget,
        &include,
    );

    if !pack.request_budget().sendable {
        let rb = pack.request_budget();
        let reason = if rb.over_by > 0 {
            format!(
                "context pack exceeds budget limit of {} characters by {} characters",
                rb.limit, rb.over_by
            )
        } else {
            "context pack has an unavailable pinned item and cannot be sent".to_owned()
        };
        return usage_error(reason);
    }

    // 6. Step 6(d) & Step 13: Render exact bytes, build bound `Delivery`, take the audit token.
    let pack_text = pack.to_text();
    let output_text = if args.json {
        render_context_json(&pack)
    } else {
        pack_text.clone()
    };

    let out_markings = labels::markings_in(&pack_text, &rules);
    let delivery = Delivery::new(
        vec![Transfer::reply(
            HANDOFF_REPLY_TARGET,
            output_text.as_bytes(),
        )],
        out_markings,
    );

    let audit_token = match egress::check(
        &effective,
        &Dest::Handoff,
        Some(pack.labels()),
        Some(&delivery),
        now,
        shipped,
    ) {
        Decision::Allowed(token) => token,
        Decision::Denied(denial) => {
            return append_refusal(
                world,
                denial.code,
                denial.reason.to_word(),
                Some(pack.labels().clone()),
                &denial.to_string(),
            );
        }
    };

    // 7. Open the journal, build the plain deterministic `Ctx`, and append `ai.pack` BEFORE printing.
    let journal = match state::open(world, world.state_dir(), Via::Cli) {
        Ok(j) => j,
        Err(err) => {
            return io_error(format!(
                "error[{}]: cannot open audit journal: {err}",
                Code::Fb0903.id()
            ));
        }
    };
    let ctx = Ctx::with_policy(world, journal, &tree, policy_ctx.clone());

    let audit_labels = audit_token
        .labels()
        .cloned()
        .unwrap_or_else(LabelSet::unlabelled);
    let policy_sha = Sha256::parse(audit_token.policy_hash())
        .unwrap_or_else(|_| Sha256::digest(audit_token.policy_hash().as_bytes()));
    let context_ids = record_ids_from_pack(&pack);

    let ai_pack_payload = EventPayload::AiPack {
        prompt_sha256: Sha256::digest(pack_text.as_bytes()),
        context_ids,
        dest: audit_token.kind(),
        labels: audit_labels.clone(),
        policy: policy_sha,
    };

    if let Err(err) = ctx.journal.append_payload(ai_pack_payload) {
        return io_error(format!(
            "error[{}]: cannot append ai.pack event: {err}",
            Code::Fb0903.id()
        ));
    }

    // 8. Step 9: Log the question after `ai.pack` and before printing; failure warns only.
    let included_items: Vec<&str> = pack
        .items()
        .iter()
        .filter(|i| i.included)
        .map(|i| i.unit.as_str())
        .collect();
    let inputs: Vec<ItemRef> = included_items
        .iter()
        .map(|id| ItemRef::new(StoreId::Corpus, (*id).to_owned()))
        .collect();
    let top_ids: Vec<String> = included_items
        .iter()
        .take(TOP_IDS)
        .map(|id| (*id).to_owned())
        .collect();
    let hits = Hits {
        count: included_items.len() as u64,
        top_ids,
        fingerprint: corpus_fp.unwrap_or_else(|| pack.view_fingerprint().to_owned()),
    };

    let ask_warning = match read_asked_settings(world.state_dir()) {
        Ok(asked_settings) => {
            crate::asked::log_question(
                ctx.io,
                &ctx.journal,
                &asked_settings,
                &effective,
                AskedVia::Context,
                &audit_labels,
                &inputs,
                question,
                &hits,
                Cited::NotChecked,
            )
            .warning
        }
        Err(err) => Some(crate::asked::warning(err)),
    };

    // 9. Step 13: Pass the bound `audit_token` to the final reply sink.
    let stdout = match deliver_pack(&audit_token, HANDOFF_REPLY_TARGET, output_text.as_bytes()) {
        Ok(text) => text,
        Err(code) => {
            return usage_error(format!(
                "error[{}]: outbound delivery does not match the bound token",
                code.id()
            ));
        }
    };

    let included_count = included_items.len();
    let left_out_count = pack.items().iter().filter(|i| !i.included).count() + view.left_out();
    let mut stderr = format!(
        "context: {included_count} items, {} characters, {left_out_count} left out\n",
        pack.chars()
    );
    if let Some(warning) = ask_warning {
        stderr.push_str(&warning);
        stderr.push('\n');
    }

    Outcome {
        stdout,
        stderr,
        code: EXIT_OK,
    }
}
