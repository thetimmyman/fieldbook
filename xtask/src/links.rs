//! Markdown link, AGENTS.md and wording lints (`docs-checks`).

use std::collections::{BTreeMap, BTreeSet};

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::cards::{Finding, Snapshot};

/// The Markdown files the document lints read: every `.md` file under `docs/` plus
/// `README.md`, `AGENTS.md`, `CHANGELOG.md` and `SECURITY.md`, in path order.
pub fn checked_markdown(snap: &Snapshot) -> Vec<(&str, &str)> {
    snap.markdown()
        .filter(|(path, _)| {
            path.starts_with("docs/")
                || matches!(
                    *path,
                    "README.md" | "AGENTS.md" | "CHANGELOG.md" | "SECURITY.md"
                )
        })
        .collect()
}

/// The 1-based line of a byte offset in `text`.
pub fn line_of(text: &str, byte_offset: usize) -> usize {
    let end = byte_offset.min(text.len());
    text.as_bytes()
        .iter()
        .take(end)
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1
}

/// The anchor of a heading text as GitHub makes it: lowercase, characters other than
/// letters, digits, spaces, hyphens and underscores removed, spaces turned into hyphens.
fn anchor_base(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_'))
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

/// The heading anchors of a Markdown file, in order, with `-1`, `-2` added to repeats.
/// A heading inside a fenced code block is not a heading. The text of a code span or a
/// link counts, without its backticks or brackets.
pub fn heading_anchors(text: &str) -> Vec<String> {
    let mut anchors = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut in_heading = false;
    let mut current = String::new();
    for event in Parser::new_ext(text, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH) {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                current.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                let base = anchor_base(&current);
                let count = seen.entry(base.clone()).or_insert(0);
                let anchor = if *count == 0 {
                    base.clone()
                } else {
                    format!("{base}-{count}")
                };
                *count += 1;
                anchors.push(anchor);
            }
            Event::Text(t) | Event::Code(t) if in_heading => current.push_str(&t),
            Event::SoftBreak | Event::HardBreak if in_heading => current.push(' '),
            _ => {}
        }
    }
    anchors
}

/// Percent-decode `%20`-style escapes in URI targets.
fn percent_decode(input: &str) -> String {
    let mut bytes = Vec::with_capacity(input.len());
    let input_bytes = input.as_bytes();
    let mut i = 0;
    while i < input_bytes.len() {
        if input_bytes[i] == b'%' && i + 2 < input_bytes.len() {
            let h1 = from_hex(input_bytes[i + 1]);
            let h2 = from_hex(input_bytes[i + 2]);
            if let (Some(v1), Some(v2)) = (h1, h2) {
                bytes.push((v1 << 4) | v2);
                i += 3;
                continue;
            }
        }
        bytes.push(input_bytes[i]);
        i += 1;
    }
    String::from_utf8(bytes).unwrap_or_else(|_| input.to_owned())
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Returns true if the target begins with a URI scheme (letters, digits, `+`, `-`, `.` then `:`).
fn is_scheme_link(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else {
        return false;
    };
    if scheme.is_empty() || scheme.contains(['/', '?', '#']) {
        return false;
    }
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

/// Check a single link or image target against the repository snapshot.
fn check_target(
    file: &str,
    line: usize,
    raw_target: &str,
    snap: &Snapshot,
    findings: &mut Vec<Finding>,
) {
    if raw_target.trim().is_empty() {
        findings.push(Finding::new(file, line, "link target not found: "));
        return;
    }
    if is_scheme_link(raw_target) {
        return;
    }
    if raw_target.starts_with('/') {
        findings.push(Finding::new(
            file,
            line,
            format!("link target is absolute: {raw_target}"),
        ));
        return;
    }

    let (path_and_query, fragment) = match raw_target.split_once('#') {
        Some((pq, f)) => (pq, Some(f)),
        None => (raw_target, None),
    };
    let raw_path = match path_and_query.split_once('?') {
        Some((p, _)) => p,
        None => path_and_query,
    };
    let decoded_path = percent_decode(raw_path);
    let decoded_frag = fragment.map(percent_decode);

    if decoded_path.is_empty() {
        if let Some(frag) = decoded_frag {
            let target_text = snap.text.get(file).map_or("", String::as_str);
            let anchors = heading_anchors(target_text);
            if !anchors.iter().any(|a| a == &frag) {
                findings.push(Finding::new(
                    file,
                    line,
                    format!("link fragment not found: #{frag} in {file}"),
                ));
            }
        } else {
            findings.push(Finding::new(file, line, "link target not found: "));
        }
        return;
    }

    let file_dir = match file.rsplit_once('/') {
        Some((dir, _)) => dir,
        None => "",
    };

    let mut segments: Vec<&str> = if file_dir.is_empty() {
        Vec::new()
    } else {
        file_dir.split('/').collect()
    };
    let mut climbed_out = false;
    for seg in decoded_path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    climbed_out = true;
                    break;
                }
            }
            other => segments.push(other),
        }
    }

    if climbed_out {
        findings.push(Finding::new(
            file,
            line,
            format!("link climbs out of the repository: {raw_target}"),
        ));
        return;
    }

    let resolved = segments.join("/");
    if !snap.exists(&resolved) {
        findings.push(Finding::new(
            file,
            line,
            format!("link target not found: {resolved}"),
        ));
        return;
    }

    if let Some(frag) = decoded_frag
        && resolved.ends_with(".md")
        && let Some(target_text) = snap.text.get(&resolved)
    {
        let anchors = heading_anchors(target_text);
        if !anchors.iter().any(|a| a == &frag) {
            findings.push(Finding::new(
                file,
                line,
                format!("link fragment not found: #{frag} in {resolved}"),
            ));
        }
    }
}

/// Check links and images in documentation Markdown files against repository files.
pub fn lint_links(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (file, text) in checked_markdown(snap) {
        let parser = Parser::new_ext(text, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH)
            .into_offset_iter();
        for (event, range) in parser {
            let (Event::Start(Tag::Link { dest_url: dest, .. })
            | Event::Start(Tag::Image { dest_url: dest, .. })) = event
            else {
                continue;
            };
            let line = line_of(text, range.start);
            check_target(file, line, dest.as_ref(), snap, &mut findings);
        }
    }
    findings
}

/// Check AGENTS.md line count, hard stop reason tags, and backticked commands.
pub fn lint_agents(snap: &Snapshot, verbs: &BTreeSet<String>) -> Vec<Finding> {
    let Some(text) = snap.text.get("AGENTS.md") else {
        return Vec::new();
    };

    let mut findings = Vec::new();

    let line_count = text.lines().count();
    if line_count > 100 {
        findings.push(Finding::new(
            "AGENTS.md",
            0,
            format!("{line_count} lines, limit 100"),
        ));
    }

    check_hard_stops(text, &mut findings);

    let justfile = snap.text.get("justfile").map_or("", String::as_str);
    let justfile_recipes = parse_justfile_recipes(justfile);

    let xtask_commands = if let Some(main_rs) = snap.text.get("xtask/src/main.rs") {
        let cmds = parse_xtask_commands(main_rs);
        if cmds.is_empty() {
            default_xtask_commands()
        } else {
            cmds
        }
    } else {
        default_xtask_commands()
    };

    check_agents_commands(
        text,
        &justfile_recipes,
        &xtask_commands,
        verbs,
        &mut findings,
    );

    findings
}

fn check_hard_stops(text: &str, findings: &mut Vec<Finding>) {
    let mut in_hard_stops = false;
    for (idx, line) in text.lines().enumerate() {
        let line_num = idx + 1;
        if line.starts_with("## ") {
            in_hard_stops = line.trim() == "## Hard stops";
            continue;
        }
        if in_hard_stops && line.starts_with("- ") {
            let bullet_content = &line[2..];
            if !has_hard_stop_reason_tag(bullet_content) {
                findings.push(Finding::new(
                    "AGENTS.md",
                    line_num,
                    "Hard stop has no reason tag",
                ));
            }
        }
    }
}

fn has_hard_stop_reason_tag(s: &str) -> bool {
    let valid_tags = ["[leak]", "[certify]", "[corrupt]"];
    let mut cur = s.trim_start();
    let mut count = 0;
    while count < 2 {
        let mut matched = false;
        for tag in &valid_tags {
            if let Some(rest) = cur.strip_prefix(tag) {
                cur = rest.trim_start();
                count += 1;
                matched = true;
                break;
            }
        }
        if !matched {
            break;
        }
    }
    count >= 1
}

fn parse_justfile_recipes(justfile: &str) -> BTreeSet<String> {
    let mut recipes = BTreeSet::new();
    for line in justfile.lines() {
        if line.is_empty() || line.starts_with([' ', '\t', '#']) {
            continue;
        }
        let s = line.strip_prefix('@').unwrap_or(line);
        if s.starts_with("set ") {
            continue;
        }
        let Some((before_colon, after_colon)) = s.split_once(':') else {
            continue;
        };
        if after_colon.starts_with('=') {
            continue;
        }
        let mut parts = before_colon.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        if name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            recipes.insert(name.to_owned());
        }
    }
    recipes
}

fn parse_xtask_commands(main_rs: &str) -> BTreeSet<String> {
    let mut commands = BTreeSet::new();
    let mut in_usage = false;
    for line in main_rs.lines() {
        if line.contains("usage: cargo xtask <command>") {
            in_usage = true;
            continue;
        }
        if in_usage {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if line.starts_with("  ") || line.starts_with('\t') {
                let mut words = line.split_whitespace();
                if let Some(cmd) = words.next() {
                    commands.insert(cmd.to_owned());
                }
            }
            if trimmed.ends_with("\";") {
                break;
            }
        }
    }
    commands
}

fn default_xtask_commands() -> BTreeSet<String> {
    [
        "doctor",
        "setup",
        "docs",
        "cards",
        "new-card",
        "scope",
        "guard-statuses",
        "denylist",
        "compat",
        "binscan",
        "archive-release",
        "verify-archive",
        "edition-impact",
        "harness",
        "hosted-limits",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn collect_bullets(text: &str) -> Vec<(usize, &str)> {
    let mut bullets = Vec::new();
    let mut start_opt = None;
    let mut current_offset = 0;
    for line in text.split_inclusive('\n') {
        let line_len = line.len();
        if line.starts_with("- ") || line.starts_with("* ") {
            if let Some(start) = start_opt {
                bullets.push((start, &text[start..current_offset]));
            }
            start_opt = Some(current_offset);
        } else if (line.starts_with('#') || line.trim().is_empty())
            && let Some(start) = start_opt
        {
            bullets.push((start, &text[start..current_offset]));
            start_opt = None;
        }
        current_offset += line_len;
    }
    if let Some(start) = start_opt {
        bullets.push((start, &text[start..current_offset]));
    }
    bullets
}

fn split_sentences(bullet: &str) -> Vec<(usize, &str)> {
    let mut sentences = Vec::new();
    let mut in_code = false;
    let mut sentence_start = 0;
    let bytes = bullet.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            in_code = !in_code;
            i += 1;
            continue;
        }
        if !in_code && bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1] == b' ' {
            let sentence = &bullet[sentence_start..=i];
            sentences.push((sentence_start, sentence));
            sentence_start = i + 2;
            i += 2;
            continue;
        }
        i += 1;
    }
    if sentence_start < bullet.len() {
        sentences.push((sentence_start, &bullet[sentence_start..]));
    }
    sentences
}

fn is_sentence_exempt(sentence: &str) -> bool {
    let trimmed = sentence
        .trim_start()
        .trim_start_matches(['-', '*'])
        .trim_start();
    let Some(after_from) = trimmed.strip_prefix("From ") else {
        return false;
    };
    let s = after_from.trim_start();
    if !s.starts_with('`') {
        return false;
    }
    let Some(end_backtick) = s[1..].find('`') else {
        return false;
    };
    let slug = &s[1..=end_backtick];
    !slug.is_empty() && slug.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn check_agents_commands(
    full_text: &str,
    recipes: &BTreeSet<String>,
    xtask_cmds: &BTreeSet<String>,
    verbs: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    let bullets = collect_bullets(full_text);
    for (bullet_offset, bullet) in bullets {
        let sentences = split_sentences(bullet);
        for (sentence_rel_offset, sentence) in sentences {
            if is_sentence_exempt(sentence) {
                continue;
            }
            let sentence_offset = bullet_offset + sentence_rel_offset;
            check_sentence_commands(
                full_text,
                sentence_offset,
                sentence,
                recipes,
                xtask_cmds,
                verbs,
                findings,
            );
        }
    }
}

fn check_sentence_commands(
    full_text: &str,
    sentence_offset: usize,
    sentence: &str,
    recipes: &BTreeSet<String>,
    xtask_cmds: &BTreeSet<String>,
    verbs: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    let bytes = sentence.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let start = i + 1;
            if let Some(end_rel) = sentence[start..].find('`') {
                let end = start + end_rel;
                let code = &sentence[start..end];
                let code_offset = sentence_offset + start;
                let line = line_of(full_text, code_offset);
                check_single_command(line, code, recipes, xtask_cmds, verbs, findings);
                i = end + 1;
                continue;
            }
        }
        i += 1;
    }
}

fn check_single_command(
    line: usize,
    code: &str,
    recipes: &BTreeSet<String>,
    xtask_cmds: &BTreeSet<String>,
    verbs: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    if let Some(rest) = code.strip_prefix("just ") {
        let first_word = rest.split_whitespace().next().unwrap_or("");
        if !recipes.contains(first_word) {
            findings.push(Finding::new(
                "AGENTS.md",
                line,
                format!("unknown command: {code}"),
            ));
        }
    } else if let Some(rest) = code.strip_prefix("cargo xtask ") {
        let first_word = rest.split_whitespace().next().unwrap_or("");
        if !xtask_cmds.contains(first_word) {
            findings.push(Finding::new(
                "AGENTS.md",
                line,
                format!("unknown command: {code}"),
            ));
        }
    } else if let Some(rest) = code.strip_prefix("fieldbook ") {
        let first_token = rest.split_whitespace().next().unwrap_or("");
        if first_token.starts_with('-') {
            return;
        }
        let first_word = first_token.split('|').next().unwrap_or("");
        if !verbs.contains(first_word) {
            findings.push(Finding::new(
                "AGENTS.md",
                line,
                format!("unknown command: {code}"),
            ));
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn match_owner_gates(sub_lower: &str) -> Option<usize> {
    let prefix_len = if sub_lower.starts_with("owner gates ") {
        12
    } else if sub_lower.starts_with("owner gate ") {
        11
    } else {
        return None;
    };
    let rest = &sub_lower[prefix_len..];
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count > 0 {
        Some(prefix_len + digit_count)
    } else {
        None
    }
}

fn match_gate(sub_lower: &str) -> Option<usize> {
    if !sub_lower.starts_with("gate ") {
        return None;
    }
    let rest = &sub_lower[5..];
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count > 0 {
        Some(5 + digit_count)
    } else {
        None
    }
}

fn match_cards(sub_lower: &str) -> Option<usize> {
    let prefix_len = if sub_lower.starts_with("cards ") {
        6
    } else if sub_lower.starts_with("card ") {
        5
    } else {
        return None;
    };
    let rest = &sub_lower[prefix_len..];
    let digit_count = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_count == 0 {
        return None;
    }
    let after_digits = &rest[digit_count..];
    if after_digits.chars().next().is_none_or(|c| !is_word_char(c)) {
        Some(prefix_len + digit_count)
    } else {
        None
    }
}

fn match_five_gates(sub_lower: &str) -> Option<usize> {
    let mlen = if sub_lower.starts_with("five owner gates") {
        16
    } else if sub_lower.starts_with("five gates") {
        10
    } else {
        return None;
    };
    let after = &sub_lower[mlen..];
    if after.chars().next().is_none_or(|c| !is_word_char(c)) {
        Some(mlen)
    } else {
        None
    }
}

fn check_wording_in_line(file: &str, line_num: usize, line: &str, findings: &mut Vec<Finding>) {
    let mut byte_pos = 0;
    while byte_pos < line.len() {
        if byte_pos > 0 {
            let Some(prev_char) = line[..byte_pos].chars().next_back() else {
                break;
            };
            if is_word_char(prev_char) {
                let Some(ch) = line[byte_pos..].chars().next() else {
                    break;
                };
                byte_pos += ch.len_utf8();
                continue;
            }
        }

        let sub = &line[byte_pos..];
        let sub_lower = sub.to_ascii_lowercase();

        if let Some(m_len) = match_owner_gates(&sub_lower) {
            let matched = &line[byte_pos..byte_pos + m_len];
            findings.push(Finding::new(
                file,
                line_num,
                format!("retired wording `{matched}`: name cards by slug and gates by name (release, status or format)"),
            ));
            byte_pos += m_len;
            continue;
        }

        if let Some(m_len) = match_gate(&sub_lower) {
            let matched = &line[byte_pos..byte_pos + m_len];
            findings.push(Finding::new(
                file,
                line_num,
                format!("retired wording `{matched}`: name cards by slug and gates by name (release, status or format)"),
            ));
            byte_pos += m_len;
            continue;
        }

        if let Some(m_len) = match_cards(&sub_lower) {
            let matched = &line[byte_pos..byte_pos + m_len];
            findings.push(Finding::new(
                file,
                line_num,
                format!("retired wording `{matched}`: name cards by slug and gates by name (release, status or format)"),
            ));
            byte_pos += m_len;
            continue;
        }

        if let Some(m_len) = match_five_gates(&sub_lower) {
            let matched = &line[byte_pos..byte_pos + m_len];
            findings.push(Finding::new(
                file,
                line_num,
                format!("retired wording `{matched}`: name cards by slug and gates by name (release, status or format)"),
            ));
            byte_pos += m_len;
            continue;
        }

        let Some(ch) = line[byte_pos..].chars().next() else {
            break;
        };
        byte_pos += ch.len_utf8();
    }
}

fn check_wording_in_file(file: &str, text: &str, findings: &mut Vec<Finding>) {
    let mut skip_next_line = false;
    for (idx, line) in text.lines().enumerate() {
        let line_num = idx + 1;
        if skip_next_line {
            skip_next_line = false;
            continue;
        }
        if line.trim() == "<!-- legacy-quote -->" {
            skip_next_line = true;
            continue;
        }
        check_wording_in_line(file, line_num, line, findings);
    }
}

/// Check documentation Markdown files for retired numbering wording (cards, gates).
pub fn lint_wording(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (file, text) in checked_markdown(snap) {
        if file.starts_with("docs/design/") {
            continue;
        }
        check_wording_in_file(file, text, &mut findings);
    }
    findings
}

/// The subcommand names and aliases of the `fieldbook` CLI.
pub fn verbs_of_cli() -> BTreeSet<String> {
    let mut verbs = BTreeSet::new();
    let cmd = <fieldbook::Cli as clap::CommandFactory>::command();
    for sub in cmd.get_subcommands() {
        verbs.insert(sub.get_name().to_string());
        for alias in sub.get_all_aliases() {
            verbs.insert(alias.to_string());
        }
    }
    verbs
}

/// Non-component PascalCase words in MAP.md to skip (a product name, not a component).
pub const NOT_COMPONENT: &[&str] = &["PowerShell"];

/// Check that stated IT question counts across Markdown files match the number of
/// question rows in `docs/intake/it-questions.md`, and that question numbers run
/// from 1 to the count with no gap or repeat.
pub fn it_count_matches_rows(snap: &Snapshot) -> Vec<Finding> {
    let Some(it_text) = snap.text.get("docs/intake/it-questions.md") else {
        return Vec::new();
    };

    let mut question_rows = Vec::new();
    for (line_idx, line) in it_text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').collect();
        if cells.len() < 3 {
            continue;
        }
        let first_cell = cells[1].trim().trim_matches('`').trim();
        if let Some(num_str) = first_cell.strip_prefix("IT-")
            && !num_str.is_empty()
            && num_str.chars().all(|c| c.is_ascii_digit())
            && let Ok(n) = num_str.parse::<usize>()
        {
            question_rows.push((line_num, n));
        }
    }

    let m = question_rows.len();
    let mut findings = Vec::new();

    let mut expected = 1;
    for (line_num, n) in &question_rows {
        if *n != expected {
            if *n < expected {
                findings.push(Finding::new(
                    "docs/intake/it-questions.md",
                    *line_num,
                    format!("duplicate or out-of-order IT question IT-{n}: expected IT-{expected}"),
                ));
            } else {
                findings.push(Finding::new(
                    "docs/intake/it-questions.md",
                    *line_num,
                    format!("gap in IT question numbering: expected IT-{expected}, found IT-{n}"),
                ));
                expected = n + 1;
                continue;
            }
        }
        expected += 1;
    }

    for (file, text) in checked_markdown(snap) {
        for (line_idx, line) in text.lines().enumerate() {
            let line_num = line_idx + 1;
            for n in extract_it_question_counts(line) {
                if n != m {
                    findings.push(Finding::new(
                        file,
                        line_num,
                        format!(
                            "states {n} IT questions, but docs/intake/it-questions.md has {m} rows"
                        ),
                    ));
                }
            }
        }
    }

    findings
}

fn extract_it_question_counts(line: &str) -> Vec<usize> {
    let mut counts = Vec::new();

    // 1. `IT-1 to IT-<N>`
    let mut pos = 0;
    while let Some(found) = line[pos..].find("IT-1 to IT-") {
        let idx = pos + found;
        let before_ok = match line[..idx].chars().next_back() {
            Some(prev) => !is_word_char(prev) && prev != '-',
            None => true,
        };
        let after_prefix = idx + 11;
        let rest = &line[after_prefix..];
        let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if before_ok && digit_len > 0 {
            let after_num = &rest[digit_len..];
            let after_ok = match after_num.chars().next() {
                Some(c) => !is_word_char(c) && c != '-',
                None => true,
            };
            if after_ok && let Ok(n) = rest[..digit_len].parse::<usize>() {
                counts.push(n);
            }
        }
        pos = idx + 11;
    }

    // 2. `all <N> questions` (or `All <N> questions`)
    for prefix in &["all ", "All "] {
        let mut p_pos = 0;
        while let Some(found) = line[p_pos..].find(prefix) {
            let idx = p_pos + found;
            let before_ok = match line[..idx].chars().next_back() {
                Some(prev) => !is_word_char(prev),
                None => true,
            };
            let rest = &line[idx + prefix.len()..];
            let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if before_ok && digit_len > 0 {
                let after_digits = &rest[digit_len..];
                if let Some(after_q) = after_digits.strip_prefix(" questions") {
                    let after_ok = match after_q.chars().next() {
                        Some(c) => !is_word_char(c),
                        None => true,
                    };
                    if after_ok && let Ok(n) = rest[..digit_len].parse::<usize>() {
                        counts.push(n);
                    }
                }
            }
            p_pos = idx + prefix.len();
        }
    }

    // 3. `All <N> IT questions` (or `all <N> IT questions`)
    for prefix in &["All ", "all "] {
        let mut p_pos = 0;
        while let Some(found) = line[p_pos..].find(prefix) {
            let idx = p_pos + found;
            let before_ok = match line[..idx].chars().next_back() {
                Some(prev) => !is_word_char(prev),
                None => true,
            };
            let rest = &line[idx + prefix.len()..];
            let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if before_ok && digit_len > 0 {
                let after_digits = &rest[digit_len..];
                if let Some(after_q) = after_digits.strip_prefix(" IT questions") {
                    let after_ok = match after_q.chars().next() {
                        Some(c) => !is_word_char(c),
                        None => true,
                    };
                    if after_ok && let Ok(n) = rest[..digit_len].parse::<usize>() {
                        counts.push(n);
                    }
                }
            }
            p_pos = idx + prefix.len();
        }
    }

    // 4. `The <N> questions` (or `the <N> questions`)
    for prefix in &["The ", "the "] {
        let mut p_pos = 0;
        while let Some(found) = line[p_pos..].find(prefix) {
            let idx = p_pos + found;
            let before_ok = match line[..idx].chars().next_back() {
                Some(prev) => !is_word_char(prev),
                None => true,
            };
            let rest = &line[idx + prefix.len()..];
            let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
            if before_ok && digit_len > 0 {
                let after_digits = &rest[digit_len..];
                if let Some(after_q) = after_digits.strip_prefix(" questions") {
                    let after_ok = match after_q.chars().next() {
                        Some(c) => !is_word_char(c),
                        None => true,
                    };
                    if after_ok && let Ok(n) = rest[..digit_len].parse::<usize>() {
                        counts.push(n);
                    }
                }
            }
            p_pos = idx + prefix.len();
        }
    }

    counts
}

struct MappingRow {
    line_num: usize,
    components: Vec<String>,
    root_cell: String,
}

/// Check that components in `docs/design/` agree across MAP.md, PLAN.md, and HANDOFF.md,
/// and that every declared component root exists in `docs/design/mockup/index.html`.
pub fn component_roots_agree(snap: &Snapshot) -> Vec<Finding> {
    let Some(handoff_text) = snap.text.get("docs/design/HANDOFF.md") else {
        return Vec::new();
    };

    let mut findings = Vec::new();

    let sec5_components = parse_section_5_components(handoff_text);
    let sec5_names: BTreeSet<String> = sec5_components
        .iter()
        .map(|(_, name)| name.clone())
        .collect();

    let (mapping_rows, reason_paragraph) = parse_mapping_table(handoff_text);

    if let Some(map_text) = snap.text.get("docs/design/MAP.md") {
        check_map_file(snap, map_text, &sec5_names, &mut findings);
    }

    check_components_agreement(&sec5_components, &mapping_rows, &mut findings);
    check_declared_roots(snap, &mapping_rows, &reason_paragraph, &mut findings);

    findings
}

fn parse_section_5_components(text: &str) -> Vec<(usize, String)> {
    let mut components = Vec::new();
    let mut in_sec5 = false;
    let mut in_table = false;

    for (line_idx, line) in text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if trimmed.starts_with("## 5. Components") {
            in_sec5 = true;
            continue;
        }
        if in_sec5
            && (trimmed.starts_with("### ")
                || (trimmed.starts_with("## ") && !trimmed.starts_with("## 5.")))
        {
            break;
        }

        if in_sec5 {
            if trimmed.starts_with("| Component") || trimmed.starts_with("|Component") {
                in_table = true;
                continue;
            }
            if in_table {
                if !trimmed.starts_with('|') {
                    in_table = false;
                    continue;
                }
                if trimmed.starts_with("|---") || trimmed.starts_with("| ---") {
                    continue;
                }
                let cells: Vec<&str> = trimmed.split('|').collect();
                if cells.len() < 3 {
                    continue;
                }
                let comp_cell = cells[1];
                for token in extract_backticked_tokens(comp_cell) {
                    if is_component_name(&token) {
                        components.push((line_num, token));
                    }
                }
            }
        }
    }
    components
}

fn is_component_name(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn parse_mapping_table(text: &str) -> (Vec<MappingRow>, String) {
    let mut rows = Vec::new();
    let mut reason_paragraph = String::new();

    let mut in_root_section = false;
    let mut in_table = false;
    let mut table_ended = false;

    for (line_idx, line) in text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if trimmed.starts_with("### Root selectors in the mockup") {
            in_root_section = true;
            continue;
        }
        if in_root_section && !table_ended {
            if (trimmed.starts_with("| Component") || trimmed.starts_with("|Component"))
                && trimmed.contains("Root")
            {
                in_table = true;
                continue;
            }
            if in_table {
                if !trimmed.starts_with('|') {
                    in_table = false;
                    table_ended = true;
                    if !trimmed.is_empty() {
                        reason_paragraph.push_str(trimmed);
                        reason_paragraph.push(' ');
                    }
                    continue;
                }
                if trimmed.starts_with("|---") || trimmed.starts_with("| ---") {
                    continue;
                }
                let cells: Vec<&str> = trimmed.split('|').collect();
                if cells.len() < 3 {
                    continue;
                }
                let comp_cell = cells[1];
                let root_cell = cells[2].trim().to_owned();
                let comps: Vec<String> = extract_backticked_tokens(comp_cell)
                    .into_iter()
                    .filter(|t| is_component_name(t))
                    .collect();
                rows.push(MappingRow {
                    line_num,
                    components: comps,
                    root_cell,
                });
            }
        } else if in_root_section && table_ended {
            if trimmed.starts_with('#') {
                break;
            }
            if !trimmed.is_empty() {
                reason_paragraph.push_str(trimmed);
                reason_paragraph.push(' ');
            }
        }
    }

    (rows, reason_paragraph)
}

fn check_map_file(
    snap: &Snapshot,
    map_text: &str,
    sec5_names: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    let plan_slugs = match crate::cards::plan_items(snap) {
        Ok(items) => items
            .into_iter()
            .map(|item| item.slug)
            .collect::<BTreeSet<String>>(),
        Err(err) => {
            findings.push(Finding::new("docs/design/MAP.md", 1, err));
            BTreeSet::new()
        }
    };

    check_map_opening_paragraph(map_text, &plan_slugs, findings);
    check_map_tables(map_text, &plan_slugs, sec5_names, findings);
}

fn is_slug(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    for part in token.split('-') {
        if part.is_empty()
            || !part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return false;
        }
    }
    true
}

fn strip_parentheses(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn extract_backticked_tokens(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut in_code = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '`' {
            if in_code {
                tokens.push(s[start..i].to_owned());
                in_code = false;
            } else {
                start = i + 1;
                in_code = true;
            }
        }
    }
    tokens
}

fn check_map_opening_paragraph(
    map_text: &str,
    plan_slugs: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    for (line_idx, line) in map_text.lines().enumerate() {
        let line_num = line_idx + 1;
        if line.starts_with("## ") {
            break;
        }
        for prefix in &["The Day 30 cards", "Some slugs"] {
            if let Some(pos) = line.find(prefix) {
                let sentence = match line[pos..].find(". ") {
                    Some(end) => &line[pos..pos + end + 1],
                    None => &line[pos..],
                };
                let stripped = strip_parentheses(sentence);
                for token in extract_backticked_tokens(&stripped) {
                    if is_slug(&token) && !plan_slugs.contains(&token) {
                        findings.push(Finding::new(
                            "docs/design/MAP.md",
                            line_num,
                            format!("card `{token}` is not an item of docs/plan/PLAN.md"),
                        ));
                    }
                }
            }
        }
    }
}

fn strip_code_spans(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_code = false;
    for c in s.chars() {
        if c == '`' {
            in_code = !in_code;
        } else if !in_code {
            out.push(c);
        }
    }
    out
}

fn is_pascal_case(w: &str) -> bool {
    let mut chars = w.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_uppercase() {
        return false;
    }
    let mut has_another_upper = false;
    let mut has_lower = false;
    for c in chars {
        if c.is_ascii_uppercase() {
            has_another_upper = true;
        } else if c.is_ascii_lowercase() {
            has_lower = true;
        }
    }
    has_another_upper && has_lower
}

fn extract_pascal_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            cur.push(c);
        } else {
            if is_pascal_case(&cur) {
                words.push(cur.clone());
            }
            cur.clear();
        }
    }
    if is_pascal_case(&cur) {
        words.push(cur);
    }
    words
}

fn check_map_tables(
    map_text: &str,
    plan_slugs: &BTreeSet<String>,
    sec5_names: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    let mut in_table = false;
    let mut card_col_idx: Option<usize> = None;

    for (line_idx, line) in map_text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if !trimmed.starts_with('|') {
            in_table = false;
            card_col_idx = None;
            continue;
        }

        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }

        if !in_table {
            for (idx, cell) in cells.iter().enumerate() {
                if *cell == "Card" {
                    card_col_idx = Some(idx);
                    in_table = true;
                    break;
                }
            }
            continue;
        }

        if trimmed.starts_with("|---") || trimmed.starts_with("| ---") {
            continue;
        }

        if let Some(card_col) = card_col_idx
            && card_col < cells.len()
        {
            let card_cell = cells[card_col];
            let stripped = strip_parentheses(card_cell);
            for token in extract_backticked_tokens(&stripped) {
                if is_slug(&token) && !plan_slugs.contains(&token) {
                    findings.push(Finding::new(
                        "docs/design/MAP.md",
                        line_num,
                        format!("card `{token}` is not an item of docs/plan/PLAN.md"),
                    ));
                }
            }
        }

        let first_col_raw = cells[1];
        let without_code = strip_code_spans(first_col_raw);
        for w in extract_pascal_words(&without_code) {
            if !NOT_COMPONENT.contains(&w.as_str()) && !sec5_names.contains(&w) {
                findings.push(Finding::new(
                    "docs/design/MAP.md",
                    line_num,
                    format!("component `{w}` is not in section 5 of docs/design/HANDOFF.md"),
                ));
            }
        }
    }
}

fn check_components_agreement(
    sec5_components: &[(usize, String)],
    mapping_rows: &[MappingRow],
    findings: &mut Vec<Finding>,
) {
    let mut mapping_components = Vec::new();
    for row in mapping_rows {
        for comp in &row.components {
            mapping_components.push((row.line_num, comp.clone()));
        }
    }

    let mut seen_mapping = BTreeSet::new();
    for (line_num, comp) in &mapping_components {
        if !seen_mapping.insert(comp.clone()) {
            findings.push(Finding::new(
                "docs/design/HANDOFF.md",
                *line_num,
                format!("duplicate mapping row for component `{comp}`"),
            ));
        }
    }

    let mut seen_sec5 = BTreeSet::new();
    for (line_num, comp) in sec5_components {
        if !seen_sec5.insert(comp.clone()) {
            findings.push(Finding::new(
                "docs/design/HANDOFF.md",
                *line_num,
                format!("duplicate section 5 row for component `{comp}`"),
            ));
        }
    }

    for (sec5_line, comp) in sec5_components {
        if !seen_mapping.contains(comp) {
            findings.push(Finding::new(
                "docs/design/HANDOFF.md",
                *sec5_line,
                format!("component `{comp}` has no mapping row in root selectors table"),
            ));
        }
    }

    for (map_line, comp) in &mapping_components {
        if !seen_sec5.contains(comp) {
            findings.push(Finding::new(
                "docs/design/HANDOFF.md",
                *map_line,
                format!("mapping row component `{comp}` is not in section 5"),
            ));
        }
    }

    if sec5_components.len() == mapping_components.len() {
        for ((_, sec5_c), (map_line, map_c)) in sec5_components.iter().zip(&mapping_components) {
            if sec5_c != map_c {
                findings.push(Finding::new(
                    "docs/design/HANDOFF.md",
                    *map_line,
                    format!("component order mismatch: expected `{sec5_c}`, found `{map_c}`"),
                ));
                break;
            }
        }
    }
}

fn class_selector_in_html(html: &str, class_name: &str) -> bool {
    let needle = format!(".{class_name}");
    let mut pos = 0;
    while let Some(found) = html[pos..].find(&needle) {
        let idx = pos + found;
        let after = idx + needle.len();
        if after >= html.len() {
            return true;
        }
        let Some(next_char) = html[after..].chars().next() else {
            return true;
        };
        if !next_char.is_ascii_alphanumeric() && next_char != '-' && next_char != '_' {
            return true;
        }
        pos = idx + 1;
    }
    false
}

fn tag_in_html(html: &str, tag_name: &str) -> bool {
    let needle = format!("<{tag_name}");
    let mut pos = 0;
    while let Some(found) = html[pos..].find(&needle) {
        let idx = pos + found;
        let after = idx + needle.len();
        if after >= html.len() {
            return true;
        }
        let Some(next_char) = html[after..].chars().next() else {
            return true;
        };
        if !next_char.is_ascii_alphanumeric() && next_char != '-' && next_char != '_' {
            return true;
        }
        pos = idx + 1;
    }
    false
}

fn check_declared_roots(
    snap: &Snapshot,
    mapping_rows: &[MappingRow],
    reason_paragraph: &str,
    findings: &mut Vec<Finding>,
) {
    let mockup_html = snap
        .text
        .get("docs/design/mockup/index.html")
        .map(String::as_str);

    let mut declared_roots: BTreeMap<String, (String, usize)> = BTreeMap::new();

    for row in mapping_rows {
        let clean_root = row.root_cell.trim().trim_matches('`').trim();
        if clean_root == "—" {
            for comp in &row.components {
                if !reason_paragraph.contains(comp) {
                    findings.push(Finding::new(
                        "docs/design/HANDOFF.md",
                        row.line_num,
                        format!("component `{comp}` has root `—` but is not mentioned in the following paragraph"),
                    ));
                }
            }
            continue;
        }

        let parts: Vec<&str> = clean_root.split(',').collect();
        for (part_idx, part) in parts.iter().enumerate() {
            let root = part.trim().trim_matches('`').trim();
            let comp_name =
                if parts.len() == row.components.len() && part_idx < row.components.len() {
                    &row.components[part_idx]
                } else if let Some(first) = row.components.first() {
                    first
                } else {
                    "unknown"
                };

            let is_class = root.starts_with('.')
                && root.len() > 1
                && root[1..]
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            let is_native = !root.is_empty()
                && !root.starts_with('.')
                && root
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());

            if !is_class && !is_native {
                findings.push(Finding::new(
                    "docs/design/HANDOFF.md",
                    row.line_num,
                    format!("root `{root}` for component `{comp_name}` is neither `—` nor a class nor an element name"),
                ));
                continue;
            }

            if let Some((prev_comp, _prev_line)) = declared_roots.get(root) {
                if prev_comp != comp_name {
                    findings.push(Finding::new(
                        "docs/design/HANDOFF.md",
                        row.line_num,
                        format!(
                            "root `{root}` is declared by both `{prev_comp}` and `{comp_name}`"
                        ),
                    ));
                }
            } else {
                declared_roots.insert(root.to_owned(), (comp_name.to_owned(), row.line_num));
            }

            if let Some(html) = mockup_html {
                let found = if is_class {
                    class_selector_in_html(html, &root[1..])
                } else {
                    tag_in_html(html, root)
                };
                if !found {
                    findings.push(Finding::new(
                        "docs/design/HANDOFF.md",
                        row.line_num,
                        format!(
                            "declared root `{root}` for component `{comp_name}` not found in mockup"
                        ),
                    ));
                }
            } else {
                findings.push(Finding::new(
                    "docs/design/mockup/index.html",
                    0,
                    format!(
                        "declared root `{root}` for component `{comp_name}` not found in mockup"
                    ),
                ));
            }
        }
    }
}

/// Check that references to "unsafe code" or "C code" in documentation Markdown files
/// are qualified with "first-party" (case-insensitive) or "build graph".
pub fn lint_unsafe_and_c(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (file, text) in checked_markdown(snap) {
        for (line_idx, line) in text.lines().enumerate() {
            let line_num = line_idx + 1;
            let line_lower = line.to_ascii_lowercase();
            if line_lower.contains("first-party") || line.contains("build graph") {
                continue;
            }
            if has_unsafe_or_c_code(line) {
                findings.push(Finding::new(
                    file,
                    line_num,
                    "says \"unsafe code\" or \"C code\" without \"first-party\" or \"build graph\"",
                ));
            }
        }
    }
    findings
}

fn has_unsafe_or_c_code(line: &str) -> bool {
    if line.contains("unsafe code") || line.contains("`unsafe` code") {
        return true;
    }

    let mut pos = 0;
    while let Some(found) = line[pos..].find("C code") {
        let idx = pos + found;
        let before_ok = match line[..idx].chars().next_back() {
            Some(prev) => !is_word_char(prev),
            None => true,
        };
        let after_idx = idx + 6;
        let after_ok = match line[after_idx..].chars().next() {
            Some(next) => !is_word_char(next),
            None => true,
        };
        if before_ok && after_ok {
            return true;
        }
        pos = idx + 1;
    }

    let mut pos = 0;
    while let Some(found) = line[pos..].find("`C` code") {
        let idx = pos + found;
        let before_ok = match line[..idx].chars().next_back() {
            Some(prev) => !is_word_char(prev),
            None => true,
        };
        let after_idx = idx + 8;
        let after_ok = match line[after_idx..].chars().next() {
            Some(next) => !is_word_char(next),
            None => true,
        };
        if before_ok && after_ok {
            return true;
        }
        pos = idx + 1;
    }

    false
}

/// Check that every control ID named in the `## Critical failures` table of `docs/controls.md`
/// is a row in the control register tables of that file.
pub fn control_ids_in_critical_failures_are_rows(snap: &Snapshot) -> Vec<Finding> {
    let Some(text) = snap.text.get("docs/controls.md") else {
        return Vec::new();
    };

    let mut register_ids: BTreeSet<String> = BTreeSet::new();
    let mut critical_ids: Vec<(usize, String)> = Vec::new();
    let mut in_critical_failures = false;

    for (line_idx, line) in text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if trimmed.starts_with("## ") {
            in_critical_failures = trimmed == "## Critical failures";
            continue;
        }

        if !trimmed.starts_with('|') {
            continue;
        }

        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }

        if in_critical_failures {
            if cells[1] == "#" || cells[1].starts_with("---") {
                continue;
            }
            let mut line_ids = BTreeSet::new();
            for id in extract_control_ids(trimmed) {
                line_ids.insert(id);
            }
            for id in line_ids {
                critical_ids.push((line_num, id));
            }
        } else {
            let first_cell = cells[1].trim_matches('`').trim();
            if is_control_id(first_cell) {
                register_ids.insert(first_cell.to_owned());
            }
        }
    }

    let mut findings = Vec::new();
    for (line_num, id) in critical_ids {
        if !register_ids.contains(&id) {
            findings.push(Finding::new(
                "docs/controls.md",
                line_num,
                format!("control id {id} is not a row of the register"),
            ));
        }
    }
    findings
}

fn is_control_id(s: &str) -> bool {
    if s.len() != 5 {
        return false;
    }
    let b = s.as_bytes();
    b[0].is_ascii_uppercase()
        && b[1].is_ascii_uppercase()
        && b[2] == b'-'
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
}

fn extract_control_ids(s: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let bytes = s.as_bytes();
    if bytes.len() < 5 {
        return ids;
    }
    for i in 0..=bytes.len() - 5 {
        if bytes[i].is_ascii_uppercase()
            && bytes[i + 1].is_ascii_uppercase()
            && bytes[i + 2] == b'-'
            && bytes[i + 3].is_ascii_digit()
            && bytes[i + 4].is_ascii_digit()
        {
            if i > 0 {
                let prev = bytes[i - 1] as char;
                if prev.is_ascii_alphanumeric() || prev == '_' || prev == '-' {
                    continue;
                }
            }
            if i + 5 < bytes.len() {
                let next = bytes[i + 5] as char;
                if next.is_ascii_alphanumeric() || next == '_' || next == '-' {
                    continue;
                }
            }
            if let Ok(id_str) = std::str::from_utf8(&bytes[i..i + 5]) {
                ids.push(id_str.to_owned());
            }
        }
    }
    ids
}

/// Run all link, AGENTS.md, and document wording checks across the snapshot.
pub fn lint_all(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(lint_links(snap));
    findings.extend(lint_agents(snap, &verbs_of_cli()));
    findings.extend(lint_wording(snap));
    findings.extend(it_count_matches_rows(snap));
    findings.extend(component_roots_agree(snap));
    findings.extend(lint_unsafe_and_c(snap));
    findings.extend(control_ids_in_critical_failures_are_rows(snap));
    findings
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        checked_markdown, heading_anchors, line_of, lint_agents, lint_all, lint_links,
        lint_unsafe_and_c, lint_wording, verbs_of_cli,
    };
    use crate::cards::Snapshot;

    #[test]
    fn checked_markdown_names_the_step_five_files() {
        let snap = Snapshot::from_files(&[
            ("README.md", "a"),
            ("docs/x.md", "b"),
            ("corpus/y.md", "c"),
            ("docs/x.txt", "d"),
            ("notes.md", "e"),
        ]);
        let paths: Vec<&str> = checked_markdown(&snap).iter().map(|(p, _)| *p).collect();
        assert_eq!(paths, vec!["README.md", "docs/x.md"]);
    }

    #[test]
    fn anchors_follow_github() {
        let text = "# Title\n\n## The `foo` verb\n\n## x\n\n## x\n\n## x\n\n```\n## not a heading\n```\n\n### Adding a token / a unit?\n";
        assert_eq!(
            heading_anchors(text),
            vec![
                "title",
                "the-foo-verb",
                "x",
                "x-1",
                "x-2",
                "adding-a-token--a-unit"
            ]
        );
    }

    #[test]
    fn line_of_counts_newlines() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 4), 3);
    }

    #[test]
    fn good_relative_link_passes() {
        let snap = Snapshot::from_files(&[
            ("docs/a.md", "See [b](b.md) for details."),
            ("docs/b.md", "# Heading\n"),
        ]);
        let findings = lint_links(&snap);
        assert!(
            findings.is_empty(),
            "expected no findings, got: {findings:?}"
        );
    }

    #[test]
    fn missing_target_fails_and_names_file_line_target() {
        let snap = Snapshot::from_files(&[(
            "docs/cards/README.md",
            "Line 1\nLine 2 [missing](../reference/missing.md)\n",
        )]);
        let findings = lint_links(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/cards/README.md");
        assert_eq!(findings[0].line, 2);
        assert_eq!(
            findings[0].message,
            "link target not found: docs/reference/missing.md"
        );
    }

    #[test]
    fn link_climbs_out_of_the_root() {
        let snap = Snapshot::from_files(&[("docs/a.md", "See [out](../../outside.md)")]);
        let findings = lint_links(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/a.md");
        assert_eq!(findings[0].line, 1);
        assert_eq!(
            findings[0].message,
            "link climbs out of the repository: ../../outside.md"
        );
    }

    #[test]
    fn scheme_link_is_ignored() {
        let snap = Snapshot::from_files(&[(
            "docs/a.md",
            "[web](https://example.com) [mail](mailto:test@example.com) [js](javascript:void(0))",
        )]);
        let findings = lint_links(&snap);
        assert!(
            findings.is_empty(),
            "scheme links should be ignored: {findings:?}"
        );
    }

    #[test]
    fn code_span_and_fenced_code_links_are_skipped() {
        let snap = Snapshot::from_files(&[(
            "docs/a.md",
            "`[inline](missing.md)`\n\n```\n[fenced](missing.md)\n```\n",
        )]);
        let findings = lint_links(&snap);
        assert!(
            findings.is_empty(),
            "code links must be skipped: {findings:?}"
        );
    }

    #[test]
    fn folder_target_passes() {
        let snap = Snapshot::from_files(&[
            ("docs/a.md", "[ref](../docs/) and [self](.)"),
            ("docs/sub/b.md", "# B"),
        ]);
        let findings = lint_links(&snap);
        assert!(
            findings.is_empty(),
            "folder targets should pass: {findings:?}"
        );
    }

    #[test]
    fn fragment_matches_heading_anchor() {
        let snap = Snapshot::from_files(&[
            (
                "docs/a.md",
                "See [anchor](b.md#the-section) and [local](#local-section)\n\n## Local section",
            ),
            ("docs/b.md", "# The section\n"),
        ]);
        let findings = lint_links(&snap);
        assert!(
            findings.is_empty(),
            "matching fragments should pass: {findings:?}"
        );
    }

    #[test]
    fn fragment_missing_fails() {
        let snap = Snapshot::from_files(&[
            (
                "docs/a.md",
                "See [missing](b.md#nonexistent) and [here](#missing-local)",
            ),
            ("docs/b.md", "# Other section\n"),
        ]);
        let findings = lint_links(&snap);
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings[0].message,
            "link fragment not found: #nonexistent in docs/b.md"
        );
        assert_eq!(
            findings[1].message,
            "link fragment not found: #missing-local in docs/a.md"
        );
    }

    #[test]
    fn image_target_checked_like_link() {
        let snap = Snapshot::from_files(&[
            ("docs/a.md", "![good](img.png) ![bad](missing.png)"),
            ("docs/img.png", "image-bytes"),
        ]);
        let findings = lint_links(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].message,
            "link target not found: docs/missing.png"
        );
    }

    #[test]
    fn reference_style_links_resolve() {
        let snap = Snapshot::from_files(&[
            ("docs/a.md", "[ref link][label]\n\n[label]: target.md"),
            ("docs/target.md", "# Target"),
        ]);
        let findings = lint_links(&snap);
        assert!(
            findings.is_empty(),
            "reference links should resolve: {findings:?}"
        );
    }

    #[test]
    fn table_cell_link_is_read() {
        let snap = Snapshot::from_files(&[(
            "docs/a.md",
            "| Header |\n|---|\n| [cell link](missing.md) |",
        )]);
        let findings = lint_links(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].message,
            "link target not found: docs/missing.md"
        );
    }

    #[test]
    fn empty_target_and_absolute_target_fail() {
        let snap = Snapshot::from_files(&[("docs/a.md", "[empty]()\n[abs](/absolute/path.md)\n")]);
        let findings = lint_links(&snap);
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].message, "link target not found: ");
        assert_eq!(
            findings[1].message,
            "link target is absolute: /absolute/path.md"
        );
    }

    #[test]
    fn percent_encoded_target_resolves() {
        let snap = Snapshot::from_files(&[
            ("docs/a.md", "See [spaced](file%20name.md)"),
            ("docs/file name.md", "# Spaced"),
        ]);
        let findings = lint_links(&snap);
        assert!(
            findings.is_empty(),
            "percent-encoded spaces should resolve: {findings:?}"
        );
    }

    #[test]
    fn agents_length_limit_passes() {
        let mut lines = String::new();
        for i in 1..=100 {
            lines.push_str(&format!("Line {i}\n"));
        }
        let snap = Snapshot::from_files(&[("AGENTS.md", &lines)]);
        let findings = lint_agents(&snap, &BTreeSet::new());
        assert!(findings.is_empty());
    }

    #[test]
    fn agents_over_100_lines_fails() {
        let mut lines = String::new();
        for i in 1..=101 {
            lines.push_str(&format!("Line {i}\n"));
        }
        let snap = Snapshot::from_files(&[("AGENTS.md", &lines)]);
        let findings = lint_agents(&snap, &BTreeSet::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "AGENTS.md");
        assert_eq!(findings[0].line, 0);
        assert_eq!(findings[0].message, "101 lines, limit 100");
        assert_eq!(findings[0].to_string(), "AGENTS.md: 101 lines, limit 100");
    }

    #[test]
    fn agents_hard_stops_with_reason_tags_pass() {
        let text = "## Hard stops\n- [leak] Secret check\n- [certify] Person check\n- [corrupt] Hash check\n- [leak][corrupt] Dual check\n";
        let snap = Snapshot::from_files(&[("AGENTS.md", text)]);
        let findings = lint_agents(&snap, &BTreeSet::new());
        assert!(
            findings.is_empty(),
            "valid reason tags should pass: {findings:?}"
        );
    }

    #[test]
    fn agents_hard_stop_without_reason_tag_fails() {
        let text = "## Hard stops\n- [leak] OK\n- Missing reason tag here\n- [other] Bad tag\n";
        let snap = Snapshot::from_files(&[("AGENTS.md", text)]);
        let findings = lint_agents(&snap, &BTreeSet::new());
        assert_eq!(findings.len(), 2);
        assert_eq!(findings[0].line, 3);
        assert_eq!(findings[0].message, "Hard stop has no reason tag");
        assert_eq!(findings[1].line, 4);
        assert_eq!(findings[1].message, "Hard stop has no reason tag");
    }

    #[test]
    fn agents_justfile_recipes_pass_and_fail() {
        let justfile = "default:\n    @true\n\nsetup:\n    cargo xtask setup\n\nnew-card slug:\n    cargo xtask new-card {{slug}}\n";
        let agents = "## Commands\n- `just setup` and `just default`\n- `just unknown_recipe`\n";
        let snap = Snapshot::from_files(&[("justfile", justfile), ("AGENTS.md", agents)]);
        let findings = lint_agents(&snap, &BTreeSet::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 3);
        assert_eq!(findings[0].message, "unknown command: just unknown_recipe");
    }

    #[test]
    fn agents_xtask_commands_pass_and_fail() {
        let agents =
            "## Commands\n- `cargo xtask setup` and `cargo xtask cards`\n- `cargo xtask bogus`\n";
        let snap = Snapshot::from_files(&[("AGENTS.md", agents)]);
        let findings = lint_agents(&snap, &BTreeSet::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 3);
        assert_eq!(findings[0].message, "unknown command: cargo xtask bogus");
    }

    #[test]
    fn agents_fieldbook_verbs_and_flags_pass_and_fail() {
        let mut verbs = BTreeSet::new();
        verbs.insert("check".to_owned());
        verbs.insert("new".to_owned());

        let agents = "## Commands\n- `fieldbook check --strict`\n- `fieldbook --version`\n- `fieldbook new page|claim|question`\n- `fieldbook nonexistent`\n";
        let snap = Snapshot::from_files(&[("AGENTS.md", agents)]);
        let findings = lint_agents(&snap, &verbs);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 5);
        assert_eq!(
            findings[0].message,
            "unknown command: fieldbook nonexistent"
        );
    }

    #[test]
    fn agents_exempt_command_in_from_sentence_passes() {
        let agents = "## Commands\n- From `edition-zips`: `just edition`. `just nonexempt_bogus`\n- From `docs-checks`: `just new-card SLUG` and `cargo xtask cards --check`\n";
        let snap = Snapshot::from_files(&[("AGENTS.md", agents)]);
        let findings = lint_agents(&snap, &BTreeSet::new());
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
        assert_eq!(findings[0].message, "unknown command: just nonexempt_bogus");
    }

    #[test]
    fn wording_passes_clean_markdown() {
        let snap = Snapshot::from_files(&[(
            "docs/clean.md",
            "Use cards by slug and gates by name (release, status, format).\n",
        )]);
        let findings = lint_wording(&snap);
        assert!(findings.is_empty(), "clean text should pass: {findings:?}");
    }

    #[test]
    fn wording_fails_for_cards_and_digits() {
        let snap = Snapshot::from_files(&[(
            "docs/a.md",
            "See Card 12 and cards 4 for details. Discard 1 passes. card 10x passes.\n",
        )]);
        let findings = lint_wording(&snap);
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings[0].message,
            "retired wording `Card 12`: name cards by slug and gates by name (release, status or format)"
        );
        assert_eq!(
            findings[1].message,
            "retired wording `cards 4`: name cards by slug and gates by name (release, status or format)"
        );
    }

    #[test]
    fn wording_fails_for_owner_gates_and_digits() {
        let snap =
            Snapshot::from_files(&[("docs/a.md", "Check Owner Gate 1 and owner gates 2 now.\n")]);
        let findings = lint_wording(&snap);
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings[0].message,
            "retired wording `Owner Gate 1`: name cards by slug and gates by name (release, status or format)"
        );
        assert_eq!(
            findings[1].message,
            "retired wording `owner gates 2`: name cards by slug and gates by name (release, status or format)"
        );
    }

    #[test]
    fn wording_fails_for_gate_and_digits() {
        let snap = Snapshot::from_files(&[("docs/a.md", "Proceed past gate 0 and Gate 3.\n")]);
        let findings = lint_wording(&snap);
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings[0].message,
            "retired wording `gate 0`: name cards by slug and gates by name (release, status or format)"
        );
        assert_eq!(
            findings[1].message,
            "retired wording `Gate 3`: name cards by slug and gates by name (release, status or format)"
        );
    }

    #[test]
    fn wording_fails_for_five_gates_and_five_owner_gates() {
        let snap =
            Snapshot::from_files(&[("docs/a.md", "Count all five gates or Five Owner Gates.\n")]);
        let findings = lint_wording(&snap);
        assert_eq!(findings.len(), 2);
        assert_eq!(
            findings[0].message,
            "retired wording `five gates`: name cards by slug and gates by name (release, status or format)"
        );
        assert_eq!(
            findings[1].message,
            "retired wording `Five Owner Gates`: name cards by slug and gates by name (release, status or format)"
        );
    }

    #[test]
    fn wording_skips_line_after_legacy_quote() {
        let snap = Snapshot::from_files(&[(
            "docs/a.md",
            "<!-- legacy-quote -->\ncard 12 was the old way\ncard 3 is still rejected\n",
        )]);
        let findings = lint_wording(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 3);
        assert_eq!(
            findings[0].message,
            "retired wording `card 3`: name cards by slug and gates by name (release, status or format)"
        );
    }

    #[test]
    fn wording_skips_files_under_docs_design() {
        let snap = Snapshot::from_files(&[(
            "docs/design/MAP.md",
            "gate 1 and cards 2 in design folder\n",
        )]);
        let findings = lint_wording(&snap);
        assert!(
            findings.is_empty(),
            "docs/design/ must be skipped: {findings:?}"
        );
    }

    #[test]
    fn wording_skips_code_span_with_punctuation_spelled_phrase() {
        let snap = Snapshot::from_files(&[("docs/a.md", "Rule: `cards? [0-9]+` is rejected.\n")]);
        let findings = lint_wording(&snap);
        assert!(
            findings.is_empty(),
            "regex pattern text must not trigger wording ban: {findings:?}"
        );
    }

    #[test]
    fn verbs_of_cli_returns_expected_subcommands() {
        let verbs = verbs_of_cli();
        assert!(verbs.contains("check"));
        assert!(verbs.contains("build"));
        assert!(verbs.contains("add"));
        assert!(verbs.contains("new"));
        assert!(verbs.contains("search"));
    }

    #[test]
    fn lint_all_combines_findings() {
        let snap = Snapshot::from_files(&[
            ("docs/a.md", "[bad](missing.md)\ncard 9\n"),
            ("AGENTS.md", "## Hard stops\n- bad stop\n"),
        ]);
        let findings = lint_all(&snap);
        assert_eq!(findings.len(), 3);
    }

    #[test]
    fn it_count_matches_rows() {
        let it_content = "\
# Questions for IT

The owner sends all 3 questions below at the first contact.

Part A (IT-1 through IT-2) is the standing list. IT-a through IT-b is a partial range.

| Id | Question |
|---|---|
| IT-1 | First question? |
| IT-2 | Second question? |
| IT-3 | Third question? |
";
        let snap = Snapshot::from_files(&[
            ("docs/intake/it-questions.md", it_content),
            (
                "docs/intake/README.md",
                "See [IT-1 to IT-3](it-questions.md)",
            ),
            ("docs/a.md", "All 3 IT questions sent."),
            ("docs/b.md", "The 3 questions are answered."),
            ("docs/c.md", "The owner sends all 3 questions here."),
        ]);
        let findings = super::it_count_matches_rows(&snap);
        assert!(
            findings.is_empty(),
            "expected matching count to pass, got: {findings:?}"
        );

        // Changed count in file's first paragraph fails
        let it_wrong = "\
# Questions for IT

The owner sends all 4 questions below at the first contact.

| Id | Question |
|---|---|
| IT-1 | First question? |
| IT-2 | Second question? |
| IT-3 | Third question? |
";
        let snap_wrong = Snapshot::from_files(&[("docs/intake/it-questions.md", it_wrong)]);
        let findings_wrong = super::it_count_matches_rows(&snap_wrong);
        assert_eq!(findings_wrong.len(), 1);
        assert_eq!(findings_wrong[0].file, "docs/intake/it-questions.md");
        assert_eq!(findings_wrong[0].line, 3);
        assert_eq!(
            findings_wrong[0].message,
            "states 4 IT questions, but docs/intake/it-questions.md has 3 rows"
        );

        // Gap in numbering fails
        let it_gap = "\
# Questions for IT

The owner sends all 2 questions below.

| Id | Question |
|---|---|
| IT-1 | First question? |
| IT-3 | Third question? |
";
        let snap_gap = Snapshot::from_files(&[("docs/intake/it-questions.md", it_gap)]);
        let findings_gap = super::it_count_matches_rows(&snap_gap);
        assert_eq!(findings_gap.len(), 1);
        assert_eq!(findings_gap[0].file, "docs/intake/it-questions.md");
        assert_eq!(findings_gap[0].line, 8);
        assert!(
            findings_gap[0]
                .message
                .contains("gap in IT question numbering")
        );

        // Absent file returns no findings
        let snap_absent = Snapshot::from_files(&[("docs/a.md", "The 3 questions are absent.")]);
        assert!(super::it_count_matches_rows(&snap_absent).is_empty());
    }

    #[test]
    fn unsafe_and_c_wording() {
        let snap_bad = Snapshot::from_files(&[
            ("docs/a.md", "We forbid unsafe code here.\n"),
            ("docs/b.md", "Here is `unsafe` code.\n"),
            ("docs/c.md", "This has C code inside.\n"),
            ("docs/d.md", "This has `C` code inside.\n"),
        ]);
        let findings = lint_unsafe_and_c(&snap_bad);
        assert_eq!(findings.len(), 4);
        for f in &findings {
            assert_eq!(
                f.message,
                "says \"unsafe code\" or \"C code\" without \"first-party\" or \"build graph\""
            );
        }

        let snap_good = Snapshot::from_files(&[
            ("docs/a.md", "First-party crates forbid unsafe code.\n"),
            ("docs/b.md", "first-party crates forbid `unsafe` code.\n"),
            ("docs/c.md", "Dependencies and build graph have C code.\n"),
            ("docs/d.md", "Safe code without hazardous patterns.\n"),
        ]);
        let findings_good = lint_unsafe_and_c(&snap_good);
        assert!(
            findings_good.is_empty(),
            "expected pass, got: {findings_good:?}"
        );
    }

    #[test]
    fn control_ids_in_critical_failures_are_rows() {
        let good_controls = "\
## General Controls

| Id | Control |
|---|---|
| AB-01 | Control 1 |
| CD-02 | Control 2 |

## Critical failures

| # | Failure | Control or test | Note |
|---|---|---|---|
| 1 | Fail 1 | AB-01 and CD-02 | Note |
";
        let snap_good = Snapshot::from_files(&[("docs/controls.md", good_controls)]);
        assert!(super::control_ids_in_critical_failures_are_rows(&snap_good).is_empty());

        let bad_controls = "\
## General Controls

| Id | Control |
|---|---|
| AB-01 | Control 1 |

## Critical failures

| # | Failure | Control or test | Note |
|---|---|---|---|
| 1 | Fail 1 | AB-01 and XX-99 | Note |
";
        let snap_bad = Snapshot::from_files(&[("docs/controls.md", bad_controls)]);
        let findings = super::control_ids_in_critical_failures_are_rows(&snap_bad);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/controls.md");
        assert_eq!(findings[0].line, 11);
        assert_eq!(
            findings[0].message,
            "control id XX-99 is not a row of the register"
        );

        let snap_none = Snapshot::from_files(&[("docs/a.md", "no controls")]);
        assert!(super::control_ids_in_critical_failures_are_rows(&snap_none).is_empty());
    }
}

/// Named at module level so the card's `links::component_roots_come_from_the_mapping` filter selects it.
#[cfg(test)]
#[test]
fn component_roots_come_from_the_mapping() {
    let plan_text = "\
# Plan

## F0 Foundation
- build-btn: Button
- build-chain: LifecycleChain
- build-pipe: BidPipeline
- build-kbd: Kbd
";
    let map_text = "\
# Mockup to build map

The Day 30 cards are `build-btn`, `build-chain` and `build-pipe`. Some slugs name cards whose file is not written yet; the slug is final.

| Mockup route or component | AD-12 output | HANDOFF | Card |
|---|---|---|---|
| Button | `index.html` | §5 | `build-btn` |
| LifecycleChain | `chain.html` | §5 | `build-chain` |
| BidPipeline | `pipe.html` | §5 | `build-pipe` |
| Kbd | `kbd.html` | §5 | `build-kbd` |
| Edge and PowerShell command chips | `sys.html` | §5 | `build-btn` |
";
    let handoff_text = "\
## 5. Components

| Component | Props |
|---|---|
| `Button` | label |
| `LifecycleChain` | nodes |
| `BidPipeline` | stages |
| `Kbd` | keys |

### Root selectors in the mockup

| Component | Root |
|---|---|
| `Button` | `.btn` |
| `LifecycleChain` | `.chain` |
| `BidPipeline` | `.pipe` |
| `Kbd` | `kbd` |

The rows with no root of their own: none.

### Next section
";
    let mockup_html = "\
<style>
.btn { color: red; }
.chain { color: blue; }
.pipe { color: green; }
.extra-mockup-class { color: white; }
</style>
<div><kbd>Ctrl</kbd></div>
";

    let snap_ok = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
        ("docs/design/HANDOFF.md", handoff_text),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_ok = component_roots_agree(&snap_ok);
    assert!(
        findings_ok.is_empty(),
        "expected base to pass, got: {findings_ok:?}"
    );

    // Absent HANDOFF.md returns no findings
    let snap_no_handoff = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
    ]);
    assert!(component_roots_agree(&snap_no_handoff).is_empty());

    // Declared root mockup does not carry fails and names component and selector (.chain-x)
    let handoff_bad_root = handoff_text.replace("`.chain`", "`.chain-x`");
    let snap_bad_root = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
        ("docs/design/HANDOFF.md", &handoff_bad_root),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_bad_root = component_roots_agree(&snap_bad_root);
    assert_eq!(findings_bad_root.len(), 1);
    assert_eq!(findings_bad_root[0].file, "docs/design/HANDOFF.md");
    assert!(findings_bad_root[0].message.contains("LifecycleChain"));
    assert!(findings_bad_root[0].message.contains(".chain-x"));

    // Same class declared by two components fails (Button given .chain)
    let handoff_dup_root = handoff_text.replace("`.btn`", "`.chain`");
    let snap_dup_root = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
        ("docs/design/HANDOFF.md", &handoff_dup_root),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_dup_root = component_roots_agree(&snap_dup_root);
    assert_eq!(findings_dup_root.len(), 1);
    assert!(findings_dup_root[0].message.contains(".chain"));
    assert!(findings_dup_root[0].message.contains("Button"));
    assert!(findings_dup_root[0].message.contains("LifecycleChain"));

    // Deleting the BidPipeline mapping row fails
    let handoff_del_mapping = handoff_text.replace("| `BidPipeline` | `.pipe` |\n", "");
    let snap_del_mapping = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
        ("docs/design/HANDOFF.md", &handoff_del_mapping),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_del_mapping = component_roots_agree(&snap_del_mapping);
    assert!(
        findings_del_mapping
            .iter()
            .any(|f| f.message.contains("BidPipeline"))
    );

    // Section 5 component with no mapping row fails
    let handoff_extra_sec5 = handoff_text.replace(
        "| `Kbd` | keys |\n",
        "| `Kbd` | keys |\n| `ExtraComp` | props |\n",
    );
    let snap_extra_sec5 = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
        ("docs/design/HANDOFF.md", &handoff_extra_sec5),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_extra_sec5 = component_roots_agree(&snap_extra_sec5);
    assert!(
        findings_extra_sec5
            .iter()
            .any(|f| f.message.contains("ExtraComp"))
    );

    // Mapping row whose component is not in section 5 fails
    let handoff_extra_mapping = handoff_text.replace(
        "| `Kbd` | `kbd` |\n",
        "| `Kbd` | `kbd` |\n| `ExtraComp` | `.extra` |\n",
    );
    let mockup_with_extra = format!("{mockup_html}\n.extra {{}}");
    let snap_extra_map = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
        ("docs/design/HANDOFF.md", &handoff_extra_mapping),
        ("docs/design/mockup/index.html", &mockup_with_extra),
    ]);
    let findings_extra_map = component_roots_agree(&snap_extra_map);
    assert!(
        findings_extra_map
            .iter()
            .any(|f| f.message.contains("ExtraComp"))
    );

    // Root cell that is neither — nor a class nor an element name fails
    let handoff_bad_syntax = handoff_text.replace("`.btn`", "`bad_root!`");
    let snap_bad_syntax = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", map_text),
        ("docs/design/HANDOFF.md", &handoff_bad_syntax),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_bad_syntax = component_roots_agree(&snap_bad_syntax);
    assert!(findings_bad_syntax.iter().any(|f| {
        f.message
            .contains("neither `—` nor a class nor an element name")
    }));

    // Card in MAP.md that is not a PLAN item fails and names it
    let map_bad_card = map_text.replace("`build-pipe`", "`unplanned-pipe`");
    let snap_bad_card = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", &map_bad_card),
        ("docs/design/HANDOFF.md", handoff_text),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_bad_card = component_roots_agree(&snap_bad_card);
    assert_eq!(findings_bad_card.len(), 2);
    assert_eq!(
        findings_bad_card[0].message,
        "card `unplanned-pipe` is not an item of docs/plan/PLAN.md"
    );

    // Component in MAP.md first column not in section 5 fails
    let map_bad_comp = map_text.replace("Button", "UnregisteredButton");
    let snap_bad_comp = Snapshot::from_files(&[
        ("docs/plan/PLAN.md", plan_text),
        ("docs/design/MAP.md", &map_bad_comp),
        ("docs/design/HANDOFF.md", handoff_text),
        ("docs/design/mockup/index.html", mockup_html),
    ]);
    let findings_bad_comp = component_roots_agree(&snap_bad_comp);
    assert_eq!(findings_bad_comp.len(), 1);
    assert_eq!(
        findings_bad_comp[0].message,
        "component `UnregisteredButton` is not in section 5 of docs/design/HANDOFF.md"
    );
}
