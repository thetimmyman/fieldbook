//! `literals`: fail when a `.rs` or `.js` file outside `strings.rs` retypes a frozen string.

use std::path::Path;

/// Every `(key, text)` pair checked between double quotes in `.rs` and `.js` source files.
fn banned_literals() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    out.push(("abstention", fieldbook_core::strings::abstention()));
    out.push(("empty_part", fieldbook_core::strings::empty_part()));
    out.push(("refusal", fieldbook_core::strings::refusal()));
    for &part in fieldbook_core::strings::parts() {
        out.push(("parts", part));
    }
    for &part in fieldbook_core::strings::brief_parts() {
        out.push(("brief_parts", part));
    }
    for &path in fieldbook_core::strings::paths() {
        out.push(("paths", path));
    }
    out.push(("rounding", fieldbook_core::strings::rounding()));
    for &label in fieldbook_core::strings::origin_labels() {
        out.push(("origin_labels", label));
    }
    out.push(("binary", fieldbook_core::strings::binary()));
    for row in fieldbook_core::strings::chips() {
        out.push(("chips", row.chip));
    }
    for &status in fieldbook_core::strings::cite_statuses() {
        out.push(("cite_statuses", status));
    }
    out.push(("verified_text", fieldbook_core::strings::verified_text()));
    out.push(("non_canonical", fieldbook_core::strings::non_canonical()));
    out.push((
        "left_out_budget",
        fieldbook_core::strings::left_out_budget(),
    ));
    out.push((
        "left_out_policy",
        fieldbook_core::strings::left_out_policy(),
    ));
    out.push(("pack_labels", fieldbook_core::strings::pack_labels()));
    out.push(("not_available", fieldbook_core::strings::not_available()));
    out.push((
        "unreleased_build",
        fieldbook_core::strings::unreleased_build(),
    ));
    out.push(("print_check", fieldbook_core::strings::print_check()));
    out.push(("not_remembered", fieldbook_core::strings::not_remembered()));
    out.push(("tag_at_home", fieldbook_core::strings::tag_at_home()));
    out.push(("tag_r2_laptop", fieldbook_core::strings::tag_r2_laptop()));
    // For assistant_labels the test checks the five labels of more than one word;
    // the one-word labels Ask, Send and Stop are common words, and the golden of
    // assistant.html pins them.
    for &label in fieldbook_core::strings::assistant_labels() {
        if label.contains(' ') {
            out.push(("assistant_labels", label));
        }
    }
    for &label in fieldbook_core::strings::ai_note_label() {
        out.push(("ai_note_label", label));
    }
    out
}

fn should_skip_dir(name: &str) -> bool {
    matches!(name, "snapshots" | "golden" | "goldens" | "target")
}

fn collect_source_files(root: &Path, dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && should_skip_dir(name)
            {
                continue;
            }
            collect_source_files(root, &path, files);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str())
            && matches!(ext, "rs" | "js")
        {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let rel_slash = rel.to_string_lossy().replace('\\', "/");
            if rel_slash == "crates/fieldbook-core/src/strings.rs" {
                continue;
            }
            files.push(path);
        }
    }
}

/// Scan `root` (`crates/` and `xtask/`) and return every violation naming the file and key.
pub fn violations(root: &Path) -> Vec<String> {
    let banned = banned_literals();
    let mut files = Vec::new();
    for top in ["crates", "xtask"] {
        collect_source_files(root, &root.join(top), &mut files);
    }

    let mut out = Vec::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let rel = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");
        for &(key, phrase) in &banned {
            let needle = format!("\"{phrase}\"");
            if text.contains(&needle) {
                out.push(format!("{rel}: retypes frozen string `{key}` ({needle})"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::violations;
    use crate::proc::repo_root;

    #[test]
    fn no_file_retypes_a_frozen_string() {
        let found = violations(&repo_root());
        assert!(
            found.is_empty(),
            "frozen literals found outside strings.rs:\n  {}",
            found.join("\n  ")
        );
    }
}
