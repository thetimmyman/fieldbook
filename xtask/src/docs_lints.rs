//! Documentation lints that read the repository as a whole (`docs-checks`).

use std::collections::BTreeSet;
use std::path::Path;

use pulldown_cmark::{Event, Parser, Tag};

use crate::cards::{Finding, Snapshot, is_merged, plan_items};
use crate::links::checked_markdown;

/// Retired terminology and model patterns that must not appear in modern documentation.
pub const RETIRED: &[&str] = &[
    "handling zone",
    "DataClass",
    "PublicView",
    "approved_for",
    "[model.route]",
    "handling = \"org\"",
    "data class",
];

const DEFAULT_CANONICAL_BLOCKS: &[&str] = &[
    "The silent build (the default, and the only build in an edition) opens no socket, starts no process, contains no model and makes no outbound connection. The AI build (feature net) adds outbound HTTPS to endpoints that the company's policy lists and the settings name, only on an explicit command and after the audit record is written. With the further feature bridge it also holds, only while fieldbook chat runs, one listener on 127.0.0.1 behind a per-session token. fieldbook mcp uses standard input and output and opens no socket in any build. Neither build starts a process; only the dev-only xtask crate does.",
    "The silent build opens no socket, starts no process, contains no model and makes no outbound connection. The AI build adds an HTTPS client for configured endpoints and, built with bridge, one loopback listener while fieldbook chat runs (IT-27, IT-32).",
    "Tier 0 and every edition open no socket and make no request.",
    "In the silent build and the AI build alike, backup and restore start no process, install no scheduled task and make no network connection.",
];

fn clean_markdown_sentence(raw: &str) -> String {
    let mut cleaned = String::new();
    let mut in_leading_gt = true;
    for c in raw.chars() {
        if in_leading_gt && (c == '>' || c == ' ' || c == '\t') {
            continue;
        }
        in_leading_gt = false;
        if c != '*' && c != '`' {
            cleaned.push(c);
        }
    }
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split_sentences(text: &str) -> Vec<(usize, String)> {
    let mut sentences = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut line_idx = 0;

    while line_idx < lines.len() {
        let line = lines[line_idx];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            line_idx += 1;
            continue;
        }

        let content = trimmed.strip_prefix('>').unwrap_or(trimmed).trim();
        let is_heading = content.starts_with('#');
        let is_table = content.starts_with('|');
        let is_list = content.starts_with("- ")
            || content.starts_with("* ")
            || content.starts_with("+ ")
            || content
                .split_once(". ")
                .is_some_and(|(num, _)| !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()));

        let mut para_chars: Vec<(usize, char)> = Vec::new();

        if is_heading || is_table || is_list {
            let line_num = line_idx + 1;
            for c in line.chars() {
                para_chars.push((line_num, c));
            }
            line_idx += 1;
        } else {
            while line_idx < lines.len() {
                let curr_line = lines[line_idx];
                let curr_trimmed = curr_line.trim();
                if curr_trimmed.is_empty() {
                    break;
                }
                let curr_content = curr_trimmed
                    .strip_prefix('>')
                    .unwrap_or(curr_trimmed)
                    .trim();
                if curr_content.starts_with('#')
                    || curr_content.starts_with('|')
                    || curr_content.starts_with("- ")
                    || curr_content.starts_with("* ")
                    || curr_content.starts_with("+ ")
                    || curr_content.split_once(". ").is_some_and(|(num, _)| {
                        !num.is_empty() && num.chars().all(|c| c.is_ascii_digit())
                    })
                {
                    break;
                }
                let line_num = line_idx + 1;
                if !para_chars.is_empty() {
                    para_chars.push((line_num, ' '));
                }
                for c in curr_line.chars() {
                    para_chars.push((line_num, c));
                }
                line_idx += 1;
            }
        }

        let mut in_code = false;
        let mut sent_start_idx = 0;
        let mut char_idx = 0;

        while char_idx < para_chars.len() {
            let (_, c) = para_chars[char_idx];
            if c == '`' {
                in_code = !in_code;
            } else if !in_code && (c == '.' || c == '?' || c == '!') {
                let mut is_end = true;
                if c == '.' {
                    let mut w_start = char_idx;
                    while w_start > sent_start_idx && !para_chars[w_start - 1].1.is_whitespace() {
                        w_start -= 1;
                    }
                    let word: String = para_chars[w_start..char_idx]
                        .iter()
                        .map(|(_, ch)| *ch)
                        .collect();
                    let trimmed_word = word.trim_matches(|ch: char| !ch.is_alphanumeric());
                    let is_digit_word = !trimmed_word.is_empty()
                        && trimmed_word.chars().all(|ch| ch.is_ascii_digit());
                    let is_abbrev = trimmed_word.eq_ignore_ascii_case("e.g")
                        || trimmed_word.eq_ignore_ascii_case("i.e")
                        || word.eq_ignore_ascii_case("e.g")
                        || word.eq_ignore_ascii_case("i.e");
                    if is_digit_word || is_abbrev {
                        is_end = false;
                    }
                }

                if is_end {
                    let mut after = char_idx + 1;
                    while after < para_chars.len()
                        && matches!(para_chars[after].1, ')' | ']' | '"' | '\'')
                    {
                        after += 1;
                    }
                    if after == para_chars.len() || para_chars[after].1.is_whitespace() {
                        let sent_chars = &para_chars[sent_start_idx..after];
                        let non_ws_pos = sent_chars.iter().position(|(_, ch)| !ch.is_whitespace());
                        if let Some(pos) = non_ws_pos {
                            let start_line = sent_chars[pos].0;
                            let raw_sent: String = sent_chars.iter().map(|(_, ch)| *ch).collect();
                            let cleaned = clean_markdown_sentence(&raw_sent);
                            if !cleaned.is_empty() {
                                sentences.push((start_line, cleaned));
                            }
                        }
                        sent_start_idx = after;
                        char_idx = after;
                        continue;
                    }
                }
            }
            char_idx += 1;
        }

        if sent_start_idx < para_chars.len() {
            let sent_chars = &para_chars[sent_start_idx..];
            let non_ws_pos = sent_chars.iter().position(|(_, ch)| !ch.is_whitespace());
            if let Some(pos) = non_ws_pos {
                let start_line = sent_chars[pos].0;
                let raw_sent: String = sent_chars.iter().map(|(_, ch)| *ch).collect();
                let cleaned = clean_markdown_sentence(&raw_sent);
                if !cleaned.is_empty() {
                    sentences.push((start_line, cleaned));
                }
            }
        }
    }

    sentences
}

fn extract_canonical_posture_sentences(snap: &Snapshot) -> Vec<String> {
    let mut canonical = Vec::new();
    if let Some(readme) = snap.text.get("docs/cards/README.md") {
        let mut in_block = false;
        let mut current_block = String::new();

        for line in readme.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("### Changing the network-posture sentence") {
                in_block = true;
                continue;
            }
            if in_block && trimmed.starts_with("### ") {
                break;
            }
            if in_block && trimmed.starts_with("> **") {
                if !current_block.is_empty() {
                    for (_, sent) in split_sentences(&current_block) {
                        canonical.push(sent);
                    }
                    current_block.clear();
                }
                let stripped = if let Some(after) = trimmed.strip_prefix("> **") {
                    if let Some((_, rest)) = after.split_once(".**") {
                        rest.trim()
                    } else if let Some((_, rest)) = after.split_once("**") {
                        rest.trim()
                    } else {
                        after
                    }
                } else {
                    trimmed
                };
                current_block.push_str(stripped);
            } else if in_block && trimmed.starts_with('>') && !current_block.is_empty() {
                let stripped = trimmed.trim_start_matches('>').trim();
                if !stripped.is_empty() {
                    current_block.push(' ');
                    current_block.push_str(stripped);
                }
            } else if in_block && !trimmed.starts_with('>') && !current_block.is_empty() {
                for (_, sent) in split_sentences(&current_block) {
                    canonical.push(sent);
                }
                current_block.clear();
            }
        }
        if !current_block.is_empty() {
            for (_, sent) in split_sentences(&current_block) {
                canonical.push(sent);
            }
        }
    }

    if canonical.is_empty() {
        for block in DEFAULT_CANONICAL_BLOCKS {
            for (_, sent) in split_sentences(block) {
                canonical.push(sent);
            }
        }
    }

    canonical
}

/// A sentence that holds the whole canonical form and then continues after a comma or semicolon quotes it unedited.
fn continues_canonical(sent: &str, canonical: &str) -> bool {
    let base = canonical.trim_end_matches('.');
    sent.strip_prefix(base)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([',', ';', '.']))
}

/// Enforces the canonical four-part network-posture claim and detects unauthorized drift.
pub fn posture_wording_names_the_build(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let canonical = extract_canonical_posture_sentences(snap);

    let canonical_eight: Vec<Vec<String>> = canonical
        .iter()
        .map(|s| {
            s.split_whitespace()
                .take(8)
                .map(ToOwned::to_owned)
                .collect()
        })
        .collect();

    for (path, text) in checked_markdown(snap) {
        if path.starts_with("docs/design/") || path == "CHANGELOG.md" {
            continue;
        }

        for (line, sent) in split_sentences(text) {
            let sent_words: Vec<String> = sent.split_whitespace().map(ToOwned::to_owned).collect();
            let mut matched_canon_idx = None;

            for (idx, c_eight) in canonical_eight.iter().enumerate() {
                if c_eight.len() == 8 && sent_words.len() >= 8 && sent_words[..8] == c_eight[..8] {
                    matched_canon_idx = Some(idx);
                    break;
                }
            }

            let lower = sent.to_lowercase();
            let has_posture_word = [
                "no network",
                "no socket",
                "no listener",
                "no model",
                "no process",
            ]
            .iter()
            .any(|word| lower.contains(word));
            if let Some(idx) = matched_canon_idx
                && has_posture_word
                && sent != canonical[idx]
                && !continues_canonical(&sent, &canonical[idx])
            {
                findings.push(Finding::new(
                    path,
                    line,
                    "network-posture form differs from the canonical sentence",
                ));
                continue;
            }

            let has_network = lower.contains("no network");
            let has_socket = lower.contains("no socket");
            let has_listener = lower.contains("no listener");
            let has_model = lower.contains("no model");

            if has_network && has_socket && has_listener && has_model {
                let names_build = lower.contains("silent build")
                    || lower.contains("edition")
                    || lower.contains("ai build");
                let equals_canon = canonical.iter().any(|c| c.eq_ignore_ascii_case(&sent));
                let holds_label = sent.contains("No model here")
                    || sent.contains("No model connected here")
                    || lower.contains("no model here")
                    || lower.contains("no model connected here");

                if !names_build && !equals_canon && !holds_label {
                    findings.push(Finding::new(
                        path,
                        line,
                        "network-posture sentence names no build (silent build, edition or AI build)",
                    ));
                }
            }
        }
    }

    findings
}

fn is_valid_later_slug(slug: &str) -> bool {
    if !slug.starts_with("later-") || slug.ends_with('-') {
        return false;
    }
    slug.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Enforces that all `later-*` items referenced across documentation are indexed in AD-24.
pub fn later_slug(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(adr_text) = snap.text.get("docs/decisions/0001-foundation.md") else {
        return findings;
    };

    let mut valid_slugs = BTreeSet::new();
    let mut seen_slugs = BTreeSet::new();
    let mut in_ad24 = false;

    for (line_idx, line) in adr_text.lines().enumerate() {
        let line_no = line_idx + 1;
        let trimmed = line.trim();
        if trimmed.starts_with("#### AD-24") {
            in_ad24 = true;
            continue;
        }
        if in_ad24
            && (trimmed.starts_with("# ")
                || trimmed.starts_with("## ")
                || trimmed.starts_with("### ")
                || trimmed.starts_with("#### "))
        {
            break;
        }
        if in_ad24 && trimmed.starts_with('|') {
            let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
            if cells.len() >= 3 {
                let cell = cells[1];
                if cell == "Slug" || cell.starts_with("---") {
                    continue;
                }
                if cell.starts_with("later-") || cell.contains("later") {
                    if !is_valid_later_slug(cell) {
                        findings.push(Finding::new(
                            "docs/decisions/0001-foundation.md",
                            line_no,
                            format!("invalid AD-24 row slug `{cell}`"),
                        ));
                    } else if !seen_slugs.insert(cell.to_owned()) {
                        findings.push(Finding::new(
                            "docs/decisions/0001-foundation.md",
                            line_no,
                            format!("duplicate AD-24 row slug `{cell}`"),
                        ));
                    } else {
                        valid_slugs.insert(cell.to_owned());
                    }
                }
            }
        }
    }

    for (path, text) in checked_markdown(snap) {
        for (line_idx, line) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let bytes = line.as_bytes();
            let mut idx = 0;

            while idx < line.len() {
                if let Some(pos) = line[idx..].find("later-") {
                    let match_pos = idx + pos;
                    let prev_ok = if match_pos == 0 {
                        true
                    } else {
                        let prev = bytes[match_pos - 1];
                        !prev.is_ascii_alphanumeric() && prev != b'-' && prev != b'#'
                    };

                    let in_link_target = {
                        let before = &line[..match_pos];
                        if let Some(last_open) = before.rfind("](") {
                            !before[last_open..].contains(')')
                        } else {
                            false
                        }
                    };

                    if !prev_ok || in_link_target {
                        idx = match_pos + 6;
                        continue;
                    }

                    let mut end = match_pos + 6;
                    while end < line.len()
                        && (bytes[end].is_ascii_lowercase()
                            || bytes[end].is_ascii_digit()
                            || bytes[end] == b'-')
                    {
                        end += 1;
                    }

                    if end == match_pos + 6 {
                        idx = match_pos + 6;
                        continue;
                    }

                    let raw_slug = &line[match_pos..end];
                    let slug = raw_slug.trim_end_matches('-');

                    if slug.len() <= 6 {
                        idx = end;
                        continue;
                    }

                    if path == "docs/decisions/0001-foundation.md"
                        && line.trim_start().starts_with('|')
                    {
                        idx = end;
                        continue;
                    }

                    if !valid_slugs.contains(slug) {
                        findings.push(Finding::new(
                            path,
                            line_no,
                            format!("later slug `{slug}` is not a row of AD-24"),
                        ));
                    }

                    idx = end;
                } else {
                    break;
                }
            }
        }
    }

    findings
}

/// Checks that all generated markdown files are synchronized with their generator outputs.
pub fn generated_in_sync(root: &Path) -> Vec<Finding> {
    match crate::docs::run_with(root, crate::docs::GENERATORS, true) {
        Ok(()) => Vec::new(),
        Err((_code, msg)) => {
            let file = if let Some(rest) = msg.strip_prefix("docs: ") {
                if let Some((f, _)) = rest.split_once(" (") {
                    f.to_owned()
                } else {
                    rest.to_owned()
                }
            } else {
                "docs".to_owned()
            };
            let line = if let Some((_, after)) = msg.split_once("differs at line ") {
                if let Some((num_str, _)) = after.split_once(':') {
                    num_str.trim().parse::<usize>().unwrap_or(1)
                } else {
                    1
                }
            } else {
                1
            };
            vec![Finding::new(file, line, msg)]
        }
    }
}

fn anchor_base(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// Verifies that all `Adding ...` recipe sections outside the cards README are indexed in its recipes section.
pub fn adding_recipes_are_indexed(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(readme) = snap.text.get("docs/cards/README.md") else {
        return findings;
    };

    let mut in_recipes = false;
    let mut recipes_text = String::new();

    for line in readme.lines() {
        let trimmed = line.trim();
        if trimmed == "## Recipes for recurring changes" {
            in_recipes = true;
            continue;
        }
        if in_recipes && trimmed.starts_with("## ") {
            break;
        }
        if in_recipes {
            recipes_text.push_str(line);
            recipes_text.push('\n');
        }
    }

    let mut indexed_tokens = Vec::new();
    for (event, _) in Parser::new(&recipes_text).into_offset_iter() {
        match event {
            Event::Code(c) => indexed_tokens.push(c.to_string()),
            Event::Start(Tag::Link { dest_url, .. }) => indexed_tokens.push(dest_url.to_string()),
            Event::Text(t) => indexed_tokens.push(t.to_string()),
            _ => {}
        }
    }

    for (path, text) in &snap.text {
        if !path.ends_with(".md") {
            continue;
        }
        if path == "docs/cards/README.md" || path.starts_with("docs/design/") {
            continue;
        }
        let in_scope =
            path.starts_with("docs/") || path.starts_with("tests/") || path.starts_with("crates/");
        if !in_scope {
            continue;
        }

        for (line_idx, line) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let trimmed = line.trim();

            let (heading_lead, heading_title) = if trimmed.starts_with("## Adding ")
                || trimmed.starts_with("### Adding ")
                || trimmed.starts_with("#### Adding ")
            {
                let title = trimmed.trim_start_matches('#').trim();
                (trimmed, title)
            } else if trimmed.starts_with("**Adding ") {
                if let Some(end) = trimmed[2..].find("**") {
                    let bold_inner = &trimmed[2..2 + end];
                    let title = bold_inner.trim_end_matches('.');
                    let lead = &trimmed[..4 + end];
                    (lead, title)
                } else {
                    continue;
                }
            } else {
                continue;
            };

            let anchor = anchor_base(heading_title);
            let expected_ref = format!("{path}#{anchor}");

            let found = indexed_tokens.iter().any(|tok| tok.contains(&expected_ref));
            if !found {
                findings.push(Finding::new(
                    path.as_str(),
                    line_no,
                    format!("recipe heading `{heading_lead}` is not indexed in the Recipes section of docs/cards/README.md"),
                ));
            }
        }
    }

    findings
}

fn has_word(text: &str, word: &str) -> bool {
    let mut start = 0;
    while let Some(idx) = text[start..].find(word) {
        let pos = start + idx;
        let before_ok = if pos == 0 {
            true
        } else {
            let prev = text[..pos].chars().next_back().unwrap_or(' ');
            !prev.is_alphanumeric() && prev != '-'
        };
        let after_pos = pos + word.len();
        let after_ok = if after_pos >= text.len() {
            true
        } else {
            let next = text[after_pos..].chars().next().unwrap_or(' ');
            !next.is_alphanumeric() && next != '-'
        };
        if before_ok && after_ok {
            return true;
        }
        start = pos + word.len().max(1);
    }
    false
}

/// Enforces that all active controls in docs/controls.md have corresponding test coverage or known-bad entries.
pub fn controls_have_tests(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(controls_text) = snap.text.get("docs/controls.md") else {
        return findings;
    };

    let plan_slugs: BTreeSet<String> = match plan_items(snap) {
        Ok(items) => items.into_iter().map(|it| it.slug).collect(),
        Err(_) => BTreeSet::new(),
    };

    let mut known_bad_controls = BTreeSet::new();
    for (path, text) in &snap.text {
        if path == "tests/known-bad/cases.toml"
            || (path.starts_with("tests/known-bad/") && path.ends_with(".toml"))
        {
            for line in text.lines() {
                if let Some(rest) = line.split_once("control")
                    && let Some((_, val_part)) = rest.1.split_once('=')
                {
                    let trimmed = val_part.trim().trim_matches('"');
                    if !trimmed.is_empty() {
                        known_bad_controls.insert(trimmed.to_owned());
                    }
                }
            }
        }
    }

    let mut marker_controls = BTreeSet::new();
    let control_marker_prefix = concat!("// con", "trol:");

    for (path, text) in &snap.text {
        if !path.ends_with(".rs") {
            continue;
        }
        if !path.starts_with("crates/") && !path.starts_with("xtask/") {
            continue;
        }

        let lines: Vec<&str> = text.lines().collect();
        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix(control_marker_prefix) {
                let ids: Vec<&str> = rest
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut fn_within_three = false;
                for offset in 1..=3 {
                    if idx + offset < lines.len() {
                        let next_line = lines[idx + offset].trim();
                        if next_line.starts_with("fn ")
                            || next_line.starts_with("pub fn ")
                            || next_line.starts_with("async fn ")
                            || next_line.starts_with("pub async fn ")
                            || next_line.starts_with("pub(crate) fn ")
                            || next_line.contains(" fn ")
                        {
                            fn_within_three = true;
                            break;
                        }
                    }
                }
                if fn_within_three {
                    for id in ids {
                        marker_controls.insert(id.to_owned());
                    }
                }
            }
        }
    }

    for (line_idx, line) in controls_text.lines().enumerate() {
        let line_no = line_idx + 1;
        let trimmed = line.trim_start();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if cells.len() < 7 {
            continue;
        }
        let id = cells[1];
        let neg_control = cells[5];
        let brought_by = cells[6];

        if id == "ID" || id.starts_with("---") {
            continue;
        }
        if !neg_control.ends_with('†') {
            continue;
        }

        let bb_norm = brought_by.replace('–', "-");
        if has_word(&bb_norm, "E1")
            || has_word(&bb_norm, "R2")
            || has_word(&bb_norm, "Day 30")
            || has_word(&bb_norm, "Day 60-75")
        {
            continue;
        }

        let mut relevant_slugs = Vec::new();
        let mut idx = 0;
        while let Some(open) = brought_by[idx..].find('`') {
            let start = idx + open + 1;
            if let Some(close) = brought_by[start..].find('`') {
                let token = &brought_by[start..start + close];
                if plan_slugs.contains(token) || token == "workspace-skeleton" {
                    relevant_slugs.push(token);
                }
                idx = start + close + 1;
            } else {
                break;
            }
        }

        if relevant_slugs.is_empty() {
            continue;
        }

        let all_merged = relevant_slugs.iter().all(|slug| is_merged(snap, slug));
        if !all_merged {
            continue;
        }

        let has_test = known_bad_controls.contains(id) || marker_controls.contains(id);
        if !has_test {
            findings.push(Finding::new(
                "docs/controls.md",
                line_no,
                format!("control {id} has no test: add `// control: {id}` above its negative-control test or `control = \"{id}\"` to a known-bad case"),
            ));
        }
    }

    findings
}

/// Enforces that retired terminology does not appear in active non-historical documentation.
pub fn retired_terms_absent(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();

    for (path, text) in checked_markdown(snap) {
        if path == "docs/decisions/0001-foundation.md"
            || path == "docs/decisions/0004-company-boundary.md"
            || path == "docs/cards/drafts/ai-config-and-keys.md"
            || path == "docs/cards/ai-config-and-keys.md"
            || path == "docs/cards/docs-checks.md"
            || path.starts_with("docs/design/")
        {
            continue;
        }

        for (line_idx, line) in text.lines().enumerate() {
            let line_no = line_idx + 1;
            let line_lower = line.to_lowercase();

            for &term in RETIRED {
                let term_lower = term.to_lowercase();
                let mut start = 0;

                while let Some(idx) = line_lower[start..].find(&term_lower) {
                    let pos = start + idx;
                    let before_ok = if pos == 0 {
                        true
                    } else {
                        let prev = line.as_bytes()[pos - 1] as char;
                        !prev.is_alphanumeric() && prev != '_' && prev != '-'
                    };
                    let after_pos = pos + term.len();
                    let after_ok = if after_pos >= line.len() {
                        true
                    } else {
                        let next = line.as_bytes()[after_pos] as char;
                        !next.is_alphanumeric() && next != '_' && next != '-'
                    };

                    if before_ok && after_ok {
                        findings.push(Finding::new(
                            path,
                            line_no,
                            format!("retired term `{term}`"),
                        ));
                    }
                    start = pos + term_lower.len().max(1);
                }
            }
        }
    }

    findings
}

fn has_phrase_or_word(text: &str, target: &str) -> bool {
    let mut start = 0;
    while let Some(idx) = text[start..].find(target) {
        let pos = start + idx;
        let before_ok = if pos == 0 {
            true
        } else {
            let prev = text.as_bytes()[pos - 1] as char;
            !prev.is_alphanumeric() && prev != '-'
        };
        let after_pos = pos + target.len();
        let after_ok = if after_pos >= text.len() {
            true
        } else {
            let next = text.as_bytes()[after_pos] as char;
            !next.is_alphanumeric() && next != '-'
        };
        if before_ok && after_ok {
            return true;
        }
        start = pos + target.len().max(1);
    }
    false
}

/// Ensures handoff documents and mockup assets do not prohibit staged pricing and estimate capabilities.
pub fn pricing_capability_is_not_prohibited(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();

    let prohibition_words = ["never", "must not", "do not", "does not", "cannot", "no"];
    let creation_words = [
        "create",
        "creates",
        "created",
        "creating",
        "generate",
        "generates",
        "generated",
        "generating",
        "build",
        "builds",
        "built",
        "building",
        "produce",
        "produces",
        "produced",
        "producing",
        "draft",
        "drafts",
        "drafted",
        "drafting",
        "write",
        "writes",
        "wrote",
        "written",
        "writing",
        "compute",
        "computes",
        "computed",
        "computing",
    ];
    let target_words = [
        "estimates",
        "estimate",
        "price volumes",
        "price volume",
        "price",
    ];
    let condition_words = [
        "until",
        "before",
        "once",
        "after",
        "when",
        "unless",
        "if",
        "not yet",
        "not built",
        "unavailable",
        "lands",
        "landing",
        "p7",
        "module",
    ];
    let pointer_phrases = [
        "pointer only",
        "pointers only",
        "only a pointer",
        "only pointers",
        "stay pointers",
        "ends at the pointer",
        "no stored estimate",
    ];
    let permitted_beside = ["stored", "company storage", "calculation", "calculations"];

    if let Some(handoff) = snap.text.get("docs/design/HANDOFF.md") {
        let mut in_section_16 = false;

        for (line_idx, line) in handoff.lines().enumerate() {
            let line_no = line_idx + 1;
            let trimmed = line.trim();

            if trimmed.starts_with("**Revision ") {
                continue;
            }
            if trimmed.starts_with('|') {
                let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
                if cells.len() > 1 && cells[1].to_lowercase().contains("(revision ") {
                    continue;
                }
            }

            if trimmed.starts_with("## 16.") {
                in_section_16 = true;
                continue;
            } else if in_section_16 && trimmed.starts_with("## ") {
                in_section_16 = false;
            }

            let is_active_checkbox =
                in_section_16 && (trimmed.starts_with("- [ ]") || trimmed.starts_with("- [x]"));

            let is_proposal_row = trimmed.starts_with('|') && {
                let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
                cells.len() > 1 && cells[1] == "**Proposal price volume**"
            };

            let mentions_landing = trimmed.to_lowercase().contains("landing line");

            if !is_active_checkbox && !is_proposal_row && !mentions_landing {
                continue;
            }

            for (_, sent) in split_sentences(trimmed) {
                let s_lower = sent.to_lowercase();

                let has_proh = prohibition_words
                    .iter()
                    .any(|&w| has_phrase_or_word(&s_lower, w));
                let has_crea = creation_words
                    .iter()
                    .any(|&w| has_phrase_or_word(&s_lower, w));
                let has_targ = target_words
                    .iter()
                    .any(|&w| has_phrase_or_word(&s_lower, w));
                let has_cond = condition_words
                    .iter()
                    .any(|&w| has_phrase_or_word(&s_lower, w));

                if has_proh && has_crea && has_targ && !has_cond {
                    findings.push(Finding::new(
                        "docs/design/HANDOFF.md",
                        line_no,
                        "sentence contains absolute prohibition on creating estimates or price volumes",
                    ));
                }

                let has_pointer = pointer_phrases.iter().any(|&p| s_lower.contains(p));
                let has_storage = permitted_beside.iter().any(|&p| s_lower.contains(p));

                if has_pointer && !has_storage {
                    findings.push(Finding::new(
                        "docs/design/HANDOFF.md",
                        line_no,
                        "unconditional pointer-only ending for estimate records",
                    ));
                }
            }
        }
    }

    if let Some(mockup) = snap.text.get("docs/design/mockup/index.html") {
        for (line_idx, line) in mockup.lines().enumerate() {
            let line_no = line_idx + 1;
            if line.contains("id:'TP-11'") {
                for (_, sent) in split_sentences(line) {
                    let s_lower = sent.to_lowercase();

                    let has_proh = prohibition_words
                        .iter()
                        .any(|&w| has_phrase_or_word(&s_lower, w));
                    let has_crea = creation_words
                        .iter()
                        .any(|&w| has_phrase_or_word(&s_lower, w));
                    let has_targ = target_words
                        .iter()
                        .any(|&w| has_phrase_or_word(&s_lower, w));
                    let has_cond = condition_words
                        .iter()
                        .any(|&w| has_phrase_or_word(&s_lower, w));

                    if has_proh && has_crea && has_targ && !has_cond {
                        findings.push(Finding::new(
                            "docs/design/mockup/index.html",
                            line_no,
                            "sentence contains absolute prohibition on creating estimates or price volumes",
                        ));
                    }

                    let has_pointer = pointer_phrases.iter().any(|&p| s_lower.contains(p));
                    let has_storage = permitted_beside.iter().any(|&p| s_lower.contains(p));

                    if has_pointer && !has_storage {
                        findings.push(Finding::new(
                            "docs/design/mockup/index.html",
                            line_no,
                            "unconditional pointer-only ending for estimate records",
                        ));
                    }
                }
            }
        }
    }

    findings
}

/// Run all pure snapshot documentation lints, returning findings sorted deterministically.
pub fn lint_all(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(posture_wording_names_the_build(snap));
    findings.extend(later_slug(snap));
    findings.extend(adding_recipes_are_indexed(snap));
    findings.extend(controls_have_tests(snap));
    findings.extend(retired_terms_absent(snap));
    findings.extend(pricing_capability_is_not_prohibited(snap));
    findings.sort();
    findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Snapshot;

    #[test]
    fn posture_lint_passes_a_sentence_with_one_of_the_four_words() {
        let snap = Snapshot::from_files(&[
            ("docs/posture.md", "Exact search needs no model.\n"),
            (
                "docs/failing.md",
                "This system features no network, no socket, no listener, and no model.\n",
            ),
        ]);
        let findings = posture_wording_names_the_build(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/failing.md");
        assert_eq!(findings[0].line, 1);
        assert_eq!(
            findings[0].message,
            "network-posture sentence names no build (silent build, edition or AI build)"
        );
    }

    #[test]
    fn posture_lint_skips_the_two_state_labels() {
        let snap = Snapshot::from_files(&[
            (
                "docs/pass1.md",
                "No model here · no network, no socket, no listener, no model.\n",
            ),
            (
                "docs/pass2.md",
                "No model connected here: no network, no socket, no listener, no model.\n",
            ),
            (
                "docs/fail.md",
                "A claim that contains no network, no socket, no listener, and no model.\n",
            ),
        ]);
        let findings = posture_wording_names_the_build(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/fail.md");
    }

    #[test]
    fn posture_lint_canonical_quoted_exactly_passes() {
        let snap = Snapshot::from_files(&[(
            "docs/canonical.md",
            "The silent build opens no socket, starts no process, contains no model and makes no outbound connection.\n",
        )]);
        let findings = posture_wording_names_the_build(&snap);
        assert!(
            findings.is_empty(),
            "expected empty findings, got: {:?}",
            findings
        );
    }

    #[test]
    fn posture_lint_allows_a_continuation_and_a_shared_opening() {
        let snap = Snapshot::from_files(&[(
            "docs/continued.md",
            "In the silent build and the AI build alike, `backup` and `restore` start no process, install no scheduled task and make no network connection, so IT's tooling integrates through plain files.\n\nIn the silent build and the AI build alike, the renderer builds the pages from the same reports.\n",
        )]);
        assert!(posture_wording_names_the_build(&snap).is_empty());
    }

    #[test]
    fn posture_lint_canonical_edited_in_one_place_fails() {
        let snap = Snapshot::from_files(&[(
            "docs/edited.md",
            "The silent build opens no socket, starts no process, contains no model and makes an altered connection.\n",
        )]);
        let findings = posture_wording_names_the_build(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/edited.md");
        assert_eq!(
            findings[0].message,
            "network-posture form differs from the canonical sentence"
        );
    }

    #[test]
    fn posture_lint_design_folder_skipped() {
        let snap = Snapshot::from_files(&[(
            "docs/design/posture.md",
            "This system features no network, no socket, no listener, and no model.\n",
        )]);
        let findings = posture_wording_names_the_build(&snap);
        assert!(findings.is_empty());
    }

    #[test]
    fn later_slug_skips_link_targets() {
        let adr_content = "#### AD-24 What is not built yet (Later)\n\n| Slug | Module | Port | Notes |\n|---|---|---|---|\n| later-calc-functions | Work | none | notes |\n";
        let doc_content = "[Adding a Later item](README.md#adding-a-later-item)\n\nSee `README.md#adding-a-later-item`.\n\nSee `later-calc-functions`.\n\nPlanted invalid: later-missing-item.\n";
        let snap = Snapshot::from_files(&[
            ("docs/decisions/0001-foundation.md", adr_content),
            ("docs/reference/guide.md", doc_content),
        ]);
        let findings = later_slug(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/reference/guide.md");
        assert_eq!(findings[0].line, 7);
        assert!(findings[0].message.contains("later-missing-item"));
    }

    #[test]
    fn later_slug_fails_on_duplicate_and_invalid_slugs() {
        let adr_content = "#### AD-24 What is not built yet (Later)\n\n| Slug | Module | Port | Notes |\n|---|---|---|---|\n| later-item-one | Work | none | notes |\n| later-item-one | Work | none | duplicate |\n| later-Bad_Slug | Work | none | bad |\n";
        let snap = Snapshot::from_files(&[("docs/decisions/0001-foundation.md", adr_content)]);
        let findings = later_slug(&snap);
        assert_eq!(findings.len(), 2);
        assert!(findings.iter().any(|f| {
            f.message
                .contains("duplicate AD-24 row slug `later-item-one`")
        }));
        assert!(findings.iter().any(|f| {
            f.message
                .contains("invalid AD-24 row slug `later-Bad_Slug`")
        }));
    }

    #[test]
    fn docs_generated_in_sync() {
        let root = crate::proc::repo_root();
        let findings = generated_in_sync(&root);
        assert!(findings.is_empty(), "findings: {:?}", findings);
    }

    #[test]
    fn adding_recipe_outside_the_readme_fails() {
        let readme_content = "## Recipes for recurring changes\n\n- [`docs/reference/widget.md#adding-a-widget`](../reference/widget.md#adding-a-widget)\n\n## Next section\n";
        let doc_fail = "# Section\n\n### Adding a gadget\n";
        let doc_pass = "# Section\n\n### Adding a widget\n";
        let design_doc = "# Design\n\n### Adding a gadget\n";

        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", readme_content),
            ("docs/reference/widget.md", doc_pass),
            ("docs/reference/gadget.md", doc_fail),
            ("docs/design/gadget.md", design_doc),
        ]);

        let findings = adding_recipes_are_indexed(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/reference/gadget.md");
        assert_eq!(findings[0].line, 3);
        assert_eq!(
            findings[0].message,
            "recipe heading `### Adding a gadget` is not indexed in the Recipes section of docs/cards/README.md"
        );
    }

    #[test]
    fn control_without_a_test_fails_and_names_the_id() {
        let controls = "# Controls\n\n| ID | Catches | Runs | Evidence | Negative control | Brought by |\n|---|---|---|---|---|---|\n| XX-01 | Test control | PR CI | log | Fault planted † | `demo-card` |\n| XX-02 | Test control 2 | PR CI | log | Fault planted † | `demo-card` |\n";
        let plan = "## P1 Phase 1\n\n- demo-card: item\n";
        let test_rs = format!(
            "{}\n\npub fn test_something() {{}}\n",
            concat!("// con", "trol: XX-01, XX-02")
        );

        let snap = Snapshot::from_files(&[
            ("docs/controls.md", controls),
            ("docs/plan/PLAN.md", plan),
            ("crates/fieldbook/tests/demo.rs", &test_rs),
        ])
        .with_completed("demo-card");

        let findings = controls_have_tests(&snap);
        assert!(
            findings.is_empty(),
            "expected both to pass, got: {:?}",
            findings
        );

        // Marker 4 lines above fn fails
        let test_rs_far = format!(
            "{}\n\n// line 1\n// line 2\n// line 3\npub fn test_something() {{}}\n",
            concat!("// con", "trol: XX-01, XX-02")
        );
        let snap_far = Snapshot::from_files(&[
            ("docs/controls.md", controls),
            ("docs/plan/PLAN.md", plan),
            ("crates/fieldbook/tests/demo.rs", &test_rs_far),
        ])
        .with_completed("demo-card");

        let findings_far = controls_have_tests(&snap_far);
        assert_eq!(findings_far.len(), 2);
        assert!(
            findings_far[0]
                .message
                .contains("control XX-01 has no test")
        );
        assert!(
            findings_far[1]
                .message
                .contains("control XX-02 has no test")
        );

        // Known-bad case passes
        let snap_kb = Snapshot::from_files(&[
            ("docs/controls.md", controls),
            ("docs/plan/PLAN.md", plan),
            ("tests/known-bad/cases.toml", "[[case]]\ncode = \"FB0101\"\ncontrol = \"XX-01\"\n\n[[case]]\ncode = \"FB0102\"\ncontrol = \"XX-02\"\n"),
        ])
        .with_completed("demo-card");
        let findings_kb = controls_have_tests(&snap_kb);
        assert!(findings_kb.is_empty());
    }

    #[test]
    fn control_is_skipped_until_its_card_merges() {
        let controls = "# Controls\n\n| ID | Catches | Runs | Evidence | Negative control | Brought by |\n|---|---|---|---|---|---|\n| XX-01 | Test control | PR CI | log | Fault planted † | `open-card` |\n| XX-02 | Test control | PR CI | log | Fault planted † | R2 |\n| XX-03 | Test control | PR CI | log | Fault planted † | Day 30 |\n| XX-04 | Skeleton control | PR CI | log | Fault planted † | `workspace-skeleton` |\n";
        let plan = "## P1 Phase 1\n\n- open-card: item\n- workspace-skeleton: item\n";
        let snap = Snapshot::from_files(&[
            ("docs/controls.md", controls),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/open-card.md", "# open-card\n"),
        ]);

        let findings = controls_have_tests(&snap);
        // XX-01 is skipped because open-card still exists.
        // XX-02 and XX-03 are skipped because of R2 and Day 30.
        // XX-04 is required because workspace-skeleton is merged (legacy) and has no test!
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 8);
        assert!(findings[0].message.contains("control XX-04 has no test"));
    }

    #[test]
    fn retired_term_fails_and_names_the_line() {
        let snap = Snapshot::from_files(&[
            (
                "docs/reference/guide.md",
                "Line 1\nLine 2 with approved_for here\nLine 3\n",
            ),
            (
                "docs/decisions/0004-company-boundary.md",
                "approved_for is retired here\n",
            ),
            ("docs/cards/docs-checks.md", "approved_for\n"),
            (
                "docs/cards/drafts/ai-config-and-keys.md",
                "approved_for and [model.route]\n",
            ),
            (
                "docs/cards/ai-config-and-keys.md",
                "approved_for and [model.route]\n",
            ),
            ("docs/cards/other.md", "approved_for\n"),
            (
                "docs/boundaries.md",
                "data classification and script/data classification pass\ndata class fails\nhandling zones pass\nhandling zone fails\n",
            ),
        ]);

        let findings = retired_terms_absent(&snap);
        assert!(findings.iter().any(|f| f.file == "docs/reference/guide.md"
            && f.line == 2
            && f.message.contains("approved_for")));
        assert!(findings.iter().any(|f| f.file == "docs/cards/other.md"
            && f.line == 1
            && f.message.contains("approved_for")));
        assert!(findings.iter().any(|f| f.file == "docs/boundaries.md"
            && f.line == 2
            && f.message.contains("data class")));
        assert!(findings.iter().any(|f| f.file == "docs/boundaries.md"
            && f.line == 4
            && f.message.contains("handling zone")));
        assert!(
            !findings
                .iter()
                .any(|f| f.file == "docs/decisions/0004-company-boundary.md")
        );
        assert!(
            !findings
                .iter()
                .any(|f| f.file == "docs/cards/docs-checks.md")
        );
        assert!(
            !findings
                .iter()
                .any(|f| f.file == "docs/cards/drafts/ai-config-and-keys.md")
        );
        assert!(
            !findings
                .iter()
                .any(|f| f.file == "docs/cards/ai-config-and-keys.md")
        );
    }

    #[test]
    fn pricing_capability_is_not_prohibited() {
        let valid_handoff = "## 16. Acceptance checklist\n\n- [ ] Before the pricing module lands, generation controls show their landing line. Once built, estimates and price volumes use the shared calculation, field-evidence, approval and destination contracts, with company content in company storage. Keep pointers where another company system remains the system of record. Estimate records and calculations are permitted in company storage under those contracts; retain an official-system pointer when applicable.\n";
        let valid_mockup = "{id:'TP-11',name:'Basis of estimate',land:'P7, after core-policy and the export destination · Bids & proposals, Estimates; retain an official-system pointer when applicable'}\n";

        let snap_valid = Snapshot::from_files(&[
            ("docs/design/HANDOFF.md", valid_handoff),
            ("docs/design/mockup/index.html", valid_mockup),
        ]);
        let findings_valid = super::pricing_capability_is_not_prohibited(&snap_valid);
        assert!(
            findings_valid.is_empty(),
            "expected valid to pass, got: {:?}",
            findings_valid
        );

        // Planted absolute prohibition in checklist
        let fail_checkbox = "## 16. Acceptance checklist\n\n- [ ] Fieldbook never creates estimates or price volumes.\n";
        let snap_fail_check = Snapshot::from_files(&[("docs/design/HANDOFF.md", fail_checkbox)]);
        let findings_fail_check = super::pricing_capability_is_not_prohibited(&snap_fail_check);
        assert_eq!(findings_fail_check.len(), 1);
        assert_eq!(findings_fail_check[0].line, 3);
        assert!(
            findings_fail_check[0]
                .message
                .contains("absolute prohibition")
        );

        // Planted in landing entry (HANDOFF states-table row and mockup land: text)
        let fail_landing =
            "| **Proposal price volume** | Fieldbook never creates estimates or price volumes. |\n";
        let fail_mockup =
            "{id:'TP-11',land:'Fieldbook never creates estimates or price volumes.'}\n";
        let snap_fail_landing = Snapshot::from_files(&[
            ("docs/design/HANDOFF.md", fail_landing),
            ("docs/design/mockup/index.html", fail_mockup),
        ]);
        let findings_landing = super::pricing_capability_is_not_prohibited(&snap_fail_landing);
        assert_eq!(findings_landing.len(), 2);

        // Planted in **Revision 17 note is ignored
        let revision_note =
            "**Revision 17 — Fieldbook never creates estimates or price volumes.**\n";
        let snap_revision = Snapshot::from_files(&[("docs/design/HANDOFF.md", revision_note)]);
        let findings_rev = super::pricing_capability_is_not_prohibited(&snap_revision);
        assert!(findings_rev.is_empty());

        // Unconditional pointer-only fails; pointer beside stored records passes
        let fail_pointer =
            "## 16. Acceptance checklist\n\n- [ ] Estimate records are pointers only.\n";
        let pass_pointer = "## 16. Acceptance checklist\n\n- [ ] Estimate records are pointers only beside stored records.\n";
        let snap_pointer_fail = Snapshot::from_files(&[("docs/design/HANDOFF.md", fail_pointer)]);
        let snap_pointer_pass = Snapshot::from_files(&[("docs/design/HANDOFF.md", pass_pointer)]);

        assert_eq!(
            super::pricing_capability_is_not_prohibited(&snap_pointer_fail).len(),
            1
        );
        assert!(super::pricing_capability_is_not_prohibited(&snap_pointer_pass).is_empty());
    }

    #[test]
    #[ignore]
    fn run_on_real_repo() {
        let root = crate::proc::repo_root();
        let snap = Snapshot::load(&root).expect("load snapshot");
        let findings = lint_all(&snap);
        println!("Real repo findings count: {}", findings.len());
        for f in &findings {
            println!("  {f}");
        }
    }
}
