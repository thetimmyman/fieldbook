//! Shared edition and originals allowlist, path validator, and checksum/fingerprint/support
//! integrity verifier (`docs/cards/edition-manifest.md` step 2,
//! `docs/reference/corpus-format.md#06-site-and-edition`, `FB0601`, `FB0602`, `FB0608`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use fieldbook_core::check::{Code, Tree};
use fieldbook_core::edition::EditionInfo;
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::VersionId;
use fieldbook_core::manifest::{SupportCatalogue, fingerprint, shipped_corpus};
use fieldbook_core::schema::SourceRow;
use fieldbook_core::schema::table;
use fieldbook_core::schema::vocab::Lifecycle;
use serde::Deserialize;

/// Maximum relative path length from the edition or originals root (`FB0602`).
pub const MAX_EDITION_REL_PATH_LEN: usize = 150;

/// Packaging allowlist profile (`Raw` vs `Site`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AllowlistProfile {
    /// Raw floor edition (`cargo xtask edition-raw`): no generated site, browser summary, or `.html`/`.js`/`.css`.
    Raw,
    /// Rendered site edition (`cargo xtask edition`).
    Site,
}

/// Default required relative paths for [`AllowlistProfile::Raw`].
pub const RAW_REQUIRED_PATHS: &[&str] = &[
    "corpus/corpus.toml",
    "corpus/eval/regression.toml",
    "README.txt",
    "TRANSFER-STATEMENT.txt",
    "edition.toml",
    "evidence/check.json",
    "evidence/regression.json",
    "evidence/corpus-support.toml",
    "intake/ai-development-statement.md",
    "intake/data-handling.md",
    "intake/integrity-guide.md",
    "intake/network-silence.md",
];

/// Default required relative paths for [`AllowlistProfile::Site`] before a consumer supplies its own slice.
pub const SITE_REQUIRED_PATHS: &[&str] = &[
    "corpus/corpus.toml",
    "README.txt",
    "TRANSFER-STATEMENT.txt",
    "edition.toml",
    "evidence/corpus-support.toml",
    "intake/ai-development-statement.md",
    "intake/data-handling.md",
    "intake/integrity-guide.md",
    "intake/network-silence.md",
    "intake/network-silence-summary.txt",
];

const PROHIBITED_EXTENSIONS: &[&str] = &[
    "exe", "dll", "ps1", "bat", "cmd", "vbs", "lnk", "xlsm", "com", "msi", "scr", "js", "jse",
    "wsf", "wsh", "hta", "jar", "sh", "py",
];

const RENDERABLE_MARKUP_EXTENSIONS: &[&str] = &["html", "htm", "svg", "xhtml"];

/// The only extensions the Raw profile admits for a shipped original. Markup originals arrive
/// here already renamed to `.txt`. Every other extension is refused, so a new original type is
/// an owner decision that adds a word to this list.
pub(crate) const RAW_ORIGINAL_EXTENSIONS: &[&str] = &["pdf", "txt", "csv"];

const WINDOWS_RESERVED_BASENAMES: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// An allowlist, path, or integrity refusal (`FB0601`, `FB0602`, or `FB0608`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowlistViolation {
    /// Diagnostic code (`Code::Fb0601`, `Code::Fb0602`, or `Code::Fb0608`).
    pub code: Code,
    /// Relative path inside `edition/` or `originals/` where the violation occurred.
    pub path: String,
    /// Diagnostic explanation naming the path and rule.
    pub message: String,
}

impl fmt::Display for AllowlistViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: error[{}]: {}",
            self.path,
            self.code.id(),
            self.message
        )
    }
}

impl std::error::Error for AllowlistViolation {}

#[derive(Debug, Deserialize)]
struct ExtractionTomlSubset {
    original_sha256: String,
    #[serde(default)]
    pages: BTreeMap<String, String>,
}

fn assistant_md_rel_path() -> &'static str {
    fieldbook_core::strings::paths()[6]
}

fn is_lower_hex_64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_nfc_ascii_or_utf8(s: &str) -> bool {
    // Fast path for ASCII; for non-ASCII check canonical composition via SupportCatalogue path helper.
    if s.is_ascii() {
        return true;
    }
    // `classify_support_path` is for support files; check NFC by ensuring no combining decomposition.
    // Every valid Corpus/edition path is ASCII lowercase/digits/punctuation anyway.
    false
}

/// Validate path syntax, length (`<= 150`), reserved Windows names, and case-collision rules (`FB0602`).
pub fn validate_edition_rel_path(
    rel_path: &str,
    seen_lowercase: &mut BTreeMap<String, String>,
) -> Result<(), AllowlistViolation> {
    if rel_path.is_empty() {
        return Err(AllowlistViolation {
            code: Code::Fb0602,
            path: rel_path.to_owned(),
            message: "empty relative path is not allowed".to_owned(),
        });
    }
    if rel_path.chars().count() > MAX_EDITION_REL_PATH_LEN {
        return Err(AllowlistViolation {
            code: Code::Fb0602,
            path: rel_path.to_owned(),
            message: format!(
                "path `{rel_path}` exceeds maximum length of {MAX_EDITION_REL_PATH_LEN} characters ({} chars)",
                rel_path.chars().count()
            ),
        });
    }
    if rel_path.starts_with('/') || rel_path.ends_with('/') || rel_path.contains('\\') {
        return Err(AllowlistViolation {
            code: Code::Fb0602,
            path: rel_path.to_owned(),
            message: format!("path `{rel_path}` must be a clean slash-separated relative path"),
        });
    }
    if !is_nfc_ascii_or_utf8(rel_path) {
        return Err(AllowlistViolation {
            code: Code::Fb0602,
            path: rel_path.to_owned(),
            message: format!("path `{rel_path}` is not ASCII/NFC normalized"),
        });
    }

    for seg in rel_path.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(AllowlistViolation {
                code: Code::Fb0602,
                path: rel_path.to_owned(),
                message: format!("path `{rel_path}` contains invalid traversal segment `{seg}`"),
            });
        }
        if seg.ends_with('.') || seg.ends_with(' ') {
            return Err(AllowlistViolation {
                code: Code::Fb0602,
                path: rel_path.to_owned(),
                message: format!(
                    "path `{rel_path}` segment `{seg}` ends with a trailing dot or space"
                ),
            });
        }
        if seg
            .bytes()
            .any(|b| b < 0x20 || matches!(b, b'<' | b'>' | b':' | b'"' | b'|' | b'?' | b'*' | 0x7f))
        {
            return Err(AllowlistViolation {
                code: Code::Fb0602,
                path: rel_path.to_owned(),
                message: format!(
                    "path `{rel_path}` segment `{seg}` contains prohibited characters"
                ),
            });
        }
        let stem = seg.split('.').next().unwrap_or(seg).to_ascii_lowercase();
        if WINDOWS_RESERVED_BASENAMES.contains(&stem.as_str()) {
            return Err(AllowlistViolation {
                code: Code::Fb0602,
                path: rel_path.to_owned(),
                message: format!("path `{rel_path}` uses Windows reserved device name `{seg}`"),
            });
        }
    }

    let lower = rel_path.to_ascii_lowercase();
    if let Some(prev) = seen_lowercase.insert(lower, rel_path.to_owned())
        && prev != rel_path
    {
        return Err(AllowlistViolation {
            code: Code::Fb0602,
            path: rel_path.to_owned(),
            message: format!("path `{rel_path}` collides case-insensitively with `{prev}`"),
        });
    }

    Ok(())
}

/// Parse a `SHA256SUMS` file (`<64-hex>  <path>\n` lines sorted bytewise by `<path>`).
pub fn parse_sha256sums(
    sums_text: &str,
    sums_label_path: &str,
) -> Result<BTreeMap<String, String>, AllowlistViolation> {
    if sums_text.is_empty() || !sums_text.ends_with('\n') || sums_text.contains('\r') {
        return Err(AllowlistViolation {
            code: Code::Fb0608,
            path: sums_label_path.to_owned(),
            message: format!(
                "`{sums_label_path}` must be non-empty LF-terminated `<sha256>  <path>` lines"
            ),
        });
    }
    let mut map = BTreeMap::new();
    let mut prev_path: Option<&str> = None;
    for (idx, line) in sums_text.lines().enumerate() {
        let line_no = idx + 1;
        let Some((sha, path)) = line.split_once("  ") else {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: sums_label_path.to_owned(),
                message: format!(
                    "`{sums_label_path}` line {line_no} is malformed (expected `<sha256>  <path>`)"
                ),
            });
        };
        if !is_lower_hex_64(sha) || path.is_empty() {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: sums_label_path.to_owned(),
                message: format!(
                    "`{sums_label_path}` line {line_no} has invalid sha256 or empty path"
                ),
            });
        }
        if path == "SHA256SUMS" || path.ends_with("/SHA256SUMS") {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: sums_label_path.to_owned(),
                message: format!("`{sums_label_path}` must not list `SHA256SUMS` itself"),
            });
        }
        if let Some(prev) = prev_path
            && path.as_bytes() <= prev.as_bytes()
        {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: sums_label_path.to_owned(),
                message: format!(
                    "`{sums_label_path}` entries are not sorted bytewise (`{prev}` before `{path}`)"
                ),
            });
        }
        prev_path = Some(path);
        map.insert(path.to_owned(), sha.to_owned());
    }
    Ok(map)
}

fn collect_staged_tree_files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, AllowlistViolation> {
    let meta = fs::symlink_metadata(root).map_err(|e| AllowlistViolation {
        code: Code::Fb0601,
        path: root.display().to_string(),
        message: format!("cannot stat staged root `{}`: {e}", root.display()),
    })?;
    if meta.file_type().is_symlink() {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: root.display().to_string(),
            message: format!("staged root `{}` must not be a symlink", root.display()),
        });
    }
    if !meta.is_dir() {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: root.display().to_string(),
            message: format!("staged root `{}` is not a directory", root.display()),
        });
    }
    let mut files = BTreeMap::new();
    walk_staged_dir(root, "", &mut files)?;
    Ok(files)
}

fn walk_staged_dir(
    cur_dir: &Path,
    rel_prefix: &str,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), AllowlistViolation> {
    let mut entries: Vec<fs::DirEntry> = fs::read_dir(cur_dir)
        .map_err(|e| AllowlistViolation {
            code: Code::Fb0601,
            path: rel_prefix.to_owned(),
            message: format!("cannot read directory `{}`: {e}", cur_dir.display()),
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AllowlistViolation {
            code: Code::Fb0601,
            path: rel_prefix.to_owned(),
            message: format!("directory entry error in `{}`: {e}", cur_dir.display()),
        })?;
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            return Err(AllowlistViolation {
                code: Code::Fb0602,
                path: rel_prefix.to_owned(),
                message: "non-UTF-8 file name in staged tree".to_owned(),
            });
        };
        let rel = if rel_prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| AllowlistViolation {
            code: Code::Fb0601,
            path: rel.clone(),
            message: format!("cannot stat `{rel}`: {e}"),
        })?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: rel.clone(),
                message: format!("symlink `{rel}` is prohibited in staged edition/originals"),
            });
        }
        // Reject prohibited state / inbox / hidden directories even if empty.
        let top_seg = rel.split('/').next().unwrap_or(&rel);
        if matches!(
            top_seg,
            ".fieldbook" | "state" | "inbox" | "overlay" | "working"
        ) {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: rel.clone(),
                message: format!("prohibited state or working directory `{rel}` in staged tree"),
            });
        }
        if ft.is_dir() {
            walk_staged_dir(&path, &rel, out)?;
        } else if ft.is_file() {
            let bytes = fs::read(&path).map_err(|e| AllowlistViolation {
                code: Code::Fb0601,
                path: rel.clone(),
                message: format!("cannot read `{rel}`: {e}"),
            })?;
            out.insert(rel, bytes);
        } else {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: rel.clone(),
                message: format!("special file `{rel}` is prohibited in staged tree"),
            });
        }
    }
    Ok(())
}

/// Validate staged `edition` and `originals` directories on disk against `profile` and
/// `required_paths`, checking path rules (`FB0602`), allowlist rules (`FB0601`), and
/// `SHA256SUMS` / `edition.toml` / `extraction.toml` / `corpus-support.toml` integrity (`FB0608`).
pub fn verify_staged_trees(
    profile: AllowlistProfile,
    edition_dir: &Path,
    originals_dir: &Path,
    required_paths: &[&str],
) -> Result<(), AllowlistViolation> {
    let edition_files = collect_staged_tree_files(edition_dir)?;
    let originals_files = collect_staged_tree_files(originals_dir)?;
    verify_staged_maps(profile, &edition_files, &originals_files, required_paths)
}

/// Validate in-memory staged `edition` and `originals` file maps against `profile` and
/// `required_paths`.
pub fn verify_staged_maps(
    profile: AllowlistProfile,
    edition_files: &BTreeMap<String, Vec<u8>>,
    originals_files: &BTreeMap<String, Vec<u8>>,
    required_paths: &[&str],
) -> Result<(), AllowlistViolation> {
    // 1. Validate all relative paths (`FB0602`) in both maps first.
    let mut seen_edition_lower = BTreeMap::new();
    for path in edition_files.keys() {
        validate_edition_rel_path(path, &mut seen_edition_lower)?;
    }
    let mut seen_orig_lower = BTreeMap::new();
    for path in originals_files.keys() {
        validate_edition_rel_path(path, &mut seen_orig_lower)?;
    }

    // 2. Check required paths (`FB0601`), including `ASSISTANT.md` and `evidence/corpus-support.toml`.
    let default_reqs = match profile {
        AllowlistProfile::Raw => RAW_REQUIRED_PATHS,
        AllowlistProfile::Site => SITE_REQUIRED_PATHS,
    };
    let assistant_path = assistant_md_rel_path();
    if !edition_files.contains_key(assistant_path) {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: assistant_path.to_owned(),
            message: format!("staged edition is missing required file `{assistant_path}`"),
        });
    }
    if !edition_files.contains_key("evidence/corpus-support.toml") {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: "evidence/corpus-support.toml".to_owned(),
            message: "staged edition is missing required file `evidence/corpus-support.toml`"
                .to_owned(),
        });
    }
    for &req in default_reqs.iter().chain(required_paths.iter()) {
        if !edition_files.contains_key(req) {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: req.to_owned(),
                message: format!("staged edition is missing required file `{req}`"),
            });
        }
    }

    // 3. Parse `corpus/registers/sources.csv` to identify active vs quarantined source versions and original hashes.
    let sources_bytes = edition_files
        .get("corpus/registers/sources.csv")
        .ok_or_else(|| AllowlistViolation {
            code: Code::Fb0601,
            path: "corpus/registers/sources.csv".to_owned(),
            message: "staged edition is missing `corpus/registers/sources.csv`".to_owned(),
        })?;
    let sources_table = table::read(sources_bytes).map_err(|e| AllowlistViolation {
        code: Code::Fb0601,
        path: "corpus/registers/sources.csv".to_owned(),
        message: format!("invalid `corpus/registers/sources.csv`: {e}"),
    })?;

    let mut quarantined_versions: BTreeSet<VersionId> = BTreeSet::new();
    let mut source_hashes: BTreeMap<VersionId, String> = BTreeMap::new();
    let mut expected_originals: BTreeMap<String, (VersionId, String)> = BTreeMap::new();

    for row in &sources_table.rows {
        let sr = SourceRow::from_row(row).map_err(|errs| AllowlistViolation {
            code: Code::Fb0601,
            path: "corpus/registers/sources.csv".to_owned(),
            message: format!(
                "invalid row in `corpus/registers/sources.csv` at line {}: {:?}",
                row.line, errs
            ),
        })?;
        if sr.handling != vec!["public".to_owned()] {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: "corpus/registers/sources.csv".to_owned(),
                message: format!(
                    "non-public source row `{}-v{}` in edition",
                    sr.source_id, sr.version
                ),
            });
        }
        let vid_str = format!("{}-v{}", sr.source_id, sr.version);
        let vid = VersionId::parse(&vid_str).map_err(|e| AllowlistViolation {
            code: Code::Fb0601,
            path: "corpus/registers/sources.csv".to_owned(),
            message: format!("invalid version id `{vid_str}`: {e}"),
        })?;
        source_hashes.insert(vid.clone(), sr.sha256.clone());

        if sr.lifecycle == Lifecycle::Quarantined {
            if sr.note.as_deref().unwrap_or("").trim().is_empty() {
                return Err(AllowlistViolation {
                    code: Code::Fb0601,
                    path: "corpus/registers/sources.csv".to_owned(),
                    message: format!(
                        "quarantined source version `{vid}` must retain its reason in `note`"
                    ),
                });
            }
            quarantined_versions.insert(vid);
        } else if !sr.location.starts_with("metadata-only:") {
            let shipped_orig_name = resolve_shipped_original(&vid, &sr.location, originals_files)?;
            expected_originals.insert(shipped_orig_name, (vid, sr.sha256.clone()));
        }
    }

    // 4. Inspect every file in `edition_files` against `AllowlistProfile` and prohibited content (`FB0601`).
    for (path, bytes) in edition_files {
        check_edition_file_allowed(profile, path, bytes, &quarantined_versions)?;
    }

    // 5. Inspect every file in `originals_files` (`FB0601` / `FB0608`).
    for (path, bytes) in originals_files {
        if path == "SHA256SUMS" {
            continue;
        }
        check_originals_file_allowed(profile, path, bytes, &quarantined_versions)?;
        let Some((_vid, expected_sha)) = expected_originals.get(path) else {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: path.clone(),
                message: format!("unregistered or prohibited original `{path}` in originals tree"),
            });
        };
        let actual_sha = sha256_hex(bytes);
        if &actual_sha != expected_sha {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: path.clone(),
                message: format!(
                    "original `{path}` sha256 `{actual_sha}` differs from `sources.csv` (`{expected_sha}`)"
                ),
            });
        }
    }

    for (expected_path, (vid, _expected_sha)) in &expected_originals {
        if !originals_files.contains_key(expected_path) {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: expected_path.clone(),
                message: format!(
                    "expected shipped original `{expected_path}` for `{vid}` is missing from `originals`"
                ),
            });
        }
    }

    // Check that no quarantined version has any original in `originals_files`.
    for q_vid in &quarantined_versions {
        let prefix = format!("{q_vid}.");
        for path in originals_files.keys() {
            if path.starts_with(&prefix) {
                return Err(AllowlistViolation {
                    code: Code::Fb0601,
                    path: path.clone(),
                    message: format!(
                        "quarantined version `{q_vid}` must not ship original `{path}`"
                    ),
                });
            }
        }
    }

    // 6. Verify `SHA256SUMS` in `edition_files` and `originals_files` (`FB0608`).
    verify_tree_sha256sums(edition_files, "SHA256SUMS")?;
    verify_tree_sha256sums(originals_files, "originals/SHA256SUMS")?;

    // 7. Verify `edition.toml` fingerprint against `shipped_corpus` (`FB0608`).
    let edition_toml_bytes =
        edition_files
            .get("edition.toml")
            .ok_or_else(|| AllowlistViolation {
                code: Code::Fb0601,
                path: "edition.toml".to_owned(),
                message: "missing `edition.toml`".to_owned(),
            })?;
    let edition_toml_str =
        std::str::from_utf8(edition_toml_bytes).map_err(|e| AllowlistViolation {
            code: Code::Fb0608,
            path: "edition.toml".to_owned(),
            message: format!("`edition.toml` is not valid UTF-8: {e}"),
        })?;
    let edition_info = EditionInfo::parse(edition_toml_str).map_err(|e| AllowlistViolation {
        code: Code::Fb0608,
        path: "edition.toml".to_owned(),
        message: format!("invalid `edition.toml`: {e}"),
    })?;

    let mut corpus_tree = Tree::new();
    corpus_tree.insert_dir("corpus");
    for (path, bytes) in edition_files {
        if path.starts_with("corpus/") {
            corpus_tree.insert_bytes(path.clone(), bytes.clone());
        }
    }
    let mut shipped =
        shipped_corpus(&corpus_tree, &quarantined_versions).map_err(|e| AllowlistViolation {
            code: Code::Fb0601,
            path: "corpus".to_owned(),
            message: format!("shipped_corpus validation failed: {e}"),
        })?;
    if profile == AllowlistProfile::Raw
        && let Some(reg_bytes) = edition_files.get("corpus/eval/regression.toml")
    {
        // EditionInfo::from accepts either pure shipped_corpus or shipped_corpus + regression.toml.
        let base_fp = fingerprint(&shipped);
        if edition_info.fingerprint != base_fp {
            shipped.push((
                "corpus/eval/regression.toml".to_owned(),
                sha256_hex(reg_bytes),
            ));
            shipped.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        }
    }
    let recomputed_fp = fingerprint(&shipped);
    if edition_info.fingerprint != recomputed_fp {
        return Err(AllowlistViolation {
            code: Code::Fb0608,
            path: "edition.toml".to_owned(),
            message: format!(
                "`edition.toml` fingerprint `{}` differs from recomputed Corpus fingerprint `{recomputed_fp}`",
                edition_info.fingerprint
            ),
        });
    }

    // 8. Verify extraction hashes in `corpus/sources/text/<version-id>/extraction.toml` (`FB0608`).
    verify_extraction_hashes(edition_files, &source_hashes)?;

    // 9. Verify `evidence/corpus-support.toml` syntax, strict schema, and present support files (`FB0608`).
    let support_bytes = edition_files
        .get("evidence/corpus-support.toml")
        .ok_or_else(|| AllowlistViolation {
            code: Code::Fb0601,
            path: "evidence/corpus-support.toml".to_owned(),
            message: "missing `evidence/corpus-support.toml`".to_owned(),
        })?;
    let support_cat =
        SupportCatalogue::parse_bytes(support_bytes).map_err(|e| AllowlistViolation {
            code: Code::Fb0608,
            path: "evidence/corpus-support.toml".to_owned(),
            message: format!("invalid `evidence/corpus-support.toml`: {e}"),
        })?;
    // Verify any support file that is also present in the staged `corpus/` tree (e.g. `eval/regression.toml`).
    for entry in &support_cat.files {
        let corpus_rel = format!("corpus/{}", entry.path);
        if let Some(staged_bytes) = edition_files.get(&corpus_rel) {
            let actual_sha = sha256_hex(staged_bytes);
            if actual_sha != entry.sha256 {
                return Err(AllowlistViolation {
                    code: Code::Fb0608,
                    path: "evidence/corpus-support.toml".to_owned(),
                    message: format!(
                        "support file `{corpus_rel}` sha256 `{actual_sha}` differs from `evidence/corpus-support.toml` (`{}`)",
                        entry.sha256
                    ),
                });
            }
        }
    }
    // Also verify `ASSISTANT.md` at the edition root against `evidence/corpus-support.toml` when listed.
    let assistant_name = assistant_md_rel_path();
    if let Some(assistant_entry) = support_cat.files.iter().find(|e| e.path == assistant_name)
        && let Some(staged_assistant) = edition_files.get(assistant_name)
    {
        let actual_sha = sha256_hex(staged_assistant);
        if actual_sha != assistant_entry.sha256 {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: "evidence/corpus-support.toml".to_owned(),
                message: format!(
                    "`{assistant_name}` sha256 `{actual_sha}` differs from `evidence/corpus-support.toml` (`{}`)",
                    assistant_entry.sha256
                ),
            });
        }
    }

    Ok(())
}

/// The shipped name of a registered original.
///
/// The version id selects the shipped original, and the location is provenance:
/// - a location whose file name ends in a prohibited extension is refused, whatever the tree
///   holds: a file name such as `setup.ps1`, or a web address whose last path segment is one
///   (`https://example.invalid/setup.ps1?raw=1`; the host, query and fragment are not read);
/// - when the name derived from `location` is in the originals tree, that is the original;
/// - otherwise the single `<version id>.*` file in the tree is the original (a web address such
///   as an eCFR renderer URL names no file).
///
/// The location does not bind the shipped type. That is bound by the checks that follow: the
/// shipped-name rules, the markup rename, the Raw extension allowlist and executable check, and
/// the registered digest.
fn resolve_shipped_original(
    vid: &VersionId,
    location: &str,
    originals_files: &BTreeMap<String, Vec<u8>>,
) -> Result<String, AllowlistViolation> {
    let path = match location.split_once("://") {
        Some((_, rest)) => rest.split_once('/').map_or("", |(_, p)| p),
        None => location,
    };
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let last = path.rsplit('/').next().unwrap_or_default();
    if let Some((_, ext)) = last.rsplit_once('.') {
        let ext = ext.to_ascii_lowercase();
        if PROHIBITED_EXTENSIONS.contains(&ext.as_str()) {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: location.to_owned(),
                message: format!(
                    "source version `{vid}` location `{location}` names a prohibited `.{ext}` file"
                ),
            });
        }
    }
    let derived = shipped_original_filename(vid, location);
    if let Ok(name) = &derived
        && originals_files.contains_key(name)
    {
        return derived;
    }
    let prefix = format!("{vid}.");
    let mut candidates = originals_files.keys().filter(|k| k.starts_with(&prefix));
    if let (Some(only), None) = (candidates.next(), candidates.next()) {
        return Ok(only.clone());
    }
    derived
}

/// Compute the shipped filename inside `originals/` for a registered source version (`ED-01`:
/// `.html`, `.htm`, `.svg`, and `.xhtml` originals gain `.txt`).
pub fn shipped_original_filename(
    vid: &VersionId,
    location: &str,
) -> Result<String, AllowlistViolation> {
    let ext = location
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if ext.is_empty() {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: location.to_owned(),
            message: format!("source version `{vid}` location `{location}` has no extension"),
        });
    }
    if PROHIBITED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: location.to_owned(),
            message: format!(
                "source version `{vid}` original `{location}` has prohibited extension `.{ext}`"
            ),
        });
    }
    let base = format!("{vid}.{ext}");
    if RENDERABLE_MARKUP_EXTENSIONS.contains(&ext.as_str()) {
        Ok(format!("{base}.txt"))
    } else {
        Ok(base)
    }
}

fn check_originals_file_allowed(
    profile: AllowlistProfile,
    path: &str,
    bytes: &[u8],
    quarantined: &BTreeSet<VersionId>,
) -> Result<(), AllowlistViolation> {
    if path.contains('/') {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: path.to_owned(),
            message: format!("nested path `{path}` is not allowed in `originals/`"),
        });
    }
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if PROHIBITED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: path.to_owned(),
            message: format!("prohibited file `{path}` in `originals/`"),
        });
    }
    if RENDERABLE_MARKUP_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: path.to_owned(),
            message: format!(
                "renderable markup original `{path}` must ship as `{path}.txt` in `originals/`"
            ),
        });
    }
    if profile == AllowlistProfile::Raw {
        if !RAW_ORIGINAL_EXTENSIONS.contains(&ext.as_str()) {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: path.to_owned(),
                message: format!(
                    "original `{path}` has extension `.{ext}`; the raw edition admits only {} originals",
                    RAW_ORIGINAL_EXTENSIONS
                        .iter()
                        .map(|e| format!(".{e}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
        if bytes.starts_with(b"MZ") || bytes.starts_with(b"\x7fELF") {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: path.to_owned(),
                message: format!("original `{path}` begins like a Windows or Linux executable"),
            });
        }
    }
    for q_vid in quarantined {
        let prefix = format!("{q_vid}.");
        if path.starts_with(&prefix) {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: path.to_owned(),
                message: format!("quarantined version `{q_vid}` original `{path}` must not ship"),
            });
        }
    }
    Ok(())
}

fn check_edition_file_allowed(
    profile: AllowlistProfile,
    path: &str,
    bytes: &[u8],
    quarantined: &BTreeSet<VersionId>,
) -> Result<(), AllowlistViolation> {
    let top_seg = path.split('/').next().unwrap_or(path);
    if matches!(
        top_seg,
        ".fieldbook" | "state" | "inbox" | "overlay" | "working"
    ) {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: path.to_owned(),
            message: format!("prohibited state or inbox path `{path}` in edition"),
        });
    }

    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    // Check prohibited extensions (`exe`, `dll`, `ps1`, `bat`, `cmd`, `vbs`, `lnk`, `xlsm`, etc.).
    // Note: `.js` is allowed ONLY in `Site` profile under `assets/*.js`.
    if ext == "js" {
        let allowed_site_js = profile == AllowlistProfile::Site
            && matches!(
                path,
                "assets/app.js"
                    | "assets/search.js"
                    | "assets/search-index.js"
                    | "assets/strings.js"
                    | "assets/assist.js"
                    | "assets/assist-data.js"
            );
        if !allowed_site_js {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: path.to_owned(),
                message: format!("prohibited script `{path}` in {profile:?} edition"),
            });
        }
    } else if PROHIBITED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: path.to_owned(),
            message: format!("prohibited executable or script file `{path}` in edition"),
        });
    }

    // Check `.html`, `.htm`, `.svg`, `.xhtml` placement.
    if RENDERABLE_MARKUP_EXTENSIONS.contains(&ext.as_str())
        && (profile == AllowlistProfile::Raw || path.starts_with("corpus/") || ext != "html")
    {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: path.to_owned(),
            message: format!(
                "prohibited markup file `{path}` outside site HTML in {profile:?} edition"
            ),
        });
    }

    // Check quarantined version leaks (`corpus/sources/text/<version-id>/`, `s/<version-id>.html`, or search units).
    for q_vid in quarantined {
        let q_text_prefix = format!("corpus/sources/text/{q_vid}/");
        let q_site_page = format!("s/{q_vid}.html");
        if path.starts_with(&q_text_prefix) || path == q_site_page {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: path.to_owned(),
                message: format!(
                    "quarantined source version `{q_vid}` payload `{path}` must not ship"
                ),
            });
        }
        if (path == "assets/search-index.js" || path == "assets/assist-data.js")
            && let Ok(text) = std::str::from_utf8(bytes)
            && contains_complete_version_id(text, q_vid.as_str())
        {
            return Err(AllowlistViolation {
                code: Code::Fb0601,
                path: path.to_owned(),
                message: format!(
                    "quarantined source version `{q_vid}` appears in search/assist unit `{path}`"
                ),
            });
        }
    }

    // Remote URLs in assets (`assets/*`) are prohibited (`FB0601`).
    if path.starts_with("assets/")
        && let Ok(text) = std::str::from_utf8(bytes)
        && (text.contains("http://") || text.contains("https://"))
    {
        return Err(AllowlistViolation {
            code: Code::Fb0601,
            path: path.to_owned(),
            message: format!("remote URL found in asset `{path}`"),
        });
    }

    // Profile-specific allowlist checks.
    match profile {
        AllowlistProfile::Raw => check_raw_profile_path(path)?,
        AllowlistProfile::Site => check_site_profile_path(path)?,
    }

    Ok(())
}

/// Match a complete version identifier (`S-NNNN-vM`) in `text` without matching a prefix of
/// another identifier (e.g. `S-0042-v1` must not match `S-0042-v10`).
#[must_use]
pub fn contains_complete_version_id(text: &str, vid: &str) -> bool {
    if vid.is_empty() {
        return false;
    }
    let bytes = text.as_bytes();
    let is_id_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
    for (idx, _) in text.match_indices(vid) {
        let before_ok = idx == 0 || !is_id_byte(bytes[idx - 1]);
        let end = idx + vid.len();
        let after_ok = end == bytes.len() || !is_id_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

fn check_raw_profile_path(path: &str) -> Result<(), AllowlistViolation> {
    let assistant_path = assistant_md_rel_path();
    if path == "SHA256SUMS"
        || path == "README.txt"
        || path == "TRANSFER-STATEMENT.txt"
        || path == "edition.toml"
        || path == assistant_path
        || matches!(
            path,
            "evidence/check.json"
                | "evidence/regression.json"
                | "evidence/corpus-support.toml"
                | "intake/ai-development-statement.md"
                | "intake/data-handling.md"
                | "intake/integrity-guide.md"
                | "intake/network-silence.md"
                | "corpus/corpus.toml"
                | "corpus/eval/regression.toml"
        )
    {
        return Ok(());
    }
    if is_allowed_shipped_corpus_subpath(path) {
        return Ok(());
    }
    Err(AllowlistViolation {
        code: Code::Fb0601,
        path: path.to_owned(),
        message: format!("path `{path}` is not permitted in the Raw edition allowlist"),
    })
}

fn check_site_profile_path(path: &str) -> Result<(), AllowlistViolation> {
    let assistant_path = assistant_md_rel_path();
    if path == "SHA256SUMS"
        || path == "README.txt"
        || path == "TRANSFER-STATEMENT.txt"
        || path == "edition.toml"
        || path == assistant_path
        || fieldbook_core::strings::paths()[3..=5].contains(&path)
        || matches!(
            path,
            "evidence/corpus-support.toml"
                | "intake/ai-development-statement.md"
                | "intake/data-handling.md"
                | "intake/integrity-guide.md"
                | "intake/network-silence.md"
                | "intake/network-silence-summary.txt"
                | "corpus/corpus.toml"
                | "index.html"
                | "about.html"
                | "claims.html"
                | "sources.html"
                | "questions.html"
                | "conflicts.html"
                | "glossary.html"
                | "ids.html"
                | "search.html"
                | "changes.html"
                | "assets/app.css"
                | "assets/app.js"
                | "assets/search.js"
                | "assets/search-index.js"
                | "assets/strings.js"
                | "assets/assist.js"
                | "assets/assist-data.js"
        )
    {
        return Ok(());
    }
    if is_allowed_shipped_corpus_subpath(path) {
        return Ok(());
    }
    if let Some(rest) = path.strip_prefix("a/")
        && !rest.contains('/')
        && rest.ends_with(".html")
    {
        return Ok(());
    }
    if let Some(rest) = path.strip_prefix("p/")
        && !rest.contains('/')
        && rest.ends_with(".html")
    {
        return Ok(());
    }
    if let Some(rest) = path.strip_prefix("s/")
        && !rest.contains('/')
        && rest.ends_with(".html")
    {
        return Ok(());
    }
    if let Some(rest) = path.strip_prefix("data/")
        && !rest.contains('/')
        && (rest == "registers.xlsx" || rest.ends_with(".csv"))
    {
        return Ok(());
    }
    if let Some(rest) = path.strip_prefix("templates/")
        && !rest.contains('/')
        && rest.ends_with(".xlsx")
    {
        return Ok(());
    }

    Err(AllowlistViolation {
        code: Code::Fb0601,
        path: path.to_owned(),
        message: format!("path `{path}` is not permitted in the Site edition allowlist"),
    })
}

fn is_allowed_shipped_corpus_subpath(path: &str) -> bool {
    if let Some(rest) = path.strip_prefix("corpus/registers/") {
        return matches!(
            rest,
            "sources.csv" | "claims.csv" | "questions.csv" | "conflicts.csv" | "glossary.csv"
        );
    }
    if let Some(rest) = path.strip_prefix("corpus/pages/") {
        let mut parts = rest.split('/');
        return matches!(
            (parts.next(), parts.next(), parts.next(), parts.next()),
            (Some(a), Some(s), Some(f), None)
                if !a.is_empty() && !s.is_empty() && f.ends_with(".md")
        );
    }
    if let Some(rest) = path.strip_prefix("corpus/sources/text/") {
        let mut parts = rest.split('/');
        return matches!(
            (parts.next(), parts.next(), parts.next()),
            (Some(v), Some(f), None)
                if !v.is_empty() && (f == "extraction.toml" || f.ends_with(".txt"))
        );
    }
    false
}

fn verify_tree_sha256sums(
    files: &BTreeMap<String, Vec<u8>>,
    sums_label_path: &str,
) -> Result<(), AllowlistViolation> {
    let sums_bytes = files.get("SHA256SUMS").ok_or_else(|| AllowlistViolation {
        code: Code::Fb0608,
        path: sums_label_path.to_owned(),
        message: format!("missing `{sums_label_path}`"),
    })?;
    let sums_text = std::str::from_utf8(sums_bytes).map_err(|e| AllowlistViolation {
        code: Code::Fb0608,
        path: sums_label_path.to_owned(),
        message: format!("`{sums_label_path}` is not valid UTF-8: {e}"),
    })?;
    let listed = parse_sha256sums(sums_text, sums_label_path)?;

    for (path, bytes) in files {
        if path == "SHA256SUMS" {
            continue;
        }
        let Some(expected_sha) = listed.get(path) else {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: path.clone(),
                message: format!("file `{path}` is missing from `{sums_label_path}`"),
            });
        };
        let actual_sha = sha256_hex(bytes);
        if &actual_sha != expected_sha {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: path.clone(),
                message: format!(
                    "file `{path}` sha256 `{actual_sha}` differs from `{sums_label_path}` (`{expected_sha}`)"
                ),
            });
        }
    }

    for listed_path in listed.keys() {
        if !files.contains_key(listed_path) {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: listed_path.clone(),
                message: format!(
                    "`{sums_label_path}` lists `{listed_path}`, which does not exist in the staged tree"
                ),
            });
        }
    }

    Ok(())
}

fn verify_extraction_hashes(
    edition_files: &BTreeMap<String, Vec<u8>>,
    source_hashes: &BTreeMap<VersionId, String>,
) -> Result<(), AllowlistViolation> {
    for (path, bytes) in edition_files {
        let Some(rest) = path.strip_prefix("corpus/sources/text/") else {
            continue;
        };
        let Some(vid_str) = rest.strip_suffix("/extraction.toml") else {
            continue;
        };
        let vid = VersionId::parse(vid_str).map_err(|e| AllowlistViolation {
            code: Code::Fb0608,
            path: path.clone(),
            message: format!("invalid version folder `{vid_str}`: {e}"),
        })?;
        let text = std::str::from_utf8(bytes).map_err(|e| AllowlistViolation {
            code: Code::Fb0608,
            path: path.clone(),
            message: format!("`{path}` is not valid UTF-8: {e}"),
        })?;
        let parsed: ExtractionTomlSubset =
            toml::from_str(text).map_err(|e| AllowlistViolation {
                code: Code::Fb0608,
                path: path.clone(),
                message: format!("invalid `{path}`: {e}"),
            })?;
        let Some(expected_orig_sha) = source_hashes.get(&vid) else {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: path.clone(),
                message: format!(
                    "`{path}` belongs to unknown source version `{vid}` not in `sources.csv`"
                ),
            });
        };
        if &parsed.original_sha256 != expected_orig_sha {
            return Err(AllowlistViolation {
                code: Code::Fb0608,
                path: path.clone(),
                message: format!(
                    "`{path}` original_sha256 `{}` differs from `sources.csv` (`{expected_orig_sha}`)",
                    parsed.original_sha256
                ),
            });
        }
        for (stem, expected_page_sha) in &parsed.pages {
            let page_path = format!("corpus/sources/text/{vid}/{stem}.txt");
            let Some(page_bytes) = edition_files.get(&page_path) else {
                return Err(AllowlistViolation {
                    code: Code::Fb0608,
                    path: page_path.clone(),
                    message: format!("extraction page `{page_path}` listed in `{path}` is missing"),
                });
            };
            let actual_page_sha = sha256_hex(page_bytes);
            if &actual_page_sha != expected_page_sha {
                return Err(AllowlistViolation {
                    code: Code::Fb0608,
                    path: page_path.clone(),
                    message: format!(
                        "extraction page `{page_path}` sha256 `{actual_page_sha}` differs from `{path}` (`{expected_page_sha}`)"
                    ),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Code, Entry, Host, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::edition::{BuildInfo, BuildProfile, EditionInfo};
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check as egress_check};
    use fieldbook_core::hash::sha256_hex;
    use fieldbook_core::ids::VersionId;
    use fieldbook_core::manifest::{SupportCatalogue, shipped_corpus, write_sums};
    use fieldbook_core::policy::EffectivePolicy;
    use fieldbook_core::schema::Register;

    use super::{
        AllowlistProfile, RAW_REQUIRED_PATHS, SITE_REQUIRED_PATHS, resolve_shipped_original,
        shipped_original_filename, verify_staged_maps,
    };

    const MINI_CORPUS_TOML: &str =
        include_str!("../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    fn build_synthetic_staged_pair(
        profile: AllowlistProfile,
    ) -> (BTreeMap<String, Vec<u8>>, BTreeMap<String, Vec<u8>>) {
        let orig_bytes = b"synthetic active original payload\n".to_vec();
        let orig_sha = sha256_hex(&orig_bytes);
        let page_txt_bytes = b"extracted page one text\n".to_vec();
        let page_txt_sha = sha256_hex(&page_txt_bytes);
        let extraction_toml = format!(
            "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 1\nthin_pages = []\noriginal_sha256 = \"{orig_sha}\"\n\n[pages]\np0001 = \"{page_txt_sha}\"\n"
        );

        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        let assistant_name = fieldbook_core::strings::paths()[6];
        tree.insert_bytes(format!("corpus/{assistant_name}"), b"rules_version = 1\n");
        tree.insert_bytes(
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: synthetic.\n",
        );
        tree.insert_dir("corpus/registers");
        for reg in Register::ALL {
            let mut header = String::new();
            for (idx, col) in reg.columns().iter().enumerate() {
                if idx > 0 {
                    header.push(',');
                }
                header.push_str(col.name);
            }
            header.push('\n');
            tree.insert_bytes(
                format!("corpus/registers/{}", reg.file_name()),
                header.into_bytes(),
            );
        }
        let sources_csv = format!(
            "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\nS-0001,1,syn-one,Synthetic Active Source,Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_one,reviewed,S-0001-v1.html,{orig_sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Active fixture.,Baseline,,,,,\nS-0002,1,syn-two,Synthetic Held Source,Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_two,reviewed,S-0002-v1.pdf,ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad,2026-01-10,2026-01-15,2026-02-01,2026-02-01,quarantined,,V2,public,Held for synthetic licence review.,Held test,,,,,\n"
        );
        tree.insert_bytes("corpus/registers/sources.csv", sources_csv.as_bytes());
        tree.insert(
            "corpus/sources/public/S-0001-v1.html",
            Entry::Original {
                sha256: orig_sha.clone(),
                len: orig_bytes.len() as u64,
            },
        );
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            extraction_toml.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/p0001.txt",
            page_txt_bytes.clone(),
        );
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-sample.md",
            b"+++\nid = \"cmp-sample\"\ntitle = \"Synthetic Title\"\ntype = \"concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\nstage = 3\nstatus = \"draft\"\nreadiness = \"SYNTHETIC-NOW\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic.\n\n## The rule\n\nSynthetic.\n\n## How it works\n\nSynthetic.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\n## Never confuse\n\nSynthetic.\n\n## Company calibration\n\nSynthetic.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n",
        );
        tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");

        let today = CivilDate::from_ymd(2026, 3, 1).expect("valid date");
        let (model, report) = load(&Host::new(tree, today), &Options::default());
        assert_eq!(report.errors(), 0, "{:?}", report.diagnostics);

        let eff = EffectivePolicy::floor(false);
        let now = Now::new(today, 0).expect("valid now");
        let Decision::Allowed(allowed) =
            egress_check(&eff, &Dest::Edition, None, None, now, Shipped::Hours(0))
        else {
            panic!("edition allowed");
        };
        let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());

        let mut quarantined = BTreeSet::new();
        quarantined.insert(VersionId::parse("S-0002-v1").expect("valid vid"));
        let shipped = shipped_corpus(&view, &quarantined).expect("shipped_corpus");

        let commit = "1111111111111111111111111111111111111111";
        let build_info = BuildInfo::new(
            Some(today),
            Some(commit.to_owned()),
            BuildProfile::Silent,
            Some(1_772_323_200),
        );
        let edition_info = EditionInfo::from(
            &view,
            &shipped,
            Some("edition-001"),
            &build_info,
            "0.1.0",
            1,
        )
        .expect("EditionInfo");

        let (support_bytes, _) = SupportCatalogue::from_public_source(&view, commit)
            .expect("SupportCatalogue")
            .to_toml_bytes_and_sha256();

        let mut edition_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        for (path, _) in &shipped {
            let bytes = view.tree().get_bytes(path).expect("shipped bytes").to_vec();
            edition_files.insert(path.clone(), bytes);
        }
        edition_files.insert(assistant_name.to_owned(), b"rules_version = 1\n".to_vec());
        edition_files.insert("README.txt".to_owned(), b"Synthetic README\n".to_vec());
        edition_files.insert(
            "TRANSFER-STATEMENT.txt".to_owned(),
            b"Synthetic transfer statement\n".to_vec(),
        );
        edition_files.insert(
            "edition.toml".to_owned(),
            edition_info.to_toml().into_bytes(),
        );
        edition_files.insert("evidence/corpus-support.toml".to_owned(), support_bytes);
        edition_files.insert(
            "intake/ai-development-statement.md".to_owned(),
            b"# AI\n".to_vec(),
        );
        edition_files.insert("intake/data-handling.md".to_owned(), b"# Data\n".to_vec());
        edition_files.insert(
            "intake/integrity-guide.md".to_owned(),
            b"# Integrity\n".to_vec(),
        );
        edition_files.insert(
            "intake/network-silence.md".to_owned(),
            b"# Network\n".to_vec(),
        );

        match profile {
            AllowlistProfile::Raw => {
                edition_files.insert(
                    "corpus/eval/regression.toml".to_owned(),
                    b"case = []\n".to_vec(),
                );
                edition_files.insert(
                    "evidence/check.json".to_owned(),
                    b"{\"errors\":0}\n".to_vec(),
                );
                edition_files.insert(
                    "evidence/regression.json".to_owned(),
                    b"{\"passed\":true}\n".to_vec(),
                );
            }
            AllowlistProfile::Site => {
                edition_files.insert(
                    "intake/network-silence-summary.txt".to_owned(),
                    b"status: pass\n".to_vec(),
                );
                edition_files.insert("index.html".to_owned(), b"<!doctype html>\n".to_vec());
                edition_files.insert("about.html".to_owned(), b"<!doctype html>\n".to_vec());
                edition_files.insert("assets/app.css".to_owned(), b"body {}\n".to_vec());
                edition_files.insert("assets/app.js".to_owned(), b"\"use strict\";\n".to_vec());
            }
        }

        refresh_sums(&mut edition_files);

        // Renderable original `.html` ships as `.html.txt` in `originals/`.
        let vid = VersionId::parse("S-0001-v1").expect("vid");
        let shipped_orig_name =
            shipped_original_filename(&vid, "S-0001-v1.html").expect("shipped_orig_name");
        assert_eq!(shipped_orig_name, "S-0001-v1.html.txt");

        let mut originals_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        originals_files.insert(shipped_orig_name, orig_bytes);
        refresh_sums(&mut originals_files);

        (edition_files, originals_files)
    }

    fn refresh_sums(files: &mut BTreeMap<String, Vec<u8>>) {
        files.remove("SHA256SUMS");
        let entries: Vec<(String, String)> = files
            .iter()
            .map(|(p, b)| (p.clone(), sha256_hex(b)))
            .collect();
        let sums = write_sums(&entries);
        files.insert("SHA256SUMS".to_owned(), sums.into_bytes());
    }

    #[test]
    fn allowlist_accepts_clean_raw_and_site_trees_and_refuses_prohibited_or_quarantined_files() {
        // 1. Clean Raw and Site trees pass.
        let (raw_ed, raw_orig) = build_synthetic_staged_pair(AllowlistProfile::Raw);
        verify_staged_maps(
            AllowlistProfile::Raw,
            &raw_ed,
            &raw_orig,
            RAW_REQUIRED_PATHS,
        )
        .expect("clean Raw tree passes");

        let (site_ed, site_orig) = build_synthetic_staged_pair(AllowlistProfile::Site);
        verify_staged_maps(
            AllowlistProfile::Site,
            &site_ed,
            &site_orig,
            SITE_REQUIRED_PATHS,
        )
        .expect("clean Site tree passes");

        // 2. Planted `.ps1` script in edition fails with FB0601.
        let mut bad_ps1 = raw_ed.clone();
        bad_ps1.insert("corpus/planted.ps1".to_owned(), b"Write-Host hi\n".to_vec());
        refresh_sums(&mut bad_ps1);
        let err = verify_staged_maps(AllowlistProfile::Raw, &bad_ps1, &raw_orig, &[])
            .expect_err("planted .ps1 must fail");
        assert_eq!(err.code, Code::Fb0601);
        assert!(err.path.contains("planted.ps1"));

        // 3. Quarantined payload in `corpus/sources/text/S-0002-v1/` or `originals/` or `s/S-0002-v1.html` fails with FB0601.
        let mut bad_q_text = raw_ed.clone();
        bad_q_text.insert(
            "corpus/sources/text/S-0002-v1/p0001.txt".to_owned(),
            b"held text\n".to_vec(),
        );
        refresh_sums(&mut bad_q_text);
        let err = verify_staged_maps(AllowlistProfile::Raw, &bad_q_text, &raw_orig, &[])
            .expect_err("quarantined text must fail");
        assert_eq!(err.code, Code::Fb0601);

        let mut bad_q_orig = raw_orig.clone();
        bad_q_orig.insert("S-0002-v1.pdf".to_owned(), b"held pdf\n".to_vec());
        refresh_sums(&mut bad_q_orig);
        let err = verify_staged_maps(AllowlistProfile::Raw, &raw_ed, &bad_q_orig, &[])
            .expect_err("quarantined original must fail");
        assert_eq!(err.code, Code::Fb0601);

        let mut bad_q_site_page = site_ed.clone();
        bad_q_site_page.insert("s/S-0002-v1.html".to_owned(), b"<!doctype html>\n".to_vec());
        refresh_sums(&mut bad_q_site_page);
        let err = verify_staged_maps(AllowlistProfile::Site, &bad_q_site_page, &site_orig, &[])
            .expect_err("quarantined s/ page must fail");
        assert_eq!(err.code, Code::Fb0601);

        // 4. Site file (`index.html`) planted in Raw profile fails with FB0601.
        let mut bad_raw_html = raw_ed.clone();
        bad_raw_html.insert("index.html".to_owned(), b"<!doctype html>\n".to_vec());
        refresh_sums(&mut bad_raw_html);
        let err = verify_staged_maps(AllowlistProfile::Raw, &bad_raw_html, &raw_orig, &[])
            .expect_err("HTML in Raw edition must fail");
        assert_eq!(err.code, Code::Fb0601);

        // 5. Missing `evidence/corpus-support.toml` fails with FB0601 naming that path.
        let mut missing_support = raw_ed.clone();
        missing_support.remove("evidence/corpus-support.toml");
        refresh_sums(&mut missing_support);
        let err = verify_staged_maps(AllowlistProfile::Raw, &missing_support, &raw_orig, &[])
            .expect_err("missing corpus-support.toml must fail");
        assert_eq!(err.code, Code::Fb0601);
        assert_eq!(err.path, "evidence/corpus-support.toml");
    }

    #[test]
    fn raw_originals_are_an_extension_allowlist_and_the_site_profile_keeps_its_denylist() {
        let (raw_ed, raw_orig) = build_synthetic_staged_pair(AllowlistProfile::Raw);
        let payload = raw_orig["S-0001-v1.html.txt"].clone();
        // Types the old denylist let through, plus ordinary office and archive types.
        for ext in [
            "url", "reg", "docm", "xlsb", "xlam", "vbe", "psm1", "chm", "pif", "cpl", "xml", "mht",
            "zip", "xls", "doc", "docx", "xlsx", "css", "pptm", "xll",
        ] {
            let mut orig = BTreeMap::new();
            orig.insert(format!("S-0001-v1.{ext}"), payload.clone());
            refresh_sums(&mut orig);
            let err = verify_staged_maps(AllowlistProfile::Raw, &raw_ed, &orig, &[])
                .expect_err(&format!(".{ext} must be refused in a raw edition"));
            assert_eq!(err.code, Code::Fb0601, "{ext}: {err}");
            assert!(err.path.contains(&format!(".{ext}")), "{ext}: {err}");
        }
        // The three admitted types, and a markup original renamed to .txt, pass.
        for ext in ["pdf", "txt", "csv"] {
            let mut orig = BTreeMap::new();
            orig.insert(format!("S-0001-v1.{ext}"), payload.clone());
            refresh_sums(&mut orig);
            verify_staged_maps(AllowlistProfile::Raw, &raw_ed, &orig, &[])
                .unwrap_or_else(|e| panic!(".{ext} must pass: {e}"));
        }
        // Executable magic is refused even under an admitted extension.
        for magic in [&b"MZ\x90\x00rest"[..], &b"\x7fELF\x02rest"[..]] {
            let mut orig = BTreeMap::new();
            orig.insert("S-0001-v1.pdf".to_owned(), magic.to_vec());
            refresh_sums(&mut orig);
            let err = verify_staged_maps(AllowlistProfile::Raw, &raw_ed, &orig, &[])
                .expect_err("executable magic must be refused");
            assert_eq!(err.code, Code::Fb0601);
        }
        // The Site profile keeps its denylist: an extension outside the raw list passes there.
        // The original is found as the single `<version id>.*` file, because the location is a
        // web address; the Site profile shares that resolution with the Raw profile.
        let (site_ed, _) = build_synthetic_staged_pair(AllowlistProfile::Site);
        let mut orig = BTreeMap::new();
        orig.insert("S-0001-v1.xml".to_owned(), payload);
        refresh_sums(&mut orig);
        verify_staged_maps(AllowlistProfile::Site, &site_ed, &orig, &[])
            .expect("the Site profile is not narrowed by the raw allowlist");
    }

    #[test]
    fn a_location_naming_a_prohibited_file_is_never_rescued_by_the_version_file() {
        let vid = VersionId::parse("S-0001-v1").expect("vid");
        let mut one: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        one.insert("S-0001-v1.txt".to_owned(), vec![1]);
        for location in [
            "https://example.invalid/setup.ps1",
            "https://example.invalid/dir/SETUP.PS1?raw=1",
            "https://example.invalid/a.exe#frag",
            "setup.ps1",
            "S-0001-v1.exe",
        ] {
            let err = resolve_shipped_original(&vid, location, &one)
                .expect_err(&format!("{location} must be refused"));
            assert_eq!(err.code, Code::Fb0601, "{location}");
        }
        // The location is provenance, not the shipped type: a harmless file name, a web address
        // with no file name, or a query that merely mentions a script, falls back to the single
        // version file, whose type the later checks bind.
        for location in [
            "https://www.example.com",
            "https://www.ecfr.gov/api/renderer/v1/content/enhanced/2026-10-27/title-48?chapter=1&part=2",
            "https://example.invalid/report.pdf?download=setup.ps1",
            "S-0001-v1.pdf",
        ] {
            assert_eq!(
                resolve_shipped_original(&vid, location, &one).as_deref(),
                Ok("S-0001-v1.txt"),
                "{location}"
            );
        }
    }

    #[test]
    fn a_registered_original_is_found_by_name_or_by_its_single_version_file() {
        let vid = VersionId::parse("S-0001-v1").expect("vid");
        let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        // A file-name location whose derived name is in the tree wins.
        files.insert("S-0001-v1.html.txt".to_owned(), vec![1]);
        files.insert("S-0001-v1.pdf".to_owned(), vec![2]);
        assert_eq!(
            resolve_shipped_original(&vid, "S-0001-v1.html", &files).as_deref(),
            Ok("S-0001-v1.html.txt")
        );
        // A web address is not a file name; the single `<version id>.*` file is the original.
        let mut one: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        one.insert("S-0001-v1.pdf".to_owned(), vec![2]);
        one.insert("S-0002-v1.pdf".to_owned(), vec![3]);
        assert_eq!(
            resolve_shipped_original(&vid, "https://example.invalid/path/doc", &one).as_deref(),
            Ok("S-0001-v1.pdf")
        );
        // Two candidates are ambiguous: nothing is guessed, and the derived result stands.
        let derived = shipped_original_filename(&vid, "https://example.invalid/path/doc");
        assert_eq!(
            resolve_shipped_original(&vid, "https://example.invalid/path/doc", &files),
            derived
        );
        // No candidate at all falls back to the derived result as well.
        let none: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        assert_eq!(
            resolve_shipped_original(&vid, "S-0001-v1.pdf", &none).as_deref(),
            Ok("S-0001-v1.pdf")
        );
    }

    #[test]
    fn allowlist_refuses_long_paths_and_changed_bytes_or_checksums() {
        let (raw_ed, raw_orig) = build_synthetic_staged_pair(AllowlistProfile::Raw);

        // 1. Path > 150 characters fails with FB0602.
        let long_seg = "a".repeat(140);
        let long_path = format!("corpus/pages/compliance/business-systems/{long_seg}.md");
        assert!(long_path.len() > 150);
        let mut bad_long = raw_ed.clone();
        bad_long.insert(long_path.clone(), b"x\n".to_vec());
        let err = verify_staged_maps(AllowlistProfile::Raw, &bad_long, &raw_orig, &[])
            .expect_err("151+ char path must fail");
        assert_eq!(err.code, Code::Fb0602);
        assert_eq!(err.path, long_path);

        // 2. Changed file byte without updating SHA256SUMS fails with FB0608.
        let mut bad_changed_file = raw_ed.clone();
        bad_changed_file.insert("README.txt".to_owned(), b"tampered readme\n".to_vec());
        let err = verify_staged_maps(AllowlistProfile::Raw, &bad_changed_file, &raw_orig, &[])
            .expect_err("tampered file must fail FB0608");
        assert_eq!(err.code, Code::Fb0608);
        assert_eq!(err.path, "README.txt");

        // 3. Changed checksum byte in SHA256SUMS fails with FB0608.
        let mut bad_sums = raw_ed.clone();
        let mut sums_bytes = bad_sums.get("SHA256SUMS").expect("sums").clone();
        sums_bytes[0] = if sums_bytes[0] == b'0' { b'1' } else { b'0' };
        bad_sums.insert("SHA256SUMS".to_owned(), sums_bytes);
        let err = verify_staged_maps(AllowlistProfile::Raw, &bad_sums, &raw_orig, &[])
            .expect_err("corrupted SHA256SUMS byte must fail FB0608");
        assert_eq!(err.code, Code::Fb0608);

        // 4. Changed Corpus file with updated SHA256SUMS but stale `edition.toml` fingerprint fails with FB0608.
        let mut bad_fp = raw_ed.clone();
        bad_fp
            .get_mut("corpus/sources/text/S-0001-v1/p0001.txt")
            .expect("p0001")
            .push(b'\n');
        refresh_sums(&mut bad_fp);
        let err = verify_staged_maps(AllowlistProfile::Raw, &bad_fp, &raw_orig, &[])
            .expect_err("stale edition.toml fingerprint must fail FB0608");
        assert_eq!(err.code, Code::Fb0608);
        assert_eq!(err.path, "edition.toml");
    }
}
