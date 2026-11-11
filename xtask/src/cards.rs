//! Card lints, plan lint, scope-overlap lint and `new-card` (`docs-checks`).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "subsequent tasks (scope, links, docs_lints) and lifecycle tests consume remaining card and scope helpers"
    )
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

/// A lint finding with file, 1-based line (or 0 for whole file), and message.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// Repository-relative path to the file.
    pub file: String,
    /// 1-based line number, or 0 if whole-file or line is unavailable.
    pub line: usize,
    /// Explanatory message.
    pub message: String,
}

impl Finding {
    /// Create a new finding.
    pub fn new(file: impl Into<String>, line: usize, message: impl Into<String>) -> Self {
        Self {
            file: file.into(),
            line,
            message: message.into(),
        }
    }
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.line == 0 {
            write!(f, "{}: {}", self.file, self.message)
        } else {
            write!(f, "{}:{}: {}", self.file, self.line, self.message)
        }
    }
}

/// Paths shared across cards where edits do not trigger scope overlap failures.
pub const SHARED_PATHS: &[&str] = &[
    "docs/plan/PLAN.md",
    "CHANGELOG.md",
    "Cargo.lock",
    "justfile",
    "xtask/Cargo.toml",
    "xtask/src/main.rs",
    "crates/fieldbook/src/cli.rs",
    "crates/fieldbook/src/io/mod.rs",
    "crates/fieldbook/tests/chips.rs",
    "crates/fieldbook-render/assets/app.css",
    "crates/fieldbook-render/assets/app.js",
    "tests/known-bad/cases.toml",
    "tests/known-bad/pending.toml",
    "tests/known-bad/elsewhere.toml",
    ".github/workflows/scheduled.yml",
    ".github/workflows/ci.yml",
];

/// Returns true if the path is in `SHARED_PATHS` or matches shared path rules.
pub fn is_shared_path(path: &str) -> bool {
    if SHARED_PATHS.contains(&path) {
        return true;
    }
    if path == "lib.rs" || path.ends_with("/lib.rs") {
        return true;
    }
    if path == "check/mod.rs" || path.ends_with("/check/mod.rs") {
        return true;
    }
    false
}

/// Returns the slugs of cards that were deleted by a merged commit in git history.
pub fn completed_slugs(root: &Path) -> Result<BTreeSet<String>, String> {
    let shallow = crate::proc::git(root, &["rev-parse", "--is-shallow-repository"])?;
    if shallow.trim() == "true" {
        return Err(
            "history is shallow: run git fetch --unshallow before cargo xtask cards --check"
                .to_owned(),
        );
    }
    if crate::proc::git(root, &["rev-parse", "--verify", "HEAD"]).is_err() {
        return Ok(BTreeSet::new());
    }
    let output = crate::proc::git(
        root,
        &[
            "log",
            "--no-renames",
            "--diff-filter=D",
            "--name-only",
            "--format=",
            "--",
            "docs/cards/",
        ],
    )?;
    let mut slugs = BTreeSet::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(rest) = line.strip_prefix("docs/cards/") else {
            continue;
        };
        let Some(slug) = rest.strip_suffix(".md") else {
            continue;
        };
        if slug.contains('/') || slug == "README" {
            continue;
        }
        slugs.insert(slug.to_owned());
    }
    Ok(slugs)
}

const EXCLUDED_PREFIXES: &[&str] = &[
    "corpus/",
    "out/",
    "target/",
    ".git/",
    ".fieldbook/",
    "node_modules/",
];

fn is_excluded(path: &str) -> bool {
    EXCLUDED_PREFIXES
        .iter()
        .any(|prefix| path.starts_with(prefix) || path == prefix.trim_end_matches('/'))
}

fn is_text_ext(path: &str) -> bool {
    if path == "justfile" || path.ends_with("/justfile") {
        return true;
    }
    let Some((_, ext)) = path.rsplit_once('.') else {
        return false;
    };
    matches!(
        ext,
        "md" | "html" | "toml" | "rs" | "yml" | "yaml" | "txt" | "csv"
    )
}

/// The repository as the documentation and card lints see it.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// Every repository-relative file path (forward slashes).
    pub files: BTreeSet<String>,
    /// UTF-8 content of text files.
    pub text: BTreeMap<String, String>,
    /// Card slugs deleted in git history.
    pub completed: BTreeSet<String>,
}

impl Snapshot {
    /// Construct a snapshot in memory from path/content pairs (for testing).
    pub fn from_files(items: &[(&str, &str)]) -> Self {
        let mut files = BTreeSet::new();
        let mut text = BTreeMap::new();
        for &(path, content) in items {
            files.insert(path.to_owned());
            text.insert(path.to_owned(), content.to_owned());
        }
        Self {
            files,
            text,
            completed: BTreeSet::new(),
        }
    }

    /// Load the repository snapshot from disk using git to list tracked and untracked files.
    pub fn load(root: &Path) -> Result<Self, String> {
        let out = crate::proc::output(
            "git",
            &[
                "ls-files",
                "-z",
                "--cached",
                "--others",
                "--exclude-standard",
            ],
            root,
        )?;
        if !out.status.success() {
            return Err(format!(
                "git ls-files failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let completed = completed_slugs(root)?;
        let mut files = BTreeSet::new();
        let mut text = BTreeMap::new();

        for chunk in out.stdout.split(|&b| b == 0) {
            if chunk.is_empty() {
                continue;
            }
            let Ok(path_str) = std::str::from_utf8(chunk) else {
                continue;
            };
            let normalized = path_str.replace('\\', "/");
            let disk_path = root.join(&normalized);
            if !disk_path.is_file() {
                continue;
            }
            // Large folders (the Corpus, build output) stay in `files`, so a link to them
            // resolves, but their text is not read: no lint reads it.
            if is_text_ext(&normalized) && !is_excluded(&normalized) {
                match std::fs::read_to_string(&disk_path) {
                    Ok(content) => {
                        text.insert(normalized.clone(), content);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) if e.kind() == std::io::ErrorKind::InvalidData => continue,
                    Err(e) => return Err(format!("cannot read {normalized}: {e}")),
                }
            }
            files.insert(normalized);
        }

        Ok(Self {
            files,
            text,
            completed,
        })
    }

    /// Return a new snapshot without the specified file path.
    pub fn without(&self, path: &str) -> Self {
        let mut cloned = self.clone();
        cloned.files.remove(path);
        cloned.text.remove(path);
        cloned
    }

    /// Return a new snapshot with an additional completed card slug.
    pub fn with_completed(&self, slug: &str) -> Self {
        let mut cloned = self.clone();
        cloned.completed.insert(slug.to_owned());
        cloned
    }

    /// Returns true if at least one file lies under `path/`.
    pub fn is_dir(&self, path: &str) -> bool {
        let prefix = if path.ends_with('/') {
            path.to_owned()
        } else {
            format!("{path}/")
        };
        self.files
            .range(prefix.clone()..)
            .next()
            .is_some_and(|f| f.starts_with(&prefix))
    }

    /// Returns true if path is a file, a directory, or the empty root.
    pub fn exists(&self, path: &str) -> bool {
        if path.is_empty() || path == "." {
            return true;
        }
        if path.ends_with('/') {
            let p = path.trim_end_matches('/');
            p.is_empty() || self.is_dir(p)
        } else {
            self.files.contains(path) || self.is_dir(path)
        }
    }

    /// Iterate over every Markdown file and its contents in path order.
    pub fn markdown(&self) -> impl Iterator<Item = (&str, &str)> {
        self.text
            .iter()
            .filter(|(p, _)| p.ends_with(".md"))
            .map(|(p, t)| (p.as_str(), t.as_str()))
    }
}

/// A path declared in the Scope section: a concrete file or a directory.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScopePath {
    /// A single file path.
    File(String),
    /// A directory path without a trailing slash.
    Dir(String),
}

/// Expand shell-style `{a,b}` brace alternatives in a path pattern in order.
pub fn expand_braces(token: &str) -> Result<Vec<String>, String> {
    let mut depth: usize = 0;
    for c in token.chars() {
        match c {
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return Err(format!("unbalanced '}}' in `{token}`"));
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(format!("unbalanced '{{' in `{token}`"));
    }

    let Some(open_idx) = token.find('{') else {
        return Ok(vec![token.to_owned()]);
    };

    let mut brace_depth = 0;
    let mut close_idx = 0;
    for (i, c) in token[open_idx..].char_indices() {
        match c {
            '{' => brace_depth += 1,
            '}' => {
                brace_depth -= 1;
                if brace_depth == 0 {
                    close_idx = open_idx + i;
                    break;
                }
            }
            _ => {}
        }
    }

    let prefix = &token[..open_idx];
    let suffix = &token[close_idx + 1..];
    let inner = &token[open_idx + 1..close_idx];

    let mut alts = Vec::new();
    let mut cur = String::new();
    let mut inner_depth = 0;
    for c in inner.chars() {
        match c {
            '{' => {
                inner_depth += 1;
                cur.push(c);
            }
            '}' => {
                inner_depth -= 1;
                cur.push(c);
            }
            ',' if inner_depth == 0 => {
                alts.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    alts.push(cur);

    let mut results = Vec::new();
    for alt in alts {
        let candidate = format!("{prefix}{alt}{suffix}");
        let expanded = expand_braces(&candidate)?;
        results.extend(expanded);
    }
    Ok(results)
}

fn looks_like_path(span: &str) -> bool {
    if span.contains(' ') {
        return false;
    }
    if span == "justfile" {
        return true;
    }
    if span.contains('/') {
        return true;
    }
    if let Some(dot_pos) = span.rfind('.') {
        let ext = &span[dot_pos + 1..];
        if !ext.is_empty() && ext.len() <= 8 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return true;
        }
    }
    false
}

/// Extract backticked code spans from a Markdown bullet line that look like file/dir paths.
pub fn path_tokens(line: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = line.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '`' {
            let mut backtick_count = 1;
            while let Some(&(_, '`')) = chars.peek() {
                chars.next();
                backtick_count += 1;
            }
            let start = i + backtick_count;
            let mut end = None;
            while let Some((j, c2)) = chars.next() {
                if c2 == '`' {
                    let mut close_count = 1;
                    while let Some(&(_, '`')) = chars.peek() {
                        chars.next();
                        close_count += 1;
                    }
                    if close_count == backtick_count {
                        end = Some(j);
                        break;
                    }
                }
            }
            if let Some(end_idx) = end {
                let span = &line[start..end_idx];
                if looks_like_path(span) {
                    tokens.push(span.to_owned());
                }
            }
        }
    }
    tokens
}

/// Parse the Scope `Files:` line into scope paths and rejection messages.
pub fn parse_scope(line: &str) -> (Vec<ScopePath>, Vec<String>) {
    let tokens = path_tokens(line);
    let mut paths = Vec::new();
    let mut errors = Vec::new();

    for token in tokens {
        if token.chars().any(|c| matches!(c, '*' | '?' | '[' | '<')) {
            errors.push(format!(
                "unsupported wildcard or placeholder in Scope path `{token}`"
            ));
            continue;
        }
        match expand_braces(&token) {
            Ok(expanded) => {
                for p in expanded {
                    if p.ends_with('/') {
                        paths.push(ScopePath::Dir(p.trim_end_matches('/').to_owned()));
                    } else {
                        paths.push(ScopePath::File(p));
                    }
                }
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    (paths, errors)
}

/// Returns true if the scope path covers the given concrete repository path.
pub fn covers(scope: &ScopePath, path: &str) -> bool {
    let clean_path = path.trim_end_matches('/');
    match scope {
        ScopePath::File(f) => f == clean_path,
        ScopePath::Dir(d) => {
            if d.is_empty() {
                return true;
            }
            clean_path == d || clean_path.starts_with(&format!("{d}/"))
        }
    }
}

/// If two scope paths overlap, returns the shared (deeper) path.
pub fn overlap(a: &ScopePath, b: &ScopePath) -> Option<String> {
    match (a, b) {
        (ScopePath::File(fa), ScopePath::File(fb)) => {
            if fa == fb {
                Some(fa.clone())
            } else {
                None
            }
        }
        (ScopePath::File(f), ScopePath::Dir(d)) | (ScopePath::Dir(d), ScopePath::File(f)) => {
            if covers(&ScopePath::Dir(d.clone()), f) {
                Some(f.clone())
            } else {
                None
            }
        }
        (ScopePath::Dir(da), ScopePath::Dir(db)) => {
            if da == db {
                Some(da.clone())
            } else if covers(&ScopePath::Dir(da.clone()), db) {
                Some(db.clone())
            } else if covers(&ScopePath::Dir(db.clone()), da) {
                Some(da.clone())
            } else {
                None
            }
        }
    }
}

/// An open or draft card file.
#[derive(Debug, Clone)]
pub struct Card {
    /// The card slug (file stem).
    pub slug: String,
    /// Repository-relative path to the card file.
    pub path: String,
    /// True if the card is in `docs/cards/drafts/`.
    pub draft: bool,
    /// Full UTF-8 text of the card.
    pub text: String,
}

impl Card {
    /// Returns 1-based line of the heading and `(1-based line, line text)` for the section body.
    pub fn section_lines(&self, heading: &str) -> Option<(usize, Vec<(usize, &str)>)> {
        let target = heading.strip_prefix("## ").unwrap_or(heading).trim();
        let mut fence_len: Option<usize> = None;
        let mut heading_line = None;
        let mut body = Vec::new();

        for (line_idx, line) in self.text.lines().enumerate() {
            let line_num = line_idx + 1;
            let trimmed = line.trim();

            if let Some(count) = fence_len {
                if trimmed.len() >= count && trimmed.chars().all(|c| c == '`') {
                    fence_len = None;
                }
            } else if trimmed.starts_with("```") {
                let count = trimmed.chars().take_while(|&c| c == '`').count();
                fence_len = Some(count);
            }

            if heading_line.is_none() {
                if fence_len.is_none() && trimmed.starts_with("## ") {
                    let h_name = trimmed.strip_prefix("## ").unwrap_or("").trim();
                    if h_name == target {
                        heading_line = Some(line_num);
                    }
                }
            } else {
                if fence_len.is_none() && (trimmed.starts_with("## ") || trimmed.starts_with("# "))
                {
                    break;
                }
                body.push((line_num, line));
            }
        }

        heading_line.map(|line| (line, body))
    }

    /// Returns the 1-based line number and full text of the bullet starting with `- <label>:`.
    pub fn bullet(&self, section: &str, label: &str) -> Option<(usize, &str)> {
        let (_, body) = self.section_lines(section)?;
        let prefix = format!("- {label}:");

        let mut bullet_idx = None;
        for (i, (_, line)) in body.iter().enumerate() {
            if line.trim_start().starts_with(&prefix) {
                bullet_idx = Some(i);
                break;
            }
        }
        let start_idx = bullet_idx?;
        let start_line_num = body[start_idx].0;

        let mut end_idx = start_idx;
        for (i, (_, line)) in body.iter().enumerate().skip(start_idx + 1) {
            let trimmed = line.trim_start();
            if trimmed.starts_with("- ") || trimmed.starts_with('#') {
                break;
            }
            if line.starts_with(' ') || line.starts_with('\t') {
                end_idx = i;
            } else {
                break;
            }
        }

        let first_str = body[start_idx].1;
        let last_str = body[end_idx].1;

        let start_offset = first_str.as_ptr() as usize - self.text.as_ptr() as usize;
        let end_offset =
            (last_str.as_ptr() as usize + last_str.len()) - self.text.as_ptr() as usize;
        Some((start_line_num, &self.text[start_offset..end_offset]))
    }

    /// Extract backticked slugs from the `Cards merged:` bullet in Preconditions present in `known`.
    pub fn prerequisites(&self, known: &BTreeSet<String>) -> Vec<String> {
        let Some((_, bullet_text)) = self.bullet("Preconditions", "Cards merged") else {
            return Vec::new();
        };
        let mut slugs = Vec::new();
        for span in extract_code_spans(bullet_text) {
            if known.contains(&span) && !slugs.contains(&span) {
                slugs.push(span);
            }
        }
        slugs
    }
}

/// Collect all open and draft cards from the repository snapshot.
pub fn cards(snap: &Snapshot) -> Vec<Card> {
    let mut result = Vec::new();
    for (path, text) in snap.markdown() {
        if path == "docs/cards/README.md" {
            continue;
        }
        if let Some(rest) = path.strip_prefix("docs/cards/drafts/")
            && let Some(slug) = rest.strip_suffix(".md")
            && !slug.contains('/')
        {
            result.push(Card {
                slug: slug.to_owned(),
                path: path.to_owned(),
                draft: true,
                text: text.to_owned(),
            });
        } else if let Some(rest) = path.strip_prefix("docs/cards/")
            && let Some(slug) = rest.strip_suffix(".md")
            && !slug.contains('/')
        {
            result.push(Card {
                slug: slug.to_owned(),
                path: path.to_owned(),
                draft: false,
                text: text.to_owned(),
            });
        }
    }
    result
}

/// Card template definition read from `docs/cards/README.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    /// Section names in order (including `## ` prefix).
    pub sections: Vec<String>,
    /// The three header field labels: Runs where, Schema impact, Review.
    pub header: Vec<String>,
    /// Allowed values for Runs where.
    pub runs_where: Vec<String>,
    /// Allowed values for Schema impact.
    pub schema_impact: Vec<String>,
    /// Allowed values for Review.
    pub review: Vec<String>,
}

fn extract_code_spans(text: &str) -> Vec<String> {
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if c == '`' {
            let mut count = 1;
            while let Some(&(_, '`')) = chars.peek() {
                chars.next();
                count += 1;
            }
            let start = i + count;
            let mut end = None;
            while let Some((j, c2)) = chars.next() {
                if c2 == '`' {
                    let mut close_count = 1;
                    while let Some(&(_, '`')) = chars.peek() {
                        chars.next();
                        close_count += 1;
                    }
                    if close_count == count {
                        end = Some(j);
                        break;
                    }
                }
            }
            if let Some(end_idx) = end {
                spans.push(text[start..end_idx].trim().to_owned());
            }
        }
    }
    spans
}

/// Read the card template and allowed header values from `docs/cards/README.md`.
pub fn read_template(snap: &Snapshot) -> Result<Template, String> {
    let readme = snap
        .text
        .get("docs/cards/README.md")
        .ok_or_else(|| "missing docs/cards/README.md in snapshot".to_owned())?;

    let begin_marker = "<!-- card-template:begin -->";
    let end_marker = "<!-- card-template:end -->";
    let Some(begin_pos) = readme.find(begin_marker) else {
        return Err("missing `<!-- card-template:begin -->` in docs/cards/README.md".to_owned());
    };
    let after_begin = &readme[begin_pos + begin_marker.len()..];
    let Some(end_pos) = after_begin.find(end_marker) else {
        return Err("missing `<!-- card-template:end -->` in docs/cards/README.md".to_owned());
    };
    let block = &after_begin[..end_pos];

    let mut outer_fence_len: Option<usize> = None;
    let mut template_lines = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim();
        match outer_fence_len {
            None => {
                if trimmed.starts_with("```") {
                    let count = trimmed.chars().take_while(|&c| c == '`').count();
                    outer_fence_len = Some(count);
                }
            }
            Some(count) => {
                let is_closing = trimmed.len() >= count && trimmed.chars().all(|c| c == '`');
                if is_closing {
                    break;
                }
                template_lines.push(line);
            }
        }
    }

    let mut sections = Vec::new();
    let mut header = Vec::new();
    let mut in_inner_fence = false;

    for line in template_lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_inner_fence = !in_inner_fence;
            continue;
        }
        if !in_inner_fence {
            if trimmed.starts_with("## ") {
                sections.push(trimmed.to_owned());
            } else if let Some(rest) = trimmed.strip_prefix("- **")
                && let Some((label, _)) = rest.split_once(":**")
            {
                header.push(label.trim().to_owned());
            }
        }
    }

    let mut runs_where = Vec::new();
    let mut schema_impact = Vec::new();
    let mut review = Vec::new();

    let heading_pos = readme
        .find("## The three header fields")
        .ok_or_else(|| "missing `## The three header fields` in README.md".to_owned())?;
    let after_fields = &readme[heading_pos..];
    let end_table_pos = after_fields.find("\n## ").unwrap_or(after_fields.len());
    let table_text = &after_fields[..end_table_pos];

    for line in table_text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed.split('|').map(str::trim).collect();
        if cells.len() < 3 {
            continue;
        }
        let field_name = cells[1];
        let values_cell = cells[2];
        let spans = extract_code_spans(values_cell);
        match field_name {
            "Runs where" => runs_where = spans,
            "Schema impact" => schema_impact = spans,
            "Review" => review = spans,
            _ => {}
        }
    }

    if sections.is_empty()
        || header.is_empty()
        || runs_where.is_empty()
        || schema_impact.is_empty()
        || review.is_empty()
    {
        return Err("incomplete card template in docs/cards/README.md".to_owned());
    }

    Ok(Template {
        sections,
        header,
        runs_where,
        schema_impact,
        review,
    })
}

fn is_valid_slug(slug: &str) -> bool {
    if slug.is_empty() {
        return false;
    }
    for word in slug.split('-') {
        if word.is_empty() {
            return false;
        }
        if !word
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        {
            return false;
        }
    }
    true
}

/// Render a new card file from the template in `docs/cards/README.md`.
pub fn render_new_card(snap: &Snapshot, slug: &str) -> Result<String, String> {
    if !is_valid_slug(slug) {
        return Err(format!(
            "invalid card slug `{slug}`: use lowercase words joined by hyphens"
        ));
    }
    let target = format!("docs/cards/{slug}.md");
    let draft = format!("docs/cards/drafts/{slug}.md");
    if snap.exists(&target) || snap.exists(&draft) {
        return Err(format!("card `{slug}` already exists"));
    }

    let readme = snap
        .text
        .get("docs/cards/README.md")
        .ok_or_else(|| "missing docs/cards/README.md in snapshot".to_owned())?;
    let begin_marker = "<!-- card-template:begin -->";
    let end_marker = "<!-- card-template:end -->";
    let Some(begin_pos) = readme.find(begin_marker) else {
        return Err("missing `<!-- card-template:begin -->` in docs/cards/README.md".to_owned());
    };
    let after_begin = &readme[begin_pos + begin_marker.len()..];
    let Some(end_pos) = after_begin.find(end_marker) else {
        return Err("missing `<!-- card-template:end -->` in docs/cards/README.md".to_owned());
    };
    let block = &after_begin[..end_pos];

    let mut outer_fence_len: Option<usize> = None;
    let mut template_lines = Vec::new();
    for line in block.lines() {
        let trimmed = line.trim();
        match outer_fence_len {
            None => {
                if trimmed.starts_with("```") {
                    let count = trimmed.chars().take_while(|&c| c == '`').count();
                    outer_fence_len = Some(count);
                }
            }
            Some(count) => {
                let is_closing = trimmed.len() >= count && trimmed.chars().all(|c| c == '`');
                if is_closing {
                    break;
                }
                template_lines.push(line);
            }
        }
    }

    let mut rendered = Vec::new();
    for (i, line) in template_lines.into_iter().enumerate() {
        if i == 0 && line.trim() == "# <slug>" {
            rendered.push(format!("# {slug}"));
        } else {
            rendered.push(line.to_owned());
        }
    }

    let mut out = rendered.join("\n");
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// Lint all `cargo nextest run` commands in a card against the nextest filter rules.
pub fn lint_nextest(card: &Card) -> Vec<Finding> {
    let mut findings = Vec::new();
    let standalone_spans: BTreeSet<String> = extract_code_spans(&card.text).into_iter().collect();

    let mut fence_len: Option<usize> = None;
    for (line_idx, line) in card.text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();
        if let Some(count) = fence_len {
            if trimmed.len() >= count && trimmed.chars().all(|c| c == '`') {
                fence_len = None;
                continue;
            }
            if trimmed.starts_with("Expected:") {
                continue;
            }
            let stripped = trimmed.strip_prefix("$ ").unwrap_or(trimmed).trim_start();
            if stripped.starts_with("cargo nextest run") {
                lint_nextest_command(
                    &card.path,
                    line_num,
                    stripped,
                    &standalone_spans,
                    &mut findings,
                );
            }
        } else if trimmed.starts_with("```") {
            let count = trimmed.chars().take_while(|&c| c == '`').count();
            fence_len = Some(count);
        } else {
            let spans = extract_code_spans(line);
            for span in spans {
                let s = span.trim();
                let stripped = s.strip_prefix("$ ").unwrap_or(s).trim_start();
                if stripped.starts_with("cargo nextest run") {
                    lint_nextest_command(
                        &card.path,
                        line_num,
                        stripped,
                        &standalone_spans,
                        &mut findings,
                    );
                }
            }
        }
    }

    findings
}

fn lint_nextest_command(
    file: &str,
    line: usize,
    cmd: &str,
    standalone_spans: &BTreeSet<String>,
    findings: &mut Vec<Finding>,
) {
    let after = cmd["cargo nextest run".len()..].trim_start();
    let tokens: Vec<&str> = after.split_whitespace().collect();

    let mut idx = 0;
    while idx < tokens.len() {
        let token = tokens[idx];
        if matches!(token, "|" | "||" | "&&" | ";" | ">" | ">>" | "<")
            || token.starts_with(';')
            || token.starts_with('|')
            || token.starts_with('&')
            || token.starts_with('>')
        {
            break;
        }

        if token.starts_with('-') {
            if token.starts_with("--features=")
                || token.starts_with("--package=")
                || token.starts_with("--exclude=")
                || token.starts_with("--test=")
                || token.starts_with("--run-ignored=")
            {
                idx += 1;
                continue;
            }
            match token {
                "--locked" | "--workspace" | "--all-features" | "--no-default-features" => {
                    idx += 1;
                }
                "-p" | "--package" | "--features" | "--exclude" | "--test" | "--run-ignored" => {
                    idx += 1;
                    if idx < tokens.len()
                        && !matches!(tokens[idx], "|" | "||" | "&&" | ";" | ">" | ">>" | "<")
                    {
                        idx += 1;
                    }
                }
                _ => {
                    findings.push(Finding::new(
                        file,
                        line,
                        format!("unknown nextest flag `{token}`"),
                    ));
                    idx += 1;
                }
            }
        } else {
            let word = token.trim_matches(['"', '\'', '`']);
            if word.contains("::") {
                idx += 1;
            } else {
                let underscore_count = word.chars().filter(|&c| c == '_').count();
                let is_code_span = standalone_spans.contains(word);
                if underscore_count >= 2 && is_code_span {
                    idx += 1;
                } else {
                    findings.push(Finding::new(
                        file,
                        line,
                        format!(
                            "invalid nextest filter `{word}`: not a module path or full test name"
                        ),
                    ));
                    idx += 1;
                }
            }
        }
    }
}

/// Lint all open and draft cards against template, header, section, Acceptance, and nextest rules.
pub fn lint_cards(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let template = match read_template(snap) {
        Ok(t) => t,
        Err(e) => {
            findings.push(Finding::new("docs/cards/README.md", 1, e));
            return findings;
        }
    };

    let all_cards = cards(snap);
    for card in all_cards {
        // 1. Title is `# <slug>`
        let first_non_empty = card
            .text
            .lines()
            .enumerate()
            .find(|(_, l)| !l.trim().is_empty());
        let expected_title = format!("# {}", card.slug);
        let title_line = match first_non_empty {
            Some((line_idx, line)) => {
                let line_num = line_idx + 1;
                if line.trim() != expected_title {
                    findings.push(Finding::new(
                        &card.path,
                        line_num,
                        format!("title is `{}`; expected `{expected_title}`", line.trim()),
                    ));
                }
                line_num
            }
            None => {
                findings.push(Finding::new(
                    &card.path,
                    1,
                    format!("empty card file; expected `{expected_title}`"),
                ));
                1
            }
        };

        // 2. The three header lines in template order with allowed values
        let mut header_entries = Vec::new();
        for (line_idx, line) in card.text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("## ") {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("- **")
                && let Some((label, val)) = rest.split_once(":**")
            {
                header_entries.push((line_idx + 1, label.trim().to_owned(), val.trim().to_owned()));
            }
        }

        let mut schema_impact_val: Option<(usize, String)> = None;
        let mut review_val: Option<(usize, String)> = None;

        let mut prev_expected_idx = None;
        for (line_num, label, val) in &header_entries {
            match template.header.iter().position(|h| h == label) {
                Some(idx) => {
                    if let Some(prev) = prev_expected_idx
                        && idx < prev
                    {
                        findings.push(Finding::new(
                            &card.path,
                            *line_num,
                            format!("header `{label}` is out of template order"),
                        ));
                    }
                    prev_expected_idx = Some(idx);
                }
                None => {
                    findings.push(Finding::new(
                        &card.path,
                        *line_num,
                        format!("unknown header field `{label}`"),
                    ));
                }
            }

            match label.as_str() {
                "Runs where" => {
                    for item in val.split(',').map(str::trim) {
                        if !item.is_empty()
                            && !template.runs_where.iter().any(|allowed| allowed == item)
                        {
                            findings.push(Finding::new(
                                &card.path,
                                *line_num,
                                format!("invalid Runs where value `{item}`"),
                            ));
                        }
                    }
                }
                "Schema impact" => {
                    schema_impact_val = Some((*line_num, val.clone()));
                    if !template.schema_impact.iter().any(|allowed| allowed == val) {
                        findings.push(Finding::new(
                            &card.path,
                            *line_num,
                            format!("invalid Schema impact value `{val}`"),
                        ));
                    }
                }
                "Review" => {
                    review_val = Some((*line_num, val.clone()));
                    if !template.review.iter().any(|allowed| allowed == val) {
                        findings.push(Finding::new(
                            &card.path,
                            *line_num,
                            format!("invalid Review value `{val}`"),
                        ));
                    }
                }
                _ => {}
            }
        }

        for expected_header in &template.header {
            if !header_entries
                .iter()
                .any(|(_, label, _)| label == expected_header)
            {
                findings.push(Finding::new(
                    &card.path,
                    title_line,
                    format!("missing header field `{expected_header}`"),
                ));
            }
        }

        if let (Some((_, schema)), Some((rev_line, rev))) = (&schema_impact_val, &review_val)
            && schema != "none"
            && rev != "owner"
        {
            findings.push(Finding::new(
                &card.path,
                *rev_line,
                "Schema impact other than none requires Review: owner",
            ));
        }

        // 3. The ten `##` sections in template order, each exactly once
        let mut sections_found = Vec::new();
        let mut fence_len: Option<usize> = None;
        for (line_idx, line) in card.text.lines().enumerate() {
            let line_num = line_idx + 1;
            let trimmed = line.trim();
            if let Some(count) = fence_len {
                if trimmed.len() >= count && trimmed.chars().all(|c| c == '`') {
                    fence_len = None;
                }
            } else if trimmed.starts_with("```") {
                let count = trimmed.chars().take_while(|&c| c == '`').count();
                fence_len = Some(count);
            } else if trimmed.starts_with("## ") {
                sections_found.push((line_num, trimmed.to_owned()));
            }
        }

        for (line_num, sec) in &sections_found {
            if sec == "## Evidence" {
                findings.push(Finding::new(
                    &card.path,
                    *line_num,
                    "an `## Evidence` section is an error",
                ));
            }
        }

        let mut seen_sections = BTreeSet::new();
        let mut prev_sec_idx = None;
        for (line_num, sec) in &sections_found {
            if sec == "## Evidence" {
                continue;
            }
            if !seen_sections.insert(sec.clone()) {
                findings.push(Finding::new(
                    &card.path,
                    *line_num,
                    format!("duplicate section {sec}"),
                ));
                continue;
            }
            match template.sections.iter().position(|s| s == sec) {
                Some(idx) => {
                    if let Some(prev) = prev_sec_idx
                        && idx < prev
                    {
                        findings.push(Finding::new(
                            &card.path,
                            *line_num,
                            format!("out-of-order section {sec}"),
                        ));
                    }
                    prev_sec_idx = Some(idx);
                }
                None => {
                    findings.push(Finding::new(
                        &card.path,
                        *line_num,
                        format!("unknown section {sec}"),
                    ));
                }
            }
        }

        for expected_sec in &template.sections {
            if !sections_found.iter().any(|(_, sec)| sec == expected_sec) {
                findings.push(Finding::new(
                    &card.path,
                    title_line,
                    format!("missing section {expected_sec}"),
                ));
            }
        }

        // 4. Last command of Acceptance is `just check`
        if let Some((acc_line, body)) = card.section_lines("Acceptance") {
            let mut in_block = false;
            let mut block_lines = Vec::new();
            for (line_num, line) in body {
                let trimmed = line.trim();
                if trimmed.starts_with("```") {
                    if !in_block {
                        in_block = true;
                    } else {
                        break;
                    }
                } else if in_block {
                    block_lines.push((line_num, line));
                }
            }
            let last_cmd = block_lines
                .into_iter()
                .rev()
                .find(|(_, l)| !l.trim().is_empty());
            match last_cmd {
                Some((cmd_line, cmd_str)) => {
                    let stripped = cmd_str
                        .trim()
                        .strip_prefix("$ ")
                        .unwrap_or(cmd_str.trim())
                        .trim();
                    if stripped != "just check" {
                        findings.push(Finding::new(
                            &card.path,
                            cmd_line,
                            format!("last command of Acceptance code block is `{stripped}`; expected `just check`"),
                        ));
                    }
                }
                None => {
                    findings.push(Finding::new(
                        &card.path,
                        acc_line,
                        "missing command block in Acceptance section; expected `just check`",
                    ));
                }
            }
        }

        // 5. Nextest filter rules
        findings.extend(lint_nextest(&card));
    }

    findings
}

/// Start `docs/cards/<slug>.md` from the template in `docs/cards/README.md`.
pub fn new_card(root: &Path, slug: &str) -> Result<bool, String> {
    if !is_valid_slug(slug) {
        eprintln!("invalid card slug `{slug}`: use lowercase words joined by hyphens");
        return Ok(false);
    }
    let target = root.join("docs/cards").join(format!("{slug}.md"));
    let draft = root.join("docs/cards/drafts").join(format!("{slug}.md"));
    if target.exists() || draft.exists() {
        eprintln!("card `{slug}` already exists");
        return Ok(false);
    }
    let snap = Snapshot::load(root)?;
    let content = match render_new_card(&snap, slug) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return Ok(false);
        }
    };
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create dir {}: {e}", parent.display()))?;
    }
    std::fs::write(&target, content)
        .map_err(|e| format!("cannot write {}: {e}", target.display()))?;
    println!("docs/cards/{slug}.md");
    Ok(true)
}

/// An item declared in `docs/plan/PLAN.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    /// The card or work item slug.
    pub slug: String,
    /// 1-based line number in `docs/plan/PLAN.md`.
    pub line: usize,
    /// The phase name (first word of the enclosing `## ` heading).
    pub phase: String,
}

/// Extract all plan items from `docs/plan/PLAN.md` in the snapshot.
pub fn plan_items(snap: &Snapshot) -> Result<Vec<PlanItem>, String> {
    let plan_text = snap
        .text
        .get("docs/plan/PLAN.md")
        .ok_or_else(|| "missing docs/plan/PLAN.md in snapshot".to_owned())?;

    let mut items = Vec::new();
    let mut current_phase: Option<String> = None;

    for (line_idx, line) in plan_text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if let Some(stripped) = trimmed.strip_prefix("## ") {
            let heading = stripped.trim();
            if let Some(first_word) = heading.split_whitespace().next() {
                current_phase = Some(first_word.to_owned());
            }
            continue;
        }

        if current_phase.is_none() {
            continue;
        }

        if trimmed.ends_with("(owner)") || trimmed.ends_with("(lead)") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ")
            && let Some((candidate_slug, _)) = rest.split_once(": ")
            && is_valid_slug(candidate_slug)
            && let Some(ref phase) = current_phase
        {
            items.push(PlanItem {
                slug: candidate_slug.to_owned(),
                line: line_num,
                phase: phase.clone(),
            });
        }
    }

    Ok(items)
}

const PHASE_SECTIONS: &[&str] = &[
    "F0", "P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8", "Hosted", "Reader",
];

fn is_phase_section(phase: &str) -> bool {
    PHASE_SECTIONS.contains(&phase)
}

/// Slugs that count as merged although no commit on `main` deletes their card file.
/// `workspace-skeleton` was built before card files existed. The other six were closed by
/// squash-merged pull requests whose commit on `main` holds no deletion of the card file.
/// This list is closed: a new card is proved by its deleting commit, never added here.
pub const LEGACY_MERGED: &[&str] = &[
    "workspace-skeleton",
    "hook-environment-isolation",
    "product-first-delivery",
    "kb-foundation-guard",
    "search-page-view",
    "search-index-budget",
    "hosted-edition-adr",
];

/// True when the card `slug` has merged: it is a legacy slug, or its card file and its
/// draft are absent and a commit on `main` deleted the file.
pub fn is_merged(snap: &Snapshot, slug: &str) -> bool {
    LEGACY_MERGED.contains(&slug)
        || (!snap.files.contains(&format!("docs/cards/{slug}.md"))
            && !snap.files.contains(&format!("docs/cards/drafts/{slug}.md"))
            && snap.completed.contains(slug))
}

fn is_strict_phase(phase: &str) -> bool {
    matches!(phase, "F0" | "P1" | "P2" | "P3")
}

/// Lint `docs/plan/PLAN.md` and check consistency with card files and completion history.
pub fn lint_plan(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let Some(plan_text) = snap.text.get("docs/plan/PLAN.md") else {
        findings.push(Finding::new(
            "docs/plan/PLAN.md",
            1,
            "missing docs/plan/PLAN.md in snapshot",
        ));
        return findings;
    };

    let items = match plan_items(snap) {
        Ok(it) => it,
        Err(e) => {
            findings.push(Finding::new("docs/plan/PLAN.md", 1, e));
            return findings;
        }
    };

    // 1. Lint list lines in phase sections
    let mut current_phase: Option<String> = None;
    for (line_idx, line) in plan_text.lines().enumerate() {
        let line_num = line_idx + 1;
        let trimmed = line.trim();

        if let Some(stripped) = trimmed.strip_prefix("## ") {
            let heading = stripped.trim();
            if let Some(first_word) = heading.split_whitespace().next() {
                current_phase = Some(first_word.to_owned());
            }
            continue;
        }

        let Some(ref phase) = current_phase else {
            continue;
        };

        if !is_phase_section(phase) {
            continue;
        }

        let is_list_line = trimmed.starts_with("- ")
            || trimmed.starts_with("-\t")
            || trimmed.starts_with("- [")
            || trimmed.starts_with("-[");

        if !is_list_line {
            continue;
        }

        let starts_with_status_bracket = trimmed.starts_with("- [ ]")
            || trimmed.starts_with("-[ ]")
            || trimmed.starts_with("- [x]")
            || trimmed.starts_with("-[x]")
            || trimmed.starts_with("- [!]")
            || trimmed.starts_with("-[!]");

        if starts_with_status_bracket || trimmed.contains("-> pass") {
            findings.push(Finding::new(
                "docs/plan/PLAN.md",
                line_num,
                format!("status mark or checklist item `{trimmed}` in phase section is forbidden"),
            ));
        } else if trimmed.ends_with("(owner)") || trimmed.ends_with("(lead)") {
            // Exempt
        } else {
            let is_item = if let Some(rest) = trimmed.strip_prefix("- ") {
                if let Some((candidate_slug, _)) = rest.split_once(": ") {
                    is_valid_slug(candidate_slug)
                } else {
                    false
                }
            } else {
                false
            };

            if !is_item {
                findings.push(Finding::new(
                    "docs/plan/PLAN.md",
                    line_num,
                    format!("free list line `{trimmed}` in phase section is not an item and does not end `(owner)` or `(lead)`"),
                ));
            }
        }
    }

    // 2. Check for duplicate slugs in PLAN.md
    let mut seen_slugs: BTreeMap<&str, usize> = BTreeMap::new();
    for item in &items {
        if let Some(&first_line) = seen_slugs.get(item.slug.as_str()) {
            findings.push(Finding::new(
                "docs/plan/PLAN.md",
                item.line,
                format!(
                    "duplicate slug `{}` in docs/plan/PLAN.md (lines {} and {})",
                    item.slug, first_line, item.line
                ),
            ));
        } else {
            seen_slugs.insert(item.slug.as_str(), item.line);
        }
    }

    // 3. Check phase requirements for each item
    for item in &items {
        if item.slug.starts_with("later-") && !matches!(item.phase.as_str(), "P6" | "P7" | "P8") {
            findings.push(Finding::new(
                "docs/plan/PLAN.md",
                item.line,
                format!(
                    "`{}` is not permitted under phase {}: later- items are only accepted under P6, P7 and P8",
                    item.slug, item.phase
                ),
            ));
            continue;
        }

        if is_strict_phase(&item.phase) && !LEGACY_MERGED.contains(&item.slug.as_str()) {
            let card_path = format!("docs/cards/{}.md", item.slug);
            let card_exists = snap.files.contains(&card_path);
            let is_completed = snap.completed.contains(&item.slug);
            if !card_exists && !is_completed {
                findings.push(Finding::new(
                    "docs/plan/PLAN.md",
                    item.line,
                    format!(
                        "item `{}` in strict phase {} needs a card file docs/cards/{}.md or completion in git history",
                        item.slug, item.phase, item.slug
                    ),
                ));
            }
        }
    }

    // 4. Every open card and draft needs a PLAN.md item
    let plan_slug_set: BTreeSet<&str> = items.iter().map(|it| it.slug.as_str()).collect();
    for card in cards(snap) {
        if !plan_slug_set.contains(card.slug.as_str()) {
            findings.push(Finding::new(
                &card.path,
                1,
                format!("card has no item in docs/plan/PLAN.md: {}", card.path),
            ));
        }
    }

    findings
}

fn reaches(graph: &BTreeMap<String, Vec<String>>, from: &str, target: &str) -> bool {
    let mut visited = BTreeSet::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(from);
    visited.insert(from);
    while let Some(curr) = queue.pop_front() {
        if let Some(neighbors) = graph.get(curr) {
            for next in neighbors {
                if next == target {
                    return true;
                }
                if visited.insert(next.as_str()) {
                    queue.push_back(next.as_str());
                }
            }
        }
    }
    false
}

/// Lint cards for scope overlaps with no prerequisite path between them.
pub fn lint_overlap(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let all_cards = cards(snap);

    let mut card_scopes = Vec::new();
    for card in &all_cards {
        if let Some((files_line, bullet_text)) = card.bullet("Scope", "Files") {
            let (paths, errors) = parse_scope(bullet_text);
            for err in errors {
                findings.push(Finding::new(&card.path, files_line, err));
            }
            card_scopes.push((card, files_line, paths));
        }
    }

    card_scopes.sort_by(|a, b| a.0.path.cmp(&b.0.path));
    let graph = prerequisite_graph(snap);

    for i in 0..card_scopes.len() {
        for j in (i + 1)..card_scopes.len() {
            let (card_a, files_line_a, paths_a) = &card_scopes[i];
            let (card_b, _, paths_b) = &card_scopes[j];

            let mut overlaps = BTreeSet::new();
            for pa in paths_a {
                for pb in paths_b {
                    if let Some(shared) = overlap(pa, pb) {
                        overlaps.insert(shared);
                    }
                }
            }

            if overlaps.is_empty() {
                continue;
            }

            if reaches(&graph, &card_a.slug, &card_b.slug)
                || reaches(&graph, &card_b.slug, &card_a.slug)
            {
                continue;
            }

            if overlaps.iter().all(|p| is_shared_path(p)) {
                continue;
            }

            let shared_path = overlaps
                .iter()
                .find(|p| !is_shared_path(p))
                .or_else(|| overlaps.iter().next())
                .map(String::as_str)
                .unwrap_or("");

            findings.push(Finding::new(
                &card_a.path,
                *files_line_a,
                format!(
                    "Scope overlap with no precondition path: {}.md and {}.md share {}",
                    card_a.slug, card_b.slug, shared_path
                ),
            ));
        }
    }

    findings
}

/// Build the prerequisite graph mapping each card slug to the slugs named in its `Cards merged:` bullet.
pub fn prerequisite_graph(snap: &Snapshot) -> BTreeMap<String, Vec<String>> {
    let mut known = BTreeSet::new();
    if let Ok(items) = plan_items(snap) {
        for it in items {
            known.insert(it.slug);
        }
    }
    let all_cards = cards(snap);
    for card in &all_cards {
        known.insert(card.slug.clone());
    }

    let mut graph = BTreeMap::new();
    for card in &all_cards {
        let prereqs = card.prerequisites(&known);
        graph.insert(card.slug.clone(), prereqs);
    }
    graph
}

fn dfs_cycles(
    graph: &BTreeMap<String, Vec<String>>,
    start: &str,
    curr: &str,
    path: &mut Vec<String>,
    cycles: &mut Vec<Vec<String>>,
) {
    if let Some(neighbors) = graph.get(curr) {
        for next in neighbors {
            if next == start {
                let mut cycle = path.clone();
                cycle.push(next.clone());
                cycles.push(cycle);
            } else if next.as_str() > start && !path.iter().any(|node| node == next) {
                path.push(next.clone());
                dfs_cycles(graph, start, next, path, cycles);
                path.pop();
            }
        }
    }
}

/// Lint cards for prerequisite cycles and ordering against `docs/plan/PLAN.md`.
pub fn lint_prerequisites(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    let graph = prerequisite_graph(snap);
    let all_cards = cards(snap);
    let card_map: BTreeMap<String, &Card> = all_cards.iter().map(|c| (c.slug.clone(), c)).collect();

    // 1. Cycle detection
    let mut cycles = Vec::new();
    for start in graph.keys() {
        let mut path = vec![start.clone()];
        dfs_cycles(&graph, start, start, &mut path, &mut cycles);
    }

    for cycle in cycles {
        let first_slug = &cycle[0];
        let card = card_map.get(first_slug.as_str());
        let line = card
            .and_then(|c| c.bullet("Preconditions", "Cards merged"))
            .map(|(l, _)| l)
            .unwrap_or(1);
        let file = card.map(|c| c.path.as_str()).unwrap_or("docs/cards");
        findings.push(Finding::new(
            file,
            line,
            format!("prerequisite cycle: {}", cycle.join(" -> ")),
        ));
    }

    // 2. Ordering check against PLAN.md
    if let Ok(items) = plan_items(snap) {
        let mut plan_lines: BTreeMap<String, usize> = BTreeMap::new();
        for it in items {
            plan_lines.entry(it.slug).or_insert(it.line);
        }

        for card in &all_cards {
            if let Some(&consumer_line) = plan_lines.get(&card.slug)
                && let Some(prereqs) = graph.get(&card.slug)
            {
                for prereq in prereqs {
                    if let Some(&prereq_line) = plan_lines.get(prereq)
                        && prereq_line > consumer_line
                    {
                        let line = card
                            .bullet("Preconditions", "Cards merged")
                            .map(|(l, _)| l)
                            .unwrap_or(1);
                        findings.push(Finding::new(
                            &card.path,
                            line,
                            format!(
                                "prerequisite `{prereq}` comes after its consumer `{}` in docs/plan/PLAN.md",
                                card.slug
                            ),
                        ));
                    }
                }
            }
        }
    }

    findings
}

/// Return a new snapshot simulating the closure of a card.
///
/// Removes `docs/cards/<slug>.md` and `docs/cards/drafts/<slug>.md` from files and text,
/// and adds `slug` to `completed`.
pub fn closing_snapshot(snap: &Snapshot, slug: &str) -> Snapshot {
    let mut cloned = snap.clone();
    let card_path = format!("docs/cards/{slug}.md");
    let draft_path = format!("docs/cards/drafts/{slug}.md");
    cloned.files.remove(&card_path);
    cloned.text.remove(&card_path);
    cloned.files.remove(&draft_path);
    cloned.text.remove(&draft_path);
    cloned.completed.insert(slug.to_owned());
    cloned
}

/// Run all card and repository lints over the snapshot.
pub fn lint_all(snap: &Snapshot) -> Vec<Finding> {
    let mut findings = Vec::new();
    findings.extend(lint_cards(snap));
    findings.extend(lint_plan(snap));
    findings.extend(lint_overlap(snap));
    findings.extend(lint_prerequisites(snap));

    findings.extend(crate::links::lint_all(snap));
    findings.extend(crate::docs_lints::lint_all(snap));

    findings.sort();
    findings
}

/// Run all lints over a snapshot, optionally simulating card closure.
pub fn run(snap: &Snapshot, closing: Option<&str>) -> Vec<Finding> {
    let effective = match closing {
        Some(slug) => closing_snapshot(snap, slug),
        None => snap.clone(),
    };
    lint_all(&effective)
}

/// `cargo xtask cards --check [--closing SLUG]`.
pub fn command(root: &Path, args: &[&str]) -> Result<bool, String> {
    let closing = match args {
        ["--check"] => None,
        ["--check", "--closing", slug] => Some(*slug),
        _ => {
            eprintln!("usage: cargo xtask cards --check [--closing SLUG]");
            return Err("bad arguments".to_owned());
        }
    };

    let snap = Snapshot::load(root)?;
    let mut findings = run(&snap, closing);
    findings.extend(crate::docs_lints::generated_in_sync(root));
    findings.sort();

    if findings.is_empty() {
        println!("cards: ok");
        Ok(true)
    } else {
        for finding in &findings {
            eprintln!("{finding}");
        }
        eprintln!("cards: {} finding(s)", findings.len());
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_README: &str = r#"# Cards

## The template

<!-- card-template:begin -->
````markdown
# <slug>

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Outcome description.

## Why

Why description.

## Scope

- Files: `xtask/src/main.rs`.
- Docs to update: `README.md`.

## Out of scope

Nothing.

## Preconditions

- Cards merged: `check-ci-and-docs`.

## Steps

1. First step.

## Acceptance

```sh
just check
```

Expected: clean check.

## Negative control

Negative control description.

## Rollback

Rollback description.

## BLOCKED

Stop when blocked.
````
<!-- card-template:end -->

## The three header fields

| Field | Values | Meaning |
|---|---|---|
| Runs where | `home`, `CI Linux`, `CI Windows`, `owner on laptop` (a comma-separated list) | Every place |
| Schema impact | `none`, `additive`, `breaking` | The effect |
| Review | `auto`, `owner` | Review rule |
"#;

    fn valid_card(slug: &str) -> String {
        format!(
            r#"# {slug}

- **Runs where:** home, CI Linux
- **Schema impact:** none
- **Review:** auto

## Outcome

Outcome text.

## Why

Why text.

## Scope

- Files: `xtask/src/main.rs`.
- Docs to update: `README.md`.

## Out of scope

Nothing.

## Preconditions

- Cards merged: `check-ci-and-docs`.

## Steps

1. Step one.

## Acceptance

```sh
just check
```

Expected: check passes.

## Negative control

Control text.

## Rollback

Rollback text.

## BLOCKED

Blocked text.
"#
        )
    }

    fn test_snapshot(card_slug: &str, card_content: &str) -> Snapshot {
        Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            (&format!("docs/cards/{card_slug}.md"), card_content),
        ])
    }

    #[test]
    fn test_finding_display() {
        let f1 = Finding::new("foo.md", 12, "some issue");
        assert_eq!(f1.to_string(), "foo.md:12: some issue");

        let f0 = Finding::new("bar.md", 0, "file-level issue");
        assert_eq!(f0.to_string(), "bar.md: file-level issue");
    }

    #[test]
    fn test_shared_paths() {
        assert!(is_shared_path("docs/plan/PLAN.md"));
        assert!(is_shared_path("crates/fieldbook/src/lib.rs"));
        assert!(is_shared_path("crates/fieldbook/src/check/mod.rs"));
        assert!(!is_shared_path("crates/fieldbook/src/other.rs"));
    }

    #[test]
    fn test_snapshot_methods() {
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", "readme"),
            ("docs/cards/demo.md", "demo"),
            ("src/lib.rs", "pub fn a() {}"),
        ]);
        assert!(snap.exists("docs/cards/README.md"));
        assert!(snap.exists("docs/cards"));
        assert!(snap.exists("docs/cards/"));
        assert!(snap.exists(""));
        assert!(snap.exists("."));
        assert!(!snap.exists("nonexistent"));
        assert!(snap.is_dir("docs/cards"));
        assert!(!snap.is_dir("docs/cards/README.md"));

        let without = snap.without("docs/cards/demo.md");
        assert!(!without.exists("docs/cards/demo.md"));
        assert!(without.exists("docs/cards/README.md"));

        let with_c = snap.with_completed("demo");
        assert!(with_c.completed.contains("demo"));

        let md_files: Vec<_> = snap.markdown().collect();
        assert_eq!(md_files.len(), 2);
        assert_eq!(md_files[0].0, "docs/cards/README.md");
        assert_eq!(md_files[1].0, "docs/cards/demo.md");
    }

    #[test]
    fn test_brace_expansion_nested_repeated() {
        assert_eq!(
            expand_braces("crates/a/{b.rs,c.rs}").unwrap(),
            vec!["crates/a/b.rs", "crates/a/c.rs"]
        );
        assert_eq!(
            expand_braces("a/{b,{c,d}}").unwrap(),
            vec!["a/b", "a/c", "a/d"]
        );
        assert_eq!(
            expand_braces("a/{b,c}/{d,e}").unwrap(),
            vec!["a/b/d", "a/b/e", "a/c/d", "a/c/e"]
        );
        assert!(expand_braces("a/{b,c").is_err());
        assert!(expand_braces("a/b,c}").is_err());
    }

    #[test]
    fn test_path_tokens_and_prose() {
        let line = "- Files: `crates/fieldbook/src/lib.rs`, `Cargo.lock`, `justfile`, `World`, `read_maintenance`, `test-hooks`.";
        let tokens = path_tokens(line);
        assert_eq!(
            tokens,
            vec!["crates/fieldbook/src/lib.rs", "Cargo.lock", "justfile"]
        );
    }

    #[test]
    fn test_wildcard_and_placeholder_tokens_reported() {
        let line = "- Files: `crates/*`, `docs/<family>/file.rs`, `tests/?[a]`.";
        let (paths, errors) = parse_scope(line);
        assert!(paths.is_empty());
        assert_eq!(errors.len(), 3);
        assert!(errors[0].contains("unsupported wildcard or placeholder in Scope path `crates/*`"));
        assert!(
            errors[1].contains(
                "unsupported wildcard or placeholder in Scope path `docs/<family>/file.rs`"
            )
        );
        assert!(
            errors[2].contains("unsupported wildcard or placeholder in Scope path `tests/?[a]`")
        );
    }

    #[test]
    fn scope_coverage_and_overlap_share_paths() {
        let file_scope = ScopePath::File("crates/a/b.rs".to_owned());
        let dir_scope = ScopePath::Dir("tests/fix".to_owned());

        assert!(covers(&file_scope, "crates/a/b.rs"));
        assert!(!covers(&file_scope, "crates/a/c.rs"));

        // Both treat tests/fix vs tests/fixtures/x as not related
        assert!(!covers(&dir_scope, "tests/fixtures/x"));
        assert_eq!(
            overlap(&dir_scope, &ScopePath::File("tests/fixtures/x".to_owned())),
            None
        );
        assert_eq!(
            overlap(&dir_scope, &ScopePath::Dir("tests/fixtures".to_owned())),
            None
        );

        // Directory covers deeper files and itself
        assert!(covers(&dir_scope, "tests/fix"));
        assert!(covers(&dir_scope, "tests/fix/foo.rs"));
        assert_eq!(
            overlap(&dir_scope, &ScopePath::File("tests/fix/foo.rs".to_owned())),
            Some("tests/fix/foo.rs".to_owned())
        );

        // Both accept crates/a/{b.rs,c.rs} brace alternatives
        let (paths, errors) = parse_scope("- Files: `crates/a/{b.rs,c.rs}`.");
        assert!(errors.is_empty());
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], ScopePath::File("crates/a/b.rs".to_owned()));
        assert_eq!(paths[1], ScopePath::File("crates/a/c.rs".to_owned()));
        assert!(covers(&paths[0], "crates/a/b.rs"));
        assert!(covers(&paths[1], "crates/a/c.rs"));
        assert_eq!(
            overlap(&paths[0], &ScopePath::File("crates/a/b.rs".to_owned())),
            Some("crates/a/b.rs".to_owned())
        );
    }

    #[test]
    fn test_card_section_lines_and_bullets() {
        let text = valid_card("sample-card");
        let card = Card {
            slug: "sample-card".to_owned(),
            path: "docs/cards/sample-card.md".to_owned(),
            draft: false,
            text,
        };
        assert!(!card.draft);

        let sec = card.section_lines("Outcome");
        assert!(sec.is_some());
        let (line, body) = sec.unwrap();
        assert_eq!(line, 7);
        assert!(!body.is_empty());

        let bullet = card.bullet("Scope", "Files");
        assert!(bullet.is_some());
        let (_, btext) = bullet.unwrap();
        assert!(btext.contains("xtask/src/main.rs"));

        let mut known = BTreeSet::new();
        known.insert("check-ci-and-docs".to_owned());
        let prereqs = card.prerequisites(&known);
        assert_eq!(prereqs, vec!["check-ci-and-docs"]);
    }

    #[test]
    fn card_out_of_template_fails_title_mismatch() {
        let mut content = valid_card("demo-card");
        content = content.replace("# demo-card", "# wrong-title");
        let snap = test_snapshot("demo-card", &content);
        let findings = lint_cards(&snap);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("expected `# demo-card`"))
        );

        // Passing
        let snap_pass = test_snapshot("demo-card", &valid_card("demo-card"));
        assert!(lint_cards(&snap_pass).is_empty());
    }

    #[test]
    fn card_out_of_template_fails_header_order() {
        let content = r#"# demo-card

- **Schema impact:** none
- **Runs where:** home, CI Linux
- **Review:** auto

## Outcome

Text.

## Why

Text.

## Scope

- Files: `xtask/src/main.rs`.
- Docs to update: `README.md`.

## Out of scope

Nothing.

## Preconditions

- Cards merged: `check-ci-and-docs`.

## Steps

1. Step.

## Acceptance

```sh
just check
```

Expected: ok.

## Negative control

Text.

## Rollback

Text.

## BLOCKED

Text.
"#;
        let snap = test_snapshot("demo-card", content);
        let findings = lint_cards(&snap);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("out of template order"))
        );
    }

    #[test]
    fn card_out_of_template_fails_bad_runs_where() {
        let content = valid_card("demo-card").replace(
            "- **Runs where:** home, CI Linux",
            "- **Runs where:** home, cloud-runner",
        );
        let snap = test_snapshot("demo-card", &content);
        let findings = lint_cards(&snap);
        assert!(findings.iter().any(|f| {
            f.message
                .contains("invalid Runs where value `cloud-runner`")
        }));
    }

    #[test]
    fn card_out_of_template_fails_schema_additive_needs_owner() {
        let content = valid_card("demo-card")
            .replace("- **Schema impact:** none", "- **Schema impact:** additive");
        let snap = test_snapshot("demo-card", &content);
        let findings = lint_cards(&snap);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("requires Review: owner"))
        );

        // Passing when Review is owner
        let content_pass = content.replace("- **Review:** auto", "- **Review:** owner");
        let snap_pass = test_snapshot("demo-card", &content_pass);
        assert!(lint_cards(&snap_pass).is_empty());
    }

    #[test]
    fn card_out_of_template_fails_missing_section_rollback() {
        let mut content = valid_card("demo-card");
        content = content.replace("## Rollback\n\nRollback text.\n\n", "");
        let snap = test_snapshot("demo-card", &content);
        let findings = lint_cards(&snap);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("missing section ## Rollback"))
        );
    }

    #[test]
    fn card_out_of_template_fails_evidence_section() {
        let content = valid_card("demo-card") + "\n## Evidence\n\nNot allowed.\n";
        let snap = test_snapshot("demo-card", &content);
        let findings = lint_cards(&snap);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("an `## Evidence` section is an error"))
        );
    }

    #[test]
    fn card_out_of_template_fails_acceptance_last_command() {
        let content = valid_card("demo-card").replace("just check", "cargo test");
        let snap = test_snapshot("demo-card", &content);
        let findings = lint_cards(&snap);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("expected `just check`"))
        );
    }

    #[test]
    fn nextest_filter_names_a_file_module_or_full_test() {
        let card_text_with_flag = valid_card("test-card").replace(
            "```sh\njust check\n```",
            "```sh\ncargo nextest run --locked -p fieldbook --all-features --test policy_load\njust check\n```",
        );
        let card = Card {
            slug: "test-card".to_owned(),
            path: "docs/cards/test-card.md".to_owned(),
            draft: false,
            text: card_text_with_flag,
        };
        let findings = lint_nextest(&card);
        assert!(
            findings.is_empty(),
            "expected clean findings for --test policy_load: {findings:?}"
        );

        // `--test known_bad` passes
        let card_test_known_bad = Card {
            slug: "test-card".to_owned(),
            path: "docs/cards/test-card.md".to_owned(),
            draft: false,
            text: valid_card("test-card").replace(
                "```sh\njust check\n```",
                "```sh\ncargo nextest run --locked -p fieldbook --test known_bad\njust check\n```",
            ),
        };
        assert!(lint_nextest(&card_test_known_bad).is_empty());

        // `-p fieldbook known_bad` fails naming line and word
        let card_bare_known_bad = Card {
            slug: "test-card".to_owned(),
            path: "docs/cards/test-card.md".to_owned(),
            draft: false,
            text: valid_card("test-card").replace(
                "```sh\njust check\n```",
                "```sh\ncargo nextest run -p fieldbook known_bad\njust check\n```",
            ),
        };
        let findings_bare = lint_nextest(&card_bare_known_bad);
        assert_eq!(findings_bare.len(), 1);
        assert!(findings_bare[0].message.contains("known_bad"));

        // Module path passes
        let card_module = Card {
            slug: "test-card".to_owned(),
            path: "docs/cards/test-card.md".to_owned(),
            draft: false,
            text: valid_card("test-card").replace(
                "```sh\njust check\n```",
                "```sh\ncargo nextest run -p xtask cards::tests::\njust check\n```",
            ),
        };
        assert!(lint_nextest(&card_module).is_empty());

        // Whole test name that is a code span of its own passes; without code span fails
        let card_full_pass = Card {
            slug: "test-card".to_owned(),
            path: "docs/cards/test-card.md".to_owned(),
            draft: false,
            text: valid_card("test-card")
                .replace("Step one.", "Step one mentions `planted_test_name_two`.")
                .replace(
                    "```sh\njust check\n```",
                    "```sh\ncargo nextest run -p fieldbook planted_test_name_two\njust check\n```",
                ),
        };
        assert!(lint_nextest(&card_full_pass).is_empty());

        let card_full_fail = Card {
            slug: "test-card".to_owned(),
            path: "docs/cards/test-card.md".to_owned(),
            draft: false,
            text: valid_card("test-card").replace(
                "```sh\njust check\n```",
                "```sh\ncargo nextest run -p fieldbook planted_test_name_two\njust check\n```",
            ),
        };
        let findings_full_fail = lint_nextest(&card_full_fail);
        assert_eq!(findings_full_fail.len(), 1);
        assert!(
            findings_full_fail[0]
                .message
                .contains("planted_test_name_two")
        );

        // `--exclude xtask` and `--no-default-features` pass
        let card_exclude_and_no_default = Card {
            slug: "test-card".to_owned(),
            path: "docs/cards/test-card.md".to_owned(),
            draft: false,
            text: valid_card("test-card").replace(
                "```sh\njust check\n```",
                "```sh\ncargo nextest run --locked --workspace --exclude xtask --no-default-features --test mcp_server\njust check\n```",
            ),
        };
        assert!(lint_nextest(&card_exclude_and_no_default).is_empty());
    }

    #[test]
    fn test_render_new_card() {
        let snap = Snapshot::from_files(&[("docs/cards/README.md", TEST_README)]);

        // Valid slug writes a file that passes lint_cards
        let rendered = render_new_card(&snap, "my-new-card").unwrap();
        assert!(rendered.starts_with("# my-new-card\n"));
        let snap_with_rendered = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/cards/my-new-card.md", &rendered),
        ]);
        let findings = lint_cards(&snap_with_rendered);
        assert!(
            findings.is_empty(),
            "rendered card must pass lint_cards: {findings:?}"
        );

        // Bad slug fails
        assert!(render_new_card(&snap, "Invalid_Slug").is_err());
        assert!(render_new_card(&snap, "bad--slug").is_err());
        assert!(render_new_card(&snap, "-bad").is_err());

        // Existing file refused
        let snap_existing = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/cards/existing.md", "content"),
        ]);
        assert!(render_new_card(&snap_existing, "existing").is_err());
    }

    #[test]
    fn test_completed_slugs_in_temp_repo() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();

        // No commit repository returns empty set
        assert!(
            std::process::Command::new("git")
                .arg("init")
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(completed_slugs(root).unwrap(), BTreeSet::new());

        // Commit a card, then delete it and commit deletion
        let cards_dir = root.join("docs/cards");
        std::fs::create_dir_all(&cards_dir).unwrap();
        std::fs::write(cards_dir.join("sample.md"), "test").unwrap();
        std::fs::write(cards_dir.join("README.md"), "readme").unwrap();

        let run_git = |args: &[&str]| {
            assert!(
                std::process::Command::new("git")
                    .args([
                        "-c",
                        "user.name=Agent",
                        "-c",
                        "user.email=agent@example.com"
                    ])
                    .args(args)
                    .current_dir(root)
                    .status()
                    .unwrap()
                    .success()
            );
        };

        run_git(&["add", "."]);
        run_git(&["commit", "-m", "add cards"]);

        // Delete sample.md and README.md
        std::fs::remove_file(cards_dir.join("sample.md")).unwrap();
        std::fs::remove_file(cards_dir.join("README.md")).unwrap();
        run_git(&["add", "."]);
        run_git(&["commit", "-m", "delete card"]);

        let slugs = completed_slugs(root).unwrap();
        assert!(slugs.contains("sample"));
        assert!(!slugs.contains("README"));
    }

    #[test]
    fn test_plan_items() {
        let plan = r#"# Plan

Header line to ignore.

## F0 Freeze

- Do something first (lead)
- Accept something (owner)

## P1 Read

- workspace-skeleton: workspace skeleton
- card-alpha: the alpha card
- card-beta: the beta card (with note)

### Day-1 content

- Pinned draft (lead)
- card-gamma: gamma card
"#;
        let snap = Snapshot::from_files(&[("docs/plan/PLAN.md", plan)]);
        let items = plan_items(&snap).unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].slug, "workspace-skeleton");
        assert_eq!(items[0].phase, "P1");
        assert_eq!(items[1].slug, "card-alpha");
        assert_eq!(items[1].phase, "P1");
        assert_eq!(items[2].slug, "card-beta");
        assert_eq!(items[2].phase, "P1");
        assert_eq!(items[3].slug, "card-gamma");
        assert_eq!(items[3].phase, "P1");
    }

    #[test]
    fn test_strict_phase_rules() {
        let plan = r#"# Plan

## P1 Read

- workspace-skeleton: workspace skeleton
- card-open: has file
- card-done: completed in git
- card-missing: neither file nor completed
"#;
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-open.md", &valid_card("card-open")),
        ])
        .with_completed("card-done");

        let findings = lint_plan(&snap);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/plan/PLAN.md");
        assert_eq!(findings[0].line, 8);
        assert!(findings[0].message.contains("card-missing"));
        assert!(findings[0].message.contains("strict phase P1"));
    }

    #[test]
    fn test_lenient_phase_rules() {
        let plan = r#"# Plan

## P4

- item-draft: draft card

## P5

- item-open: open card

## P6

- item-done: completed card

## Reader

- item-bare: one line item with no file
"#;
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/drafts/item-draft.md", &valid_card("item-draft")),
            ("docs/cards/item-open.md", &valid_card("item-open")),
        ])
        .with_completed("item-done");

        let findings = lint_plan(&snap);
        assert!(
            findings.is_empty(),
            "all lenient phase states should be accepted: {findings:?}"
        );
    }

    #[test]
    fn plan_accepts_later_lines_in_p6_to_p8() {
        let card_content = valid_card("later-sample");

        for phase in &["P6", "P7", "P8"] {
            let plan_text = format!("# Plan\n\n## {phase} Phase\n\n- later-sample: description\n");

            // 1. Bare line (no file)
            let snap_bare = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
            ]);
            let findings = lint_plan(&snap_bare);
            assert!(
                findings.is_empty(),
                "phase {phase} must accept bare later- line: {findings:?}"
            );

            // 2. Draft
            let snap_draft = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
                ("docs/cards/drafts/later-sample.md", &card_content),
            ]);
            let findings_draft = lint_plan(&snap_draft);
            assert!(
                findings_draft.is_empty(),
                "phase {phase} must accept later- draft: {findings_draft:?}"
            );

            // 3. Open card
            let snap_open = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
                ("docs/cards/later-sample.md", &card_content),
            ]);
            let findings_open = lint_plan(&snap_open);
            assert!(
                findings_open.is_empty(),
                "phase {phase} must accept later- open card: {findings_open:?}"
            );

            // 4. Completed
            let snap_comp = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
            ])
            .with_completed("later-sample");
            let findings_comp = lint_plan(&snap_comp);
            assert!(
                findings_comp.is_empty(),
                "phase {phase} must accept later- completed: {findings_comp:?}"
            );

            // Exercise promotion (draft -> open card)
            let mut snap_promoted = snap_draft.without("docs/cards/drafts/later-sample.md");
            snap_promoted
                .files
                .insert("docs/cards/later-sample.md".to_owned());
            snap_promoted.text.insert(
                "docs/cards/later-sample.md".to_owned(),
                card_content.clone(),
            );
            assert!(
                lint_plan(&snap_promoted).is_empty(),
                "promotion should pass in {phase}"
            );

            // Exercise closure (open card -> completed)
            let snap_closed = snap_open
                .without("docs/cards/later-sample.md")
                .with_completed("later-sample");
            assert!(
                lint_plan(&snap_closed).is_empty(),
                "closure should pass in {phase}"
            );
        }

        for phase in &["F0", "P1", "P2", "P3", "P4", "P5", "Hosted", "Reader"] {
            let plan_text = format!("# Plan\n\n## {phase} Phase\n\n- later-sample: description\n");

            // 1. Bare line
            let snap_bare = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
            ]);
            let f_bare = lint_plan(&snap_bare);
            assert!(
                f_bare
                    .iter()
                    .any(|f| f.message.contains("later-") && f.message.contains(phase)),
                "phase {phase} must reject bare later- line: {f_bare:?}"
            );

            // 2. Draft
            let snap_draft = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
                ("docs/cards/drafts/later-sample.md", &card_content),
            ]);
            let f_draft = lint_plan(&snap_draft);
            assert!(
                f_draft
                    .iter()
                    .any(|f| f.message.contains("later-") && f.message.contains(phase)),
                "phase {phase} must reject later- draft: {f_draft:?}"
            );

            // 3. Open card
            let snap_open = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
                ("docs/cards/later-sample.md", &card_content),
            ]);
            let f_open = lint_plan(&snap_open);
            assert!(
                f_open
                    .iter()
                    .any(|f| f.message.contains("later-") && f.message.contains(phase)),
                "phase {phase} must reject later- open card: {f_open:?}"
            );

            // 4. Completed
            let snap_comp = Snapshot::from_files(&[
                ("docs/cards/README.md", TEST_README),
                ("docs/plan/PLAN.md", &plan_text),
            ])
            .with_completed("later-sample");
            let f_comp = lint_plan(&snap_comp);
            assert!(
                f_comp
                    .iter()
                    .any(|f| f.message.contains("later-") && f.message.contains(phase)),
                "phase {phase} must reject later- completed: {f_comp:?}"
            );
        }
    }

    #[test]
    fn test_plan_list_lines_and_status_marks() {
        let plan = r#"# Plan

## P1 Read

- [ ] status-bracket: task
- status-pass: item -> pass
- [x] status-x: done
- [!] status-bang: alert
- free list line without colon
- valid-owner-line (owner)
- valid-lead-line (lead)
- valid-card: text
"#;
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/valid-card.md", &valid_card("valid-card")),
        ])
        .with_completed("status-pass");

        let findings = lint_plan(&snap);
        assert_eq!(findings.len(), 5);
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("status-bracket"))
        );
        assert!(findings.iter().any(|f| f.message.contains("status-pass")));
        assert!(findings.iter().any(|f| f.message.contains("status-x")));
        assert!(findings.iter().any(|f| f.message.contains("status-bang")));
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("free list line"))
        );
    }

    #[test]
    fn test_card_without_plan_item_flagged() {
        let plan = "# Plan\n\n## P1 Read\n\n- other-card: description\n";
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/other-card.md", &valid_card("other-card")),
            ("docs/cards/unlisted-card.md", &valid_card("unlisted-card")),
        ]);
        let findings = lint_plan(&snap);
        let unlisted = findings
            .iter()
            .find(|f| f.file == "docs/cards/unlisted-card.md");
        assert!(unlisted.is_some());
        let f = unlisted.unwrap();
        assert_eq!(f.line, 1);
        assert!(f.message.contains("card has no item in docs/plan/PLAN.md"));
    }

    #[test]
    fn test_duplicate_plan_slug_flagged() {
        let plan = "# Plan\n\n## P1 Read\n\n- dup-card: first line\n- dup-card: second line\n";
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/dup-card.md", &valid_card("dup-card")),
        ]);
        let findings = lint_plan(&snap);
        let dup = findings
            .iter()
            .find(|f| f.message.contains("duplicate slug `dup-card`"));
        assert!(dup.is_some());
        let f = dup.unwrap();
        assert!(f.message.contains("lines 5 and 6"));
    }

    #[test]
    fn test_scope_overlap_lint() {
        // 1. A pair with one shared non-shared path and no precondition fails
        let card_a = valid_card("card-a").replace(
            "- Files: `xtask/src/main.rs`.",
            "- Files: `crates/shared.rs`, `crates/a.rs`.",
        );
        let card_b = valid_card("card-b").replace(
            "- Files: `xtask/src/main.rs`.",
            "- Files: `crates/shared.rs`, `crates/b.rs`.",
        );
        let plan = "# Plan\n\n## P1 Read\n\n- card-a: a\n- card-b: b\n";
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-a.md", &card_a),
            ("docs/cards/card-b.md", &card_b),
        ]);
        let findings = lint_overlap(&snap);
        assert_eq!(findings.len(), 1);
        assert!(findings[0]
            .message
            .contains("Scope overlap with no precondition path: card-a.md and card-b.md share crates/shared.rs"));

        // 2. A pair whose only shared path is in SHARED_PATHS passes
        let card_shared_a = valid_card("card-a").replace(
            "- Files: `xtask/src/main.rs`.",
            "- Files: `justfile`, `crates/a.rs`.",
        );
        let card_shared_b = valid_card("card-b").replace(
            "- Files: `xtask/src/main.rs`.",
            "- Files: `justfile`, `crates/b.rs`.",
        );
        let snap_shared = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-a.md", &card_shared_a),
            ("docs/cards/card-b.md", &card_shared_b),
        ]);
        assert!(lint_overlap(&snap_shared).is_empty());

        // 3. A pair that reaches through intermediate slug (A -> M -> B) passes
        let card_trans_a = valid_card("card-a")
            .replace(
                "- Files: `xtask/src/main.rs`.",
                "- Files: `crates/shared.rs`.",
            )
            .replace(
                "- Cards merged: `check-ci-and-docs`.",
                "- Cards merged: `card-m`.",
            );
        let card_trans_m = valid_card("card-m")
            .replace("- Files: `xtask/src/main.rs`.", "- Files: `crates/m.rs`.")
            .replace(
                "- Cards merged: `check-ci-and-docs`.",
                "- Cards merged: `card-b`.",
            );
        let card_trans_b = valid_card("card-b")
            .replace(
                "- Files: `xtask/src/main.rs`.",
                "- Files: `crates/shared.rs`.",
            )
            .replace(
                "- Cards merged: `check-ci-and-docs`.",
                "- Cards merged: none.",
            );
        let plan_trans = "# Plan\n\n## P1 Read\n\n- card-b: b\n- card-m: m\n- card-a: a\n";
        let snap_trans = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan_trans),
            ("docs/cards/card-a.md", &card_trans_a),
            ("docs/cards/card-m.md", &card_trans_m),
            ("docs/cards/card-b.md", &card_trans_b),
        ]);
        assert!(
            lint_overlap(&snap_trans).is_empty(),
            "transitive prerequisite path should satisfy overlap: {:?}",
            lint_overlap(&snap_trans)
        );

        // 4. Directory/sibling-prefix pair does not overlap
        let card_dir_a =
            valid_card("card-a").replace("- Files: `xtask/src/main.rs`.", "- Files: `tests/fix/`.");
        let card_dir_b = valid_card("card-b").replace(
            "- Files: `xtask/src/main.rs`.",
            "- Files: `tests/fixtures/x.rs`.",
        );
        let snap_dir = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-a.md", &card_dir_a),
            ("docs/cards/card-b.md", &card_dir_b),
        ]);
        assert!(lint_overlap(&snap_dir).is_empty());

        // 5. Wildcards and <family> are reported with file and line
        let card_wild = valid_card("card-a").replace(
            "- Files: `xtask/src/main.rs`.",
            "- Files: `crates/*`, `docs/<family>/file.rs`.",
        );
        let snap_wild = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-a.md", &card_wild),
        ]);
        let f_wild = lint_overlap(&snap_wild);
        assert_eq!(f_wild.len(), 2);
        assert_eq!(f_wild[0].file, "docs/cards/card-a.md");
        assert!(f_wild[0].line > 0);
        assert!(f_wild[0].message.contains("crates/*"));
        assert_eq!(f_wild[1].file, "docs/cards/card-a.md");
        assert!(f_wild[1].message.contains("<family>"));
    }

    #[test]
    fn prerequisite_cycle_is_rejected_unit() {
        let card_a = valid_card("card-a").replace(
            "- Cards merged: `check-ci-and-docs`.",
            "- Cards merged: `card-b`.",
        );
        let card_b = valid_card("card-b").replace(
            "- Cards merged: `check-ci-and-docs`.",
            "- Cards merged: `card-a`.",
        );
        let plan = "# Plan\n\n## P1 Read\n\n- card-a: a\n- card-b: b\n";
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-a.md", &card_a),
            ("docs/cards/card-b.md", &card_b),
        ]);
        let findings = lint_prerequisites(&snap);
        assert!(findings.iter().any(|f| {
            f.message
                .contains("prerequisite cycle: card-a -> card-b -> card-a")
        }));

        // Passing case: same two described as safe to run beside each other in a separate bullet
        let card_beside_a = valid_card("card-a").replace(
            "- Cards merged: `check-ci-and-docs`.",
            "- Cards merged: none.\n- Beside: `card-b`.",
        );
        let card_beside_b = valid_card("card-b").replace(
            "- Cards merged: `check-ci-and-docs`.",
            "- Cards merged: none.\n- Beside: `card-a`.",
        );
        let snap_beside = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-a.md", &card_beside_a),
            ("docs/cards/card-b.md", &card_beside_b),
        ]);
        assert!(lint_prerequisites(&snap_beside).is_empty());
    }

    #[test]
    fn prerequisite_precedes_consumer_unit() {
        // Valid order: provider precedes consumer
        let plan_valid = "# Plan\n\n## P1 Read\n\n- provider-card: p\n- consumer-card: c\n";
        let consumer = valid_card("consumer-card").replace(
            "- Cards merged: `check-ci-and-docs`.",
            "- Cards merged: `provider-card`.",
        );
        let provider = valid_card("provider-card");
        let snap_valid = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan_valid),
            ("docs/cards/provider-card.md", &provider),
            ("docs/cards/consumer-card.md", &consumer),
        ]);
        assert!(lint_prerequisites(&snap_valid).is_empty());

        // Later prerequisite: prerequisite appears after consumer
        let plan_invalid = "# Plan\n\n## P1 Read\n\n- consumer-card: c\n- provider-card: p\n";
        let snap_invalid = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan_invalid),
            ("docs/cards/provider-card.md", &provider),
            ("docs/cards/consumer-card.md", &consumer),
        ]);
        let findings = lint_prerequisites(&snap_invalid);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].file, "docs/cards/consumer-card.md");
        assert!(findings[0].line > 0);
        assert!(findings[0].message.contains(
            "prerequisite `provider-card` comes after its consumer `consumer-card` in docs/plan/PLAN.md"
        ));
    }

    #[test]
    fn closing_card_keeps_permanent_links_valid_unit() {
        let plan = "# Plan\n\n## P1 Read\n\n- closed-card: desc\n";
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/closed-card.md", &valid_card("closed-card")),
        ]);

        let closing = closing_snapshot(&snap, "closed-card");
        assert!(!closing.files.contains("docs/cards/closed-card.md"));
        assert!(closing.completed.contains("closed-card"));
        assert!(lint_plan(&closing).is_empty());
        assert!(lint_prerequisites(&closing).is_empty());
        assert!(lint_overlap(&closing).is_empty());
    }

    #[test]
    fn closing_with_inbound_card_link_fails_unit() {
        let plan = "# Plan\n\n## P1 Read\n\n- closed-card: desc\n- missing-card: desc\n";
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/closed-card.md", &valid_card("closed-card")),
        ]);

        let closing = closing_snapshot(&snap, "closed-card");
        let findings = lint_plan(&closing);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("missing-card"));
        assert!(!findings.iter().any(|f| f.message.contains("closed-card")));
    }

    #[test]
    fn test_command_and_run() {
        let plan = "# Plan\n\n## P1 Read\n\n- card-a: test\n";
        let snap = Snapshot::from_files(&[
            ("docs/cards/README.md", TEST_README),
            ("docs/plan/PLAN.md", plan),
            ("docs/cards/card-a.md", &valid_card("card-a")),
        ]);
        let findings = run(&snap, None);
        assert!(findings.is_empty(), "clean run: {findings:?}");

        // Closing run
        let findings_closing = run(&snap, Some("card-a"));
        assert!(
            findings_closing.is_empty(),
            "clean closing run: {findings_closing:?}"
        );

        // Bad args to command
        let temp = tempfile::tempdir().unwrap();
        let res = command(temp.path(), &["--invalid"]);
        assert_eq!(res, Err("bad arguments".to_owned()));
    }

    #[test]
    fn merged_means_deleted_in_history_or_legacy() {
        let snap = Snapshot::from_files(&[
            ("docs/cards/open.md", "x"),
            ("docs/cards/drafts/later.md", "x"),
        ])
        .with_completed("open")
        .with_completed("later")
        .with_completed("gone");
        assert!(is_merged(&snap, "gone"));
        assert!(is_merged(&snap, "workspace-skeleton"));
        assert!(is_merged(&snap, "search-page-view"));
        assert!(!is_merged(&snap, "open"), "its card file still exists");
        assert!(!is_merged(&snap, "later"), "its draft still exists");
        assert!(!is_merged(&snap, "never-seen"));
    }
}
