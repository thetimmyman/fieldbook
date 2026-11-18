//! Check pass `extraction`: `FB0212` and `FB0223`
//! (`docs/reference/corpus-format.md#extraction`).
//!
//! **Responsibility.**
//! - `FB0212`: extraction folder validation against `sources.csv`, presence of `extraction.toml`,
//!   validity of parsed `ExtractionToml`, text page file inventory, page hashes, original hash,
//!   page count, thin pages list, and locator anchor uniqueness and page bounds.
//! - `FB0223`: warnings on consequential claims whose cited source version provides weak evidence
//!   (metadata-only, thin pages, or supplied text).
//!
//! **Rules.** Pure: no I/O. Deterministic traversal in `VersionId` order.

use std::collections::BTreeMap;

use crate::extraction::{
    ExtractionToml, PageText, section_anchor, thin_page_numbers, weak_evidence,
};
use crate::ids::VersionId;
use crate::schema::Register;
use crate::schema::sources::SourceRow;

use super::registers::register_path;
use super::{Code, Diagnostic, Entry, Located, Model, Options, Report};

/// Run the `extraction` check pass (`FB0212`, `FB0223`) over `model`.
pub fn check(model: &Model, options: &Options, report: &mut Report) {
    if model.corpus().is_none() {
        return;
    }

    let source_map = build_source_map(model.sources());

    // A. Gather.
    let mut folders: BTreeMap<VersionId, Folder<'_>> = BTreeMap::new();
    for (path, entry) in model.tree().iter() {
        let Some(rest) = path.strip_prefix("corpus/sources/text/") else {
            continue;
        };
        let Some((folder_name, file_name)) = rest.split_once('/') else {
            continue;
        };
        if file_name.contains('/') || file_name.is_empty() {
            continue;
        }
        let Ok(vid) = VersionId::parse(folder_name) else {
            continue;
        };
        folders
            .entry(vid)
            .or_insert_with(|| Folder {
                files: BTreeMap::new(),
            })
            .files
            .insert(file_name.to_owned(), entry);
    }

    // B. FB0212 per folder.
    for (vid, folder) in &folders {
        check_folder(options, vid, folder, &source_map, report);
    }

    // C. FB0223 per consequential claim.
    check_claims_weak_evidence(model, options, &source_map, report);
}

struct Folder<'a> {
    files: BTreeMap<String, &'a Entry>,
}

fn check_folder(
    options: &Options,
    vid: &VersionId,
    folder: &Folder<'_>,
    source_map: &BTreeMap<VersionId, &Located<SourceRow>>,
    report: &mut Report,
) {
    // 1. No row for the version in sources.csv
    let Some(source_row) = source_map.get(vid) else {
        let path = if folder.files.contains_key("extraction.toml") {
            format!("corpus/sources/text/{vid}/extraction.toml")
        } else if let Some(first_file) = folder.files.keys().next() {
            format!("corpus/sources/text/{vid}/{first_file}")
        } else {
            format!("corpus/sources/text/{vid}/extraction.toml")
        };
        report.push(Diagnostic::new(
            Code::Fb0212,
            options.strict,
            path,
            1,
            1,
            "extraction folder has no row in `sources.csv`",
        ));
        return;
    };

    // 2. extraction.toml missing from the folder
    let toml_path = format!("corpus/sources/text/{vid}/extraction.toml");
    let Some(toml_entry) = folder.files.get("extraction.toml") else {
        report.push(Diagnostic::new(
            Code::Fb0212,
            options.strict,
            toml_path,
            1,
            1,
            "no `extraction.toml` beside the text files",
        ));
        return;
    };

    // 3. extraction.toml bytes not valid UTF-8, or ExtractionToml::parse returns Err(e)
    let Some(bytes) = toml_entry.as_bytes() else {
        return;
    };
    let Ok(toml_text) = std::str::from_utf8(bytes) else {
        report.push(Diagnostic::new(
            Code::Fb0212,
            options.strict,
            toml_path,
            1,
            1,
            "text file is not valid UTF-8",
        ));
        return;
    };
    let rec = match ExtractionToml::parse(toml_text) {
        Ok(rec) => rec,
        Err(e) => {
            report.push(Diagnostic::new(
                Code::Fb0212,
                options.strict,
                toml_path,
                e.line,
                e.col,
                e.message,
            ));
            return;
        }
    };

    // 4.a. Every key of rec.pages without a file <stem>.txt in the folder
    for stem in rec.pages.keys() {
        let file_name = format!("{stem}.txt");
        if !folder.files.contains_key(&file_name) {
            let line = usize::try_from(line_of(toml_text, stem)).unwrap_or(1);
            report.push(Diagnostic::new(
                Code::Fb0212,
                options.strict,
                &toml_path,
                line,
                1,
                format!("text file `{file_name}` is missing"),
            ));
        }
    }

    // 4.b. Every page file whose stem is not a key of rec.pages, and every other file
    // that is not extraction.toml and not a page file
    for name in folder.files.keys() {
        if name == "extraction.toml" {
            continue;
        }
        let is_listed_page = if is_page_file_name(name) {
            let stem = name.strip_suffix(".txt").unwrap_or("");
            rec.pages.contains_key(stem)
        } else {
            false
        };
        if !is_listed_page {
            let file_path = format!("corpus/sources/text/{vid}/{name}");
            report.push(Diagnostic::new(
                Code::Fb0212,
                options.strict,
                file_path,
                1,
                1,
                format!("extra file `{name}` is not listed in `extraction.toml`"),
            ));
        }
    }

    // 4.c. For each page file listed in rec.pages that has bytes: sha-256
    for (stem, expected_hash) in &rec.pages {
        let file_name = format!("{stem}.txt");
        if let Some(entry) = folder.files.get(&file_name)
            && let Some(page_bytes) = entry.as_bytes()
        {
            let actual_hash = crate::hash::sha256_hex(page_bytes);
            if &actual_hash != expected_hash {
                let file_path = format!("corpus/sources/text/{vid}/{file_name}");
                report.push(Diagnostic::new(
                    Code::Fb0212,
                    options.strict,
                    file_path,
                    1,
                    1,
                    format!("hash of `{file_name}` differs from `extraction.toml`"),
                ));
            }
        }
    }

    // 4.d. rec.original_sha256 must equal the sources row's sha256
    if rec.original_sha256 != source_row.value.sha256 {
        let line = usize::try_from(line_of(toml_text, "original_sha256")).unwrap_or(1);
        report.push(Diagnostic::new(
            Code::Fb0212,
            options.strict,
            &toml_path,
            line,
            1,
            "`original_sha256` differs from the `sha256` in `sources.csv`",
        ));
    }

    // 4.e. rec.page_count (u32) must equal the number of page files
    let page_files_count = folder
        .files
        .keys()
        .filter(|name| is_page_file_name(name))
        .count();
    let page_files_u32 = u32::try_from(page_files_count).unwrap_or(u32::MAX);
    if rec.page_count != page_files_u32 {
        let line = usize::try_from(line_of(toml_text, "page_count")).unwrap_or(1);
        report.push(Diagnostic::new(
            Code::Fb0212,
            options.strict,
            &toml_path,
            line,
            1,
            format!(
                "`page_count` is {} but the folder holds {} text files",
                rec.page_count, page_files_u32
            ),
        ));
    }

    // 4.f. Thin pages
    let mut pages = Vec::new();
    let mut all_had_bytes_and_utf8 = true;
    for (name, entry) in &folder.files {
        if !is_page_file_name(name) {
            continue;
        }
        let Some(page_bytes) = entry.as_bytes() else {
            all_had_bytes_and_utf8 = false;
            continue;
        };
        let Ok(page_text) = std::str::from_utf8(page_bytes) else {
            let file_path = format!("corpus/sources/text/{vid}/{name}");
            report.push(Diagnostic::new(
                Code::Fb0212,
                options.strict,
                file_path,
                1,
                1,
                "text file is not valid UTF-8",
            ));
            all_had_bytes_and_utf8 = false;
            continue;
        };
        let stem = name.strip_suffix(".txt").unwrap_or("");
        let digits = stem.strip_prefix('p').unwrap_or("");
        if let Ok(number) = digits.parse::<u32>() {
            pages.push(PageText {
                number,
                text: page_text.to_owned(),
            });
        }
    }

    if all_had_bytes_and_utf8 {
        pages.sort_by_key(|p| p.number);
        let actual_thin = thin_page_numbers(&pages);
        if actual_thin != rec.thin_pages {
            let line = usize::try_from(line_of(toml_text, "thin_pages")).unwrap_or(1);
            report.push(Diagnostic::new(
                Code::Fb0212,
                options.strict,
                &toml_path,
                line,
                1,
                format!(
                    "`thin_pages` is [{}] but the text files give [{}]",
                    format_u32_list(&rec.thin_pages),
                    format_u32_list(&actual_thin)
                ),
            ));
        }
    }

    // 4.g. Anchors and locator page bounds
    let mut seen_anchors: BTreeMap<String, &str> = BTreeMap::new();
    for label in rec.locators.keys() {
        let anchor = section_anchor(label);
        if let Some(&first_label) = seen_anchors.get(&anchor) {
            let line = usize::try_from(line_of(toml_text, label)).unwrap_or(1);
            report.push(Diagnostic::new(
                Code::Fb0212,
                options.strict,
                &toml_path,
                line,
                1,
                format!("labels `{first_label}` and `{label}` share the anchor `{anchor}`"),
            ));
        } else {
            seen_anchors.insert(anchor, label);
        }
    }

    for (label, loc) in &rec.locators {
        if loc.last > rec.page_count || loc.first == 0 {
            let line = usize::try_from(line_of(toml_text, label)).unwrap_or(1);
            report.push(Diagnostic::new(
                Code::Fb0212,
                options.strict,
                &toml_path,
                line,
                1,
                format!(
                    "locator `{label}` names a page outside 1 to {}",
                    rec.page_count
                ),
            ));
        }
    }
}

fn check_claims_weak_evidence(
    model: &Model,
    options: &Options,
    source_map: &BTreeMap<VersionId, &Located<SourceRow>>,
    report: &mut Report,
) {
    let mut parsed_records: BTreeMap<VersionId, Option<ExtractionToml>> = BTreeMap::new();

    for claim_loc in model.claims() {
        let claim = &claim_loc.value;
        if !claim.consequential {
            continue;
        }
        let (Some(source_id), Some(version)) = (&claim.source_id, claim.version) else {
            continue;
        };
        let Ok(vid) = VersionId::parse(&format!("{source_id}-v{version}")) else {
            continue;
        };
        let Some(row) = source_map.get(&vid) else {
            continue;
        };
        let record = parsed_records.entry(vid.clone()).or_insert_with(|| {
            let toml_path = format!("corpus/sources/text/{vid}/extraction.toml");
            let bytes = model.tree().get_bytes(&toml_path)?;
            let text = std::str::from_utf8(bytes).ok()?;
            ExtractionToml::parse(text).ok()
        });
        if let Some(w) = weak_evidence(&row.value.location, record.as_ref()) {
            let line = usize::try_from(claim_loc.line).unwrap_or(1);
            let col = col_1based(model, Register::Claims, claim_loc.line, "version");
            report.push(Diagnostic::new(
                Code::Fb0223,
                options.strict,
                register_path(Register::Claims),
                line,
                col,
                format!(
                    "consequential claim `{}` rests on `{vid}`: {w}",
                    claim.claim_id
                ),
            ));
        }
    }
}

fn line_of(text: &str, needle_prefix: &str) -> u32 {
    let quoted = format!("\"{needle_prefix}\"");
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(&quoted) {
            let rest_trimmed = rest.trim_start();
            if rest_trimmed.starts_with('=') || rest.is_empty() {
                return u32::try_from(idx.saturating_add(1)).unwrap_or(1);
            }
        }
        if let Some(rest) = trimmed.strip_prefix(needle_prefix) {
            let rest_trimmed = rest.trim_start();
            if rest_trimmed.starts_with('=') || rest.is_empty() {
                return u32::try_from(idx.saturating_add(1)).unwrap_or(1);
            }
        }
    }
    1
}

fn col_1based(model: &Model, reg: Register, line: u32, col_name: &str) -> usize {
    let idx = reg
        .columns()
        .iter()
        .position(|c| c.name == col_name)
        .unwrap_or(0);
    model
        .register_rows(reg)
        .iter()
        .find(|r| r.line == line)
        .and_then(|r| r.cols.get(idx).copied())
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(1)
}

fn build_source_map(sources: &[Located<SourceRow>]) -> BTreeMap<VersionId, &Located<SourceRow>> {
    let mut map = BTreeMap::new();
    for located in sources {
        if let Ok(vid) = VersionId::parse(&format!(
            "{}-v{}",
            located.value.source_id, located.value.version
        )) {
            map.entry(vid).or_insert(located);
        }
    }
    map
}

fn format_u32_list(list: &[u32]) -> String {
    list.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn is_page_stem(stem: &str) -> bool {
    let Some(digits) = stem.strip_prefix('p') else {
        return false;
    };
    digits.len() >= 4 && digits.chars().all(|c| c.is_ascii_digit())
}

fn is_page_file_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".txt") else {
        return false;
    };
    is_page_stem(stem)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::check::{Code, Host, Options, Severity, Tree, run};
    use crate::date::CivilDate;
    use crate::extraction::{
        ExtractionMethod, ExtractionToml, Extractor, LocatorPages, Plain, build_record,
    };
    use crate::hash::sha256_hex;

    use super::line_of;

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
    const MINI_S0001_V1_TXT: &[u8] = include_bytes!(
        "../../../../tests/fixtures/corpus-mini/corpus/sources/public/S-0001-v1.txt"
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
        tree.insert_bytes("corpus/sources/public/S-0001-v1.txt", MINI_S0001_V1_TXT);
        tree
    }

    fn mini_tree_with_extraction() -> (Tree, ExtractionToml, String) {
        let mut tree = mini_tree_with_originals();
        tree.insert_dir("corpus/sources/text");
        tree.insert_dir("corpus/sources/text/S-0001-v1");
        let extraction = Plain
            .extract(MINI_S0001_V1_TXT, "txt")
            .expect("plain extraction");
        let sha = sha256_hex(MINI_S0001_V1_TXT);
        let record = build_record(&extraction, "plain", "1", &sha, &BTreeMap::new());
        let rendered = record.render();
        let page_text = extraction.pages[0].text.clone();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/p0001.txt",
            page_text.as_bytes(),
        );
        (tree, record, page_text)
    }

    #[test]
    fn a_matching_extraction_is_clean() {
        let (tree, _, _) = mini_tree_with_extraction();
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        assert!(
            report.diagnostics.is_empty(),
            "expected clean report, got: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn a_missing_text_file_is_fb0212() {
        let mut tree = mini_tree_with_originals();
        tree.insert_dir("corpus/sources/text");
        tree.insert_dir("corpus/sources/text/S-0001-v1");
        let extraction = Plain
            .extract(MINI_S0001_V1_TXT, "txt")
            .expect("plain extraction");
        let sha = sha256_hex(MINI_S0001_V1_TXT);
        let record = build_record(&extraction, "plain", "1", &sha, &BTreeMap::new());
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            record.render().as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(
            |d| d.path == "corpus/sources/text/S-0001-v1/extraction.toml"
                && d.message.contains("text file `p0001.txt` is missing")
        ));
    }

    #[test]
    fn an_extra_text_file_is_fb0212() {
        let (mut tree, _, _) = mini_tree_with_extraction();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/p0002.txt",
            b"extra page text\n",
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(|d| {
            d.path == "corpus/sources/text/S-0001-v1/p0002.txt"
                && d.line == 1
                && d.col == 1
                && d.message
                    .contains("extra file `p0002.txt` is not listed in `extraction.toml`")
        }));
    }

    #[test]
    fn a_changed_byte_is_fb0212() {
        let (mut tree, _, page_text) = mini_tree_with_extraction();
        let mut corrupted = page_text.into_bytes();
        corrupted.push(b'X');
        tree.insert_bytes("corpus/sources/text/S-0001-v1/p0001.txt", corrupted);
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(|d| {
            d.path == "corpus/sources/text/S-0001-v1/p0001.txt"
                && d.message
                    .contains("hash of `p0001.txt` differs from `extraction.toml`")
        }));
    }

    #[test]
    fn a_different_original_hash_is_fb0212() {
        let (mut tree, mut record, _) = mini_tree_with_extraction();
        record.original_sha256 = "a".repeat(64);
        let rendered = record.render();
        let line = line_of(&rendered, "original_sha256");
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(|d| {
            d.path == "corpus/sources/text/S-0001-v1/extraction.toml"
                && d.line == line as usize
                && d.message
                    .contains("`original_sha256` differs from the `sha256` in `sources.csv`")
        }));
    }

    #[test]
    fn a_wrong_page_count_is_fb0212() {
        let (mut tree, mut record, _) = mini_tree_with_extraction();
        record.page_count = 2;
        let rendered = record.render();
        let line = line_of(&rendered, "page_count");
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(|d| {
            d.path == "corpus/sources/text/S-0001-v1/extraction.toml"
                && d.line == line as usize
                && d.message
                    .contains("`page_count` is 2 but the folder holds 1 text files")
        }));
    }

    #[test]
    fn a_stale_thin_pages_is_fb0212() {
        let (mut tree, _, _) = mini_tree_with_extraction();
        let extraction = Plain
            .extract(MINI_S0001_V1_TXT, "txt")
            .expect("plain extraction");
        let sha = sha256_hex(MINI_S0001_V1_TXT);
        let record = build_record(&extraction, "plain", "1", &sha, &BTreeMap::new());
        let rendered = record
            .render()
            .replace("thin_pages = []", "thin_pages = [1]");
        let line = line_of(&rendered, "thin_pages");
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let has_fb0212 = report.diagnostics.iter().any(|d| {
            d.code == Code::Fb0212
                && d.path == "corpus/sources/text/S-0001-v1/extraction.toml"
                && d.line == line as usize
                && d.message
                    .contains("`thin_pages` is [1] but the text files give []")
        });
        assert!(has_fb0212, "expected FB0212 for stale thin pages");

        let fb0223: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0223)
            .collect();
        assert!(
            fb0223.iter().any(|d| d.message.contains("CL-CMP-0001")),
            "expected FB0223 for CL-CMP-0001"
        );
        assert!(
            fb0223.iter().any(|d| d.message.contains("CL-CMP-0002")),
            "expected FB0223 for CL-CMP-0002"
        );
    }

    #[test]
    fn two_labels_with_one_anchor_are_fb0212() {
        let (mut tree, mut record, _) = mini_tree_with_extraction();
        record
            .locators
            .insert("(e)(1)".to_string(), LocatorPages { first: 1, last: 1 });
        record
            .locators
            .insert("e 1".to_string(), LocatorPages { first: 1, last: 1 });
        let rendered = record.render();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(|d| {
            d.path == "corpus/sources/text/S-0001-v1/extraction.toml"
                && d.message
                    .contains("labels `(e)(1)` and `e 1` share the anchor `sec-e-1`")
        }));
    }

    #[test]
    fn a_locator_outside_the_pages_is_fb0212() {
        let (mut tree, mut record, _) = mini_tree_with_extraction();
        record
            .locators
            .insert("(e)(1)".to_string(), LocatorPages { first: 1, last: 2 });
        let rendered = record.render();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(|d| {
            d.path == "corpus/sources/text/S-0001-v1/extraction.toml"
                && d.message
                    .contains("locator `(e)(1)` names a page outside 1 to 1")
        }));
    }

    #[test]
    fn an_unparsable_extraction_is_fb0212_with_a_position() {
        let (mut tree, _, _) = mini_tree_with_extraction();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            b"tool = [[\n",
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(
            |d| d.path == "corpus/sources/text/S-0001-v1/extraction.toml"
                && d.line >= 1
                && d.col >= 1
        ));
    }

    #[test]
    fn a_folder_with_no_row_is_fb0212() {
        let (mut tree, _, _) = mini_tree_with_extraction();
        tree.insert_dir("corpus/sources/text/S-0099-v1");
        tree.insert_bytes(
            "corpus/sources/text/S-0099-v1/extraction.toml",
            b"tool = \"plain\"\n",
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.iter().any(|d| {
            d.path == "corpus/sources/text/S-0099-v1/extraction.toml"
                && d.line == 1
                && d.col == 1
                && d.message
                    .contains("extraction folder has no row in `sources.csv`")
        }));
    }

    #[test]
    fn a_thin_page_warns_on_a_consequential_claim() {
        let (mut tree, mut record, _) = mini_tree_with_extraction();
        let short_text = "short\n";
        record
            .pages
            .insert("p0001".to_string(), sha256_hex(short_text.as_bytes()));
        record.thin_pages = vec![1];
        let rendered = record.render();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/p0001.txt",
            short_text.as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.is_empty(), "unexpected FB0212: {:?}", fb0212);

        let fb0223: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0223)
            .collect();
        assert_eq!(fb0223.len(), 2, "expected 2 FB0223, got: {:?}", fb0223);
        assert!(fb0223.iter().any(|d| d.message.contains("CL-CMP-0001")));
        assert!(fb0223.iter().any(|d| d.message.contains("CL-CMP-0002")));
        for d in &fb0223 {
            assert_eq!(d.severity, Severity::Warning);
        }
    }

    #[test]
    fn a_supplied_extraction_warns() {
        let (mut tree, mut record, _) = mini_tree_with_extraction();
        record.method = ExtractionMethod::parse("supplied");
        record.tool = "supplied".to_string();
        record.tool_version = "1".to_string();
        let rendered = record.render();
        tree.insert_bytes(
            "corpus/sources/text/S-0001-v1/extraction.toml",
            rendered.as_bytes(),
        );
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        assert!(fb0212.is_empty(), "unexpected FB0212: {:?}", fb0212);

        let fb0223: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0223)
            .collect();
        assert_eq!(fb0223.len(), 2, "expected 2 FB0223, got: {:?}", fb0223);
        assert!(fb0223.iter().any(|d| d.message.contains("CL-CMP-0001")));
        assert!(fb0223.iter().any(|d| d.message.contains("CL-CMP-0002")));
        for d in &fb0223 {
            assert_eq!(d.severity, Severity::Warning);
            assert!(
                d.message.contains(
                    "the text was supplied by hand and is not checked against the original"
                )
            );
        }
    }

    #[test]
    fn metadata_only_without_extraction_warns() {
        let mut tree = mini_tree_with_originals();
        let modified_claims = MINI_CLAIMS_CSV.replace(
            "S-0002,2,text_section,sec. 3",
            "S-0002,1,text_section,sec. 3",
        );
        assert_ne!(modified_claims, MINI_CLAIMS_CSV);
        tree.insert_bytes("corpus/registers/claims.csv", modified_claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0223: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0223)
            .collect();
        assert_eq!(fb0223.len(), 1, "expected 1 FB0223, got: {:?}", fb0223);
        assert_eq!(fb0223[0].line, 5);
        assert!(fb0223[0].col > 1);
        assert!(fb0223[0].message.contains("CL-EST-0002"));
        assert!(fb0223[0].message.contains("the version is metadata-only"));
    }

    #[test]
    fn ordinary_version_without_extraction_does_not_warn() {
        let tree = mini_tree_with_originals();
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0212: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0212)
            .collect();
        let fb0223: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0223)
            .collect();
        assert!(fb0212.is_empty(), "expected no FB0212: {:?}", fb0212);
        assert!(fb0223.is_empty(), "expected no FB0223: {:?}", fb0223);
    }

    #[test]
    fn a_non_consequential_claim_never_warns() {
        let mut tree = mini_tree_with_originals();
        let modified_claims = MINI_CLAIMS_CSV.replace(
            "S-0002,2,text_section,sec. 3,Synthetic rate review window was 15 days.,true,codex,",
            "S-0002,1,text_section,sec. 3,Synthetic rate review window was 15 days.,false,codex,",
        );
        assert_ne!(modified_claims, MINI_CLAIMS_CSV);
        tree.insert_bytes("corpus/registers/claims.csv", modified_claims.as_bytes());
        let report = run(&Host::new(tree, sample_today()), &Options::new(true));
        let fb0223: Vec<_> = report
            .diagnostics
            .iter()
            .filter(|d| d.code == Code::Fb0223)
            .collect();
        assert!(fb0223.is_empty(), "expected no FB0223: {:?}", fb0223);
    }
}
