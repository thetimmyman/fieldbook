//! The pinned developer tools: reading the pins, `doctor` and `setup`.

use crate::proc::{capture, output, run_visible};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The outcome of checking an installed poppler version against its pin.
#[derive(Debug, PartialEq, Eq)]
pub enum PinCheck {
    /// The installed version matches the pin.
    Match,
    /// The installed version differs from the pin.
    Differs(String),
}

/// Check an installed poppler / pdftotext version against the pinned poppler version.
pub fn poppler_pin_check(pin: &str, found: &str) -> PinCheck {
    let match_found = match (Version::parse(pin), Version::parse(found)) {
        (Some(p), Some(f)) => p == f,
        _ => pin == found,
    };
    if match_found {
        PinCheck::Match
    } else {
        PinCheck::Differs(format!(
            "pdftotext {found} differs from the pinned poppler {pin}; extracting anyway"
        ))
    }
}

/// The version of poppler pinned in `.config/tools.toml`.
pub fn poppler_pin(root: &Path) -> Result<String, String> {
    let pins = Pins::read(&root.join(".config/tools.toml"))?;
    pins.get("poppler", "version").map(str::to_owned)
}

/// A version such as `1.99.1`, compared number by number. Missing parts count as zero.
#[derive(Debug, Clone)]
pub struct Version(Vec<u64>);

impl Version {
    /// `1.99.1` or `v8.30.1`; at least two numeric parts and nothing else.
    pub fn parse(text: &str) -> Option<Self> {
        let parts: Option<Vec<u64>> = text
            .trim()
            .trim_start_matches('v')
            .split('.')
            .map(|part| {
                if part.bytes().all(|b| b.is_ascii_digit()) {
                    part.parse().ok()
                } else {
                    None
                }
            })
            .collect();
        parts.filter(|parts| parts.len() >= 2).map(Self)
    }

    /// The first token of a tool's `--version` output that is a version.
    pub fn find_in(output: &str) -> Option<Self> {
        output
            .split_whitespace()
            .find_map(|token| Self::parse(token.trim_matches(['(', ')', ','])))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (0..self.0.len().max(other.0.len()))
            .map(|i| {
                self.0
                    .get(i)
                    .unwrap_or(&0)
                    .cmp(other.0.get(i).unwrap_or(&0))
            })
            .find(|ordering| ordering.is_ne())
            .unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}
impl Eq for Version {}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let parts: Vec<String> = self.0.iter().map(u64::to_string).collect();
        f.write_str(&parts.join("."))
    }
}

/// The `[section]` and `key = "value"` lines of a TOML file. Enough for the two files
/// read here; a line it cannot read is an error, never skipped.
pub struct Pins(BTreeMap<String, BTreeMap<String, String>>);

impl Pins {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut current = String::new();
        for (number, line) in text
            .lines()
            .enumerate()
            .map(|(i, line)| (i + 1, line.trim()))
        {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                current = name.trim().to_owned();
                continue;
            }
            let (key, rest) = line
                .split_once('=')
                .ok_or(format!("line {number}: expected `key = value`"))?;
            let rest = rest.trim();
            let value = match rest.strip_prefix('"') {
                Some(quoted) => {
                    quoted
                        .split_once('"')
                        .ok_or(format!("line {number}: unclosed quote"))?
                        .0
                }
                None => rest.split('#').next().unwrap_or(rest).trim(),
            };
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_owned(), value.to_owned());
        }
        Ok(Self(sections))
    }

    pub fn get(&self, section: &str, key: &str) -> Result<&str, String> {
        self.0
            .get(section)
            .and_then(|keys| keys.get(key))
            .map(String::as_str)
            .ok_or(format!("no `{key}` under [{section}]"))
    }

    #[cfg(test)]
    pub fn keys(&self, section: &str) -> Vec<&str> {
        self.0
            .get(section)
            .map(|keys| keys.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    fn read(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        Self::parse(&text).map_err(|error| format!("{}: {error}", path.display()))
    }
}

/// A tool to check: how to run it, and which pin it answers to.
struct Tool {
    /// The pin key in `.config/tools.toml` (`rustc` answers to `rust-toolchain.toml`).
    key: &'static str,
    /// The program to find on PATH.
    binary: &'static str,
    /// The command that prints its version.
    probe: &'static [&'static str],
}

const TOOLS: [Tool; 6] = [
    Tool {
        key: "rustc",
        binary: "rustc",
        probe: &["rustc", "--version"],
    },
    Tool {
        key: "just",
        binary: "just",
        probe: &["just", "--version"],
    },
    Tool {
        key: "cargo-nextest",
        binary: "cargo-nextest",
        probe: &["cargo", "nextest", "--version"],
    },
    Tool {
        key: "cargo-deny",
        binary: "cargo-deny",
        probe: &["cargo", "deny", "--version"],
    },
    Tool {
        key: "typos-cli",
        binary: "typos",
        probe: &["typos", "--version"],
    },
    Tool {
        key: "gitleaks",
        binary: "gitleaks",
        probe: &["gitleaks", "version"],
    },
];

/// The version a tool is pinned to.
fn pinned(key: &str, tools: &Pins, toolchain: &Pins) -> Result<Version, String> {
    let text = match key {
        "rustc" => toolchain.get("toolchain", "channel")?,
        "gitleaks" => tools.get("gitleaks", "version")?,
        _ => tools.get("cargo", key)?,
    };
    Version::parse(text).ok_or(format!("the pin for {key} (`{text}`) is not a version"))
}

fn installed(tool: &Tool, root: &Path) -> Result<Version, String> {
    let (program, args) = tool.probe.split_first().ok_or("an empty probe")?;
    let text = capture(program, args, root)?;
    Version::find_in(&text).ok_or(format!("cannot read a version from `{}`", text.trim()))
}

fn cargo_bin() -> PathBuf {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .unwrap_or_default();
    home.join("bin")
}

/// Where `binary` is found on PATH, first match first.
fn on_path(binary: &str) -> Vec<PathBuf> {
    let paths = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&paths)
        .map(|dir| dir.join(binary))
        .filter(|candidate| candidate.is_file())
        .collect()
}

/// What to do about a tool that is missing or too old.
fn path_hint(binary: &str) -> String {
    let wanted = cargo_bin().join(binary);
    match on_path(binary).first() {
        None => format!(
            "`{binary}` is not on PATH: run `just setup`, and put {} on PATH",
            cargo_bin().display()
        ),
        Some(first) if wanted.is_file() && *first != wanted => {
            format!(
                "{} shadows {}: put {} first on PATH",
                first.display(),
                wanted.display(),
                cargo_bin().display()
            )
        }
        Some(first) => format!(
            "found {}: run `just setup` to install the pinned version",
            first.display()
        ),
    }
}

/// Whether an installed version meets its pin. CI (`exact`) requires the pin itself, so a
/// newer tool fails there too; a local run accepts the pin or newer.
fn version_ok(found: &Version, want: &Version, exact: bool) -> bool {
    if exact { found == want } else { found >= want }
}

/// Check the tools. Local runs accept the pin or newer; CI (`exact`) requires the pin.
pub fn doctor(root: &Path, exact: bool) -> Result<bool, String> {
    let tools = Pins::read(&root.join(".config/tools.toml"))?;
    let toolchain = Pins::read(&root.join("rust-toolchain.toml"))?;
    let mut all_good = true;
    println!(
        "doctor: {} versions against .config/tools.toml and rust-toolchain.toml",
        if exact { "exact" } else { "minimum" }
    );
    for tool in &TOOLS {
        let want = pinned(tool.key, &tools, &toolchain)?;
        match installed(tool, root) {
            Ok(found) if version_ok(&found, &want, exact) => {
                println!("ok    {:<14} {found} (pinned {want})", tool.key);
            }
            Ok(found) => {
                all_good = false;
                println!("FAIL  {:<14} {found} (pinned {want})", tool.key);
                println!("      {}", path_hint(tool.binary));
            }
            Err(reason) => {
                all_good = false;
                println!("FAIL  {:<14} {reason}", tool.key);
                println!("      {}", path_hint(tool.binary));
            }
        }
    }
    if !exact {
        let hooks =
            capture("git", &["config", "--get", "core.hooksPath"], root).unwrap_or_default();
        if hooks.trim() == ".githooks" {
            println!("ok    git hooks      enabled");
        } else {
            println!("warn  git hooks      not enabled: run `just setup`");
        }
        let pin = tools.get("poppler", "version")?;
        let want = Version::parse(pin)
            .ok_or_else(|| format!("the pin for poppler (`{pin}`) is not a version"))?;
        let found = output("pdftotext", &["-v"], root).ok().and_then(|out| {
            let err_text = String::from_utf8_lossy(&out.stderr);
            Version::find_in(&err_text).or_else(|| {
                let out_text = String::from_utf8_lossy(&out.stdout);
                Version::find_in(&out_text)
            })
        });
        match found {
            Some(found) if version_ok(&found, &want, false) => {
                println!("ok    pdftotext      {found} (pinned {pin})");
            }
            Some(found) => {
                println!(
                    "warn  pdftotext      {found} is older than the pinned {pin}: install poppler {pin} or newer"
                );
            }
            None => {
                println!(
                    "warn  pdftotext      not found: install poppler (it provides pdftotext, needed only for `cargo xtask extract` of PDFs)"
                );
            }
        }
    }
    Ok(all_good)
}

/// Install the pinned tools (all, or those named) and enable the git hooks.
pub fn setup(root: &Path, args: &[&str]) -> Result<bool, String> {
    if let Some(flag) = args
        .iter()
        .find(|arg| arg.starts_with('-') && **arg != "--no-hooks")
    {
        return Err(format!("unknown option {flag}"));
    }
    let hooks = !args.contains(&"--no-hooks");
    let named: Vec<&str> = args
        .iter()
        .copied()
        .filter(|arg| !arg.starts_with('-'))
        .collect();
    let tools = Pins::read(&root.join(".config/tools.toml"))?;
    let toolchain = Pins::read(&root.join("rust-toolchain.toml"))?;
    if let Some(unknown) = named.iter().find(|name| {
        !TOOLS
            .iter()
            .any(|tool| tool.key == **name && tool.key != "rustc")
    }) {
        return Err(format!("`{unknown}` is not an installable pinned tool"));
    }
    let outside = std::env::temp_dir();
    for tool in TOOLS
        .iter()
        .filter(|tool| tool.key != "rustc" && (named.is_empty() || named.contains(&tool.key)))
    {
        let want = pinned(tool.key, &tools, &toolchain)?;
        if installed(tool, root).is_ok_and(|found| found >= want) {
            println!("ok       {} is already at {want} or newer", tool.key);
            continue;
        }
        println!("install  {} {want}", tool.key);
        if tool.key == "gitleaks" {
            install_gitleaks(&tools, &want.to_string())?;
        } else {
            // From outside the repository, so its toolchain pin and cargo config do not apply.
            let spec = format!("{}@{want}", tool.key);
            if !run_visible("cargo", &["install", "--locked", &spec], &outside)? {
                return Err(format!("cargo install {spec} failed"));
            }
        }
    }
    if hooks {
        capture("git", &["config", "core.hooksPath", ".githooks"], root)?;
        println!("ok       git hooks enabled (core.hooksPath = .githooks)");
    }
    Ok(true)
}

fn install_gitleaks(tools: &Pins, version: &str) -> Result<(), String> {
    if std::env::consts::OS != "linux" || std::env::consts::ARCH != "x86_64" {
        return Err(format!(
            "no gitleaks checksum is pinned for this platform: install gitleaks {version} by hand"
        ));
    }
    let want_hash = tools.get("gitleaks", "linux-x64-sha256")?;
    let work = std::env::temp_dir().join(format!("fieldbook-setup-{}", std::process::id()));
    std::fs::create_dir_all(&work).map_err(|error| error.to_string())?;
    let result = download_and_install(&work, version, want_hash);
    let _ = std::fs::remove_dir_all(&work);
    result
}

fn download_and_install(work: &Path, version: &str, want_hash: &str) -> Result<(), String> {
    let url = format!(
        "https://github.com/gitleaks/gitleaks/releases/download/v{version}/gitleaks_{version}_linux_x64.tar.gz"
    );
    let archive = work.join("gitleaks.tar.gz");
    let archive_text = archive.to_string_lossy().into_owned();
    capture(
        "curl",
        &[
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--output",
            &archive_text,
            &url,
        ],
        work,
    )?;
    let sums = capture("sha256sum", &[&archive_text], work)?;
    let got = sums.split_whitespace().next().unwrap_or_default();
    if got != want_hash {
        return Err(format!(
            "gitleaks checksum mismatch: pinned {want_hash}, downloaded {got}; nothing was installed"
        ));
    }
    capture(
        "tar",
        &[
            "-xzf",
            &archive_text,
            "-C",
            &work.to_string_lossy(),
            "gitleaks",
        ],
        work,
    )?;
    let bin = cargo_bin();
    std::fs::create_dir_all(&bin).map_err(|error| error.to_string())?;
    let target = bin.join("gitleaks");
    let ok = output(
        "install",
        &[
            "-m",
            "0755",
            &work.join("gitleaks").to_string_lossy(),
            &target.to_string_lossy(),
        ],
        work,
    )?;
    if ok.status.success() {
        Ok(())
    } else {
        Err(format!("cannot install {}", target.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        PinCheck, Pins, TOOLS, Version, installed, pinned, poppler_pin, poppler_pin_check,
        version_ok,
    };
    use std::path::Path;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("a version")
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(v("1.100") > v("1.99"));
        assert!(v("1.100.0") > v("1.99.9"));
        assert!(v("0.9.148") > v("0.9.99"));
        assert_eq!(v("1.99"), v("1.99.0"));
        assert!(v("1.58.0") < v("1.59.0"));
    }

    // control: DP-06
    #[test]
    fn doctor_fails_when_a_pinned_version_differs() {
        // CI runs exact: a newer tool fails as well as an older one.
        assert!(version_ok(&v("0.9.148"), &v("0.9.148"), true));
        assert!(!version_ok(&v("0.9.149"), &v("0.9.148"), true));
        assert!(!version_ok(&v("0.9.147"), &v("0.9.148"), true));
        // A local run accepts the pin or newer and never older.
        assert!(version_ok(&v("0.9.149"), &v("0.9.148"), false));
        assert!(!version_ok(&v("0.9.147"), &v("0.9.148"), false));
    }

    #[test]
    fn versions_parse_only_dotted_numbers() {
        assert_eq!(v("v8.30.1").to_string(), "8.30.1");
        for bad in ["", "7", "1.x", "1..2", "nightly-2026-10-08", "typos-cli"] {
            assert!(Version::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn versions_are_found_in_each_tools_output() {
        for (text, want) in [
            ("rustc 1.99.1 (58cf8f9c3 2026-10-13)\n", "1.99.1"),
            ("just 1.58.0\n", "1.58.0"),
            ("cargo-nextest 0.9.148 (a1b2c3d4e 2026-10-08)\n", "0.9.148"),
            ("cargo-deny 0.20.2\n", "0.20.2"),
            ("typos-cli 1.51.1\n", "1.51.1"),
            ("8.30.1\n", "8.30.1"),
        ] {
            assert_eq!(
                Version::find_in(text)
                    .map(|found| found.to_string())
                    .as_deref(),
                Some(want),
                "{text}"
            );
        }
    }

    #[test]
    fn pins_read_sections_comments_and_arrays() {
        let pins = Pins::parse(
            "# top\n[a]\nx = \"1.0\" # note\nlist = [\"p\", \"q\"]\n\n[b]\ny = \"2\"\n",
        )
        .expect("parses");
        assert_eq!(pins.get("a", "x"), Ok("1.0"));
        assert_eq!(pins.get("a", "list"), Ok("[\"p\", \"q\"]"));
        assert_eq!(pins.get("b", "y"), Ok("2"));
        assert!(pins.get("b", "x").is_err());
        assert!(Pins::parse("[a]\nnot a pin\n").is_err());
        assert!(Pins::parse("[a]\nx = \"open\n").is_err());
    }

    fn root() -> &'static Path {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the repository root")
    }

    #[test]
    fn every_checked_tool_has_a_readable_pin() {
        let tools = Pins::read(&root().join(".config/tools.toml")).expect("tools.toml");
        let toolchain =
            Pins::read(&root().join("rust-toolchain.toml")).expect("rust-toolchain.toml");
        for tool in &TOOLS {
            assert!(pinned(tool.key, &tools, &toolchain).is_ok(), "{}", tool.key);
        }
        assert!(
            installed(&TOOLS[0], root()).is_ok(),
            "rustc reports a version"
        );
    }

    /// The CI workflow repeats the tool versions; this keeps the two from drifting.
    #[test]
    fn ci_installs_the_pinned_versions() {
        let tools = Pins::read(&root().join(".config/tools.toml")).expect("tools.toml");
        let workflow =
            std::fs::read_to_string(root().join(".github/workflows/ci.yml")).expect("ci.yml");
        for key in tools.keys("cargo") {
            let action_name = if key == "typos-cli" { "typos" } else { key };
            let spec = format!("{action_name}@{}", tools.get("cargo", key).expect("pin"));
            assert!(workflow.contains(&spec), "ci.yml does not install {spec}");
        }
    }

    #[test]
    fn the_poppler_pin_is_readable() {
        let pin = poppler_pin(root()).expect("poppler pin");
        assert!(Version::parse(&pin).is_some());
    }

    #[test]
    fn a_differing_version_is_a_warning() {
        let check = poppler_pin_check("26.10.0", "25.01.0");
        match check {
            PinCheck::Differs(msg) => {
                assert!(msg.contains("25.01.0"));
                assert!(msg.contains("26.10.0"));
            }
            PinCheck::Match => panic!("expected differs"),
        }
        assert_eq!(poppler_pin_check("26.10.0", "26.10.0"), PinCheck::Match);
        assert!(matches!(
            poppler_pin_check("26.10.0", "27.01.0"),
            PinCheck::Differs(_)
        ));
        assert!(matches!(
            poppler_pin_check("26.10.0", "invalid-version"),
            PinCheck::Differs(_)
        ));
    }
}
