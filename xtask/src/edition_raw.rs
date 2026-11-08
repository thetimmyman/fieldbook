//! `cargo xtask edition-raw --out DIR`: the paired public raw edition and originals archives.
//!
//! The raw edition is the floor delivery: the shipped public Corpus, the regression set, the
//! intake pack and the clean strict-check evidence, with no generated site, no script, no
//! browser evidence and no executable (`docs/runbooks/release.md#raw-edition-floor`). Nothing in
//! this module starts a renderer, a site build or a browser. The only programs it runs, through
//! the [`Runner`], are `git`, the built `fieldbook` command-line program's `check` verb, and
//! `cargo run` for that same verb when no prebuilt program is given with `--fieldbook`.
//!
//! Everything is built in memory from one clean commit, verified against the Raw allowlist
//! profile and written to a private staging folder before any file reaches `--out`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Options, load};
use fieldbook_core::edition::{BuildInfo, BuildProfile, EditionInfo, latest_edition_heading};
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check as egress_check};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::{EditionId, VersionId};
use fieldbook_core::manifest::{
    SupportCatalogue, shipped_corpus, verify_support_catalogue_bytes, write_sums,
};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::schema::vocab::Lifecycle;

use crate::allowlist::{AllowlistProfile, RAW_ORIGINAL_EXTENSIONS, verify_staged_maps};
use crate::proc;
use crate::zipdet::{read_and_verify_deterministic_zip, write_deterministic_zip_bytes};

/// Environment variable that supplies the `{security_contact}` text (control IN-01).
pub const SECURITY_CONTACT_ENV: &str = "FIELDBOOK_SECURITY_CONTACT";

const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");
const REGRESSION_PATH: &str = "corpus/eval/regression.toml";
const SUPPORT_NAME: &str = "corpus-support.toml";
const SUPPORT_SHA_NAME: &str = "corpus-support.sha256";

const USAGE: &str = "usage: cargo xtask edition-raw --out DIR [--fieldbook PROGRAM]";

/// Captured result of one program run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// Exit code (`-1` when the program was ended by a signal).
    pub code: i32,
    /// Standard output bytes.
    pub stdout: Vec<u8>,
    /// Standard error bytes.
    pub stderr: Vec<u8>,
}

/// Starts programs for [`edition_raw`]. Tests replace it with a script that refuses every
/// renderer, site or browser invocation.
pub trait Runner {
    /// Run `program` with `args` in `cwd` and return its captured output.
    fn run(&mut self, cwd: &Path, program: &str, args: &[&str]) -> Result<RunOutput, String>;
}

/// The real [`Runner`]: starts the program as a child process.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemRunner;

impl Runner for SystemRunner {
    fn run(&mut self, cwd: &Path, program: &str, args: &[&str]) -> Result<RunOutput, String> {
        let out = proc::output(program, args, cwd)?;
        Ok(RunOutput {
            code: out.status.code().unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
        })
    }
}

/// Parsed `edition-raw` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawArgs {
    /// Output folder (`--out DIR`); holds the five delivered files.
    pub out: PathBuf,
    /// Optional prebuilt `fieldbook` program (`--fieldbook PROGRAM`); default builds with cargo.
    pub fieldbook: Option<PathBuf>,
}

/// Parse `edition-raw --out DIR [--fieldbook PROGRAM]`.
pub fn parse_edition_raw_cli(args: &[&str]) -> Result<RawArgs, String> {
    let mut out: Option<PathBuf> = None;
    let mut fieldbook: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(&arg) = it.next() {
        match arg {
            "--out" => {
                let v = it
                    .next()
                    .ok_or_else(|| format!("--out needs a value\n{USAGE}"))?;
                if out.replace(PathBuf::from(*v)).is_some() {
                    return Err(format!("--out given twice\n{USAGE}"));
                }
            }
            "--fieldbook" => {
                let v = it
                    .next()
                    .ok_or_else(|| format!("--fieldbook needs a value\n{USAGE}"))?;
                if fieldbook.replace(PathBuf::from(*v)).is_some() {
                    return Err(format!("--fieldbook given twice\n{USAGE}"));
                }
            }
            other => return Err(format!("unknown argument `{other}`\n{USAGE}")),
        }
    }
    let out = out.ok_or_else(|| format!("--out DIR is required\n{USAGE}"))?;
    Ok(RawArgs { out, fieldbook })
}

/// One delivered file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveredFile {
    /// File name beside the others in `--out`.
    pub name: String,
    /// Byte length.
    pub size: u64,
    /// Lowercase SHA-256 of the bytes.
    pub sha256: String,
}

/// The result of a successful [`edition_raw`] run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawPackage {
    /// Edition tag (`edition-NNN`).
    pub tag: String,
    /// Source commit.
    pub commit: String,
    /// The five delivered files, sorted by name.
    pub files: Vec<DeliveredFile>,
}

/// Command handler for `cargo xtask edition-raw`.
pub fn command_edition_raw(root: &Path, args: &[&str]) -> Result<bool, String> {
    let parsed = parse_edition_raw_cli(args)?;
    let contact = std::env::var(SECURITY_CONTACT_ENV).ok();
    let mut runner = SystemRunner;
    let package = edition_raw(root, &parsed, &mut runner, contact.as_deref())?;
    println!("raw edition {} at commit {}", package.tag, package.commit);
    for file in &package.files {
        println!("{}  {:>10}  {}", file.sha256, file.size, file.name);
    }
    Ok(true)
}

fn git_text(runner: &mut dyn Runner, root: &Path, args: &[&str]) -> Result<String, String> {
    let out = runner.run(root, "git", args)?;
    if out.code != 0 {
        return Err(format!(
            "`git {}` failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
}

/// Refuse a working tree whose files are not exactly the bytes of `HEAD`.
///
/// The packer and the strict check read the working tree, while the edition names the commit,
/// so the tree must hold that commit and nothing else. `git status` alone does not see:
/// - an ignored file under `corpus/` (the Corpus reader picks it up);
/// - a tracked file marked assume-unchanged or skip-worktree and then edited (or, under a sparse
///   checkout, left out), reported by `git ls-files -v` with a tag other than `H`;
/// - a sparse checkout, which can leave part of the Corpus out of the tree.
///
/// `git` runs one git command at the repository root and returns its standard output.
pub(crate) fn require_clean_tree(
    git: &mut dyn FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let dirty = git(&["status", "--porcelain", "--untracked-files=all"])?;
    if !dirty.trim().is_empty() {
        return Err("the working tree is not clean; commit or remove changes first".to_owned());
    }
    let ignored = git(&[
        "status",
        "--porcelain",
        "--untracked-files=all",
        "--ignored=matching",
        "--",
        "corpus",
    ])?;
    if let Some(first) = ignored.lines().find(|l| !l.trim().is_empty()) {
        return Err(format!(
            "the working tree is not clean: an ignored file sits under corpus/ ({}); remove it first",
            first.trim()
        ));
    }
    let flagged = git(&["ls-files", "-v"])?;
    if let Some(first) = flagged
        .lines()
        .find(|l| !l.is_empty() && !l.starts_with("H "))
    {
        return Err(format!(
            "the working tree is not clean: a tracked file is marked assume-unchanged, skip-worktree or unmerged ({first}); clear the flag first"
        ));
    }
    let sparse = git(&[
        "config",
        "--bool",
        "--default",
        "false",
        "core.sparseCheckout",
    ])?;
    if sparse.trim() != "false" {
        return Err(
            "the working tree is not clean: the checkout is sparse; use a full checkout".to_owned(),
        );
    }
    Ok(())
}

fn is_lower_hex(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The strict-check and regression evidence derived from one `fieldbook check --json` run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckEvidence {
    /// The report bytes exactly as the program printed them (with one final LF).
    pub check_json: Vec<u8>,
    /// The deterministic regression result.
    pub regression_json: Vec<u8>,
    /// Regression cases in the set (counted from `regression.toml`).
    pub cases: usize,
    /// Cases marked `critical` in the set.
    pub critical_cases: usize,
    /// `FB0127` miss diagnostics in the report.
    pub misses: usize,
}

/// The size of the regression set, counted from `corpus/eval/regression.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegressionSet {
    /// Number of `[[case]]` entries.
    pub cases: usize,
    /// Number of entries marked `critical`.
    pub critical_cases: usize,
}

impl RegressionSet {
    /// Count the cases of a `regression.toml`; a malformed file is refused.
    pub fn from_toml(bytes: &[u8]) -> Result<Self, String> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| format!("{REGRESSION_PATH} is not valid UTF-8"))?;
        let cases = fieldbook_core::eval::parse(text).map_err(|errs| {
            format!(
                "{REGRESSION_PATH} is malformed: {}",
                errs.first().map(|e| e.message.as_str()).unwrap_or_default()
            )
        })?;
        Ok(Self {
            cases: cases.len(),
            critical_cases: cases.iter().filter(|c| c.critical).count(),
        })
    }
}

/// Judge one `fieldbook check --strict --json` run. Refuses a failed run, a report that is
/// missing or malformed, any error, and any regression error (`FB0126`, `FB0128`).
///
/// A non-critical regression miss (`FB0127`) is a warning by design: it is counted in the
/// regression result and never hidden.
pub fn judge_check(
    out: &RunOutput,
    today: &str,
    tag: &str,
    commit: &str,
    set: RegressionSet,
) -> Result<CheckEvidence, String> {
    if out.stdout.iter().all(u8::is_ascii_whitespace) {
        return Err(format!(
            "the strict check produced no report (exit {}): {}",
            out.code,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("the strict check report is not valid JSON: {e}"))?;
    let obj = report
        .as_object()
        .ok_or_else(|| "the strict check report is not a JSON object".to_owned())?;
    if obj
        .get("report_version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
    {
        return Err("the strict check report has an unsupported report_version".to_owned());
    }
    if obj.get("strict").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err("the check report was not produced under --strict".to_owned());
    }
    if obj.get("today").and_then(serde_json::Value::as_str) != Some(today) {
        return Err(format!(
            "the check report is not dated the commit date {today}"
        ));
    }
    let errors = obj
        .get("errors")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "the check report has no error count".to_owned())?;
    if out.code != 0 || errors != 0 {
        return Err(format!(
            "the strict check is not clean: exit {}, {errors} error(s)",
            out.code
        ));
    }
    let diagnostics = obj
        .get("diagnostics")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "the check report has no diagnostics list".to_owned())?;

    let mut misses: Vec<serde_json::Value> = Vec::new();
    let mut hard: Vec<String> = Vec::new();
    let mut critical_misses = 0_usize;
    for diag in diagnostics {
        let code = diag
            .get("code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let message = diag
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let line = diag
            .get("line")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        match code {
            "FB0126" | "FB0128" => {
                if code == "FB0128" {
                    critical_misses += 1;
                }
                hard.push(format!("{code}: {message}"));
            }
            "FB0127" => misses.push(serde_json::json!({
                "code": code,
                "line": line,
                "message": message,
            })),
            _ => {}
        }
    }
    if !hard.is_empty() {
        return Err(format!(
            "the regression pass failed ({} error(s)): {}",
            hard.len(),
            hard.join("; ")
        ));
    }
    misses.sort_by_key(ToString::to_string);

    let mut check_json = out.stdout.clone();
    if check_json.last() != Some(&b'\n') {
        check_json.push(b'\n');
    }
    // `passed` means: no error in the report and no critical case missed. A set with no critical
    // case passes that test vacuously, so the counts are written beside it.
    let n_misses = misses.len();
    let passed = errors == 0 && hard.is_empty() && critical_misses == 0;
    let regression = serde_json::json!({
        "report_version": 1,
        "edition": tag,
        "commit": commit,
        "today": today,
        "check_sha256": sha256_hex(&check_json),
        "passed": passed,
        "errors": errors,
        "cases": set.cases,
        "critical_cases": set.critical_cases,
        "critical_misses": critical_misses,
        "misses": n_misses,
        "miss_diagnostics": misses,
    });
    let mut regression_json = serde_json::to_string_pretty(&regression)
        .map_err(|e| format!("cannot write the regression result: {e}"))?
        .into_bytes();
    regression_json.push(b'\n');
    Ok(CheckEvidence {
        check_json,
        regression_json,
        cases: set.cases,
        critical_cases: set.critical_cases,
        misses: n_misses,
    })
}

/// Every placeholder `{name}` still present in `text` (lowercase letters, digits, underscore).
pub fn leftover_placeholders(text: &str) -> Vec<String> {
    let mut found = BTreeSet::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len()
                && (bytes[j].is_ascii_lowercase() || bytes[j].is_ascii_digit() || bytes[j] == b'_')
            {
                j += 1;
            }
            if j > start && j < bytes.len() && bytes[j] == b'}' {
                found.insert(text[start..j].to_owned());
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    found.into_iter().collect()
}

/// Fill `{name}` placeholders from `values` and refuse any placeholder left over (IN-01).
pub fn fill_placeholders(
    label: &str,
    template: &str,
    values: &BTreeMap<&str, String>,
) -> Result<String, String> {
    let mut text = template.to_owned();
    for (name, value) in values {
        text = text.replace(&format!("{{{name}}}"), value);
    }
    let left = leftover_placeholders(&text);
    if left.is_empty() {
        Ok(text)
    } else {
        Err(format!(
            "IN-01: `{label}` has unfilled placeholder(s): {}",
            left.join(", ")
        ))
    }
}

fn extension_class(path: &str) -> &'static str {
    let ext = path
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "csv" => "csv",
        "md" | "toml" => "md_toml",
        "txt" => "txt",
        "pdf" => "pdf",
        "json" => "json",
        _ => "other",
    }
}

const README_TEMPLATE: &str = r#"FIELDBOOK RAW EDITION {edition}

This is the raw edition: plain public files only. The packaging step
admits only Markdown, TOML, CSV, JSON, text and PDF files, so this
edition has no generated web site, no JavaScript, no style sheet, no
workbook and no program. No browser test was run for it. The PDF files
are third-party public documents, shipped unchanged; they were not
inspected for embedded content.

Edition {edition}. Tool version {tool_version}. Source commit {commit}.
Source date (UTC): {source_date}.

WHAT YOU RECEIVED

Through the approved transfer channel:
  fieldbook-edition-{edition_number}.zip    {n_files_edition} files
  fieldbook-originals-{edition_number}.zip  {n_files_originals} files
By the separate hash route that your IT department settled:
  SHA256SUMS             the SHA-256 of each zip, one line per zip
  corpus-support.sha256  the SHA-256 of the support-file catalogue,
                         edition\evidence\corpus-support.toml, one line

CHECK, THEN EXTRACT

1. Open PowerShell in the folder that holds the files.
2. Run Get-FileHash -Algorithm SHA256 on each zip. Compare each value
   with the matching line in SHA256SUMS. Ignore letter case. Every
   character must match. If one differs, do not extract. Delete the
   files and ask the sender.
3. Extract both zips into %LOCALAPPDATA%\Fieldbook\ . Each zip holds one
   top folder, so you get edition\ and originals\ side by side.
4. Run Get-FileHash -Algorithm SHA256 on
   %LOCALAPPDATA%\Fieldbook\edition\evidence\corpus-support.toml and
   compare the value with corpus-support.sha256. If it differs, do not
   use the edition. Ask the sender.

MARK-OF-THE-WEB
Files that arrive through the approved channel, and files taken
out of the zips, may carry a Windows marker, the Mark-of-the-Web.
It is normal and it is IT's control. Leave it in place: do not
run Unblock-File, do not use the Unblock box in a file's
Properties, and do not delete the stream. If a prompt or policy
stops you from opening a file, tell IT; do not work around it. The
transfer runbook explains it under "Mark-of-the-Web".

BACK UP YOUR WORK
Your working copies live in Fieldbook\working\. It is the only folder
that cannot be rebuilt: Fieldbook\edition\ and Fieldbook\originals\
come from the two zips. This edition creates no working\ folder and
makes no backup of its own. If you keep files in working\, mark the
folder not content-indexed once:
  New-Item -ItemType Directory -Force "$env:LOCALAPPDATA\Fieldbook\working"
  attrib +I "$env:LOCALAPPDATA\Fieldbook\working" /S /D
Then copy working\ to the place your IT policy covers after each piece
of work and before each new edition. The commands, with a hash
comparison that uses Get-FileHash, are in the transfer runbook under
"Back up working".

SUPPORT EVIDENCE
The edition holds evidence\corpus-support.toml, a catalogue of the
public support files used to build it. Its digest, in
corpus-support.sha256, travels with SHA256SUMS by the separate route.
Keep both with the pair. The catalogue is evidence of bytes, not proof
that your copy is public or admitted.

WHERE TO START

  edition\corpus\pages\      the reference pages, one Markdown file each
  edition\corpus\registers\  the registers, as CSV files
{sources_text_line}  edition\ASSISTANT.md       the assistant rules
  edition\evidence\          the strict check and regression results, and the
                             support-file catalogue
  edition\intake\            the statements for IT
  originals\                 the public source documents, byte for byte
                             (HTML and SVG ones carry an added .txt)

Read TRANSFER-STATEMENT.txt first. Its contact is in section 7.
"#;

const TRANSFER_TEMPLATE: &str = r#"FIELDBOOK TRANSFER STATEMENT (RAW EDITION)
Edition {edition}. Tool version {tool_version}. Source commit {commit}.
Source date (UTC): {source_date}. Schema version {schema_version}.

This statement ships inside the edition. The edition build fills in the
figures below from the files being packed, so they are counted, not typed.

1. WHAT THIS IS

Fieldbook is a library of public finance and government-contracting reference
material for one reader. This delivery is the raw edition: plain files only.
The packaging step admits only the file types listed in section 2, so it has
no generated web site, no JavaScript, no style sheet, no workbook and no
program or executable. No browser test was run for it. You read the Markdown,
CSV, text and PDF files with the reader your IT department has approved for
those file types.
A later, separate request may bring a signed program that runs on the company
laptop as part of the company's own systems. If IT approves it, that program
keeps the company's records in a store under the data folder, never in an
edition, and sends text to an AI endpoint only as a policy file written by IT
allows. Editions stay public. Nothing in this edition does any of that.

2. WHAT IS IN THE TWO ZIP FILES

  fieldbook-edition-{edition_number}.zip    {n_files_edition} files
  fieldbook-originals-{edition_number}.zip  {n_files_originals} files

Count by file type, both zips together:

  CSV                   {n_csv}
  Markdown and TOML     {n_md_toml}
  Text                  {n_txt}
  PDF                   {n_pdf}
  JSON                  {n_json}
  Other                 {n_other}

The edition build packs into the edition zip only files at fixed Corpus,
evidence and intake paths with the extensions .md, .toml, .csv, .json and
.txt, plus its SHA256SUMS list, and into the originals zip only files with the
extensions {raw_original_extensions}, plus its SHA256SUMS list. It refuses any
other file, including any executable, library, script, shortcut, workbook or
markup (HTML, SVG) file, and any original that begins like a Windows or Linux
executable. HTML and SVG originals are shipped as .txt files, with the same
bytes. PDF originals are third-party public documents
shipped byte for byte; they are not inspected for embedded content.

3. WHAT THESE FILES NEVER DO

They install nothing. They add no program, service, driver, scheduled task,
startup entry, registry key or shortcut. They need no administrator rights.
No network test and no browser test was run for this delivery, and this
statement makes no claim about the network behaviour of any program you use
to open the files.

4. WHAT IS IN THEM

Public sources and synthetic examples only. Every number in an example is marked
synthetic. There is no company, customer, personal or employee information, and
no controlled unclassified or export-controlled material. If you find any,
stop using the edition and tell the contact in section 7.

The content counts are: {n_pages} pages, {n_sources} source versions,
{n_claims} claims, {n_questions} open questions. The Corpus fingerprint is
{corpus_fingerprint}.

5. HOW TO CHECK WHAT YOU RECEIVED

The two zip files are not signed. Their SHA-256 hashes are not inside them. The
hash list, SHA256SUMS, reaches you separately, by the route your IT department
settled, so a change to a zip in transit shows up as a different hash. Steps are
in intake\integrity-guide.md. Inside each zip, a file named SHA256SUMS lists
every file and its hash.

Extract both zips into %LOCALAPPDATA%\Fieldbook\ (README.txt shows how). Do not
remove the Mark-of-the-Web from the zips or from the files. Leave Windows to
treat the files as downloaded files.

Support-file evidence. evidence\corpus-support.toml lists the SHA-256 of each
support file of the source Corpus at this commit (rules, templates, evaluation
and pipeline files). Most of those files are not part of this delivery; the
catalogue records their digests so that a later edition can be compared. A
one-line digest, corpus-support.sha256, arrives with SHA256SUMS by the same
separate route. It is the SHA-256 of evidence\corpus-support.toml: after you
extract the edition, hash that file and compare. The digest is evidence of
bytes, not proof that any file is public or admitted.

Check evidence. evidence\check.json is the report of the strict Corpus check run
on the source commit just before packing, and evidence\regression.json is its
regression result. The regression set has {n_regression_cases} cases,
{n_critical_cases} of them marked critical. {n_regression_misses} regression-miss
warnings are listed in evidence\regression.json: each is a case whose expected
page was not found where the case expects it. A miss of a critical case would
have stopped the packaging. The check and regression files were produced by the
sender's own tool at packing time. They are not signed and not independently
reproduced.

6. AI

This delivery contains no program and no page, so none of the steps in this
section can be done with it alone.

Tool versions before 0.2.0 contain no artificial-intelligence model and contact
none. An optional step builds a block of public text that you copy into an
assistant your IT department has approved, and then checks the citations in the
answer. You do the copying. From version 0.2.0 the AI build can also send that
block to an endpoint your IT department has approved in its policy file, only when you run the
command for it, after writing a line to its audit log. Which build is installed
on this laptop is your IT department's decision. Nothing in the checks, the
search, the calculations or the editions uses a model. See
intake\ai-development-statement.md.

7. CONTACT

{security_contact}

8. REMOVAL

Delete the folder %LOCALAPPDATA%\Fieldbook\ under the retention procedure. Clear
any application storage separately through IT's procedure. Folder deletion does
not clear other data; neither step promises secure erasure.
"#;

const RAW_NOTE: &str = "> Raw edition note. This delivery is the raw edition: it has no generated site,\n> no JavaScript and no browser pages, and no browser or network test was run for it.\n> Where the text below describes a site, pages in a browser or browser storage, it\n> describes the full edition and does not apply to this delivery. Any sentence below\n> that says an interaction was tested, or that points to network-silence.md for a test\n> result, does not apply to this delivery either.\n\n";

const INTEGRITY_TEMPLATE: &str = r#"# Fieldbook integrity guide (raw edition)

Edition {edition}. Tool version {tool_version}.

This guide shows how to check that the files you received are the ones the sender built, and how to extract them. The raw edition is files only. It needs no program, no administrator rights and no command other than `Get-FileHash`.

## 1. What you should have

| Item | Name | Arrives by |
|---|---|---|
| Edition zip | `fieldbook-edition-{edition_number}.zip` | The transfer channel IT approved |
| Originals zip | `fieldbook-originals-{edition_number}.zip` | The same channel |
| Hash list | `SHA256SUMS`, two lines, one per zip | A different route from the zips, settled with IT |
| Support digest | `corpus-support.sha256`, one line: the SHA-256 of `edition\evidence\corpus-support.toml` | The same separate route as the hash list |

The hash list and the support digest are not inside the zips and the zips do not contain their own hashes. A zip cannot vouch for itself.

## 2. Check each zip

Open PowerShell in the folder that holds the zips. Run these commands.

```powershell
Get-FileHash -Algorithm SHA256 .\fieldbook-edition-{edition_number}.zip
Get-FileHash -Algorithm SHA256 .\fieldbook-originals-{edition_number}.zip
```

Compare the `Hash` values with the lines in `SHA256SUMS`. Windows prints capital letters and the lists use small letters. Ignore the case. Every character must match.

| Result | What to do |
|---|---|
| Both match | Go to section 3 |
| Either differs | Do not extract. Delete the files. Tell the contact in `TRANSFER-STATEMENT.txt`, or whoever sent them, and ask for them to be sent again |
| `Get-FileHash` is blocked | Do not look for another way. Ask IT which hash tool it approves and check with that |

## 3. Extract

Both zips go to the same place, `%LOCALAPPDATA%\Fieldbook\`. Each zip already holds one top folder (`edition` or `originals`), so extracting to the Fieldbook folder gives `edition\` and `originals\` without a doubled folder.

1. Open File Explorer. Type `%LOCALAPPDATA%` in the address bar and press Enter.
2. Create a folder named `Fieldbook`. Open it. Click the address bar and copy the full path.
3. Right-click the edition zip and choose **Extract All**.
4. Replace the destination in the box with the path you copied. The box fills in a folder named after the zip, so delete that last part. Choose **Extract**.
5. Do the same for the originals zip.

If Extract All shows a long-path error, the destination is too deep. Use the path above and nothing longer.

## 4. Check the support catalogue

After extracting, hash the catalogue that came inside the edition zip and compare it with `corpus-support.sha256`, which arrived by the separate route.

```powershell
Get-FileHash -Algorithm SHA256 "$env:LOCALAPPDATA\Fieldbook\edition\evidence\corpus-support.toml"
```

Ignore the case. Every character must match. If it differs, do not use the edition: delete `edition\` and `originals\` and ask the sender. A match shows byte equality only. It does not show that any file is public or admitted.

## 5. Mark-of-the-Web

Files that come from outside carry a Windows marker, the Mark-of-the-Web. Leave it in place. Do not use **Unblock** in file Properties, `Unblock-File`, or any tool that strips the marker.

## 6. Check one file later

Each zip has a `SHA256SUMS` file inside it, in the sha256sum text format: one line per file, with the hash, two spaces and the path relative to the folder that holds `SHA256SUMS`. It lists every file in that zip except itself. To check one file, run `Get-FileHash -Algorithm SHA256` on it and find its line.

## 7. Update and roll back

Keep the current `edition\` and `originals\` together as one verified pair. To update, check and extract the new pair into a fresh staging folder, check the edition identifier in `edition\edition.toml`, then replace both folders together. Keep the previous pair so that you can put both back. Never replace only one of the two.

## 8. What the hashes show

The check shows that the files you hold are byte-for-byte the files the sender hashed. It does not show who built them. The zips are unsigned.
"#;

const NETWORK_TEMPLATE: &str = r#"# Fieldbook network-silence report (raw edition)

Edition {edition}. Tool version {tool_version}.

The raw edition has no generated pages, no JavaScript and no browser summary, so no network-silence test was run for it and there is no `network-silence-summary.txt`. This delivery makes no claim about network behaviour.

What the build does check is the file list: the edition zip holds only files with the extensions .md, .toml, .csv, .json and .txt at fixed paths, and the originals zip holds only {raw_original_extensions} files (each zip also holds its `SHA256SUMS` list). The build refuses any other file, including any executable, library, script, shortcut, workbook or markup (HTML, SVG) file, and any original that begins like a Windows or Linux executable. PDF originals are third-party public documents shipped byte for byte and are not inspected for embedded content.

The full edition's network-silence check is a separate delivery with its own report.
"#;

const AI_STATEMENT: &str = include_str!("../../docs/intake/ai-development-statement.md");
const DATA_HANDLING: &str = include_str!("../../docs/intake/data-handling.md");

/// Everything built from one clean commit, before anything is written.
struct Built {
    tag: String,
    commit: String,
    edition_zip_name: String,
    originals_zip_name: String,
    edition_zip: Vec<u8>,
    originals_zip: Vec<u8>,
    outer_sums: Vec<u8>,
    support_toml: Vec<u8>,
    support_sha: Vec<u8>,
}

fn civil_utc(epoch: i64) -> Result<Now, String> {
    let secs = u64::try_from(epoch).map_err(|_| "negative commit time".to_owned())?;
    Now::from_unix_secs(secs).map_err(|e| format!("commit time out of range: {e}"))
}

/// Build the paired raw archives and the support sidecars from the clean commit at `root`.
fn build(
    root: &Path,
    args: &RawArgs,
    runner: &mut dyn Runner,
    contact: Option<&str>,
) -> Result<Built, String> {
    let contact = contact
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .ok_or_else(|| {
            format!("IN-01: {SECURITY_CONTACT_ENV} is missing or blank; set it to the security contact text")
        })?;
    if contact.chars().any(char::is_control) {
        return Err(format!(
            "IN-01: {SECURITY_CONTACT_ENV} must be one line of text"
        ));
    }

    // 1. A clean tree, and the identity of the commit.
    require_clean_tree(&mut |a: &[&str]| git_text(runner, root, a))?;
    let commit = git_text(runner, root, &["rev-parse", "HEAD"])?;
    if !is_lower_hex(&commit, 40) {
        return Err(format!("`git rev-parse HEAD` returned `{commit}`"));
    }
    let epoch: i64 = git_text(runner, root, &["log", "-1", "--format=%ct", "HEAD"])?
        .parse()
        .map_err(|e| format!("cannot read the commit time: {e}"))?;
    let now = civil_utc(epoch)?;
    let today = now.date.to_string();

    let changes = fs::read_to_string(root.join("corpus/changes.md"))
        .map_err(|e| format!("cannot read corpus/changes.md: {e}"))?;
    let heading = latest_edition_heading(&changes)
        .ok_or_else(|| "corpus/changes.md has no `## edition-NNN` heading".to_owned())?
        .to_owned();
    let edition_id = EditionId::parse(&heading).map_err(|e| format!("bad edition heading: {e}"))?;
    let tag = edition_id.as_str().to_owned();
    let edition_number = format!("{:03}", edition_id.number());

    // 2. The strict check, with the regression pass, through the built program.
    let check_args = ["check", "--strict", "--json", "--today", today.as_str()];
    let out = match &args.fieldbook {
        Some(program) => {
            let program = program.to_string_lossy().into_owned();
            runner.run(root, &program, &check_args)?
        }
        None => {
            let mut full = vec![
                "run",
                "-q",
                "--locked",
                "-p",
                fieldbook_core::strings::binary(),
                "--",
            ];
            full.extend_from_slice(&check_args);
            runner.run(root, "cargo", &full)?
        }
    };
    let set = RegressionSet::from_toml(&read_regular_file(root, REGRESSION_PATH)?)?;
    let evidence = judge_check(&out, &today, &tag, &commit, set)?;
    let (n_cases, n_critical, n_misses) =
        (evidence.cases, evidence.critical_cases, evidence.misses);

    // 3. One validated public Corpus generation.
    let tree =
        fieldbook::io::tree::read_tree(root).map_err(|e| format!("cannot read the Corpus: {e}"))?;
    let host_today = now.date;
    let (model, report) = load(&Host::new(tree, host_today), &Options::default());
    if report.errors() != 0 {
        let first = report
            .render_text()
            .lines()
            .find(|l| l.contains("error["))
            .unwrap_or_default()
            .to_owned();
        return Err(format!(
            "the Corpus does not load cleanly ({} error(s)); first: {first}",
            report.errors()
        ));
    }
    let eff = EffectivePolicy::floor(false);
    let Decision::Allowed(allowed) =
        egress_check(&eff, &Dest::Edition, None, None, now, Shipped::Never)
    else {
        return Err("the edition destination refused the Corpus view".to_owned());
    };
    let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());

    let mut quarantined: BTreeSet<VersionId> = BTreeSet::new();
    for s in view.sources() {
        if s.value.lifecycle == Lifecycle::Quarantined {
            let vid = VersionId::parse(&format!("{}-v{}", s.value.source_id, s.value.version))
                .map_err(|e| format!("bad version id: {e}"))?;
            quarantined.insert(vid);
        }
    }
    let base = shipped_corpus(&view, &quarantined).map_err(|e| format!("shipped Corpus: {e}"))?;
    let view_tree = view.tree();
    let reg_bytes = view_tree
        .get_bytes(REGRESSION_PATH)
        .ok_or_else(|| format!("{REGRESSION_PATH} is missing"))?
        .to_vec();
    let mut files = base;
    files.push((REGRESSION_PATH.to_owned(), sha256_hex(&reg_bytes)));
    files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    files.dedup_by(|a, b| a.0 == b.0);

    let schema_version = view
        .corpus()
        .map(|c| c.schema_version)
        .ok_or_else(|| "corpus/corpus.toml is missing".to_owned())?;
    let build_info = BuildInfo::new(
        Some(now.date),
        Some(commit.clone()),
        BuildProfile::Silent,
        Some(epoch),
    );
    let mut info = EditionInfo::from(
        &view,
        &files,
        Some(&tag),
        &build_info,
        TOOL_VERSION,
        schema_version,
    )
    .map_err(|e| format!("edition metadata: {e}"))?;

    // 4. The originals: public, not quarantined, registered by hash. The labelled view carries
    // no original bytes, so the stored names come from the clean public originals folder.
    let mut stored_originals: Vec<String> = Vec::new();
    for entry in fs::read_dir(root.join("corpus/sources/public"))
        .map_err(|e| format!("cannot read corpus/sources/public: {e}"))?
    {
        let entry = entry.map_err(|e| format!("cannot read corpus/sources/public: {e}"))?;
        stored_originals.push(entry.file_name().to_string_lossy().into_owned());
    }
    stored_originals.sort();
    let mut originals: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for s in view.sources() {
        let row = &s.value;
        if row.lifecycle == Lifecycle::Quarantined || row.location.starts_with("metadata-only:") {
            continue;
        }
        let vid = format!("{}-v{}", row.source_id, row.version);
        let prefix = format!("{vid}.");
        let found: Vec<String> = stored_originals
            .iter()
            .filter(|name| name.starts_with(&prefix))
            .map(|name| format!("corpus/sources/public/{name}"))
            .collect();
        let [stored] = found.as_slice() else {
            return Err(format!(
                "source version {vid} must have exactly one stored original, found {}",
                found.len()
            ));
        };
        let bytes = read_regular_file(root, stored)?;
        let actual = sha256_hex(&bytes);
        if actual != row.sha256 {
            return Err(format!(
                "original `{stored}` has sha256 {actual}, not the registered {}",
                row.sha256
            ));
        }
        let name = stored.rsplit('/').next().unwrap_or(stored).to_owned();
        let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let shipped_name = if matches!(ext.as_str(), "html" | "htm" | "svg" | "xhtml") {
            format!("{name}.txt")
        } else {
            name
        };
        originals.insert(shipped_name, bytes);
    }
    info.counts.originals = originals.len();

    // 5. The edition tree.
    let assistant_name = fieldbook_core::strings::paths()[6];
    let mut edition: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (path, _) in &files {
        let bytes = view_tree
            .get_bytes(path)
            .ok_or_else(|| format!("shipped file `{path}` has no bytes"))?;
        edition.insert(path.clone(), bytes.to_vec());
    }
    let assistant = view_tree
        .get_bytes(&format!("corpus/{assistant_name}"))
        .ok_or_else(|| format!("corpus/{assistant_name} is missing"))?;
    edition.insert(assistant_name.to_owned(), assistant.to_vec());
    edition.insert("edition.toml".to_owned(), info.to_toml().into_bytes());

    let (support_toml, support_hex) = SupportCatalogue::from_public_source(&view, &commit)
        .map_err(|e| format!("support catalogue: {e}"))?
        .to_toml_bytes_and_sha256();
    verify_support_catalogue_bytes(&support_toml, &support_hex)
        .map_err(|e| format!("support catalogue: {e}"))?;
    edition.insert(
        "evidence/corpus-support.toml".to_owned(),
        support_toml.clone(),
    );
    edition.insert("evidence/check.json".to_owned(), evidence.check_json);
    edition.insert(
        "evidence/regression.json".to_owned(),
        evidence.regression_json,
    );

    // 6. The fixed raw intake texts, with every placeholder filled.
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut paths: Vec<&str> = edition.keys().map(String::as_str).collect();
    paths.extend(["README.txt", "TRANSFER-STATEMENT.txt", "SHA256SUMS"]);
    paths.extend([
        "intake/ai-development-statement.md",
        "intake/data-handling.md",
    ]);
    paths.extend(["intake/integrity-guide.md", "intake/network-silence.md"]);
    let n_files_edition = paths.iter().collect::<BTreeSet<_>>().len();
    for p in paths.iter().copied().collect::<BTreeSet<_>>() {
        *counts.entry(extension_class(p)).or_default() += 1;
    }
    for name in originals.keys().map(String::as_str).chain(["SHA256SUMS"]) {
        *counts.entry(extension_class(name)).or_default() += 1;
    }
    let n_files_originals = originals.len() + 1;
    let count = |k: &str| counts.get(k).copied().unwrap_or(0).to_string();
    let mut values: BTreeMap<&str, String> = BTreeMap::new();
    values.insert("edition", tag.clone());
    values.insert("edition_number", edition_number.clone());
    values.insert("tool_version", info.tool_version.clone());
    values.insert("commit", commit.clone());
    values.insert("schema_version", info.schema_version.to_string());
    values.insert("corpus_fingerprint", info.fingerprint.clone());
    values.insert("source_date", today.clone());
    values.insert("n_files_edition", n_files_edition.to_string());
    values.insert("n_files_originals", n_files_originals.to_string());
    for k in ["csv", "md_toml", "txt", "pdf", "json", "other"] {
        values.insert(
            match k {
                "csv" => "n_csv",
                "md_toml" => "n_md_toml",
                "txt" => "n_txt",
                "pdf" => "n_pdf",
                "json" => "n_json",
                _ => "n_other",
            },
            count(k),
        );
    }
    values.insert("n_pages", info.counts.pages.to_string());
    values.insert("n_sources", info.counts.source_versions.to_string());
    values.insert("n_claims", info.counts.claims.to_string());
    values.insert("n_questions", info.counts.questions.to_string());
    values.insert("security_contact", contact.to_owned());
    values.insert(
        "raw_original_extensions",
        dotted_list(RAW_ORIGINAL_EXTENSIONS),
    );
    values.insert("n_regression_cases", n_cases.to_string());
    values.insert("n_critical_cases", n_critical.to_string());
    values.insert("n_regression_misses", n_misses.to_string());
    // The extracted-text folder is named only when the staged edition holds it.
    values.insert(
        "sources_text_line",
        if edition
            .keys()
            .any(|p| p.starts_with("corpus/sources/text/"))
        {
            "  edition\\corpus\\sources\\text\\  the text extracted from each source\n".to_owned()
        } else {
            String::new()
        },
    );

    let readme = fill_placeholders("README.txt", README_TEMPLATE, &values)?;
    let transfer = fill_placeholders("TRANSFER-STATEMENT.txt", TRANSFER_TEMPLATE, &values)?;
    let integrity = fill_placeholders("intake/integrity-guide.md", INTEGRITY_TEMPLATE, &values)?;
    let network = fill_placeholders("intake/network-silence.md", NETWORK_TEMPLATE, &values)?;
    let ai = fill_placeholders(
        "intake/ai-development-statement.md",
        &raw_note_after_title(AI_STATEMENT),
        &values,
    )?;
    let data = fill_placeholders(
        "intake/data-handling.md",
        &raw_note_after_title(DATA_HANDLING),
        &values,
    )?;
    edition.insert("README.txt".to_owned(), readme.into_bytes());
    edition.insert("TRANSFER-STATEMENT.txt".to_owned(), transfer.into_bytes());
    edition.insert(
        "intake/integrity-guide.md".to_owned(),
        integrity.into_bytes(),
    );
    edition.insert("intake/network-silence.md".to_owned(), network.into_bytes());
    edition.insert(
        "intake/ai-development-statement.md".to_owned(),
        ai.into_bytes(),
    );
    edition.insert("intake/data-handling.md".to_owned(), data.into_bytes());

    // 7. Internal checksums, deterministic zips, verified against the Raw profile.
    let edition_sums = sums_of(&edition);
    edition.insert("SHA256SUMS".to_owned(), edition_sums.into_bytes());
    let originals_sums = sums_of(&originals);
    originals.insert("SHA256SUMS".to_owned(), originals_sums.into_bytes());
    if edition.len() != n_files_edition || originals.len() != n_files_originals {
        return Err(format!(
            "internal error: counted {n_files_edition}/{n_files_originals} files, packed {}/{}",
            edition.len(),
            originals.len()
        ));
    }
    verify_staged_maps(AllowlistProfile::Raw, &edition, &originals, &[])
        .map_err(|e| format!("raw allowlist: {e}"))?;

    let edition_zip = write_deterministic_zip_bytes("edition", epoch, &edition)?;
    let originals_zip = write_deterministic_zip_bytes("originals", epoch, &originals)?;
    let re_edition: BTreeMap<String, Vec<u8>> =
        read_and_verify_deterministic_zip(&edition_zip, "edition", Some(epoch))?
            .into_iter()
            .collect();
    let re_originals: BTreeMap<String, Vec<u8>> =
        read_and_verify_deterministic_zip(&originals_zip, "originals", Some(epoch))?
            .into_iter()
            .collect();
    if re_edition != edition || re_originals != originals {
        return Err("the reopened archives differ from the staged trees".to_owned());
    }

    // 8. The sidecars, byte for byte from the staged catalogue.
    let staged_support = re_edition
        .get("evidence/corpus-support.toml")
        .ok_or_else(|| "staged catalogue is missing".to_owned())?
        .clone();
    let sidecar_hex = sha256_hex(&staged_support);
    let support_sha = write_sums(&[(SUPPORT_NAME.to_owned(), sidecar_hex.clone())]).into_bytes();
    if sidecar_hex != support_hex
        || support_sha != SupportCatalogue::sidecar_sha256_line(&staged_support).into_bytes()
    {
        return Err("the support digest does not name the staged catalogue bytes".to_owned());
    }
    let edition_zip_name = format!("fieldbook-edition-{edition_number}.zip");
    let originals_zip_name = format!("fieldbook-originals-{edition_number}.zip");
    let outer_sums = write_sums(&[
        (edition_zip_name.clone(), sha256_hex(&edition_zip)),
        (originals_zip_name.clone(), sha256_hex(&originals_zip)),
    ])
    .into_bytes();

    Ok(Built {
        tag,
        commit,
        edition_zip_name,
        originals_zip_name,
        edition_zip,
        originals_zip,
        outer_sums,
        support_toml: staged_support,
        support_sha,
    })
}

/// `.a, .b and .c` from `["a", "b", "c"]`, for the statements.
fn dotted_list(exts: &[&str]) -> String {
    let dotted: Vec<String> = exts.iter().map(|e| format!(".{e}")).collect();
    match dotted.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::new(),
    }
}

fn raw_note_after_title(text: &str) -> String {
    match text.split_once('\n') {
        Some((title, rest)) => format!("{title}\n\n{RAW_NOTE}{}", rest.trim_start_matches('\n')),
        None => text.to_owned(),
    }
}

pub(crate) fn sums_of(files: &BTreeMap<String, Vec<u8>>) -> String {
    let mut entries: Vec<(String, String)> = files
        .iter()
        .filter(|(p, _)| p.as_str() != "SHA256SUMS")
        .map(|(p, b)| (p.clone(), sha256_hex(b)))
        .collect();
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    write_sums(&entries)
}

fn read_regular_file(root: &Path, rel: &str) -> Result<Vec<u8>, String> {
    let mut cur = root.to_path_buf();
    for seg in rel.split('/') {
        cur.push(seg);
        let meta = fs::symlink_metadata(&cur).map_err(|e| format!("cannot stat `{rel}`: {e}"))?;
        if meta.file_type().is_symlink() {
            return Err(format!("`{rel}` must not be or traverse a symlink"));
        }
    }
    fs::read(&cur).map_err(|e| format!("cannot read `{rel}`: {e}"))
}

/// Package the raw edition of the clean commit at `root` into `args.out`.
///
/// Every refusal happens before a file reaches `args.out`; the previous output is left alone.
pub fn edition_raw(
    root: &Path,
    args: &RawArgs,
    runner: &mut dyn Runner,
    contact: Option<&str>,
) -> Result<RawPackage, String> {
    let built = build(root, args, runner, contact)?;
    let delivered: Vec<(String, Vec<u8>)> = vec![
        (built.edition_zip_name.clone(), built.edition_zip.clone()),
        (
            built.originals_zip_name.clone(),
            built.originals_zip.clone(),
        ),
        ("SHA256SUMS".to_owned(), built.outer_sums.clone()),
        (SUPPORT_NAME.to_owned(), built.support_toml.clone()),
        (SUPPORT_SHA_NAME.to_owned(), built.support_sha.clone()),
    ];

    // Publication check first: an existing file may only be identical.
    for (name, bytes) in &delivered {
        let target = args.out.join(name);
        match fs::read(&target) {
            Ok(existing) if &existing == bytes => {}
            Ok(_) => {
                return Err(format!(
                    "`{}` already exists with different bytes; use an empty output folder",
                    target.display()
                ));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("cannot read `{}`: {e}", target.display())),
        }
    }

    fs::create_dir_all(&args.out)
        .map_err(|e| format!("cannot create `{}`: {e}", args.out.display()))?;
    let staging = args.out.join(format!(".staging-{}", built.tag));
    if staging.exists() {
        fs::remove_dir_all(&staging)
            .map_err(|e| format!("cannot clear old staging `{}`: {e}", staging.display()))?;
    }
    fs::create_dir_all(&staging)
        .map_err(|e| format!("cannot create staging `{}`: {e}", staging.display()))?;
    let result = (|| -> Result<(), String> {
        for (name, bytes) in &delivered {
            fs::write(staging.join(name), bytes)
                .map_err(|e| format!("cannot stage `{name}`: {e}"))?;
        }
        for (name, bytes) in &delivered {
            let staged = fs::read(staging.join(name))
                .map_err(|e| format!("cannot reopen staged `{name}`: {e}"))?;
            if &staged != bytes {
                return Err(format!("staged `{name}` differs from the built bytes"));
            }
        }
        for (name, _) in &delivered {
            fs::rename(staging.join(name), args.out.join(name))
                .map_err(|e| format!("cannot publish `{name}`: {e}"))?;
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staging);
    result?;

    let mut files: Vec<DeliveredFile> = delivered
        .iter()
        .map(|(name, bytes)| DeliveredFile {
            name: name.clone(),
            size: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(RawPackage {
        tag: built.tag,
        commit: built.commit,
        files,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use fieldbook_core::schema::Register;

    const MINI_CORPUS_TOML: &str =
        include_str!("../../tests/fixtures/corpus-mini/corpus/corpus.toml");
    pub(crate) const CONTACT: &str = "Synthetic security contact, security@example.invalid";
    const ORIG_BYTES: &[u8] = b"synthetic active original payload\n";
    const HELD_BYTES: &[u8] = b"synthetic held original payload\n";

    /// A scripted runner: real `git`, a canned `fieldbook check`, and a refusal for everything
    /// else. Every call is recorded so a test can prove no renderer or browser was started.
    pub(crate) struct Scripted {
        pub(crate) calls: Vec<String>,
        pub(crate) check_stdout: Vec<u8>,
        pub(crate) check_code: i32,
    }

    impl Scripted {
        pub(crate) fn clean() -> Self {
            Self {
                calls: Vec::new(),
                check_stdout: clean_report("2026-03-02"),
                check_code: 0,
            }
        }
    }

    impl Runner for Scripted {
        fn run(&mut self, cwd: &Path, program: &str, args: &[&str]) -> Result<RunOutput, String> {
            self.calls.push(format!("{program} {}", args.join(" ")));
            if program == "git" {
                return SystemRunner.run(cwd, program, args);
            }
            if program == "fieldbook-test" && args.first() == Some(&"check") {
                return Ok(RunOutput {
                    code: self.check_code,
                    stdout: self.check_stdout.clone(),
                    stderr: Vec::new(),
                });
            }
            Err(format!(
                "forbidden program in a raw run: {program} {}",
                args.join(" ")
            ))
        }
    }

    pub(crate) fn clean_report(today: &str) -> Vec<u8> {
        format!(
            "{{\n  \"report_version\": 1,\n  \"strict\": true,\n  \"today\": \"{today}\",\n  \"errors\": 0,\n  \"warnings\": 0,\n  \"notes\": [],\n  \"diagnostics\": []\n}}\n"
        )
        .into_bytes()
    }

    pub(crate) fn report_with(today: &str, diagnostics: &str, errors: u32) -> Vec<u8> {
        format!(
            "{{\"report_version\":1,\"strict\":true,\"today\":\"{today}\",\"errors\":{errors},\"warnings\":1,\"notes\":[],\"diagnostics\":[{diagnostics}]}}\n"
        )
        .into_bytes()
    }

    pub(crate) fn write(root: &Path, rel: &str, bytes: &[u8]) {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, bytes).expect("write");
    }

    /// A tiny synthetic public Corpus: one active HTML original, one held PDF original.
    pub(crate) fn write_corpus(root: &Path) {
        let orig_sha = sha256_hex(ORIG_BYTES);
        let held_sha = sha256_hex(HELD_BYTES);
        let page_txt = b"extracted page one text\n";
        let page_sha = sha256_hex(page_txt);
        let extraction = format!(
            "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 1\nthin_pages = []\noriginal_sha256 = \"{orig_sha}\"\n\n[pages]\np0001 = \"{page_sha}\"\n"
        );
        write(root, "corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        write(root, "corpus/ASSISTANT.md", b"rules_version = 1\n");
        write(
            root,
            "corpus/changes.md",
            b"# Corpus changes\n\nSynthetic changes log.\n\n## edition-001\n\n- Added: synthetic.\n",
        );
        for reg in Register::ALL {
            let mut header = String::new();
            for (idx, col) in reg.columns().iter().enumerate() {
                if idx > 0 {
                    header.push(',');
                }
                header.push_str(col.name);
            }
            header.push('\n');
            write(
                root,
                &format!("corpus/registers/{}", reg.file_name()),
                header.as_bytes(),
            );
        }
        let sources = format!(
            "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\nS-0001,1,syn-one,Synthetic Active Source,Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_one,reviewed,https://example.invalid/synthetic/one,{orig_sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Active fixture.,Baseline,,,,,\nS-0002,1,syn-two,Synthetic Held Source,Synthetic Board,primary_law,primary_evidence,A,Synthetic,syn_two,reviewed,S-0002-v1.pdf,{held_sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,quarantined,,V2,public,Held for synthetic licence review.,Held test,,,,,\n"
        );
        write(root, "corpus/registers/sources.csv", sources.as_bytes());
        write(root, "corpus/sources/public/S-0001-v1.html", ORIG_BYTES);
        write(root, "corpus/sources/public/S-0002-v1.pdf", HELD_BYTES);
        write(
            root,
            "corpus/sources/text/S-0001-v1/extraction.toml",
            extraction.as_bytes(),
        );
        write(root, "corpus/sources/text/S-0001-v1/p0001.txt", page_txt);
        write(
            root,
            "corpus/pages/compliance/business-systems/cmp-sample.md",
            b"+++\nid = \"cmp-sample\"\ntitle = \"Synthetic Title\"\ntype = \"concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\nstage = 3\nstatus = \"draft\"\nreadiness = \"SYNTHETIC-NOW\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic.\n\n## The rule\n\nSynthetic.\n\n## How it works\n\nSynthetic.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\n## Never confuse\n\nSynthetic.\n\n## Company calibration\n\nSynthetic.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n",
        );
        write(root, "corpus/eval/regression.toml", b"case = []\n");
    }

    pub(crate) fn git(dir: &Path, args: &[&str]) {
        proc::git(dir, args).expect("git");
    }

    pub(crate) fn commit_all(dir: &Path) {
        git(dir, &["add", "-A"]);
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Test Owner",
                "-c",
                "user.email=owner@example.invalid",
                "commit",
                "-q",
                "-m",
                "synthetic",
            ])
            .env("GIT_AUTHOR_DATE", "2026-03-02T12:00:00Z")
            .env("GIT_COMMITTER_DATE", "2026-03-02T12:00:00Z")
            .current_dir(dir)
            .output()
            .expect("commit");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    pub(crate) fn clean_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        git(dir.path(), &["init", "-q", "-b", "main"]);
        write_corpus(dir.path());
        commit_all(dir.path());
        dir
    }

    pub(crate) fn args(out: &Path) -> RawArgs {
        RawArgs {
            out: out.to_path_buf(),
            fieldbook: Some(PathBuf::from("fieldbook-test")),
        }
    }

    pub(crate) fn package(
        repo: &Path,
        out: &Path,
        runner: &mut Scripted,
    ) -> Result<RawPackage, String> {
        edition_raw(repo, &args(out), runner, Some(CONTACT))
    }

    pub(crate) fn dir_listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    pub(crate) fn zip_map(path: &Path, top: &str) -> BTreeMap<String, Vec<u8>> {
        read_and_verify_deterministic_zip(&fs::read(path).expect("zip"), top, None)
            .expect("verified zip")
            .into_iter()
            .collect()
    }

    #[test]
    fn valid_raw_packaging_has_no_site_files_and_keeps_regression_and_support_evidence() {
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        let mut runner = Scripted::clean();
        let pkg = package(repo.path(), out.path(), &mut runner).expect("raw packaging");
        assert_eq!(pkg.tag, "edition-001");

        // Only git and the check ever ran: no renderer, site build or browser.
        for call in &runner.calls {
            assert!(
                call.starts_with("git ") || call.starts_with("fieldbook-test check "),
                "unexpected program call: {call}"
            );
        }
        assert_eq!(
            dir_listing(out.path()),
            vec![
                "SHA256SUMS",
                "corpus-support.sha256",
                "corpus-support.toml",
                "fieldbook-edition-001.zip",
                "fieldbook-originals-001.zip",
            ]
        );

        let ed = zip_map(&out.path().join("fieldbook-edition-001.zip"), "edition");
        let orig = zip_map(&out.path().join("fieldbook-originals-001.zip"), "originals");
        for path in ed.keys() {
            assert!(
                !path.ends_with(".html") && !path.ends_with(".js") && !path.ends_with(".css"),
                "site file in the raw edition: {path}"
            );
            assert!(
                !path.starts_with("assets/") && !path.starts_with("s/"),
                "{path}"
            );
        }
        assert!(!ed.contains_key("index.html"));
        assert!(!ed.contains_key("intake/network-silence-summary.txt"));
        for required in [
            fieldbook_core::strings::paths()[6],
            "edition.toml",
            "README.txt",
            "TRANSFER-STATEMENT.txt",
            "SHA256SUMS",
            "corpus/eval/regression.toml",
            "evidence/check.json",
            "evidence/regression.json",
            "evidence/corpus-support.toml",
            "intake/ai-development-statement.md",
            "intake/data-handling.md",
            "intake/integrity-guide.md",
            "intake/network-silence.md",
        ] {
            assert!(ed.contains_key(required), "missing {required}");
        }
        // The regression evidence is preserved, and the held original never ships.
        let regression = String::from_utf8(ed["evidence/regression.json"].clone()).expect("utf8");
        assert!(regression.contains("\"passed\": true"), "{regression}");
        assert_eq!(ed["evidence/check.json"], clean_report("2026-03-02"));
        assert_eq!(
            orig.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["S-0001-v1.html.txt", "SHA256SUMS"]
        );
        assert!(!ed.keys().any(|p| p.contains("S-0002-v1")));
        assert_eq!(orig["S-0001-v1.html.txt"], ORIG_BYTES);

        // The sidecars name the staged catalogue bytes exactly.
        let staged = &ed["evidence/corpus-support.toml"];
        let sidecar = fs::read(out.path().join("corpus-support.toml")).expect("sidecar");
        assert_eq!(staged, &sidecar);
        let digest = fs::read_to_string(out.path().join("corpus-support.sha256")).expect("digest");
        assert_eq!(
            digest,
            format!("{}  corpus-support.toml\n", sha256_hex(staged))
        );
        let outer = fs::read_to_string(out.path().join("SHA256SUMS")).expect("sums");
        assert_eq!(outer.lines().count(), 2);
        for file in &pkg.files {
            if file.name.ends_with(".zip") {
                assert!(outer.contains(&format!("{}  {}\n", file.sha256, file.name)));
            }
        }

        // The statements are raw variants with the contact filled and no placeholder left.
        let transfer = String::from_utf8(ed["TRANSFER-STATEMENT.txt"].clone()).expect("utf8");
        assert!(transfer.contains("raw edition"));
        assert!(transfer.contains(CONTACT));
        assert!(transfer.contains("no generated web site"));
        assert!(transfer.contains("No browser test was run"));
        for (path, bytes) in ed
            .iter()
            .filter(|(p, _)| p.ends_with(".txt") || p.starts_with("intake/"))
        {
            let text = String::from_utf8(bytes.clone()).expect("utf8");
            assert!(leftover_placeholders(&text).is_empty(), "{path}");
        }
        // The in-archive counts match the packed trees.
        assert!(transfer.contains(&format!("{} files", ed.len())));
        assert!(transfer.contains(&format!("{} files", orig.len())));
    }

    #[test]
    fn raw_packaging_is_deterministic() {
        let repo = clean_repo();
        let a = tempfile::tempdir().expect("a");
        let b = tempfile::tempdir().expect("b");
        package(repo.path(), a.path(), &mut Scripted::clean()).expect("first");
        package(repo.path(), b.path(), &mut Scripted::clean()).expect("second");
        for name in dir_listing(a.path()) {
            assert_eq!(
                fs::read(a.path().join(&name)).expect("a"),
                fs::read(b.path().join(&name)).expect("b"),
                "{name} differs between two runs"
            );
        }
    }

    #[test]
    fn dirty_tree_is_refused_before_publication() {
        let repo = clean_repo();
        fs::write(repo.path().join("corpus/stray.txt"), b"stray").expect("stray");
        let out = tempfile::tempdir().expect("out");
        let err = package(repo.path(), out.path(), &mut Scripted::clean()).expect_err("dirty");
        assert!(err.contains("not clean"), "{err}");
        assert!(dir_listing(out.path()).is_empty());
    }

    /// Changes `git status` does not report still make the tree unclean: the packer reads the
    /// working tree, and the edition names the commit.
    #[test]
    fn hidden_changes_git_status_does_not_show_are_refused_before_publication() {
        let page = "corpus/pages/compliance/business-systems/cmp-sample.md";
        // A tracked page marked skip-worktree or assume-unchanged, then edited.
        for flag in ["--skip-worktree", "--assume-unchanged"] {
            let repo = clean_repo();
            git(repo.path(), &["update-index", flag, page]);
            let text = fs::read_to_string(repo.path().join(page)).expect("page");
            fs::write(
                repo.path().join(page),
                text.replace("Synthetic Title", "Edited Title"),
            )
            .expect("edit");
            assert!(
                proc::git(repo.path(), &["status", "--porcelain"])
                    .expect("status")
                    .is_empty(),
                "{flag}: git status must not see the edit for this test to mean anything"
            );
            let (err, _out) = refused_with(repo.path(), &mut Scripted::clean());
            assert!(err.contains("not clean"), "{flag}: {err}");
        }

        // A well-formed page hidden by .git/info/exclude.
        let repo = clean_repo();
        let text = fs::read_to_string(repo.path().join(page)).expect("page");
        write(
            repo.path(),
            "corpus/pages/compliance/business-systems/cmp-hidden.md",
            text.replace("cmp-sample", "cmp-hidden").as_bytes(),
        );
        write(repo.path(), ".git/info/exclude", b"cmp-hidden.md\n");
        assert!(
            proc::git(
                repo.path(),
                &["status", "--porcelain", "--untracked-files=all"]
            )
            .expect("status")
            .is_empty()
        );
        let (err, _out) = refused_with(repo.path(), &mut Scripted::clean());
        assert!(
            err.contains("not clean") && err.contains("ignored"),
            "{err}"
        );

        // A sparse checkout.
        let repo = clean_repo();
        git(repo.path(), &["config", "core.sparseCheckout", "true"]);
        let (err, _out) = refused_with(repo.path(), &mut Scripted::clean());
        assert!(err.contains("not clean") && err.contains("sparse"), "{err}");
    }

    fn refused_with(repo: &Path, runner: &mut Scripted) -> (String, tempfile::TempDir) {
        let out = tempfile::tempdir().expect("out");
        let err = package(repo, out.path(), runner).expect_err("must refuse");
        assert!(
            dir_listing(out.path()).is_empty(),
            "a refusal left output behind: {:?}",
            dir_listing(out.path())
        );
        (err, out)
    }

    #[test]
    fn planted_script_is_refused() {
        let repo = clean_repo();
        write(
            repo.path(),
            "corpus/sources/text/S-0001-v1/run.ps1",
            b"Write-Host x\n",
        );
        commit_all(repo.path());
        let (err, _out) = refused_with(repo.path(), &mut Scripted::clean());
        assert!(err.contains("run.ps1") || err.contains("ps1"), "{err}");
    }

    #[test]
    fn a_registered_script_original_is_refused_by_the_raw_allowlist() {
        let repo = clean_repo();
        git(
            repo.path(),
            &[
                "mv",
                "corpus/sources/public/S-0001-v1.html",
                "corpus/sources/public/S-0001-v1.js",
            ],
        );
        commit_all(repo.path());
        let (err, out) = refused_with(repo.path(), &mut Scripted::clean());
        assert!(err.contains("S-0001-v1") && err.contains("js"), "{err}");
        assert!(dir_listing(out.path()).is_empty());
    }

    #[test]
    fn nonpublic_content_is_refused() {
        let repo = clean_repo();
        let page = repo
            .path()
            .join("corpus/pages/compliance/business-systems/cmp-sample.md");
        let text = fs::read_to_string(&page)
            .expect("page")
            .replace("handling = [\"public\"]", "handling = [\"internal\"]");
        fs::write(&page, text).expect("rewrite");
        commit_all(repo.path());
        let (err, _out) = refused_with(repo.path(), &mut Scripted::clean());
        assert!(!err.is_empty());
    }

    #[test]
    fn a_held_original_is_not_shipped_and_a_changed_original_is_refused() {
        // The held PDF is on disk in the Corpus and must stay out of the originals zip.
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let orig = zip_map(&out.path().join("fieldbook-originals-001.zip"), "originals");
        assert!(!orig.keys().any(|p| p.contains("S-0002")));

        // An active original whose bytes no longer match its registered hash is refused.
        write(
            repo.path(),
            "corpus/sources/public/S-0001-v1.html",
            b"tampered\n",
        );
        commit_all(repo.path());
        let (err, _out) = refused_with(repo.path(), &mut Scripted::clean());
        assert!(err.contains("sha256") || err.contains("hash"), "{err}");
    }

    #[test]
    fn missing_or_unreadable_check_report_is_refused() {
        let repo = clean_repo();
        let mut runner = Scripted::clean();
        runner.check_stdout = Vec::new();
        let (err, _out) = refused_with(repo.path(), &mut runner);
        assert!(err.contains("no report"), "{err}");

        let mut runner = Scripted::clean();
        runner.check_stdout = b"not json".to_vec();
        let (err, _out) = refused_with(repo.path(), &mut runner);
        assert!(err.contains("not valid JSON"), "{err}");
    }

    #[test]
    fn nonclean_check_is_refused() {
        let repo = clean_repo();
        let mut runner = Scripted::clean();
        runner.check_stdout = report_with(
            "2026-03-02",
            "{\"code\":\"FB0201\",\"severity\":\"error\",\"message\":\"x\",\"line\":1}",
            1,
        );
        runner.check_code = 1;
        let (err, _out) = refused_with(repo.path(), &mut runner);
        assert!(err.contains("not clean"), "{err}");

        // A report that is not for the commit date is refused as well.
        let mut runner = Scripted::clean();
        runner.check_stdout = clean_report("2026-01-01");
        let (err, _out) = refused_with(repo.path(), &mut runner);
        assert!(err.contains("commit date"), "{err}");
    }

    #[test]
    fn failed_regression_is_refused_and_a_plain_miss_is_recorded() {
        let repo = clean_repo();
        let mut runner = Scripted::clean();
        runner.check_stdout = report_with(
            "2026-03-02",
            "{\"code\":\"FB0128\",\"severity\":\"error\",\"message\":\"critical case misses\",\"line\":3}",
            0,
        );
        let (err, _out) = refused_with(repo.path(), &mut runner);
        assert!(err.contains("regression pass failed"), "{err}");

        // A non-critical miss is a warning: it ships, and the count is in the result.
        let mut runner = Scripted::clean();
        runner.check_stdout = report_with(
            "2026-03-02",
            "{\"code\":\"FB0127\",\"severity\":\"warning\",\"message\":\"case miss\",\"line\":4}",
            0,
        );
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut runner).expect("a plain miss does not refuse");
        let ed = zip_map(&out.path().join("fieldbook-edition-001.zip"), "edition");
        let regression = String::from_utf8(ed["evidence/regression.json"].clone()).expect("utf8");
        assert!(regression.contains("\"misses\": 1"), "{regression}");
    }

    #[test]
    fn missing_or_blank_contact_fails_in_01_and_runs_nothing() {
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        for contact in [None, Some(""), Some("   ")] {
            let mut runner = Scripted::clean();
            let err = edition_raw(repo.path(), &args(out.path()), &mut runner, contact)
                .expect_err("IN-01");
            assert!(err.contains("IN-01"), "{err}");
            assert!(runner.calls.is_empty());
        }
    }

    #[test]
    fn unfilled_placeholder_is_refused() {
        let mut values = BTreeMap::new();
        values.insert("a", "1".to_owned());
        assert_eq!(
            fill_placeholders("t", "x {a}", &values).as_deref(),
            Ok("x 1")
        );
        let err = fill_placeholders("t", "x {a} {left_over}", &values).expect_err("left over");
        assert!(err.contains("IN-01") && err.contains("left_over"), "{err}");
        assert!(leftover_placeholders("no braces; {Not-A-Name} and {}").is_empty());
    }

    #[test]
    fn existing_different_output_is_refused_and_left_alone() {
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        fs::write(out.path().join("SHA256SUMS"), b"previous\n").expect("previous");
        let err = package(repo.path(), out.path(), &mut Scripted::clean()).expect_err("collision");
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(
            fs::read(out.path().join("SHA256SUMS")).expect("read"),
            b"previous\n"
        );
        assert_eq!(dir_listing(out.path()), vec!["SHA256SUMS"]);

        // An identical rerun into the same folder is accepted.
        let fresh = tempfile::tempdir().expect("fresh");
        package(repo.path(), fresh.path(), &mut Scripted::clean()).expect("first");
        package(repo.path(), fresh.path(), &mut Scripted::clean()).expect("identical rerun");
    }

    #[test]
    fn changed_catalogue_or_digest_is_caught_by_the_shared_verifier() {
        // The catalogue bytes and the digest line in the archive must agree.
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let ed = zip_map(&out.path().join("fieldbook-edition-001.zip"), "edition");
        let staged = ed["evidence/corpus-support.toml"].clone();
        let mut changed = staged.clone();
        changed.extend_from_slice(b"# changed\n");
        let digest_of_staged = sha256_hex(&staged);
        assert!(
            verify_support_catalogue_bytes(&changed, &digest_of_staged).is_err(),
            "a changed catalogue must not match the staged digest"
        );
        assert!(verify_support_catalogue_bytes(&staged, &digest_of_staged).is_ok());
    }

    #[test]
    fn the_raw_path_declares_no_renderer_dependency_and_calls_no_renderer() {
        // `xtask` itself reaches the renderer only through `fieldbook`, which it needs for the
        // Corpus reader; this checks the declared dependencies and the raw path's own source.
        let manifest = include_str!("../Cargo.toml");
        for banned in ["fieldbook-render", "chart", "headless", "playwright"] {
            assert!(!manifest.contains(banned), "xtask depends on {banned}");
        }
        for (name, source) in [
            ("edition_raw.rs", include_str!("edition_raw.rs")),
            ("raw_release.rs", include_str!("raw_release.rs")),
        ] {
            let production = source.split("#[cfg(test)]").next().expect("source");
            for banned in [
                "fieldbook_render",
                "msedge",
                "--headless",
                "\"build\", \"--out\"",
                "site::render",
            ] {
                assert!(!production.contains(banned), "{name} mentions {banned}");
            }
        }
    }

    fn edition_files(out: &Path) -> BTreeMap<String, Vec<u8>> {
        zip_map(&out.join("fieldbook-edition-001.zip"), "edition")
    }

    fn text_of(files: &BTreeMap<String, Vec<u8>>, name: &str) -> String {
        String::from_utf8(files[name].clone()).expect("utf8")
    }

    /// The lines of the README section whose heading line is `first`, up to the next heading
    /// (an unindented line of capitals) or the end.
    fn paragraph<'a>(readme: &'a str, first: &str) -> Vec<&'a str> {
        let mut lines = readme.lines().skip_while(|l| *l != first);
        assert_eq!(
            lines.next(),
            Some(first),
            "no paragraph opens with `{first}`"
        );
        lines
            .take_while(|l| {
                l.is_empty()
                    || l.starts_with(' ')
                    || l.chars().any(|c| c.is_ascii_lowercase())
                    || !l.chars().any(|c| c.is_ascii_uppercase())
            })
            .collect()
    }

    // control: MW-01
    #[test]
    fn readme_keeps_the_mark_of_the_web_paragraph_and_never_tells_the_reader_to_remove_it() {
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let readme = text_of(&edition_files(out.path()), "README.txt");
        let body = paragraph(&readme, "MARK-OF-THE-WEB").join("\n");
        assert!(body.contains("Leave it in place"), "{body}");
        assert!(body.contains("do not\nrun Unblock-File"), "{body}");
    }

    // control: DR-01
    #[test]
    fn readme_gives_the_working_folder_backup_and_attrib_instruction() {
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let readme = text_of(&edition_files(out.path()), "README.txt");
        let body = paragraph(&readme, "BACK UP YOUR WORK").join("\n");
        assert!(body.contains("attrib +I"), "{body}");
        assert!(body.contains("Get-FileHash"), "{body}");
        assert!(body.contains("makes no backup of its own"), "{body}");
        let support = paragraph(&readme, "SUPPORT EVIDENCE")
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        assert!(
            support.contains("evidence of bytes, not proof that your copy is public"),
            "{support}"
        );
    }

    #[test]
    fn the_files_the_texts_say_you_receive_are_the_delivery_contract_of_the_runbook() {
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let ed = edition_files(out.path());
        let readme = text_of(&ed, "README.txt");

        // The received list: two zips by the channel, the hash list and digest by the other route.
        let received: Vec<&str> = paragraph(&readme, "WHAT YOU RECEIVED")
            .into_iter()
            .filter(|l| l.starts_with("  ") && !l.starts_with("   "))
            .map(|l| l.split_whitespace().next().expect("name"))
            .collect();
        assert_eq!(
            received,
            vec![
                "fieldbook-edition-001.zip",
                "fieldbook-originals-001.zip",
                "SHA256SUMS",
                "corpus-support.sha256",
            ]
        );
        // The loose catalogue copy is a retention copy: it is never listed as received or hashed
        // from beside the zips, and no text says the digest arrives with the zips.
        for (name, text) in [
            ("README.txt", readme.clone()),
            (
                "TRANSFER-STATEMENT.txt",
                text_of(&ed, "TRANSFER-STATEMENT.txt"),
            ),
            (
                "intake/integrity-guide.md",
                text_of(&ed, "intake/integrity-guide.md"),
            ),
        ] {
            assert!(!text.contains("beside the two zips"), "{name}");
            assert!(!text.contains("With the zips"), "{name}");
            assert!(!text.contains(".\\corpus-support.toml"), "{name}");
            assert!(text.contains("evidence\\corpus-support.toml"), "{name}");
        }
        // The runbook says the same: the digest travels by the separate route and the loose
        // catalogue is not sent.
        let runbook = include_str!("../../docs/runbooks/transfer.md");
        assert!(runbook.contains("`corpus-support.sha256` comes by that same separate route"));
        assert!(runbook.contains("it is not sent"));
        // The workflow still uploads the whole delivered set for the owner, who sends the
        // four named files; the fifth is the retention copy.
        let delivered = dir_listing(out.path());
        assert!(delivered.contains(&"corpus-support.toml".to_owned()));
        assert!(!received.contains(&"corpus-support.toml"));
    }

    #[test]
    fn where_to_start_names_only_folders_the_edition_holds() {
        // With extraction text staged the line is present and true.
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let ed = edition_files(out.path());
        let readme = text_of(&ed, "README.txt");
        assert!(readme.contains("edition\\corpus\\sources\\text\\"));
        for folder in start_folders(&readme) {
            assert!(
                ed.keys().any(|p| p.starts_with(&folder)) || folder == "originals/",
                "README names {folder}, which the edition zip does not hold"
            );
        }

        // Without extraction text the line is not written (the real Corpus has none yet).
        let bare = clean_repo();
        git(bare.path(), &["rm", "-r", "-q", "corpus/sources/text"]);
        commit_all(bare.path());
        let out = tempfile::tempdir().expect("out");
        package(bare.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let ed = edition_files(out.path());
        assert!(!ed.keys().any(|p| p.starts_with("corpus/sources/")));
        let readme = text_of(&ed, "README.txt");
        assert!(!readme.contains("sources\\text"), "{readme}");
        for folder in start_folders(&readme) {
            assert!(
                ed.keys().any(|p| p.starts_with(&folder)) || folder == "originals/",
                "README names {folder}, which the edition zip does not hold"
            );
        }
    }

    /// The folders (with `/`) that the WHERE TO START paragraph names.
    fn start_folders(readme: &str) -> Vec<String> {
        paragraph(readme, "WHERE TO START")
            .into_iter()
            .skip(1)
            .filter_map(|l| {
                let first = l.split_whitespace().next()?;
                first
                    .ends_with('\\')
                    .then(|| first.trim_start_matches("edition\\").replace('\\', "/"))
            })
            .collect()
    }

    #[test]
    fn the_statements_name_only_what_the_packaging_step_enforces() {
        let repo = clean_repo();
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut Scripted::clean()).expect("packaging");
        let ed = edition_files(out.path());
        let orig = zip_map(&out.path().join("fieldbook-originals-001.zip"), "originals");

        // The extensions the statements list are the ones the zips hold, and nothing absolute
        // like "nothing in it runs" is claimed.
        let allowed_edition = ["md", "toml", "csv", "json", "txt"];
        for path in ed.keys().filter(|p| p.as_str() != "SHA256SUMS") {
            let ext = path.rsplit('.').next().unwrap_or_default();
            assert!(allowed_edition.contains(&ext), "{path}");
        }
        for path in orig.keys().filter(|p| p.as_str() != "SHA256SUMS") {
            let ext = path.rsplit('.').next().unwrap_or_default();
            assert!(RAW_ORIGINAL_EXTENSIONS.contains(&ext), "{path}");
        }
        let transfer = text_of(&ed, "TRANSFER-STATEMENT.txt");
        assert!(
            transfer.contains("only files with the\nextensions .pdf, .txt and .csv"),
            "{transfer}"
        );
        assert!(text_of(&ed, "intake/network-silence.md").contains(".pdf, .txt and .csv files"));
        for (name, text) in [
            ("README.txt", text_of(&ed, "README.txt")),
            ("TRANSFER-STATEMENT.txt", transfer.clone()),
            (
                "intake/network-silence.md",
                text_of(&ed, "intake/network-silence.md"),
            ),
        ] {
            for banned in [
                "Nothing in it runs",
                "Nothing in this delivery runs",
                "knows how to run",
            ] {
                assert!(!text.contains(banned), "{name} says `{banned}`");
            }
        }
        // The AI section says what this delivery cannot do.
        assert!(transfer.contains("contains no program and no page"));
        // The inherited data-handling sentence about tested interactions is withdrawn first.
        let data = text_of(&ed, "intake/data-handling.md");
        let note = data
            .find("does not apply to this delivery either")
            .expect("the override");
        let tested = data
            .find("in the interactions tested")
            .expect("the inherited sentence");
        assert!(note < tested);
    }

    #[test]
    fn a_registered_original_outside_the_raw_extension_list_is_refused() {
        for ext in ["url", "xls", "css"] {
            let repo = clean_repo();
            git(
                repo.path(),
                &[
                    "mv",
                    "corpus/sources/public/S-0001-v1.html",
                    &format!("corpus/sources/public/S-0001-v1.{ext}"),
                ],
            );
            commit_all(repo.path());
            let (err, out) = refused_with(repo.path(), &mut Scripted::clean());
            assert!(
                err.contains("S-0001-v1") && err.contains(ext),
                "{ext}: {err}"
            );
            assert!(dir_listing(out.path()).is_empty());
        }
    }

    #[test]
    fn regression_evidence_counts_the_set_instead_of_asserting_success() {
        let repo = clean_repo();
        write(
            repo.path(),
            "corpus/eval/regression.toml",
            b"[[case]]\nid = \"rg-001\"\nquery = \"synthetic\"\nexpect = [\"cmp-sample\"]\n\n[[case]]\nid = \"rg-002\"\nquery = \"synthetic title\"\nexpect = [\"cmp-sample\"]\ncritical = true\n\n[[case]]\nid = \"rg-003\"\nquery = \"synthetic example\"\nexpect = [\"cmp-sample\"]\n",
        );
        commit_all(repo.path());
        let mut runner = Scripted::clean();
        runner.check_stdout = report_with(
            "2026-03-02",
            "{\"code\":\"FB0127\",\"severity\":\"warning\",\"message\":\"case miss\",\"line\":4}",
            0,
        );
        let out = tempfile::tempdir().expect("out");
        package(repo.path(), out.path(), &mut runner).expect("packaging");
        let ed = edition_files(out.path());
        let json: serde_json::Value =
            serde_json::from_slice(&ed["evidence/regression.json"]).expect("json");
        assert_eq!(json["cases"], 3);
        assert_eq!(json["critical_cases"], 1);
        assert_eq!(json["critical_misses"], 0);
        assert_eq!(json["misses"], 1);
        assert_eq!(json["passed"], true);
        let transfer = text_of(&ed, "TRANSFER-STATEMENT.txt");
        assert!(
            transfer.contains("regression set has 3 cases,"),
            "{transfer}"
        );
        assert!(
            transfer.contains("1 of them marked critical. 1 regression-miss"),
            "{transfer}"
        );

        // A malformed set is refused before anything is written.
        write(
            repo.path(),
            "corpus/eval/regression.toml",
            b"[[case]]\nid = \"\"\n",
        );
        commit_all(repo.path());
        let (err, _out) = refused_with(repo.path(), &mut Scripted::clean());
        assert!(err.contains("regression.toml"), "{err}");
    }

    #[test]
    fn untrusted_report_and_command_line_inputs_never_panic_on_seeded_input() {
        use std::panic::{AssertUnwindSafe, catch_unwind};
        let valid_report = String::from_utf8(clean_report("2026-03-02")).expect("utf8");
        let set = RegressionSet {
            cases: 3,
            critical_cases: 1,
        };
        let judge = |bytes: &[u8]| {
            judge_check(
                &RunOutput {
                    code: 0,
                    stdout: bytes.to_vec(),
                    stderr: Vec::new(),
                },
                "2026-03-02",
                "edition-001",
                &"a".repeat(40),
                set,
            )
        };
        assert!(judge(valid_report.as_bytes()).is_ok());
        let mut gen_ = crate::compat::rng::Rng::new(0xFB05_2026_0001_0008);
        for _ in 0..200 {
            let bytes = gen_.bytes(256);
            assert!(catch_unwind(AssertUnwindSafe(|| judge(&bytes))).is_ok());
        }
        for idx in 0..valid_report.len() {
            let mut flipped = valid_report.clone().into_bytes();
            flipped[idx] ^= 0x5A;
            let got = catch_unwind(AssertUnwindSafe(|| judge(&flipped)))
                .unwrap_or_else(|_| panic!("judge_check panicked on a flip at {idx}"));
            // A mutation is accepted only when it left the clean meaning intact.
            if let Ok(evidence) = got {
                assert_eq!(evidence.cases, 3);
                assert_eq!(evidence.misses, 0);
            }
            let mut deleted = valid_report.clone().into_bytes();
            deleted.remove(idx);
            assert!(catch_unwind(AssertUnwindSafe(|| judge(&deleted))).is_ok());
        }
        let line = "--out out/dist --fieldbook target/release/fieldbook";
        for idx in 0..line.len() {
            let mut bytes = line.as_bytes().to_vec();
            bytes[idx] ^= 0x5A;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let words: Vec<&str> = text.split_whitespace().collect();
            assert!(catch_unwind(AssertUnwindSafe(|| parse_edition_raw_cli(&words))).is_ok());
        }
        for _ in 0..200 {
            let bytes = gen_.bytes(64);
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let words: Vec<&str> = text.split_whitespace().collect();
            assert!(catch_unwind(AssertUnwindSafe(|| parse_edition_raw_cli(&words))).is_ok());
        }
    }

    #[test]
    fn cli_parsing_is_strict() {
        let ok = parse_edition_raw_cli(&["--out", "d", "--fieldbook", "p"]).expect("ok");
        assert_eq!(ok.out, PathBuf::from("d"));
        assert_eq!(ok.fieldbook, Some(PathBuf::from("p")));
        assert!(parse_edition_raw_cli(&[]).is_err());
        assert!(parse_edition_raw_cli(&["--out"]).is_err());
        assert!(parse_edition_raw_cli(&["--out", "a", "--out", "b"]).is_err());
        assert!(parse_edition_raw_cli(&["--out", "a", "--site"]).is_err());
    }
}
