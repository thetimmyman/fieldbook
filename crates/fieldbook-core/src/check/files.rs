//! Pass `files`: `FB0120`–`FB0125` (file encoding, layout, normal form, path rules,
//! `corpus/changes.md` and rule files) (`docs/reference/corpus-format.md#layout-of-corpus`).

use std::collections::{BTreeMap, BTreeSet};

use unicode_normalization::is_nfc;

use crate::check::{Code, Diagnostic, Entry, Model, Options, Report};
use crate::ids::{EditionId, MAX_SLUG_ID_LEN};
use crate::schema::CorpusToml;
use crate::schema::corpus_toml::byte_offset_to_line_col;
use crate::schema::{PageType, Register, table};

/// Return the sorted list of `corpus/pages/<area-slug>/<subarea>/<page-id>.md` paths present in
/// `model.tree()` that belong to a declared `(area.slug, subarea)` pair in `corpus/corpus.toml`.
#[must_use]
pub fn page_paths(model: &Model) -> Vec<String> {
    page_paths_for(model.tree(), model.corpus())
}

pub(crate) fn page_paths_for(tree: &super::Tree, corpus: Option<&CorpusToml>) -> Vec<String> {
    let Some(corpus) = corpus else {
        return Vec::new();
    };
    let declared = declared_subareas(corpus);
    let mut out = Vec::new();
    for path in tree.paths() {
        if is_declared_page_path(path, &declared) {
            out.push(path.to_owned());
        }
    }
    out
}

/// Run the `files` check pass (`FB0120`–`FB0125`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    let Some(corpus) = model.corpus() else {
        return;
    };

    let non_utf8_files = check_fb0120(model, options, report);
    check_fb0121(model, corpus, &non_utf8_files, options, report);
    check_fb0122(model, &non_utf8_files, options, report);
    check_fb0123(model, &non_utf8_files, options, report);
    check_fb0124(model, &non_utf8_files, options, report);
    check_fb0125(model, &non_utf8_files, options, report);
}

fn is_register_path(path: &str) -> bool {
    let Some(name) = path.strip_prefix("corpus/registers/") else {
        return false;
    };
    Register::ALL.iter().any(|reg| reg.file_name() == name)
}

fn declared_areas(corpus: &CorpusToml) -> BTreeSet<&str> {
    corpus.area.iter().map(|a| a.slug.as_str()).collect()
}

fn declared_subareas(corpus: &CorpusToml) -> BTreeSet<(&str, &str)> {
    let mut out = BTreeSet::new();
    for area in &corpus.area {
        for sub in &area.subareas {
            out.insert((area.slug.as_str(), sub.as_str()));
        }
    }
    out
}

fn is_declared_page_path(path: &str, declared: &BTreeSet<(&str, &str)>) -> bool {
    let Some(rest) = path.strip_prefix("corpus/pages/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let (Some(area_slug), Some(subarea), Some(file), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Some(stem) = file.strip_suffix(".md") else {
        return false;
    };
    !stem.is_empty() && declared.contains(&(area_slug, subarea))
}

fn is_page_txt_name(file: &str) -> bool {
    if file.len() != 9 || !file.starts_with('p') || !file.ends_with(".txt") {
        return false;
    }
    let digits = &file[1..5];
    digits.bytes().all(|b| b.is_ascii_digit()) && digits != "0000"
}

fn is_allowed_file_path(path: &str, declared: &BTreeSet<(&str, &str)>) -> bool {
    if matches!(
        path,
        "corpus/AGENTS.md"
            | "corpus/ASSISTANT.md"
            | "corpus/README.md"
            | "corpus/corpus.toml"
            | "corpus/changes.md"
            | "corpus/scan-allow.toml"
            | "corpus/eval/regression.toml"
            | "corpus/pipeline/rules.toml"
    ) {
        return true;
    }
    if is_register_path(path) {
        return true;
    }
    if let Some(rest) = path.strip_prefix("corpus/templates/workbooks/") {
        return !rest.contains('/')
            && rest
                .strip_suffix(".csv")
                .is_some_and(|stem| !stem.is_empty());
    }
    if let Some(rest) = path.strip_prefix("corpus/templates/") {
        return !rest.contains('/')
            && rest
                .strip_suffix(".md")
                .is_some_and(|stem| PageType::ALL.iter().any(|t| t.as_str() == stem));
    }
    if is_declared_page_path(path, declared) {
        return true;
    }
    if let Some(rest) = path.strip_prefix("corpus/sources/public/") {
        return !rest.is_empty() && !rest.contains('/');
    }
    if let Some(rest) = path.strip_prefix("corpus/sources/text/") {
        let mut parts = rest.split('/');
        let (Some(dir), Some(file), None) = (parts.next(), parts.next(), parts.next()) else {
            return false;
        };
        return !dir.is_empty() && (file == "extraction.toml" || is_page_txt_name(file));
    }
    false
}

fn is_allowed_dir_path(
    dir: &str,
    areas: &BTreeSet<&str>,
    declared: &BTreeSet<(&str, &str)>,
) -> bool {
    if matches!(
        dir,
        "corpus"
            | "corpus/pages"
            | "corpus/registers"
            | "corpus/sources"
            | "corpus/sources/public"
            | "corpus/sources/text"
            | "corpus/templates"
            | "corpus/templates/workbooks"
            | "corpus/eval"
            | "corpus/pipeline"
    ) {
        return true;
    }
    if let Some(rest) = dir.strip_prefix("corpus/pages/") {
        if !rest.contains('/') {
            return areas.contains(rest);
        }
        let mut parts = rest.split('/');
        let (Some(area_slug), Some(subarea), None) = (parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        return declared.contains(&(area_slug, subarea));
    }
    if let Some(rest) = dir.strip_prefix("corpus/sources/text/") {
        return !rest.is_empty() && !rest.contains('/');
    }
    false
}

fn is_strict_ancestor(ancestor: &str, descendant: &str) -> bool {
    descendant.len() > ancestor.len()
        && descendant.starts_with(ancestor)
        && descendant.as_bytes().get(ancestor.len()) == Some(&b'/')
}

fn line_col_at_valid_prefix(prefix: &str) -> (usize, usize) {
    let (line, col) = byte_offset_to_line_col(prefix, prefix.len());
    (
        usize::try_from(line).unwrap_or(1),
        usize::try_from(col).unwrap_or(1),
    )
}

/// `FB0120`: non-UTF-8 text file (excluding originals and the five registers).
fn check_fb0120<'a>(model: &'a Model, options: &Options, report: &mut Report) -> BTreeSet<&'a str> {
    let mut bad = BTreeSet::new();
    for (path, entry) in model.tree().iter() {
        if !path.starts_with("corpus/")
            || path.starts_with("corpus/sources/public/")
            || is_register_path(path)
        {
            continue;
        }
        if let Entry::Bytes(bytes) = entry
            && let Err(err) = std::str::from_utf8(bytes)
        {
            let valid_prefix = std::str::from_utf8(&bytes[..err.valid_up_to()]).unwrap_or("");
            let (line, col) = line_col_at_valid_prefix(valid_prefix);
            report.push(Diagnostic::new(
                Code::Fb0120,
                options.strict,
                path,
                line,
                col,
                "file is not valid UTF-8",
            ));
            bad.insert(path);
        }
    }
    bad
}

/// `FB0121`: unexpected file or directory in `corpus/`.
fn check_fb0121(
    model: &Model,
    corpus: &CorpusToml,
    non_utf8_files: &BTreeSet<&str>,
    options: &Options,
    report: &mut Report,
) {
    let areas = declared_areas(corpus);
    let declared = declared_subareas(corpus);

    let mut unexpected_files: Vec<&str> = Vec::new();
    for path in model.tree().paths() {
        if !path.starts_with("corpus/") || non_utf8_files.contains(path) {
            continue;
        }
        if !is_allowed_file_path(path, &declared) {
            unexpected_files.push(path);
            report.push(Diagnostic::new(
                Code::Fb0121,
                options.strict,
                path,
                1,
                1,
                "unexpected file in corpus/",
            ));
        }
    }

    let unexpected_dirs: Vec<&str> = model
        .tree()
        .dirs()
        .filter(|&d| d == "corpus" || d.starts_with("corpus/"))
        .filter(|&d| !is_allowed_dir_path(d, &areas, &declared))
        .collect();

    for &dir in &unexpected_dirs {
        let has_reported_descendant = unexpected_files.iter().any(|&f| is_strict_ancestor(dir, f))
            || unexpected_dirs
                .iter()
                .any(|&d2| is_strict_ancestor(dir, d2))
            || non_utf8_files.iter().any(|&f| is_strict_ancestor(dir, f));
        if !has_reported_descendant {
            report.push(Diagnostic::new(
                Code::Fb0121,
                options.strict,
                dir,
                1,
                1,
                "unexpected directory in corpus/",
            ));
        }
    }
}

/// `FB0122`: file is not in normal form.
fn check_fb0122(
    model: &Model,
    non_utf8_files: &BTreeSet<&str>,
    options: &Options,
    report: &mut Report,
) {
    for reg in Register::ALL {
        let path = format!("corpus/registers/{}", reg.file_name());
        if let Some(bytes) = model.tree().get_bytes(&path)
            && table::read(bytes).is_ok()
            && !table::is_normal(reg, bytes)
        {
            report.push(Diagnostic::new(
                Code::Fb0122,
                options.strict,
                path,
                1,
                1,
                "register is not in normal form",
            ));
        }
    }

    for (path, entry) in model.tree().iter() {
        if !path.starts_with("corpus/")
            || path.starts_with("corpus/sources/")
            || is_register_path(path)
            || non_utf8_files.contains(path)
        {
            continue;
        }
        let Entry::Bytes(bytes) = entry else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        if let Some((line, col, msg)) = first_text_normal_form_fault(bytes, text) {
            report.push(Diagnostic::new(
                Code::Fb0122,
                options.strict,
                path,
                line,
                col,
                msg,
            ));
        }
    }
}

fn first_text_normal_form_fault(bytes: &[u8], text: &str) -> Option<(usize, usize, &'static str)> {
    // 1. UTF-8 BOM.
    if text.starts_with('\u{feff}') {
        return Some((1, 1, "file starts with a UTF-8 byte order mark"));
    }

    // 2. CR (`\r`).
    if let Some(cr_idx) = text.find('\r') {
        let (line, col) = line_col_at_valid_prefix(&text[..cr_idx]);
        return Some((line, col, "file contains CR line ending; use LF"));
    }

    // 3. Non-NFC characters.
    if !is_nfc(text) {
        for (idx, line_str) in text.split('\n').enumerate() {
            if !is_nfc(line_str) {
                return Some((idx + 1, 1, "line is not in Unicode NFC normal form"));
            }
        }
        return Some((1, 1, "file is not in Unicode NFC normal form"));
    }

    // 4. Trailing whitespace (`' '` or `'\t'` before `\n` or EOF).
    for (idx, line_str) in text.split('\n').enumerate() {
        if line_str.ends_with(' ') || line_str.ends_with('\t') {
            let col = line_str.chars().count();
            return Some((idx + 1, col, "line has trailing whitespace"));
        }
    }

    // 5. File does not end with a single `\n` (empty, no trailing `\n`, or ending with `\n\n`).
    if text.is_empty() {
        return Some((1, 1, "file is empty; must end with a single newline"));
    }
    if !text.ends_with('\n') {
        let last_line = bytes.iter().filter(|&&b| b == b'\n').count() + 1;
        return Some((last_line, 1, "file does not end with a newline"));
    }
    if text.ends_with("\n\n") {
        let last_line = bytes.iter().filter(|&&b| b == b'\n').count();
        return Some((last_line, 1, "file ends with a blank line"));
    }

    None
}

/// `FB0123`: path breaks a path rule.
fn check_fb0123(
    model: &Model,
    non_utf8_files: &BTreeSet<&str>,
    options: &Options,
    report: &mut Report,
) {
    let mut seen_lower: BTreeSet<String> = BTreeSet::new();
    let mut seen_dir_lower: BTreeMap<String, String> = BTreeMap::new();
    let mut reported_colliding_dirs: BTreeSet<String> = BTreeSet::new();

    // Collect all paths to check: every file path (except non-UTF-8 files skipped by FB0120)
    // plus any leaf explicit directories that have no files or deeper directories under them.
    let file_paths: Vec<&str> = model
        .tree()
        .paths()
        .filter(|&p| p.starts_with("corpus/") && !non_utf8_files.contains(p))
        .collect();
    let dir_paths: Vec<&str> = model
        .tree()
        .dirs()
        .filter(|&d| d.starts_with("corpus/"))
        .collect();

    let mut all_paths: BTreeSet<&str> = BTreeSet::new();
    for &p in &file_paths {
        all_paths.insert(p);
    }
    for &d in &dir_paths {
        let has_descendant = file_paths.iter().any(|&f| is_strict_ancestor(d, f))
            || dir_paths.iter().any(|&d2| is_strict_ancestor(d, d2));
        if !has_descendant {
            all_paths.insert(d);
        }
    }

    for path in all_paths {
        let lower = path.to_lowercase();
        let mut case_collision = !seen_lower.insert(lower);
        for (idx, _) in path.match_indices('/') {
            let prefix = &path[..idx];
            let lower_prefix = prefix.to_lowercase();
            match seen_dir_lower.get(&lower_prefix) {
                Some(first) if first != prefix => {
                    if reported_colliding_dirs.insert(prefix.to_owned()) {
                        case_collision = true;
                    }
                }
                Some(_) => {}
                None => {
                    seen_dir_lower.insert(lower_prefix, prefix.to_owned());
                }
            }
        }
        if let Some(msg) = first_path_rule_fault(path, case_collision) {
            report.push(Diagnostic::new(
                Code::Fb0123,
                options.strict,
                path,
                1,
                1,
                msg,
            ));
        }
    }
}

fn first_path_rule_fault(path: &str, case_collision: bool) -> Option<&'static str> {
    // 1. Path length over 150 characters.
    if path.chars().count() > 150 {
        return Some("path exceeds 150 characters");
    }

    // 2. Two paths whose lowercase forms are equal.
    if case_collision {
        return Some("path collides with another path when lowercased");
    }

    // 3. Segment rules.
    for seg in path.split('/') {
        let stem = seg.split('.').next().unwrap_or(seg);
        if is_windows_reserved(stem) {
            return Some("path segment uses a Windows reserved name");
        }
        if seg.ends_with('.') || seg.ends_with(' ') {
            return Some("path segment ends with a dot or space");
        }
        if seg.chars().any(is_forbidden_path_char) {
            return Some("path segment contains a forbidden or control character");
        }
        if !is_nfc(seg) {
            return Some("path segment is not in Unicode NFC normal form");
        }
    }

    // 4. Page file `<page-id>` stem over 64 characters.
    if path.starts_with("corpus/pages/")
        && let Some(file) = path.rsplit('/').next()
        && let Some(stem) = file.strip_suffix(".md")
        && stem.chars().count() > MAX_SLUG_ID_LEN
    {
        return Some("page id stem exceeds 64 characters");
    }

    // 5. Original file (`corpus/sources/public/<name>`) must split on the last `.` into
    // `<version-id>.<ext>` with `<version-id>` at most 64 characters and `<ext>` 1..=5
    // lowercase ASCII letters or digits.
    if let Some(name) = path.strip_prefix("corpus/sources/public/")
        && !name.contains('/')
        && !is_valid_original_filename(name)
    {
        return Some(
            "original filename must be <version-id>.<ext> with stem <= 64 chars and 1..=5 lowercase ASCII alphanumeric extension",
        );
    }

    None
}

fn is_windows_reserved(stem: &str) -> bool {
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

fn is_forbidden_path_char(c: char) -> bool {
    matches!(c, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*') || c.is_control()
}

fn is_valid_original_filename(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    let stem_len = stem.chars().count();
    if stem_len == 0 || stem_len > MAX_SLUG_ID_LEN {
        return false;
    }
    (1..=5).contains(&ext.len())
        && ext
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// `FB0124`: `corpus/changes.md` is malformed.
fn check_fb0124(
    model: &Model,
    non_utf8_files: &BTreeSet<&str>,
    options: &Options,
    report: &mut Report,
) {
    let path = "corpus/changes.md";
    if non_utf8_files.contains(path) {
        return;
    }
    let Some(bytes) = model.tree().get_bytes(path) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return;
    };

    let lines: Vec<&str> = text.lines().collect();

    // Line 1: `# ` followed by non-empty title.
    let line1_ok = lines
        .first()
        .and_then(|l| l.strip_prefix("# "))
        .is_some_and(|title| !title.trim().is_empty());
    if !line1_ok {
        report.push(Diagnostic::new(
            Code::Fb0124,
            options.strict,
            path,
            1,
            1,
            "line 1 of corpus/changes.md must start with `# <title>`",
        ));
        return;
    }

    // Line 2: blank.
    match lines.get(1) {
        Some(&"") => {}
        Some(_) => {
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                2,
                1,
                "line 2 of corpus/changes.md must be blank",
            ));
            return;
        }
        None => {
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                1,
                1,
                "corpus/changes.md is missing its blank line 2 and explanation line 3",
            ));
            return;
        }
    }

    // Line 3: non-empty, non-heading explanation line.
    match lines.get(2) {
        Some(line3) if !line3.trim().is_empty() && !line3.starts_with('#') => {}
        Some(_) => {
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                3,
                1,
                "line 3 of corpus/changes.md must be a non-empty, non-heading explanation line",
            ));
            return;
        }
        None => {
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                2,
                1,
                "corpus/changes.md is missing its explanation line 3",
            ));
            return;
        }
    }

    let mut prev_edition: Option<u32> = None;
    let mut in_section = false;

    for (idx, &line) in lines.iter().enumerate().skip(3) {
        let line_no = idx + 1;
        if line.is_empty() {
            continue;
        }
        if line.starts_with('#') {
            let Some(rest) = line.strip_prefix("## ") else {
                report.push(Diagnostic::new(
                    Code::Fb0124,
                    options.strict,
                    path,
                    line_no,
                    1,
                    "section heading must be `## edition-NNN`",
                ));
                continue;
            };
            let Ok(ed) = EditionId::parse(rest) else {
                report.push(Diagnostic::new(
                    Code::Fb0124,
                    options.strict,
                    path,
                    line_no,
                    1,
                    "section heading must be a valid `## edition-NNN` identifier",
                ));
                continue;
            };
            if let Some(prev) = prev_edition
                && ed.number() >= prev
            {
                report.push(Diagnostic::new(
                    Code::Fb0124,
                    options.strict,
                    path,
                    line_no,
                    1,
                    "sections in corpus/changes.md must be ordered newest first (strictly decreasing)",
                ));
            }
            prev_edition = Some(ed.number());
            in_section = true;
            continue;
        }

        if !in_section {
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                line_no,
                1,
                "content after line 3 must appear inside a `## edition-NNN` section",
            ));
            continue;
        }

        let bullet_body = line
            .strip_prefix("- Added: ")
            .or_else(|| line.strip_prefix("- Changed: "))
            .or_else(|| line.strip_prefix("- Verified: "));
        let Some(body) = bullet_body else {
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                line_no,
                1,
                "section entry must start with `- Added: `, `- Changed: ` or `- Verified: `",
            ));
            continue;
        };
        if body.trim().is_empty() {
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                line_no,
                1,
                "section entry text after bullet prefix must not be empty",
            ));
            continue;
        }
        if let Some(byte_idx) = find_yyyy_mm_dd(line) {
            let col = Diagnostic::col_at_byte(line, byte_idx);
            report.push(Diagnostic::new(
                Code::Fb0124,
                options.strict,
                path,
                line_no,
                col,
                "entries in corpus/changes.md must not carry YYYY-MM-DD dates",
            ));
        }
    }
}

fn find_yyyy_mm_dd(line: &str) -> Option<usize> {
    let bytes = line.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    for i in 0..=(bytes.len() - 10) {
        let w = &bytes[i..i + 10];
        let is_date = w[0].is_ascii_digit()
            && w[1].is_ascii_digit()
            && w[2].is_ascii_digit()
            && w[3].is_ascii_digit()
            && w[4] == b'-'
            && w[5].is_ascii_digit()
            && w[6].is_ascii_digit()
            && w[7] == b'-'
            && w[8].is_ascii_digit()
            && w[9].is_ascii_digit();
        if is_date {
            return Some(i);
        }
    }
    None
}

/// `FB0125`: rule file missing or over 40 lines (`corpus/ASSISTANT.md`, `corpus/AGENTS.md`).
fn check_fb0125(
    model: &Model,
    non_utf8_files: &BTreeSet<&str>,
    options: &Options,
    report: &mut Report,
) {
    let assistant_path = "corpus/ASSISTANT.md";
    if model.tree().get(assistant_path).is_none() {
        report.push(Diagnostic::new(
            Code::Fb0125,
            options.strict,
            assistant_path,
            1,
            1,
            "missing required rule file corpus/ASSISTANT.md",
        ));
    } else if !non_utf8_files.contains(assistant_path)
        && let Some(bytes) = model.tree().get_bytes(assistant_path)
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        let first_line = text.lines().next().unwrap_or("");
        if !has_valid_rules_version(first_line) {
            report.push(Diagnostic::new(
                Code::Fb0125,
                options.strict,
                assistant_path,
                1,
                1,
                "line 1 of corpus/ASSISTANT.md must be `rules_version = <positive integer>`",
            ));
        }
        if text.lines().count() > 40 {
            report.push(Diagnostic::new(
                Code::Fb0125,
                options.strict,
                assistant_path,
                41,
                1,
                "corpus/ASSISTANT.md exceeds 40 lines",
            ));
        }
    }

    let agents_path = "corpus/AGENTS.md";
    if !non_utf8_files.contains(agents_path)
        && let Some(bytes) = model.tree().get_bytes(agents_path)
        && let Ok(text) = std::str::from_utf8(bytes)
        && text.lines().count() > 40
    {
        report.push(Diagnostic::new(
            Code::Fb0125,
            options.strict,
            agents_path,
            41,
            1,
            "corpus/AGENTS.md exceeds 40 lines",
        ));
    }
}

fn has_valid_rules_version(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("rules_version = ") else {
        return false;
    };
    !rest.is_empty()
        && rest.bytes().all(|b| b.is_ascii_digit())
        && rest.parse::<u64>().is_ok_and(|n| n >= 1)
}

#[cfg(test)]
mod tests {
    use crate::check::{Code, Entry, Host, Options, Tree, load, run};
    use crate::date::CivilDate;
    use crate::schema::{Register, table};

    use super::page_paths;

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

    const PAGE_1: (&str, &str) = (
        "corpus/pages/compliance/business-systems/cmp-review-steps.md",
        include_str!(
            "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-review-steps.md"
        ),
    );
    const PAGE_2: (&str, &str) = (
        "corpus/pages/compliance/business-systems/cmp-system-review.md",
        include_str!(
            "../../../../tests/fixtures/corpus-mini/corpus/pages/compliance/business-systems/cmp-system-review.md"
        ),
    );
    const PAGE_3: (&str, &str) = (
        "corpus/pages/estimating/pricing/est-rate-window-brief.md",
        include_str!(
            "../../../../tests/fixtures/corpus-mini/corpus/pages/estimating/pricing/est-rate-window-brief.md"
        ),
    );

    fn sample_today() -> CivilDate {
        CivilDate::from_ymd(2026, 3, 1).expect("valid date")
    }

    fn mini_tree() -> Tree {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_dir("corpus/pages/estimating/pricing");
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
        for (path, content) in [PAGE_1, PAGE_2, PAGE_3] {
            tree.insert_bytes(path, content.as_bytes());
        }
        tree
    }

    #[test]
    fn clean_mini_fixture_and_page_paths() {
        let host = Host::new(mini_tree(), sample_today());
        let (model, report) = load(&host, &Options::default());
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let pages = page_paths(&model);
        assert_eq!(
            pages,
            vec![
                PAGE_1.0.to_owned(),
                PAGE_2.0.to_owned(),
                PAGE_3.0.to_owned()
            ]
        );
    }

    #[test]
    fn fb0120_reports_non_utf8_with_line_and_col_and_skips_originals_and_registers() {
        let mut tree = mini_tree();
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-bad-utf8.md",
            b"line one\nabc\xffdef\n".to_vec(),
        );
        tree.insert_bytes("corpus/changes.md", b"# Changes\n\nok\x80\n".to_vec());
        // Invalid UTF-8 in registers is handled by check-registers (FB0101), not FB0120.
        tree.insert_bytes("corpus/registers/glossary.csv", b"term_id\n\xff\n".to_vec());
        tree.insert(
            "corpus/sources/public/S-0001-v1.pdf",
            Entry::Original {
                sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_owned(),
                len: 10,
            },
        );

        let report = run(&Host::new(tree, sample_today()), &Options::default());
        let fb0120: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0120)
            .collect();
        assert_eq!(fb0120.len(), 2, "{:?}", report.diagnostics);

        let d0 = fb0120[0];
        assert_eq!(d0.code, Code::Fb0120);
        assert_eq!(d0.path, "corpus/changes.md");
        assert_eq!((d0.line, d0.col), (3, 3));

        let d1 = fb0120[1];
        assert_eq!(d1.code, Code::Fb0120);
        assert_eq!(
            d1.path,
            "corpus/pages/compliance/business-systems/cmp-bad-utf8.md"
        );
        assert_eq!((d1.line, d1.col), (2, 4));
    }

    #[test]
    fn fb0121_reports_unexpected_files_and_directories() {
        let cases: &[(&str, bool)] = &[
            ("corpus/notes.txt", false),
            ("corpus/pages/unknown/sub/p.md", false),
            ("corpus/pages/compliance/unknown/p.md", false),
            ("corpus/registers/extra.csv", false),
            ("corpus/templates/unknown.md", false),
            ("corpus/unexpected-empty-dir", true),
        ];

        for &(path, is_dir) in cases {
            let mut tree = mini_tree();
            if is_dir {
                tree.insert_dir(path);
            } else {
                if let Some((parent, _)) = path.rsplit_once('/') {
                    tree.insert_dir(parent);
                }
                tree.insert_bytes(path, b"valid text\n");
            }
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            assert_eq!(
                report.diagnostics.len(),
                1,
                "case {path}: {:?}",
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].code, Code::Fb0121);
            assert_eq!(report.diagnostics[0].path, path);
            assert_eq!(
                (report.diagnostics[0].line, report.diagnostics[0].col),
                (1, 1)
            );
        }
    }

    #[test]
    fn trailing_whitespace_is_not_normal() {
        let mut tree = mini_tree();
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/cmp-review-steps.md",
            b"line 1\nline 2 \nline 3\n",
        );
        let report = run(&Host::new(tree, sample_today()), &Options::default());
        let fb0122: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0122)
            .collect();
        assert_eq!(fb0122.len(), 1, "{:?}", report.diagnostics);
        let d = fb0122[0];
        assert_eq!(d.code, Code::Fb0122);
        assert_eq!(
            d.path,
            "corpus/pages/compliance/business-systems/cmp-review-steps.md"
        );
        assert_eq!((d.line, d.col), (2, 7));
    }

    #[test]
    fn a_register_out_of_order_is_not_normal() {
        let parsed = table::read(MINI_GLOSSARY_CSV.as_bytes()).expect("valid glossary csv");
        let mut scrambled = parsed;
        scrambled.rows.reverse();
        assert!(
            scrambled.rows.len() >= 2,
            "fixture glossary must have at least 2 rows"
        );
        let raw_bytes = table::write(&scrambled);
        assert!(!table::is_normal(Register::Glossary, raw_bytes.as_bytes()));

        let mut tree = mini_tree();
        tree.insert_bytes("corpus/registers/glossary.csv", raw_bytes);
        let report = run(&Host::new(tree, sample_today()), &Options::default());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0122);
        assert_eq!(report.diagnostics[0].path, "corpus/registers/glossary.csv");
    }

    #[test]
    fn register_normal_form_agrees_with_checker() {
        let parsed = table::read(MINI_GLOSSARY_CSV.as_bytes()).expect("valid glossary csv");
        let mut scrambled = parsed;
        scrambled.rows.reverse();
        let scrambled_bytes = table::write(&scrambled);

        let mut tree = mini_tree();
        tree.insert_bytes("corpus/registers/glossary.csv", scrambled_bytes.clone());
        let report_bad = run(
            &Host::new(tree.clone(), sample_today()),
            &Options::default(),
        );
        assert!(
            report_bad
                .diagnostics
                .iter()
                .any(|d| d.code == Code::Fb0122 && d.path == "corpus/registers/glossary.csv")
        );

        let normalized =
            table::normalize(Register::Glossary, scrambled_bytes.as_bytes()).expect("normalizes");
        tree.insert_bytes("corpus/registers/glossary.csv", normalized);
        let report_clean = run(&Host::new(tree, sample_today()), &Options::default());
        assert!(
            report_clean.diagnostics.is_empty(),
            "expected clean after table::normalize, got {:?}",
            report_clean.diagnostics
        );
    }

    #[test]
    fn fb0122_checks_all_text_normal_form_faults() {
        let target = "corpus/pages/compliance/business-systems/cmp-review-steps.md";
        let cases: &[(&[u8], usize)] = &[
            ("\u{feff}hello\n".as_bytes(), 1),
            (b"line1\r\nline2\n", 1),
            (b"line1\nline2\rline3\n", 2),
            ("line1\ncafe\u{0301}\n".as_bytes(), 2),
            (b"line1\nline2\t\n", 2),
            (b"line1\nline2", 2),
            (b"line1\nline2\n\n", 3),
            (b"", 1),
        ];

        for &(bytes, expected_line) in cases {
            let mut tree = mini_tree();
            tree.insert_bytes(target, bytes.to_vec());
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            let fb0122: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|d| d.code == Code::Fb0122)
                .collect();
            assert_eq!(fb0122.len(), 1, "{:?}", report.diagnostics);
            assert_eq!(fb0122[0].code, Code::Fb0122);
            assert_eq!(fb0122[0].path, target);
            assert_eq!(fb0122[0].line, expected_line);
        }
    }

    #[test]
    fn fb0123_checks_all_path_rules() {
        // 1. Over 150 characters (using an original path so stem <= 64 is not what trips first).
        let long_path = format!(
            "corpus/pages/compliance/business-systems/{}.md",
            "a".repeat(110)
        );
        assert!(long_path.chars().count() > 150);
        let mut tree = mini_tree();
        tree.insert_bytes(&long_path, b"ok\n");
        let report = run(&Host::new(tree, sample_today()), &Options::default());
        let fb0123: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0123)
            .collect();
        assert_eq!(fb0123.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(fb0123[0].code, Code::Fb0123);
        assert_eq!(fb0123[0].path, long_path);

        // 2. Case collision (file and directory).
        let mut tree = mini_tree();
        tree.insert_bytes(
            "corpus/pages/compliance/business-systems/Cmp-Review-Steps.md",
            b"ok\n",
        );
        let report = run(&Host::new(tree, sample_today()), &Options::default());
        let fb0123: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0123)
            .collect();
        assert_eq!(fb0123.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(fb0123[0].code, Code::Fb0123);

        let mut tree = mini_tree();
        tree.insert_bytes("corpus/sources/text/S-0001-v1/p0001.txt", b"ok\n");
        tree.insert_bytes("corpus/sources/text/s-0001-v1/p0002.txt", b"ok\n");
        let report = run(&Host::new(tree, sample_today()), &Options::default());
        let fb0123: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0123)
            .collect();
        assert_eq!(fb0123.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(fb0123[0].code, Code::Fb0123);
        assert_eq!(fb0123[0].path, "corpus/sources/text/s-0001-v1/p0002.txt");

        // 3. Windows reserved names, trailing dot/space, forbidden chars, control char, NFD segment,
        // 65-char page stem.
        let page_bad_segments = [
            "corpus/pages/compliance/business-systems/CON.md",
            "corpus/pages/compliance/business-systems/nul.md",
            "corpus/pages/compliance/business-systems/com1.md",
            "corpus/pages/compliance/business-systems/LPT9.txt.md",
            "corpus/sources/text/bad./extraction.toml",
            "corpus/sources/text/bad /extraction.toml",
            "corpus/pages/compliance/business-systems/bad<1.md",
            "corpus/pages/compliance/business-systems/bad>1.md",
            "corpus/pages/compliance/business-systems/bad:1.md",
            "corpus/pages/compliance/business-systems/bad\"1.md",
            "corpus/pages/compliance/business-systems/bad\\1.md",
            "corpus/pages/compliance/business-systems/bad|1.md",
            "corpus/pages/compliance/business-systems/bad?1.md",
            "corpus/pages/compliance/business-systems/bad*1.md",
            "corpus/pages/compliance/business-systems/bad\x071.md",
            "corpus/pages/compliance/business-systems/bad\x7f1.md",
            "corpus/pages/compliance/business-systems/bad\u{0085}1.md",
            "corpus/pages/compliance/business-systems/cafe\u{0301}.md",
        ];
        for bad_path in page_bad_segments {
            let mut tree = mini_tree();
            tree.insert_bytes(bad_path, b"ok\n");
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            let fb0123: Vec<_> = report
                .diagnostics
                .iter()
                .filter(|d| d.code == Code::Fb0123)
                .collect();
            assert_eq!(
                fb0123.len(),
                1,
                "path {bad_path:?}: {:?}",
                report.diagnostics
            );
            assert_eq!(fb0123[0].code, Code::Fb0123);
            assert_eq!(fb0123[0].path, bad_path);
        }

        // 65-char page stem.
        let stem65 = format!(
            "corpus/pages/compliance/business-systems/cmp-{}.md",
            "a".repeat(61)
        );
        let mut tree = mini_tree();
        tree.insert_bytes(&stem65, b"ok\n");
        let report = run(&Host::new(tree, sample_today()), &Options::default());
        let fb0123: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0123)
            .collect();
        assert_eq!(fb0123.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(fb0123[0].code, Code::Fb0123);
        assert_eq!(fb0123[0].path, stem65);

        // Original file rules: missing dot, stem > 64 chars, ext > 5 chars, uppercase ext.
        let orig_65 = format!("corpus/sources/public/S-0001-v1-{}.pdf", "a".repeat(56));
        for bad_orig in [
            "corpus/sources/public/S-0001-v1",
            orig_65.as_str(),
            "corpus/sources/public/S-0001-v1.toolong",
            "corpus/sources/public/S-0001-v1.PDF",
        ] {
            let mut tree = mini_tree();
            tree.insert(
                bad_orig,
                Entry::Original {
                    sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                        .to_owned(),
                    len: 1,
                },
            );
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            assert_eq!(
                report.diagnostics.len(),
                1,
                "orig {bad_orig}: {:?}",
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].code, Code::Fb0123);
            assert_eq!(report.diagnostics[0].path, bad_orig);
        }
    }

    #[test]
    fn fb0124_checks_changes_md_format() {
        let cases: &[(&str, usize)] = &[
            ("Corpus changes\n\nExplanation line.\n", 1),
            (
                "# Corpus changes\n\nExplanation line.\n\n## edition-001\n- Added: First.\n\n## edition-002\n- Added: Second.\n",
                8,
            ),
            (
                "# Corpus changes\n\nExplanation line.\n\n## edition-002\n- Updated: Wrong prefix.\n",
                6,
            ),
            (
                "# Corpus changes\n\nExplanation line.\n\n## edition-002\n- Added: Cut on 2026-03-01.\n",
                6,
            ),
        ];

        for &(content, expected_line) in cases {
            let mut tree = mini_tree();
            tree.insert_bytes("corpus/changes.md", content.as_bytes());
            let report = run(&Host::new(tree, sample_today()), &Options::default());
            assert_eq!(
                report.diagnostics.len(),
                1,
                "content {content:?}: {:?}",
                report.diagnostics
            );
            assert_eq!(report.diagnostics[0].code, Code::Fb0124);
            assert_eq!(report.diagnostics[0].path, "corpus/changes.md");
            assert_eq!(report.diagnostics[0].line, expected_line);
        }
    }

    #[test]
    fn fb0125_checks_rule_files() {
        // 1. Missing corpus/ASSISTANT.md.
        let mut tree = TimeTree::without_assistant();
        let report = run(&Host::new(tree.0, sample_today()), &Options::default());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0125);
        assert_eq!(report.diagnostics[0].path, "corpus/ASSISTANT.md");
        assert_eq!(report.diagnostics[0].line, 1);

        // 2. Missing rules_version = <n> on line 1.
        tree = TimeTree(mini_tree());
        tree.0
            .insert_bytes("corpus/ASSISTANT.md", b"# Assistant rules\n");
        let report = run(&Host::new(tree.0, sample_today()), &Options::default());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0125);
        assert_eq!(report.diagnostics[0].path, "corpus/ASSISTANT.md");
        assert_eq!(report.diagnostics[0].line, 1);

        // 3. corpus/ASSISTANT.md with 41 lines.
        let mut lines41 = String::from("rules_version = 1\n");
        for i in 2..=41 {
            lines41.push_str(&format!("line {i}\n"));
        }
        let mut tree41 = mini_tree();
        tree41.insert_bytes("corpus/ASSISTANT.md", lines41.as_bytes());
        let report = run(&Host::new(tree41, sample_today()), &Options::default());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0125);
        assert_eq!(report.diagnostics[0].path, "corpus/ASSISTANT.md");
        assert_eq!(report.diagnostics[0].line, 41);

        // 4. corpus/AGENTS.md with 41 lines.
        let mut agents41 = String::new();
        for i in 1..=41 {
            agents41.push_str(&format!("agent rule {i}\n"));
        }
        let mut tree_agents = mini_tree();
        tree_agents.insert_bytes("corpus/AGENTS.md", agents41.as_bytes());
        let report = run(&Host::new(tree_agents, sample_today()), &Options::default());
        assert_eq!(report.diagnostics.len(), 1, "{:?}", report.diagnostics);
        assert_eq!(report.diagnostics[0].code, Code::Fb0125);
        assert_eq!(report.diagnostics[0].path, "corpus/AGENTS.md");
        assert_eq!(report.diagnostics[0].line, 41);
    }

    struct TimeTree(Tree);
    impl TimeTree {
        fn without_assistant() -> Self {
            let mut tree = Tree::new();
            for (p, e) in mini_tree().iter() {
                if p != "corpus/ASSISTANT.md" {
                    tree.insert(p, e.clone());
                }
            }
            for d in mini_tree().dirs() {
                tree.insert_dir(d);
            }
            Self(tree)
        }
    }
}
