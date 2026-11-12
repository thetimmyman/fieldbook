//! `cargo xtask edition`: build, verify, and package the rendered site edition.
//!
//! Checks the working tree at the current commit, performs a strict check,
//! builds the site into `out/edition` and `out/originals`, instruments them
//! with the browser harness, verifies allowlist rules (`FB0601`, `FB0602`, `FB0608`),
//! fills intake placeholders (`IN-01`), and writes deterministic archives and checksums
//! into `out/dist`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use fieldbook_core::edition::{EditionInfo, latest_edition_heading};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::{EditionId, VersionId};
use fieldbook_core::manifest::{
    SupportCatalogue, shipped_corpus, verify_support_catalogue_bytes, write_sums,
};
use fieldbook_core::schema::vocab::Lifecycle;

use crate::allowlist::{AllowlistProfile, parse_sha256sums, verify_staged_maps};
use crate::edition_raw::{
    RunOutput, SECURITY_CONTACT_ENV, fill_placeholders, require_clean_tree, sums_of,
};
use crate::zipdet::{read_and_verify_deterministic_zip, write_deterministic_zip_bytes};

/// Required relative paths for the rendered site edition (`FB0601`).
pub const REQUIRED_FILES: &[&str] = &[
    "index.html",
    "about.html",
    "corpus/corpus.toml",
    // `ASSISTANT.md`: one frozen spelling, from `strings`.
    fieldbook_core::strings::PATHS[6],
    "README.txt",
    "TRANSFER-STATEMENT.txt",
    "edition.toml",
    "evidence/corpus-support.toml",
    "intake/ai-development-statement.md",
    "intake/data-handling.md",
    "intake/integrity-guide.md",
    "intake/network-silence.md",
    "intake/network-silence-summary.txt",
    // `day-one.html`: the day-one binder, a Site page.
    fieldbook_core::strings::PATHS[3],
    // data/: the workbook and the CSV copies of the registers.
    fieldbook_core::strings::PATHS[0],
    "data/claims.csv",
    "data/conflicts.csv",
    "data/glossary.csv",
    "data/questions.csv",
    "data/sources.csv",
];

/// The two attestation files that carry measured browser evidence and are compared separately.
pub const ATTESTATION_PATHS: [&str; 2] = [
    "intake/network-silence-summary.txt",
    "TRANSFER-STATEMENT.txt",
];

const SUMMARY_FIELD_NAMES: [&str; 12] = [
    "edition",
    "browser",
    "options",
    "pages loaded",
    "text-renamed originals loaded",
    "requests started by those loads",
    "requests to file: URLs",
    "requests to any other scheme or host",
    "self-test",
    "negative page, requests blocked",
    "negative page, requests reported",
    "verdict",
];

const README_TEMPLATE: &str = include_str!("../../docs/intake/edition-readme.txt");

const TRANSFER_TEMPLATE: &str = include_str!("../../docs/intake/transfer-statement.txt");
const AI_STATEMENT: &str = include_str!("../../docs/intake/ai-development-statement.md");
const DATA_HANDLING: &str = include_str!("../../docs/intake/data-handling.md");
const INTEGRITY_GUIDE: &str = include_str!("../../docs/intake/integrity-guide.md");
const NETWORK_SILENCE: &str = include_str!("../../docs/intake/network-silence.md");

/// Scriptable external stage runner for `edition`.
pub trait Stage {
    /// Run `git` with `args` in `root`, returning trimmed standard output.
    fn git(&mut self, root: &Path, args: &[&str]) -> Result<String, String>;

    /// Run `fieldbook check --strict --today <today>`.
    fn check(&mut self, root: &Path, today: &str) -> Result<RunOutput, String>;

    /// Run the site build with `SOURCE_DATE_EPOCH` and `FIELDBOOK_GIT_SHA`.
    fn build(&mut self, root: &Path, epoch: i64, sha: &str) -> Result<RunOutput, String>;

    /// Run the browser harness over `out/edition`.
    fn harness(&mut self, root: &Path) -> Result<RunOutput, String>;
}

/// The production [`Stage`] implementation executing actual commands.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemStage;

impl Stage for SystemStage {
    fn git(&mut self, root: &Path, args: &[&str]) -> Result<String, String> {
        let out = crate::proc::git(root, args)?;
        Ok(out.trim().to_owned())
    }

    fn check(&mut self, root: &Path, today: &str) -> Result<RunOutput, String> {
        let out = std::process::Command::new("cargo")
            .args([
                "run",
                "-q",
                "--locked",
                "-p",
                fieldbook_core::strings::binary(),
                "--",
                "check",
                "--strict",
                "--today",
                today,
            ])
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("cannot run cargo check: {e}"))?;
        Ok(RunOutput {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    fn build(&mut self, root: &Path, epoch: i64, sha: &str) -> Result<RunOutput, String> {
        let out = std::process::Command::new("cargo")
            .args([
                "run",
                "--locked",
                "-p",
                fieldbook_core::strings::binary(),
                "--",
                "build",
                "--out",
                "out",
            ])
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .env("SOURCE_DATE_EPOCH", epoch.to_string())
            .env("FIELDBOOK_GIT_SHA", sha)
            .output()
            .map_err(|e| format!("cannot run cargo build: {e}"))?;
        Ok(RunOutput {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }

    fn harness(&mut self, root: &Path) -> Result<RunOutput, String> {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot determine current executable: {e}"))?;
        let status = std::process::Command::new(exe)
            .arg("harness")
            .current_dir(root)
            .env("XTASK_REPO_ROOT", root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .status()
            .map_err(|e| format!("cannot run harness: {e}"))?;
        Ok(RunOutput {
            code: status.code().unwrap_or(-1),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }
}

/// Reason why an edition pipeline stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// A finding or refusal (exit code 1).
    Refused(String),
    /// An operation failed (exit code 3).
    Failed(String),
}

impl std::fmt::Display for Stop {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refused(msg) | Self::Failed(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Stop {}

/// The result of a successful edition packaging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Edition tag (`edition-NNN`).
    pub tag: String,
    /// Git commit SHA (40 lowercase hex characters).
    pub commit: String,
    /// Delivered files for `out/dist` in delivery order.
    pub files: Vec<(String, Vec<u8>)>,
    /// Summary size lines for the two archives.
    pub size_lines: Vec<String>,
}

/// Parsed `network-silence-summary.txt` evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    /// Microsoft Edge version string from the `browser:` line.
    pub edge_version: String,
    /// Number of pages loaded.
    pub pages: usize,
    /// Number of text-renamed originals loaded.
    pub text_originals: usize,
    /// Number of requests to any other scheme or host.
    pub other_requests: usize,
    /// Raw summary text.
    pub raw_text: String,
}

/// Format a file size line for the build summary.
pub fn size_line(name: &str, bytes: usize) -> String {
    const ONE_MB: usize = 1_000_000;
    if bytes < ONE_MB {
        format!("{name} <1 MB")
    } else {
        let rounded = bytes.saturating_add(ONE_MB / 2) / ONE_MB;
        format!("{name} {rounded} MB")
    }
}

fn is_lower_hex_40(s: &str) -> bool {
    s.len() == 40
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn civil_utc(epoch: i64) -> Result<fieldbook_core::audit::Now, Stop> {
    let secs = u64::try_from(epoch).map_err(|_| Stop::Failed("negative commit time".to_owned()))?;
    fieldbook_core::audit::Now::from_unix_secs(secs)
        .map_err(|e| Stop::Failed(format!("commit time out of range: {e}")))
}

fn read_disk_tree(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, Stop> {
    let meta = fs::symlink_metadata(dir)
        .map_err(|e| Stop::Failed(format!("cannot stat `{}`: {e}", dir.display())))?;
    if meta.file_type().is_symlink() {
        return Err(Stop::Refused(format!(
            "{}: error[FB0601]: staged root `{}` must not be a symlink",
            dir.display(),
            dir.display()
        )));
    }
    if !meta.is_dir() {
        return Err(Stop::Failed(format!(
            "staged root `{}` is not a directory",
            dir.display()
        )));
    }
    let mut map = BTreeMap::new();
    walk_disk_tree_rec(dir, "", &mut map)?;
    Ok(map)
}

fn walk_disk_tree_rec(
    cur: &Path,
    rel_prefix: &str,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), Stop> {
    let entries = fs::read_dir(cur)
        .map_err(|e| Stop::Failed(format!("cannot read dir `{}`: {e}", cur.display())))?;
    for entry in entries {
        let entry = entry
            .map_err(|e| Stop::Failed(format!("dir entry error in `{}`: {e}", cur.display())))?;
        let os_name = entry.file_name();
        let Some(name) = os_name.to_str() else {
            return Err(Stop::Failed(format!(
                "non-UTF-8 file name in `{}`",
                cur.display()
            )));
        };
        let rel = if rel_prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)
            .map_err(|e| Stop::Failed(format!("cannot stat `{}`: {e}", path.display())))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(Stop::Refused(format!(
                "{rel}: error[FB0601]: symlink `{rel}` is prohibited in tree"
            )));
        }
        if ft.is_dir() {
            walk_disk_tree_rec(&path, &rel, out)?;
        } else if ft.is_file() {
            let bytes = fs::read(&path)
                .map_err(|e| Stop::Failed(format!("cannot read `{}`: {e}", path.display())))?;
            out.insert(rel, bytes);
        } else {
            return Err(Stop::Refused(format!(
                "{rel}: error[FB0601]: special file `{rel}` is prohibited in tree"
            )));
        }
    }
    Ok(())
}

fn require_tree_unchanged(
    before: &BTreeMap<String, Vec<u8>>,
    after: &BTreeMap<String, Vec<u8>>,
) -> Result<(), Stop> {
    for (path, before_bytes) in before {
        let Some(after_bytes) = after.get(path) else {
            return Err(Stop::Refused(format!(
                "{path}: error[FB0608]: path `{path}` was removed after the harness measured it"
            )));
        };
        if before_bytes != after_bytes {
            return Err(Stop::Refused(format!(
                "{path}: error[FB0608]: path `{path}` changed after the harness measured it"
            )));
        }
    }
    for path in after.keys() {
        if !before.contains_key(path) {
            return Err(Stop::Refused(format!(
                "{path}: error[FB0608]: path `{path}` was added after the harness measured it"
            )));
        }
    }
    Ok(())
}

/// Parse and validate the twelve-line `network-silence-summary.txt`.
pub fn parse_summary(text: &str, tag: &str) -> Result<Summary, String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() != 12 {
        return Err(format!(
            "summary must contain exactly 12 lines, found {}",
            lines.len()
        ));
    }

    let mut fields: Vec<(&str, &str)> = Vec::with_capacity(12);
    for (i, line) in lines.iter().enumerate() {
        let expected_key = SUMMARY_FIELD_NAMES.get(i).copied().unwrap_or_default();
        // One key (`requests to file: URLs`) holds the separator itself, so the key is matched by
        // its expected text and not split off at the first `: `.
        let Some(val) = line
            .strip_prefix(expected_key)
            .and_then(|rest| rest.strip_prefix(": "))
        else {
            return Err(format!(
                "line {} `{line}` does not start with `{expected_key}: `",
                i + 1
            ));
        };
        let key = expected_key;
        fields.push((key, val));
    }

    let edition_val = fields.first().map(|(_, v)| *v).unwrap_or_default();
    if edition_val != tag {
        return Err(format!(
            "edition `{edition_val}` does not match expected tag `{tag}`"
        ));
    }

    let browser_val = fields.get(1).map(|(_, v)| *v).unwrap_or_default();
    let Some(edge_ver) = browser_val.strip_prefix("Microsoft Edge ") else {
        return Err(format!(
            "browser `{browser_val}` must start with `Microsoft Edge `"
        ));
    };
    let edge_version = edge_ver.trim();
    if edge_version.is_empty() {
        return Err("browser version must not be empty".to_owned());
    }

    let pages_str = fields.get(3).map(|(_, v)| *v).unwrap_or_default();
    let pages: usize = pages_str
        .parse()
        .map_err(|e| format!("cannot parse pages loaded `{pages_str}`: {e}"))?;

    let text_orig_str = fields.get(4).map(|(_, v)| *v).unwrap_or_default();
    let text_originals: usize = text_orig_str.parse().map_err(|e| {
        format!("cannot parse text-renamed originals loaded `{text_orig_str}`: {e}")
    })?;

    let started_str = fields.get(5).map(|(_, v)| *v).unwrap_or_default();
    let _started: usize = started_str
        .parse()
        .map_err(|e| format!("cannot parse requests started `{started_str}`: {e}"))?;

    let file_req_str = fields.get(6).map(|(_, v)| *v).unwrap_or_default();
    let _file_req: usize = file_req_str
        .parse()
        .map_err(|e| format!("cannot parse requests to file: URLs `{file_req_str}`: {e}"))?;

    let other_str = fields.get(7).map(|(_, v)| *v).unwrap_or_default();
    let other_requests: usize = other_str.parse().map_err(|e| {
        format!("cannot parse requests to any other scheme or host `{other_str}`: {e}")
    })?;
    if other_requests != 0 {
        return Err(format!(
            "requests to any other scheme or host must be 0, found {other_requests}"
        ));
    }

    let selftest_val = fields.get(8).map(|(_, v)| *v).unwrap_or_default();
    if selftest_val != "pass" {
        return Err(format!("self-test must be `pass`, found `{selftest_val}`"));
    }

    let blocked_str = fields.get(9).map(|(_, v)| *v).unwrap_or_default();
    let blocked: usize = blocked_str
        .parse()
        .map_err(|e| format!("cannot parse negative page requests blocked `{blocked_str}`: {e}"))?;
    if blocked < 1 {
        return Err(format!(
            "negative page, requests blocked must be >= 1, found {blocked}"
        ));
    }

    let reported_str = fields.get(10).map(|(_, v)| *v).unwrap_or_default();
    let reported: usize = reported_str.parse().map_err(|e| {
        format!("cannot parse negative page requests reported `{reported_str}`: {e}")
    })?;
    if reported < 1 {
        return Err(format!(
            "negative page, requests reported must be >= 1, found {reported}"
        ));
    }

    let verdict_val = fields.get(11).map(|(_, v)| *v).unwrap_or_default();
    if verdict_val != "pass" {
        return Err(format!("verdict must be `pass`, found `{verdict_val}`"));
    }

    Ok(Summary {
        edge_version: edge_version.to_owned(),
        pages,
        text_originals,
        other_requests,
        raw_text: text.to_owned(),
    })
}

fn classify_extension(path: &str) -> &'static str {
    if path.rsplit('/').next() == Some("SHA256SUMS") {
        return "other";
    }
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "html",
        "js" => "js",
        "css" => "css",
        "csv" => "csv",
        "xlsx" => "xlsx",
        "md" | "toml" => "md_toml",
        "txt" => "txt",
        "pdf" => "pdf",
        _ => "other",
    }
}

fn stage_corpus_and_companions(
    root: &Path,
    now: fieldbook_core::audit::Now,
    commit: &str,
    measured_site: &BTreeMap<String, Vec<u8>>,
    summary_bytes: Vec<u8>,
) -> Result<BTreeMap<String, Vec<u8>>, Stop> {
    let tree = fieldbook::io::tree::read_tree(root)
        .map_err(|e| Stop::Failed(format!("cannot read the Corpus: {e}")))?;
    let (model, report) = fieldbook_core::check::load(
        &fieldbook_core::check::Host::new(tree, now.date),
        &fieldbook_core::check::Options::default(),
    );
    if report.errors() != 0 {
        return Err(Stop::Refused(format!(
            "the Corpus does not load cleanly: {} error(s)",
            report.errors()
        )));
    }

    let eff = fieldbook_core::policy::EffectivePolicy::floor(false);
    let fieldbook_core::egress::Decision::Allowed(allowed) = fieldbook_core::egress::check(
        &eff,
        &fieldbook_core::egress::Dest::Edition,
        None,
        None,
        now,
        fieldbook_core::egress::Shipped::Never,
    ) else {
        return Err(Stop::Failed(
            "the edition destination refused the Corpus view".to_owned(),
        ));
    };
    let view = fieldbook_core::egress::LabelledView::new(
        &model,
        &allowed,
        &fieldbook_core::authz::ReadableStores::corpus(),
    );

    let mut quarantined = BTreeSet::new();
    for s in view.sources() {
        if s.value.lifecycle == Lifecycle::Quarantined {
            let vid_str = format!("{}-v{}", s.value.source_id, s.value.version);
            let vid = VersionId::parse(&vid_str)
                .map_err(|e| Stop::Failed(format!("bad version id: {e}")))?;
            quarantined.insert(vid);
        }
    }

    let shipped = shipped_corpus(&view, &quarantined)
        .map_err(|e| Stop::Failed(format!("shipped Corpus: {e}")))?;

    let mut edition = measured_site.clone();
    let view_tree = view.tree();
    for (path, _) in &shipped {
        if edition.contains_key(path) {
            return Err(Stop::Refused(format!(
                "{path}: error[FB0601]: path collision between site file and shipped corpus file `{path}`"
            )));
        }
        let bytes = view_tree
            .get_bytes(path)
            .ok_or_else(|| Stop::Failed(format!("shipped file `{path}` has no bytes")))?;
        edition.insert(path.clone(), bytes.to_vec());
    }

    let assistant_name = fieldbook_core::strings::paths()[6];
    let assistant_corpus_path = format!("corpus/{assistant_name}");
    let assistant_bytes = view_tree.get_bytes(&assistant_corpus_path).ok_or_else(|| {
        Stop::Refused(format!(
            "{assistant_name}: error[FB0601]: {assistant_corpus_path} is missing"
        ))
    })?;
    edition.insert(assistant_name.to_owned(), assistant_bytes.to_vec());

    edition.insert(
        "intake/network-silence-summary.txt".to_owned(),
        summary_bytes,
    );

    let catalogue = SupportCatalogue::from_public_source(&view, commit)
        .map_err(|e| Stop::Failed(format!("support catalogue: {e}")))?;
    let (support_toml, support_hex) = catalogue.to_toml_bytes_and_sha256();
    verify_support_catalogue_bytes(&support_toml, &support_hex)
        .map_err(|e| Stop::Failed(format!("support catalogue verification: {e}")))?;
    edition.insert("evidence/corpus-support.toml".to_owned(), support_toml);

    edition.insert("README.txt".to_owned(), Vec::new());
    edition.insert("TRANSFER-STATEMENT.txt".to_owned(), Vec::new());
    edition.insert("intake/ai-development-statement.md".to_owned(), Vec::new());
    edition.insert("intake/data-handling.md".to_owned(), Vec::new());
    edition.insert("intake/integrity-guide.md".to_owned(), Vec::new());
    edition.insert("intake/network-silence.md".to_owned(), Vec::new());

    Ok(edition)
}

struct IntakeInputs<'a> {
    tag: &'a str,
    edition_number: &'a str,
    commit: &'a str,
    today: &'a str,
    contact: &'a str,
    summary: &'a Summary,
}

fn fill_intake_templates(
    edition: &mut BTreeMap<String, Vec<u8>>,
    measured_originals: &BTreeMap<String, Vec<u8>>,
    inputs: &IntakeInputs<'_>,
) -> Result<(), Stop> {
    let edition_toml_bytes = edition.get("edition.toml").ok_or_else(|| {
        Stop::Refused(
            "edition.toml: error[FB0601]: staged site is missing `edition.toml`".to_owned(),
        )
    })?;
    let edition_toml_str = std::str::from_utf8(edition_toml_bytes)
        .map_err(|e| Stop::Refused(format!("edition.toml: error[FB0608]: invalid UTF-8: {e}")))?;
    let info = EditionInfo::parse(edition_toml_str).map_err(|e| {
        Stop::Refused(format!(
            "edition.toml: error[FB0608]: cannot parse edition.toml: {e}"
        ))
    })?;

    if info.edition != inputs.tag || info.commit != inputs.commit {
        return Err(Stop::Refused(format!(
            "edition.toml: error[FB0608]: identity mismatch: edition `{}` (expected `{}`), commit `{}` (expected `{}`)",
            info.edition, inputs.tag, info.commit, inputs.commit
        )));
    }

    let n_files_edition = edition.len() + 1;
    let n_files_originals = measured_originals.len() + 1;

    let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
    for path in edition.keys() {
        *counts.entry(classify_extension(path)).or_default() += 1;
    }
    *counts.entry(classify_extension("SHA256SUMS")).or_default() += 1;

    for path in measured_originals.keys() {
        *counts.entry(classify_extension(path)).or_default() += 1;
    }
    *counts.entry(classify_extension("SHA256SUMS")).or_default() += 1;

    let total_counted: usize = counts.values().sum();
    if total_counted != n_files_edition + n_files_originals {
        return Err(Stop::Failed(format!(
            "internal count mismatch: classified {total_counted}, expected {}",
            n_files_edition + n_files_originals
        )));
    }

    let mut values: BTreeMap<&str, String> = BTreeMap::new();
    values.insert("edition", inputs.tag.to_owned());
    values.insert("edition_number", inputs.edition_number.to_owned());
    values.insert("tool_version", info.tool_version.clone());
    values.insert("commit", inputs.commit.to_owned());
    values.insert("schema_version", info.schema_version.to_string());
    values.insert("corpus_fingerprint", info.fingerprint.clone());
    values.insert("source_date", inputs.today.to_owned());
    values.insert("n_pages", info.counts.pages.to_string());
    values.insert("n_sources", info.counts.source_versions.to_string());
    values.insert("n_claims", info.counts.claims.to_string());
    values.insert("n_questions", info.counts.questions.to_string());
    values.insert("security_contact", inputs.contact.to_owned());

    values.insert("edge_version", inputs.summary.edge_version.clone());
    values.insert("n_pages_checked", inputs.summary.pages.to_string());
    values.insert(
        "n_text_originals_checked",
        inputs.summary.text_originals.to_string(),
    );
    values.insert(
        "n_nonlocal_requests",
        inputs.summary.other_requests.to_string(),
    );

    values.insert("n_files_edition", n_files_edition.to_string());
    values.insert("n_files_originals", n_files_originals.to_string());
    values.insert(
        "n_html",
        counts.get("html").copied().unwrap_or(0).to_string(),
    );
    values.insert("n_js", counts.get("js").copied().unwrap_or(0).to_string());
    values.insert("n_css", counts.get("css").copied().unwrap_or(0).to_string());
    values.insert("n_csv", counts.get("csv").copied().unwrap_or(0).to_string());
    values.insert(
        "n_xlsx",
        counts.get("xlsx").copied().unwrap_or(0).to_string(),
    );
    values.insert(
        "n_md_toml",
        counts.get("md_toml").copied().unwrap_or(0).to_string(),
    );
    values.insert("n_txt", counts.get("txt").copied().unwrap_or(0).to_string());
    values.insert("n_pdf", counts.get("pdf").copied().unwrap_or(0).to_string());
    values.insert(
        "n_other",
        counts.get("other").copied().unwrap_or(0).to_string(),
    );

    let readme =
        fill_placeholders("README.txt", README_TEMPLATE, &values).map_err(Stop::Refused)?;
    let transfer = fill_placeholders("TRANSFER-STATEMENT.txt", TRANSFER_TEMPLATE, &values)
        .map_err(Stop::Refused)?;
    let ai = fill_placeholders("intake/ai-development-statement.md", AI_STATEMENT, &values)
        .map_err(Stop::Refused)?;
    let data = fill_placeholders("intake/data-handling.md", DATA_HANDLING, &values)
        .map_err(Stop::Refused)?;
    let integrity = fill_placeholders("intake/integrity-guide.md", INTEGRITY_GUIDE, &values)
        .map_err(Stop::Refused)?;
    let network = fill_placeholders("intake/network-silence.md", NETWORK_SILENCE, &values)
        .map_err(Stop::Refused)?;

    edition.insert("README.txt".to_owned(), readme.into_bytes());
    edition.insert("TRANSFER-STATEMENT.txt".to_owned(), transfer.into_bytes());
    edition.insert(
        "intake/ai-development-statement.md".to_owned(),
        ai.into_bytes(),
    );
    edition.insert("intake/data-handling.md".to_owned(), data.into_bytes());
    edition.insert(
        "intake/integrity-guide.md".to_owned(),
        integrity.into_bytes(),
    );
    edition.insert("intake/network-silence.md".to_owned(), network.into_bytes());

    Ok(())
}

fn publish_dist(
    root: &Path,
    tag: String,
    commit: String,
    files: Vec<(String, Vec<u8>)>,
    size_lines: Vec<String>,
) -> Result<Package, Stop> {
    let dist_dir = root.join("out/dist");
    let staging_dir = root.join("out/dist.staging");
    if staging_dir.exists() {
        fs::remove_dir_all(&staging_dir)
            .map_err(|e| Stop::Failed(format!("cannot remove staging dir: {e}")))?;
    }
    fs::create_dir_all(&staging_dir)
        .map_err(|e| Stop::Failed(format!("cannot create staging dir: {e}")))?;

    let write_res = (|| -> Result<(), Stop> {
        for (name, bytes) in &files {
            fs::write(staging_dir.join(name), bytes)
                .map_err(|e| Stop::Failed(format!("cannot write staged file `{name}`: {e}")))?;
        }
        for (name, bytes) in &files {
            let read_back = fs::read(staging_dir.join(name))
                .map_err(|e| Stop::Failed(format!("cannot reread staged file `{name}`: {e}")))?;
            if &read_back != bytes {
                return Err(Stop::Failed(format!(
                    "staged `{name}` does not match memory bytes"
                )));
            }
        }
        if dist_dir.exists() {
            fs::remove_dir_all(&dist_dir)
                .map_err(|e| Stop::Failed(format!("cannot remove old out/dist: {e}")))?;
        }
        fs::rename(&staging_dir, &dist_dir)
            .map_err(|e| Stop::Failed(format!("cannot rename staging to out/dist: {e}")))?;
        Ok(())
    })();

    if let Err(e) = &write_res {
        if staging_dir.exists() {
            let _ = fs::remove_dir_all(&staging_dir);
        }
        return Err(e.clone());
    }

    Ok(Package {
        tag,
        commit,
        files,
        size_lines,
    })
}

fn run_output_message(out: &RunOutput) -> String {
    let mut msg = String::new();
    if !out.stdout.is_empty() {
        msg.push_str(&String::from_utf8_lossy(&out.stdout));
    }
    if !out.stderr.is_empty() {
        if !msg.is_empty() {
            msg.push('\n');
        }
        msg.push_str(&String::from_utf8_lossy(&out.stderr));
    }
    msg
}

/// Package the rendered site edition of the clean commit at `root`.
pub fn edition(root: &Path, stage: &mut dyn Stage, contact: Option<&str>) -> Result<Package, Stop> {
    // 5a. Refuse a dirty tree.
    require_clean_tree(&mut |args| stage.git(root, args)).map_err(Stop::Refused)?;

    // 5b. Git rev-parse HEAD, commit epoch, today.
    let commit = stage
        .git(root, &["rev-parse", "HEAD"])
        .map_err(Stop::Failed)?;
    if !is_lower_hex_40(&commit) {
        return Err(Stop::Failed(format!(
            "`git rev-parse HEAD` returned `{commit}`"
        )));
    }
    let epoch_str = stage
        .git(root, &["log", "-1", "--format=%ct", "HEAD"])
        .map_err(Stop::Failed)?;
    let epoch: i64 = epoch_str
        .parse()
        .map_err(|e| Stop::Failed(format!("cannot parse commit epoch `{epoch_str}`: {e}")))?;
    let now = civil_utc(epoch)?;
    let today = now.date.to_string();

    // 5c. Edition id from newest heading.
    let changes = fs::read_to_string(root.join("corpus/changes.md"))
        .map_err(|e| Stop::Failed(format!("cannot read corpus/changes.md: {e}")))?;
    let heading = latest_edition_heading(&changes).ok_or_else(|| {
        Stop::Refused("corpus/changes.md has no `## edition-NNN` heading".to_owned())
    })?;
    let edition_id = EditionId::parse(heading)
        .map_err(|e| Stop::Refused(format!("bad edition heading: {e}")))?;
    let tag = edition_id.as_str().to_owned();
    let edition_number = format!("{:03}", edition_id.number());

    // 5d. The security contact.
    let contact = contact
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            Stop::Refused(format!(
                "IN-01: {SECURITY_CONTACT_ENV} is missing or blank; set it to the security contact text"
            ))
        })?;
    if contact.chars().any(char::is_control) {
        return Err(Stop::Refused(format!(
            "IN-01: {SECURITY_CONTACT_ENV} must not contain control characters"
        )));
    }

    // 5e. Run check.
    let check_out = stage.check(root, &today).map_err(Stop::Failed)?;
    if check_out.code == 1 {
        let msg = run_output_message(&check_out);
        return Err(Stop::Refused(
            format!("check failed (exit 1):\n{msg}").trim().to_owned(),
        ));
    } else if check_out.code != 0 {
        return Err(Stop::Failed(format!(
            "check failed with exit code {}",
            check_out.code
        )));
    }

    // 5f. Remove old harness summary, then run build.
    let summary_file = root.join("out/harness/network-silence-summary.txt");
    match fs::remove_file(&summary_file) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(Stop::Failed(format!(
                "cannot remove old harness summary: {e}"
            )));
        }
    }

    let build_out = stage.build(root, epoch, &commit).map_err(Stop::Failed)?;
    if build_out.code == 1 {
        let msg = run_output_message(&build_out);
        return Err(Stop::Refused(
            format!("build failed (exit 1):\n{msg}").trim().to_owned(),
        ));
    } else if build_out.code != 0 {
        return Err(Stop::Failed(format!(
            "build failed with exit code {}",
            build_out.code
        )));
    }

    // 5g. Read trees, run harness, require trees unchanged.
    let measured_site = read_disk_tree(&root.join("out/edition"))?;
    let measured_originals = read_disk_tree(&root.join("out/originals"))?;

    let harness_out = stage.harness(root).map_err(Stop::Failed)?;
    if harness_out.code == 1 {
        let msg = run_output_message(&harness_out);
        let trimmed = msg.trim();
        let refusal = if trimmed.is_empty() {
            "harness reported findings (exit 1)".to_owned()
        } else {
            format!("harness failed (exit 1):\n{trimmed}")
        };
        return Err(Stop::Refused(refusal));
    } else if harness_out.code != 0 {
        return Err(Stop::Failed(format!(
            "harness failed with exit code {}",
            harness_out.code
        )));
    }

    let after_site = read_disk_tree(&root.join("out/edition"))?;
    let after_originals = read_disk_tree(&root.join("out/originals"))?;
    require_tree_unchanged(&measured_site, &after_site)?;
    require_tree_unchanged(&measured_originals, &after_originals)?;

    // 5h. Read and parse summary.
    let summary_bytes = match fs::read(&summary_file) {
        Ok(b) => b,
        Err(e) => {
            return Err(Stop::Refused(format!(
                "IN-01: out/harness/network-silence-summary.txt is missing or unreadable ({e}); the harness writes it only for a passing Microsoft Edge run over out/edition"
            )));
        }
    };
    let summary_text = match std::str::from_utf8(&summary_bytes) {
        Ok(s) => s,
        Err(e) => {
            return Err(Stop::Refused(format!(
                "IN-01: out/harness/network-silence-summary.txt is not valid UTF-8 ({e}); the harness writes it only for a passing Microsoft Edge run over out/edition"
            )));
        }
    };

    let summary = parse_summary(summary_text, &tag).map_err(|e| {
        Stop::Refused(format!(
            "IN-01: out/harness/network-silence-summary.txt is invalid: {e}"
        ))
    })?;

    let expected_html_pages = measured_site
        .keys()
        .filter(|p| p.ends_with(".html"))
        .count();
    if summary.pages != expected_html_pages {
        return Err(Stop::Refused(format!(
            "IN-01: out/harness/network-silence-summary.txt pages loaded ({}) does not match measured site .html count ({expected_html_pages})",
            summary.pages
        )));
    }

    let expected_text_renamed = measured_originals
        .keys()
        .filter(|p| {
            p.ends_with(".html.txt")
                || p.ends_with(".htm.txt")
                || p.ends_with(".xhtml.txt")
                || p.ends_with(".svg.txt")
        })
        .count();
    if summary.text_originals != expected_text_renamed {
        return Err(Stop::Refused(format!(
            "IN-01: out/harness/network-silence-summary.txt text-renamed originals loaded ({}) does not match measured originals count ({expected_text_renamed})",
            summary.text_originals
        )));
    }

    // 5i. Load Corpus and stage edition files.
    let mut edition =
        stage_corpus_and_companions(root, now, &commit, &measured_site, summary_bytes)?;

    // 5j. Fill placeholders.
    let inputs = IntakeInputs {
        tag: &tag,
        edition_number: &edition_number,
        commit: &commit,
        today: &today,
        contact,
        summary: &summary,
    };
    fill_intake_templates(&mut edition, &measured_originals, &inputs)?;

    // 5k. Internal checksums and allowlist verification.
    let edition_sums = sums_of(&edition);
    edition.insert("SHA256SUMS".to_owned(), edition_sums.into_bytes());

    let mut originals = measured_originals;
    let originals_sums = sums_of(&originals);
    originals.insert("SHA256SUMS".to_owned(), originals_sums.into_bytes());

    verify_staged_maps(AllowlistProfile::Site, &edition, &originals, REQUIRED_FILES)
        .map_err(|v| Stop::Refused(v.to_string()))?;

    // 5l. Write deterministic zips, reopen, verify four-way equality.
    let edition_zip =
        write_deterministic_zip_bytes("edition", epoch, &edition).map_err(Stop::Failed)?;
    let originals_zip =
        write_deterministic_zip_bytes("originals", epoch, &originals).map_err(Stop::Failed)?;

    let re_edition: BTreeMap<String, Vec<u8>> =
        read_and_verify_deterministic_zip(&edition_zip, "edition", Some(epoch))
            .map_err(Stop::Failed)?
            .into_iter()
            .collect();
    let re_originals: BTreeMap<String, Vec<u8>> =
        read_and_verify_deterministic_zip(&originals_zip, "originals", Some(epoch))
            .map_err(Stop::Failed)?
            .into_iter()
            .collect();

    if re_edition != edition || re_originals != originals {
        return Err(Stop::Refused(
            "evidence/corpus-support.toml: error[FB0608]: reopened archives differ from staged trees".to_owned(),
        ));
    }

    let staged_support = re_edition
        .get("evidence/corpus-support.toml")
        .ok_or_else(|| {
            Stop::Refused(
                "evidence/corpus-support.toml: error[FB0601]: staged catalogue is missing"
                    .to_owned(),
            )
        })?
        .clone();

    let support_toml_bytes = staged_support.clone();
    let staged_support_sha = sha256_hex(&staged_support);
    let support_sha_line = write_sums(&[("corpus-support.toml".to_owned(), staged_support_sha)]);
    let support_sha_bytes = support_sha_line.into_bytes();

    // 5m. Outer SHA256SUMS.
    let edition_zip_name = format!("fieldbook-edition-{edition_number}.zip");
    let originals_zip_name = format!("fieldbook-originals-{edition_number}.zip");
    let outer_sums = write_sums(&[
        (edition_zip_name.clone(), sha256_hex(&edition_zip)),
        (originals_zip_name.clone(), sha256_hex(&originals_zip)),
    ])
    .into_bytes();

    let files = vec![
        (edition_zip_name.clone(), edition_zip.clone()),
        (originals_zip_name.clone(), originals_zip.clone()),
        ("SHA256SUMS".to_owned(), outer_sums),
        ("corpus-support.toml".to_owned(), support_toml_bytes),
        ("corpus-support.sha256".to_owned(), support_sha_bytes),
    ];

    let delivery = delivery_from_files(&files)?;
    verify_delivery(&delivery)?;

    let size_lines = vec![
        size_line(&edition_zip_name, edition_zip.len()),
        size_line(&originals_zip_name, originals_zip.len()),
    ];

    // 5n. Publish to out/dist.
    publish_dist(root, tag, commit, files, size_lines)
}

/// Command handler for `cargo xtask edition`.
pub fn command_edition(root: &Path, args: &[&str]) -> Result<bool, String> {
    if !args.is_empty() {
        return Err("usage: cargo xtask edition".to_owned());
    }
    let contact = std::env::var(SECURITY_CONTACT_ENV).ok();
    let mut stage = SystemStage;
    match edition(root, &mut stage, contact.as_deref()) {
        Ok(package) => {
            println!("edition {} at commit {}", package.tag, package.commit);
            for line in &package.size_lines {
                println!("{line}");
            }
            Ok(true)
        }
        Err(Stop::Refused(msg)) => {
            eprintln!("error: {msg}");
            Ok(false)
        }
        Err(Stop::Failed(msg)) => Err(msg),
    }
}

/// A verified delivery set holding edition and originals archives, metadata, and reopened trees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// File name of the edition zip archive (`fieldbook-edition-NNN.zip`).
    pub edition_name: String,
    /// File name of the originals zip archive (`fieldbook-originals-NNN.zip`).
    pub originals_name: String,
    /// Raw bytes of the delivered edition zip archive.
    pub edition_zip: Vec<u8>,
    /// Raw bytes of the delivered originals zip archive.
    pub originals_zip: Vec<u8>,
    /// Raw bytes of the outer `SHA256SUMS` file.
    pub outer_sums: Vec<u8>,
    /// Raw bytes of the delivered `corpus-support.toml`.
    pub support_toml: Vec<u8>,
    /// Raw bytes of the delivered `corpus-support.sha256` sidecar.
    pub support_sha: Vec<u8>,
    /// Reopened edition tree entries without the top-level `edition/` prefix.
    pub edition: BTreeMap<String, Vec<u8>>,
    /// Reopened originals tree entries without the top-level `originals/` prefix.
    pub originals: BTreeMap<String, Vec<u8>>,
}

/// Construct a verified [`Delivery`] set from the five raw delivery files.
pub fn delivery_from_files(files: &[(String, Vec<u8>)]) -> Result<Delivery, Stop> {
    let mut seen = BTreeSet::new();
    let mut edition_zip: Option<(&str, &[u8], &str)> = None;
    let mut originals_zip: Option<(&str, &[u8], &str)> = None;
    let mut outer_sums: Option<&[u8]> = None;
    let mut support_toml: Option<&[u8]> = None;
    let mut support_sha: Option<&[u8]> = None;

    for (name, bytes) in files {
        if !seen.insert(name.as_str()) {
            return Err(Stop::Refused(format!(
                "duplicate file `{name}` in delivery"
            )));
        }
        if let Some(rest) = name.strip_prefix("fieldbook-edition-")
            && let Some(nnn) = rest.strip_suffix(".zip")
            && nnn.len() == 3
            && nnn.bytes().all(|b| b.is_ascii_digit())
        {
            if edition_zip.is_some() {
                return Err(Stop::Refused(format!(
                    "multiple edition zips found: `{name}`"
                )));
            }
            edition_zip = Some((name.as_str(), bytes.as_slice(), nnn));
            continue;
        }
        if let Some(rest) = name.strip_prefix("fieldbook-originals-")
            && let Some(nnn) = rest.strip_suffix(".zip")
            && nnn.len() == 3
            && nnn.bytes().all(|b| b.is_ascii_digit())
        {
            if originals_zip.is_some() {
                return Err(Stop::Refused(format!(
                    "multiple originals zips found: `{name}`"
                )));
            }
            originals_zip = Some((name.as_str(), bytes.as_slice(), nnn));
            continue;
        }
        if name == "SHA256SUMS" {
            outer_sums = Some(bytes.as_slice());
        } else if name == "corpus-support.toml" {
            support_toml = Some(bytes.as_slice());
        } else if name == "corpus-support.sha256" {
            support_sha = Some(bytes.as_slice());
        } else {
            return Err(Stop::Refused(format!(
                "unexpected file `{name}` in delivery"
            )));
        }
    }

    let Some((edition_name, edition_bytes, edition_nnn)) = edition_zip else {
        return Err(Stop::Refused(
            "missing edition zip (fieldbook-edition-NNN.zip) in delivery".to_owned(),
        ));
    };
    let Some((originals_name, originals_bytes, originals_nnn)) = originals_zip else {
        return Err(Stop::Refused(
            "missing originals zip (fieldbook-originals-NNN.zip) in delivery".to_owned(),
        ));
    };
    if edition_nnn != originals_nnn {
        return Err(Stop::Refused(format!(
            "edition zip number `{edition_nnn}` does not match originals zip number `{originals_nnn}`"
        )));
    }
    let Some(outer_sums_bytes) = outer_sums else {
        return Err(Stop::Refused("missing `SHA256SUMS` in delivery".to_owned()));
    };
    let Some(support_toml_bytes) = support_toml else {
        return Err(Stop::Refused(
            "missing `corpus-support.toml` in delivery".to_owned(),
        ));
    };
    let Some(support_sha_bytes) = support_sha else {
        return Err(Stop::Refused(
            "missing `corpus-support.sha256` in delivery".to_owned(),
        ));
    };

    let edition: BTreeMap<String, Vec<u8>> =
        read_and_verify_deterministic_zip(edition_bytes, "edition", None)
            .map_err(|e| Stop::Refused(format!("{edition_name}: error[FB0608]: {e}")))?
            .into_iter()
            .collect();
    let originals: BTreeMap<String, Vec<u8>> =
        read_and_verify_deterministic_zip(originals_bytes, "originals", None)
            .map_err(|e| Stop::Refused(format!("{originals_name}: error[FB0608]: {e}")))?
            .into_iter()
            .collect();

    Ok(Delivery {
        edition_name: edition_name.to_owned(),
        originals_name: originals_name.to_owned(),
        edition_zip: edition_bytes.to_vec(),
        originals_zip: originals_bytes.to_vec(),
        outer_sums: outer_sums_bytes.to_vec(),
        support_toml: support_toml_bytes.to_vec(),
        support_sha: support_sha_bytes.to_vec(),
        edition,
        originals,
    })
}

/// Load regular delivery files from `dir` and unpack them into a [`Delivery`].
pub fn load_delivery(dir: &Path) -> Result<Delivery, Stop> {
    let meta = fs::symlink_metadata(dir).map_err(|e| {
        Stop::Failed(format!(
            "cannot stat delivery directory `{}`: {e}",
            dir.display()
        ))
    })?;
    if meta.file_type().is_symlink() {
        return Err(Stop::Refused(format!(
            "{}: error[FB0601]: delivery directory must not be a symlink",
            dir.display()
        )));
    }
    if !meta.is_dir() {
        return Err(Stop::Failed(format!(
            "delivery directory `{}` is not a directory",
            dir.display()
        )));
    }

    let entries = fs::read_dir(dir).map_err(|e| {
        Stop::Failed(format!(
            "cannot read delivery directory `{}`: {e}",
            dir.display()
        ))
    })?;

    let mut files = Vec::new();
    for entry in entries {
        let entry = entry
            .map_err(|e| Stop::Failed(format!("cannot read entry in `{}`: {e}", dir.display())))?;
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            return Err(Stop::Failed(format!(
                "non-UTF-8 filename in `{}`",
                dir.display()
            )));
        };
        let path = entry.path();
        let emeta = fs::symlink_metadata(&path)
            .map_err(|e| Stop::Failed(format!("cannot stat `{}`: {e}", path.display())))?;
        let ft = emeta.file_type();
        if ft.is_symlink() {
            return Err(Stop::Refused(format!(
                "{name}: error[FB0601]: symlink `{name}` is prohibited in delivery"
            )));
        }
        if ft.is_dir() {
            return Err(Stop::Refused(format!(
                "{name}: error[FB0601]: subfolder `{name}` is prohibited in delivery"
            )));
        }
        if !ft.is_file() {
            return Err(Stop::Refused(format!(
                "{name}: error[FB0601]: special file `{name}` is prohibited in delivery"
            )));
        }
        let bytes = fs::read(&path)
            .map_err(|e| Stop::Failed(format!("cannot read `{}`: {e}", path.display())))?;
        files.push((name.to_owned(), bytes));
    }
    files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));

    delivery_from_files(&files)
}

/// Verify the delivery package against the Site allowlist profile, outer sums, and support catalogue.
pub fn verify_delivery(d: &Delivery) -> Result<(), Stop> {
    // (a) Outer SHA256SUMS parsing and validation.
    let sums_str = std::str::from_utf8(&d.outer_sums)
        .map_err(|e| Stop::Refused(format!("SHA256SUMS: error[FB0608]: invalid UTF-8: {e}")))?;
    let parsed =
        parse_sha256sums(sums_str, "SHA256SUMS").map_err(|v| Stop::Refused(v.to_string()))?;

    if parsed.len() != 2 {
        return Err(Stop::Refused(format!(
            "SHA256SUMS: error[FB0608]: outer SHA256SUMS must contain exactly two entries, found {}",
            parsed.len()
        )));
    }

    let expected_edition_sha = sha256_hex(&d.edition_zip);
    let actual_edition_sha = parsed.get(&d.edition_name).ok_or_else(|| {
        Stop::Refused(format!(
            "SHA256SUMS: error[FB0608]: outer SHA256SUMS is missing entry for `{}`",
            d.edition_name
        ))
    })?;
    if actual_edition_sha != &expected_edition_sha {
        return Err(Stop::Refused(format!(
            "{}: error[FB0608]: checksum in outer SHA256SUMS differs from archive bytes",
            d.edition_name
        )));
    }

    let expected_orig_sha = sha256_hex(&d.originals_zip);
    let actual_orig_sha = parsed.get(&d.originals_name).ok_or_else(|| {
        Stop::Refused(format!(
            "SHA256SUMS: error[FB0608]: outer SHA256SUMS is missing entry for `{}`",
            d.originals_name
        ))
    })?;
    if actual_orig_sha != &expected_orig_sha {
        return Err(Stop::Refused(format!(
            "{}: error[FB0608]: checksum in outer SHA256SUMS differs from archive bytes",
            d.originals_name
        )));
    }

    // (b) Allowlist Site profile verification over full internal manifests.
    verify_staged_maps(
        AllowlistProfile::Site,
        &d.edition,
        &d.originals,
        REQUIRED_FILES,
    )
    .map_err(|v| Stop::Refused(v.to_string()))?;

    // (c) Support catalogue and sidecar verification.
    let staged_support = d.edition.get("evidence/corpus-support.toml").ok_or_else(|| {
        Stop::Refused(
            "evidence/corpus-support.toml: error[FB0608]: missing evidence/corpus-support.toml in edition".to_owned(),
        )
    })?;
    if staged_support != &d.support_toml {
        return Err(Stop::Refused(
            "evidence/corpus-support.toml: error[FB0608]: staged catalogue differs from delivered corpus-support.toml".to_owned(),
        ));
    }
    let staged_support_sha = sha256_hex(staged_support);
    let expected_support_sha_line =
        write_sums(&[("corpus-support.toml".to_owned(), staged_support_sha.clone())]);
    let expected_sidecar = SupportCatalogue::sidecar_sha256_line(staged_support);

    if d.support_sha != expected_support_sha_line.as_bytes()
        || d.support_sha != expected_sidecar.as_bytes()
    {
        return Err(Stop::Refused(
            "evidence/corpus-support.toml: error[FB0608]: sidecar sha256 line mismatch".to_owned(),
        ));
    }

    let edition_sums_bytes = d.edition.get("SHA256SUMS").ok_or_else(|| {
        Stop::Refused(
            "SHA256SUMS: error[FB0608]: missing internal SHA256SUMS in edition".to_owned(),
        )
    })?;
    let edition_sums_str = std::str::from_utf8(edition_sums_bytes).map_err(|e| {
        Stop::Refused(format!(
            "SHA256SUMS: error[FB0608]: invalid UTF-8 in internal SHA256SUMS: {e}"
        ))
    })?;

    let internal_entry_sha = edition_sums_str
        .lines()
        .find_map(|line| {
            let (sha, path) = line.split_once("  ")?;
            if path == "evidence/corpus-support.toml" {
                Some(sha)
            } else {
                None
            }
        })
        .ok_or_else(|| {
            Stop::Refused(
                "evidence/corpus-support.toml: error[FB0608]: missing SHA256SUMS entry for evidence/corpus-support.toml".to_owned(),
            )
        })?;

    if internal_entry_sha != staged_support_sha {
        return Err(Stop::Refused(
            "evidence/corpus-support.toml: error[FB0608]: internal SHA256SUMS entry does not match catalogue digest".to_owned(),
        ));
    }

    Ok(())
}

/// Compare two delivery packages, verifying each first and requiring matching deterministic payloads.
pub fn compare_payload(left: &Delivery, right: &Delivery) -> Result<(), Stop> {
    // 1. First verify each delivery package.
    verify_delivery(left).map_err(|e| match e {
        Stop::Refused(m) => Stop::Refused(format!("left: {m}")),
        Stop::Failed(m) => Stop::Failed(format!("left: {m}")),
    })?;
    verify_delivery(right).map_err(|e| match e {
        Stop::Refused(m) => Stop::Refused(format!("right: {m}")),
        Stop::Failed(m) => Stop::Failed(format!("right: {m}")),
    })?;

    // 2. Explicit presence of attestation paths.
    for &att in &ATTESTATION_PATHS {
        if !left.edition.contains_key(att) {
            return Err(Stop::Refused(format!(
                "{att}: error[FB0608]: {att} is missing from the left delivery"
            )));
        }
        if !right.edition.contains_key(att) {
            return Err(Stop::Refused(format!(
                "{att}: error[FB0608]: {att} is missing from the right delivery"
            )));
        }
    }

    // 3. Edition path set and payload comparison (excluding attestations and SHA256SUMS).
    for (path, left_bytes) in &left.edition {
        let Some(right_bytes) = right.edition.get(path) else {
            return Err(Stop::Refused(format!(
                "{path}: error[FB0608]: {path} is missing from the right delivery"
            )));
        };
        if !ATTESTATION_PATHS.contains(&path.as_str())
            && path != "SHA256SUMS"
            && left_bytes != right_bytes
        {
            return Err(Stop::Refused(format!(
                "{path}: error[FB0608]: {path} differs between the deliveries"
            )));
        }
    }
    for path in right.edition.keys() {
        if !left.edition.contains_key(path) {
            return Err(Stop::Refused(format!(
                "{path}: error[FB0608]: {path} is extra in the right delivery"
            )));
        }
    }

    // 4. Edition SHA256SUMS comparison (excluding attestation lines).
    let left_sums_bytes = left.edition.get("SHA256SUMS").ok_or_else(|| {
        Stop::Refused("SHA256SUMS: error[FB0608]: missing SHA256SUMS in left edition".to_owned())
    })?;
    let right_sums_bytes = right.edition.get("SHA256SUMS").ok_or_else(|| {
        Stop::Refused("SHA256SUMS: error[FB0608]: missing SHA256SUMS in right edition".to_owned())
    })?;
    let left_sums_str = std::str::from_utf8(left_sums_bytes).map_err(|e| {
        Stop::Refused(format!(
            "SHA256SUMS: error[FB0608]: invalid UTF-8 in left SHA256SUMS: {e}"
        ))
    })?;
    let right_sums_str = std::str::from_utf8(right_sums_bytes).map_err(|e| {
        Stop::Refused(format!(
            "SHA256SUMS: error[FB0608]: invalid UTF-8 in right SHA256SUMS: {e}"
        ))
    })?;

    let left_lines: Vec<&str> = left_sums_str
        .lines()
        .filter(|line| {
            if let Some((_sha, path)) = line.split_once("  ") {
                !ATTESTATION_PATHS.contains(&path)
            } else {
                true
            }
        })
        .collect();
    let right_lines: Vec<&str> = right_sums_str
        .lines()
        .filter(|line| {
            if let Some((_sha, path)) = line.split_once("  ") {
                !ATTESTATION_PATHS.contains(&path)
            } else {
                true
            }
        })
        .collect();

    if left_lines != right_lines {
        return Err(Stop::Refused(
            "SHA256SUMS: error[FB0608]: edition SHA256SUMS differs between the deliveries"
                .to_owned(),
        ));
    }

    // 5. Originals trees comparison (must be fully equal including SHA256SUMS).
    for (path, left_bytes) in &left.originals {
        let Some(right_bytes) = right.originals.get(path) else {
            return Err(Stop::Refused(format!(
                "originals/{path}: error[FB0608]: originals/{path} is missing from the right delivery"
            )));
        };
        if left_bytes != right_bytes {
            return Err(Stop::Refused(format!(
                "originals/{path}: error[FB0608]: originals/{path} differs between the deliveries"
            )));
        }
    }
    for path in right.originals.keys() {
        if !left.originals.contains_key(path) {
            return Err(Stop::Refused(format!(
                "originals/{path}: error[FB0608]: originals/{path} is extra in the right delivery"
            )));
        }
    }

    // 6. Support files equality.
    if left.support_toml != right.support_toml {
        return Err(Stop::Refused(
            "corpus-support.toml: error[FB0608]: corpus-support.toml differs between the deliveries"
                .to_owned(),
        ));
    }
    if left.support_sha != right.support_sha {
        return Err(Stop::Refused(
            "corpus-support.sha256: error[FB0608]: corpus-support.sha256 differs between the deliveries"
                .to_owned(),
        ));
    }

    Ok(())
}

/// Command handler for `cargo xtask edition-compare --left DIR --right DIR`.
pub fn command_edition_compare(args: &[&str]) -> Result<bool, String> {
    let ["--left", left_dir, "--right", right_dir] = args else {
        return Err("usage: cargo xtask edition-compare --left DIR --right DIR".to_owned());
    };
    let left = match load_delivery(Path::new(left_dir)) {
        Ok(d) => d,
        Err(Stop::Refused(m)) => {
            eprintln!("error: {m}");
            return Ok(false);
        }
        Err(Stop::Failed(m)) => return Err(m),
    };
    let right = match load_delivery(Path::new(right_dir)) {
        Ok(d) => d,
        Err(Stop::Refused(m)) => {
            eprintln!("error: {m}");
            return Ok(false);
        }
        Err(Stop::Failed(m)) => return Err(m),
    };
    match compare_payload(&left, &right) {
        Ok(()) => {
            println!("the deliveries hold the same payload, apart from the browser attestations");
            Ok(true)
        }
        Err(Stop::Refused(m)) => {
            eprintln!("error: {m}");
            Ok(false)
        }
        Err(Stop::Failed(m)) => Err(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edition_raw::tests::{
        CONTACT, clean_repo, commit_all, git, package, write, write_corpus, zip_map,
    };
    use fieldbook_core::edition::{BuildInfo, BuildProfile};

    /// The commit time of every fixture repository (2026-03-02T12:00:00Z, set by `commit_all`).
    const EPOCH: i64 = 1_772_452_800;

    /// A clean fixture repository: the synthetic Corpus of `edition_raw`'s tests (one active HTML
    /// original, one held PDF original, heading `## edition-001`) with `out/`, `.fieldbook/` and
    /// `inbox/` ignored as in the real repository.
    fn fixture_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        write_corpus(dir.path());
        write(dir.path(), ".gitignore", b"out/\n.fieldbook/\ninbox/\n");
        commit_all(dir.path());
        dir
    }

    /// Copy a whole folder, `.git` included, so that the copy has the same commit.
    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).expect("mkdir");
        for entry in fs::read_dir(from).expect("read_dir") {
            let entry = entry.expect("entry");
            let target = to.join(entry.file_name());
            if entry.file_type().expect("type").is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).expect("copy");
            }
        }
    }

    /// The synthetic summary the scripted harness writes for what it found on disk.
    fn summary_for(tag: &str, browser: &str, pages: usize, texts: usize) -> String {
        format!(
            "edition: {tag}\nbrowser: {browser}\noptions: --headless=new --dump-dom --log-net-log=<file> --disable-background-networking\npages loaded: {pages}\ntext-renamed originals loaded: {texts}\nrequests started by those loads: 12\nrequests to file: URLs: 12\nrequests to any other scheme or host: 0\nself-test: pass\nnegative page, requests blocked: 1\nnegative page, requests reported: 1\nverdict: pass\n"
        )
    }

    /// What `fieldbook build` leaves in `out/edition` and `out/originals`, for the fixture: a few
    /// synthetic pages and assets, the real `edition.toml` of the fixture Corpus, and the renamed
    /// original. `build_no` is written into `about.html`, so a second build is visible.
    fn synthetic_site(root: &Path, epoch: i64, sha: &str, build_no: usize) -> Result<(), String> {
        use fieldbook_core::check::{Host, Options, load};
        use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped};
        let tree = fieldbook::io::tree::read_tree(root).map_err(|e| e.to_string())?;
        let now = civil_utc(epoch).map_err(|e| e.to_string())?;
        let (model, report) = load(&Host::new(tree, now.date), &Options::default());
        if report.errors() != 0 {
            return Err("the fixture Corpus does not load".to_owned());
        }
        let eff = fieldbook_core::policy::EffectivePolicy::floor(false);
        let Decision::Allowed(allowed) =
            fieldbook_core::egress::check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
        else {
            return Err("edition destination refused".to_owned());
        };
        let view = LabelledView::new(
            &model,
            &allowed,
            &fieldbook_core::authz::ReadableStores::corpus(),
        );
        let mut quarantined = BTreeSet::new();
        for s in view.sources() {
            if s.value.lifecycle == Lifecycle::Quarantined {
                quarantined.insert(
                    VersionId::parse(&format!("{}-v{}", s.value.source_id, s.value.version))
                        .map_err(|e| e.to_string())?,
                );
            }
        }
        let files = shipped_corpus(&view, &quarantined).map_err(|e| e.to_string())?;
        let changes =
            fs::read_to_string(root.join("corpus/changes.md")).map_err(|e| e.to_string())?;
        let heading = latest_edition_heading(&changes)
            .ok_or("no heading")?
            .to_owned();
        let schema = view.corpus().map(|c| c.schema_version).ok_or("no corpus")?;
        let mut info = EditionInfo::from(
            &view,
            &files,
            Some(&heading),
            &BuildInfo::new(
                Some(now.date),
                Some(sha.to_owned()),
                BuildProfile::Silent,
                Some(epoch),
            ),
            env!("CARGO_PKG_VERSION"),
            schema,
        )
        .map_err(|e| e.to_string())?;
        info.counts.originals = 1;

        let edition_dir = root.join("out/edition");
        let originals_dir = root.join("out/originals");
        for dir in [&edition_dir, &originals_dir] {
            if dir.exists() {
                fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
            }
        }
        let page = |title: &str| {
            format!("<!doctype html>\n<title>{title}</title>\n<p>synthetic page</p>\n")
        };
        write(root, "out/edition/index.html", page("Fieldbook").as_bytes());
        write(
            root,
            "out/edition/about.html",
            format!("{}<!-- build {build_no} -->\n", page("About")).as_bytes(),
        );
        write(
            root,
            &format!("out/edition/{}", fieldbook_core::strings::PATHS[3]),
            page("Day-One Binder").as_bytes(),
        );
        write(
            root,
            "out/edition/p/cmp-sample.html",
            page("Synthetic Title").as_bytes(),
        );
        write(
            root,
            "out/edition/s/S-0001-v1.html",
            page("Synthetic Active Source").as_bytes(),
        );
        write(root, "out/edition/assets/app.js", b"// synthetic script\n");
        write(
            root,
            "out/edition/assets/app.css",
            b"/* synthetic style */\n",
        );
        write(
            root,
            "out/edition/assets/strings.js",
            b"// synthetic strings\n",
        );
        write(
            root,
            "out/edition/assets/search-index.js",
            b"// synthetic index of S-0001-v1\n",
        );
        write(root, "out/edition/edition.toml", info.to_toml().as_bytes());
        write(
            root,
            "out/edition/data/registers.xlsx",
            b"PK synthetic workbook",
        );
        write(root, "out/edition/data/claims.csv", b"claim_id\r\n");
        write(root, "out/edition/data/conflicts.csv", b"conflict_id\r\n");
        write(root, "out/edition/data/glossary.csv", b"term_id\r\n");
        write(root, "out/edition/data/questions.csv", b"question_id\r\n");
        write(root, "out/edition/data/sources.csv", b"source_id\r\n");
        let original = fs::read(root.join("corpus/sources/public/S-0001-v1.html"))
            .map_err(|e| e.to_string())?;
        write(root, "out/originals/S-0001-v1.html.txt", &original);
        Ok(())
    }

    /// A site tree and an originals tree, by path.
    type TreePair = (BTreeMap<String, Vec<u8>>, BTreeMap<String, Vec<u8>>);

    /// A [`Stage`] that runs real `git` and scripts everything else. Every call is recorded.
    struct Scripted {
        calls: Vec<String>,
        browser: String,
        check_code: i32,
        harness_code: i32,
        write_summary: bool,
        summary_edition: Option<String>,
        /// Files written below `out/` after the build, as `edition/<rel>` or `originals/<rel>`.
        planted: Vec<(String, Vec<u8>)>,
        /// A site file rewritten while the harness runs.
        changed_by_harness: Option<(String, Vec<u8>)>,
        /// The two trees as they were on disk when the harness ran.
        seen_by_harness: Option<TreePair>,
        builds: usize,
    }

    impl Scripted {
        fn new() -> Self {
            Self::with_browser("Microsoft Edge 130.0.2849.46")
        }

        fn with_browser(browser: &str) -> Self {
            Self {
                calls: Vec::new(),
                browser: browser.to_owned(),
                check_code: 0,
                harness_code: 0,
                write_summary: true,
                summary_edition: None,
                planted: Vec::new(),
                changed_by_harness: None,
                seen_by_harness: None,
                builds: 0,
            }
        }

        fn called(&self, what: &str) -> bool {
            self.calls.iter().any(|c| c.starts_with(what))
        }
    }

    impl Stage for Scripted {
        fn git(&mut self, root: &Path, args: &[&str]) -> Result<String, String> {
            self.calls.push(format!("git {}", args.join(" ")));
            Ok(crate::proc::git(root, args)?.trim().to_owned())
        }

        fn check(&mut self, _root: &Path, today: &str) -> Result<RunOutput, String> {
            self.calls.push(format!("check {today}"));
            Ok(RunOutput {
                code: self.check_code,
                stdout: b"synthetic check output\n".to_vec(),
                stderr: Vec::new(),
            })
        }

        fn build(&mut self, root: &Path, epoch: i64, sha: &str) -> Result<RunOutput, String> {
            self.calls.push(format!("build {epoch} {sha}"));
            self.builds += 1;
            synthetic_site(root, epoch, sha, self.builds)?;
            for (path, bytes) in &self.planted {
                write(root, &format!("out/{path}"), bytes);
            }
            Ok(RunOutput {
                code: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }

        fn harness(&mut self, root: &Path) -> Result<RunOutput, String> {
            self.calls.push("harness".to_owned());
            let site = read_disk_tree(&root.join("out/edition")).map_err(|e| e.to_string())?;
            let originals =
                read_disk_tree(&root.join("out/originals")).map_err(|e| e.to_string())?;
            if let Some((path, bytes)) = &self.changed_by_harness {
                write(root, &format!("out/edition/{path}"), bytes);
            }
            if self.harness_code == 0 && self.write_summary {
                let pages = site.keys().filter(|p| p.ends_with(".html")).count();
                let texts = originals
                    .keys()
                    .filter(|p| p.ends_with(".html.txt"))
                    .count();
                let tag = self
                    .summary_edition
                    .clone()
                    .unwrap_or_else(|| "edition-001".to_owned());
                write(
                    root,
                    "out/harness/network-silence-summary.txt",
                    summary_for(&tag, &self.browser, pages, texts).as_bytes(),
                );
            }
            self.seen_by_harness = Some((site, originals));
            Ok(RunOutput {
                code: self.harness_code,
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }

    /// Package the fixture with a fresh scripted stage; the stage is returned for inspection.
    fn run(repo: &Path) -> (Result<Package, Stop>, Scripted) {
        let mut stage = Scripted::new();
        let result = edition(repo, &mut stage, Some(CONTACT));
        (result, stage)
    }

    /// The five files of `out/dist` of `repo`, by name.
    fn dist_files(repo: &Path) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(repo.join("out/dist"))
            .expect("out/dist")
            .map(|e| {
                let e = e.expect("entry");
                (
                    e.file_name().to_string_lossy().into_owned(),
                    fs::read(e.path()).expect("read"),
                )
            })
            .collect();
        files.sort();
        files
    }

    fn file<'a>(files: &'a [(String, Vec<u8>)], name: &str) -> &'a [u8] {
        files
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, b)| b.as_slice())
            .expect("file in delivery")
    }

    /// The two trees of a package, reopened from its zips.
    fn trees(pkg: &Package) -> TreePair {
        let d = delivery_from_files(&pkg.files).expect("delivery");
        (d.edition, d.originals)
    }

    /// The five delivery files for trees changed by `edit`, with every derived file recomputed so
    /// that it is a well-formed package: internal checksums (unless `fix_sums` is false), both
    /// zips, the outer list, the support copy and its digest.
    fn rebuilt_files(
        pkg: &Package,
        fix_sums: bool,
        edit: impl FnOnce(&mut BTreeMap<String, Vec<u8>>, &mut BTreeMap<String, Vec<u8>>),
    ) -> Vec<(String, Vec<u8>)> {
        let (mut edition_tree, mut originals_tree) = trees(pkg);
        edit(&mut edition_tree, &mut originals_tree);
        if fix_sums {
            edition_tree.remove("SHA256SUMS");
            originals_tree.remove("SHA256SUMS");
            let a = sums_of(&edition_tree);
            let b = sums_of(&originals_tree);
            edition_tree.insert("SHA256SUMS".to_owned(), a.into_bytes());
            originals_tree.insert("SHA256SUMS".to_owned(), b.into_bytes());
        }
        let edition_zip =
            write_deterministic_zip_bytes("edition", EPOCH, &edition_tree).expect("zip");
        let originals_zip =
            write_deterministic_zip_bytes("originals", EPOCH, &originals_tree).expect("zip");
        let names: Vec<&String> = pkg.files.iter().map(|(n, _)| n).collect();
        let support = edition_tree
            .get("evidence/corpus-support.toml")
            .cloned()
            .unwrap_or_default();
        vec![
            (names[0].clone(), edition_zip.clone()),
            (names[1].clone(), originals_zip.clone()),
            (
                "SHA256SUMS".to_owned(),
                write_sums(&[
                    (names[0].clone(), sha256_hex(&edition_zip)),
                    (names[1].clone(), sha256_hex(&originals_zip)),
                ])
                .into_bytes(),
            ),
            ("corpus-support.toml".to_owned(), support.clone()),
            (
                "corpus-support.sha256".to_owned(),
                write_sums(&[("corpus-support.toml".to_owned(), sha256_hex(&support))])
                    .into_bytes(),
            ),
        ]
    }

    /// [`rebuilt_files`] as a [`Delivery`], not yet verified.
    fn rebuilt(
        pkg: &Package,
        fix_sums: bool,
        edit: impl FnOnce(&mut BTreeMap<String, Vec<u8>>, &mut BTreeMap<String, Vec<u8>>),
    ) -> Delivery {
        delivery_from_files(&rebuilt_files(pkg, fix_sums, edit)).expect("rebuilt delivery")
    }

    /// Write delivery files into a folder, as `out/dist` holds them.
    fn write_files(dir: &Path, files: &[(String, Vec<u8>)]) {
        fs::create_dir_all(dir).expect("mkdir");
        for (name, bytes) in files {
            fs::write(dir.join(name), bytes).expect("write");
        }
    }

    fn zip_entry_names(bytes: &[u8]) -> Vec<String> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("zip");
        (0..archive.len())
            .map(|i| {
                archive
                    .by_index(i)
                    .expect("entry")
                    .name()
                    .expect("name")
                    .into_owned()
            })
            .collect()
    }

    fn text(bytes: &[u8]) -> String {
        String::from_utf8(bytes.to_vec()).expect("utf8")
    }

    fn refused(result: Result<Package, Stop>) -> String {
        match result {
            Err(Stop::Refused(message)) => message,
            Err(Stop::Failed(message)) => panic!("failed, not refused: {message}"),
            Ok(_) => panic!("expected a refusal"),
        }
    }

    #[test]
    fn a_fixture_run_builds_the_five_files() {
        let repo = fixture_repo();
        let (result, stage) = run(repo.path());
        let pkg = result.expect("package");
        assert_eq!(pkg.tag, "edition-001");
        assert_eq!(
            pkg.files
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            vec![
                "fieldbook-edition-001.zip",
                "fieldbook-originals-001.zip",
                "SHA256SUMS",
                "corpus-support.toml",
                "corpus-support.sha256",
            ]
        );
        assert_eq!(dist_files(repo.path()).len(), 5);
        let order: Vec<&str> = stage
            .calls
            .iter()
            .filter(|c| !c.starts_with("git "))
            .map(|c| c.split(' ').next().unwrap_or_default())
            .collect();
        assert_eq!(order, vec!["check", "build", "harness"]);
        let delivery = delivery_from_files(&pkg.files).expect("delivery");
        verify_delivery(&delivery).expect("a fresh package verifies");
    }

    // control: DT-01
    #[test]
    fn two_runs_give_identical_zips() {
        let a = fixture_repo();
        let b = tempfile::tempdir().expect("tempdir b");
        copy_dir(a.path(), b.path());
        let (result_a, _) = run(a.path());
        let pkg_a = result_a.expect("package a");
        let child_out_dir = tempfile::tempdir().expect("child out");
        let child_cwd = tempfile::tempdir().expect("child cwd");
        let child_out = std::process::Command::new(std::env::current_exe().expect("exe"))
            .args([
                "edition::tests::build_in_child",
                "--exact",
                "--ignored",
                "--nocapture",
            ])
            .env("TZ", "Pacific/Auckland")
            .env("FIELDBOOK_TEST_FIXTURE", b.path())
            .env("FIELDBOOK_TEST_OUT", child_out_dir.path())
            .current_dir(child_cwd.path())
            .output()
            .expect("child process");
        assert!(
            child_out.status.success(),
            "child failed: status {:?}\nstdout: {}\nstderr: {}",
            child_out.status,
            String::from_utf8_lossy(&child_out.stdout),
            String::from_utf8_lossy(&child_out.stderr)
        );
        assert_eq!(pkg_a.files.len(), 5);
        for (name, bytes) in &pkg_a.files {
            let child_bytes = fs::read(child_out_dir.path().join(name)).expect("read child file");
            assert_eq!(bytes, &child_bytes, "file {name} differs between runs");
        }
        let on_disk_a = dist_files(a.path());
        let mut sorted_pkg = pkg_a.files.clone();
        sorted_pkg.sort();
        assert_eq!(sorted_pkg, on_disk_a);
    }

    #[test]
    fn zip_top_folders() {
        let repo = fixture_repo();
        let (result, _) = run(repo.path());
        let pkg = result.expect("package");
        let ed_bytes = file(&pkg.files, "fieldbook-edition-001.zip");
        let orig_bytes = file(&pkg.files, "fieldbook-originals-001.zip");
        let ed_entries = zip_entry_names(ed_bytes);
        let orig_entries = zip_entry_names(orig_bytes);
        assert!(!ed_entries.is_empty(), "edition zip is empty");
        for entry in &ed_entries {
            assert!(
                entry.starts_with("edition/"),
                "edition entry does not start with edition/: {entry}"
            );
        }
        assert!(!orig_entries.is_empty(), "originals zip is empty");
        for entry in &orig_entries {
            assert!(
                entry.starts_with("originals/"),
                "originals entry does not start with originals/: {entry}"
            );
        }
        assert!(
            orig_entries
                .iter()
                .any(|e| e == "originals/S-0001-v1.html.txt"),
            "originals zip missing originals/S-0001-v1.html.txt"
        );
    }

    #[test]
    fn two_summaries_preserve_the_deterministic_payload() {
        let repo_a = fixture_repo();
        let repo_b = tempfile::tempdir().expect("tempdir b");
        copy_dir(repo_a.path(), repo_b.path());
        let mut stage_a = Scripted::with_browser("Microsoft Edge 130.0.2849.46");
        let pkg_a = edition(repo_a.path(), &mut stage_a, Some(CONTACT)).expect("package a");
        let mut stage_b = Scripted::with_browser("Microsoft Edge 131.0.2903.9");
        let pkg_b = edition(repo_b.path(), &mut stage_b, Some(CONTACT)).expect("package b");

        let d_a = delivery_from_files(&pkg_a.files).expect("delivery a");
        let d_b = delivery_from_files(&pkg_b.files).expect("delivery b");

        for (pkg, repo_path, d) in [(&pkg_a, repo_a.path(), &d_a), (&pkg_b, repo_b.path(), &d_b)] {
            verify_delivery(d).expect("delivery verifies");
            let staged_cat = d
                .edition
                .get("evidence/corpus-support.toml")
                .expect("staged cat");
            let delivered_cat = file(&pkg.files, "corpus-support.toml");
            let on_disk_cat =
                fs::read(repo_path.join("out/dist/corpus-support.toml")).expect("disk cat");
            assert_eq!(staged_cat, delivered_cat);
            assert_eq!(staged_cat, &on_disk_cat);

            let sums_text = text(d.edition.get("SHA256SUMS").expect("internal sums"));
            let cat_row_hash = sums_text
                .lines()
                .find_map(|line| {
                    let (sha, path) = line.split_once("  ")?;
                    if path == "evidence/corpus-support.toml" {
                        Some(sha)
                    } else {
                        None
                    }
                })
                .expect("support row in SHA256SUMS");
            let sidecar_text = text(file(&pkg.files, "corpus-support.sha256"));
            let sidecar_first_token = sidecar_text
                .split_whitespace()
                .next()
                .expect("sidecar first token");
            assert_eq!(cat_row_hash, sidecar_first_token);
        }

        assert_ne!(
            file(&pkg_a.files, "fieldbook-edition-001.zip"),
            file(&pkg_b.files, "fieldbook-edition-001.zip"),
            "edition zips should not be byte-equal"
        );

        let (ed_a, orig_a) = trees(&pkg_a);
        let (ed_b, orig_b) = trees(&pkg_b);

        let mut differing_paths = BTreeSet::new();
        for (path, bytes_a) in &ed_a {
            if ed_b.get(path) != Some(bytes_a) {
                differing_paths.insert(path.as_str());
            }
        }
        for path in ed_b.keys() {
            if !ed_a.contains_key(path) {
                differing_paths.insert(path.as_str());
            }
        }
        let expected_diff: BTreeSet<&str> = [
            "intake/network-silence-summary.txt",
            "TRANSFER-STATEMENT.txt",
            "SHA256SUMS",
        ]
        .into_iter()
        .collect();
        assert_eq!(differing_paths, expected_diff);

        for path in ed_a.keys() {
            if !expected_diff.contains(path.as_str()) {
                assert_eq!(ed_a[path], ed_b[path], "path {path} should be byte-equal");
            }
        }

        assert_eq!(orig_a, orig_b);
        assert_eq!(
            file(&pkg_a.files, "corpus-support.toml"),
            file(&pkg_b.files, "corpus-support.toml")
        );
        assert_eq!(
            file(&pkg_a.files, "corpus-support.sha256"),
            file(&pkg_b.files, "corpus-support.sha256")
        );
        compare_payload(&d_a, &d_b).expect("compare payload");
    }

    // control: DR-01
    #[test]
    fn readme_gives_the_working_folder_and_attrib_instruction() {
        let repo = fixture_repo();
        let (result, _) = run(repo.path());
        let pkg = result.expect("package");
        let (edition_tree, _) = trees(&pkg);
        let readme = text(edition_tree.get("README.txt").expect("README.txt"));
        assert!(readme.contains("attrib +I"), "{readme}");
        assert!(
            readme.contains("$env:LOCALAPPDATA\\Fieldbook\\working"),
            "{readme}"
        );
        assert!(readme.contains("First transfer only"), "{readme}");
    }

    // control: IN-01
    #[test]
    fn placeholders_all_filled() {
        let repo = fixture_repo();
        let (result, _) = run(repo.path());
        let pkg = result.expect("package");
        let (edition_tree, _) = trees(&pkg);

        let has_placeholder = |s: &str| -> bool {
            let bytes = s.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'{' {
                    let mut j = i + 1;
                    while j < bytes.len()
                        && (bytes[j].is_ascii_lowercase()
                            || bytes[j].is_ascii_digit()
                            || bytes[j] == b'_')
                    {
                        j += 1;
                    }
                    if j > i + 1 && j < bytes.len() && bytes[j] == b'}' {
                        return true;
                    }
                }
                i += 1;
            }
            false
        };

        for (path, content) in &edition_tree {
            let is_target = path == "README.txt"
                || path == "TRANSFER-STATEMENT.txt"
                || (path.starts_with("intake/")
                    && (path.ends_with(".md") || path.ends_with(".txt")));
            if is_target {
                let content_str = text(content);
                assert!(
                    !has_placeholder(&content_str),
                    "placeholder found in {path}: {content_str}"
                );
            }
        }

        let transfer_text = text(
            edition_tree
                .get("TRANSFER-STATEMENT.txt")
                .expect("TRANSFER-STATEMENT.txt"),
        );
        assert!(
            transfer_text.contains(CONTACT),
            "TRANSFER-STATEMENT.txt missing CONTACT"
        );
        let readme_text = text(edition_tree.get("README.txt").expect("README.txt"));
        assert!(
            readme_text.contains("edition-001"),
            "README.txt missing edition-001"
        );

        for bad_contact in [None, Some(""), Some("   \t ")] {
            let fresh_repo = fixture_repo();
            let mut stage = Scripted::new();
            let res = edition(fresh_repo.path(), &mut stage, bad_contact);
            let msg = refused(res);
            assert!(
                msg.contains("IN-01") && msg.contains("FIELDBOOK_SECURITY_CONTACT"),
                "unexpected refusal message: {msg}"
            );
            assert!(
                !stage.called("build"),
                "build was called for bad contact {bad_contact:?}"
            );
        }

        {
            let fresh_repo = fixture_repo();
            let mut stage = Scripted::new();
            let res = edition(fresh_repo.path(), &mut stage, Some("two\nlines"));
            let msg = refused(res);
            assert!(msg.contains("IN-01"), "expected IN-01 for two lines: {msg}");
        }
    }

    #[test]
    fn missing_summary_names_in_01() {
        let repo = fixture_repo();
        let mut stage = Scripted::new();
        stage.write_summary = false;
        stage.harness_code = 0;
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        let msg = refused(res);
        assert!(
            msg.contains("IN-01") && msg.contains("network-silence-summary.txt"),
            "expected IN-01 and network-silence-summary.txt, got: {msg}"
        );
        assert!(!repo.path().join("out/dist").exists(), "out/dist exists");
    }

    #[test]
    fn missing_required_file_is_fb0601() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        for path in [
            fieldbook_core::strings::PATHS[6],
            "evidence/corpus-support.toml",
        ] {
            let d = rebuilt(&pkg, true, |e, _| {
                e.remove(path);
            });
            match verify_delivery(&d) {
                Err(Stop::Refused(m)) => {
                    assert!(
                        m.contains("FB0601") && m.contains(path),
                        "expected FB0601 and {path}, got: {m}"
                    );
                }
                other => panic!("expected Err(Stop::Refused), got: {other:?}"),
            }
        }
    }

    // control: IN-01
    #[test]
    fn missing_intake_file_is_fb0601() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let path = "intake/data-handling.md";
        let d = rebuilt(&pkg, true, |e, _| {
            e.remove(path);
        });
        match verify_delivery(&d) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains(path),
                    "expected FB0601 and {path}, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    #[test]
    fn missing_binder_is_fb0601() {
        // Part one: healthy Raw-profile stage without day-one.html succeeds.
        let raw_repo = clean_repo();
        let raw_out = tempfile::tempdir().expect("raw out");
        let mut runner = crate::edition_raw::tests::Scripted::clean();
        let _raw_pkg = package(raw_repo.path(), raw_out.path(), &mut runner).expect("raw package");
        let raw_edition = zip_map(&raw_out.path().join("fieldbook-edition-001.zip"), "edition");
        let raw_originals = zip_map(
            &raw_out.path().join("fieldbook-originals-001.zip"),
            "originals",
        );
        let binder = fieldbook_core::strings::PATHS[3];
        assert!(
            !raw_edition.contains_key(binder),
            "raw edition must not hold day-one.html"
        );
        verify_staged_maps(AllowlistProfile::Raw, &raw_edition, &raw_originals, &[])
            .expect("healthy raw stage succeeds without day-one.html");

        // Part two: healthy Site stage holds day-one.html and passes, then fails FB0601 when removed.
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let delivery = delivery_from_files(&pkg.files).expect("delivery");
        assert!(
            delivery.edition.contains_key(binder),
            "healthy site stage must hold day-one.html"
        );
        verify_delivery(&delivery).expect("healthy site stage delivery verifies");
        verify_staged_maps(
            AllowlistProfile::Site,
            &delivery.edition,
            &delivery.originals,
            REQUIRED_FILES,
        )
        .expect("healthy site stage passes verify_staged_maps");

        let d = rebuilt(&pkg, true, |e, _| {
            let removed = e.remove(binder);
            assert!(removed.is_some(), "day-one.html was present before removal");
        });
        match verify_delivery(&d) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains(binder),
                    "expected FB0601 and {binder}, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    #[test]
    fn missing_workbook_is_fb0601() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let delivery = delivery_from_files(&pkg.files).expect("delivery");
        assert!(
            delivery
                .edition
                .contains_key(fieldbook_core::strings::PATHS[0]),
            "healthy site stage must hold data/registers.xlsx"
        );
        let d = rebuilt(&pkg, true, |e, _| {
            e.remove(fieldbook_core::strings::PATHS[0]);
        });
        match verify_delivery(&d) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains(fieldbook_core::strings::PATHS[0]),
                    "expected FB0601 and data/registers.xlsx, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    // control: ED-01
    #[test]
    fn planted_ps1_is_fb0601() {
        let repo = fixture_repo();
        let mut stage = Scripted::new();
        stage.planted = vec![(
            "originals/planted.ps1".to_owned(),
            b"Write-Host x\n".to_vec(),
        )];
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        let msg = refused(res);
        assert!(
            msg.contains("FB0601") && msg.contains("planted.ps1"),
            "expected FB0601 and planted.ps1, got: {msg}"
        );
        assert!(!repo.path().join("out/dist").exists(), "out/dist exists");

        // A registered original whose location names a script: refused by its extension alone,
        // because every other rule accepts the tree.
        let repo = fixture_repo();
        let sources = fs::read_to_string(repo.path().join("corpus/registers/sources.csv"))
            .expect("sources.csv");
        let changed = sources.replace(
            "https://example.invalid/synthetic/one",
            "https://example.invalid/setup.ps1?raw=1",
        );
        assert_ne!(sources, changed, "the fixture location was not found");
        write(
            repo.path(),
            "corpus/registers/sources.csv",
            changed.as_bytes(),
        );
        commit_all(repo.path());
        let (res, _) = run(repo.path());
        let msg = refused(res);
        assert!(
            msg.contains("FB0601") && msg.contains("ps1"),
            "expected FB0601 for a ps1 location, got: {msg}"
        );
    }

    #[test]
    fn long_path_is_fb0602() {
        let repo = fixture_repo();
        let mut stage = Scripted::new();
        let long_name = format!("edition/p/{}.txt", "a".repeat(160));
        stage.planted = vec![(long_name, b"x\n".to_vec())];
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        let msg = refused(res);
        assert!(msg.contains("FB0602"), "expected FB0602, got: {msg}");
        assert!(!repo.path().join("out/dist").exists(), "out/dist exists");
    }

    #[test]
    fn changed_file_is_fb0608() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");

        // (a)
        let d_a = rebuilt(&pkg, false, |e, _| {
            let bytes = e
                .get_mut("evidence/corpus-support.toml")
                .expect("corpus-support.toml");
            bytes[0] ^= 1;
        });
        match verify_delivery(&d_a) {
            Err(Stop::Refused(m)) => {
                assert!(m.contains("FB0608"), "expected FB0608 in (a), got: {m}");
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        // (b)
        let dir = tempfile::tempdir().expect("tempdir");
        write_files(dir.path(), &pkg.files);
        let orig_cat = fs::read(dir.path().join("corpus-support.toml")).expect("read cat");
        let mut modified_cat = orig_cat;
        modified_cat.push(b'x');
        fs::write(dir.path().join("corpus-support.toml"), &modified_cat).expect("write cat");
        let res_b = load_delivery(dir.path()).and_then(|d| verify_delivery(&d));
        match res_b {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0608") && m.contains("corpus-support"),
                    "expected FB0608 and corpus-support in (b), got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        // (b2) A changed `out/dist` copy whose digest file was rewritten to match it is still
        // refused: the digest is taken from the staged catalogue, never from the copy it describes.
        let line = write_sums(&[("corpus-support.toml".to_owned(), sha256_hex(&modified_cat))]);
        fs::write(dir.path().join("corpus-support.sha256"), line).expect("write sidecar");
        let res_b2 = load_delivery(dir.path()).and_then(|d| verify_delivery(&d));
        match res_b2 {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0608") && m.contains("corpus-support"),
                    "expected FB0608 and corpus-support in (b2), got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        // (c)
        let d_c = rebuilt(&pkg, false, |e, _| {
            let bytes = e.get_mut("about.html").expect("about.html");
            bytes[0] ^= 1;
        });
        match verify_delivery(&d_c) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0608") && m.contains("about.html"),
                    "expected FB0608 and about.html in (c), got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    // control: ED-01
    #[test]
    fn renderable_original_stays_txt() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let (_, orig_tree) = trees(&pkg);
        assert!(
            orig_tree.contains_key("S-0001-v1.html.txt"),
            "missing S-0001-v1.html.txt in originals"
        );
        assert!(
            !orig_tree.keys().any(|k| k.ends_with(".html")),
            "found key ending in .html in originals tree"
        );

        let d = rebuilt(&pkg, true, |_, o| {
            o.insert("S-0001-v1.html".to_owned(), b"<p>x</p>".to_vec());
        });
        match verify_delivery(&d) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains("S-0001-v1.html"),
                    "expected FB0601 and S-0001-v1.html, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    #[test]
    fn state_never_ships() {
        let repo = fixture_repo();
        write(repo.path(), ".fieldbook/journal.jsonl", b"{}\n");
        write(repo.path(), "inbox/note.txt", b"x\n");
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let (ed_tree, orig_tree) = trees(&pkg);
        for k in ed_tree.keys().chain(orig_tree.keys()) {
            assert!(
                !k.contains(".fieldbook"),
                "tree contains .fieldbook path: {k}"
            );
            assert!(!k.contains("inbox"), "tree contains inbox path: {k}");
        }

        for planted in [".fieldbook/journal.jsonl", "inbox/note.txt"] {
            let d = rebuilt(&pkg, true, |e, _| {
                e.insert(planted.to_owned(), b"planted\n".to_vec());
            });
            match verify_delivery(&d) {
                Err(Stop::Refused(m)) => {
                    assert!(
                        m.contains("FB0601") && m.contains(planted),
                        "expected FB0601 and {planted}, got: {m}"
                    );
                }
                other => panic!("expected Err(Stop::Refused), got: {other:?}"),
            }
        }
    }

    #[test]
    fn held_version_absent() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let (ed_tree, orig_tree) = trees(&pkg);

        let sources_csv = text(
            ed_tree
                .get("corpus/registers/sources.csv")
                .expect("sources.csv"),
        );
        assert!(sources_csv.contains("S-0002,1,"));
        assert!(sources_csv.contains("quarantined"));
        assert!(sources_csv.contains("Held for synthetic licence review."));
        let s0002_row = sources_csv
            .lines()
            .find(|line| line.starts_with("S-0002,"))
            .expect("S-0002 row");
        assert!(
            s0002_row
                .split(',')
                .any(|field| field.len() == 64 && field.chars().all(|c| c.is_ascii_hexdigit())),
            "sources.csv row missing 64-hex hash"
        );

        for k in ed_tree.keys() {
            assert!(
                !k.contains("S-0002-v1"),
                "edition key contains S-0002-v1: {k}"
            );
        }
        for k in orig_tree.keys() {
            assert!(!k.contains("S-0002"), "originals key contains S-0002: {k}");
        }
        let search_idx = text(
            ed_tree
                .get("assets/search-index.js")
                .expect("search-index.js"),
        );
        assert!(
            !search_idx.contains("S-0002-v1"),
            "search-index.js contains S-0002-v1"
        );

        // 1. plant original o.insert("S-0002-v1.pdf".into(), b"x".to_vec())
        let d1 = rebuilt(&pkg, true, |_, o| {
            o.insert("S-0002-v1.pdf".to_owned(), b"x".to_vec());
        });
        match verify_delivery(&d1) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains("S-0002-v1"),
                    "expected FB0601 and S-0002-v1 in plant 1, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        // 2. plant e.insert("corpus/sources/text/S-0002-v1/p0001.txt".into(), b"x".to_vec())
        let d2 = rebuilt(&pkg, true, |e, _| {
            e.insert(
                "corpus/sources/text/S-0002-v1/p0001.txt".to_owned(),
                b"x".to_vec(),
            );
        });
        match verify_delivery(&d2) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains("S-0002-v1"),
                    "expected FB0601 and S-0002-v1 in plant 2, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        // 3. plant an EMPTY e.insert("s/S-0002-v1.html".into(), Vec::new())
        let d3 = rebuilt(&pkg, true, |e, _| {
            e.insert("s/S-0002-v1.html".to_owned(), Vec::new());
        });
        match verify_delivery(&d3) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains("S-0002-v1"),
                    "expected FB0601 and S-0002-v1 in plant 3, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        // 4. search unit: replace assets/search-index.js with text that contains S-0002-v1
        let d4 = rebuilt(&pkg, true, |e, _| {
            e.insert(
                "assets/search-index.js".to_owned(),
                b"// index containing S-0002-v1\n".to_vec(),
            );
        });
        match verify_delivery(&d4) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0601") && m.contains("S-0002-v1"),
                    "expected FB0601 and S-0002-v1 in plant 4, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    #[test]
    fn zip_sizes_are_printed() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        assert_eq!(
            pkg.size_lines,
            vec![
                "fieldbook-edition-001.zip <1 MB".to_owned(),
                "fieldbook-originals-001.zip <1 MB".to_owned(),
            ]
        );
        assert_eq!(
            size_line("fieldbook-originals-001.zip", 1_500_000),
            "fieldbook-originals-001.zip 2 MB"
        );
        assert_eq!(size_line("x.zip", 999_999), "x.zip <1 MB");
        assert_eq!(size_line("x.zip", 41_000_000), "x.zip 41 MB");
    }

    #[test]
    fn compare_payload_accepts_different_browser_versions() {
        let repo_a = fixture_repo();
        let repo_b = tempfile::tempdir().expect("tempdir b");
        copy_dir(repo_a.path(), repo_b.path());
        let mut stage_a = Scripted::with_browser("Microsoft Edge 130.0.2849.46");
        let pkg_a = edition(repo_a.path(), &mut stage_a, Some(CONTACT)).expect("package a");
        let mut stage_b = Scripted::with_browser("Microsoft Edge 131.0.2903.9");
        let pkg_b = edition(repo_b.path(), &mut stage_b, Some(CONTACT)).expect("package b");
        let da = delivery_from_files(&pkg_a.files).expect("delivery a");
        let db = delivery_from_files(&pkg_b.files).expect("delivery b");
        compare_payload(&da, &db).expect("compare payload a to b");
        compare_payload(&db, &da).expect("compare payload b to a");
    }

    #[test]
    fn compare_payload_detects_changed_deterministic_file() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let left = delivery_from_files(&pkg.files).expect("left delivery");

        let right = rebuilt(&pkg, true, |e, _| {
            let bytes = e.get_mut("about.html").expect("about.html");
            bytes.extend_from_slice(b"\n<!-- changed -->");
        });
        match compare_payload(&left, &right) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("FB0608") && m.contains("about.html"),
                    "expected FB0608 and about.html, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        let right_cat = rebuilt(&pkg, true, |e, _| {
            let bytes = e
                .get_mut("evidence/corpus-support.toml")
                .expect("corpus-support.toml");
            bytes.extend_from_slice(b"# x\n");
        });
        match compare_payload(&left, &right_cat) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("corpus-support"),
                    "expected refusal mentioning corpus-support, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        let right_orig = rebuilt(&pkg, true, |_, o| {
            let bytes = o.get_mut("S-0001-v1.html.txt").expect("S-0001-v1.html.txt");
            bytes.extend_from_slice(b"\n<!-- changed -->");
        });
        match compare_payload(&left, &right_orig) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.contains("S-0001-v1.html.txt"),
                    "expected refusal mentioning S-0001-v1.html.txt, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    #[test]
    fn compare_payload_rejects_corrupted_manifest() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let left = delivery_from_files(&pkg.files).expect("left delivery");

        let right = rebuilt(&pkg, false, |e, _| {
            let bytes = e.get_mut("about.html").expect("about.html");
            bytes[0] ^= 1;
        });
        match compare_payload(&left, &right) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.starts_with("right:") && m.contains("FB0608"),
                    "expected right: and FB0608, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        let left_bad = rebuilt(&pkg, false, |e, _| {
            let bytes = e.get_mut("about.html").expect("about.html");
            bytes[0] ^= 1;
        });
        match compare_payload(&left_bad, &left) {
            Err(Stop::Refused(m)) => {
                assert!(
                    m.starts_with("left:") && m.contains("FB0608"),
                    "expected left: and FB0608, got: {m}"
                );
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }

        let bad_att = rebuilt(&pkg, false, |e, _| {
            let bytes = e
                .get_mut("TRANSFER-STATEMENT.txt")
                .expect("TRANSFER-STATEMENT.txt");
            bytes.extend_from_slice(b"corrupted\n");
        });
        match compare_payload(&left, &bad_att) {
            Err(Stop::Refused(m)) => {
                assert!(m.contains("FB0608"), "expected FB0608, got: {m}");
            }
            other => panic!("expected Err(Stop::Refused), got: {other:?}"),
        }
    }

    #[test]
    fn compare_payload_rejects_missing_attestation() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");
        let left = delivery_from_files(&pkg.files).expect("left delivery");
        for path in ATTESTATION_PATHS {
            let right = rebuilt(&pkg, true, |e, _| {
                e.remove(path);
            });
            match compare_payload(&left, &right) {
                Err(Stop::Refused(m)) => {
                    assert!(
                        m.contains(path),
                        "expected message mentioning {path}, got: {m}"
                    );
                }
                other => panic!("expected Err(Stop::Refused), got: {other:?}"),
            }
        }
    }

    #[test]
    fn edition_compare_accepts_differing_browser_attestations() {
        let repo_a = fixture_repo();
        let repo_b = tempfile::tempdir().expect("tempdir b");
        copy_dir(repo_a.path(), repo_b.path());
        let mut stage_a = Scripted::with_browser("Microsoft Edge 130.0.2849.46");
        let pkg_a = edition(repo_a.path(), &mut stage_a, Some(CONTACT)).expect("package a");
        let mut stage_b = Scripted::with_browser("Microsoft Edge 131.0.2903.9");
        let pkg_b = edition(repo_b.path(), &mut stage_b, Some(CONTACT)).expect("package b");

        let tmp = tempfile::tempdir().expect("tempdir");
        let l = tmp.path().join("l");
        let r = tmp.path().join("r");
        write_files(&l, &pkg_a.files);
        write_files(&r, &pkg_b.files);

        let l_str = l.to_str().expect("utf8");
        let r_str = r.to_str().expect("utf8");
        let res = command_edition_compare(&["--left", l_str, "--right", r_str]);
        assert_eq!(res, Ok(true));
    }

    #[test]
    fn edition_compare_rejects_changed_content_byte() {
        let repo = fixture_repo();
        let (res, _) = run(repo.path());
        let pkg = res.expect("package");

        let tmp = tempfile::tempdir().expect("tempdir");
        let l = tmp.path().join("l");
        let r = tmp.path().join("r");
        write_files(&l, &pkg.files);

        let right_files = rebuilt_files(&pkg, true, |e, _| {
            let bytes = e.get_mut("about.html").expect("about.html");
            bytes[0] ^= 1;
        });
        write_files(&r, &right_files);

        let l_str = l.to_str().expect("utf8");
        let r_str = r.to_str().expect("utf8");
        let res = command_edition_compare(&["--left", l_str, "--right", r_str]);
        assert_eq!(res, Ok(false));

        let missing_dir = tmp.path().join("missing");
        write_files(&missing_dir, &pkg.files);
        fs::remove_file(missing_dir.join("corpus-support.sha256")).expect("remove sha256");
        let missing_str = missing_dir.to_str().expect("utf8");
        let res_missing = command_edition_compare(&["--left", missing_str, "--right", r_str]);
        assert_ne!(res_missing, Ok(true));
    }

    #[test]
    fn packaged_site_is_the_harness_input() {
        let repo = fixture_repo();
        let (res, stage) = run(repo.path());
        let pkg = res.expect("package");
        let (site, originals) = stage.seen_by_harness.expect("seen by harness");

        assert_eq!(stage.builds, 1);
        let call_verbs: Vec<&str> = stage
            .calls
            .iter()
            .filter(|c| !c.starts_with("git "))
            .map(|c| c.split_whitespace().next().unwrap_or_default())
            .collect();
        assert_eq!(call_verbs, vec!["check", "build", "harness"]);

        let (ed_tree, orig_tree) = trees(&pkg);

        for (k, v) in &site {
            assert_eq!(
                ed_tree.get(k),
                Some(v),
                "measured site file {k} differs in packaged edition"
            );
        }

        let added_paths: BTreeSet<&str> = [
            fieldbook_core::strings::PATHS[6],
            "README.txt",
            "TRANSFER-STATEMENT.txt",
            "SHA256SUMS",
            "evidence/corpus-support.toml",
            "intake/ai-development-statement.md",
            "intake/data-handling.md",
            "intake/integrity-guide.md",
            "intake/network-silence.md",
            "intake/network-silence-summary.txt",
        ]
        .into_iter()
        .collect();

        for key in ed_tree.keys() {
            if site.contains_key(key) {
                continue;
            }
            assert!(
                added_paths.contains(key.as_str()) || key.starts_with("corpus/"),
                "unexpected extra key in packaged edition tree: {key}"
            );
        }
        for req in &added_paths {
            assert!(
                ed_tree.contains_key(*req),
                "packaged edition missing added path {req}"
            );
        }

        for (k, v) in &originals {
            assert_eq!(
                orig_tree.get(k),
                Some(v),
                "measured original {k} differs in packaged originals"
            );
        }
        for key in orig_tree.keys() {
            assert!(
                originals.contains_key(key) || key == "SHA256SUMS",
                "unexpected extra key in packaged originals tree: {key}"
            );
        }
        assert!(
            orig_tree.contains_key("SHA256SUMS"),
            "packaged originals missing SHA256SUMS"
        );
    }

    #[test]
    fn harness_that_changes_a_measured_file_is_refused() {
        let repo = fixture_repo();
        let mut stage = Scripted::new();
        stage.changed_by_harness = Some(("about.html".to_owned(), b"changed\n".to_vec()));
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        let msg = refused(res);
        assert!(
            msg.contains("FB0608") && msg.contains("about.html"),
            "expected FB0608 and about.html, got: {msg}"
        );
        assert!(!repo.path().join("out/dist").exists(), "out/dist exists");
    }

    #[test]
    fn summary_edition_mismatch_is_refused() {
        let repo = fixture_repo();
        let mut stage = Scripted::new();
        stage.summary_edition = Some("edition-002".to_owned());
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        let msg = refused(res);
        assert!(msg.contains("IN-01"), "expected IN-01, got: {msg}");
        assert!(!repo.path().join("out/dist").exists(), "out/dist exists");
    }

    #[test]
    fn stale_harness_summary_is_refused() {
        let repo = fixture_repo();
        let (res1, _) = run(repo.path());
        assert!(res1.is_ok(), "first run must succeed");
        let summary_path = repo.path().join("out/harness/network-silence-summary.txt");
        assert!(summary_path.exists(), "summary exists");
        assert!(repo.path().join("out/dist").exists(), "out/dist exists");

        fs::remove_dir_all(repo.path().join("out/dist")).expect("remove out/dist");

        let mut stage2 = Scripted::new();
        stage2.harness_code = 1;
        stage2.write_summary = false;
        let res2 = edition(repo.path(), &mut stage2, Some(CONTACT));
        assert!(res2.is_err(), "stage 2 must fail");
        assert!(
            !repo.path().join("out/dist").exists(),
            "out/dist exists after stage 2"
        );
        assert!(
            !summary_path.exists(),
            "summary file still exists after stage 2"
        );

        let mut stage3 = Scripted::new();
        stage3.harness_code = 0;
        stage3.write_summary = false;
        let res3 = edition(repo.path(), &mut stage3, Some(CONTACT));
        let msg3 = refused(res3);
        assert!(msg3.contains("IN-01"), "expected IN-01, got: {msg3}");
        assert!(
            !repo.path().join("out/dist").exists(),
            "out/dist exists after stage 3"
        );
    }

    #[test]
    fn missing_heading_is_refused() {
        let repo = fixture_repo();
        write(
            repo.path(),
            "corpus/changes.md",
            b"# Corpus changes\n\nnothing\n",
        );
        commit_all(repo.path());
        let mut stage = Scripted::new();
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        let msg = refused(res);
        assert!(
            msg.contains("no `## edition-NNN` heading"),
            "expected missing heading refusal, got: {msg}"
        );
        assert!(!stage.called("check"), "check was called");
    }

    #[test]
    fn dirty_tree_is_refused() {
        let repo = fixture_repo();
        write(repo.path(), "stray.txt", b"x\n");
        let mut stage = Scripted::new();
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        assert!(matches!(res, Err(Stop::Refused(_))));
        assert!(!stage.called("build"));
    }

    #[test]
    fn check_findings_stop_before_build() {
        let repo = fixture_repo();
        let mut stage = Scripted::new();
        stage.check_code = 1;
        let res = edition(repo.path(), &mut stage, Some(CONTACT));
        match res {
            Err(_) => {}
            Ok(_) => panic!("expected Err"),
        }
        assert!(!stage.called("build"));
        assert!(!stage.called("harness"));
    }

    /// The child half of `two_runs_give_identical_zips`: it builds the fixture named by the
    /// environment and copies `out/dist` to the folder named by the environment. It does nothing
    /// when run by itself.
    #[test]
    #[ignore = "started by two_runs_give_identical_zips"]
    fn build_in_child() {
        let (Some(fixture), Some(out)) = (
            std::env::var_os("FIELDBOOK_TEST_FIXTURE"),
            std::env::var_os("FIELDBOOK_TEST_OUT"),
        ) else {
            return;
        };
        let (result, _) = run(Path::new(&fixture));
        let pkg = result.expect("child package");
        write_files(Path::new(&out), &pkg.files);
    }
}
