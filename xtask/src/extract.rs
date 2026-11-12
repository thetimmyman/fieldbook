//! Text extraction from stored originals and locator generation.
//!
//! **Responsibility.** Extract text pages and extraction records (`extraction.toml`)
//! from stored source originals, or attach supplied UTF-8 text.
//!
//! **Rules.** Follow repo rules: pure fieldbook-core types, no unchecked indexing,
//! respect size limit `MAX_ORIGINAL_BYTES`, no unwrap/expect/panic in non-test code.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use fieldbook_core::extraction::methods::{PDFTOTEXT_LAYOUT, PDFTOTEXT_LAYOUT_ID};
use fieldbook_core::extraction::{
    ExtractError, Extraction, ExtractionToml, Extractor, LocatorPages, MAX_ORIGINAL_BYTES,
    PageText, SUPPLIED_TOOL, SUPPLIED_TOOL_VERSION, build_record, page_file_name, select,
};
use fieldbook_core::ids::VersionId;

#[cfg(test)]
#[expect(
    clippy::duplicate_mod,
    reason = "shared extractor suite is included at file level per card spec"
)]
#[path = "../../tests/support/extractor_suite.rs"]
mod extractor_suite;
#[cfg(test)]
#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included at file level per card spec"
)]
#[path = "../../tests/support/rng.rs"]
mod rng;

/// PDF text extractor using the external `pdftotext` tool with layout preservation.
#[derive(Debug, Clone)]
pub struct PdftotextLayout {
    program: String,
}

impl PdftotextLayout {
    /// Construct a new PDF layout extractor using the default `pdftotext` binary.
    pub fn new() -> Self {
        Self {
            program: "pdftotext".to_owned(),
        }
    }

    /// Construct a PDF layout extractor using a specified executable name.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "tests build the extractor with another program name"
        )
    )]
    pub fn with_program(name: &str) -> Self {
        Self {
            program: name.to_owned(),
        }
    }

    /// Probe the version of the `pdftotext` executable.
    pub fn tool_version(&self) -> Result<String, ExtractError> {
        let output = Command::new(&self.program)
            .arg("-v")
            .stdin(Stdio::null())
            .output();
        let out = match output {
            Ok(o) => o,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Err(ExtractError::ToolMissing(self.program.clone()));
            }
            Err(_) => return Err(ExtractError::ToolFailed { status: -1 }),
        };
        let err_text = String::from_utf8_lossy(&out.stderr);
        let out_text = String::from_utf8_lossy(&out.stdout);
        let version = crate::tools::Version::find_in(&err_text)
            .or_else(|| crate::tools::Version::find_in(&out_text));
        match version {
            Some(v) => Ok(v.to_string()),
            None => Err(ExtractError::ToolFailed { status: 0 }),
        }
    }
}

impl Default for PdftotextLayout {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for PdftotextLayout {
    fn id(&self) -> &'static str {
        PDFTOTEXT_LAYOUT_ID
    }

    fn extract(&self, bytes: &[u8]) -> Result<Extraction, ExtractError> {
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if len > MAX_ORIGINAL_BYTES {
            return Err(ExtractError::TooLarge { bytes: len });
        }

        let mut child = Command::new(&self.program)
            .args(["-layout", "-enc", "UTF-8", "-", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| {
                if err.kind() == std::io::ErrorKind::NotFound {
                    ExtractError::ToolMissing(self.program.clone())
                } else {
                    ExtractError::ToolFailed { status: -1 }
                }
            })?;

        let output = std::thread::scope(|s| {
            let stdin = child.stdin.take();
            s.spawn(move || {
                if let Some(mut pipe) = stdin {
                    use std::io::Write;
                    let _ = pipe.write_all(bytes);
                }
            });
            child.wait_with_output()
        });

        let out = output.map_err(|_| ExtractError::ToolFailed { status: -1 })?;
        match out.status.code() {
            Some(0) => {}
            Some(code) => return Err(ExtractError::ToolFailed { status: code }),
            None => return Err(ExtractError::ToolFailed { status: -1 }),
        }

        let decoded = String::from_utf8_lossy(&out.stdout);
        let mut parts: Vec<&str> = decoded.split('\u{c}').collect();
        if parts.last().is_some_and(|p| p.is_empty()) {
            parts.pop();
        }
        if parts.is_empty() {
            return Err(ExtractError::ToolFailed { status: 0 });
        }

        let mut pages = Vec::with_capacity(parts.len());
        for (i, part) in parts.iter().enumerate() {
            let number = u32::try_from(i.saturating_add(1)).unwrap_or(u32::MAX);
            pages.push(PageText {
                number,
                text: (*part).to_owned(),
            });
        }

        Ok(Extraction {
            method: PDFTOTEXT_LAYOUT,
            pages,
        })
    }
}

/// Settings for extracting originals.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// PDF extractor instance.
    pub pdf: PdftotextLayout,
    /// Pinned poppler version, if available.
    pub pin: Option<String>,
}

/// The outcome of running an extraction command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Process exit status: 0 success, 1 failure, 3 tool missing.
    pub exit: u8,
    /// Standard output lines.
    pub out: Vec<String>,
    /// Standard error lines.
    pub err: Vec<String>,
}

struct OriginalFile {
    path: PathBuf,
    file_name: String,
    ext: String,
}

fn find_stored_original(root: &Path, id: &str) -> Result<OriginalFile, String> {
    let public_dir = root.join("corpus/sources/public");
    let Ok(read_dir) = std::fs::read_dir(&public_dir) else {
        return Err(format!(
            "{id}: no stored original under corpus/sources/public"
        ));
    };

    let mut entries: Vec<std::fs::DirEntry> = read_dir.flatten().collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if let Some((stem, ext)) = file_name.split_once('.')
            && stem == id
        {
            let ext = ext.to_owned();
            return Ok(OriginalFile {
                path: entry.path(),
                file_name,
                ext,
            });
        }
    }

    Err(format!(
        "{id}: no stored original under corpus/sources/public"
    ))
}

fn read_kept_locators(
    toml_path: &Path,
    id: &str,
) -> Result<BTreeMap<String, LocatorPages>, String> {
    if !toml_path.exists() {
        return Ok(BTreeMap::new());
    }
    let bytes = std::fs::read(toml_path)
        .map_err(|err| format!("{id}: the existing extraction.toml cannot be read (1:1: {err})"))?;
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        format!("{id}: the existing extraction.toml cannot be read (1:1: not valid UTF-8 text)")
    })?;
    let record = ExtractionToml::parse(text)
        .map_err(|err| format!("{id}: the existing extraction.toml cannot be read ({err})"))?;
    Ok(record.locators)
}

fn write_file(path: &Path, content: &[u8]) -> Result<(), String> {
    std::fs::write(path, content).map_err(|err| err.to_string())
}

fn is_page_file(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('p').and_then(|s| s.strip_suffix(".txt")) else {
        return false;
    };
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
}

fn write_pages_and_record(
    text_dir: &Path,
    id: &str,
    extraction: &Extraction,
    record: &ExtractionToml,
) -> Result<(), String> {
    std::fs::create_dir_all(text_dir)
        .map_err(|err| format!("{id}: cannot write {}: {err}", text_dir.display()))?;

    let mut new_page_names = BTreeSet::new();
    for page in &extraction.pages {
        let fname = page_file_name(page.number);
        let page_path = text_dir.join(&fname);
        write_file(&page_path, page.text.as_bytes())
            .map_err(|err| format!("{id}: cannot write {}: {err}", page_path.display()))?;
        new_page_names.insert(fname);
    }

    if let Ok(entries) = std::fs::read_dir(text_dir) {
        for entry in entries.flatten() {
            let fname_os = entry.file_name();
            let fname = fname_os.to_string_lossy();
            if is_page_file(&fname) && !new_page_names.contains(fname.as_ref()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let toml_path = text_dir.join("extraction.toml");
    let rendered = record.render();
    write_file(&toml_path, rendered.as_bytes())
        .map_err(|err| format!("{id}: cannot write {}: {err}", toml_path.display()))?;

    Ok(())
}

enum OneOutcome {
    Success,
    Failure,
    ToolMissing(String),
}

fn extract_one(
    root: &Path,
    id: &str,
    settings: &Settings,
    out: &mut Vec<String>,
    err: &mut Vec<String>,
) -> OneOutcome {
    if VersionId::parse(id).is_err() {
        err.push(format!("{id}: not a version id"));
        return OneOutcome::Failure;
    }

    let orig = match find_stored_original(root, id) {
        Ok(f) => f,
        Err(e) => {
            err.push(e);
            return OneOutcome::Failure;
        }
    };

    let meta = match std::fs::metadata(&orig.path) {
        Ok(m) => m,
        Err(e) => {
            err.push(format!("{}: cannot read metadata: {e}", orig.file_name));
            return OneOutcome::Failure;
        }
    };
    let meta_len = meta.len();
    if meta_len > MAX_ORIGINAL_BYTES {
        err.push(format!(
            "{}: {}",
            orig.file_name,
            ExtractError::TooLarge { bytes: meta_len }
        ));
        return OneOutcome::Failure;
    }

    let bytes = match std::fs::read(&orig.path) {
        Ok(b) => b,
        Err(e) => {
            err.push(format!("{}: cannot read: {e}", orig.file_name));
            return OneOutcome::Failure;
        }
    };

    let lower_ext = orig.ext.to_ascii_lowercase();
    let head_len = 512.min(bytes.len());
    let head = bytes.get(..head_len).unwrap_or(&[]);

    let (extractor, is_pdf): (&dyn Extractor, bool) = if let Some(extor) = select(&lower_ext, head)
    {
        (extor, false)
    } else if lower_ext == "pdf" {
        (&settings.pdf, true)
    } else {
        err.push(format!(
            "{}: no extractor for this type; attach text with --source VERSION --text FILE",
            orig.file_name
        ));
        return OneOutcome::Failure;
    };

    let extraction = match extractor.extract(&bytes) {
        Ok(e) => e,
        Err(ExtractError::ToolMissing(name)) => return OneOutcome::ToolMissing(name),
        Err(e) => {
            err.push(format!("{}: {e}", orig.file_name));
            return OneOutcome::Failure;
        }
    };

    let (tool, tool_version) = if is_pdf {
        let ver = match settings.pdf.tool_version() {
            Ok(v) => v,
            Err(ExtractError::ToolMissing(name)) => return OneOutcome::ToolMissing(name),
            Err(e) => {
                err.push(format!("{}: {e}", orig.file_name));
                return OneOutcome::Failure;
            }
        };
        if let Some(pin) = &settings.pin
            && let crate::tools::PinCheck::Differs(msg) = crate::tools::poppler_pin_check(pin, &ver)
        {
            err.push(format!("warning: {msg}"));
        }
        ("pdftotext".to_owned(), ver)
    } else {
        let (name, ver) = extractor
            .id()
            .split_once('@')
            .unwrap_or((extractor.id(), "1"));
        (name.to_owned(), ver.to_owned())
    };

    let original_sha256 = fieldbook_core::hash::sha256_hex(&bytes);

    let text_dir = root.join("corpus/sources/text").join(id);
    let toml_path = text_dir.join("extraction.toml");
    let kept = match read_kept_locators(&toml_path, id) {
        Ok(k) => k,
        Err(e) => {
            err.push(e);
            return OneOutcome::Failure;
        }
    };

    let record = build_record(&extraction, &tool, &tool_version, &original_sha256, &kept);

    if let Err(e) = write_pages_and_record(&text_dir, id, &extraction, &record) {
        err.push(e);
        return OneOutcome::Failure;
    }

    let n = extraction.pages.len();
    let t = record.thin_pages.len();
    let page_word = if n == 1 { "page" } else { "pages" };
    out.push(format!("{id}: {n} {page_word}, {t} with little or no text"));
    if !record.thin_pages.is_empty() {
        let thin_list = record
            .thin_pages
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        out.push(format!("{id}: thin pages {thin_list}; read the original"));
    }

    OneOutcome::Success
}

/// Extract text and record metadata for a list of source versions.
pub fn run(root: &Path, versions: &[&str], settings: &Settings) -> Outcome {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let mut has_failure = false;

    for version in versions {
        match extract_one(root, version, settings, &mut out, &mut err) {
            OneOutcome::Success => {}
            OneOutcome::Failure => {
                has_failure = true;
            }
            OneOutcome::ToolMissing(name) => {
                err.push(format!("error: {name} is not installed"));
                return Outcome { exit: 3, out, err };
            }
        }
    }

    let exit = if has_failure { 1 } else { 0 };
    Outcome { exit, out, err }
}

/// Attach supplied text to a stored original version.
pub fn run_supplied(root: &Path, version: &str, text_file: &Path) -> Outcome {
    let mut out = Vec::new();
    let mut err = Vec::new();

    if VersionId::parse(version).is_err() {
        err.push(format!("{version}: not a version id"));
        return Outcome { exit: 1, out, err };
    }

    let orig = match find_stored_original(root, version) {
        Ok(f) => f,
        Err(e) => {
            err.push(e);
            return Outcome { exit: 1, out, err };
        }
    };

    let orig_meta = match std::fs::metadata(&orig.path) {
        Ok(m) => m,
        Err(e) => {
            err.push(format!("{}: cannot read metadata: {e}", orig.file_name));
            return Outcome { exit: 1, out, err };
        }
    };
    let orig_len = orig_meta.len();
    if orig_len > MAX_ORIGINAL_BYTES {
        err.push(format!(
            "{}: {}",
            orig.file_name,
            ExtractError::TooLarge { bytes: orig_len }
        ));
        return Outcome { exit: 1, out, err };
    }

    let orig_bytes = match std::fs::read(&orig.path) {
        Ok(b) => b,
        Err(e) => {
            err.push(format!("{}: cannot read: {e}", orig.file_name));
            return Outcome { exit: 1, out, err };
        }
    };
    let original_sha256 = fieldbook_core::hash::sha256_hex(&orig_bytes);

    let text_bytes = match std::fs::read(text_file) {
        Ok(b) => b,
        Err(e) => {
            err.push(format!("{}: cannot read: {e}", text_file.display()));
            return Outcome { exit: 1, out, err };
        }
    };

    let text_file_name = text_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_else(|| text_file.to_str().unwrap_or(""));

    let extraction = match fieldbook_core::extraction::supplied_text(&text_bytes) {
        Ok(ext) => ext,
        Err(e) => {
            err.push(format!("{text_file_name}: {e}"));
            return Outcome { exit: 1, out, err };
        }
    };

    for page in &extraction.pages {
        let findings = fieldbook_core::scan::marking::scan(&page.text);
        if let Some(first) = findings.first() {
            err.push(format!(
                "{version}: the supplied text holds a marking banner (`{}`); nothing written",
                first.matched
            ));
            return Outcome { exit: 1, out, err };
        }
    }

    let text_dir = root.join("corpus/sources/text").join(version);
    let toml_path = text_dir.join("extraction.toml");
    let kept = match read_kept_locators(&toml_path, version) {
        Ok(k) => k,
        Err(e) => {
            err.push(e);
            return Outcome { exit: 1, out, err };
        }
    };

    let record = build_record(
        &extraction,
        SUPPLIED_TOOL,
        SUPPLIED_TOOL_VERSION,
        &original_sha256,
        &kept,
    );

    if let Err(e) = write_pages_and_record(&text_dir, version, &extraction, &record) {
        err.push(e);
        return Outcome { exit: 1, out, err };
    }

    let n = extraction.pages.len();
    let t = record.thin_pages.len();
    let page_word = if n == 1 { "page" } else { "pages" };
    out.push(format!(
        "{version}: {n} supplied {page_word}, {t} with little or no text; the text is not checked against the original"
    ));

    Outcome { exit: 0, out, err }
}

fn handle_outcome(outcome: &Outcome) -> Result<bool, String> {
    for line in &outcome.out {
        println!("{line}");
    }
    match outcome.exit {
        0 => {
            for line in &outcome.err {
                eprintln!("{line}");
            }
            Ok(true)
        }
        1 => {
            for line in &outcome.err {
                eprintln!("{line}");
            }
            Ok(false)
        }
        3 => {
            let last_idx = outcome.err.len().saturating_sub(1);
            for line in outcome.err.iter().take(last_idx) {
                eprintln!("{line}");
            }
            let last_err = outcome
                .err
                .last()
                .map(String::as_str)
                .unwrap_or("extraction failed");
            let msg = last_err.strip_prefix("error: ").unwrap_or(last_err);
            Err(msg.to_owned())
        }
        other => Err(format!("unexpected exit code {other}")),
    }
}

/// The `cargo xtask extract VERSION...` entry point.
pub fn command(root: &Path, versions: &[&str]) -> Result<bool, String> {
    let settings = Settings {
        pdf: PdftotextLayout::new(),
        pin: crate::tools::poppler_pin(root).ok(),
    };
    let outcome = run(root, versions, &settings);
    handle_outcome(&outcome)
}

/// The `cargo xtask extract --source VERSION --text FILE` entry point.
pub fn command_supplied(root: &Path, version: &str, text_file: &Path) -> Result<bool, String> {
    let outcome = run_supplied(root, version, text_file);
    handle_outcome(&outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fieldbook_core::extraction::methods::{HTML_STRIP, PDFTOTEXT_LAYOUT, PLAIN};
    use fieldbook_core::extraction::{
        ExtractError, LocatorPages, MAX_ORIGINAL_BYTES, WeakEvidence,
    };
    use std::path::{Path, PathBuf};

    fn corpus_with(vid: &str, ext: &str, bytes: &[u8]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let public_dir = tmp.path().join("corpus/sources/public");
        std::fs::create_dir_all(&public_dir).expect("create_dir_all");
        let file_path = public_dir.join(format!("{vid}.{ext}"));
        std::fs::write(&file_path, bytes).expect("write original");
        tmp
    }

    fn settings() -> Settings {
        Settings {
            pdf: PdftotextLayout::new(),
            pin: None,
        }
    }

    fn text_dir(root: &Path, vid: &str) -> PathBuf {
        root.join("corpus/sources/text").join(vid)
    }

    fn read_record(root: &Path, vid: &str) -> ExtractionToml {
        let path = text_dir(root, vid).join("extraction.toml");
        let text = std::fs::read_to_string(&path).expect("read extraction.toml");
        ExtractionToml::parse(&text).expect("parse extraction.toml")
    }

    fn long_text() -> Vec<u8> {
        b"Synthetic section text for the extraction tests, long enough that no page is thin.\n\
Synthetic section text for the extraction tests, long enough that no page is thin.\n"
            .to_vec()
    }

    #[test]
    fn plain_text_is_copied() {
        let orig_bytes = long_text();
        let tmp = corpus_with("S-0001-v1", "txt", &orig_bytes);
        let orig_path = tmp.path().join("corpus/sources/public/S-0001-v1.txt");

        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 0);

        let p0001_path = text_dir(tmp.path(), "S-0001-v1").join("p0001.txt");
        let p0001_bytes = std::fs::read(&p0001_path).expect("read p0001.txt");
        assert_eq!(p0001_bytes, orig_bytes);

        let toml_path = text_dir(tmp.path(), "S-0001-v1").join("extraction.toml");
        let raw_toml = std::fs::read_to_string(&toml_path).expect("read extraction.toml");
        assert!(raw_toml.lines().any(|l| l == "thin_pages = []"));

        let record = read_record(tmp.path(), "S-0001-v1");
        assert_eq!(record.method.name(), "plain");
        assert_eq!(record.tool, "plain");
        assert_eq!(record.tool_version, "1");
        assert_eq!(record.page_count, 1);
        assert!(record.thin_pages.is_empty());
        assert_eq!(
            record.original_sha256,
            fieldbook_core::hash::sha256_hex(&orig_bytes)
        );
        assert_eq!(
            record.pages.get("p0001").map(String::as_str),
            Some(fieldbook_core::hash::sha256_hex(&p0001_bytes).as_str())
        );

        let current_orig = std::fs::read(&orig_path).expect("read original");
        assert_eq!(current_orig, orig_bytes);
    }

    #[test]
    fn html_is_stripped() {
        let sample_html = include_bytes!("../../tests/fixtures/extract/html-strip/sample.html");
        let expected_txt =
            include_str!("../../tests/fixtures/extract/html-strip/sample.expected.txt");

        let tmp = corpus_with("S-0001-v1", "html", sample_html);
        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 0);

        let p0001_path = text_dir(tmp.path(), "S-0001-v1").join("p0001.txt");
        let p0001_str = std::fs::read_to_string(&p0001_path).expect("read p0001.txt");
        assert_eq!(p0001_str, expected_txt);

        let record = read_record(tmp.path(), "S-0001-v1");
        assert_eq!(record.method.name(), "html-strip");
    }

    #[test]
    fn rerun_keeps_hand_locators() {
        let orig_text = format!(
            "Section 3 Synthetic scope\n{}",
            String::from_utf8(long_text()).expect("utf8")
        );
        let tmp = corpus_with("S-0001-v1", "txt", orig_text.as_bytes());

        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 0);

        let record = read_record(tmp.path(), "S-0001-v1");
        assert_eq!(
            record.locators.get("3"),
            Some(&LocatorPages { first: 1, last: 1 })
        );

        let mut mod_record = record.clone();
        mod_record
            .locators
            .insert("Hand (a)".to_owned(), LocatorPages { first: 1, last: 1 });
        mod_record
            .locators
            .insert("3".to_owned(), LocatorPages { first: 1, last: 2 });
        let toml_path = text_dir(tmp.path(), "S-0001-v1").join("extraction.toml");
        std::fs::write(&toml_path, mod_record.render()).expect("write rendered toml");

        let outcome2 = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome2.exit, 0);

        let second_record = read_record(tmp.path(), "S-0001-v1");
        assert_eq!(
            second_record.locators.get("Hand (a)"),
            Some(&LocatorPages { first: 1, last: 1 })
        );
        assert_eq!(
            second_record.locators.get("3"),
            Some(&LocatorPages { first: 1, last: 2 })
        );

        let p0001_path = text_dir(tmp.path(), "S-0001-v1").join("p0001.txt");
        let p0001_str = std::fs::read_to_string(&p0001_path).expect("read p0001.txt");
        assert_eq!(p0001_str, orig_text);
    }

    #[test]
    fn same_input_same_output() {
        let bytes = long_text();
        let tmp1 = corpus_with("S-0001-v1", "txt", &bytes);
        let tmp2 = corpus_with("S-0001-v1", "txt", &bytes);

        let outcome1 = run(tmp1.path(), &["S-0001-v1"], &settings());
        let outcome2 = run(tmp2.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome1.exit, 0);
        assert_eq!(outcome2.exit, 0);

        let p0001_1 = std::fs::read(text_dir(tmp1.path(), "S-0001-v1").join("p0001.txt"))
            .expect("read p0001");
        let p0001_2 = std::fs::read(text_dir(tmp2.path(), "S-0001-v1").join("p0001.txt"))
            .expect("read p0001");
        assert_eq!(p0001_1, p0001_2);

        let toml1 = std::fs::read(text_dir(tmp1.path(), "S-0001-v1").join("extraction.toml"))
            .expect("read toml");
        let toml2 = std::fs::read(text_dir(tmp2.path(), "S-0001-v1").join("extraction.toml"))
            .expect("read toml");
        assert_eq!(toml1, toml2);

        let outcome3 = run(tmp1.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome3.exit, 0);

        let p0001_3 = std::fs::read(text_dir(tmp1.path(), "S-0001-v1").join("p0001.txt"))
            .expect("read p0001");
        let toml3 = std::fs::read(text_dir(tmp1.path(), "S-0001-v1").join("extraction.toml"))
            .expect("read toml");
        assert_eq!(p0001_1, p0001_3);
        assert_eq!(toml1, toml3);
    }

    #[test]
    fn not_text_is_refused() {
        let tmp = corpus_with("S-0001-v1", "txt", &[0xff, 0xfe, b'a', 0x80]);
        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 1);
        assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
        assert!(
            outcome
                .err
                .iter()
                .any(|line| line.contains("S-0001-v1.txt") && line.contains("UTF-8"))
        );
    }

    #[test]
    fn too_large_is_refused() {
        assert_eq!(MAX_ORIGINAL_BYTES, 33_554_432);

        {
            let bytes = vec![b'a'; 33_554_432];
            let tmp = corpus_with("S-0001-v1", "txt", &bytes);
            drop(bytes);
            let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
            assert_eq!(outcome.exit, 0);
            assert!(text_dir(tmp.path(), "S-0001-v1").exists());
        }

        {
            let bytes = vec![b'a'; 33_554_433];
            let tmp = corpus_with("S-0001-v1", "txt", &bytes);
            drop(bytes);
            let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
            assert_eq!(outcome.exit, 1);
            assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
            assert!(outcome.err.iter().any(|line| line.contains("33554433")));
        }

        {
            let bytes = vec![b'a'; 33_554_433];
            let tmp = corpus_with("S-0001-v1", "pdf", &bytes);
            drop(bytes);
            let s = Settings {
                pdf: PdftotextLayout::with_program("fieldbook-no-such-program"),
                pin: None,
            };
            let outcome = run(tmp.path(), &["S-0001-v1"], &s);
            assert_eq!(outcome.exit, 1);
            assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
            assert!(outcome.err.iter().any(|line| line.contains("33554433")));
        }
    }

    #[test]
    fn a_missing_tool_is_exit_3() {
        let extractor = PdftotextLayout::with_program("fieldbook-no-such-program");
        assert_eq!(
            extractor.extract(b"%PDF-1.4"),
            Err(ExtractError::ToolMissing(
                "fieldbook-no-such-program".to_owned()
            ))
        );

        let tmp = corpus_with("S-0001-v1", "pdf", b"%PDF-1.4");
        let s = Settings {
            pdf: extractor,
            pin: None,
        };
        let outcome = run(tmp.path(), &["S-0001-v1"], &s);
        assert_eq!(outcome.exit, 3);
        assert!(
            outcome
                .err
                .iter()
                .any(|line| line.contains("fieldbook-no-such-program"))
        );
        assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
    }

    #[test]
    #[cfg(unix)]
    fn a_failing_tool_names_the_status() {
        let extractor = PdftotextLayout::with_program("false");
        assert_eq!(
            extractor.extract(b"%PDF-1.4"),
            Err(ExtractError::ToolFailed { status: 1 })
        );

        let tmp = corpus_with("S-0001-v1", "pdf", b"%PDF-1.4");
        let s = Settings {
            pdf: extractor,
            pin: None,
        };
        let outcome = run(tmp.path(), &["S-0001-v1"], &s);
        assert_eq!(outcome.exit, 1);
        assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
        assert!(
            outcome
                .err
                .iter()
                .any(|line| line.contains("S-0001-v1.pdf") && line.contains("status 1"))
        );
    }

    #[test]
    fn html_with_no_text_is_thin() {
        let tmp = corpus_with("S-0001-v1", "html", b"<html><body></body></html>");
        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 0);

        let record = read_record(tmp.path(), "S-0001-v1");
        assert_eq!(record.page_count, 1);
        assert_eq!(record.thin_pages, vec![1]);
        assert!(outcome.out.iter().any(|line| line.contains("thin pages")));
    }

    #[test]
    fn exit_is_one_after_a_refusal_and_the_rest_are_still_written() {
        let tmp = corpus_with("S-0001-v1", "txt", &[0xff, 0xfe, b'a', 0x80]);
        let public_dir = tmp.path().join("corpus/sources/public");
        std::fs::write(public_dir.join("S-0002-v1.txt"), long_text()).expect("write S-0002-v1");

        let outcome = run(tmp.path(), &["S-0001-v1", "S-0002-v1"], &settings());
        assert_eq!(outcome.exit, 1);

        assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
        assert!(text_dir(tmp.path(), "S-0002-v1").exists());
        assert!(text_dir(tmp.path(), "S-0002-v1").join("p0001.txt").exists());
    }

    #[test]
    fn a_type_with_no_extractor_names_the_fallback() {
        let tmp = corpus_with("S-0001-v1", "xlsx", b"synthetic xlsx content");
        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 1);
        assert!(outcome.err.iter().any(|line| line.contains("--source")));
    }

    #[test]
    fn a_missing_original_is_exit_1() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 1);
        assert!(
            outcome
                .err
                .iter()
                .any(|line| line.contains("no stored original"))
        );
    }

    #[test]
    fn a_bad_version_id_is_exit_1() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outcome = run(tmp.path(), &["not-a-valid-version-id"], &settings());
        assert_eq!(outcome.exit, 1);
        assert!(
            outcome
                .err
                .iter()
                .any(|line| line.contains("not a version id"))
        );
    }

    #[test]
    fn stale_pages_are_removed() {
        let tmp = corpus_with("S-0001-v1", "txt", &long_text());
        let tdir = text_dir(tmp.path(), "S-0001-v1");
        std::fs::create_dir_all(&tdir).expect("create_dir_all");
        let p0009 = tdir.join("p0009.txt");
        let notes = tdir.join("notes.md");
        std::fs::write(&p0009, b"stale page").expect("write p0009");
        std::fs::write(&notes, b"# Notes").expect("write notes.md");

        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 0);

        assert!(!p0009.exists(), "p0009.txt must be removed");
        assert!(notes.exists(), "notes.md must remain");
        assert!(tdir.join("p0001.txt").exists(), "p0001.txt must be written");
    }

    #[test]
    fn supplied_text_is_attached_to_any_original_type() {
        let exts = [
            "xls", "xml", "json", "yaml", "pdf", "eml", "msg", "doc", "docx", "txt", "md",
        ];
        for ext in exts {
            let orig_bytes = format!("synthetic {ext} original bytes\n").into_bytes();
            let tmp = corpus_with("S-0001-v1", ext, &orig_bytes);
            let orig_path = tmp
                .path()
                .join(format!("corpus/sources/public/S-0001-v1.{ext}"));

            let mut supplied_content = long_text();
            supplied_content.extend_from_slice(b"[page break]\nshort\n");
            let supplied_path = tmp.path().join("supplied.txt");
            std::fs::write(&supplied_path, &supplied_content).expect("write supplied.txt");

            let outcome = run_supplied(tmp.path(), "S-0001-v1", &supplied_path);
            assert_eq!(outcome.exit, 0);

            let record = read_record(tmp.path(), "S-0001-v1");
            assert_eq!(record.method.name(), "supplied");
            assert_eq!(record.tool, "supplied");
            assert_eq!(record.tool_version, "1");
            assert_eq!(record.page_count, 2);
            assert_eq!(record.thin_pages, vec![2]);
            assert_eq!(
                record.original_sha256,
                fieldbook_core::hash::sha256_hex(&orig_bytes)
            );

            let current_orig = std::fs::read(&orig_path).expect("read original");
            assert_eq!(current_orig, orig_bytes);

            let location = format!("S-0001-v1.{ext}");
            assert_eq!(
                fieldbook_core::extraction::weak_evidence(&location, Some(&record)),
                Some(WeakEvidence::Supplied)
            );

            let mut modified_bytes = orig_bytes.clone();
            if let Some(first_byte) = modified_bytes.first_mut() {
                *first_byte ^= 0x01;
            }
            std::fs::write(&orig_path, &modified_bytes).expect("write modified original");
            let new_hash = fieldbook_core::hash::sha256_hex(&modified_bytes);
            assert_ne!(new_hash, record.original_sha256);
            let second_record = read_record(tmp.path(), "S-0001-v1");
            assert_eq!(second_record, record);
        }
    }

    #[test]
    fn supplied_text_with_a_marking_banner_is_refused() {
        let tmp = corpus_with("S-0001-v1", "txt", &long_text());
        let text_path = tmp.path().join("supplied.txt");
        std::fs::write(
            &text_path,
            b"NOFORN synthetic marking\nSynthetic body text\n",
        )
        .expect("write text");

        let outcome = run_supplied(tmp.path(), "S-0001-v1", &text_path);
        assert_eq!(outcome.exit, 1);
        assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
    }

    #[test]
    fn supplied_text_that_is_not_utf8_is_refused() {
        let tmp = corpus_with("S-0001-v1", "txt", &long_text());
        let text_path = tmp.path().join("supplied.txt");
        std::fs::write(&text_path, [0xff, 0xfe, b'a', 0x80]).expect("write text");

        let outcome = run_supplied(tmp.path(), "S-0001-v1", &text_path);
        assert_eq!(outcome.exit, 1);
        assert!(!text_dir(tmp.path(), "S-0001-v1").exists());
    }

    #[test]
    fn supplied_text_needs_a_stored_original() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let text_path = tmp.path().join("supplied.txt");
        std::fs::write(&text_path, long_text()).expect("write text");

        let outcome = run_supplied(tmp.path(), "S-0001-v1", &text_path);
        assert_eq!(outcome.exit, 1);
    }

    #[test]
    fn supplied_crlf_and_bom_are_normalised() {
        let tmp = corpus_with("S-0001-v1", "txt", &long_text());
        let text_path = tmp.path().join("supplied.txt");
        let mut text_bytes = vec![0xEF, 0xBB, 0xBF];
        text_bytes
            .extend_from_slice(b"Synthetic line 1 with CRLF\r\nSynthetic line 2 with CRLF\r\n");
        std::fs::write(&text_path, &text_bytes).expect("write text");

        let outcome = run_supplied(tmp.path(), "S-0001-v1", &text_path);
        assert_eq!(outcome.exit, 0);

        let p0001_path = text_dir(tmp.path(), "S-0001-v1").join("p0001.txt");
        let p0001_bytes = std::fs::read(&p0001_path).expect("read p0001.txt");
        assert!(!p0001_bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
        assert!(!p0001_bytes.contains(&b'\r'));
        assert_eq!(
            p0001_bytes,
            b"Synthetic line 1 with CRLF\nSynthetic line 2 with CRLF\n"
        );
    }

    #[test]
    #[ignore = "needs pdftotext"]
    fn pdf_gives_one_file_per_page() {
        let pdf_bytes = include_bytes!("../tests/fixtures/synthetic-two-pages.pdf");
        let tmp = corpus_with("S-0001-v1", "pdf", pdf_bytes);

        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 0);

        let tdir = text_dir(tmp.path(), "S-0001-v1");
        let mut entries: Vec<String> = std::fs::read_dir(&tdir)
            .expect("read_dir")
            .filter_map(|e| {
                e.ok()
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
            })
            .collect();
        entries.sort();
        assert_eq!(entries, vec!["extraction.toml", "p0001.txt", "p0002.txt"]);

        let record = read_record(tmp.path(), "S-0001-v1");
        assert_eq!(record.tool, "pdftotext");
        assert_eq!(record.method.name(), "pdftotext-layout");
        assert_eq!(record.page_count, 2);
        assert!(record.thin_pages.is_empty());

        let output = std::process::Command::new("pdftotext")
            .arg("-v")
            .output()
            .expect("run pdftotext -v");
        let err_text = String::from_utf8_lossy(&output.stderr);
        let out_text = String::from_utf8_lossy(&output.stdout);
        let expected_version = crate::tools::Version::find_in(&err_text)
            .or_else(|| crate::tools::Version::find_in(&out_text))
            .expect("find version")
            .to_string();
        assert_eq!(record.tool_version, expected_version);
    }

    #[test]
    #[ignore = "needs pdftotext"]
    fn image_only_page_is_listed() {
        let pdf_bytes = include_bytes!("../tests/fixtures/synthetic-blank-page.pdf");
        let tmp = corpus_with("S-0001-v1", "pdf", pdf_bytes);

        let outcome = run(tmp.path(), &["S-0001-v1"], &settings());
        assert_eq!(outcome.exit, 0);

        let record = read_record(tmp.path(), "S-0001-v1");
        assert_eq!(record.page_count, 2);
        assert_eq!(record.thin_pages, vec![2]);

        let p0002_path = text_dir(tmp.path(), "S-0001-v1").join("p0002.txt");
        let p0002_text = std::fs::read_to_string(&p0002_path).expect("read p0002.txt");
        assert!(p0002_text.trim().is_empty());
    }

    #[test]
    #[ignore = "needs pdftotext"]
    fn pdf_extracts_when_the_version_differs() {
        let pdf_bytes = include_bytes!("../tests/fixtures/synthetic-two-pages.pdf");
        let tmp = corpus_with("S-0001-v1", "pdf", pdf_bytes);

        let config_dir = tmp.path().join(".config");
        std::fs::create_dir_all(&config_dir).expect("create .config");
        std::fs::write(
            config_dir.join("tools.toml"),
            b"[poppler]\nversion = \"0.0.0\"\n",
        )
        .expect("write tools.toml");

        let s = Settings {
            pdf: PdftotextLayout::new(),
            pin: crate::tools::poppler_pin(tmp.path()).ok(),
        };
        let outcome = run(tmp.path(), &["S-0001-v1"], &s);
        assert_eq!(outcome.exit, 0);

        let record = read_record(tmp.path(), "S-0001-v1");
        let installed_version = s.pdf.tool_version().expect("installed tool_version");
        assert_eq!(record.tool_version, installed_version);

        let warning_line = outcome
            .err
            .iter()
            .find(|line| line.starts_with("warning:"))
            .expect("warning line in err");
        assert!(warning_line.contains("0.0.0"));
        assert!(warning_line.contains(&installed_version));
    }

    #[test]
    fn extractor_contract_plain() {
        super::extractor_suite::extractor_contract(
            &fieldbook_core::extraction::Plain,
            &super::extractor_suite::text_vectors(),
            &|e| {
                if e.method == PLAIN {
                    Ok(())
                } else {
                    Err(format!("unexpected method {}", e.method))
                }
            },
        );
    }

    #[test]
    fn extractor_contract_html_strip() {
        super::extractor_suite::extractor_contract(
            &fieldbook_core::extraction::HtmlStrip,
            &super::extractor_suite::text_vectors(),
            &|e| {
                if e.method == HTML_STRIP {
                    Ok(())
                } else {
                    Err(format!("unexpected method {}", e.method))
                }
            },
        );
    }

    #[test]
    fn extractor_contract_missing_tool() {
        let extractor = PdftotextLayout::with_program("fieldbook-no-such-program");
        let vectors = [super::extractor_suite::Vector {
            name: "any input",
            input: b"%PDF-1.4".to_vec(),
            expect: super::extractor_suite::Expect::Refused("ToolMissing"),
        }];
        super::extractor_suite::extractor_contract(&extractor, &vectors, &|_| {
            Err("validator should not be called".to_owned())
        });
    }

    #[test]
    #[cfg(unix)]
    fn extractor_contract_failing_tool() {
        let extractor = PdftotextLayout::with_program("false");
        let vectors = [super::extractor_suite::Vector {
            name: "any input",
            input: b"%PDF-1.4".to_vec(),
            expect: super::extractor_suite::Expect::Refused("ToolFailed"),
        }];
        super::extractor_suite::extractor_contract(&extractor, &vectors, &|_| {
            Err("validator should not be called".to_owned())
        });
    }

    #[test]
    #[ignore = "needs pdftotext"]
    fn extractor_contract_pdftotext() {
        let extractor = PdftotextLayout::new();
        let vectors = [
            super::extractor_suite::Vector {
                name: "empty bytes",
                input: Vec::new(),
                expect: super::extractor_suite::Expect::Refused("ToolFailed"),
            },
            super::extractor_suite::Vector {
                name: "not a pdf",
                input: b"not a pdf".to_vec(),
                expect: super::extractor_suite::Expect::Refused("ToolFailed"),
            },
            super::extractor_suite::Vector {
                name: "synthetic blank page",
                input: include_bytes!("../tests/fixtures/synthetic-blank-page.pdf").to_vec(),
                expect: super::extractor_suite::Expect::Pages {
                    pages: 2,
                    thin: vec![2],
                },
            },
            super::extractor_suite::Vector {
                name: "synthetic two pages",
                input: include_bytes!("../tests/fixtures/synthetic-two-pages.pdf").to_vec(),
                expect: super::extractor_suite::Expect::Pages {
                    pages: 2,
                    thin: vec![],
                },
            },
        ];
        super::extractor_suite::extractor_contract(&extractor, &vectors, &|e| {
            if e.method == PDFTOTEXT_LAYOUT {
                Ok(())
            } else {
                Err(format!("unexpected method {}", e.method))
            }
        });
    }

    #[test]
    fn pdf_extractor_survives_seeded_bytes() {
        let mut rng = super::rng::Rng::new(1);
        let extractor = PdftotextLayout::with_program("fieldbook-no-such-program");
        for i in 0..20 {
            let len = usize::try_from(rng.next_u64() % 257).unwrap_or(0);
            let bytes = rng.bytes(len);
            let res = extractor.extract(&bytes);
            assert_eq!(
                res,
                Err(ExtractError::ToolMissing(
                    "fieldbook-no-such-program".to_owned()
                )),
                "iteration {i} failed"
            );
        }
    }
}
