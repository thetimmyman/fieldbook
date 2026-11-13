//! The frozen strings of `docs/reference/corpus-format.md`.
//!
//! **Responsibility.** One source for every frozen word, heading, chip and fixed label
//! that the tool and the reader pages share. `build` writes [`js`] to `assets/strings.js`
//! and `cargo xtask docs` regenerates the three tables of the Strings section from the
//! tables in this module.
//!
//! **Rules.** Pure: no I/O, no clock, no environment. Never edit a frozen string without
//! going through the format gate (AD-10, AD-21).

use std::fmt;

/// A row of the first (key) table in the Strings section of `corpus-format.md`.
///
/// The four continuation rows under `brief_parts` carry `key: None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRow {
    /// The `strings` key, or `None` for a continuation row.
    pub key: Option<&'static str>,
    /// The label in the `String` column.
    pub string: &'static str,
    /// The text in the `Value` column.
    pub value: &'static str,
}

/// The release window or phase at which a command chip arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseFrom {
    /// Day 1.
    Day1,
    /// Day 30.
    Day30,
    /// Day 90.
    Day90,
    /// Release 2 (`R2`).
    R2,
}

impl ReleaseFrom {
    /// The exact text printed in the `From` column of the verb map.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Day1 => "Day 1",
            Self::Day30 => "Day 30",
            Self::Day90 => "Day 90",
            Self::R2 => "R2",
        }
    }
}

impl fmt::Display for ReleaseFrom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A row of the verb map (`chips`) in the Strings section of `corpus-format.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChipRow {
    /// What the command does (`Intent` column).
    pub intent: &'static str,
    /// The command template (`Chip string` column, without surrounding backticks).
    pub chip: &'static str,
    /// When the command arrives (`From` column).
    pub from: ReleaseFrom,
}

/// A row of the `Other fixed wording` table in the Strings section of `corpus-format.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtherRow {
    /// The `strings` key (`Key` column, without surrounding backticks).
    pub key: &'static str,
    /// The text in the `Wording` column.
    pub wording: &'static str,
    /// The decision in ADR 0001 or ADR 0004 (`Decision` column).
    pub decision: &'static str,
}

/// The first table of the Strings section, in document order, continuation rows included.
pub const KEY_TABLE: [KeyRow; 13] = [
    KeyRow {
        key: Some("abstention"),
        string: "Abstention phrase",
        value: "\"Not established in the library.\" Matched case-insensitively; final punctuation optional",
    },
    KeyRow {
        key: Some("empty_part"),
        string: "Empty part",
        value: "\"Not established.\"",
    },
    KeyRow {
        key: Some("refusal"),
        string: "Refusal (`cite-check`)",
        value: "\"Do not use this answer.\"",
    },
    KeyRow {
        key: Some("parts"),
        string: "The eight parts, in order",
        value: "Purpose · The rule · How it works · Worked example · Never confuse · Company calibration · Sources · Change note",
    },
    KeyRow {
        key: Some("brief_parts"),
        string: "`brief` parts",
        value: "Question · Recommendation · Evidence · Options · Unknowns · Next action",
    },
    KeyRow {
        key: None,
        string: "`note`",
        value: "No fixed parts",
    },
    KeyRow {
        key: None,
        string: "Page links",
        value: "Plain relative `.md` links, rewritten to `.html`",
    },
    KeyRow {
        key: None,
        string: "Terms",
        value: "Listed in front matter `terms`",
    },
    KeyRow {
        key: None,
        string: "Precise wording",
        value: "A blockquote that starts `Precise:`",
    },
    KeyRow {
        key: Some("paths"),
        string: "Paths",
        value: "`data/registers.xlsx`, `data/<register>.csv`, `templates/<name>.xlsx`, `day-one.html`, `assistant.html`, `map.html`, `ASSISTANT.md`",
    },
    KeyRow {
        key: Some("rounding"),
        string: "Rounding",
        value: "\"Each extension rounds half away from zero to cents; totals sum the rounded extensions.\"",
    },
    KeyRow {
        key: Some("origin_labels"),
        string: "Origin labels",
        value: "Quoted, Computed, Judgment. FB0380 (missing computed or judgment evidence) and FB0381 (missing or inconsistent quoted evidence) are reserved and become active with the card `r2-records`",
    },
    KeyRow {
        key: Some("binary"),
        string: "Binary",
        value: "`fieldbook`",
    },
];

/// The verb map (`chips`), in document order.
pub const CHIPS: [ChipRow; 35] = [
    ChipRow {
        intent: "Start a page",
        chip: "fieldbook new page --confirm-public <area>-<slug> --type <type> --subarea <subarea>",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Add a question",
        chip: "fieldbook new question --confirm-public --area <a> \"<text>\"",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Start a claim",
        chip: "fieldbook new claim --confirm-public --area <a> --source <version-id> --at \"<locator>\" --quote \"<text>\"",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Start a conflict",
        chip: "fieldbook new conflict --confirm-public --area <a>",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Start a glossary term",
        chip: "fieldbook new term --confirm-public --area <a>",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Add a source file",
        chip: "fieldbook add <file>",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Add a new version of a source",
        chip: "fieldbook add <file> --source <source-id>",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Check the Corpus",
        chip: "fieldbook check",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Format the files",
        chip: "fieldbook fmt",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Rebuild the site",
        chip: "fieldbook build",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Find an id or a word; list the versions of a source",
        chip: "fieldbook search <query>",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "See what needs work",
        chip: "fieldbook status",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Write the desk pages",
        chip: "fieldbook status --page",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Record that the owner verified a claim",
        chip: "fieldbook verify <claim-id> --on YYYY-MM-DD",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Add every file in a folder",
        chip: "fieldbook add <folder>",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Verify the audit journal",
        chip: "fieldbook audit verify",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Show recent activity",
        chip: "fieldbook audit show",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Back up the working data",
        chip: "fieldbook backup",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Check a backup",
        chip: "fieldbook backup verify",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "List backups",
        chip: "fieldbook backup list",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Prove a backup restores",
        chip: "fieldbook restore --snapshot <n> --drill",
        from: ReleaseFrom::Day1,
    },
    ChipRow {
        intent: "Build a context pack",
        chip: "fieldbook context \"<question>\"",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Check a pasted answer",
        chip: "fieldbook cite-check <file>",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Save an answer as a note",
        chip: "fieldbook new note --ai-drafted --confirm-public --area <a> --from <file>",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Propose sources from a folder",
        chip: "fieldbook add <folder> --propose",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Review proposals",
        chip: "fieldbook queue review",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Accept a proposal",
        chip: "fieldbook queue accept <proposal-id>",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Reject a proposal",
        chip: "fieldbook queue reject <proposal-id> --reason <word>",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Flag the last question",
        chip: "fieldbook queue flag",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Ask a question through the gate",
        chip: "fieldbook ask \"<question>\"",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "See the state of each AI port",
        chip: "fieldbook ai status",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "See the policy in force",
        chip: "fieldbook policy show",
        from: ReleaseFrom::Day30,
    },
    ChipRow {
        intent: "Put a decision to the model",
        chip: "fieldbook decide <file>",
        from: ReleaseFrom::Day90,
    },
    ChipRow {
        intent: "Build the overlay site, then print from Edge",
        chip: "fieldbook build --overlay",
        from: ReleaseFrom::R2,
    },
    ChipRow {
        intent: "See what the company data sources would add",
        chip: "fieldbook sync --dry-run",
        from: ReleaseFrom::R2,
    },
];

/// The third table (`Other fixed wording`) of the Strings section, in document order.
pub const OTHER_TABLE: [OtherRow; 18] = [
    OtherRow {
        key: "cite_statuses",
        wording: "broken · broken locator · outside context · superseded · weak · unchecked locator · unchecked quote · quote mismatch · verified, in precedence order",
        decision: "AD-18",
    },
    OtherRow {
        key: "verified_text",
        wording: "\"Citation resolves at this locator\"",
        decision: "AD-18",
    },
    OtherRow {
        key: "non_canonical",
        wording: "\"non-canonical\"",
        decision: "AD-10",
    },
    OtherRow {
        key: "left_out_budget",
        wording: "\"Left out (budget)\"",
        decision: "AD-18",
    },
    OtherRow {
        key: "left_out_policy",
        wording: "\"Left out (policy)\"",
        decision: "ADR 0004",
    },
    OtherRow {
        key: "pack_labels",
        wording: "\"Labels: <ids>\", the ids sorted and joined by a comma and a space. It replaces the earlier handling line of a pack",
        decision: "ADR 0004",
    },
    OtherRow {
        key: "not_available",
        wording: "\"<Thing> is not available until <release or trigger>.\"",
        decision: "AD-09",
    },
    OtherRow {
        key: "unreleased_build",
        wording: "\"unreleased build\"",
        decision: "AD-12",
    },
    OtherRow {
        key: "print_check",
        wording: "\"Check the current edition\"",
        decision: "AD-12",
    },
    OtherRow {
        key: "not_remembered",
        wording: "\"Not remembered in this browser\"",
        decision: "AD-15",
    },
    OtherRow {
        key: "set_start_date",
        wording: "\"Set your start date\"",
        decision: "AD-15",
    },
    OtherRow {
        key: "set_start_date_plan",
        wording: "\"Set a start date to mark today\"",
        decision: "AD-15",
    },
    OtherRow {
        key: "day_n_of_90",
        wording: "\"Day N of 90\", with N the day count the page script computes",
        decision: "AD-15",
    },
    OtherRow {
        key: "day_n_today",
        wording: "\"Day N of 90 · the line marks today\", with N the day count the page script computes",
        decision: "AD-15",
    },
    OtherRow {
        key: "tag_at_home",
        wording: "\"At home\"",
        decision: "AD-17",
    },
    OtherRow {
        key: "tag_r2_laptop",
        wording: "\"R2 · laptop\"",
        decision: "AD-17",
    },
    OtherRow {
        key: "assistant_labels",
        wording: "\"Ask\" · \"Send\" · \"Stop\" · \"Copy context pack\" · \"Check citations\" · \"Save as note\" · \"Copy as draft note\" · \"This session only\"",
        decision: "AD-15",
    },
    OtherRow {
        key: "ai_note_label",
        wording: "\"AI-drafted · unreviewed\" · \"AI-drafted · reviewed\"",
        decision: "AD-15",
    },
];

/// The five frozen vocabulary words (AD-23). "Pack" is never used alone.
pub const FROZEN_WORDS: [&str; 5] = [
    "edition",
    "transfer statement",
    "intake pack",
    "day-one binder",
    "context pack",
];

/// The eight part headings of a standard Corpus page, in order (`parts`).
pub const PARTS: [&str; 8] = [
    "Purpose",
    "The rule",
    "How it works",
    "Worked example",
    "Never confuse",
    "Company calibration",
    "Sources",
    "Change note",
];

/// The six part headings of a `brief` page, in order (`brief_parts`).
pub const BRIEF_PARTS: [&str; 6] = [
    "Question",
    "Recommendation",
    "Evidence",
    "Options",
    "Unknowns",
    "Next action",
];

/// The seven frozen path patterns (`paths`), in order.
pub const PATHS: [&str; 7] = [
    "data/registers.xlsx",
    "data/<register>.csv",
    "templates/<name>.xlsx",
    "day-one.html",
    "assistant.html",
    "map.html",
    "ASSISTANT.md",
];

/// The three figure-origin labels (`origin_labels`), in order.
pub const ORIGIN_LABELS: [&str; 3] = ["Quoted", "Computed", "Judgment"];

/// The nine citation-check statuses in precedence order (`cite_statuses`).
pub const CITE_STATUSES: [&str; 9] = [
    "broken",
    "broken locator",
    "outside context",
    "superseded",
    "weak",
    "unchecked locator",
    "unchecked quote",
    "quote mismatch",
    "verified",
];

/// The eight `(id, text)` label pairs of the Assistant page (`assistant_labels`), in order.
pub const ASSISTANT_LABELS: [(&str, &str); 8] = [
    ("ask", "Ask"),
    ("send", "Send"),
    ("stop", "Stop"),
    ("copy_pack", "Copy context pack"),
    ("check_citations", "Check citations"),
    ("save_note", "Save as note"),
    ("copy_draft_note", "Copy as draft note"),
    ("this_session_only", "This session only"),
];

/// The eight text labels of the Assistant page (`assistant_labels`), in order.
pub const ASSISTANT_LABEL_TEXTS: [&str; 8] = [
    "Ask",
    "Send",
    "Stop",
    "Copy context pack",
    "Check citations",
    "Save as note",
    "Copy as draft note",
    "This session only",
];

/// The two labels for an AI-drafted note (`ai_note_label`): unreviewed first, reviewed second.
pub const AI_NOTE_LABELS: [&str; 2] = ["AI-drafted · unreviewed", "AI-drafted · reviewed"];

/// The note on a source page whose text was supplied by hand (corpus-format.md, Extraction, `supplied`).
pub const EXTRACTION_SUPPLIED: &str = "text supplied by hand; not checked against the original";

/// The abstention phrase (`abstention`).
#[must_use]
pub const fn abstention() -> &'static str {
    "Not established in the library."
}

/// The text shown for an empty page part (`empty_part`).
#[must_use]
pub const fn empty_part() -> &'static str {
    "Not established."
}

/// The refusal line printed by `cite-check` when nothing verifies (`refusal`).
#[must_use]
pub const fn refusal() -> &'static str {
    "Do not use this answer."
}

/// The eight part headings of a standard Corpus page, in order (`parts`).
#[must_use]
pub const fn parts() -> &'static [&'static str; 8] {
    &PARTS
}

/// The six part headings of a `brief` page, in order (`brief_parts`).
#[must_use]
pub const fn brief_parts() -> &'static [&'static str; 6] {
    &BRIEF_PARTS
}

/// The seven frozen path patterns (`paths`), in order.
#[must_use]
pub const fn paths() -> &'static [&'static str; 7] {
    &PATHS
}

/// The rounding rule statement (`rounding`).
#[must_use]
pub const fn rounding() -> &'static str {
    "Each extension rounds half away from zero to cents; totals sum the rounded extensions."
}

/// The three figure-origin labels (`origin_labels`), in order.
#[must_use]
pub const fn origin_labels() -> &'static [&'static str; 3] {
    &ORIGIN_LABELS
}

/// The binary name (`binary`).
#[must_use]
pub const fn binary() -> &'static str {
    "fieldbook"
}

/// The verb map (`chips`), in document order.
#[must_use]
pub const fn chips() -> &'static [ChipRow; 35] {
    &CHIPS
}

/// The nine citation-check statuses in precedence order (`cite_statuses`).
#[must_use]
pub const fn cite_statuses() -> &'static [&'static str; 9] {
    &CITE_STATUSES
}

/// The text shown when a citation resolves at its locator (`verified_text`).
#[must_use]
pub const fn verified_text() -> &'static str {
    "Citation resolves at this locator"
}

/// The flag word for a non-canonical citation (`non_canonical`).
#[must_use]
pub const fn non_canonical() -> &'static str {
    "non-canonical"
}

/// The reason shown when an item is left out of a context pack by the budget (`left_out_budget`).
#[must_use]
pub const fn left_out_budget() -> &'static str {
    "Left out (budget)"
}

/// The reason shown when an item is left out of a context pack by policy (`left_out_policy`).
#[must_use]
pub const fn left_out_policy() -> &'static str {
    "Left out (policy)"
}

/// The labels line of a context pack (`pack_labels`).
#[must_use]
pub const fn pack_labels() -> &'static str {
    "Labels: <ids>"
}

/// The placeholder line for an object or section that has not landed (`not_available`).
#[must_use]
pub const fn not_available() -> &'static str {
    "<Thing> is not available until <release or trigger>."
}

/// The build identity and header text when built outside a release (`unreleased_build`).
#[must_use]
pub const fn unreleased_build() -> &'static str {
    "unreleased build"
}

/// The print footer reminder (`print_check`).
#[must_use]
pub const fn print_check() -> &'static str {
    "Check the current edition"
}

/// The notice shown when `localStorage` is blocked or fails (`not_remembered`).
#[must_use]
pub const fn not_remembered() -> &'static str {
    "Not remembered in this browser"
}

/// The label of the start-date control and of the button that opens it (`set_start_date`).
#[must_use]
pub const fn set_start_date() -> &'static str {
    "Set your start date"
}

/// The plan timeline header while no start date is set (`set_start_date_plan`).
#[must_use]
pub const fn set_start_date_plan() -> &'static str {
    "Set a start date to mark today"
}

/// The day chip, with `N` for the page script to fill (`day_n_of_90`).
#[must_use]
pub const fn day_n_of_90() -> &'static str {
    "Day N of 90"
}

/// The plan timeline header while a start date is set, with `N` for the page script to fill
/// (`day_n_today`).
#[must_use]
pub const fn day_n_today() -> &'static str {
    "Day N of 90 \u{b7} the line marks today"
}

/// The command chip tag for commands that run on the home machine (`tag_at_home`).
#[must_use]
pub const fn tag_at_home() -> &'static str {
    "At home"
}

/// The command chip tag for Release 2 commands on the laptop (`tag_r2_laptop`).
#[must_use]
pub const fn tag_r2_laptop() -> &'static str {
    "R2 · laptop"
}

/// The eight text labels of the Assistant page (`assistant_labels`), in order.
#[must_use]
pub const fn assistant_labels() -> &'static [&'static str; 8] {
    &ASSISTANT_LABEL_TEXTS
}

/// The two labels for an AI-drafted note (`ai_note_label`), in order.
#[must_use]
pub const fn ai_note_label() -> &'static [&'static str; 2] {
    &AI_NOTE_LABELS
}

/// Escape `s` as a double-quoted JS/JSON string literal, writing `</` as `<\/`.
#[must_use]
pub fn escape_js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    let mut prev_lt = false;
    for ch in s.chars() {
        if prev_lt && ch == '/' {
            out.push_str("\\/");
            prev_lt = false;
            continue;
        }
        prev_lt = ch == '<';
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            c if c.is_control() => {
                let code = c as u32;
                out.push_str("\\u");
                for shift in [12, 8, 4, 0] {
                    let nibble = ((code >> shift) & 0x0f) as u8;
                    let hex = match nibble {
                        0..=9 => char::from(b'0' + nibble),
                        _ => char::from(b'a' + nibble - 10),
                    };
                    out.push(hex);
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn push_str_prop(out: &mut String, key: &str, val: &str, comma: bool) {
    out.push_str("  ");
    out.push_str(&escape_js(key));
    out.push_str(": ");
    out.push_str(&escape_js(val));
    if comma {
        out.push(',');
    }
    out.push('\n');
}

fn push_list_prop(out: &mut String, key: &str, items: &[&str], comma: bool) {
    out.push_str("  ");
    out.push_str(&escape_js(key));
    out.push_str(": [\n");
    for (idx, item) in items.iter().enumerate() {
        out.push_str("    ");
        out.push_str(&escape_js(item));
        if idx + 1 < items.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ]");
    if comma {
        out.push(',');
    }
    out.push('\n');
}

/// The text of `assets/strings.js`: one assignment to `window.FB_STRINGS` with all 28 keys
/// in the order of `docs/reference/corpus-format.md`.
#[must_use]
pub fn js() -> String {
    let mut out = String::from("window.FB_STRINGS = {\n");
    push_str_prop(&mut out, "abstention", abstention(), true);
    push_str_prop(&mut out, "empty_part", empty_part(), true);
    push_str_prop(&mut out, "refusal", refusal(), true);
    push_list_prop(&mut out, "parts", parts(), true);
    push_list_prop(&mut out, "brief_parts", brief_parts(), true);
    push_list_prop(&mut out, "paths", paths(), true);
    push_str_prop(&mut out, "rounding", rounding(), true);
    push_list_prop(&mut out, "origin_labels", origin_labels(), true);
    push_str_prop(&mut out, "binary", binary(), true);

    out.push_str("  \"chips\": [\n");
    for (idx, row) in chips().iter().enumerate() {
        out.push_str("    {\"intent\": ");
        out.push_str(&escape_js(row.intent));
        out.push_str(", \"chip\": ");
        out.push_str(&escape_js(row.chip));
        out.push_str(", \"from\": ");
        out.push_str(&escape_js(row.from.as_str()));
        out.push('}');
        if idx + 1 < chips().len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n");

    push_list_prop(&mut out, "cite_statuses", cite_statuses(), true);
    push_str_prop(&mut out, "verified_text", verified_text(), true);
    push_str_prop(&mut out, "non_canonical", non_canonical(), true);
    push_str_prop(&mut out, "left_out_budget", left_out_budget(), true);
    push_str_prop(&mut out, "left_out_policy", left_out_policy(), true);
    push_str_prop(&mut out, "pack_labels", pack_labels(), true);
    push_str_prop(&mut out, "not_available", not_available(), true);
    push_str_prop(&mut out, "unreleased_build", unreleased_build(), true);
    push_str_prop(&mut out, "print_check", print_check(), true);
    push_str_prop(&mut out, "not_remembered", not_remembered(), true);
    push_str_prop(&mut out, "set_start_date", set_start_date(), true);
    push_str_prop(&mut out, "set_start_date_plan", set_start_date_plan(), true);
    push_str_prop(&mut out, "day_n_of_90", day_n_of_90(), true);
    push_str_prop(&mut out, "day_n_today", day_n_today(), true);
    push_str_prop(&mut out, "tag_at_home", tag_at_home(), true);
    push_str_prop(&mut out, "tag_r2_laptop", tag_r2_laptop(), true);

    out.push_str("  \"assistant_labels\": {\n");
    for (idx, (id, text)) in ASSISTANT_LABELS.iter().enumerate() {
        out.push_str("    ");
        out.push_str(&escape_js(id));
        out.push_str(": ");
        out.push_str(&escape_js(text));
        if idx + 1 < ASSISTANT_LABELS.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  },\n");

    push_list_prop(&mut out, "ai_note_label", ai_note_label(), false);
    out.push_str("};\n");
    out
}

/// Format the first (key) table of the Strings section in `corpus-format.md`.
#[must_use]
pub fn render_key_table() -> String {
    let mut out = String::from("| Key | String | Value |\n|---|---|---|\n");
    for row in KEY_TABLE {
        out.push('|');
        if let Some(key) = row.key {
            out.push_str(" `");
            out.push_str(key);
            out.push_str("` |");
        } else {
            out.push_str(" |");
        }
        out.push(' ');
        out.push_str(row.string);
        out.push_str(" | ");
        out.push_str(row.value);
        out.push_str(" |\n");
    }
    out
}

/// Format the verb map (`chips`) table of the Strings section in `corpus-format.md`.
#[must_use]
pub fn render_verb_table() -> String {
    let mut out = String::from("| Intent | Chip string | From |\n|---|---|---|\n");
    for row in CHIPS {
        out.push_str("| ");
        out.push_str(row.intent);
        out.push_str(" | `");
        out.push_str(row.chip);
        out.push_str("` | ");
        out.push_str(row.from.as_str());
        out.push_str(" |\n");
    }
    out
}

/// Format the `Other fixed wording` table of the Strings section in `corpus-format.md`.
#[must_use]
pub fn render_other_table() -> String {
    let mut out = String::from("| Key | Wording | Decision |\n|---|---|---|\n");
    for row in OTHER_TABLE {
        out.push_str("| `");
        out.push_str(row.key);
        out.push_str("` | ");
        out.push_str(row.wording);
        out.push_str(" | ");
        out.push_str(row.decision);
        out.push_str(" |\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        AI_NOTE_LABELS, ASSISTANT_LABELS, ReleaseFrom, abstention, ai_note_label, assistant_labels,
        binary, brief_parts, chips, cite_statuses, day_n_of_90, day_n_today, empty_part, escape_js,
        js, left_out_budget, left_out_policy, non_canonical, not_available, not_remembered,
        origin_labels, pack_labels, parts, paths, print_check, refusal, rounding, set_start_date,
        set_start_date_plan, tag_at_home, tag_r2_laptop, unreleased_build, verified_text,
    };

    const CORPUS_FORMAT: &str = include_str!("../../../docs/reference/corpus-format.md");

    fn doc_strings_keys(doc: &str) -> Vec<String> {
        let (_, after_begin) = doc
            .split_once("<!-- strings:begin -->")
            .expect("strings:begin marker exists");
        let (section, _) = after_begin
            .split_once("<!-- strings:end -->")
            .expect("strings:end marker exists");

        let mut keys = Vec::new();
        let mut table_kind = 0u8;
        for line in section.lines() {
            match line {
                "| Key | String | Value |" => {
                    table_kind = 1;
                    continue;
                }
                "| Intent | Chip string | From |" => {
                    table_kind = 2;
                    keys.push("chips".to_owned());
                    continue;
                }
                "| Key | Wording | Decision |" => {
                    table_kind = 3;
                    continue;
                }
                "|---|---|---|" => continue,
                _ if !line.starts_with('|') => {
                    table_kind = 0;
                    continue;
                }
                _ => {}
            }
            if table_kind == 1 || table_kind == 3 {
                let first_cell = line
                    .strip_prefix('|')
                    .and_then(|rest| rest.split('|').next())
                    .map(str::trim)
                    .unwrap_or("");
                if let Some(key) = first_cell
                    .strip_prefix('`')
                    .and_then(|s| s.strip_suffix('`'))
                {
                    keys.push(key.to_owned());
                }
            }
        }
        keys
    }

    #[test]
    fn release_from_prints_exact_labels() {
        assert_eq!(ReleaseFrom::Day1.to_string(), "Day 1");
        assert_eq!(ReleaseFrom::Day30.to_string(), "Day 30");
        assert_eq!(ReleaseFrom::Day90.to_string(), "Day 90");
        assert_eq!(ReleaseFrom::R2.to_string(), "R2");
    }

    #[test]
    fn accessors_return_unquoted_strings_and_tables() {
        assert_eq!(abstention(), "Not established in the library.");
        assert_eq!(empty_part(), "Not established.");
        assert_eq!(refusal(), "Do not use this answer.");
        assert_eq!(parts().len(), 8);
        assert_eq!(brief_parts().len(), 6);
        assert_eq!(paths().len(), 7);
        assert_eq!(
            rounding(),
            "Each extension rounds half away from zero to cents; totals sum the rounded extensions."
        );
        assert_eq!(origin_labels(), &["Quoted", "Computed", "Judgment"]);
        assert_eq!(binary(), "fieldbook");
        assert_eq!(chips().len(), 35);
        assert_eq!(cite_statuses().len(), 9);
        assert_eq!(verified_text(), "Citation resolves at this locator");
        assert_eq!(non_canonical(), "non-canonical");
        assert_eq!(left_out_budget(), "Left out (budget)");
        assert_eq!(left_out_policy(), "Left out (policy)");
        assert_eq!(pack_labels(), "Labels: <ids>");
        assert_eq!(
            not_available(),
            "<Thing> is not available until <release or trigger>."
        );
        assert_eq!(unreleased_build(), "unreleased build");
        assert_eq!(print_check(), "Check the current edition");
        assert_eq!(not_remembered(), "Not remembered in this browser");
        assert_eq!(set_start_date(), "Set your start date");
        assert_eq!(set_start_date_plan(), "Set a start date to mark today");
        assert_eq!(day_n_of_90(), "Day N of 90");
        assert_eq!(day_n_today(), "Day N of 90 \u{b7} the line marks today");
        assert_eq!(tag_at_home(), "At home");
        assert_eq!(tag_r2_laptop(), "R2 · laptop");
        assert_eq!(assistant_labels().len(), 8);
        assert_eq!(ASSISTANT_LABELS.len(), 8);
        assert_eq!(ai_note_label(), &AI_NOTE_LABELS);
    }

    #[test]
    fn escape_js_writes_closing_tag_slash_escaped() {
        assert_eq!(escape_js("</script>"), r#""<\/script>""#);
        assert_eq!(escape_js("a\"b\\c\nd"), r#""a\"b\\c\nd""#);
    }

    #[test]
    fn strings_js_matches_snapshot() {
        let rendered = js();
        assert!(rendered.starts_with("window.FB_STRINGS = {\n"));
        assert!(!rendered.contains("</"));
        insta::assert_snapshot!("strings_js", rendered);
    }

    #[test]
    fn strings_js_has_every_key_of_the_document() {
        let doc_keys = doc_strings_keys(CORPUS_FORMAT);
        assert_eq!(doc_keys.len(), 28);

        let rendered = js();
        let json_body = rendered
            .strip_prefix("window.FB_STRINGS = ")
            .and_then(|s| s.strip_suffix(";\n"))
            .expect("window.FB_STRINGS assignment");
        let parsed: serde_json::Value =
            serde_json::from_str(json_body).expect("valid JSON payload");
        assert!(parsed.is_object());

        let js_keys: Vec<String> = rendered
            .lines()
            .filter_map(|line| {
                let rest = line.strip_prefix("  \"")?;
                let (key, _) = rest.split_once("\": ")?;
                Some(key.to_owned())
            })
            .collect();

        assert_eq!(js_keys, doc_keys);
    }
}
