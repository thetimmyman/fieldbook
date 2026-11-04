//! `cli_doc`: render `docs/reference/cli.md` from `fieldbook::Cli`, `EXIT_CODES`,
//! the Check catalogue of `docs/reference/corpus-format.md` and `tests/golden/json/*.json`
//! (`check-ci-and-docs`, AD-17, AD-22).

use std::fmt::Write as _;
use std::path::Path;

use clap::CommandFactory;
use fieldbook_core::check::{Code, Level};
use serde_json::Value;

use crate::errors::parse_catalogue;
use crate::proc::repo_root;

/// Render the generated body between `<!-- generated:cli -->` and `<!-- /generated:cli -->`
/// in `docs/reference/cli.md`.
///
/// # Errors
///
/// Returns an error if `docs/reference/corpus-format.md` or `tests/golden/json/` cannot be
/// read or parsed.
pub fn render_cli_section(_current_body: &str) -> Result<String, String> {
    render_from_root(&repo_root())
}

/// Render the `cli.md` body using inputs under `root`.
///
/// # Errors
///
/// Returns an error if the catalogue or golden JSON files cannot be read or parsed.
pub fn render_from_root(root: &Path) -> Result<String, String> {
    let catalogue_path = root.join("docs/reference/corpus-format.md");
    let catalogue_md = std::fs::read_to_string(&catalogue_path)
        .map_err(|e| format!("cannot read {}: {e}", catalogue_path.display()))?;
    let golden_dir = root.join("tests/golden/json");
    let entries = std::fs::read_dir(&golden_dir)
        .map_err(|e| format!("cannot read {}: {e}", golden_dir.display()))?;
    let mut golden_paths: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    golden_paths.sort();

    let mut goldens = Vec::with_capacity(golden_paths.len());
    for path in golden_paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        goldens.push((stem.to_owned(), raw));
    }

    render(&catalogue_md, &goldens)
}

fn json_value_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn format_arg_flag(arg: &clap::Arg) -> String {
    if let Some(long) = arg.get_long() {
        format!("`--{long}`")
    } else if let Some(short) = arg.get_short() {
        format!("`-{short}`")
    } else {
        let names = arg.get_value_names().unwrap_or_default();
        if let Some(first) = names.first() {
            format!("`<{first}>`")
        } else {
            format!("`<{}>`", arg.get_id())
        }
    }
}

fn format_arg_value(arg: &clap::Arg) -> String {
    if !arg.get_action().takes_values() {
        return "-".to_owned();
    }
    let names = arg.get_value_names().unwrap_or_default();
    if names.is_empty() {
        format!("`{}`", arg.get_id())
    } else {
        let joined: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
        joined.join(" ")
    }
}

fn format_arg_default(arg: &clap::Arg) -> String {
    let defaults = arg.get_default_values();
    if defaults.is_empty() {
        "-".to_owned()
    } else {
        let joined: Vec<String> = defaults
            .iter()
            .map(|d| format!("`{}`", d.to_string_lossy()))
            .collect();
        joined.join(", ")
    }
}

fn format_arg_help(arg: &clap::Arg) -> String {
    arg.get_help()
        .map(std::string::ToString::to_string)
        .unwrap_or_default()
}

/// Render the complete `cli.md` generated section from the parsed inputs.
///
/// # Errors
///
/// Returns an error if the catalogue tables or golden JSON payloads are invalid.
pub fn render(catalogue_md: &str, goldens: &[(String, String)]) -> Result<String, String> {
    let mut out = String::new();
    out.push_str(
        "Generated from the command-line parser, the [Check catalogue](corpus-format.md#check-catalogue) and `tests/golden/json/` by `cargo xtask docs`. Do not edit by hand.\n\n",
    );

    let cmd = <fieldbook::Cli as CommandFactory>::command();
    let bin = fieldbook_core::strings::binary();

    if let Some(about) = cmd.get_about() {
        let _ = writeln!(out, "{about}\n");
    }

    out.push_str("## Global options\n\n");
    out.push_str("| Option | Value | Default | Help |\n|---|---|---|---|\n");
    for arg in cmd.get_arguments() {
        if arg.get_id() == "help" || arg.is_hide_set() {
            continue;
        }
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            format_arg_flag(arg),
            format_arg_value(arg),
            format_arg_default(arg),
            format_arg_help(arg)
        );
    }

    out.push_str("\n## Commands\n");
    for sub in cmd.get_subcommands() {
        if sub.get_name() == "help" || sub.is_hide_set() {
            continue;
        }
        let _ = write!(out, "\n### `{bin} {}`\n\n", sub.get_name());
        if let Some(about) = sub.get_about() {
            let _ = writeln!(out, "{about}\n");
        }
        out.push_str("| Argument / flag | Value | Default | Help |\n|---|---|---|---|\n");
        for arg in sub.get_arguments() {
            if arg.get_id() == "help" || arg.get_id() == "version" || arg.is_hide_set() {
                continue;
            }
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                format_arg_flag(arg),
                format_arg_value(arg),
                format_arg_default(arg),
                format_arg_help(arg)
            );
        }
    }

    out.push_str("\n## Exit codes\n\n");
    out.push_str("| Code | Meaning |\n|---|---|\n");
    for &(code, meaning) in fieldbook::cli::EXIT_CODES {
        let _ = writeln!(out, "| `{code}` | {meaning} |");
    }

    out.push_str("\n## Strict rules (`--strict`)\n\n");
    out.push_str("| Code | Check |\n|---|---|\n");
    let rows = parse_catalogue(catalogue_md)?;
    for row in &rows {
        let code = Code::parse(&row.code)
            .map_err(|_| format!("catalogue code `{}` is absent from `Code`", row.code))?;
        if code.level() == Level::Strict {
            let _ = writeln!(out, "| `{}` | {} |", code.id(), row.check);
        }
    }

    if let Some((_, check_raw)) = goldens.iter().find(|(name, _)| name == "check") {
        let parsed: Value = serde_json::from_str(check_raw)
            .map_err(|e| format!("invalid JSON in check.json: {e}"))?;
        let obj = parsed
            .as_object()
            .ok_or_else(|| "check.json root must be a JSON object".to_owned())?;

        out.push_str("\n## `check --json` layout\n\n");
        out.push_str("| Key | Type |\n|---|---|\n");
        if let Some(rv) = obj.get("report_version") {
            let _ = writeln!(out, "| `report_version` | `{}` |", json_value_type(rv));
        }
        for (k, v) in obj {
            if k == "report_version" {
                continue;
            }
            let _ = writeln!(out, "| `{k}` | `{}` |", json_value_type(v));
        }
    }

    out.push_str("\n## JSON reports\n");
    for (name, raw) in goldens {
        let _ = write!(
            out,
            "\n### `{name}`\n\n```json\n{}\n```\n",
            raw.trim_end_matches('\n')
        );
    }

    Ok(out)
}
