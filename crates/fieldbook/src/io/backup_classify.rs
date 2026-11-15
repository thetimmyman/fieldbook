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
use fieldbook_core::manifest::SupportFacts;
use fieldbook_core::policy::{EffectivePolicy, PolicyState};
use fieldbook_core::report::{AddReport, LastBackupReport};
use fieldbook_core::schema::Register;

use crate::World;
use crate::io::state::BackupSettings;

/// Per-thread callback invoked after [`read_file_content`] reads a file.
pub type AfterReadHook = Box<dyn FnMut(&str) + Send>;

thread_local! {
    static AFTER_READ_HOOK: std::cell::RefCell<Option<AfterReadHook>> =
        const { std::cell::RefCell::new(None) };
}

/// Installs or clears a per-thread hook invoked after [`read_file_content`] reads a file.
pub fn set_after_read_hook(hook: Option<AfterReadHook>) {
    AFTER_READ_HOOK.with(|cell| {
        *cell.borrow_mut() = hook;
    });
}

/// Check whether a Corpus support file has public provenance.
#[must_use]
pub fn support_has_public_provenance(facts: &SupportFacts, rel_path: &str, bytes: &[u8]) -> bool {
    let norm = rel_path.replace('\\', "/");
    let clean = norm.trim_start_matches('/');
    let corpus_rel = clean.strip_prefix("corpus/").unwrap_or(clean);
    if fieldbook_core::manifest::classify_support_path(corpus_rel).is_err() {
        return false;
    }
    let current_sha256 = fieldbook_core::hash::sha256_hex(bytes);
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
    ledgers: std::collections::BTreeMap<StoreId, Ledger>,
    empty_ledger: Ledger,
}

fn broken_ledger() -> Ledger {
    Ledger::parse("\0").unwrap_or_else(Ledger::broken)
}

fn load_store_ledger(world: &dyn World, model: &Model, store: StoreId) -> Ledger {
    let rel = format!("{}/labels.csv", store.as_str());
    if let Some(bytes) = model.tree().get_bytes(&rel) {
        return match std::str::from_utf8(bytes) {
            Ok(text) => Ledger::parse(text).unwrap_or_else(Ledger::broken),
            Err(_) => broken_ledger(),
        };
    }
    let path = if store == StoreId::State {
        world.state_dir().join("labels.csv")
    } else {
        world.tree_root().join(store.as_str()).join("labels.csv")
    };
    match std::fs::symlink_metadata(&path) {
        Ok(meta) => {
            if !meta.file_type().is_file() {
                return broken_ledger();
            }
            match std::fs::read(&path) {
                Ok(bytes) => match std::str::from_utf8(&bytes) {
                    Ok(text) => Ledger::parse(text).unwrap_or_else(Ledger::broken),
                    Err(_) => broken_ledger(),
                },
                Err(_) => broken_ledger(),
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ledger::empty(),
        Err(_) => broken_ledger(),
    }
}

impl<'a> ModelLabelGraph<'a> {
    fn from_model(model: &'a Model) -> Self {
        let mut ledgers = std::collections::BTreeMap::new();
        for store in [
            StoreId::Corpus,
            StoreId::Overlay,
            StoreId::Working,
            StoreId::State,
        ] {
            let rel = format!("{}/labels.csv", store.as_str());
            let ledger = match model.tree().get_bytes(&rel) {
                Some(bytes) => match std::str::from_utf8(bytes) {
                    Ok(text) => Ledger::parse(text).unwrap_or_else(Ledger::broken),
                    Err(_) => broken_ledger(),
                },
                None => Ledger::empty(),
            };
            ledgers.insert(store, ledger);
        }
        Self {
            model,
            ledgers,
            empty_ledger: Ledger::default(),
        }
    }

    fn from_world(world: &dyn World, model: &'a Model) -> Self {
        let mut ledgers = std::collections::BTreeMap::new();
        for store in [
            StoreId::Corpus,
            StoreId::Overlay,
            StoreId::Working,
            StoreId::State,
        ] {
            ledgers.insert(store, load_store_ledger(world, model, store));
        }
        Self {
            model,
            ledgers,
            empty_ledger: Ledger::default(),
        }
    }
}

impl LabelGraph for ModelLabelGraph<'_> {
    fn node(&self, item: &ItemRef) -> Option<LabelNode> {
        if item.store == StoreId::Corpus {
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
            return None;
        }
        // A ledger records labels, not proof that an item or its dependencies exist.
        None
    }

    fn ledger(&self, store: StoreId) -> &Ledger {
        self.ledgers.get(&store).unwrap_or(&self.empty_ledger)
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

/// True when `file_name` is exactly `<version-id>.<ext>`, where `<ext>` is one or more ASCII
/// letters or digits and holds no further dot.
fn is_version_file_name(file_name: &str, version_id: &str) -> bool {
    let Some((stem, ext)) = file_name.split_once('.') else {
        return false;
    };
    stem == version_id && !ext.is_empty() && ext.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// True when the model parsed `register` and typed every one of its data rows.
fn register_fully_loaded(model: &Model, register: Register) -> bool {
    if model.register(register).is_none() {
        return false;
    }
    let typed = match register {
        Register::Sources => model.sources().len(),
        Register::Claims => model.claims().len(),
        Register::Questions => model.questions().len(),
        Register::Conflicts => model.conflicts().len(),
        Register::Glossary => model.glossary().len(),
    };
    model.register_rows(register).len() == typed
}

/// Classifies a single file and resolves its effective labels.
#[must_use]
pub fn classify_file(
    facts: &SupportFacts,
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
            let strict = m.sources().iter().find(|s| {
                let vid = format!("{}-v{}", s.value.source_id, s.value.version);
                is_version_file_name(file_name, &vid)
            });
            if let Some(src) = strict {
                parse_handling_vec(&src.value.handling)
            } else {
                // A name that is not exactly `<version-id>.<ext>` keeps the labels the old
                // source-id match gave it, beside `unlabelled`, so the result is never wider
                // than either the old set or plain `unlabelled`.
                let stem = file_name.split('.').next().unwrap_or(file_name);
                let prior = m
                    .sources()
                    .iter()
                    .find(|s| {
                        let vid = format!("{}-v{}", s.value.source_id, s.value.version);
                        vid == stem || s.value.source_id.as_str() == stem
                    })
                    .map_or_else(LabelSet::unlabelled, |s| {
                        parse_handling_vec(&s.value.handling)
                    });
                LabelSet::unlabelled().union(&prior)
            }
        } else {
            LabelSet::unlabelled()
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
                LabelSet::unlabelled()
            }
        } else {
            LabelSet::unlabelled()
        };
        return CopiedFile::new(path, BackupFileKind::ExtractionText, labels);
    }

    // 4. Registers
    if let Some(name) = path.strip_prefix("corpus/registers/")
        && !name.contains('/')
        && Register::ALL.iter().any(|r| r.file_name() == name)
    {
        // Fail closed: a register contributes labels only when the model loaded it completely
        // (every data row typed) and `sources.csv` itself is complete, so no source handling is
        // dropped. Otherwise it is unlabelled, as the search index treats a register that does
        // not parse.
        let loaded = model.filter(|m| register_fully_loaded(m, Register::Sources));
        let own_loaded = model.is_some_and(|m| {
            Register::ALL
                .iter()
                .find(|r| r.file_name() == name)
                .is_some_and(|r| register_fully_loaded(m, *r))
        });
        let Some(m) = loaded.filter(|_| own_loaded) else {
            return CopiedFile::new(path, BackupFileKind::Record, LabelSet::unlabelled());
        };
        let mut row_labels = BTreeSet::new();
        row_labels.insert(PUBLIC.clone());
        for s in m.sources() {
            let set = parse_handling_vec(&s.value.handling);
            for id in set.ids() {
                row_labels.insert(id.clone());
            }
        }

        return CopiedFile::new(path, BackupFileKind::Record, LabelSet::from_ids(row_labels));
    }

    // 5. Corpus support files
    if is_corpus_support(path) {
        let prov = support_has_public_provenance(facts, path, bytes);
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
            let graph = ModelLabelGraph::from_model(m);
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
            let graph = ModelLabelGraph::from_model(m);
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
        if bytes.is_empty() {
            return CopiedFile::new(path, BackupFileKind::TextFreeControl, LabelSet::public());
        }
        return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
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
                        if clean.is_empty() {
                            continue;
                        }
                        // An id that is not a label makes the whole page unlabelled.
                        let Ok(id) = LabelId::parse(clean) else {
                            return LabelSet::unlabelled();
                        };
                        ids.push(id);
                    }
                    return LabelSet::from_ids(ids);
                }
            }
        }
    }
    LabelSet::unlabelled()
}

/// An error encountered while enumerating or reading files for backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupReadError {
    pub path: String,
    pub reason: String,
}

impl std::fmt::Display for BackupReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.reason.starts_with("no corpus folder at ") {
            write!(f, "{}", self.reason)
        } else {
            write!(f, "cannot read {}: {}", self.path, self.reason)
        }
    }
}

impl std::error::Error for BackupReadError {}

/// Enumerates all relative file paths under `corpus/` and `state/`, skipping derived directories.
///
/// # Errors
///
/// Returns [`BackupReadError`] if root or subdirectories cannot be enumerated or contain non-regular files.
pub fn enumerate_roots(world: &dyn World) -> Result<Vec<String>, BackupReadError> {
    let mut files = Vec::new();
    let corpus_root = world.tree_root().join("corpus");
    match std::fs::symlink_metadata(&corpus_root) {
        Ok(meta) if meta.file_type().is_dir() => {
            collect_rel_files(&corpus_root, "corpus", &mut files)?;
        }
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(BackupReadError {
                path: corpus_root.display().to_string(),
                reason: "not a regular file or folder (symbolic link or special file)".to_string(),
            });
        }
        Ok(_) => {
            return Err(BackupReadError {
                path: corpus_root.display().to_string(),
                reason: format!("no corpus folder at {}", corpus_root.display()),
            });
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Err(BackupReadError {
                path: corpus_root.display().to_string(),
                reason: format!("no corpus folder at {}", corpus_root.display()),
            });
        }
        Err(err) => {
            return Err(BackupReadError {
                path: corpus_root.display().to_string(),
                reason: err.to_string(),
            });
        }
    }
    let state_root = world.state_dir();
    if let Ok(meta) = std::fs::symlink_metadata(state_root) {
        if meta.file_type().is_dir() {
            collect_rel_files(state_root, "state", &mut files)?;
        } else if !meta.file_type().is_file() {
            return Err(BackupReadError {
                path: state_root.display().to_string(),
                reason: "not a regular file or folder (symbolic link or special file)".to_string(),
            });
        }
    }
    Ok(files)
}

fn collect_rel_files(
    dir: &Path,
    prefix: &str,
    out: &mut Vec<String>,
) -> Result<(), BackupReadError> {
    let read_dir = std::fs::read_dir(dir).map_err(|err| BackupReadError {
        path: dir.display().to_string(),
        reason: err.to_string(),
    })?;
    for entry_res in read_dir {
        let entry = entry_res.map_err(|err| BackupReadError {
            path: dir.display().to_string(),
            reason: err.to_string(),
        })?;
        let entry_path = entry.path();
        let Some(name) = entry_path.file_name().and_then(|n| n.to_str()) else {
            return Err(BackupReadError {
                path: entry_path.display().to_string(),
                reason: "file name is not valid UTF-8".to_string(),
            });
        };
        let file_type = entry.file_type().map_err(|err| BackupReadError {
            path: entry_path.display().to_string(),
            reason: err.to_string(),
        })?;
        let rel = format!("{prefix}/{name}");
        if file_type.is_dir() {
            // Skip derived folders when they are real directories
            if rel == "state/pages" || rel == "state/drill" || rel == "corpus/out" {
                continue;
            }
            collect_rel_files(&entry_path, &rel, out)?;
        } else if file_type.is_file() {
            out.push(rel);
        } else {
            return Err(BackupReadError {
                path: entry_path.display().to_string(),
                reason: "not a regular file or folder (symbolic link or special file)".to_string(),
            });
        }
    }
    Ok(())
}

/// Reads file bytes given a normalized relative path starting with `corpus/` or `state/`.
///
/// # Errors
///
/// Returns [`BackupReadError`] if the path is not a regular file or cannot be read.
pub fn read_file_content(world: &dyn World, rel_path: &str) -> Result<Vec<u8>, BackupReadError> {
    let norm = rel_path.replace('\\', "/");
    let path = norm.trim_start_matches('/');
    let p = if let Some(corpus_rel) = path.strip_prefix("corpus/") {
        world.tree_root().join("corpus").join(corpus_rel)
    } else if let Some(state_rel) = path.strip_prefix("state/") {
        world.state_dir().join(state_rel)
    } else {
        world.tree_root().join(path)
    };
    let meta = std::fs::symlink_metadata(&p).map_err(|err| BackupReadError {
        path: p.display().to_string(),
        reason: err.to_string(),
    })?;
    if !meta.file_type().is_file() {
        return Err(BackupReadError {
            path: p.display().to_string(),
            reason: "not a regular file or folder (symbolic link or special file)".to_string(),
        });
    }
    let bytes = std::fs::read(&p).map_err(|err| BackupReadError {
        path: p.display().to_string(),
        reason: err.to_string(),
    })?;
    let hook = AFTER_READ_HOOK.with(|cell| cell.borrow_mut().take());
    if let Some(mut f) = hook {
        f(path);
        AFTER_READ_HOOK.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = Some(f);
            }
        });
    }
    Ok(bytes)
}

/// Resolves all report inputs against one coherent Corpus generation and store ledgers.
pub fn report_input_labels(
    world: &dyn World,
    model: &Model,
    report: &AddReport,
) -> Option<LabelSet> {
    let graph = ModelLabelGraph::from_world(world, model);
    let mut labels = report.labels.clone();
    for input in &report.inputs {
        let item = ItemRef::new(input.store, &input.id);
        graph.node(&item)?;
        labels = labels.union(&effective_item(&graph, &item));
    }
    Some(labels)
}

/// True when `state/labels.csv` or any referenced input store's `labels.csv` fails strict ledger parsing.
pub fn broken_desk_ledger(world: &dyn World, model: &Model, report: Option<&AddReport>) -> bool {
    if load_store_ledger(world, model, StoreId::State).is_broken() {
        return true;
    }
    if let Some(report) = report {
        for input in &report.inputs {
            if load_store_ledger(world, model, input.store).is_broken() {
                return true;
            }
        }
    }
    false
}
