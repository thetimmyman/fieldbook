//! Binary import scanner (`core-policy` step 13, control `EG-01`).
//!
//! Reads the operating-system symbol/function imports of a built ELF or PE binary via `object`
//! and compares them against the checked-in allowlists (`xtask/binscan/silent.txt` and
//! `xtask/binscan/ai.txt`) and the hard-coded `NETWORK`, `PROCESS_START`, and `CLIPBOARD`
//! group tables.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use object::{Object, ObjectSymbol};

/// Network import group: forbidden in the `silent` build regardless of the allowlist.
pub const NETWORK: &[&str] = &[
    "socket",
    "connect",
    "bind",
    "listen",
    "accept",
    "accept4",
    "getaddrinfo",
    "gethostbyname",
    "sendto",
    "recvfrom",
    "WSAStartup",
    "WSASocketW",
    "GetAddrInfoW",
    "InternetOpenW",
    "WinHttpOpen",
    "URLDownloadToFileW",
];

/// Process-start import group: forbidden in both `silent` and `ai` builds regardless of the allowlist.
pub const PROCESS_START: &[&str] = &[
    "fork",
    "vfork",
    "execve",
    "execv",
    "execvp",
    "posix_spawn",
    "posix_spawnp",
    "system",
    "popen",
    "CreateProcessW",
    "CreateProcessA",
    "CreateProcessAsUserW",
    "ShellExecuteW",
    "ShellExecuteExW",
    "WinExec",
];

/// Clipboard import group: forbidden in both `silent` and `ai` builds regardless of the allowlist.
pub const CLIPBOARD: &[&str] = &[
    "OpenClipboard",
    "GetClipboardData",
    "SetClipboardData",
    "AddClipboardFormatListener",
];

/// Target build kind (`--build silent|ai`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildKind {
    Silent,
    Ai,
}

impl BuildKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "silent" => Some(Self::Silent),
            "ai" => Some(Self::Ai),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Silent => "silent",
            Self::Ai => "ai",
        }
    }
}

/// Parsed import allowlist for a build kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinList {
    pub build: BuildKind,
    pub allowed: BTreeSet<String>,
}

impl BinList {
    /// Parse a list file (one import name per line, `#` comments and blank lines ignored).
    pub fn parse(build: BuildKind, text: &str) -> Self {
        let mut allowed = BTreeSet::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            allowed.insert(trimmed.to_owned());
        }
        Self { build, allowed }
    }
}

/// Reason a binary import failed [`scan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindingKind {
    /// Import is in `NETWORK` on a `silent` build.
    ForbiddenNetwork,
    /// Import is in `PROCESS_START`.
    ForbiddenProcessStart,
    /// Import is in `CLIPBOARD`.
    ForbiddenClipboard,
    /// Import is not present in the build's allowlist.
    Unlisted,
}

/// One import violation returned by [`scan`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub name: String,
    pub kind: FindingKind,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            FindingKind::ForbiddenNetwork => {
                write!(
                    f,
                    "forbidden network import `{}` in silent build",
                    self.name
                )
            }
            FindingKind::ForbiddenProcessStart => {
                write!(f, "forbidden process-start import `{}`", self.name)
            }
            FindingKind::ForbiddenClipboard => {
                write!(f, "forbidden clipboard import `{}`", self.name)
            }
            FindingKind::Unlisted => {
                write!(f, "unlisted binary import `{}`", self.name)
            }
        }
    }
}

/// Compare imported symbol/function names against `list` and the hard-coded group bans.
#[must_use]
pub fn scan(imports: &[String], list: &BinList) -> Vec<Finding> {
    let mut findings = Vec::new();
    for name in imports {
        let s = name.as_str();
        if PROCESS_START.contains(&s) {
            findings.push(Finding {
                name: name.clone(),
                kind: FindingKind::ForbiddenProcessStart,
            });
            continue;
        }
        if CLIPBOARD.contains(&s) {
            findings.push(Finding {
                name: name.clone(),
                kind: FindingKind::ForbiddenClipboard,
            });
            continue;
        }
        if list.build == BuildKind::Silent && NETWORK.contains(&s) {
            findings.push(Finding {
                name: name.clone(),
                kind: FindingKind::ForbiddenNetwork,
            });
            continue;
        }
        if !list.allowed.contains(s) {
            findings.push(Finding {
                name: name.clone(),
                kind: FindingKind::Unlisted,
            });
        }
    }
    findings
}

/// Read the sorted, deduplicated imported function/symbol names of an ELF or PE binary at `path`.
pub fn read_imports(path: &Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    read_imports_from_bytes(&bytes).map_err(|err| format!("{}: {err}", path.display()))
}

fn read_imports_from_bytes(bytes: &[u8]) -> Result<Vec<String>, String> {
    let file = object::File::parse(bytes).map_err(|err| err.to_string())?;
    let mut names = BTreeSet::new();

    if let Ok(imports) = file.imports() {
        for imp in imports.flatten() {
            if let object::NameOrOrdinal::Name(raw_bytes) = imp.name()
                && let Ok(raw) = std::str::from_utf8(raw_bytes)
            {
                let clean = strip_symbol_version(raw);
                if !clean.is_empty() {
                    names.insert(clean.to_owned());
                }
            }
        }
    }

    for sym in file.dynamic_symbols() {
        if sym.is_undefined()
            && let Ok(raw) = sym.name()
        {
            let clean = strip_symbol_version(raw);
            if !clean.is_empty() {
                names.insert(clean.to_owned());
            }
        }
    }

    Ok(names.into_iter().collect())
}

fn strip_symbol_version(name: &str) -> &str {
    name.split('@').next().unwrap_or(name)
}

/// Run `cargo xtask binscan <exe> --build silent|ai`.
pub fn command(root: &Path, exe: &Path, build_arg: &str) -> Result<bool, String> {
    let Some(build) = BuildKind::parse(build_arg) else {
        return Err(format!(
            "invalid --build `{build_arg}` (expected `silent` or `ai`)"
        ));
    };
    let list_rel = match build {
        BuildKind::Silent => "xtask/binscan/silent.txt",
        BuildKind::Ai => "xtask/binscan/ai.txt",
    };
    let list_path = root.join(list_rel);
    let list_text =
        std::fs::read_to_string(&list_path).map_err(|e| format!("{}: {e}", list_path.display()))?;
    let list = BinList::parse(build, &list_text);

    let exe_path = if exe.is_absolute() {
        exe.to_path_buf()
    } else {
        root.join(exe)
    };
    let imports = read_imports(&exe_path)?;
    if imports.is_empty() {
        return Err(format!(
            "{}: object reader found zero imports",
            exe_path.display()
        ));
    }

    let findings = scan(&imports, &list);
    if findings.is_empty() {
        Ok(true)
    } else {
        for finding in &findings {
            eprintln!("binscan ({}) {}: {finding}", build.as_str(), exe.display());
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SILENT_TXT: &str = include_str!("../binscan/silent.txt");
    const AI_TXT: &str = include_str!("../binscan/ai.txt");

    fn sample_silent_list() -> BinList {
        BinList::parse(
            BuildKind::Silent,
            "CloseHandle\nCreateFileW\nmemcpy\nread\nwrite\n",
        )
    }

    fn sample_ai_list() -> BinList {
        BinList::parse(
            BuildKind::Ai,
            "CloseHandle\nCreateFileW\nconnect\nmemcpy\nread\nsocket\nwrite\n",
        )
    }

    #[test]
    fn a_clean_import_list_passes() {
        let list = sample_silent_list();
        let imports = vec![
            "CloseHandle".to_owned(),
            "memcpy".to_owned(),
            "read".to_owned(),
        ];
        assert!(scan(&imports, &list).is_empty());
    }

    #[test]
    fn a_network_name_in_the_silent_build_fails() {
        // Even if a network name were added to silent's BinList, `scan` fails it for Silent.
        let mut list = sample_silent_list();
        list.allowed.insert("socket".to_owned());
        let imports = vec!["read".to_owned(), "socket".to_owned()];
        let findings = scan(&imports, &list);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].name, "socket");
        assert_eq!(findings[0].kind, FindingKind::ForbiddenNetwork);
    }

    #[test]
    fn a_process_start_name_in_either_build_fails() {
        let mut silent = sample_silent_list();
        silent.allowed.insert("fork".to_owned());
        let mut ai = sample_ai_list();
        ai.allowed.insert("CreateProcessW".to_owned());

        let f_silent = scan(&["fork".to_owned()], &silent);
        assert_eq!(f_silent.len(), 1);
        assert_eq!(f_silent[0].kind, FindingKind::ForbiddenProcessStart);

        let f_ai = scan(&["CreateProcessW".to_owned()], &ai);
        assert_eq!(f_ai.len(), 1);
        assert_eq!(f_ai[0].kind, FindingKind::ForbiddenProcessStart);
    }

    #[test]
    fn a_clipboard_name_in_either_build_fails() {
        let mut silent = sample_silent_list();
        silent.allowed.insert("OpenClipboard".to_owned());
        let mut ai = sample_ai_list();
        ai.allowed.insert("GetClipboardData".to_owned());

        let f_silent = scan(&["OpenClipboard".to_owned()], &silent);
        assert_eq!(f_silent.len(), 1);
        assert_eq!(f_silent[0].kind, FindingKind::ForbiddenClipboard);

        let f_ai = scan(&["GetClipboardData".to_owned()], &ai);
        assert_eq!(f_ai.len(), 1);
        assert_eq!(f_ai[0].kind, FindingKind::ForbiddenClipboard);
    }

    #[test]
    fn an_unlisted_name_fails_and_is_printed() {
        let list = sample_silent_list();
        let imports = vec!["read".to_owned(), "SomeSurpriseOsCall".to_owned()];
        let findings = scan(&imports, &list);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].name, "SomeSurpriseOsCall");
        assert_eq!(findings[0].kind, FindingKind::Unlisted);
        assert!(
            findings[0].to_string().contains("SomeSurpriseOsCall"),
            "finding display must print the unlisted name"
        );
    }

    #[test]
    fn checked_in_lists_keep_unknown_and_forbidden_imports_failing() {
        for (build, text) in [(BuildKind::Silent, SILENT_TXT), (BuildKind::Ai, AI_TXT)] {
            let list = BinList::parse(build, text);
            for (name, kind) in [
                ("CreateProcessW", FindingKind::ForbiddenProcessStart),
                ("OpenClipboard", FindingKind::ForbiddenClipboard),
                ("SomeSurpriseOsCall", FindingKind::Unlisted),
            ] {
                assert_eq!(
                    scan(&[name.to_owned()], &list),
                    vec![Finding {
                        name: name.to_owned(),
                        kind,
                    }]
                );
            }
            if build == BuildKind::Silent {
                for name in ["socket", "WSAStartup"] {
                    assert_eq!(
                        scan(&[name.to_owned()], &list),
                        vec![Finding {
                            name: name.to_owned(),
                            kind: FindingKind::ForbiddenNetwork,
                        }]
                    );
                }
            }
        }
    }

    #[test]
    fn the_lists_are_sorted_and_have_no_duplicates() {
        for (label, raw) in [("silent.txt", SILENT_TXT), ("ai.txt", AI_TXT)] {
            let entries: Vec<&str> = raw
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            assert!(!entries.is_empty(), "{label} must not be empty");
            for pair in entries.windows(2) {
                assert!(
                    pair[0].as_bytes() < pair[1].as_bytes(),
                    "{label} is not strictly byte-sorted or has duplicates: `{}` vs `{}`",
                    pair[0],
                    pair[1]
                );
            }
        }
    }

    #[test]
    fn the_groups_are_not_on_the_silent_list() {
        let silent = BinList::parse(BuildKind::Silent, SILENT_TXT);
        for &name in NETWORK.iter().chain(PROCESS_START).chain(CLIPBOARD) {
            assert!(
                !silent.allowed.contains(name),
                "silent.txt must not contain group symbol `{name}`"
            );
        }

        let ai = BinList::parse(BuildKind::Ai, AI_TXT);
        for &name in PROCESS_START.iter().chain(CLIPBOARD) {
            assert!(
                !ai.allowed.contains(name),
                "ai.txt must not contain process-start or clipboard symbol `{name}`"
            );
        }

        // Verify ai.txt is silent.txt plus only symbols from NETWORK.
        for sym in &ai.allowed {
            assert!(
                silent.allowed.contains(sym.as_str()) || NETWORK.contains(&sym.as_str()),
                "ai.txt contains `{sym}` which is in neither silent.txt nor NETWORK"
            );
        }
    }

    // control: EG-01
    #[test]
    fn imports_of_a_real_binary_are_read() {
        let exe = std::env::current_exe().expect("current_exe");
        let imports = read_imports(&exe).expect("read_imports on current_exe");
        assert!(
            !imports.is_empty(),
            "imports of test binary {} must not be empty",
            exe.display()
        );
    }
}
