//! Check pass `register_links`: `FB0207`, `FB0208`, `FB0211` and `FB0214` across the five registers
//! and `corpus/sources/public/` (`docs/reference/corpus-format.md#02-registers-and-sources`).
//!
//! **Responsibility.**
//! - `FB0207`: every id cell, list cell and area cell across the five registers resolves in
//!   [`Index`](super::Index), and a claim's `conflict_id` lists that claim in `claim_ids`.
//! - `FB0208`: register supersession on sources, claims and glossary terms (reciprocity on claims,
//!   self-supersession and loops reported once at the first row in key order).
//! - `FB0211`: the four original-file cases under `corpus/sources/public/` (skipped with a report
//!   note only when `Tree::contains_dir("corpus/sources/public")` is `false`).
//! - `FB0214`: glossary `label` and `aliases` uniqueness, ignoring case, reported at the second use
//!   and naming the first term.

use std::collections::{BTreeMap, BTreeSet};

use crate::hash::Sha256Stream;
use crate::ids::{AreaCode, ClaimId, ConflictId, QuestionId, SourceId, TermId, VersionId};
use crate::schema::Register;
use crate::schema::rows::RefId;
use crate::schema::table::Row;

use super::registers::register_path;
use super::{Code, Diagnostic, Entry, Model, Options, Report};

const PUBLIC_ORIGINALS_DIR: &str = "corpus/sources/public";
const PUBLIC_ORIGINALS_PREFIX: &str = "corpus/sources/public/";

/// Run the `register_links` check pass (`FB0207`, `FB0208`, `FB0211`, `FB0214`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    let strict = options.strict;
    let known = KnownKeys::from_model(model);

    check_sources_links_and_supersession(model, &known, strict, report);
    check_claims_links_and_supersession(model, &known, strict, report);
    check_questions_links(model, &known, strict, report);
    check_conflicts_links(model, &known, strict, report);
    check_glossary_links_supersession_and_labels(model, &known, strict, report);
    check_originals(model, &known, strict, report);
}

/// Known keys across all five registers and `corpus.toml`.
///
/// Built from [`Model::index`] plus any raw row whose key is valid and non-duplicate (so a cell
/// error in a non-key column of a referenced row does not trigger a cascading `FB0207` on rows
/// that point to it).
struct KnownKeys {
    sources_loaded: bool,
    claims_loaded: bool,
    questions_loaded: bool,
    conflicts_loaded: bool,
    source_versions: BTreeSet<VersionId>,
    source_ids: BTreeSet<SourceId>,
    claims: BTreeSet<ClaimId>,
    questions: BTreeSet<QuestionId>,
    conflicts: BTreeSet<ConflictId>,
    conflict_claims: BTreeMap<ConflictId, BTreeSet<ClaimId>>,
    terms: BTreeSet<TermId>,
    areas: BTreeSet<AreaCode>,
}

impl KnownKeys {
    fn from_model(model: &Model) -> Self {
        let idx = model.index();
        let mut source_versions: BTreeSet<VersionId> =
            idx.source_versions.keys().cloned().collect();
        for row in model.register_rows(Register::Sources) {
            if let (Ok(sid), Ok(ver)) = (
                SourceId::parse(cell_str(Register::Sources, row, "source_id")),
                cell_str(Register::Sources, row, "version").parse::<u32>(),
            ) && let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}"))
            {
                source_versions.insert(vid);
            }
        }
        let source_ids: BTreeSet<SourceId> =
            source_versions.iter().map(VersionId::source).collect();

        let mut claims: BTreeSet<ClaimId> = idx.claims.keys().cloned().collect();
        for row in model.register_rows(Register::Claims) {
            if let Ok(cid) = ClaimId::parse(cell_str(Register::Claims, row, "claim_id")) {
                claims.insert(cid);
            }
        }

        let mut questions: BTreeSet<QuestionId> = idx.questions.keys().cloned().collect();
        for row in model.register_rows(Register::Questions) {
            if let Ok(qid) = QuestionId::parse(cell_str(Register::Questions, row, "question_id")) {
                questions.insert(qid);
            }
        }

        let mut conflicts: BTreeSet<ConflictId> = idx.conflicts.keys().cloned().collect();
        let mut conflict_claims: BTreeMap<ConflictId, BTreeSet<ClaimId>> = BTreeMap::new();
        for located in model.conflicts() {
            conflict_claims.insert(
                located.value.conflict_id.clone(),
                located.value.claim_ids.iter().cloned().collect(),
            );
        }
        for row in model.register_rows(Register::Conflicts) {
            if let Ok(xcid) = ConflictId::parse(cell_str(Register::Conflicts, row, "conflict_id")) {
                conflicts.insert(xcid.clone());
                conflict_claims.entry(xcid).or_insert_with(|| {
                    cell_str(Register::Conflicts, row, "claim_ids")
                        .split('|')
                        .filter_map(|s| ClaimId::parse(s).ok())
                        .collect()
                });
            }
        }

        let mut terms: BTreeSet<TermId> = idx.terms.keys().cloned().collect();
        for row in model.register_rows(Register::Glossary) {
            if let Ok(tid) = TermId::parse(cell_str(Register::Glossary, row, "term_id")) {
                terms.insert(tid);
            }
        }

        Self {
            sources_loaded: model.register(Register::Sources).is_some(),
            claims_loaded: model.register(Register::Claims).is_some(),
            questions_loaded: model.register(Register::Questions).is_some(),
            conflicts_loaded: model.register(Register::Conflicts).is_some(),
            source_versions,
            source_ids,
            claims,
            questions,
            conflicts,
            conflict_claims,
            terms,
            areas: idx.areas.clone(),
        }
    }

    fn contains_ref(&self, r: &RefId) -> bool {
        match r {
            RefId::Claim(c) => !self.claims_loaded || self.claims.contains(c),
            RefId::SourceVersion(v) => !self.sources_loaded || self.source_versions.contains(v),
        }
    }
}

fn col_index(reg: Register, col_name: &str) -> usize {
    reg.columns()
        .iter()
        .position(|c| c.name == col_name)
        .unwrap_or(0)
}

fn col_1based(reg: Register, row: &Row, col_name: &str) -> usize {
    let idx = col_index(reg, col_name);
    row.cols
        .get(idx)
        .copied()
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(1)
}

fn cell_str<'a>(reg: Register, row: &'a Row, col_name: &str) -> &'a str {
    let idx = col_index(reg, col_name);
    row.cells.get(idx).map_or("", String::as_str)
}

fn push_cell_diag(
    reg: Register,
    row: &Row,
    col_name: &str,
    code: Code,
    strict: bool,
    message: String,
    report: &mut Report,
) {
    let line = usize::try_from(row.line).unwrap_or(1);
    let col = col_1based(reg, row, col_name);
    report.push(Diagnostic::new(
        code,
        strict,
        register_path(reg),
        line,
        col,
        message,
    ));
}

fn has_cell_problem(model: &Model, reg: Register, line: u32, col_name: &str) -> bool {
    model
        .register_problems(reg)
        .iter()
        .find(|(l, _)| *l == line)
        .is_some_and(|(_, probs)| probs.iter().any(|p| p.column == col_name))
}

fn check_sources_links_and_supersession(
    model: &Model,
    known: &KnownKeys,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Sources;
    let rows = model.register_rows(reg);
    if rows.is_empty() {
        return;
    }

    let mut graph: BTreeMap<VersionId, Vec<VersionId>> = BTreeMap::new();
    let mut row_by_key: BTreeMap<VersionId, &Row> = BTreeMap::new();

    for row in rows {
        let Ok(sid) = SourceId::parse(cell_str(reg, row, "source_id")) else {
            continue;
        };
        let Ok(ver) = cell_str(reg, row, "version").parse::<u32>() else {
            continue;
        };
        let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}")) else {
            continue;
        };
        row_by_key.entry(vid.clone()).or_insert(row);

        if has_cell_problem(model, reg, row.line, "supersedes") {
            continue;
        }
        let raw_sup = cell_str(reg, row, "supersedes");
        if raw_sup.is_empty() {
            continue;
        }

        let mut edges = Vec::new();
        for item in raw_sup.split('|') {
            let Ok(target) = VersionId::parse(item) else {
                continue;
            };
            if target == vid {
                push_cell_diag(
                    reg,
                    row,
                    "supersedes",
                    Code::Fb0208,
                    strict,
                    format!("source version `{vid}` cannot supersede itself"),
                    report,
                );
            } else if !known.source_versions.contains(&target) {
                push_cell_diag(
                    reg,
                    row,
                    "supersedes",
                    Code::Fb0207,
                    strict,
                    format!("`supersedes` references unknown source version `{target}`"),
                    report,
                );
            } else {
                edges.push(target);
            }
        }
        if !edges.is_empty() {
            graph.entry(vid).or_default().extend(edges);
        }
    }

    for cycle in find_cycles(&graph) {
        if let Some(first) = cycle.iter().next()
            && let Some(row) = row_by_key.get(first)
        {
            let names: Vec<String> = cycle.iter().map(ToString::to_string).collect();
            push_cell_diag(
                reg,
                row,
                "supersedes",
                Code::Fb0208,
                strict,
                format!("source version supersession loop: {}", names.join(" -> ")),
                report,
            );
        }
    }
}

fn check_claims_links_and_supersession(
    model: &Model,
    known: &KnownKeys,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Claims;
    let rows = model.register_rows(reg);
    if rows.is_empty() {
        return;
    }

    let mut row_by_key: BTreeMap<ClaimId, &Row> = BTreeMap::new();
    let mut supersedes_map: BTreeMap<ClaimId, Vec<ClaimId>> = BTreeMap::new();
    let mut superseded_by_map: BTreeMap<ClaimId, Vec<ClaimId>> = BTreeMap::new();

    for row in rows {
        let Ok(cid) = ClaimId::parse(cell_str(reg, row, "claim_id")) else {
            continue;
        };
        if row_by_key.contains_key(&cid) {
            continue;
        }
        row_by_key.insert(cid.clone(), row);

        // `area` -> FB0207
        if !has_cell_problem(model, reg, row.line, "area")
            && let Ok(area) = AreaCode::parse(cell_str(reg, row, "area"))
            && !known.areas.contains(&area)
        {
            push_cell_diag(
                reg,
                row,
                "area",
                Code::Fb0207,
                strict,
                format!("`area` references unknown area `{area}`"),
                report,
            );
        }

        // `source_id` + `version` -> FB0207
        let locator_kind = cell_str(reg, row, "locator_kind");
        if known.sources_loaded
            && locator_kind != "record_field"
            && !has_cell_problem(model, reg, row.line, "source_id")
            && !has_cell_problem(model, reg, row.line, "version")
        {
            let sid_str = cell_str(reg, row, "source_id");
            let ver_str = cell_str(reg, row, "version");
            if !sid_str.is_empty()
                && !ver_str.is_empty()
                && let (Ok(sid), Ok(ver)) = (SourceId::parse(sid_str), ver_str.parse::<u32>())
                && let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}"))
                && !known.source_versions.contains(&vid)
            {
                let target_col = if known.source_ids.contains(&sid) {
                    "version"
                } else {
                    "source_id"
                };
                push_cell_diag(
                    reg,
                    row,
                    target_col,
                    Code::Fb0207,
                    strict,
                    format!("claim `{cid}` references unknown source version `{vid}`"),
                    report,
                );
            }
        }

        // `supersedes` -> FB0207 / self FB0208
        if !has_cell_problem(model, reg, row.line, "supersedes") {
            let raw = cell_str(reg, row, "supersedes");
            if !raw.is_empty() {
                let mut valid = Vec::new();
                for item in raw.split('|') {
                    let Ok(target) = ClaimId::parse(item) else {
                        continue;
                    };
                    if target == cid {
                        push_cell_diag(
                            reg,
                            row,
                            "supersedes",
                            Code::Fb0208,
                            strict,
                            format!("claim `{cid}` cannot supersede itself"),
                            report,
                        );
                    } else if !known.claims.contains(&target) {
                        push_cell_diag(
                            reg,
                            row,
                            "supersedes",
                            Code::Fb0207,
                            strict,
                            format!("`supersedes` references unknown claim `{target}`"),
                            report,
                        );
                    } else {
                        valid.push(target);
                    }
                }
                supersedes_map.insert(cid.clone(), valid);
            }
        }

        // `superseded_by` -> FB0207 / self FB0208
        if !has_cell_problem(model, reg, row.line, "superseded_by") {
            let raw = cell_str(reg, row, "superseded_by");
            if !raw.is_empty() {
                let mut valid = Vec::new();
                for item in raw.split('|') {
                    let Ok(target) = ClaimId::parse(item) else {
                        continue;
                    };
                    if target == cid {
                        push_cell_diag(
                            reg,
                            row,
                            "superseded_by",
                            Code::Fb0208,
                            strict,
                            format!("claim `{cid}` cannot be superseded by itself"),
                            report,
                        );
                    } else if !known.claims.contains(&target) {
                        push_cell_diag(
                            reg,
                            row,
                            "superseded_by",
                            Code::Fb0207,
                            strict,
                            format!("`superseded_by` references unknown claim `{target}`"),
                            report,
                        );
                    } else {
                        valid.push(target);
                    }
                }
                superseded_by_map.insert(cid.clone(), valid);
            }
        }

        // `conflict_id` -> FB0207 (existence + backlink in conflict's `claim_ids`)
        if known.conflicts_loaded && !has_cell_problem(model, reg, row.line, "conflict_id") {
            let raw = cell_str(reg, row, "conflict_id");
            if !raw.is_empty()
                && let Ok(xcid) = ConflictId::parse(raw)
            {
                if !known.conflicts.contains(&xcid) {
                    push_cell_diag(
                        reg,
                        row,
                        "conflict_id",
                        Code::Fb0207,
                        strict,
                        format!("`conflict_id` references unknown conflict `{xcid}`"),
                        report,
                    );
                } else if let Some(member_claims) = known.conflict_claims.get(&xcid)
                    && !member_claims.contains(&cid)
                {
                    push_cell_diag(
                        reg,
                        row,
                        "conflict_id",
                        Code::Fb0207,
                        strict,
                        format!(
                            "claim `{cid}` names conflict `{xcid}`, which does not list `{cid}` in `claim_ids`"
                        ),
                        report,
                    );
                }
            }
        }

        // `question_id` -> FB0207
        if known.questions_loaded && !has_cell_problem(model, reg, row.line, "question_id") {
            let raw = cell_str(reg, row, "question_id");
            if !raw.is_empty()
                && let Ok(qid) = QuestionId::parse(raw)
                && !known.questions.contains(&qid)
            {
                push_cell_diag(
                    reg,
                    row,
                    "question_id",
                    Code::Fb0207,
                    strict,
                    format!("`question_id` references unknown question `{qid}`"),
                    report,
                );
            }
        }
    }

    // Reciprocity and loop graph for claims (`FB0208`).
    // Directed edges run in replacement order (`old -> new`).
    let mut graph: BTreeMap<ClaimId, Vec<ClaimId>> = BTreeMap::new();

    for (cid, targets) in &supersedes_map {
        let Some(row) = row_by_key.get(cid) else {
            continue;
        };
        for old_id in targets {
            let reciprocated = superseded_by_map
                .get(old_id)
                .is_some_and(|list| list.contains(cid));
            if !reciprocated {
                push_cell_diag(
                    reg,
                    row,
                    "supersedes",
                    Code::Fb0208,
                    strict,
                    format!(
                        "claim `{cid}` lists `{old_id}` in `supersedes`, but `{old_id}` does not list `{cid}` in `superseded_by`"
                    ),
                    report,
                );
            }
            graph.entry(old_id.clone()).or_default().push(cid.clone());
        }
    }

    for (cid, targets) in &superseded_by_map {
        let Some(row) = row_by_key.get(cid) else {
            continue;
        };
        for new_id in targets {
            let reciprocated = supersedes_map
                .get(new_id)
                .is_some_and(|list| list.contains(cid));
            if !reciprocated {
                push_cell_diag(
                    reg,
                    row,
                    "superseded_by",
                    Code::Fb0208,
                    strict,
                    format!(
                        "claim `{cid}` lists `{new_id}` in `superseded_by`, but `{new_id}` does not list `{cid}` in `supersedes`"
                    ),
                    report,
                );
            }
            graph.entry(cid.clone()).or_default().push(new_id.clone());
        }
    }

    for edges in graph.values_mut() {
        edges.sort();
        edges.dedup();
    }

    for cycle in find_cycles(&graph) {
        if let Some(first) = cycle.iter().next()
            && let Some(row) = row_by_key.get(first)
        {
            let col_name = if !cell_str(reg, row, "superseded_by").is_empty() {
                "superseded_by"
            } else {
                "supersedes"
            };
            let names: Vec<String> = cycle.iter().map(ToString::to_string).collect();
            push_cell_diag(
                reg,
                row,
                col_name,
                Code::Fb0208,
                strict,
                format!("claim supersession loop: {}", names.join(" -> ")),
                report,
            );
        }
    }
}

fn check_questions_links(model: &Model, known: &KnownKeys, strict: bool, report: &mut Report) {
    let reg = Register::Questions;
    for row in model.register_rows(reg) {
        if !has_cell_problem(model, reg, row.line, "area")
            && let Ok(area) = AreaCode::parse(cell_str(reg, row, "area"))
            && !known.areas.contains(&area)
        {
            push_cell_diag(
                reg,
                row,
                "area",
                Code::Fb0207,
                strict,
                format!("`area` references unknown area `{area}`"),
                report,
            );
        }
    }
}

fn check_conflicts_links(model: &Model, known: &KnownKeys, strict: bool, report: &mut Report) {
    let reg = Register::Conflicts;
    for row in model.register_rows(reg) {
        if !has_cell_problem(model, reg, row.line, "area")
            && let Ok(area) = AreaCode::parse(cell_str(reg, row, "area"))
            && !known.areas.contains(&area)
        {
            push_cell_diag(
                reg,
                row,
                "area",
                Code::Fb0207,
                strict,
                format!("`area` references unknown area `{area}`"),
                report,
            );
        }

        if !has_cell_problem(model, reg, row.line, "evidence") {
            let raw = cell_str(reg, row, "evidence");
            if !raw.is_empty() {
                for item in raw.split('|') {
                    if let Ok(r) = RefId::parse(item)
                        && !known.contains_ref(&r)
                    {
                        push_cell_diag(
                            reg,
                            row,
                            "evidence",
                            Code::Fb0207,
                            strict,
                            format!("`evidence` references unknown id `{r}`"),
                            report,
                        );
                    }
                }
            }
        }

        if known.claims_loaded && !has_cell_problem(model, reg, row.line, "claim_ids") {
            let raw = cell_str(reg, row, "claim_ids");
            if !raw.is_empty() {
                for item in raw.split('|') {
                    if let Ok(cid) = ClaimId::parse(item)
                        && !known.claims.contains(&cid)
                    {
                        push_cell_diag(
                            reg,
                            row,
                            "claim_ids",
                            Code::Fb0207,
                            strict,
                            format!("`claim_ids` references unknown claim `{cid}`"),
                            report,
                        );
                    }
                }
            }
        }

        if known.questions_loaded && !has_cell_problem(model, reg, row.line, "question_id") {
            let raw = cell_str(reg, row, "question_id");
            if !raw.is_empty()
                && let Ok(qid) = QuestionId::parse(raw)
                && !known.questions.contains(&qid)
            {
                push_cell_diag(
                    reg,
                    row,
                    "question_id",
                    Code::Fb0207,
                    strict,
                    format!("`question_id` references unknown question `{qid}`"),
                    report,
                );
            }
        }
    }
}

fn check_glossary_links_supersession_and_labels(
    model: &Model,
    known: &KnownKeys,
    strict: bool,
    report: &mut Report,
) {
    let reg = Register::Glossary;
    let rows = model.register_rows(reg);
    if rows.is_empty() {
        return;
    }

    let mut row_by_key: BTreeMap<TermId, &Row> = BTreeMap::new();
    let mut graph: BTreeMap<TermId, Vec<TermId>> = BTreeMap::new();
    let mut seen_labels: BTreeMap<String, TermId> = BTreeMap::new();

    for row in rows {
        let Ok(tid) = TermId::parse(cell_str(reg, row, "term_id")) else {
            continue;
        };
        if row_by_key.contains_key(&tid) {
            continue;
        }
        row_by_key.insert(tid.clone(), row);

        if !has_cell_problem(model, reg, row.line, "owner_area")
            && let Ok(area) = AreaCode::parse(cell_str(reg, row, "owner_area"))
            && !known.areas.contains(&area)
        {
            push_cell_diag(
                reg,
                row,
                "owner_area",
                Code::Fb0207,
                strict,
                format!("`owner_area` references unknown area `{area}`"),
                report,
            );
        }

        if !has_cell_problem(model, reg, row.line, "source_ref") {
            let raw = cell_str(reg, row, "source_ref");
            if !raw.is_empty() {
                for item in raw.split('|') {
                    if let Ok(r) = RefId::parse(item)
                        && !known.contains_ref(&r)
                    {
                        push_cell_diag(
                            reg,
                            row,
                            "source_ref",
                            Code::Fb0207,
                            strict,
                            format!("`source_ref` references unknown id `{r}`"),
                            report,
                        );
                    }
                }
            }
        }

        if !has_cell_problem(model, reg, row.line, "superseded_by") {
            let raw = cell_str(reg, row, "superseded_by");
            if !raw.is_empty()
                && let Ok(target) = TermId::parse(raw)
            {
                if target == tid {
                    push_cell_diag(
                        reg,
                        row,
                        "superseded_by",
                        Code::Fb0208,
                        strict,
                        format!("term `{tid}` cannot be superseded by itself"),
                        report,
                    );
                } else if !known.terms.contains(&target) {
                    push_cell_diag(
                        reg,
                        row,
                        "superseded_by",
                        Code::Fb0207,
                        strict,
                        format!("`superseded_by` references unknown term `{target}`"),
                        report,
                    );
                } else {
                    graph.entry(tid.clone()).or_default().push(target);
                }
            }
        }

        // FB0214: label and aliases uniqueness (case-insensitive)
        if !has_cell_problem(model, reg, row.line, "label") {
            let label = cell_str(reg, row, "label");
            if !label.is_empty() && !label.contains('|') {
                let lower = label.to_lowercase();
                if let Some(first_tid) = seen_labels.get(&lower) {
                    push_cell_diag(
                        reg,
                        row,
                        "label",
                        Code::Fb0214,
                        strict,
                        format!(
                            "glossary label `{label}` duplicates term `{first_tid}` (ignoring case)"
                        ),
                        report,
                    );
                } else {
                    seen_labels.insert(lower, tid.clone());
                }
            }
        }

        if !has_cell_problem(model, reg, row.line, "aliases") {
            let raw_aliases = cell_str(reg, row, "aliases");
            if !raw_aliases.is_empty() {
                for alias in raw_aliases.split('|') {
                    if alias.is_empty() {
                        continue;
                    }
                    let lower = alias.to_lowercase();
                    if let Some(first_tid) = seen_labels.get(&lower) {
                        push_cell_diag(
                            reg,
                            row,
                            "aliases",
                            Code::Fb0214,
                            strict,
                            format!(
                                "glossary alias `{alias}` duplicates term `{first_tid}` (ignoring case)"
                            ),
                            report,
                        );
                    } else {
                        seen_labels.insert(lower, tid.clone());
                    }
                }
            }
        }
    }

    for cycle in find_cycles(&graph) {
        if let Some(first) = cycle.iter().next()
            && let Some(row) = row_by_key.get(first)
        {
            let names: Vec<String> = cycle.iter().map(ToString::to_string).collect();
            push_cell_diag(
                reg,
                row,
                "superseded_by",
                Code::Fb0208,
                strict,
                format!("term supersession loop: {}", names.join(" -> ")),
                report,
            );
        }
    }
}

fn check_originals(model: &Model, known: &KnownKeys, strict: bool, report: &mut Report) {
    if !model.tree().contains_dir(PUBLIC_ORIGINALS_DIR) {
        report
            .push_note("corpus/sources/public/ is absent; skipping original-file checks (FB0211).");
        return;
    }
    if !known.sources_loaded {
        return;
    }

    let mut files_by_vid: BTreeMap<VersionId, Vec<(&str, &Entry)>> = BTreeMap::new();
    for (path, entry) in model.tree().iter() {
        let Some(file_name) = path.strip_prefix(PUBLIC_ORIGINALS_PREFIX) else {
            continue;
        };
        if file_name.contains('/') {
            continue;
        }
        let Some((stem, ext)) = file_name.split_once('.') else {
            report.push(Diagnostic::new(
                Code::Fb0211,
                strict,
                path,
                1,
                1,
                format!("original file `{file_name}` has no source version row"),
            ));
            continue;
        };
        if ext.is_empty() {
            report.push(Diagnostic::new(
                Code::Fb0211,
                strict,
                path,
                1,
                1,
                format!("original file `{file_name}` has no source version row"),
            ));
            continue;
        }
        match VersionId::parse(stem) {
            Ok(vid) => {
                files_by_vid.entry(vid).or_default().push((path, entry));
            }
            Err(_) => {
                report.push(Diagnostic::new(
                    Code::Fb0211,
                    strict,
                    path,
                    1,
                    1,
                    format!("original file `{file_name}` has no source version row"),
                ));
            }
        }
    }

    // Check every file whose VersionId has no row in `sources.csv`.
    for (vid, entries) in &files_by_vid {
        if !known.source_versions.contains(vid) {
            for (path, _) in entries {
                report.push(Diagnostic::new(
                    Code::Fb0211,
                    strict,
                    *path,
                    1,
                    1,
                    format!("original file for `{vid}` has no row in `sources.csv`"),
                ));
            }
        }
    }

    // Check every row in `sources.csv`.
    let reg = Register::Sources;
    let mut seen_vids = BTreeSet::new();
    for row in model.register_rows(reg) {
        let Ok(sid) = SourceId::parse(cell_str(reg, row, "source_id")) else {
            continue;
        };
        let Ok(ver) = cell_str(reg, row, "version").parse::<u32>() else {
            continue;
        };
        let Ok(vid) = VersionId::parse(&format!("{sid}-v{ver}")) else {
            continue;
        };
        if !seen_vids.insert(vid.clone()) {
            continue;
        }
        if has_cell_problem(model, reg, row.line, "location")
            || has_cell_problem(model, reg, row.line, "sha256")
        {
            continue;
        }

        let location = cell_str(reg, row, "location");
        let expected_sha = cell_str(reg, row, "sha256");
        let is_metadata_only = location.starts_with("metadata-only: ");

        match files_by_vid.get(&vid) {
            Some(entries) if !entries.is_empty() => {
                if is_metadata_only {
                    push_cell_diag(
                        reg,
                        row,
                        "sha256",
                        Code::Fb0211,
                        strict,
                        format!(
                            "metadata-only source version `{vid}` must not have a stored original file"
                        ),
                        report,
                    );
                } else {
                    for (_, entry) in entries {
                        let actual_sha = entry_sha256(entry);
                        if actual_sha != expected_sha {
                            push_cell_diag(
                                reg,
                                row,
                                "sha256",
                                Code::Fb0211,
                                strict,
                                format!(
                                    "stored original for `{vid}` hashes to `{actual_sha}`, expected `{expected_sha}`"
                                ),
                                report,
                            );
                        }
                    }
                }
            }
            _ => {
                if !is_metadata_only {
                    push_cell_diag(
                        reg,
                        row,
                        "sha256",
                        Code::Fb0211,
                        strict,
                        format!(
                            "source version `{vid}` has no stored original under `corpus/sources/public/`"
                        ),
                        report,
                    );
                }
            }
        }
    }
}

fn entry_sha256(entry: &Entry) -> String {
    match entry {
        Entry::Original { sha256, .. } => sha256.clone(),
        Entry::Bytes(bytes) => {
            let mut hasher = Sha256Stream::new();
            hasher.update(bytes);
            hasher.finish()
        }
    }
}

/// Find distinct elementary cycles in `graph`, returning each cycle's node set in `BTreeSet` key order.
fn find_cycles<K: Ord + Clone>(graph: &BTreeMap<K, Vec<K>>) -> Vec<BTreeSet<K>> {
    let mut cycles: Vec<BTreeSet<K>> = Vec::new();
    let mut seen_sets: BTreeSet<Vec<K>> = BTreeSet::new();

    for start in graph.keys() {
        let mut path = vec![start.clone()];
        let mut on_path = BTreeSet::new();
        on_path.insert(start.clone());
        dfs_cycles(
            graph,
            start,
            start,
            &mut path,
            &mut on_path,
            &mut seen_sets,
            &mut cycles,
        );
    }

    cycles
}

fn dfs_cycles<K: Ord + Clone>(
    graph: &BTreeMap<K, Vec<K>>,
    start: &K,
    current: &K,
    path: &mut Vec<K>,
    on_path: &mut BTreeSet<K>,
    seen_sets: &mut BTreeSet<Vec<K>>,
    cycles: &mut Vec<BTreeSet<K>>,
) {
    let Some(neighbors) = graph.get(current) else {
        return;
    };
    for next in neighbors {
        if next < start {
            // Only explore nodes >= `start` so each elementary cycle is found from its minimum key.
            continue;
        }
        if next == start {
            if path.len() >= 2 {
                let set: BTreeSet<K> = path.iter().cloned().collect();
                let key: Vec<K> = set.iter().cloned().collect();
                if seen_sets.insert(key) {
                    cycles.push(set);
                }
            }
        } else if !on_path.contains(next) {
            on_path.insert(next.clone());
            path.push(next.clone());
            dfs_cycles(graph, start, next, path, on_path, seen_sets, cycles);
            path.pop();
            on_path.remove(next);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Host, Options, Tree, run};
    use crate::date::CivilDate;
    use crate::hash::sha256_hex;

    const MINI_CORPUS_TOML: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    const MINI_ASSISTANT_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");
    const MINI_CHANGES_MD: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/changes.md");
    const MINI_SOURCES_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/sources.csv");
    const MINI_CLAIMS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/claims.csv");
    const MINI_QUESTIONS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/questions.csv");
    const MINI_CONFLICTS_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/conflicts.csv");
    const MINI_GLOSSARY_CSV: &str =
        include_str!("../../../../tests/fixtures/corpus-mini/corpus/registers/glossary.csv");
    const MINI_S0002_V2_TXT: &[u8] = include_bytes!(
        "../../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0002-v2.txt"
    );

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree_with_originals() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );
        tree.insert_bytes("corpus/sources/public/S-0002-v2.txt", MINI_S0002_V2_TXT);
        tree
    }

    #[test]
    fn fixture_is_clean() {
        let tree = mini_tree_with_originals();
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert!(
            report.diagnostics.is_empty(),
            "expected fixture to be clean, got: {:?}",
            report.diagnostics
        );
        assert!(
            report.notes.is_empty(),
            "unexpected notes: {:?}",
            report.notes
        );
    }

    #[test]
    fn absent_originals_directory_has_skip_note() {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );

        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        assert!(
            report.diagnostics.is_empty(),
            "expected no diagnostics when corpus/sources/public is absent, got: {:?}",
            report.diagnostics
        );
        assert!(
            report
                .notes
                .iter()
                .any(|n| n.contains("corpus/sources/public/") && n.contains("skip")),
            "expected skip note when corpus/sources/public is absent, got: {:?}",
            report.notes
        );
    }

    #[test]
    fn empty_originals_directory_reports_missing_file() {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", MINI_ASSISTANT_MD.as_bytes());
        tree.insert_bytes("corpus/changes.md", MINI_CHANGES_MD.as_bytes());
        tree.insert_bytes("corpus/registers/sources.csv", MINI_SOURCES_CSV.as_bytes());
        tree.insert_bytes("corpus/registers/claims.csv", MINI_CLAIMS_CSV.as_bytes());
        tree.insert_bytes(
            "corpus/registers/questions.csv",
            MINI_QUESTIONS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/conflicts.csv",
            MINI_CONFLICTS_CSV.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/glossary.csv",
            MINI_GLOSSARY_CSV.as_bytes(),
        );

        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let fb0211: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0211)
            .collect();
        assert_eq!(
            fb0211.len(),
            1,
            "expected FB0211 for missing S-0002-v2.txt: {:?}",
            report.diagnostics
        );
        assert_eq!(fb0211[0].path, "corpus/registers/sources.csv");
        assert_eq!(fb0211[0].line, 4);
        assert!(fb0211[0].message.contains("S-0002-v2"));
    }

    #[test]
    fn originals_four_cases_raise_fb0211() {
        // Case 2: file in sources/public/ with no row in sources.csv
        let mut tree = mini_tree_with_originals();
        tree.insert_bytes(
            "corpus/sources/public/S-0099-v1.txt",
            b"orphan synthetic text\n",
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let fb0211: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0211)
            .collect();
        assert_eq!(fb0211.len(), 1, "diagnostics: {:?}", report.diagnostics);
        assert_eq!(fb0211[0].path, "corpus/sources/public/S-0099-v1.txt");
        assert!(fb0211[0].message.contains("S-0099-v1"));

        // Case 3: hash mismatch (both Bytes and Original entry forms)
        let mut tree_bytes = mini_tree_with_originals();
        tree_bytes.insert_bytes("corpus/sources/public/S-0002-v2.txt", b"changed bytes\n");
        let report_bytes = run(&Host::new(tree_bytes, sample_today()), &Options::new(false));
        let fb0211_b: Vec<_> = report_bytes
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0211)
            .collect();
        assert_eq!(fb0211_b.len(), 1);
        assert_eq!(fb0211_b[0].path, "corpus/registers/sources.csv");
        assert_eq!(fb0211_b[0].line, 4);
        assert!(fb0211_b[0].message.contains("S-0002-v2"));

        let mut tree_orig = mini_tree_with_originals();
        tree_orig.insert_original(
            "corpus/sources/public/S-0002-v2.txt",
            sha256_hex(b"other bytes\n"),
            12,
        );
        let report_orig = run(&Host::new(tree_orig, sample_today()), &Options::new(false));
        assert!(
            report_orig
                .diagnostics
                .iter()
                .any(|d| d.code == Code::Fb0211 && d.message.contains("S-0002-v2"))
        );

        // Case 4: metadata-only row has a file
        let mut tree_meta = mini_tree_with_originals();
        tree_meta.insert_bytes("corpus/sources/public/S-0001-v1.txt", b"unexpected file\n");
        let report_meta = run(&Host::new(tree_meta, sample_today()), &Options::new(false));
        let fb0211_m: Vec<_> = report_meta
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0211)
            .collect();
        assert_eq!(fb0211_m.len(), 1);
        assert_eq!(fb0211_m[0].path, "corpus/registers/sources.csv");
        assert_eq!(fb0211_m[0].line, 2);
        assert!(fb0211_m[0].message.contains("S-0001-v1"));
    }

    #[test]
    fn fb0207_broken_references_across_registers() {
        let mut tree = mini_tree_with_originals();
        let claims = MINI_CLAIMS_CSV.replace(
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-0001,1,",
            "CL-CMP-0002,cmp,fact,Synthetic withholding threshold is 5 percent of billed amounts.,2026-02-01,S-9999,1,",
        );
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let fb0207: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0207)
            .collect();
        assert_eq!(fb0207.len(), 1, "diagnostics: {:?}", report.diagnostics);
        assert_eq!(fb0207[0].path, "corpus/registers/claims.csv");
        assert_eq!(fb0207[0].line, 3);
        assert!(fb0207[0].message.contains("S-9999-v1"));
    }

    #[test]
    fn fb0208_reciprocity_and_three_claim_loop_reported_once_in_key_order() {
        // Non-reciprocal claim supersession
        let mut tree = mini_tree_with_originals();
        let claims = MINI_CLAIMS_CSV.replace(",CL-EST-0002,,,,", ",,,,,");
        tree.insert_bytes("corpus/registers/claims.csv", claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let fb0208: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0208)
            .collect();
        assert_eq!(fb0208.len(), 1, "diagnostics: {:?}", report.diagnostics);
        assert_eq!(fb0208[0].path, "corpus/registers/claims.csv");
        assert_eq!(fb0208[0].line, 5);

        // Loop of three claims: CL-EST-0004 -> CL-EST-0005 -> CL-EST-0006 -> CL-EST-0004
        let mut tree_loop = mini_tree_with_originals();
        let loop_claims = format!(
            "{MINI_CLAIMS_CSV}\
CL-EST-0004,est,fact,Synthetic loop claim four.,2026-02-01,S-0002,2,text_section,sec. 4,Synthetic quote four.,true,codex,unchecked,not_applicable,superseded,observed,V2,,,,,,,,,,,,,,CL-EST-0006,CL-EST-0005,,,Loop 4.\n\
CL-EST-0005,est,fact,Synthetic loop claim five.,2026-02-01,S-0002,2,text_section,sec. 4,Synthetic quote five.,true,codex,unchecked,not_applicable,superseded,observed,V2,,,,,,,,,,,,,,CL-EST-0004,CL-EST-0006,,,Loop 5.\n\
CL-EST-0006,est,fact,Synthetic loop claim six.,2026-02-01,S-0002,2,text_section,sec. 4,Synthetic quote six.,true,codex,unchecked,not_applicable,superseded,observed,V2,,,,,,,,,,,,,,CL-EST-0005,CL-EST-0004,,,Loop 6.\n"
        );
        tree_loop.insert_bytes("corpus/registers/claims.csv", loop_claims.as_bytes());
        let report_loop = run(&Host::new(tree_loop, sample_today()), &Options::new(false));
        let fb0208_loop: Vec<_> = report_loop
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0208)
            .collect();
        assert_eq!(
            fb0208_loop.len(),
            1,
            "expected loop to be reported once: {:?}",
            report_loop.diagnostics
        );
        assert_eq!(fb0208_loop[0].path, "corpus/registers/claims.csv");
        assert_eq!(fb0208_loop[0].line, 7); // CL-EST-0004 is line 7
        assert!(
            fb0208_loop[0]
                .message
                .contains("CL-EST-0004 -> CL-EST-0005 -> CL-EST-0006"),
            "unexpected loop message: {}",
            fb0208_loop[0].message
        );
    }

    #[test]
    fn fb0214_repeated_glossary_label_or_alias_ignoring_case() {
        let mut tree = mini_tree_with_originals();
        let glossary = MINI_GLOSSARY_CSV.replace(
            "rate_window,rate window,Synthetic period allowed to complete a forward pricing rate check.,Distinct from the synthetic billing cycle review interval.,est,CL-EST-0003,public_reference,,",
            "rate_window,rate window,Synthetic period allowed to complete a forward pricing rate check.,Distinct from the synthetic billing cycle review interval.,est,CL-EST-0003,public_reference,Withholding Holdback,",
        );
        tree.insert_bytes("corpus/registers/glossary.csv", glossary.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(false));
        let fb0214: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0214)
            .collect();
        assert_eq!(fb0214.len(), 1, "diagnostics: {:?}", report.diagnostics);
        assert_eq!(fb0214[0].path, "corpus/registers/glossary.csv");
        assert_eq!(fb0214[0].line, 4);
        assert!(fb0214[0].message.contains("billing_withhold"));
    }
}
