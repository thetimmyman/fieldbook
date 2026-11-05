//! `docs`: regenerate generated documentation blocks and check for drift (AD-22).

use std::path::Path;
use std::process::ExitCode;

/// A registered documentation generator.
#[derive(Clone, Copy)]
pub struct Generator {
    /// Path relative to the repository root.
    pub file: &'static str,
    /// Generator name.
    pub name: &'static str,
    /// Opening marker line.
    pub begin: &'static str,
    /// Closing marker line.
    pub end: &'static str,
    /// Function that takes the current body between `begin` and `end` and returns the regenerated body.
    pub render: fn(&str) -> Result<String, String>,
}

/// The registered documentation generators.
pub const GENERATORS: &[Generator] = &[
    Generator {
        file: "docs/reference/corpus-format.md",
        name: "strings",
        begin: "<!-- strings:begin -->",
        end: "<!-- strings:end -->",
        render: render_strings_section,
    },
    Generator {
        file: "docs/reference/errors.md",
        name: "errors",
        begin: "<!-- generated:errors -->",
        end: "<!-- /generated:errors -->",
        render: crate::errors::render_errors_section,
    },
    Generator {
        file: "docs/reference/cli.md",
        name: "cli",
        begin: "<!-- generated:cli -->",
        end: "<!-- /generated:cli -->",
        render: crate::cli_doc::render_cli_section,
    },
    Generator {
        file: "docs/reference/audit.md",
        name: "audit",
        begin: "<!-- generated:audit -->",
        end: "<!-- /generated:audit -->",
        render: crate::audit_doc::render_audit_section,
    },
];

/// Replace the text between the marker lines `begin` and `end` with `body`, leaving all
/// other text alone. Returns an error naming the missing marker when either marker line is
/// absent or out of order.
pub fn fill(text: &str, begin: &str, end: &str, body: &str) -> Result<String, String> {
    let mut before = String::new();
    let mut after = String::new();
    let mut state = 0u8;

    for chunk in text.split_inclusive('\n') {
        let line = chunk.strip_suffix('\n').unwrap_or(chunk);
        let line = line.strip_suffix('\r').unwrap_or(line);
        match state {
            0 => {
                before.push_str(chunk);
                if line == begin {
                    if !before.ends_with('\n') {
                        before.push('\n');
                    }
                    state = 1;
                }
            }
            1 => {
                if line == end {
                    after.push_str(chunk);
                    state = 2;
                }
            }
            _ => {
                after.push_str(chunk);
            }
        }
    }

    if state == 0 {
        return Err(format!("missing begin marker `{begin}`"));
    }
    if state == 1 {
        return Err(format!("missing end marker `{end}`"));
    }

    let mut out = before;
    out.push_str(body);
    if !body.is_empty() && !body.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&after);
    Ok(out)
}

/// Replace the markdown table starting with `header` inside `body` with `replacement`.
fn replace_table_by_header(body: &str, header: &str, replacement: &str) -> Result<String, String> {
    let mut out = String::with_capacity(body.len());
    let mut found = false;
    let mut in_table = false;

    for chunk in body.split_inclusive('\n') {
        let line = chunk.strip_suffix('\n').unwrap_or(chunk);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !in_table {
            if !found && line == header {
                found = true;
                in_table = true;
                out.push_str(replacement);
                if !replacement.ends_with('\n') {
                    out.push('\n');
                }
            } else {
                out.push_str(chunk);
            }
        } else if !line.starts_with('|') {
            in_table = false;
            out.push_str(chunk);
        }
    }

    if !found {
        return Err(format!("missing table header `{header}`"));
    }
    Ok(out)
}

/// Regenerate the three tables inside the `strings` section of `corpus-format.md`.
pub fn render_strings_section(current_body: &str) -> Result<String, String> {
    let step1 = replace_table_by_header(
        current_body,
        "| Key | String | Value |",
        &fieldbook_core::strings::render_key_table(),
    )?;
    let step2 = replace_table_by_header(
        &step1,
        "| Intent | Chip string | From |",
        &fieldbook_core::strings::render_verb_table(),
    )?;
    replace_table_by_header(
        &step2,
        "| Key | Wording | Decision |",
        &fieldbook_core::strings::render_other_table(),
    )
}

/// Extract the body between `begin` and `end` marker lines, or return an error naming the missing marker.
fn extract_between<'a>(text: &'a str, begin: &str, end: &str) -> Result<&'a str, String> {
    let mut offset = 0usize;
    let mut body_start = None;

    for chunk in text.split_inclusive('\n') {
        let line = chunk.strip_suffix('\n').unwrap_or(chunk);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let next_offset = offset + chunk.len();
        if body_start.is_none() {
            if line == begin {
                body_start = Some(next_offset);
            }
        } else if line == end {
            let start = body_start.unwrap_or(0);
            return Ok(&text[start..offset]);
        }
        offset = next_offset;
    }

    if body_start.is_none() {
        Err(format!("missing begin marker `{begin}`"))
    } else {
        Err(format!("missing end marker `{end}`"))
    }
}

/// Find the first differing 1-based line number and its expected/actual text between `actual` and `expected`.
#[must_use]
pub fn first_difference(actual: &str, expected: &str) -> Option<(usize, String)> {
    if actual == expected {
        return None;
    }
    let mut actual_lines = actual.lines();
    let mut expected_lines = expected.lines();
    let mut line_no = 1usize;
    loop {
        match (actual_lines.next(), expected_lines.next()) {
            (Some(a), Some(e)) if a == e => {
                line_no += 1;
            }
            (Some(a), Some(e)) => {
                return Some((line_no, format!("line {line_no}: `{a}` != `{e}`")));
            }
            (Some(a), None) => {
                return Some((line_no, format!("line {line_no}: extra line `{a}`")));
            }
            (None, Some(e)) => {
                return Some((line_no, format!("line {line_no}: missing line `{e}`")));
            }
            (None, None) => {
                return Some((line_no, format!("line {line_no}: trailing newline differs")));
            }
        }
    }
}

/// Run the generators on `root` with the provided slice of `Generator`s.
pub fn run_with(root: &Path, generators: &[Generator], check: bool) -> Result<(), (u8, String)> {
    for generator in generators {
        let path = root.join(generator.file);
        let current = std::fs::read_to_string(&path).map_err(|err| {
            (
                2u8,
                format!("docs: {} ({}): {err}", generator.file, generator.name),
            )
        })?;
        let current_body =
            extract_between(&current, generator.begin, generator.end).map_err(|err| {
                (
                    2u8,
                    format!("docs: {} ({}): {err}", generator.file, generator.name),
                )
            })?;
        let new_body = (generator.render)(current_body).map_err(|err| {
            (
                2u8,
                format!("docs: {} ({}): {err}", generator.file, generator.name),
            )
        })?;
        let updated = fill(&current, generator.begin, generator.end, &new_body).map_err(|err| {
            (
                2u8,
                format!("docs: {} ({}): {err}", generator.file, generator.name),
            )
        })?;

        if check {
            if let Some((_, diff)) = first_difference(&current, &updated) {
                return Err((
                    1u8,
                    format!(
                        "docs: {} ({}) differs at {diff}",
                        generator.file, generator.name
                    ),
                ));
            }
        } else if current != updated {
            std::fs::write(&path, updated)
                .map_err(|err| (3u8, format!("docs: cannot write {}: {err}", generator.file)))?;
        }
    }
    if check {
        println!("docs: up to date");
    }
    Ok(())
}

/// Entrypoint for `cargo xtask docs [--check]`.
pub fn command(root: &Path, check: bool) -> ExitCode {
    match run_with(root, GENERATORS, check) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, msg)) => {
            eprintln!("{msg}");
            ExitCode::from(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Generator, fill, run_with};

    fn sample_gen(_: &str) -> Result<String, String> {
        Ok("alpha\nbeta\n".to_owned())
    }

    #[test]
    fn fill_replaces_only_between_markers() {
        let input = "before 1\nbefore 2\n<!-- generated:demo -->\nold 1\nold 2\n<!-- /generated:demo -->\nafter 1\nafter 2\n";
        let out = fill(
            input,
            "<!-- generated:demo -->",
            "<!-- /generated:demo -->",
            "new 1\nnew 2\n",
        )
        .expect("markers present");
        assert_eq!(
            out,
            "before 1\nbefore 2\n<!-- generated:demo -->\nnew 1\nnew 2\n<!-- /generated:demo -->\nafter 1\nafter 2\n"
        );
    }

    #[test]
    fn check_names_the_first_differing_line() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("demo.md");
        std::fs::write(
            &file_path,
            "# Demo\n<!-- generated:demo -->\nalpha\nwrong\n<!-- /generated:demo -->\n",
        )
        .expect("write demo.md");

        let gens = [Generator {
            file: "demo.md",
            name: "demo",
            begin: "<!-- generated:demo -->",
            end: "<!-- /generated:demo -->",
            render: sample_gen,
        }];

        let err = run_with(dir.path(), &gens, true).expect_err("must detect difference");
        assert_eq!(err.0, 1);
        assert!(err.1.contains("demo.md"), "error was: {}", err.1);
        assert!(err.1.contains("line 4"), "error was: {}", err.1);
        assert!(err.1.contains("wrong"), "error was: {}", err.1);
        assert!(err.1.contains("beta"), "error was: {}", err.1);
    }

    #[test]
    fn missing_marker_is_exit_2() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("demo.md");
        std::fs::write(&file_path, "# Demo\n<!-- generated:demo -->\nalpha\n")
            .expect("write demo.md");

        let gens = [Generator {
            file: "demo.md",
            name: "demo",
            begin: "<!-- generated:demo -->",
            end: "<!-- /generated:demo -->",
            render: sample_gen,
        }];

        let err = run_with(dir.path(), &gens, true).expect_err("must fail on missing marker");
        assert_eq!(err.0, 2);
        assert!(
            err.1.contains("<!-- /generated:demo -->"),
            "error was: {}",
            err.1
        );

        let missing_file_gens = [Generator {
            file: "nonexistent.md",
            name: "demo",
            begin: "<!-- generated:demo -->",
            end: "<!-- /generated:demo -->",
            render: sample_gen,
        }];
        let err_missing =
            run_with(dir.path(), &missing_file_gens, true).expect_err("must fail on missing file");
        assert_eq!(err_missing.0, 2);
        assert!(
            err_missing.1.contains("nonexistent.md"),
            "error was: {}",
            err_missing.1
        );
    }
}

#[cfg(test)]
mod cli {
    use clap::CommandFactory;
    use fieldbook_core::check::Code;

    use crate::cli_doc::render_from_root;
    use crate::errors::parse_catalogue;
    use crate::proc::repo_root;

    #[test]
    fn strict_rows_match_catalogue() {
        let root = repo_root();
        let rendered = render_from_root(&root).expect("render cli.md");
        let catalogue_md = std::fs::read_to_string(root.join("docs/reference/corpus-format.md"))
            .expect("read corpus-format.md");
        let catalogue_rows = parse_catalogue(&catalogue_md).expect("parse catalogue");

        let expected_strict: Vec<(String, String)> = catalogue_rows
            .into_iter()
            .filter(|r| r.severity == "strict")
            .map(|r| (r.code, r.check))
            .collect();
        assert!(!expected_strict.is_empty());

        let mut in_strict = false;
        let mut actual_strict = Vec::new();
        for line in rendered.lines() {
            if let Some(h2) = line.strip_prefix("## ") {
                in_strict = h2.starts_with("Strict rules");
                continue;
            }
            if !in_strict || !line.starts_with('|') {
                continue;
            }
            let cells: Vec<&str> = line[1..line.len() - 1].split('|').map(str::trim).collect();
            if cells.len() == 2 && cells[0].starts_with("`FB") {
                let code_id = cells[0].trim_matches('`').to_owned();
                assert!(Code::parse(&code_id).is_ok());
                actual_strict.push((code_id, cells[1].to_owned()));
            }
        }

        assert_eq!(actual_strict, expected_strict);
    }

    #[test]
    fn every_flag_of_check_appears() {
        let root = repo_root();
        let rendered = render_from_root(&root).expect("render cli.md");
        let cmd = <fieldbook::Cli as CommandFactory>::command();
        let check_sub = cmd
            .get_subcommands()
            .find(|s| s.get_name() == "check")
            .expect("check subcommand");

        for arg in check_sub.get_arguments() {
            if arg.get_id() == "help" || arg.get_id() == "version" || arg.is_hide_set() {
                continue;
            }
            if let Some(long) = arg.get_long() {
                let needle = format!("`--{long}`");
                assert!(
                    rendered.contains(&needle),
                    "expected `{needle}` in rendered cli.md"
                );
            } else if let Some(names) = arg.get_value_names()
                && let Some(first) = names.first()
            {
                let needle = format!("`<{first}>`");
                assert!(
                    rendered.contains(&needle),
                    "expected `{needle}` in rendered cli.md"
                );
            }
        }
    }

    #[test]
    fn two_runs_give_the_same_bytes() {
        let root = repo_root();
        let first = render_from_root(&root).expect("first render");
        let second = render_from_root(&root).expect("second render");
        assert_eq!(first.as_bytes(), second.as_bytes());
    }
}
