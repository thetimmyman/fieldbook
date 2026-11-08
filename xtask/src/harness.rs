//! `cargo xtask harness [--edition DIR]`: open every page and every renderable original of an
//! edition in headless Edge with a net-log (`edge-harness`, AD-08; controls NS-03, WB-01 and the
//! harness half of ED-02).
//!
//! Edge is the target. Where no Edge is installed (the Linux home machine and the self-hosted
//! runner), the harness falls back to Chromium and names the browser that ran, in its first
//! line, its summary line and `network-silence-summary.txt`.
//!
//! The edition is never changed: the harness copies it to `out/harness/edition/`, copies
//! `selftest.js` once to the root of that copy and injects one script element as the first child
//! of `<head>` of each copied HTML page. Renderable `.txt` originals are opened unchanged from the
//! `originals` folder beside the edition.

use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

/// The AD-08 browser options. The harness adds only `--dump-dom` and `--log-net-log=<file>`, and
/// never `--allow-file-access-from-files`.
pub(crate) const EDGE_FLAGS: [&str; 6] = [
    "--headless=new",
    "--disable-background-networking",
    "--disable-component-update",
    "--disable-sync",
    "--no-default-browser-check",
    "--disable-features=OptimizationHints,MediaRouter",
];

/// The harness-owned self-test script, copied once to the root of each harness copy.
const SELFTEST_JS: &str = include_str!("../harness/selftest.js");
/// The negative page: a remote script under the edition's policy.
const NEGATIVE_HTML: &str = include_str!("../harness/negative.html");
/// The file name of the self-test script inside a harness copy.
const SELFTEST_NAME: &str = "selftest.js";
/// The id of the element the self-test writes its result into.
const SELFTEST_MARKER: &str = "<pre id=\"fb-selftest\">";
/// How long one page may take.
const PAGE_LIMIT: Duration = Duration::from_secs(60);

/// Which browser ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserKind {
    /// Microsoft Edge, the target.
    Edge,
    /// Chromium, the fallback where Edge is not installed.
    Chromium,
}

impl BrowserKind {
    /// The short name of the first and summary lines.
    fn short(self) -> &'static str {
        match self {
            Self::Edge => "edge",
            Self::Chromium => "chromium",
        }
    }

    /// The product name of the `browser:` line of the summary file.
    fn product(self) -> &'static str {
        match self {
            Self::Edge => "Microsoft Edge",
            Self::Chromium => "Chromium",
        }
    }
}

/// A browser found on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Browser {
    pub(crate) kind: BrowserKind,
    pub(crate) path: PathBuf,
}

/// A browser and the dotted version it reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BrowserId {
    pub(crate) kind: BrowserKind,
    pub(crate) version: String,
}

fn edge_names(windows: bool) -> &'static [&'static str] {
    if windows {
        &["msedge.exe"]
    } else {
        &["microsoft-edge", "microsoft-edge-stable"]
    }
}

fn chromium_names(windows: bool) -> &'static [&'static str] {
    if windows {
        &["chromium.exe"]
    } else {
        &["chromium", "chromium-browser"]
    }
}

/// Picks the browser: Edge first, then Chromium. `lookup` resolves a program name to a path.
pub(crate) fn pick_browser(
    windows: bool,
    lookup: &dyn Fn(&str) -> Option<PathBuf>,
) -> Result<Browser, String> {
    let first = |names: &[&str]| names.iter().find_map(|name| lookup(name));
    if let Some(path) = first(edge_names(windows)) {
        return Ok(Browser {
            kind: BrowserKind::Edge,
            path,
        });
    }
    if let Some(path) = first(chromium_names(windows)) {
        return Ok(Browser {
            kind: BrowserKind::Chromium,
            path,
        });
    }
    Err(format!(
        "Edge not found ({}), and no Chromium fallback ({})",
        edge_names(windows).join(", "),
        chromium_names(windows).join(", ")
    ))
}

/// Finds Edge alone (`microsoft-edge` or `microsoft-edge-stable` on Linux, `msedge.exe` on
/// Windows), for the drill steps that need real Edge.
pub(crate) fn find_edge() -> Result<PathBuf, String> {
    let windows = cfg!(windows);
    edge_names(windows)
        .iter()
        .find_map(|name| lookup_program(name))
        .ok_or_else(|| "Edge not found".to_owned())
}

/// Finds the browser the harness runs: Edge, else Chromium.
pub(crate) fn find_browser() -> Result<Browser, String> {
    if let Ok(path) = find_edge() {
        return Ok(Browser {
            kind: BrowserKind::Edge,
            path,
        });
    }
    pick_browser(cfg!(windows), &lookup_program)
}

/// Resolves `name` on `PATH`, and on Windows also in the standard Edge folders.
fn lookup_program(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();
    if cfg!(windows) && name == "msedge.exe" {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(base) = std::env::var_os(var) {
                dirs.push(PathBuf::from(base).join("Microsoft/Edge/Application"));
            }
        }
    }
    dirs.into_iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// The dotted version in the `--version` output of a browser
/// (`Microsoft Edge 131.0.2903.51`, `Chromium 156.0.8078.79 Arch Linux`).
pub(crate) fn parse_version(kind: BrowserKind, output: &str) -> Result<BrowserId, String> {
    output
        .split_whitespace()
        .find(|token| {
            token.starts_with(|c: char| c.is_ascii_digit())
                && token.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
        .map(|version| BrowserId {
            kind,
            version: version.to_owned(),
        })
        .ok_or_else(|| format!("no version in `--version` output `{}`", output.trim()))
}

/// The first line of every run: `harness: edge <version>` or `harness: chromium <version>`.
pub(crate) fn first_line(id: &BrowserId) -> String {
    format!("harness: {} {}", id.kind.short(), id.version)
}

/// The summary line of a passing run.
pub(crate) fn summary_line(id: &BrowserId, pages: usize, files: usize) -> String {
    format!(
        "harness: {pages} pages, {files} files, 0 non-file requests, negative page blocked ({} {})",
        id.kind.short(),
        id.version
    )
}

/// The reference from the copied page `page` to the one harness-owned self-test at the root of
/// the copy `root`: `selftest.js` at the root, `../selftest.js` one folder down, and so on.
pub(crate) fn selftest_src(page: &Path, root: &Path) -> Result<String, String> {
    let relative = page.strip_prefix(root).map_err(|_| {
        format!(
            "page {} is outside the harness root {}",
            page.display(),
            root.display()
        )
    })?;
    let mut depth = 0_usize;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    for component in parent.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            _ => {
                return Err(format!(
                    "page {} is outside the harness root {}",
                    page.display(),
                    root.display()
                ));
            }
        }
    }
    if relative.file_name().is_none() {
        return Err(format!("page {} names no file", page.display()));
    }
    Ok(format!("{}{SELFTEST_NAME}", "../".repeat(depth)))
}

/// `html` with one `<script src="{src}"></script>` as the first child of `<head>`.
fn inject_script(html: &str, src: &str) -> Result<String, String> {
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower[from..].find("<head") {
        let start = from + found;
        let after = lower.as_bytes().get(start + 5).copied();
        if matches!(after, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r')) {
            let close = lower[start..]
                .find('>')
                .map(|offset| start + offset + 1)
                .ok_or("unterminated <head> tag")?;
            let mut out = String::with_capacity(html.len() + src.len() + 32);
            out.push_str(&html[..close]);
            let _ = write!(out, "<script src=\"{src}\"></script>");
            out.push_str(&html[close..]);
            return Ok(out);
        }
        from = start + 5;
    }
    Err("no <head> element".to_owned())
}

/// Copies `edition` to `copy` (replacing it), writes the self-test at the copy's root and injects
/// its script element into every copied HTML page. Returns the copied HTML pages, sorted.
pub(crate) fn prepare_copy(edition: &Path, copy: &Path) -> Result<Vec<PathBuf>, String> {
    if copy.exists() {
        fs::remove_dir_all(copy)
            .map_err(|error| format!("cannot clear {}: {error}", copy.display()))?;
    }
    let mut pages = Vec::new();
    copy_tree(edition, copy, &mut pages)?;
    fs::write(copy.join(SELFTEST_NAME), SELFTEST_JS)
        .map_err(|error| format!("cannot write the self-test: {error}"))?;
    pages.sort();
    for page in &pages {
        let src = selftest_src(page, copy)?;
        let html = fs::read_to_string(page)
            .map_err(|error| format!("cannot read {}: {error}", page.display()))?;
        let injected = inject_script(&html, &src)
            .map_err(|error| format!("page {}: {error}", page.display()))?;
        fs::write(page, injected)
            .map_err(|error| format!("cannot write {}: {error}", page.display()))?;
    }
    Ok(pages)
}

fn copy_tree(from: &Path, to: &Path, pages: &mut Vec<PathBuf>) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|error| format!("cannot create {}: {error}", to.display()))?;
    let entries =
        fs::read_dir(from).map_err(|error| format!("cannot read {}: {error}", from.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read {}: {error}", from.display()))?;
        let source = entry.path();
        let target = to.join(entry.file_name());
        let kind = entry
            .file_type()
            .map_err(|error| format!("cannot read {}: {error}", source.display()))?;
        if kind.is_dir() {
            copy_tree(&source, &target, pages)?;
        } else if kind.is_file() {
            fs::copy(&source, &target)
                .map_err(|error| format!("cannot copy {}: {error}", source.display()))?;
            if is_html(&target) {
                pages.push(target);
            }
        } else {
            return Err(format!(
                "{} is neither a file nor a folder",
                source.display()
            ));
        }
    }
    Ok(())
}

fn is_html(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
}

/// True for a renderable original shipped as text (`<version-id>.<ext>.txt`, ED-02).
fn is_renderable_original(name: &str) -> bool {
    name.strip_suffix(".txt")
        .and_then(|stem| stem.split_once('.'))
        .is_some_and(|(id, ext)| {
            !ext.is_empty() && fieldbook_render::source::shipped_name(id, ext) == name
        })
}

/// The renderable `.txt` originals of `originals`, sorted. A missing folder holds none.
fn renderable_originals(originals: &Path) -> Result<Vec<PathBuf>, String> {
    if !originals.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(originals)
        .map_err(|error| format!("cannot read {}: {error}", originals.display()))?;
    let mut out = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("cannot read {}: {error}", originals.display()))?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_renderable_original)
        {
            out.push(path);
        }
    }
    out.sort();
    Ok(out)
}

/// What the net-log of one load shows.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct NetFindings {
    /// Requests from a `file:` page to a `file:` URL.
    pub(crate) file: usize,
    /// URLs a `file:` page requested that are not `file:`.
    pub(crate) non_file: Vec<String>,
}

#[derive(Deserialize)]
struct NetLog {
    constants: NetConstants,
    #[serde(default)]
    events: Vec<NetEvent>,
}

#[derive(Deserialize)]
struct NetConstants {
    #[serde(rename = "logEventTypes")]
    log_event_types: std::collections::BTreeMap<String, u64>,
}

#[derive(Deserialize)]
struct NetEvent {
    #[serde(rename = "type")]
    kind: u64,
    #[serde(default)]
    params: Option<serde_json::Value>,
}

/// Reads a net-log: every request a `file:` page started. The browser's own traffic (initiator
/// `not an origin`, a network isolation key that is not `file:`) is ignored.
pub(crate) fn read_netlog(text: &str) -> Result<NetFindings, String> {
    let log: NetLog =
        serde_json::from_str(text).map_err(|error| format!("unreadable net-log: {error}"))?;
    let start_job = log
        .constants
        .log_event_types
        .get("URL_REQUEST_START_JOB")
        .copied()
        .ok_or("net-log has no URL_REQUEST_START_JOB event type")?;
    let mut findings = NetFindings::default();
    for event in log.events.iter().filter(|event| event.kind == start_job) {
        let Some(params) = &event.params else {
            continue;
        };
        let field = |name: &str| params.get(name).and_then(|v| v.as_str()).unwrap_or("");
        let url = field("url");
        if url.is_empty() {
            continue;
        }
        let initiator = field("initiator");
        let from_file_page = initiator.starts_with("file:")
            || field("network_isolation_key").starts_with("file:")
            || field("site_for_cookies").contains("site=file:");
        if !from_file_page {
            continue;
        }
        if url.starts_with("file:") {
            findings.file += 1;
        } else {
            findings.non_file.push(url.to_owned());
        }
    }
    Ok(findings)
}

/// The failures the net-log of `page` shows: one per request outside `file:`.
pub(crate) fn netlog_failures(page: &str, findings: &NetFindings) -> Vec<String> {
    findings
        .non_file
        .iter()
        .map(|url| format!("{page}: non-file request to {url}"))
        .collect()
}

/// One Content Security Policy violation the self-test saw.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Violation {
    pub(crate) directive: String,
    pub(crate) blocked: String,
}

/// One resource entry the self-test saw.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Resource {
    pub(crate) name: String,
    pub(crate) initiator: String,
}

/// The self-test result of one HTML page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct Selftest {
    pub(crate) violations: Vec<Violation>,
    pub(crate) resources: Vec<Resource>,
}

fn unescape_html(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", "\u{a0}")
        .replace("&amp;", "&")
}

/// The self-test result in a dumped DOM, or an error naming `page`.
pub(crate) fn read_selftest(page: &str, dom: &str) -> Result<Selftest, String> {
    let start = dom
        .find(SELFTEST_MARKER)
        .map(|at| at + SELFTEST_MARKER.len())
        .ok_or_else(|| format!("{page}: missing selftest result"))?;
    let end = dom[start..]
        .find("</pre>")
        .map(|at| start + at)
        .ok_or_else(|| format!("{page}: unterminated selftest result"))?;
    serde_json::from_str(&unescape_html(&dom[start..end]))
        .map_err(|error| format!("{page}: unreadable selftest result: {error}"))
}

/// Judges one HTML page by its dumped DOM: a missing result or any violation fails.
pub(crate) fn judge_html_page(page: &str, dom: &str) -> Result<Selftest, Vec<String>> {
    let selftest = read_selftest(page, dom).map_err(|error| vec![error])?;
    if selftest.violations.is_empty() {
        Ok(selftest)
    } else {
        Err(selftest
            .violations
            .iter()
            .map(|v| {
                format!(
                    "{page}: CSP violation of {} by {}",
                    v.directive,
                    if v.blocked.is_empty() {
                        "an inline resource"
                    } else {
                        &v.blocked
                    }
                )
            })
            .collect())
    }
}

/// What the negative page showed: requests blocked and requests reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Negative {
    pub(crate) blocked: usize,
    pub(crate) reported: usize,
}

/// Judges the negative page: a remote load must be reported by the self-test and absent from
/// the net-log. A negative page that reports nothing fails.
pub(crate) fn judge_negative(dom: &str, net: &NetFindings) -> Result<Negative, Vec<String>> {
    let mut failures: Vec<String> = net
        .non_file
        .iter()
        .map(|url| format!("negative page: request to {url} started"))
        .collect();
    let reported: Vec<Violation> = match read_selftest("negative page", dom) {
        Ok(selftest) => selftest
            .violations
            .into_iter()
            .filter(|v| !v.blocked.is_empty() && !v.blocked.starts_with("file:"))
            .collect(),
        Err(error) => {
            failures.push(error);
            Vec::new()
        }
    };
    let blocked = reported
        .iter()
        .filter(|v| !net.non_file.iter().any(|url| url == &v.blocked))
        .count();
    if blocked == 0 {
        failures.push("negative page: nothing blocked".to_owned());
    }
    if failures.is_empty() {
        Ok(Negative {
            blocked,
            reported: reported.len(),
        })
    } else {
        Err(failures)
    }
}

/// The counts of `network-silence-summary.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) pages: usize,
    pub(crate) text_originals: usize,
    pub(crate) file_requests: usize,
    pub(crate) other_requests: usize,
    pub(crate) selftest_pass: bool,
    pub(crate) negative: Negative,
}

/// The `edition` field of an `edition.toml`.
fn edition_field(edition_toml: &str) -> Result<String, String> {
    let table: toml::Table = edition_toml
        .parse()
        .map_err(|error| format!("unreadable edition.toml: {error}"))?;
    table
        .get("edition")
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "edition.toml has no `edition` string".to_owned())
}

/// Every command-line option of a page load, in order, with the net-log path as `<file>`.
pub(crate) fn options_text() -> String {
    let mut options: Vec<&str> = vec![EDGE_FLAGS[0], "--dump-dom", "--log-net-log=<file>"];
    options.extend_from_slice(&EDGE_FLAGS[1..]);
    options.join(" ")
}

/// The text of `network-silence-summary.txt`: twelve `name: value` lines in the order of
/// section 4 of `docs/intake/network-silence.md`.
pub(crate) fn summary_text(
    edition_toml: &str,
    browser: &BrowserId,
    options: &str,
    counts: &Counts,
) -> Result<String, String> {
    let edition = edition_field(edition_toml)?;
    let verdict = counts.other_requests == 0
        && counts.selftest_pass
        && counts.negative.blocked >= 1
        && counts.negative.reported >= 1;
    let lines = [
        format!("edition: {edition}"),
        format!("browser: {} {}", browser.kind.product(), browser.version),
        format!("options: {options}"),
        format!("pages loaded: {}", counts.pages),
        format!("text-renamed originals loaded: {}", counts.text_originals),
        format!(
            "requests started by those loads: {}",
            counts.file_requests + counts.other_requests
        ),
        format!("requests to file: URLs: {}", counts.file_requests),
        format!(
            "requests to any other scheme or host: {}",
            counts.other_requests
        ),
        format!(
            "self-test: {}",
            if counts.selftest_pass { "pass" } else { "fail" }
        ),
        format!(
            "negative page, requests blocked: {}",
            counts.negative.blocked
        ),
        format!(
            "negative page, requests reported: {}",
            counts.negative.reported
        ),
        format!("verdict: {}", if verdict { "pass" } else { "fail" }),
    ];
    let mut text = lines.join("\n");
    text.push('\n');
    Ok(text)
}

/// A `file:` URL for an absolute path.
fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut url = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            url.push(char::from(byte));
        } else {
            let _ = write!(url, "%{byte:02X}");
        }
    }
    url
}

/// One finished browser load: its dumped DOM and its net-log.
struct Load {
    dom: String,
    net: NetFindings,
}

/// Opens `target` in the browser with the AD-08 options, `--dump-dom` and a net-log, within
/// [`PAGE_LIMIT`].
fn load(browser: &Path, target: &Path, work: &Path, name: &str) -> Result<Load, String> {
    let netlog = work.join("netlog.json");
    let dom_path = work.join("dom.html");
    let _ = fs::remove_file(&netlog);
    let dom_file = fs::File::create(&dom_path)
        .map_err(|error| format!("cannot create {}: {error}", dom_path.display()))?;
    let mut child = Command::new(browser)
        .args(&EDGE_FLAGS[..1])
        .arg("--dump-dom")
        .arg(format!("--log-net-log={}", netlog.display()))
        .args(&EDGE_FLAGS[1..])
        .arg(file_url(target))
        .stdin(Stdio::null())
        .stdout(dom_file)
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", browser.display()))?;
    let deadline = Instant::now() + PAGE_LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{name}: not done in {} seconds",
                    PAGE_LIMIT.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(error) => return Err(format!("{name}: cannot wait for the browser: {error}")),
        }
    }
    let dom = fs::read_to_string(&dom_path)
        .map_err(|error| format!("{name}: cannot read the dumped DOM: {error}"))?;
    let text = fs::read_to_string(&netlog)
        .map_err(|error| format!("{name}: no net-log was written: {error}"))?;
    let net = read_netlog(&text).map_err(|error| format!("{name}: {error}"))?;
    Ok(Load { dom, net })
}

/// The edition-relative name of a page for messages.
fn display_name(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// The `content` of the negative page's Content Security Policy meta element.
fn negative_policy(html: &str) -> Option<&str> {
    let start = html.find("http-equiv=\"Content-Security-Policy\" content=\"")?
        + "http-equiv=\"Content-Security-Policy\" content=\"".len();
    let end = html[start..].find('"')? + start;
    Some(&html[start..end])
}

/// `cargo xtask harness [--edition DIR]`.
pub(crate) fn command(root: &Path, args: &[&str]) -> Result<bool, String> {
    let edition = match args {
        [] => root.join("out/edition"),
        ["--edition", dir] => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                root.join(dir)
            }
        }
        _ => return Err("usage: cargo xtask harness [--edition DIR]".to_owned()),
    };
    let browser = find_browser()?;
    let version_output = Command::new(&browser.path)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run {} --version: {error}", browser.path.display()))?;
    let id = parse_version(
        browser.kind,
        &String::from_utf8_lossy(&version_output.stdout),
    )?;
    println!("{}", first_line(&id));

    let edition_toml_path = edition.join(fieldbook_render::edition::OUTPUT_PATH);
    let edition_toml = fs::read_to_string(&edition_toml_path).map_err(|error| {
        format!(
            "cannot read {} (run `just site` first): {error}",
            edition_toml_path.display()
        )
    })?;
    let originals = edition.parent().map_or_else(
        || PathBuf::from("originals"),
        |parent| parent.join("originals"),
    );

    let harness = root.join("out/harness");
    let copy = harness.join("edition");
    let work = harness.join("work");
    fs::create_dir_all(&work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))?;
    let summary_path = harness.join("network-silence-summary.txt");
    let _ = fs::remove_file(&summary_path);

    let mut failures: Vec<String> = Vec::new();

    // The negative page first: an instrument that cannot see a remote load proves nothing.
    if negative_policy(NEGATIVE_HTML) != Some(fieldbook_render::csp::CSP_STATIC) {
        return Err("negative.html does not carry the edition's CSP_STATIC policy".to_owned());
    }
    let negative_dir = harness.join("negative");
    if negative_dir.exists() {
        fs::remove_dir_all(&negative_dir)
            .map_err(|error| format!("cannot clear {}: {error}", negative_dir.display()))?;
    }
    fs::create_dir_all(&negative_dir)
        .map_err(|error| format!("cannot create {}: {error}", negative_dir.display()))?;
    fs::write(negative_dir.join(SELFTEST_NAME), SELFTEST_JS)
        .map_err(|error| format!("cannot write the self-test: {error}"))?;
    let negative_page = negative_dir.join("negative.html");
    let negative_html =
        inject_script(NEGATIVE_HTML, &selftest_src(&negative_page, &negative_dir)?)?;
    fs::write(&negative_page, negative_html)
        .map_err(|error| format!("cannot write the negative page: {error}"))?;
    let negative = match load(&browser.path, &negative_page, &work, "negative page") {
        Ok(loaded) => match judge_negative(&loaded.dom, &loaded.net) {
            Ok(negative) => Some(negative),
            Err(found) => {
                failures.extend(found);
                None
            }
        },
        Err(error) => {
            failures.push(error);
            None
        }
    };

    let pages = prepare_copy(&edition, &copy)?;
    let texts = renderable_originals(&originals)?;
    let total = pages.len() + texts.len();
    let mut file_requests = 0_usize;
    let mut other_requests = 0_usize;
    let mut selftest_pass = true;
    for (index, page) in pages.iter().enumerate() {
        let name = display_name(page, &copy);
        match load(&browser.path, page, &work, &name) {
            Ok(loaded) => {
                // The page's own navigation is a request to a file: URL.
                file_requests += 1 + loaded.net.file;
                other_requests += loaded.net.non_file.len();
                failures.extend(netlog_failures(&name, &loaded.net));
                match judge_html_page(&name, &loaded.dom) {
                    Ok(selftest) => {
                        file_requests += selftest
                            .resources
                            .iter()
                            .filter(|r| r.name.starts_with("file:"))
                            .count();
                    }
                    Err(found) => {
                        selftest_pass = false;
                        failures.extend(found);
                    }
                }
            }
            Err(error) => {
                selftest_pass = false;
                failures.push(error);
            }
        }
        progress(index + 1, total);
    }
    for (index, text) in texts.iter().enumerate() {
        let name = format!("originals/{}", display_name(text, &originals));
        match load(&browser.path, text, &work, &name) {
            Ok(loaded) => {
                file_requests += 1 + loaded.net.file;
                other_requests += loaded.net.non_file.len();
                failures.extend(netlog_failures(&name, &loaded.net));
            }
            Err(error) => failures.push(error),
        }
        progress(pages.len() + index + 1, total);
    }

    let Some(negative) = negative else {
        return report_failures(&failures);
    };
    if !failures.is_empty() {
        return report_failures(&failures);
    }
    let counts = Counts {
        pages: pages.len(),
        text_originals: texts.len(),
        file_requests,
        other_requests,
        selftest_pass,
        negative,
    };
    let summary = summary_text(&edition_toml, &id, &options_text(), &counts)?;
    fs::write(&summary_path, summary)
        .map_err(|error| format!("cannot write {}: {error}", summary_path.display()))?;
    println!("{}", summary_line(&id, pages.len(), texts.len()));
    Ok(true)
}

fn progress(done: usize, total: usize) {
    if done.is_multiple_of(100) || done == total {
        eprintln!("harness: {done}/{total} loaded");
    }
}

fn report_failures(failures: &[String]) -> Result<bool, String> {
    for failure in failures {
        eprintln!("harness: FAIL {failure}");
    }
    eprintln!("harness: {} failures", failures.len());
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NETLOG_FILE_PAGE_HTTPS: &str =
        include_str!("../tests/fixtures/harness/netlog-file-page-https.json");
    const NETLOG_BROWSER_OWN: &str =
        include_str!("../tests/fixtures/harness/netlog-browser-own.json");
    const DOM_WITH_VIOLATION: &str =
        include_str!("../tests/fixtures/harness/dom-with-violation.html");
    const DOM_WITHOUT_SELFTEST: &str =
        include_str!("../tests/fixtures/harness/dom-without-selftest.html");
    const DOM_NESTED_WITH_VIOLATION: &str =
        include_str!("../tests/fixtures/harness/dom-nested-with-violation.html");
    const DOM_NESTED_WITHOUT_SELFTEST: &str =
        include_str!("../tests/fixtures/harness/dom-nested-without-selftest.html");
    const EDGE_VERSION: &str = include_str!("../tests/fixtures/harness/edge-version.txt");
    const NETWORK_SILENCE: &str = include_str!("../../docs/intake/network-silence.md");

    const UNRELEASED_TOML: &str = "edition = \"unreleased\"\nnumber = 0\nbuilt = \"\"\n";
    const STAMPED_TOML: &str = "edition = \"edition-007\"\nnumber = 7\nbuilt = \"2000-01-01\"\n";

    fn edge_id() -> BrowserId {
        parse_version(BrowserKind::Edge, EDGE_VERSION).unwrap()
    }

    fn counts() -> Counts {
        Counts {
            pages: 12,
            text_originals: 3,
            file_requests: 40,
            other_requests: 0,
            selftest_pass: true,
            negative: Negative {
                blocked: 1,
                reported: 1,
            },
        }
    }

    /// `^harness: edge [0-9][0-9.]*$` (or `chromium`), checked by hand.
    fn matches_first_line(line: &str, short: &str) -> bool {
        line.strip_prefix("harness: ")
            .and_then(|rest| rest.strip_prefix(short))
            .and_then(|rest| rest.strip_prefix(' '))
            .is_some_and(is_dotted_version)
    }

    fn is_dotted_version(text: &str) -> bool {
        text.starts_with(|c: char| c.is_ascii_digit())
            && text.chars().all(|c| c.is_ascii_digit() || c == '.')
    }

    #[test]
    fn https_request_from_file_page_fails() {
        let findings = read_netlog(NETLOG_FILE_PAGE_HTTPS).unwrap();
        let failures = netlog_failures("p/S-0001.html", &findings);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("p/S-0001.html"), "{failures:?}");
        assert!(
            failures[0].contains("https://example.invalid/font.woff2"),
            "{failures:?}"
        );
    }

    #[test]
    fn browser_own_request_is_ignored() {
        let findings = read_netlog(NETLOG_BROWSER_OWN).unwrap();
        assert_eq!(findings, NetFindings::default());
        assert!(netlog_failures("index.html", &findings).is_empty());
    }

    #[test]
    fn selftest_src_is_relative_to_each_copied_page() {
        let root = Path::new("/harness/edition");
        for (page, expected) in [
            ("index.html", "selftest.js"),
            ("p/S-0001.html", "../selftest.js"),
            ("p/x/S-0002.html", "../../selftest.js"),
        ] {
            let page_path = root.join(page);
            let src = selftest_src(&page_path, root).unwrap();
            assert_eq!(src, expected, "page {page}");
            let resolved = normalise(&page_path.parent().unwrap().join(&src));
            assert_eq!(resolved, root.join(SELFTEST_NAME), "page {page}");
        }
        assert!(selftest_src(Path::new("/elsewhere/index.html"), root).is_err());
        assert!(selftest_src(&root.join("../index.html"), root).is_err());
    }

    fn normalise(path: &Path) -> PathBuf {
        let mut out = PathBuf::new();
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    }

    #[test]
    fn nested_page_gets_the_relative_selftest_reference() {
        let dir = tempfile::tempdir().unwrap();
        let edition = dir.path().join("edition");
        fs::create_dir_all(edition.join("p")).unwrap();
        let page = "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\"><title>t</title></head><body></body></html>\n";
        fs::write(edition.join("index.html"), page).unwrap();
        fs::write(edition.join("p/S-0001.html"), page).unwrap();
        let copy = dir.path().join("harness/edition");
        let pages = prepare_copy(&edition, &copy).unwrap();
        assert_eq!(pages.len(), 2);
        let nested = copy.join("p/S-0001.html");
        let html = fs::read_to_string(&nested).unwrap();
        let expected = "<head><script src=\"../selftest.js\"></script>";
        assert!(
            html.contains(expected),
            "nested page p/S-0001.html lacks `{expected}`:\n{html}"
        );
        let resolved = normalise(&nested.parent().unwrap().join("../selftest.js"));
        assert_eq!(resolved, normalise(&copy.join(SELFTEST_NAME)));
        assert!(resolved.is_file(), "the harness-owned self-test exists");
        assert!(
            !copy.join("p/selftest.js").exists(),
            "no page points at a p/selftest.js"
        );
        // The edition itself is never changed.
        assert_eq!(
            fs::read_to_string(edition.join("p/S-0001.html")).unwrap(),
            page
        );
    }

    #[test]
    fn selftest_violation_fails() {
        let failures = judge_html_page("index.html", DOM_WITH_VIOLATION).unwrap_err();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("index.html"));
        assert!(failures[0].contains("https://example.invalid/i.png"));
        let nested = judge_html_page("p/S-0001.html", DOM_NESTED_WITH_VIOLATION).unwrap_err();
        assert!(nested[0].contains("p/S-0001.html"), "{nested:?}");
    }

    #[test]
    fn missing_selftest_result_fails() {
        let failures = judge_html_page("index.html", DOM_WITHOUT_SELFTEST).unwrap_err();
        assert!(failures[0].contains("index.html: missing selftest result"));
        let nested = judge_html_page("p/S-0001.html", DOM_NESTED_WITHOUT_SELFTEST).unwrap_err();
        assert!(
            nested[0].contains("p/S-0001.html: missing selftest result"),
            "{nested:?}"
        );
    }

    #[test]
    fn first_line_matches_version_pattern() {
        let line = first_line(&edge_id());
        assert_eq!(line, "harness: edge 131.0.2903.51");
        assert!(matches_first_line(&line, "edge"), "{line}");
    }

    #[test]
    fn summary_line_ends_with_same_version() {
        let id = edge_id();
        let line = summary_line(&id, 12, 3);
        assert!(line.contains("0 non-file requests"), "{line}");
        assert!(line.contains("negative page blocked"), "{line}");
        assert!(line.ends_with(&format!("(edge {})", id.version)), "{line}");
    }

    #[test]
    fn edge_is_preferred_over_chromium() {
        let lookup = |name: &str| Some(PathBuf::from(format!("/usr/bin/{name}")));
        let browser = pick_browser(false, &lookup).unwrap();
        assert_eq!(browser.kind, BrowserKind::Edge);
        assert_eq!(browser.path, PathBuf::from("/usr/bin/microsoft-edge"));
        let windows = pick_browser(true, &lookup).unwrap();
        assert_eq!(windows.path, PathBuf::from("/usr/bin/msedge.exe"));
    }

    #[test]
    fn chromium_is_used_when_edge_is_absent() {
        let lookup = |name: &str| {
            (name == "chromium-browser").then(|| PathBuf::from("/usr/bin/chromium-browser"))
        };
        let browser = pick_browser(false, &lookup).unwrap();
        assert_eq!(browser.kind, BrowserKind::Chromium);
        let id = parse_version(browser.kind, "Chromium 156.0.8078.79 Arch Linux\n").unwrap();
        let line = first_line(&id);
        assert_eq!(line, "harness: chromium 156.0.8078.79");
        assert!(matches_first_line(&line, "chromium"), "{line}");
        assert!(summary_line(&id, 1, 0).ends_with("(chromium 156.0.8078.79)"));
        let summary = summary_text(UNRELEASED_TOML, &id, &options_text(), &counts()).unwrap();
        assert!(
            summary.contains("\nbrowser: Chromium 156.0.8078.79\n"),
            "{summary}"
        );
    }

    #[test]
    fn no_browser_is_an_error_naming_edge_and_chromium() {
        let error = pick_browser(false, &|_: &str| None).unwrap_err();
        assert!(error.starts_with("Edge not found"), "{error}");
        assert!(error.contains("Chromium"), "{error}");
    }

    const TWELVE_NAMES: [&str; 12] = [
        "edition",
        "browser",
        "options",
        "pages loaded",
        "text-renamed originals loaded",
        "requests started by those loads",
        "requests to file: URLs",
        "requests to any other scheme or host",
        "self-test",
        "negative page, requests blocked",
        "negative page, requests reported",
        "verdict",
    ];

    /// The name of a `name: value` line: up to the last `: ` (`requests to file: URLs: 0`).
    fn line_name(line: &str) -> &str {
        line.rsplit_once(": ").map_or(line, |(name, _)| name)
    }

    fn line_value(line: &str) -> &str {
        line.rsplit_once(": ").map_or("", |(_, value)| value)
    }

    #[test]
    fn summary_has_the_twelve_lines_in_order() {
        for (toml, edition) in [
            (UNRELEASED_TOML, "unreleased"),
            (STAMPED_TOML, "edition-007"),
        ] {
            let text = summary_text(toml, &edge_id(), &options_text(), &counts()).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            let names: Vec<&str> = lines.iter().map(|line| line_name(line)).collect();
            assert_eq!(names, TWELVE_NAMES, "{text}");
            assert!(!text.contains("SYNTHETIC EXAMPLE"), "{text}");
            assert_eq!(lines[0], format!("edition: {edition}"));
        }
    }

    /// The names of the section 4 example of `network-silence.md`, without its synthetic line.
    fn attested_names() -> Vec<String> {
        let section = NETWORK_SILENCE
            .split_once("## 4.")
            .map(|(_, rest)| rest)
            .unwrap();
        let block = section
            .split_once("```text\n")
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(block, _)| block)
            .unwrap();
        block
            .lines()
            .filter(|line| !line.starts_with("SYNTHETIC EXAMPLE"))
            .map(|line| {
                if line.starts_with("options: ") {
                    "options".to_owned()
                } else {
                    line_name(line).to_owned()
                }
            })
            .collect()
    }

    /// The hand-written matcher of the twelve-line attestation contract.
    fn matches_attestation(text: &str) -> Result<(), String> {
        let names = attested_names();
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() != names.len() {
            return Err(format!("{} lines, not {}", lines.len(), names.len()));
        }
        for (line, name) in lines.iter().zip(&names) {
            let value = line
                .strip_prefix(name.as_str())
                .and_then(|rest| rest.strip_prefix(": "))
                .ok_or_else(|| format!("`{line}` is not the `{name}` line"))?;
            let ok = match name.as_str() {
                "edition" => !value.is_empty(),
                "browser" => value
                    .strip_prefix("Microsoft Edge ")
                    .is_some_and(|v| is_dotted_version(v) && v.contains('.')),
                "options" => value.starts_with("--"),
                "self-test" => value == "pass" || value == "fail",
                "verdict" => value == "pass",
                _ => !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()),
            };
            if !ok {
                return Err(format!("`{line}` has a value outside the contract"));
            }
        }
        Ok(())
    }

    #[test]
    fn summary_matches_attestation_format() {
        assert_eq!(attested_names(), TWELVE_NAMES);
        let text = summary_text(UNRELEASED_TOML, &edge_id(), &options_text(), &counts()).unwrap();
        matches_attestation(&text).unwrap();
        assert_eq!(line_value(text.lines().nth(3).unwrap()), "12");
        assert_eq!(line_value(text.lines().nth(4).unwrap()), "3");
        assert_eq!(line_value(text.lines().nth(5).unwrap()), "40");
        assert_eq!(line_value(text.lines().nth(7).unwrap()), "0");
    }

    #[test]
    fn negative_page_carries_the_edition_policy() {
        assert_eq!(
            negative_policy(NEGATIVE_HTML),
            Some(fieldbook_render::csp::CSP_STATIC)
        );
        assert!(NEGATIVE_HTML.contains("<script src=\"https://example.invalid/x.js\">"));
    }

    #[test]
    fn negative_page_that_reports_nothing_fails() {
        let error = judge_negative(DOM_WITHOUT_SELFTEST, &NetFindings::default()).unwrap_err();
        assert!(error.iter().any(|e| e == "negative page: nothing blocked"));
        let quiet = "<pre id=\"fb-selftest\">{\"violations\":[],\"resources\":[]}</pre>";
        let error = judge_negative(quiet, &NetFindings::default()).unwrap_err();
        assert_eq!(error, vec!["negative page: nothing blocked".to_owned()]);
        let blocked = "<pre id=\"fb-selftest\">{\"violations\":[{\"directive\":\"script-src-elem\",\"blocked\":\"https://example.invalid/x.js\"}],\"resources\":[]}</pre>";
        assert_eq!(
            judge_negative(blocked, &NetFindings::default()).unwrap(),
            Negative {
                blocked: 1,
                reported: 1
            }
        );
    }

    #[test]
    fn options_never_allow_file_access() {
        assert!(!options_text().contains("--allow-file-access-from-files"));
        for flag in EDGE_FLAGS {
            assert!(options_text().contains(flag), "{flag}");
        }
    }

    #[test]
    fn renderable_originals_are_the_txt_names() {
        assert!(is_renderable_original("S-0001-v1.html.txt"));
        assert!(is_renderable_original("S-0001-v1.SVG.txt"));
        assert!(!is_renderable_original("S-0001-v1.txt"));
        assert!(!is_renderable_original("S-0001-v1.pdf"));
        assert!(!is_renderable_original("S-0001-v1.csv.txt"));
    }

    #[test]
    fn file_url_escapes_the_path() {
        assert_eq!(
            file_url(Path::new("/a b/p/S-1.html")),
            "file:///a%20b/p/S-1.html"
        );
    }
}
