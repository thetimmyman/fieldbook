//! Classification of files copied into a backup snapshot and provenance label resolution.
//!
//! Implements file classification into [`BackupFileKind`] with effective [`LabelSet`]
//! according to card steps 1, 20, 21, 25, and 31.

#![expect(
    clippy::disallowed_methods,
    reason = "io::backup_classify reads files for backup classification"
)]

use std::collections::BTreeSet;
use std::path::Path;

use fieldbook_core::audit::{Event, Now, PendingWrite};
use fieldbook_core::backup::{BackupFileKind, CopiedFile};
use fieldbook_core::check::Model;
use fieldbook_core::labels::{
    ItemRef, LabelGraph, LabelId, LabelNode, LabelSet, Ledger, PUBLIC, StoreId, UNLABELLED,
    effective_item, markings_in,
};
use fieldbook_core::policy::{EffectivePolicy, PolicyState};
use fieldbook_core::report::{AddReport, LastBackupReport};

use crate::World;
use crate::io::state::BackupSettings;

/// Check whether a Corpus support file has public provenance.
#[must_use]
pub fn support_has_public_provenance(world: &dyn World, rel_path: &str, bytes: &[u8]) -> bool {
    let norm = rel_path.replace('\\', "/");
    let clean = norm.trim_start_matches('/');
    let corpus_rel = clean.strip_prefix("corpus/").unwrap_or(clean);
    if fieldbook_core::manifest::classify_support_path(corpus_rel).is_err() {
        return false;
    }
    let current_sha256 = fieldbook_core::hash::sha256_hex(bytes);
    let facts = world.support_facts();
    facts.has_provenance(corpus_rel, &current_sha256)
}

/// Checks if a relative path under `corpus/` corresponds to a recognized Corpus support file.
#[must_use]
pub fn is_corpus_support(rel_path: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    let clean = norm.trim_start_matches('/');
    let Some(corpus_rel) = clean.strip_prefix("corpus/") else {
        return false;
    };
    fieldbook_core::manifest::classify_support_path(corpus_rel).is_ok()
}

struct ModelLabelGraph<'a> {
    model: &'a Model,
    empty_ledger: Ledger,
}

impl LabelGraph for ModelLabelGraph<'_> {
    fn node(&self, item: &ItemRef) -> Option<LabelNode> {
        for s in self.model.sources() {
            let vid = format!("{}-v{}", s.value.source_id, s.value.version);
            if item.id == vid || item.id == s.value.source_id.as_str() {
                let own = parse_handling_vec(&s.value.handling);
                return Some(LabelNode::new(own, Vec::new()));
            }
        }
        for p in self.model.pages() {
            if item.id == p.path {
                let own = parse_handling_vec(&p.meta.handling);
                return Some(LabelNode::new(own, Vec::new()));
            }
        }
        None
    }

    fn ledger(&self, _store: StoreId) -> &Ledger {
        &self.empty_ledger
    }
}

fn parse_handling_vec(items: &[String]) -> LabelSet {
    if items.is_empty() {
        return LabelSet::unlabelled();
    }
    let mut ids = Vec::with_capacity(items.len());
    for s in items {
        let Ok(id) = LabelId::parse(s) else {
            return LabelSet::unlabelled();
        };
        ids.push(id);
    }
    LabelSet::from_ids(ids)
}

/// Classifies a single file and resolves its effective labels.
#[must_use]
pub fn classify_file(
    world: &dyn World,
    model: Option<&Model>,
    eff: &EffectivePolicy,
    rel_path: &str,
    bytes: &[u8],
) -> CopiedFile {
    let norm = rel_path.replace('\\', "/");
    let path = norm.trim_start_matches('/');

    // 1. Corpus pages
    if path.starts_with("corpus/pages/") && path.ends_with(".md") {
        let labels = if let Some(m) = model {
            if let Some(page) = m.pages().iter().find(|p| p.path == path) {
                parse_handling_vec(&page.meta.handling)
            } else {
                extract_handling_from_markdown(bytes)
            }
        } else {
            extract_handling_from_markdown(bytes)
        };
        return CopiedFile::new(path, BackupFileKind::Page, labels);
    }

    // 2. Originals under corpus/sources/public/
    if let Some(file_name) = path.strip_prefix("corpus/sources/public/") {
        let labels = if let Some(m) = model {
            let stem = file_name.split('.').next().unwrap_or(file_name);
            if let Some(src) = m.sources().iter().find(|s| {
                let vid = format!("{}-v{}", s.value.source_id, s.value.version);
                vid == stem || s.value.source_id.as_str() == stem
            }) {
                parse_handling_vec(&src.value.handling)
            } else {
                LabelSet::public()
            }
        } else {
            LabelSet::public()
        };
        return CopiedFile::new(path, BackupFileKind::Original, labels);
    }

    // 3. Extraction text
    if let Some(rest) = path.strip_prefix("corpus/sources/text/") {
        let vid_str = rest.split('/').next().unwrap_or("");
        let labels = if let Some(m) = model {
            if let Some(src) = m.sources().iter().find(|s| {
                let vid = format!("{}-v{}", s.value.source_id, s.value.version);
                vid == vid_str
            }) {
                parse_handling_vec(&src.value.handling)
            } else {
                LabelSet::public()
            }
        } else {
            LabelSet::public()
        };
        return CopiedFile::new(path, BackupFileKind::ExtractionText, labels);
    }

    // 4. Registers
    if path.starts_with("corpus/registers/") && path.ends_with(".csv") {
        let mut row_labels = BTreeSet::new();
        row_labels.insert(PUBLIC.clone());

        if let Some(m) = model
            && path == "corpus/registers/sources.csv"
        {
            for s in m.sources() {
                let set = parse_handling_vec(&s.value.handling);
                for id in set.ids() {
                    row_labels.insert(id.clone());
                }
            }
        }

        return CopiedFile::new(path, BackupFileKind::Record, LabelSet::from_ids(row_labels));
    }

    // 5. Corpus support files
    if is_corpus_support(path) {
        let prov = support_has_public_provenance(world, path, bytes);
        let mut labels = if prov {
            LabelSet::public()
        } else {
            LabelSet::unlabelled()
        };
        if prov {
            if let Ok(text) = std::str::from_utf8(bytes) {
                let rules = eff.marking_rules();
                if let Some(mark_set) = markings_in(text, &rules) {
                    labels = labels.union(&mark_set);
                }
            } else {
                labels = LabelSet::unlabelled();
            }
            if let PolicyState::Loaded(p) = eff.state() {
                let corpus_rule = p.store_rule(&StoreId::Corpus);
                if !corpus_rule.default.contains(&UNLABELLED) {
                    labels = labels.union(&corpus_rule.default);
                }
            }
        }
        return CopiedFile::new(path, BackupFileKind::CorpusSupport, labels);
    }

    // 6. Settings file
    if path == "state/settings.toml" {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return CopiedFile::new(path, BackupFileKind::Settings, LabelSet::unlabelled());
        };
        if BackupSettings::parse(text).is_err() {
            return CopiedFile::new(path, BackupFileKind::Settings, LabelSet::unlabelled());
        }
        let mut labels = LabelSet::owner_supplied();
        let rules = eff.marking_rules();
        if let Some(mark_set) = markings_in(text, &rules) {
            labels = labels.union(&mark_set);
        }
        if let PolicyState::Loaded(p) = eff.state() {
            let state_rule = p.store_rule(&StoreId::State);
            if !state_rule.default.contains(&UNLABELLED) {
                labels = labels.union(&state_rule.default);
            }
        }
        return CopiedFile::new(path, BackupFileKind::Settings, labels);
    }

    // 7. Last add report
    if path == "state/reports/last-add.json" {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return CopiedFile::new(path, BackupFileKind::LastAddReport, LabelSet::unlabelled());
        };
        let Ok(report) = serde_json::from_str::<AddReport>(text) else {
            return CopiedFile::new(path, BackupFileKind::LastAddReport, LabelSet::unlabelled());
        };
        if report.labels.ids().is_empty() || report.labels.contains(&UNLABELLED) {
            return CopiedFile::new(path, BackupFileKind::LastAddReport, LabelSet::unlabelled());
        }

        let mut union_labels = report.labels;
        if let Some(m) = model {
            let graph = ModelLabelGraph {
                model: m,
                empty_ledger: Ledger::default(),
            };
            for inp in &report.inputs {
                let item_ref = ItemRef::new(inp.store, &inp.id);
                let eff_item = effective_item(&graph, &item_ref);
                union_labels = union_labels.union(&eff_item);
            }
        } else if !report.inputs.is_empty() {
            union_labels = union_labels.union(&LabelSet::unlabelled());
        }

        return CopiedFile::new(path, BackupFileKind::LastAddReport, union_labels);
    }

    // 8. Last backup cache
    if path == "state/reports/last-backup.json" {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return CopiedFile::new(
                path,
                BackupFileKind::LastBackupCache,
                LabelSet::unlabelled(),
            );
        };
        let Ok(report) = serde_json::from_str::<LastBackupReport>(text) else {
            return CopiedFile::new(
                path,
                BackupFileKind::LastBackupCache,
                LabelSet::unlabelled(),
            );
        };
        if report.verification != "snapshot" || report.labels.is_empty() {
            return CopiedFile::new(
                path,
                BackupFileKind::LastBackupCache,
                LabelSet::unlabelled(),
            );
        }
        if Now::parse(&report.verified_utc).is_err() {
            return CopiedFile::new(
                path,
                BackupFileKind::LastBackupCache,
                LabelSet::unlabelled(),
            );
        }

        let mut labels = LabelSet::from_ids(report.labels);
        let rules = eff.marking_rules();
        if let Some(mark_set) = markings_in(&report.target_path, &rules) {
            labels = labels.union(&mark_set);
        }
        if let PolicyState::Loaded(p) = eff.state() {
            let state_rule = p.store_rule(&StoreId::State);
            if !state_rule.default.contains(&UNLABELLED) {
                labels = labels.union(&state_rule.default);
            }
        }
        if labels.ids().is_empty() || labels.contains(&UNLABELLED) {
            return CopiedFile::new(
                path,
                BackupFileKind::LastBackupCache,
                LabelSet::unlabelled(),
            );
        }
        return CopiedFile::new(path, BackupFileKind::LastBackupCache, labels);
    }

    // 9. Pending write descriptor
    if path == "state/pending.toml" {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
        };
        let Ok(pw) = PendingWrite::parse_toml(text) else {
            return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
        };
        let mut labels = pw.labels;
        if let Some(m) = model {
            let graph = ModelLabelGraph {
                model: m,
                empty_ledger: Ledger::default(),
            };
            for rec in &pw.affected {
                let item_ref = ItemRef::new(rec.store(), rec.id().as_str());
                let eff_item = effective_item(&graph, &item_ref);
                labels = labels.union(&eff_item);
            }
        }
        return CopiedFile::new(path, BackupFileKind::PendingWrite, labels);
    }

    // 10. Generation control file
    if path == "state/generation" {
        let is_valid = if let Ok(text) = std::str::from_utf8(bytes) {
            let trimmed = text.trim();
            trimmed.len() == 64
                && trimmed
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        } else {
            false
        };
        if is_valid {
            return CopiedFile::new(path, BackupFileKind::TextFreeControl, LabelSet::public());
        }
        return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
    }

    // 11. State locks and init markers
    if path == "state/.init"
        || path == "state/store.lock"
        || path == "state/journal.lock"
        || path == "state/lock"
    {
        return CopiedFile::new(path, BackupFileKind::TextFreeControl, LabelSet::public());
    }

    // 12. Audit journal files
    if let Some(subpath) = path.strip_prefix("state/audit/") {
        if subpath == "journal.head" || subpath.ends_with("/journal.head") {
            let is_valid = if let Ok(text) = std::str::from_utf8(bytes) {
                let trimmed = text.trim();
                trimmed.len() == 64
                    && trimmed
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            } else {
                false
            };
            if is_valid {
                return CopiedFile::new(path, BackupFileKind::TextFreeControl, LabelSet::public());
            }
            return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
        }

        if subpath.ends_with(".jsonl") {
            let is_valid = if let Ok(text) = std::str::from_utf8(bytes) {
                text.lines().all(|l| {
                    let trimmed = l.trim();
                    trimmed.is_empty() || Event::parse_line(trimmed).is_ok()
                })
            } else {
                false
            };
            if is_valid {
                return CopiedFile::new(path, BackupFileKind::TextFreeControl, LabelSet::public());
            }
            return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
        }

        // Unknown or invalid audit file contributes unlabelled
        return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
    }

    // 13. Handoff packs
    if path
        .strip_prefix("state/handoff/")
        .is_some_and(|sub| sub.starts_with("H-") && sub.ends_with(".txt"))
    {
        return CopiedFile::new(path, BackupFileKind::HandoffPack, LabelSet::unlabelled());
    }

    // 14. Retained public support catalogues
    if let Some(subpath) = path.strip_prefix("state/public-support/") {
        if !subpath.contains('/')
            && let Some(stem) = subpath.strip_suffix(".toml")
            && fieldbook_core::manifest::is_lower_hex(stem, 64)
        {
            let actual_sha = fieldbook_core::hash::sha256_hex(bytes);
            let facts = world.support_facts();
            let has_event = facts
                .catalogues
                .iter()
                .any(|(_, sha)| sha.eq_ignore_ascii_case(stem));
            let parse_ok = fieldbook_core::manifest::SupportCatalogue::parse_bytes(bytes).is_ok();

            if actual_sha == stem && has_event && parse_ok {
                let mut labels = LabelSet::public();
                if let Ok(text) = std::str::from_utf8(bytes) {
                    let rules = eff.marking_rules();
                    if let Some(mark_set) = markings_in(text, &rules) {
                        labels = labels.union(&mark_set);
                    }
                } else {
                    labels = LabelSet::unlabelled();
                }
                if let PolicyState::Loaded(p) = eff.state() {
                    let state_rule = p.store_rule(&StoreId::State);
                    if !state_rule.default.contains(&UNLABELLED) {
                        labels = labels.union(&state_rule.default);
                    }
                }
                return CopiedFile::new(path, BackupFileKind::CorpusSupport, labels);
            }
            return CopiedFile::new(path, BackupFileKind::CorpusSupport, LabelSet::unlabelled());
        }
        return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
    }

    // 15. Everything else is Stray
    CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled())
}

fn extract_handling_from_markdown(bytes: &[u8]) -> LabelSet {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return LabelSet::unlabelled();
    };
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("handling") {
            let rest = rest.trim_start();
            if let Some(val) = rest.strip_prefix('=') {
                let val = val.trim();
                if let Some(inner) = val.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                    return LabelSet::parse_cell(inner).unwrap_or_else(|_| LabelSet::unlabelled());
                }
                if let Some(arr) = val.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                    let mut ids = Vec::new();
                    for item in arr.split(',') {
                        let clean = item.trim().trim_matches('"');
                        if !clean.is_empty()
                            && let Ok(id) = LabelId::parse(clean)
                        {
                            ids.push(id);
                        }
                    }
                    return LabelSet::from_ids(ids);
                }
            }
        }
    }
    LabelSet::unlabelled()
}

/// Enumerates all relative file paths under `corpus/` and `state/`, skipping derived directories.
#[must_use]
pub fn enumerate_roots(world: &dyn World) -> Vec<String> {
    let mut files = Vec::new();
    let corpus_root = world.tree_root().join("corpus");
    if corpus_root.is_dir() {
        collect_rel_files(&corpus_root, "corpus", &mut files);
    }
    let state_root = world.state_dir();
    if state_root.is_dir() {
        collect_rel_files(state_root, "state", &mut files);
    }
    files
}

fn collect_rel_files(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let rel = format!("{prefix}/{name}");
        if path.is_dir() {
            // Skip derived folders
            if rel == "state/pages" || rel == "state/drill" || rel == "corpus/out" {
                continue;
            }
            collect_rel_files(&path, &rel, out);
        } else if path.is_file() {
            out.push(rel);
        }
    }
}

/// Reads file bytes given a normalized relative path starting with `corpus/` or `state/`.
#[must_use]
pub fn read_file_content(world: &dyn World, rel_path: &str) -> Vec<u8> {
    let norm = rel_path.replace('\\', "/");
    let path = norm.trim_start_matches('/');
    if let Some(corpus_rel) = path.strip_prefix("corpus/") {
        let p = world.tree_root().join("corpus").join(corpus_rel);
        std::fs::read(&p).unwrap_or_default()
    } else if let Some(state_rel) = path.strip_prefix("state/") {
        let p = world.state_dir().join(state_rel);
        std::fs::read(&p).unwrap_or_default()
    } else {
        std::fs::read(world.tree_root().join(path)).unwrap_or_default()
    }
}
