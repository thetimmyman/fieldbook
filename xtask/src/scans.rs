//! Source code scans for network-capable types (NS-05), model usage in deterministic verbs (DT-05),
//! and scan integrity (SCAN-00: fail closed; nothing scanned is a failure).

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const NS_05_TOKENS: &[&str] = &[
    "TcpStream",
    "TcpListener",
    "UdpSocket",
    "UreqHttp",
    "ureq::",
    "rustls",
];

const DT_05_TOKENS: &[&str] = &[
    "ModelPort",
    "Guarded",
    "HttpPost",
    "UreqHttp",
    "adapters::model",
];

/// A finding produced by code scanning.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// Repository-relative path with forward slashes.
    pub file: String,
    /// 1-based line number.
    pub line: usize,
    /// Rule identifier (e.g. `NS-05`, `DT-05`, or `SCAN-00` for fail closed: nothing scanned is a failure).
    pub rule: &'static str,
    /// The trimmed text of the line containing the finding.
    pub text: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}: {}: {}",
            self.file,
            self.line,
            self.rule,
            self.text.trim()
        )
    }
}

fn normalize_path(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn walk_rs_files(dir: &Path, acc: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        if ft.is_symlink() {
            continue;
        }
        entries.push((entry.path(), ft.is_dir()));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, is_dir) in entries {
        if is_dir {
            walk_rs_files(&path, acc)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            acc.push(path);
        }
    }
    Ok(())
}

fn is_io_net(subpath: &str) -> bool {
    subpath == "io/net.rs" || subpath.starts_with("io/net/")
}

fn is_adapters(subpath: &str) -> bool {
    subpath.starts_with("adapters/") || subpath == "adapters.rs"
}

fn is_ai_verb_file(subpath: &str, ai_verb_files: &[&str]) -> bool {
    ai_verb_files.contains(&subpath)
}

/// Scan `crates/fieldbook/src/` under `root` with the compiled `AI_VERB_FILES`.
pub fn scan(root: &Path) -> Vec<Finding> {
    scan_with(root, fieldbook::verbs::AI_VERB_FILES)
}

/// Scan `crates/fieldbook/src/` under `root` with a caller-supplied `ai_verb_files` list.
pub fn scan_with(root: &Path, ai_verb_files: &[&str]) -> Vec<Finding> {
    let src_dir = root.join("crates/fieldbook/src");
    if !src_dir.is_dir() {
        let err = std::fs::read_dir(&src_dir).err().unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "not a directory")
        });
        return vec![Finding {
            file: "crates/fieldbook/src".to_owned(),
            line: 0,
            rule: "SCAN-00",
            text: format!(
                "source folder is missing or cannot be walked: crates/fieldbook/src ({err})"
            ),
        }];
    }

    let mut files = Vec::new();
    if let Err(err) = walk_rs_files(&src_dir, &mut files) {
        return vec![Finding {
            file: "crates/fieldbook/src".to_owned(),
            line: 0,
            rule: "SCAN-00",
            text: format!(
                "source folder is missing or cannot be walked: crates/fieldbook/src ({err})"
            ),
        }];
    }
    files.sort();

    let mut findings = Vec::new();
    for file_path in files {
        let rel_path = file_path.strip_prefix(root).unwrap_or(&file_path);
        let file_rel = normalize_path(rel_path);
        let subpath = file_rel
            .strip_prefix("crates/fieldbook/src/")
            .unwrap_or(&file_rel);

        let io_net = is_io_net(subpath);
        let adapters = is_adapters(subpath);
        let ai_verb = is_ai_verb_file(subpath, ai_verb_files);

        let content = match std::fs::read_to_string(&file_path) {
            Ok(content) => content,
            Err(err) => {
                findings.push(Finding {
                    file: file_rel.clone(),
                    line: 0,
                    rule: "SCAN-00",
                    text: format!("file cannot be read: {file_rel} ({err})"),
                });
                continue;
            }
        };

        for (idx, line) in content.lines().enumerate() {
            let line_no = idx + 1;
            let trimmed = line.trim();

            if !io_net && NS_05_TOKENS.iter().any(|tok| line.contains(tok)) {
                findings.push(Finding {
                    file: file_rel.clone(),
                    line: line_no,
                    rule: "NS-05",
                    text: trimmed.to_owned(),
                });
            }

            if !adapters && !io_net && !ai_verb && DT_05_TOKENS.iter().any(|tok| line.contains(tok))
            {
                findings.push(Finding {
                    file: file_rel.clone(),
                    line: line_no,
                    rule: "DT-05",
                    text: trimmed.to_owned(),
                });
            }
        }
    }

    findings.sort();
    findings
}

/// Entrypoint for `cargo xtask scans`.
pub fn command(root: &Path) -> ExitCode {
    let findings = scan(root);
    if findings.is_empty() {
        println!("scans: ok");
        ExitCode::SUCCESS
    } else {
        for finding in &findings {
            println!("{finding}");
        }
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // control: NS-05
    #[test]
    fn scan_rejects_a_socket_type_outside_io_net() {
        let temp = tempfile::tempdir().expect("tempdir");
        let check_rs = temp.path().join("crates/fieldbook/src/check.rs");
        std::fs::create_dir_all(check_rs.parent().expect("parent")).expect("create_dir_all");
        let planted = "let _ = std::net::TcpStream::connect(\"127.0.0.1:1\");";
        std::fs::write(&check_rs, format!("{planted}\n")).expect("write");

        let findings = scan(temp.path());
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.file, "crates/fieldbook/src/check.rs");
        assert_eq!(f.line, 1);
        assert_eq!(f.rule, "NS-05");
        assert_eq!(f.text, planted);
        assert_eq!(
            f.to_string(),
            format!("crates/fieldbook/src/check.rs:1: NS-05: {planted}")
        );
    }

    // control: DT-05
    #[test]
    fn scan_rejects_a_model_token_in_a_deterministic_verb() {
        let temp = tempfile::tempdir().expect("tempdir");
        let check_rs = temp.path().join("crates/fieldbook/src/check.rs");
        std::fs::create_dir_all(check_rs.parent().expect("parent")).expect("create_dir_all");
        let planted = "use crate::adapters::model::Guarded;";
        std::fs::write(&check_rs, format!("{planted}\n")).expect("write");

        let findings = scan(temp.path());
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.file, "crates/fieldbook/src/check.rs");
        assert_eq!(f.line, 1);
        assert_eq!(f.rule, "DT-05");
        assert_eq!(f.text, planted);
        assert_eq!(
            f.to_string(),
            format!("crates/fieldbook/src/check.rs:1: DT-05: {planted}")
        );
    }

    #[test]
    fn scan_accepts_the_allowed_places() {
        let temp = tempfile::tempdir().expect("tempdir");
        let src = temp.path().join("crates/fieldbook/src");

        // 1. crates/fieldbook/src/io/net/client.rs (both NS-05 and DT-05 allowed)
        let net_client = src.join("io/net/client.rs");
        std::fs::create_dir_all(net_client.parent().expect("parent")).expect("create_dir_all");
        std::fs::write(
            &net_client,
            "use std::net::TcpStream;\nuse crate::adapters::model::Guarded;\n",
        )
        .expect("write");

        // 2. crates/fieldbook/src/io/net.rs (both allowed)
        let net_rs = src.join("io/net.rs");
        std::fs::write(
            &net_rs,
            "use std::net::TcpListener;\nuse crate::ModelPort;\n",
        )
        .expect("write");

        // 3. crates/fieldbook/src/adapters/model.rs (DT-05 allowed; NS-05 still applies to TcpStream)
        let adapter_model = src.join("adapters/model.rs");
        std::fs::create_dir_all(adapter_model.parent().expect("parent")).expect("create_dir_all");
        std::fs::write(
            &adapter_model,
            "pub struct ModelPort;\npub struct Guarded;\n",
        )
        .expect("write");

        // 4. crates/fieldbook/src/ask.rs (listed in ai_verb_files, so DT-05 allowed)
        let ask_rs = src.join("ask.rs");
        std::fs::write(&ask_rs, "use crate::adapters::model::Guarded;\n").expect("write");

        let findings = scan_with(temp.path(), &["ask.rs"]);
        assert!(
            findings.is_empty(),
            "expected no findings, got: {:?}",
            findings
        );
    }

    #[test]
    fn scan_of_the_real_repo_is_clean() {
        let root = crate::proc::repo_root();
        let findings = scan(&root);
        assert!(
            findings.is_empty(),
            "expected real repo to be clean, got: {:?}",
            findings
        );
    }

    #[test]
    fn scan_fails_closed_without_the_source_folder() {
        let temp = tempfile::tempdir().expect("tempdir");
        let findings = scan(temp.path());
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.file, "crates/fieldbook/src");
        assert_eq!(f.line, 0);
        assert_eq!(f.rule, "SCAN-00");
        assert!(
            f.text.contains("crates/fieldbook/src"),
            "expected text to contain crates/fieldbook/src, got: {}",
            f.text
        );
    }

    #[test]
    fn scan_fails_closed_on_an_unreadable_source_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let bad_rs = temp.path().join("crates/fieldbook/src/bad.rs");
        std::fs::create_dir_all(bad_rs.parent().expect("parent")).expect("create_dir_all");
        std::fs::write(&bad_rs, vec![0xff, 0xfe, 0xfd]).expect("write");

        let findings = scan(temp.path());
        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.file, "crates/fieldbook/src/bad.rs");
        assert_eq!(f.line, 0);
        assert_eq!(f.rule, "SCAN-00");
        assert!(
            f.text.contains("crates/fieldbook/src/bad.rs"),
            "expected text to name crates/fieldbook/src/bad.rs, got: {}",
            f.text
        );
    }

    #[test]
    fn scans_command_fails_for_a_root_without_the_source_folder() {
        let temp = tempfile::tempdir().expect("tempdir");
        let code = command(temp.path());
        assert_eq!(format!("{:?}", code), format!("{:?}", ExitCode::from(1)));
    }
}
