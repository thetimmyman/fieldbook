//! The known-bad harness (`check-known-bad`, `AD-07`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "known-bad harness reads fixture and card files and runs the binary for 08/09 cases"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use fieldbook_core::check::{Code, Entry, Host, Options, Tree};
use fieldbook_core::date::CivilDate;
use serde::Deserialize;

const FIXTURE_TODAY: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");
const CONTROLS_MD: &str = include_str!("../../../docs/controls.md");

#[derive(Debug, Clone, Deserialize)]
struct CasesFile {
    #[serde(default, rename = "case")]
    cases: Vec<KnownBadCase>,
}

#[derive(Debug, Clone, Deserialize)]
struct KnownBadCase {
    code: String,
    #[serde(default)]
    strict: bool,
    at: String,
    #[serde(default)]
    also: Vec<String>,
    #[serde(default)]
    control: Option<String>,
    #[serde(default)]
    edits: Vec<CaseEdit>,
}

#[derive(Debug, Clone, Deserialize)]
struct CaseEdit {
    path: String,
    #[serde(default)]
    find: Option<String>,
    #[serde(default)]
    replace: Option<String>,
    #[serde(default)]
    create: Option<String>,
    #[serde(default)]
    delete: Option<bool>,
    #[serde(default)]
    append_hex: Option<String>,
    #[serde(default)]
    prepend_hex: Option<String>,
    #[serde(default)]
    crlf: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct PendingFile {
    #[serde(default)]
    pending: Vec<PendingCode>,
    #[serde(default)]
    pending_control: Vec<PendingControl>,
}

#[derive(Debug, Clone, Deserialize)]
struct PendingCode {
    code: String,
    card: String,
}

#[derive(Debug, Clone, Deserialize)]
struct PendingControl {
    control: String,
    card: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ElsewhereFile {
    #[serde(default)]
    elsewhere: Vec<ElsewhereRow>,
}

#[derive(Debug, Clone, Deserialize)]
struct ElsewhereRow {
    code: String,
    card: String,
    test: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct RawPerCodeToml {
    code: Option<String>,
    in_process: Option<String>,
    verb: Option<String>,
    args: Option<Vec<String>>,
    state: Option<String>,
    expect: Option<ExpectedOutcome>,
    http: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ExpectedOutcome {
    code: String,
    #[serde(alias = "exit")]
    status: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InvocationCase {
    verb: String,
    args: Vec<String>,
    state: String,
    expect: ExpectedOutcome,
    http: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PerCodeCase {
    InProcess { code: String, in_process: String },
    Invocation(InvocationCase),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PerCodeParseError(String);

impl fmt::Display for PerCodeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PerCodeParseError {}

fn parse_per_code_case(text: &str) -> Result<PerCodeCase, PerCodeParseError> {
    let raw: RawPerCodeToml =
        toml::from_str(text).map_err(|e| PerCodeParseError(format!("TOML error: {e}")))?;

    if let Some(in_process) = raw.in_process {
        let code = raw
            .code
            .ok_or_else(|| PerCodeParseError("in_process case requires `code` field".to_owned()))?;
        if raw.verb.is_some()
            || raw.args.is_some()
            || raw.state.is_some()
            || raw.expect.is_some()
            || raw.http.is_some()
        {
            return Err(PerCodeParseError(
                "in_process case must not carry invocation or http fields".to_owned(),
            ));
        }
        return Ok(PerCodeCase::InProcess { code, in_process });
    }

    let verb = raw
        .verb
        .ok_or_else(|| PerCodeParseError("invocation case missing required `verb`".to_owned()))?;
    let state = raw
        .state
        .ok_or_else(|| PerCodeParseError("invocation case missing required `state`".to_owned()))?;
    let expect = raw
        .expect
        .ok_or_else(|| PerCodeParseError("invocation case missing required `expect`".to_owned()))?;
    let args = raw.args.unwrap_or_default();

    Ok(PerCodeCase::Invocation(InvocationCase {
        verb,
        args,
        state,
        expect,
        http: raw.http,
    }))
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_root() -> PathBuf {
    repo_root().join("tests/fixtures/corpus-mini")
}

fn known_bad_dir() -> PathBuf {
    repo_root().join("tests/known-bad")
}

fn card_file_exists(root: &Path, card: &str) -> bool {
    root.join("docs/cards").join(format!("{card}.md")).is_file()
        || root
            .join("docs/cards/drafts")
            .join(format!("{card}.md"))
            .is_file()
}

fn decode_hex(hex_str: &str) -> Result<Vec<u8>, String> {
    let clean: String = hex_str.chars().filter(|c| !c.is_whitespace()).collect();
    if !clean.len().is_multiple_of(2) {
        return Err(format!("odd-length hex string `{hex_str}`"));
    }
    let mut out = Vec::with_capacity(clean.len() / 2);
    let bytes = clean.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let pair = std::str::from_utf8(&bytes[i..i + 2]).map_err(|e| e.to_string())?;
        let byte = u8::from_str_radix(pair, 16)
            .map_err(|_| format!("invalid hex byte `{pair}` in `{hex_str}`"))?;
        out.push(byte);
        i += 2;
    }
    Ok(out)
}

fn remove_path_from_tree(tree: &Tree, target: &str) -> Result<Tree, String> {
    if tree.get(target).is_none() {
        return Err(format!("cannot delete missing path `{target}`"));
    }
    let mut next = Tree::new();
    for dir in tree.dirs() {
        next.insert_dir(dir);
    }
    for (p, entry) in tree.iter() {
        if p != target {
            next.insert(p, entry.clone());
        }
    }
    Ok(next)
}

fn apply_edit(tree: &mut Tree, edit: &CaseEdit) -> Result<(), String> {
    if edit.delete == Some(true) {
        *tree = remove_path_from_tree(tree, &edit.path)?;
        return Ok(());
    }

    if let Some(content) = &edit.create {
        if let Some((parent, _)) = edit.path.rsplit_once('/') {
            tree.insert_dir(parent);
        }
        tree.insert_bytes(&edit.path, content.as_bytes().to_vec());
        return Ok(());
    }

    let Some(bytes) = tree.get_bytes(&edit.path).map(<[u8]>::to_vec) else {
        return Err(format!("edit target `{}` not found in tree", edit.path));
    };

    if let Some(find) = &edit.find {
        let replace = edit
            .replace
            .as_deref()
            .ok_or_else(|| format!("edit on `{}` has `find` without `replace`", edit.path))?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| format!("`{}` is not UTF-8: {e}", edit.path))?;
        let count = text.matches(find.as_str()).count();
        if count != 1 {
            return Err(format!(
                "find string `{find}` in `{}` matched {count} times (expected exactly 1)",
                edit.path
            ));
        }
        let updated = text.replacen(find.as_str(), replace, 1);
        tree.insert_bytes(&edit.path, updated.into_bytes());
        return Ok(());
    }

    if let Some(hex) = &edit.append_hex {
        let extra = decode_hex(hex)?;
        let mut out = bytes;
        out.extend_from_slice(&extra);
        tree.insert_bytes(&edit.path, out);
        return Ok(());
    }

    if let Some(hex) = &edit.prepend_hex {
        let mut out = decode_hex(hex)?;
        out.extend_from_slice(&bytes);
        tree.insert_bytes(&edit.path, out);
        return Ok(());
    }

    if edit.crlf == Some(true) {
        let mut out = Vec::with_capacity(bytes.len() + 32);
        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' && (i == 0 || bytes[i - 1] != b'\r') {
                out.push(b'\r');
            }
            out.push(b);
        }
        tree.insert_bytes(&edit.path, out);
        return Ok(());
    }

    Err(format!("edit on `{}` specifies no operation", edit.path))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeRange {
    Fb01To07,
    Fb08To09,
}

impl CodeRange {
    fn contains(self, code_id: &str) -> bool {
        let is_08_09 = code_id.starts_with("FB08") || code_id.starts_with("FB09");
        match self {
            Self::Fb01To07 => !is_08_09,
            Self::Fb08To09 => is_08_09,
        }
    }
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn test_pointer_resolves(root: &Path, pointer: &str) -> bool {
    let trimmed = pointer.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains('/') {
        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
            return false;
        }
        let target = root
            .join("crates")
            .join(parts[0])
            .join("tests")
            .join(format!("{}.rs", parts[1]));
        let Ok(src) = std::fs::read_to_string(target) else {
            return false;
        };
        return src.contains(&format!("fn {}", parts[2]));
    }

    let fn_name = trimmed.rsplit("::").next().unwrap_or(trimmed);
    let needle = format!("fn {fn_name}");
    let mut files = Vec::new();
    collect_rs_files(&root.join("crates"), &mut files);
    collect_rs_files(&root.join("xtask"), &mut files);
    files.into_iter().any(|file| {
        std::fs::read_to_string(file)
            .map(|src| src.contains(&needle))
            .unwrap_or(false)
    })
}

fn validate_elsewhere_pointers(
    root: &Path,
    rows: &[ElsewhereRow],
    range: CodeRange,
) -> Result<(), String> {
    let mut checked = 0usize;
    for row in rows {
        if !range.contains(&row.code) {
            continue;
        }
        checked += 1;
        let open = card_file_exists(root, &row.card);
        if row.test.trim().is_empty() {
            if !open {
                return Err(format!(
                    "elsewhere row `{}` names completed/missing card `{}` with an empty `test` pointer",
                    row.code, row.card
                ));
            }
        } else if !test_pointer_resolves(root, &row.test) {
            return Err(format!(
                "elsewhere row `{}` (card `{}`) names non-existent test `{}`",
                row.code, row.card, row.test
            ));
        }
    }
    if checked == 0 {
        return Err(format!("no elsewhere rows checked for range {range:?}"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ControlRow {
    id: String,
    runs: String,
    brought_by: String,
}

fn parse_controls_md(markdown: &str) -> Vec<ControlRow> {
    let mut out = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed[1..trimmed.len() - 1]
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() != 6 {
            continue;
        }
        let id = cells[0];
        if id == "ID" || id.starts_with("---") || !id.contains('-') {
            continue;
        }
        out.push(ControlRow {
            id: id.to_owned(),
            runs: cells[2].to_owned(),
            brought_by: cells[5].to_owned(),
        });
    }
    out
}

fn runs_cell_names_fieldbook_check(runs_cell: &str) -> bool {
    let fb_check = format!("{} check", fieldbook_core::strings::binary());
    let bt_fb_check = format!("`{fb_check}`");
    let paren_bt_fb_check = format!("(`{fb_check}`)");
    let paren_fb_check = format!("({fb_check})");
    for segment in runs_cell.split(';') {
        let s = segment.trim();
        if s.contains("cargo deny check")
            || s.contains("deps --check")
            || s.contains("tag check")
            || s.contains("tag-check")
        {
            continue;
        }
        if s == "check"
            || s == "`check`"
            || s == fb_check
            || s == bt_fb_check
            || s.contains("(`check`)")
            || s.contains(&paren_bt_fb_check)
            || s.contains("(check)")
            || s.contains(&paren_fb_check)
        {
            return true;
        }
    }
    false
}

fn validate_pending_controls(
    root: &Path,
    controls: &[ControlRow],
    cases: &[KnownBadCase],
    pending_controls: &[PendingControl],
) -> Result<(), String> {
    let check_controls: BTreeMap<&str, &ControlRow> = controls
        .iter()
        .filter(|c| runs_cell_names_fieldbook_check(&c.runs))
        .map(|c| (c.id.as_str(), c))
        .collect();

    let case_controls: BTreeSet<&str> = cases.iter().filter_map(|c| c.control.as_deref()).collect();

    let mut errors = Vec::new();
    let mut pending_map: BTreeMap<&str, &str> = BTreeMap::new();
    for pc in pending_controls {
        if !check_controls.contains_key(pc.control.as_str()) {
            errors.push(format!(
                "pending_control `{}` is not a Fieldbook `check` control in docs/controls.md",
                pc.control
            ));
            continue;
        }
        if !pending_map
            .insert(pc.control.as_str(), pc.card.as_str())
            .is_none()
        {
            errors.push(format!("duplicate pending_control `{}`", pc.control));
        }
        let ctrl_row = check_controls[pc.control.as_str()];
        let named_in_brought_by = ctrl_row.brought_by.contains(&format!("`{}`", pc.card));
        if !named_in_brought_by {
            errors.push(format!(
                "pending_control `{}` names card `{}` which is not in Brought by `{}`",
                pc.control, pc.card, ctrl_row.brought_by
            ));
        }
        if !card_file_exists(root, &pc.card) {
            errors.push(format!(
                "pending_control `{}` names completed or missing provider card `{}`",
                pc.control, pc.card
            ));
        }
    }

    for &id in check_controls.keys() {
        let in_cases = case_controls.contains(id);
        let in_pending = pending_map.contains_key(id);
        match (in_cases, in_pending) {
            (true, true) => {
                errors.push(format!(
                    "check control `{id}` is in both cases.toml and pending.toml"
                ));
            }
            (false, false) => {
                errors.push(format!(
                    "check control `{id}` has neither a known-bad case `control` nor a `[[pending_control]]` row"
                ));
            }
            _ => {}
        }
    }

    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }

    Ok(())
}

fn check_every_code_is_accounted_for(
    root: &Path,
    cases_file: &CasesFile,
    pending_file: &PendingFile,
    elsewhere_file: &ElsewhereFile,
) -> Result<(), String> {
    let mut in_cases = BTreeSet::new();
    for c in &cases_file.cases {
        in_cases.insert(c.code.as_str());
    }

    let mut in_pending: BTreeMap<&str, &str> = BTreeMap::new();
    for p in &pending_file.pending {
        if CodeRange::Fb01To07.contains(&p.code) {
            if in_pending
                .insert(p.code.as_str(), p.card.as_str())
                .is_some()
            {
                return Err(format!("duplicate pending row for `{}`", p.code));
            }
            if !card_file_exists(root, &p.card) {
                return Err(format!(
                    "pending code `{}` names completed or missing card `{}`",
                    p.code, p.card
                ));
            }
        }
    }

    let mut in_elsewhere = BTreeSet::new();
    for e in &elsewhere_file.elsewhere {
        if CodeRange::Fb01To07.contains(&e.code) && !in_elsewhere.insert(e.code.as_str()) {
            return Err(format!("duplicate elsewhere row for `{}`", e.code));
        }
    }

    validate_elsewhere_pointers(root, &elsewhere_file.elsewhere, CodeRange::Fb01To07)?;

    for &code in Code::ALL {
        let id = code.id();
        if !CodeRange::Fb01To07.contains(id) {
            continue;
        }
        let count = usize::from(in_cases.contains(id))
            + usize::from(in_pending.contains_key(id))
            + usize::from(in_elsewhere.contains(id));
        if count == 0 {
            return Err(format!(
                "code `{id}` is not accounted for in cases.toml, pending.toml or elsewhere.toml"
            ));
        }
        if count > 1 {
            return Err(format!(
                "code `{id}` appears in more than one of cases.toml, pending.toml and elsewhere.toml"
            ));
        }
    }

    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}

fn check_every_code_has_a_known_bad_file(
    root: &Path,
    kb_dir: &Path,
    pending_file: &PendingFile,
    elsewhere_file: &ElsewhereFile,
) -> Result<(), String> {
    let mut in_pending: BTreeMap<&str, &str> = BTreeMap::new();
    for p in &pending_file.pending {
        if CodeRange::Fb08To09.contains(&p.code) {
            if in_pending
                .insert(p.code.as_str(), p.card.as_str())
                .is_some()
            {
                return Err(format!("duplicate pending row for `{}`", p.code));
            }
            if !card_file_exists(root, &p.card) {
                return Err(format!(
                    "pending code `{}` names completed or missing card `{}`",
                    p.code, p.card
                ));
            }
        }
    }

    let mut in_elsewhere = BTreeSet::new();
    for e in &elsewhere_file.elsewhere {
        if CodeRange::Fb08To09.contains(&e.code) && !in_elsewhere.insert(e.code.as_str()) {
            return Err(format!("duplicate elsewhere row for `{}`", e.code));
        }
    }

    validate_elsewhere_pointers(root, &elsewhere_file.elsewhere, CodeRange::Fb08To09)?;

    for &code in Code::ALL {
        let id = code.id();
        if !CodeRange::Fb08To09.contains(id) {
            continue;
        }
        let file_path = kb_dir.join(format!("{id}.toml"));
        let has_file = file_path.is_file();
        let has_pending = in_pending.contains_key(id);
        let has_elsewhere = in_elsewhere.contains(id);
        let count = usize::from(has_file) + usize::from(has_pending) + usize::from(has_elsewhere);
        if count == 0 {
            return Err(format!(
                "code `{id}` has neither tests/known-bad/{id}.toml nor a pending/elsewhere row"
            ));
        }
        if count > 1 {
            return Err(format!(
                "code `{id}` appears in more than one of tests/known-bad/{id}.toml, pending.toml and elsewhere.toml"
            ));
        }

        if has_file {
            let text = std::fs::read_to_string(&file_path)
                .map_err(|e| format!("failed to read {}: {e}", file_path.display()))?;
            let parsed =
                parse_per_code_case(&text).map_err(|e| format!("{}: {e}", file_path.display()))?;
            if let PerCodeCase::Invocation(inv) = parsed {
                let state_src = root.join("tests/fixtures").join(&inv.state);
                if !state_src.is_dir() {
                    return Err(format!(
                        "{}: state folder `{}` does not exist under tests/fixtures/",
                        file_path.display(),
                        inv.state
                    ));
                }
                if inv.http.is_none() {
                    let tmp = tempfile::tempdir().map_err(|e| e.to_string())?;
                    let work_corpus = tmp.path().join("work");
                    let work_state = tmp.path().join("state");
                    copy_dir_all(&fixture_root(), &work_corpus).map_err(|e| e.to_string())?;
                    copy_dir_all(&state_src, &work_state).map_err(|e| e.to_string())?;

                    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fieldbook"));
                    cmd.current_dir(&work_corpus);
                    for word in inv.verb.split_whitespace() {
                        cmd.arg(word);
                    }
                    cmd.args(&inv.args);
                    cmd.arg("--state").arg(&work_state);
                    let out = cmd.output().map_err(|e| e.to_string())?;
                    let actual_status =
                        u8::try_from(out.status.code().unwrap_or(255)).unwrap_or(255);
                    let combined = format!(
                        "{}{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                    if actual_status != inv.expect.status || !combined.contains(&inv.expect.code) {
                        return Err(format!(
                            "{}: expected exit {} with `{}`, got exit {actual_status} with output:\n{combined}",
                            file_path.display(),
                            inv.expect.status,
                            inv.expect.code
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

fn check_each_case_raises_its_code(
    base_tree: &Tree,
    today: CivilDate,
    cases_file: &CasesFile,
) -> Result<(), String> {
    for case in &cases_file.cases {
        let mut tree = base_tree.clone();
        for edit in &case.edits {
            apply_edit(&mut tree, edit).map_err(|e| format!("case `{}`: {e}", case.code))?;
        }
        let report =
            fieldbook_core::check::run(&Host::new(tree, today), &Options::new(case.strict));

        let actual_summary: Vec<String> = report
            .diagnostics
            .iter()
            .map(|d| format!("{}:{} ({})", d.path, d.line, d.code.id()))
            .collect();

        let primary_match = report
            .diagnostics
            .iter()
            .any(|d| d.code.id() == case.code && format!("{}:{}", d.path, d.line) == case.at);
        if !primary_match {
            return Err(format!(
                "case `{}`: expected `{}` at `{}`, but got diagnostics: {:?}",
                case.code, case.code, case.at, actual_summary
            ));
        }

        let allowed: BTreeSet<&str> = std::iter::once(case.code.as_str())
            .chain(case.also.iter().map(String::as_str))
            .collect();
        for diag in &report.diagnostics {
            if !allowed.contains(diag.code.id()) {
                return Err(format!(
                    "case `{}` raised unexpected diagnostic `{}` at `{}:{}` (allowed: {:?})",
                    case.code,
                    diag.code.id(),
                    diag.path,
                    diag.line,
                    allowed
                ));
            }
        }
        for also_code in &case.also {
            if !report.diagnostics.iter().any(|d| d.code.id() == also_code) {
                return Err(format!(
                    "case `{}` did not raise expected `also` code `{also_code}` (got {:?})",
                    case.code, actual_summary
                ));
            }
        }
    }
    Ok(())
}

mod known_bad {
    use super::*;

    fn load_files() -> (CasesFile, PendingFile, ElsewhereFile) {
        let kb = known_bad_dir();
        let cases_text = std::fs::read_to_string(kb.join("cases.toml"))
            .unwrap_or_else(|e| panic!("read cases.toml: {e}"));
        let pending_text = std::fs::read_to_string(kb.join("pending.toml"))
            .unwrap_or_else(|e| panic!("read pending.toml: {e}"));
        let elsewhere_text = std::fs::read_to_string(kb.join("elsewhere.toml"))
            .unwrap_or_else(|e| panic!("read elsewhere.toml: {e}"));
        let cases: CasesFile =
            toml::from_str(&cases_text).unwrap_or_else(|e| panic!("parse cases.toml: {e}"));
        let pending: PendingFile =
            toml::from_str(&pending_text).unwrap_or_else(|e| panic!("parse pending.toml: {e}"));
        let elsewhere: ElsewhereFile =
            toml::from_str(&elsewhere_text).unwrap_or_else(|e| panic!("parse elsewhere.toml: {e}"));
        (cases, pending, elsewhere)
    }

    #[test]
    fn the_fixture_is_strict_clean() {
        let tree = fieldbook::io::read_tree(&fixture_root()).expect("read corpus-mini");
        let today = CivilDate::parse(FIXTURE_TODAY.trim()).expect("valid TODAY");
        let report = fieldbook_core::check::run(&Host::new(tree, today), &Options::new(true));
        assert!(
            report.diagnostics.is_empty(),
            "expected corpus-mini to be strict-clean, got:\n{}",
            report.render_text()
        );
    }

    #[test]
    fn each_case_raises_its_code() {
        let (cases, _, _) = load_files();
        let base_tree = fieldbook::io::read_tree(&fixture_root()).expect("read corpus-mini");
        let today = CivilDate::parse(FIXTURE_TODAY.trim()).expect("valid TODAY");
        if let Err(msg) = check_each_case_raises_its_code(&base_tree, today, &cases) {
            panic!("{msg}");
        }

        // Verify wrong `at` line fails and shows both expected and actual lines:
        let mut wrong_at = cases.clone();
        wrong_at.cases[0].at = "corpus/corpus.toml:99".to_owned();
        let err = check_each_case_raises_its_code(&base_tree, today, &wrong_at).unwrap_err();
        assert!(err.contains("corpus/corpus.toml:99"), "{err}");
        assert!(err.contains("corpus/corpus.toml:1"), "{err}");
    }

    #[test]
    fn every_code_is_accounted_for() {
        let root = repo_root();
        let (cases, pending, elsewhere) = load_files();
        if let Err(msg) = check_every_code_is_accounted_for(&root, &cases, &pending, &elsewhere) {
            panic!("{msg}");
        }
    }

    #[test]
    fn every_code_has_a_known_bad_file() {
        let root = repo_root();
        let kb = known_bad_dir();
        let (_, pending, elsewhere) = load_files();
        if let Err(msg) = check_every_code_has_a_known_bad_file(&root, &kb, &pending, &elsewhere) {
            panic!("{msg}");
        }
    }

    #[test]
    fn an_edit_must_apply_exactly_once() {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_bytes("corpus/a.txt", b"alpha\nbeta\nalpha\n".to_vec());

        // 0 occurrences -> error
        let zero_edit = CaseEdit {
            path: "corpus/a.txt".to_owned(),
            find: Some("gamma".to_owned()),
            replace: Some("delta".to_owned()),
            create: None,
            delete: None,
            append_hex: None,
            prepend_hex: None,
            crlf: None,
        };
        assert!(apply_edit(&mut tree, &zero_edit).is_err());

        // 2 occurrences -> error
        let two_edit = CaseEdit {
            path: "corpus/a.txt".to_owned(),
            find: Some("alpha".to_owned()),
            replace: Some("delta".to_owned()),
            create: None,
            delete: None,
            append_hex: None,
            prepend_hex: None,
            crlf: None,
        };
        assert!(apply_edit(&mut tree, &two_edit).is_err());

        // 1 occurrence -> ok
        let one_edit = CaseEdit {
            path: "corpus/a.txt".to_owned(),
            find: Some("beta".to_owned()),
            replace: Some("delta".to_owned()),
            create: None,
            delete: None,
            append_hex: None,
            prepend_hex: None,
            crlf: None,
        };
        assert!(apply_edit(&mut tree, &one_edit).is_ok());
        assert_eq!(
            tree.get_bytes("corpus/a.txt"),
            Some(&b"alpha\ndelta\nalpha\n"[..])
        );

        // create, prepend_hex, append_hex, crlf, delete:
        let create_edit = CaseEdit {
            path: "corpus/b.txt".to_owned(),
            find: None,
            replace: None,
            create: Some("line1\nline2\n".to_owned()),
            delete: None,
            append_hex: None,
            prepend_hex: None,
            crlf: None,
        };
        assert!(apply_edit(&mut tree, &create_edit).is_ok());

        let crlf_edit = CaseEdit {
            path: "corpus/b.txt".to_owned(),
            find: None,
            replace: None,
            create: None,
            delete: None,
            append_hex: None,
            prepend_hex: None,
            crlf: Some(true),
        };
        assert!(apply_edit(&mut tree, &crlf_edit).is_ok());
        assert_eq!(
            tree.get_bytes("corpus/b.txt"),
            Some(&b"line1\r\nline2\r\n"[..])
        );

        let prepend_edit = CaseEdit {
            path: "corpus/b.txt".to_owned(),
            find: None,
            replace: None,
            create: None,
            delete: None,
            append_hex: None,
            prepend_hex: Some("efbbbf".to_owned()),
            crlf: None,
        };
        assert!(apply_edit(&mut tree, &prepend_edit).is_ok());
        assert!(
            tree.get_bytes("corpus/b.txt")
                .expect("b.txt")
                .starts_with(&[0xef, 0xbb, 0xbf])
        );

        let append_edit = CaseEdit {
            path: "corpus/b.txt".to_owned(),
            find: None,
            replace: None,
            create: None,
            delete: None,
            append_hex: Some("ff".to_owned()),
            prepend_hex: None,
            crlf: None,
        };
        assert!(apply_edit(&mut tree, &append_edit).is_ok());
        assert_eq!(
            tree.get_bytes("corpus/b.txt").expect("b.txt").last(),
            Some(&0xff)
        );

        let delete_edit = CaseEdit {
            path: "corpus/b.txt".to_owned(),
            find: None,
            replace: None,
            create: None,
            delete: Some(true),
            append_hex: None,
            prepend_hex: None,
            crlf: None,
        };
        assert!(apply_edit(&mut tree, &delete_edit).is_ok());
        assert!(tree.get("corpus/b.txt").is_none());
        assert_eq!(
            Entry::Original {
                sha256: String::new(),
                len: 0
            }
            .as_bytes(),
            None
        );
    }

    #[test]
    fn every_check_control_has_a_known_bad_case() {
        let root = repo_root();
        let controls = parse_controls_md(CONTROLS_MD);
        let (cases, pending, _) = load_files();
        if let Err(msg) =
            validate_pending_controls(&root, &controls, &cases.cases, &pending.pending_control)
        {
            panic!("{msg}");
        }
    }

    #[test]
    fn pending_controls_follow_owner_completion() {
        let root = repo_root();
        let controls = parse_controls_md(CONTROLS_MD);
        let (cases, pending, _) = load_files();
        if let Err(msg) =
            validate_pending_controls(&root, &controls, &cases.cases, &pending.pending_control)
        {
            panic!("{msg}");
        }

        let scratch = tempfile::tempdir().expect("tempdir");
        let cards_dir = scratch.path().join("docs/cards");
        std::fs::create_dir_all(&cards_dir).expect("create docs/cards");
        std::fs::write(cards_dir.join("check-scans.md"), "# check-scans\n")
            .expect("write check-scans.md");
        std::fs::write(
            cards_dir.join("check-register-links.md"),
            "# check-register-links\n",
        )
        .expect("write check-register-links.md");

        // Multi-owner control SC-02 (`check-scans`, `add-new-and-edition`, `r2-org-intake`, Day 30)
        // passes while its single check provider `check-scans` exists:
        assert!(
            validate_pending_controls(
                scratch.path(),
                &controls,
                &cases.cases,
                &pending.pending_control
            )
            .is_ok()
        );

        // Removing `check-scans.md` while retaining SC-02 pending_control fails naming SC-02 and check-scans:
        std::fs::remove_file(cards_dir.join("check-scans.md")).expect("remove check-scans.md");
        let sc02_only = vec![PendingControl {
            control: "SC-02".to_owned(),
            card: "check-scans".to_owned(),
        }];
        let err = validate_pending_controls(scratch.path(), &controls, &cases.cases, &sc02_only)
            .unwrap_err();
        assert!(err.contains("SC-02"), "{err}");
        assert!(err.contains("check-scans"), "{err}");
    }

    #[test]
    fn runs_cell_selects_only_fieldbook_check() {
        let bin = fieldbook_core::strings::binary();
        let pos_samples = [
            format!("PR CI (`{bin} check`)"),
            format!("`{bin} add`; PR CI (`check`); laptop browser"),
            "PR CI (`check`)".to_owned(),
            "PR CI (`check`); edition build".to_owned(),
            "check".to_owned(),
            format!("`{bin} check`"),
        ];
        for pos in &pos_samples {
            assert!(
                runs_cell_names_fieldbook_check(pos),
                "expected `{pos}` to be selected as a Fieldbook check control"
            );
        }

        for neg in [
            "PR CI (`cargo deny check bans`, default and all features, offline; metadata test per profile)",
            "PR CI (`cargo deny check bans licenses sources`, offline); weekly (control)",
            "PR CI (AGENTS.md length and tag check); the weekly read",
            "deps --check",
            "tag-check",
            "PR CI (clippy); weekly (control)",
        ] {
            assert!(
                !runs_cell_names_fieldbook_check(neg),
                "expected `{neg}` NOT to be selected as a Fieldbook check control"
            );
        }
    }

    #[test]
    fn http_case_keeps_invocation_fields() {
        let sample = r#"
verb = "ask"
args = ["What is the policy?"]
state = "state-mini"
http = "http.toml"

[expect]
code = "FB0905"
status = 1
"#;
        let parsed = parse_per_code_case(sample).expect("valid http invocation case");
        assert_eq!(
            parsed,
            PerCodeCase::Invocation(InvocationCase {
                verb: "ask".to_owned(),
                args: vec!["What is the policy?".to_owned()],
                state: "state-mini".to_owned(),
                expect: ExpectedOutcome {
                    code: "FB0905".to_owned(),
                    status: 1,
                },
                http: Some("http.toml".to_owned()),
            })
        );
    }

    #[test]
    fn http_case_requires_state_and_expect() {
        let missing_state = r#"
verb = "ask"
args = ["What is the policy?"]
http = "http.toml"

[expect]
code = "FB0905"
status = 1
"#;
        let err = parse_per_code_case(missing_state).unwrap_err();
        assert!(err.to_string().contains("state"), "{err}");

        let missing_expect = r#"
verb = "ask"
args = ["What is the policy?"]
state = "state-mini"
http = "http.toml"
"#;
        let err = parse_per_code_case(missing_expect).unwrap_err();
        assert!(err.to_string().contains("expect"), "{err}");
    }

    #[test]
    fn elsewhere_pointers_follow_owner_completion() {
        let root = repo_root();

        // Open/draft cards with empty pointer pass in both ranges (using a synthetic temp root
        // so closing any real repository card never invalidates this fixture):
        let temp_open_root =
            std::env::temp_dir().join(format!("fieldbook-elsewhere-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_open_root);
        std::fs::create_dir_all(temp_open_root.join("docs/cards/drafts"))
            .unwrap_or_else(|e| panic!("create temp card dirs: {e}"));
        std::fs::write(
            temp_open_root.join("docs/cards/synthetic-open.md"),
            "# synthetic-open\n",
        )
        .unwrap_or_else(|e| panic!("write synthetic-open.md: {e}"));
        std::fs::write(
            temp_open_root.join("docs/cards/drafts/synthetic-draft.md"),
            "# synthetic-draft\n",
        )
        .unwrap_or_else(|e| panic!("write synthetic-draft.md: {e}"));

        let open_rows = vec![
            ElsewhereRow {
                code: "FB0501".to_owned(),
                card: "synthetic-open".to_owned(),
                test: String::new(),
            },
            ElsewhereRow {
                code: "FB0807".to_owned(),
                card: "synthetic-draft".to_owned(),
                test: String::new(),
            },
        ];
        assert!(
            validate_elsewhere_pointers(&temp_open_root, &open_rows, CodeRange::Fb01To07).is_ok()
        );
        assert!(
            validate_elsewhere_pointers(&temp_open_root, &open_rows, CodeRange::Fb08To09).is_ok()
        );
        let _ = std::fs::remove_dir_all(&temp_open_root);

        // Completed/missing owner with empty pointer fails in both FB01..=FB07 and FB08..=FB09:
        let completed_empty = vec![
            ElsewhereRow {
                code: "FB0501".to_owned(),
                card: "check-command".to_owned(),
                test: String::new(),
            },
            ElsewhereRow {
                code: "FB0807".to_owned(),
                card: "check-command".to_owned(),
                test: String::new(),
            },
        ];
        let err_01_07 =
            validate_elsewhere_pointers(&root, &completed_empty, CodeRange::Fb01To07).unwrap_err();
        assert!(err_01_07.contains("FB0501"), "{err_01_07}");
        let err_08_09 =
            validate_elsewhere_pointers(&root, &completed_empty, CodeRange::Fb08To09).unwrap_err();
        assert!(err_08_09.contains("FB0807"), "{err_08_09}");

        // Completed owner with nonexistent test pointer fails in both ranges:
        let completed_bad_ptr = vec![
            ElsewhereRow {
                code: "FB0501".to_owned(),
                card: "check-command".to_owned(),
                test: "no_such_test_anywhere".to_owned(),
            },
            ElsewhereRow {
                code: "FB0807".to_owned(),
                card: "check-command".to_owned(),
                test: "fieldbook/cli/no_such_test".to_owned(),
            },
        ];
        let err_01_07 = validate_elsewhere_pointers(&root, &completed_bad_ptr, CodeRange::Fb01To07)
            .unwrap_err();
        assert!(err_01_07.contains("FB0501"), "{err_01_07}");
        let err_08_09 = validate_elsewhere_pointers(&root, &completed_bad_ptr, CodeRange::Fb08To09)
            .unwrap_err();
        assert!(err_08_09.contains("FB0807"), "{err_08_09}");

        // Completed owner with existing test pointer passes in both ranges:
        let completed_valid_ptr = vec![
            ElsewhereRow {
                code: "FB0501".to_owned(),
                card: "check-command".to_owned(),
                test: "version_prints_name_version_and_identity".to_owned(),
            },
            ElsewhereRow {
                code: "FB0807".to_owned(),
                card: "check-command".to_owned(),
                test: "fieldbook/cli/version_prints_name_version_and_identity".to_owned(),
            },
        ];
        assert!(
            validate_elsewhere_pointers(&root, &completed_valid_ptr, CodeRange::Fb01To07).is_ok()
        );
        assert!(
            validate_elsewhere_pointers(&root, &completed_valid_ptr, CodeRange::Fb08To09).is_ok()
        );
    }
}
