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

/// Run all link, AGENTS.md, and document wording checks across the snapshot.
pub fn lint_all(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(lint_links(snap));
    findings.extend(lint_agents(snap, &verbs_of_cli()));
    findings.extend(lint_wording(snap));
    // Additional lints from subsequent tasks will be added here.
    findings
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        checked_markdown, heading_anchors, line_of, lint_agents, lint_all, lint_links,
        lint_wording, verbs_of_cli,
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
}
