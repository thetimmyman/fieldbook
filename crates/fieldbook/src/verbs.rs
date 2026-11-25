//! Verb classification and determinism boundaries (`ai-kernel`, AD-01, AD-17).
//!
//! Representation rule: an entry in [`AI_VERBS`] or [`DETERMINISTIC_VERBS`] is
//! either one runnable leaf verb path spelled exactly as the clap leaf command,
//! or the one flagged AI invocation written as its leaf plus its flag
//! (`add --propose`).
//!
//! Group words (such as `policy`, `label`, `new`, `ai`, `audit`, or `queue`)
//! never appear on their own in these lists; they are expanded to their full
//! leaf paths (e.g. `policy show`, `queue accept`), except that `queue ingest`
//! is the AI leaf of its own group.
//!
//! Ordinary `add` is a deterministic leaf; `add --propose` is not a second clap
//! leaf but the explicitly flagged AI invocation of that same leaf.

/// AI verbs that may invoke a model or require AI execution contexts.
pub const AI_VERBS: &[&str] = &[
    "ask",
    "decide",
    "ai test",
    "chat",
    "queue ingest",
    "add --propose",
];

/// Deterministic verbs that never touch a model, key, or network socket.
pub const DETERMINISTIC_VERBS: &[&str] = &[
    "check",
    "build",
    "fmt",
    "verify",
    "status",
    "search",
    "policy show",
    "policy check",
    "label add",
    "label reapply",
    "label show",
    "sync",
    "audit verify",
    "audit show",
    "audit export",
    "backup",
    "backup verify",
    "backup list",
    "restore",
    "context",
    "cite-check",
    "ai status",
    "mcp",
    "new page",
    "new claim",
    "new question",
    "new conflict",
    "new term",
    "new note",
    "add",
    "queue list",
    "queue show",
    "queue review",
    "queue accept",
    "queue reject",
    "queue flag",
    "queue pin",
];

/// Source files, relative to `crates/fieldbook/src`, allowed to name the model gate.
///
/// Each later card that adds a file naming the model gate appends it here.
pub const AI_VERB_FILES: &[&str] = &["io/ai_journal.rs", "io/pace.rs", "io/call_store.rs"];
