//! Markdown link, AGENTS.md and wording lints (`docs-checks`).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "wired into `cards --check` by the integration step"
    )
)]

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::cards::Snapshot;

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
    let mut seen: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
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

#[cfg(test)]
mod tests {
    use super::{checked_markdown, heading_anchors, line_of};
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
}
