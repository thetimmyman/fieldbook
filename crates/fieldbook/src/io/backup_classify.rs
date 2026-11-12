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

use fieldbook_core::audit::Event;
use fieldbook_core::backup::{BackupFileKind, CopiedFile};
use fieldbook_core::check::Model;
use fieldbook_core::labels::{LabelId, LabelSet, OWNER_SUPPLIED, PUBLIC, markings_in};
use fieldbook_core::policy::EffectivePolicy;

use crate::World;

/// Check whether a Corpus support file has public provenance.
///
/// For now this returns true for all recognized support files. The next-but-one part
/// replaces it with the real catalogue/journal check of steps 20-23a.
#[must_use]
pub fn support_has_public_provenance(_world: &dyn World, _rel_path: &str, _bytes: &[u8]) -> bool {
    true
}

/// Checks if a relative path under `corpus/` corresponds to a recognized Corpus support file.
#[must_use]
pub fn is_corpus_support(rel_path: &str) -> bool {
    let norm = rel_path.replace('\\', "/");
    let clean = norm.trim_start_matches('/');
    if clean == "corpus/AGENTS.md"
        || clean == "corpus/ASSISTANT.md"
        || clean == "corpus/README.md"
        || clean == "corpus/corpus.toml"
        || clean == "corpus/changes.md"
        || clean == "corpus/scan-allow.toml"
        || clean == "corpus/eval/regression.toml"
        || clean == "corpus/pipeline/rules.toml"
    {
        return true;
    }
    if let Some(rest) = clean.strip_prefix("corpus/templates/") {
        if rest.ends_with(".md") {
            return true;
        }
        if let Some(wb_rest) = rest.strip_prefix("workbooks/")
            && wb_rest.ends_with(".csv")
        {
            return true;
        }
    }
    false
}

fn is_valid_audit_segment_name(name: &str) -> bool {
    name == "journal.jsonl"
        || name == "journal.head"
        || name.starts_with("journal.")
        || name.starts_with("audit.export")
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
        let labels = if prov {
            LabelSet::public()
        } else {
            LabelSet::unlabelled()
        };
        return CopiedFile::new(path, BackupFileKind::CorpusSupport, labels);
    }

    // 6. Settings file
    if path == "state/settings.toml" {
        let mut ids = vec![OWNER_SUPPLIED.clone()];
        if let Ok(text) = std::str::from_utf8(bytes) {
            let rules = eff.marking_rules();
            if let Some(mark_set) = markings_in(text, &rules) {
                for id in mark_set.ids() {
                    ids.push(id.clone());
                }
            }
        }
        return CopiedFile::new(path, BackupFileKind::Settings, LabelSet::from_ids(ids));
    }

    // 7. Last add report
    if path == "state/reports/last-add.json" {
        return CopiedFile::new(
            path,
            BackupFileKind::LastAddReport,
            LabelSet::from_ids([OWNER_SUPPLIED.clone()]),
        );
    }

    // 8. Last backup cache
    if path == "state/reports/last-backup.json" {
        return CopiedFile::new(
            path,
            BackupFileKind::LastBackupCache,
            LabelSet::from_ids([OWNER_SUPPLIED.clone()]),
        );
    }

    // 9. State locks
    if path == "state/store.lock" || path == "state/journal.lock" || path == "state/lock" {
        return CopiedFile::new(path, BackupFileKind::TextFreeControl, LabelSet::public());
    }

    // 10. Audit journal files
    if let Some(file_name) = path.strip_prefix("state/audit/") {
        if is_valid_audit_segment_name(file_name) {
            // Verify lines parse as valid JSONL event lines or are empty
            let valid_schema = if let Ok(text) = std::str::from_utf8(bytes) {
                text.lines()
                    .all(|l| l.is_empty() || Event::parse_line(l).is_ok())
            } else {
                false
            };

            if valid_schema
                || file_name == "journal.head"
                || file_name.starts_with("journal.broken")
            {
                return CopiedFile::new(path, BackupFileKind::TextFreeControl, LabelSet::public());
            }
        }
        // Unknown or invalid audit file contributes unlabelled
        return CopiedFile::new(path, BackupFileKind::Stray, LabelSet::unlabelled());
    }

    // 11. Everything else is Stray
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
