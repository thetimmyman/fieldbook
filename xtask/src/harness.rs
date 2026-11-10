//! `cargo xtask harness [--edition DIR]`: open every page and every renderable original of an
//! edition in headless Edge with a net-log (`edge-harness`, AD-08; controls NS-03, WB-01 and the
//! harness half of ED-02).
//!
//! Edge is the target. Where no Edge is installed (the Linux home machine and the self-hosted
//! runner), the harness falls back to Chromium and names the browser that ran in its first line
//! and its summary line. Only an Edge run over `out/edition` writes
//! `network-silence-summary.txt`, whose `browser:` line section 4 of `network-silence.md` fixes
//! as Microsoft Edge.
//!
//! The edition is never changed: the harness copies it to `out/harness/edition/`, copies
//! `selftest.js` once to the root of that copy and injects one script element as the first child
//! of `<head>` of each copied HTML page. Renderable `.txt` originals are opened unchanged from the
//! `originals` folder beside the edition.
//!
//! Before the edition pages, the harness writes the golden wrappers (`golden_wrap`) and opens
//! three generated search pages under `out/harness/search/` (`search-js`): every query-side
//! tokenize case, every ranked and page-view case and the render cases through `search.js`, the
//! case `skeleton_waits_300ms`, and an index whose `v` is 2. It prints each passing case, fails
//! on any case that fails and names it, and prints the advisory p95 of 10 lookups.

use std::ffi::{OsStr, OsString};
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::golden_wrap;

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

/// The `[counts] originals` field of an `edition.toml` (0 when absent).
fn originals_count(edition_toml: &str) -> Result<i64, String> {
    let table: toml::Table = edition_toml
        .parse()
        .map_err(|error| format!("unreadable edition.toml: {error}"))?;
    Ok(table
        .get("counts")
        .and_then(|counts| counts.get("originals"))
        .and_then(toml::Value::as_integer)
        .unwrap_or(0))
}

/// The renderable `.txt` originals of `originals`, sorted. A missing folder fails when the
/// edition counts any originals, so the text half of ED-02 is never skipped unnoticed.
fn renderable_originals(originals: &Path, expected: i64) -> Result<Vec<PathBuf>, String> {
    if !originals.exists() {
        return if expected > 0 {
            Err(format!(
                "edition.toml counts {expected} originals, but {} does not exist",
                originals.display()
            ))
        } else {
            Ok(Vec::new())
        };
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
///
/// A request is a `URL_REQUEST_START_JOB` from a `file:` page (by its initiator, its network
/// isolation key or its site for cookies), or a `HOST_RESOLVER_MANAGER_REQUEST` whose network
/// anonymization key is `file:`: a preconnect or DNS prefetch from a page resolves a host and
/// may connect to it without ever starting a URL request.
pub(crate) fn read_netlog(text: &str) -> Result<NetFindings, String> {
    let log: NetLog =
        serde_json::from_str(text).map_err(|error| format!("unreadable net-log: {error}"))?;
    let event_type = |name: &str| {
        log.constants
            .log_event_types
            .get(name)
            .copied()
            .ok_or_else(|| format!("net-log has no {name} event type"))
    };
    let start_job = event_type("URL_REQUEST_START_JOB")?;
    let resolve = event_type("HOST_RESOLVER_MANAGER_REQUEST")?;
    let mut findings = NetFindings::default();
    for event in &log.events {
        let Some(params) = &event.params else {
            continue;
        };
        let field = |name: &str| params.get(name).and_then(|v| v.as_str()).unwrap_or("");
        if event.kind == resolve {
            let host = field("host");
            if !host.is_empty() && field("network_anonymization_key").starts_with("file:") {
                findings.non_file.push(host.to_owned());
            }
            continue;
        }
        if event.kind != start_job {
            continue;
        }
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

/// True for a resource entry that names a local load: a `file:` or `data:` URL.
fn is_local_resource(name: &str) -> bool {
    name.starts_with("file:") || name.starts_with("data:")
}

/// Judges one HTML page by its dumped DOM: a missing result, any violation, or any resource entry
/// that is not local fails. The resource check is independent of the net-log: the browser lists a
/// remote load there even when it was blocked.
pub(crate) fn judge_html_page(page: &str, dom: &str) -> Result<Selftest, Vec<String>> {
    let selftest = read_selftest(page, dom).map_err(|error| vec![error])?;
    let mut failures: Vec<String> = selftest
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
        .collect();
    failures.extend(
        selftest
            .resources
            .iter()
            .filter(|r| !is_local_resource(&r.name))
            .map(|r| format!("{page}: non-file resource {}", r.name)),
    );
    if failures.is_empty() {
        Ok(selftest)
    } else {
        Err(failures)
    }
}

/// Judges a renderable `.txt` original by its dumped DOM: it must have loaded in the text viewer,
/// one `<pre>` in the body and no `<script>` or `<img>`, so the browser did not sniff it as HTML.
pub(crate) fn judge_text_original(name: &str, dom: &str) -> Result<(), String> {
    let lower = dom.to_ascii_lowercase();
    let body = lower
        .find("<body")
        .map(|at| &lower[at..])
        .ok_or_else(|| format!("{name}: did not load as text (no body)"))?;
    let pres = body.matches("<pre").count();
    if pres != 1 || lower.contains("<script") || lower.contains("<img") {
        return Err(format!(
            "{name}: did not load as text ({pres} <pre> elements, script {}, img {})",
            lower.contains("<script"),
            lower.contains("<img")
        ));
    }
    Ok(())
}

/// What the negative page showed: requests blocked and requests reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Negative {
    pub(crate) blocked: usize,
    pub(crate) reported: usize,
}

/// The remote script of the negative page.
pub(crate) const NEGATIVE_URL: &str = "https://example.invalid/x.js";

/// Judges the negative page: its remote script must be reported by the self-test and absent from
/// the net-log. A negative page that reports nothing fails; so does one whose only violations
/// name something else (an inline script, a keyword).
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
            .filter(|v| v.blocked == NEGATIVE_URL)
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

/// Every command-line option of a page load, in order, before the page's URL: the AD-08 flags,
/// `--dump-dom` and the net-log. [`load`] passes exactly these, and [`options_text`] attests them.
pub(crate) fn browser_args(netlog: &OsStr) -> Vec<OsString> {
    let mut log = OsString::from("--log-net-log=");
    log.push(netlog);
    let mut args: Vec<OsString> = vec![
        OsString::from(EDGE_FLAGS[0]),
        OsString::from("--dump-dom"),
        log,
    ];
    args.extend(EDGE_FLAGS[1..].iter().map(OsString::from));
    args
}

/// The attested `options:` value: [`browser_args`] with the net-log path as `<file>`.
pub(crate) fn options_text() -> String {
    browser_args(OsStr::new("<file>"))
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
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

/// The last lines of the browser's standard error, for a failure message.
fn stderr_tail(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(5)..].join(" | ");
    if tail.is_empty() {
        "no standard error".to_owned()
    } else {
        format!("standard error: {tail}")
    }
}

/// Opens `target` in the browser with [`browser_args`], within `limit`. A load that is not done
/// in time, or whose browser exits with a failure, fails and names `name`.
fn load(
    browser: &Path,
    target: &Path,
    work: &Path,
    name: &str,
    limit: Duration,
) -> Result<Load, String> {
    let netlog = work.join("netlog.json");
    let dom_path = work.join("dom.html");
    let stderr_path = work.join("stderr.txt");
    let _ = fs::remove_file(&netlog);
    let dom_file = fs::File::create(&dom_path)
        .map_err(|error| format!("cannot create {}: {error}", dom_path.display()))?;
    let stderr_file = fs::File::create(&stderr_path)
        .map_err(|error| format!("cannot create {}: {error}", stderr_path.display()))?;
    let mut child = Command::new(browser)
        .args(browser_args(netlog.as_os_str()))
        .arg(file_url(target))
        .stdin(Stdio::null())
        .stdout(dom_file)
        .stderr(stderr_file)
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", browser.display()))?;
    let deadline = Instant::now() + limit;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(status)) => {
                return Err(format!(
                    "{name}: the browser exited with {status} ({})",
                    stderr_tail(&stderr_path)
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{name}: not done within its limit of {} seconds ({})",
                    limit.as_secs(),
                    stderr_tail(&stderr_path)
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

/// The template of the search pages (`search-js`): `@MODE@` and `@GOLDEN_SCRIPTS@` are filled in.
const SEARCH_TEMPLATE: &str = include_str!("../harness/search_goldens.html");
/// The search cases the search pages run.
const SEARCH_CASES_JS: &str = include_str!("../harness/search_goldens.js");
/// `hold.js`: the empty script `search_goldens.js` keeps loading, so that the load event, and
/// with it `--dump-dom`, waits until every case has reported.
const SEARCH_HOLD_JS: &str = "// Empty: loading it delays the load event.\n";
/// The search pages, each with the cases it must report beyond those the goldens name.
const SEARCH_PAGES: [(&str, &[&str]); 3] = [
    (
        "goldens",
        &[
            "ordered_hits_render_without_reranking",
            "index_text_goes_in_as_text",
            "hash_query_is_decoded",
            "a_non_query_hash_keeps_the_results",
            "an_undecodable_hash_is_an_empty_query",
            "enter_keeps_the_typed_results",
        ],
    ),
    ("skeleton", &["skeleton_waits_300ms"]),
    (
        "version",
        &["unsupported_index_version_keeps_the_id_list_line"],
    ),
];
/// The ranked golden whose index, with `v` set to 2, the version page loads.
const VERSION_GOLDEN: &str = "search/seed-ranked.json";

/// The template of the shortcut pages (`search-palette`): `@MODE@` is filled in.
const SHORTCUT_TEMPLATE: &str = include_str!("../harness/shortcuts.html");
/// The shortcut and palette cases the shortcut pages run.
const SHORTCUT_CASES_JS: &str = include_str!("../harness/shortcuts.js");
/// The shortcut pages, each with every case it must report as passing.
const SHORTCUT_PAGES: [(&str, &[&str]); 2] = [
    (
        "shortcuts",
        &[
            "trigger_is_a_button_showing_ctrl_k",
            "ctrl_k_opens",
            "slash_opens",
            "slash_in_a_text_field_types",
            "esc_closes_and_returns_focus",
            "question_mark_opens_the_shortcuts_dialog",
            "modal_dialogs_contain_focus",
            "switch_off_stops_single_keys_but_not_ctrl_k",
            "palette_empty_input_lists_latest_pages",
            "palette_one_character_prefix_first",
            "palette_duplicates_appear_once",
            "palette_caps_at_eight",
            "palette_absent_link_enter_does_not_navigate",
            "palette_linked_result_enter_navigates",
            "palette_row_click_follows_its_link",
            "palette_backdrop_and_close_button_close",
            "palette_final_search_row",
            "menu_opens_even_when_the_slide_stalls",
        ],
    ),
    ("storage-throws", &["storage_throwing_page_still_works"]),
];

/// One harness page that runs cases (a search page or a shortcut page), ready to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CasePage {
    /// Search: `goldens`, `skeleton` or `version`; shortcuts: `shortcuts` or `storage-throws`.
    pub(crate) mode: &'static str,
    /// The generated page.
    pub(crate) path: PathBuf,
    /// Every case id the page must report as passing.
    pub(crate) required: Vec<String>,
}

/// The ids of the golden cases the goldens page runs: every query-side tokenize case and every
/// ranked and page-view case, from the golden files' raw text keyed by path.
pub(crate) fn golden_case_ids(files: &[(String, String)]) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    for (key, raw) in files {
        let value: serde_json::Value =
            serde_json::from_str(raw).map_err(|error| format!("{key}: not JSON: {error}"))?;
        let kind = value.get("kind").and_then(serde_json::Value::as_str);
        if !matches!(kind, Some("tokenize" | "ranked" | "page-view")) {
            continue;
        }
        let cases = value
            .get("cases")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| format!("{key}: no cases"))?;
        for case in cases {
            if kind == Some("tokenize")
                && case.get("side").and_then(serde_json::Value::as_str) != Some("query")
            {
                continue;
            }
            let id = case
                .get("id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| format!("{key}: a case has no id"))?;
            ids.push(id.to_owned());
        }
    }
    Ok(ids)
}

/// The text of `search-index.js` for the golden `raw` with its `v` set to 2.
fn version_two_index(raw: &str) -> Result<String, String> {
    let script = golden_wrap::wrap_index(VERSION_GOLDEN, raw)?
        .ok_or_else(|| format!("{VERSION_GOLDEN}: no units"))?;
    let payload = script
        .lines()
        .nth(1)
        .and_then(|line| line.split_once("] = "))
        .and_then(|(_, rest)| rest.strip_suffix(';'))
        .ok_or_else(|| format!("{VERSION_GOLDEN}: the index wrapper changed its form"))?;
    let mut index: serde_json::Value = serde_json::from_str(payload)
        .map_err(|error| format!("{VERSION_GOLDEN}: index is not JSON: {error}"))?;
    let object = index
        .as_object_mut()
        .ok_or_else(|| format!("{VERSION_GOLDEN}: the index is not an object"))?;
    object.insert("v".to_owned(), serde_json::Value::from(2));
    Ok(format!("window.FB_INDEX = {index};"))
}

fn attribute_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn write_file(path: &Path, text: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Writes the golden wrappers (`golden_wrap`) and the three search pages under
/// `<harness>/search/`: `goldens` loads every wrapper and, when `site_index` is given, the
/// edition's `search-index.js` for the timing; `skeleton` loads none; `version` has a
/// `search-index.js` whose `v` is 2. Each page has `search.js`, `strings.js` and the self-test.
pub(crate) fn prepare_search_pages(
    root: &Path,
    harness: &Path,
    site_index: Option<&Path>,
) -> Result<Vec<CasePage>, String> {
    let wrappers = golden_wrap::write(root)?;
    let golden_root = root.join(golden_wrap::GOLDEN_DIR);
    let mut files = Vec::new();
    for wrapper in wrappers.iter().filter(|w| !w.path.ends_with(".index.js")) {
        let path = golden_root.join(&wrapper.key);
        let raw = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        files.push((wrapper.key.clone(), raw));
    }
    let golden_ids = golden_case_ids(&files)?;
    let seed = files
        .iter()
        .find(|(key, _)| key == VERSION_GOLDEN)
        .map(|(_, raw)| raw.as_str())
        .ok_or_else(|| format!("{VERSION_GOLDEN} is missing"))?;

    let base = harness.join("search");
    if base.exists() {
        fs::remove_dir_all(&base)
            .map_err(|error| format!("cannot clear {}: {error}", base.display()))?;
    }
    let golden_src = golden_wrap::OUT_DIR
        .strip_prefix("out/harness/")
        .ok_or("the golden wrappers are not under out/harness")?;
    let mut pages = Vec::new();
    for (mode, named) in SEARCH_PAGES {
        let dir = base.join(mode);
        write_file(
            &dir.join(fieldbook_render::search_page::SCRIPT_PATH),
            fieldbook_render::search_page::SCRIPT.as_bytes(),
        )?;
        write_file(
            &dir.join("assets/strings.js"),
            fieldbook_core::strings::js().as_bytes(),
        )?;
        write_file(&dir.join("search_goldens.js"), SEARCH_CASES_JS.as_bytes())?;
        write_file(&dir.join(SELFTEST_NAME), SELFTEST_JS.as_bytes())?;
        write_file(&dir.join("hold.js"), SEARCH_HOLD_JS.as_bytes())?;
        let index_path = dir.join(fieldbook_render::search_index::OUTPUT_PATH);
        let mut scripts = String::new();
        let mut required: Vec<String> = named.iter().map(|id| (*id).to_owned()).collect();
        match mode {
            "goldens" => {
                for wrapper in &wrappers {
                    let _ = writeln!(
                        scripts,
                        "<script src=\"../../{golden_src}/{}\"></script>",
                        attribute_text(&wrapper.path)
                    );
                }
                if let Some(site) = site_index {
                    fs::copy(site, &index_path)
                        .map_err(|error| format!("cannot copy {}: {error}", site.display()))?;
                    let _ = writeln!(
                        scripts,
                        "<script src=\"{}\"></script>",
                        fieldbook_render::search_index::OUTPUT_PATH
                    );
                }
                required.extend(golden_ids.iter().cloned());
            }
            "version" => write_file(&index_path, version_two_index(seed)?.as_bytes())?,
            _ => {}
        }
        let html = SEARCH_TEMPLATE
            .replace("@GOLDEN_SCRIPTS@", scripts.trim_end())
            .replace("@MODE@", mode);
        let path = dir.join(format!("{mode}.html"));
        let html = inject_script(&html, &selftest_src(&path, &dir)?)?;
        write_file(&path, html.as_bytes())?;
        pages.push(CasePage {
            mode,
            path,
            required,
        });
    }
    Ok(pages)
}

/// Writes the shortcut pages under `<harness>/shortcuts/<mode>/<mode>.html`, one per entry of
/// [`SHORTCUT_PAGES`]: each loads `shortcuts.js` first (its synthetic index and, for
/// `storage-throws`, storage that throws), then the site's `strings.js`, `search.js` and
/// `app.js`, with the self-test and `hold.js` beside them.
pub(crate) fn prepare_shortcut_pages(harness: &Path) -> Result<Vec<CasePage>, String> {
    let base = harness.join("shortcuts");
    if base.exists() {
        fs::remove_dir_all(&base)
            .map_err(|error| format!("cannot clear {}: {error}", base.display()))?;
    }
    let mut pages = Vec::new();
    for (mode, named) in SHORTCUT_PAGES {
        let dir = base.join(mode);
        write_file(
            &dir.join("assets/strings.js"),
            fieldbook_core::strings::js().as_bytes(),
        )?;
        write_file(
            &dir.join(fieldbook_render::search_page::SCRIPT_PATH),
            fieldbook_render::search_page::SCRIPT.as_bytes(),
        )?;
        write_file(
            &dir.join("assets/app.js"),
            fieldbook_render::site::APP_JS.as_bytes(),
        )?;
        write_file(&dir.join("shortcuts.js"), SHORTCUT_CASES_JS.as_bytes())?;
        write_file(&dir.join(SELFTEST_NAME), SELFTEST_JS.as_bytes())?;
        write_file(&dir.join("hold.js"), SEARCH_HOLD_JS.as_bytes())?;
        let path = dir.join(format!("{mode}.html"));
        let html = SHORTCUT_TEMPLATE.replace("@MODE@", mode);
        let html = inject_script(&html, &selftest_src(&path, &dir)?)?;
        write_file(&path, html.as_bytes())?;
        pages.push(CasePage {
            mode,
            path,
            required: named.iter().map(|id| (*id).to_owned()).collect(),
        });
    }
    Ok(pages)
}

/// One case a case page reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchCase {
    pub(crate) id: String,
    pub(crate) ok: bool,
    pub(crate) text: String,
}

/// What a finished case page reported: its cases in order and its summary line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchRun {
    pub(crate) cases: Vec<SearchCase>,
    pub(crate) summary: String,
}

/// The value of attribute `name` in the start tag `tag`.
fn tag_attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(&tag[start..end])
}

/// Reads a case page's dumped DOM: every `<li data-case data-ok>` and `#harness-summary`.
/// Fails, naming `page`, its `group` (`search` or `shortcut`) and each case, when the page did
/// not finish, a case failed, the failed count disagrees, or a `required` case did not run.
pub(crate) fn judge_case_page(
    page: &str,
    group: &str,
    dom: &str,
    required: &[String],
) -> Result<SearchRun, Vec<String>> {
    let mut failures = Vec::new();
    let mut cases = Vec::new();
    let mut from = 0;
    while let Some(found) = dom[from..].find("<li data-case=\"") {
        let start = from + found;
        let Some(tag_end) = dom[start..].find('>').map(|i| start + i) else {
            break;
        };
        let tag = &dom[start..tag_end];
        let close = dom[tag_end..]
            .find("</li>")
            .map_or(dom.len(), |i| tag_end + i);
        let id = unescape_html(tag_attribute(tag, "data-case").unwrap_or_default());
        let ok = tag_attribute(tag, "data-ok") == Some("true");
        let text = unescape_html(&dom[tag_end + 1..close]);
        if !ok {
            failures.push(format!("{page}: {group} case {id} failed: {text}"));
        }
        cases.push(SearchCase { id, ok, text });
        from = close;
    }
    let summary = dom.find("id=\"harness-summary\"").and_then(|at| {
        let start = dom[..at].rfind('<')?;
        let tag_end = dom[at..].find('>')? + at;
        let close = dom[tag_end..].find("</p>")? + tag_end;
        Some((
            &dom[start..tag_end],
            unescape_html(&dom[tag_end + 1..close]),
        ))
    });
    let Some((tag, summary)) = summary else {
        failures.push(format!("{page}: no #harness-summary in the page"));
        return Err(failures);
    };
    if tag_attribute(tag, "data-done") != Some("true") {
        failures.push(format!(
            "{page}: the {group} cases did not finish ({} reported)",
            cases.len()
        ));
    }
    let failed = cases.iter().filter(|c| !c.ok).count();
    if tag_attribute(tag, "data-failed").and_then(|n| n.parse::<usize>().ok()) != Some(failed) {
        failures.push(format!(
            "{page}: the summary's failed count is not the {failed} failed cases"
        ));
    }
    for id in required {
        if !cases.iter().any(|c| &c.id == id) {
            failures.push(format!("{page}: {group} case {id} did not run"));
        }
    }
    if cases.is_empty() {
        failures.push(format!("{page}: no {group} case ran"));
    }
    if failures.is_empty() {
        Ok(SearchRun { cases, summary })
    } else {
        Err(failures)
    }
}

/// Opens each case page of `group` (`search`, under `search/`, or `shortcut`, under
/// `shortcuts/`) and prints each case; returns the failures.
fn run_case_pages(browser: &Path, work: &Path, group: &str, pages: &[CasePage]) -> Vec<String> {
    let mut failures = Vec::new();
    let folder = if group == "search" {
        "search"
    } else {
        "shortcuts"
    };
    for page in pages {
        let name = format!("{folder}/{}.html", page.mode);
        let loaded = match load(browser, &page.path, work, &name, PAGE_LIMIT) {
            Ok(loaded) => loaded,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        failures.extend(netlog_failures(&name, &loaded.net));
        if let Err(found) = judge_html_page(&name, &loaded.dom) {
            failures.extend(found);
        }
        match judge_case_page(&name, group, &loaded.dom, &page.required) {
            Ok(run) => {
                for case in &run.cases {
                    println!("harness: {group}: {}: ok", case.id);
                }
                println!("harness: {group}: {name}: {}", run.summary);
            }
            Err(found) => {
                for failure in &found {
                    eprintln!("harness: FAIL {failure}");
                }
                failures.extend(found);
            }
        }
    }
    failures
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
    let harness = root.join("out/harness");
    // First, before anything can fail: an earlier pass must never outlive this run.
    let summary_path = harness.join("network-silence-summary.txt");
    if let Err(error) = fs::remove_file(&summary_path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        return Err(format!("cannot remove {}: {error}", summary_path.display()));
    }
    let default_tree = same_dir(&edition, &root.join("out/edition"));
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

    let copy = harness.join("edition");
    let work = harness.join("work");
    fs::create_dir_all(&work)
        .map_err(|error| format!("cannot create {}: {error}", work.display()))?;

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
    let negative = match load(
        &browser.path,
        &negative_page,
        &work,
        "negative page",
        PAGE_LIMIT,
    ) {
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

    // The search pages (`search-js`): the goldens through search.js, the Skeleton and the
    // index-version line. The edition's own index, when there is one, is timed.
    let site_index = edition.join(fieldbook_render::search_index::OUTPUT_PATH);
    let search_pages =
        prepare_search_pages(root, &harness, site_index.is_file().then_some(&*site_index))?;
    failures.extend(run_case_pages(
        &browser.path,
        &work,
        "search",
        &search_pages,
    ));

    // The shortcut pages (`search-palette`): the trigger, the palette and the shortcuts dialog
    // through the site's app.js, once with working storage and once with storage that throws.
    let shortcut_pages = prepare_shortcut_pages(&harness)?;
    failures.extend(run_case_pages(
        &browser.path,
        &work,
        "shortcut",
        &shortcut_pages,
    ));

    let pages = prepare_copy(&edition, &copy)?;
    let texts = renderable_originals(&originals, originals_count(&edition_toml)?)?;
    eprintln!(
        "harness: {} pages and {} renderable originals in {}",
        pages.len(),
        texts.len(),
        originals.display()
    );
    let total = pages.len() + texts.len();
    let mut file_requests = 0_usize;
    let mut other_requests = 0_usize;
    let mut selftest_pass = true;
    for (index, page) in pages.iter().enumerate() {
        let name = display_name(page, &copy);
        match load(&browser.path, page, &work, &name, PAGE_LIMIT) {
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
        match load(&browser.path, text, &work, &name, PAGE_LIMIT) {
            Ok(loaded) => {
                file_requests += 1 + loaded.net.file;
                other_requests += loaded.net.non_file.len();
                failures.extend(netlog_failures(&name, &loaded.net));
                if let Err(error) = judge_text_original(&name, &loaded.dom) {
                    failures.push(error);
                }
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
    match attestation(browser.kind, default_tree) {
        Attestation::Write => fs::write(&summary_path, summary)
            .map_err(|error| format!("cannot write {}: {error}", summary_path.display()))?,
        Attestation::NotEdge => eprintln!(
            "harness: no network-silence-summary.txt: {} ran, and the attestation names Microsoft Edge",
            browser.kind.product()
        ),
        Attestation::NotDefaultTree => eprintln!(
            "harness: no network-silence-summary.txt: {} is not out/edition",
            edition.display()
        ),
    }
    println!("{}", summary_line(&id, pages.len(), texts.len()));
    Ok(true)
}

/// Whether a passing run writes `network-silence-summary.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attestation {
    /// Edge ran over `out/edition`: write it.
    Write,
    /// Another browser ran: section 4 of `network-silence.md` attests Microsoft Edge only.
    NotEdge,
    /// Another tree was tested: its result must not sit beside `out/edition`.
    NotDefaultTree,
}

/// Decides whether a passing run writes the attestation.
pub(crate) fn attestation(kind: BrowserKind, default_tree: bool) -> Attestation {
    match (kind, default_tree) {
        (_, false) => Attestation::NotDefaultTree,
        (BrowserKind::Edge, true) => Attestation::Write,
        (BrowserKind::Chromium, true) => Attestation::NotEdge,
    }
}

/// True when `a` and `b` name the same folder (by canonical path, or literally when either is
/// missing).
fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
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
    const NETLOG_FILE_PAGE_PRECONNECT: &str =
        include_str!("../tests/fixtures/harness/netlog-file-page-preconnect.json");
    const NETLOG_FILE_PAGE_NAVIGATION: &str =
        include_str!("../tests/fixtures/harness/netlog-file-page-navigation.json");
    const NETLOG_FILE_PAGE_ISOLATION_KEY: &str =
        include_str!("../tests/fixtures/harness/netlog-file-page-isolation-key.json");
    const NETLOG_FILE_PAGE_SITE_FOR_COOKIES: &str =
        include_str!("../tests/fixtures/harness/netlog-file-page-site-for-cookies.json");
    const DOM_TEXT_VIEWER: &str = include_str!("../tests/fixtures/harness/dom-text-viewer.html");
    const DOM_TEXT_SNIFFED_AS_HTML: &str =
        include_str!("../tests/fixtures/harness/dom-text-sniffed-as-html.html");
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
        // The blocked image is both a violation and a (remote) resource entry.
        assert_eq!(
            failures,
            vec![
                "index.html: CSP violation of img-src by https://example.invalid/i.png".to_owned(),
                "index.html: non-file resource https://example.invalid/i.png".to_owned(),
            ]
        );
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

    #[test]
    fn preconnect_from_file_page_fails() {
        let findings = read_netlog(NETLOG_FILE_PAGE_PRECONNECT).unwrap();
        let failures = netlog_failures("p/S-0001.html", &findings);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("p/S-0001.html"), "{failures:?}");
        assert!(
            failures[0].contains("https://cdn.example.invalid"),
            "{failures:?}"
        );
    }

    #[test]
    fn navigation_from_file_page_fails() {
        let findings = read_netlog(NETLOG_FILE_PAGE_NAVIGATION).unwrap();
        let failures = netlog_failures("index.html", &findings);
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].contains("index.html"), "{failures:?}");
        assert!(
            failures[0].contains("https://example.invalid/meta-refresh"),
            "{failures:?}"
        );
    }

    #[test]
    fn each_file_page_clause_alone_finds_the_request() {
        for (fixture, url) in [
            (
                NETLOG_FILE_PAGE_ISOLATION_KEY,
                "https://example.invalid/key.png",
            ),
            (
                NETLOG_FILE_PAGE_SITE_FOR_COOKIES,
                "https://example.invalid/cookie.png",
            ),
            (
                NETLOG_FILE_PAGE_NAVIGATION,
                "https://example.invalid/meta-refresh",
            ),
        ] {
            let findings = read_netlog(fixture).unwrap();
            assert_eq!(findings.non_file, vec![url.to_owned()], "{url}");
        }
    }

    #[test]
    fn netlog_without_resolver_events_is_refused() {
        let text =
            r#"{"constants": {"logEventTypes": {"URL_REQUEST_START_JOB": 7}}, "events": []}"#;
        let error = read_netlog(text).unwrap_err();
        assert!(error.contains("HOST_RESOLVER_MANAGER_REQUEST"), "{error}");
    }

    #[test]
    fn non_file_resource_entry_fails() {
        let dom = "<pre id=\"fb-selftest\">{\"violations\":[],\"resources\":[{\"name\":\"https://example.invalid/f.woff2\",\"initiator\":\"css\"},{\"name\":\"file:///e/a.css\",\"initiator\":\"link\"}]}</pre>";
        let failures = judge_html_page("p/S-0001.html", dom).unwrap_err();
        assert_eq!(
            failures,
            vec!["p/S-0001.html: non-file resource https://example.invalid/f.woff2".to_owned()]
        );
    }

    #[test]
    fn negative_page_counts_only_its_own_script() {
        let inline = "<pre id=\"fb-selftest\">{\"violations\":[{\"directive\":\"script-src-elem\",\"blocked\":\"inline\"}],\"resources\":[]}</pre>";
        let error = judge_negative(inline, &NetFindings::default()).unwrap_err();
        assert_eq!(error, vec!["negative page: nothing blocked".to_owned()]);
        let blocked = format!(
            "<pre id=\"fb-selftest\">{{\"violations\":[{{\"directive\":\"script-src-elem\",\"blocked\":\"{NEGATIVE_URL}\"}}],\"resources\":[]}}</pre>"
        );
        let started = NetFindings {
            file: 0,
            non_file: vec![NEGATIVE_URL.to_owned()],
        };
        let error = judge_negative(&blocked, &started).unwrap_err();
        assert!(
            error.contains(&format!("negative page: request to {NEGATIVE_URL} started")),
            "{error:?}"
        );
        assert!(
            error.contains(&"negative page: nothing blocked".to_owned()),
            "{error:?}"
        );
        assert!(NEGATIVE_HTML.contains(&format!("<script src=\"{NEGATIVE_URL}\">")));
    }

    #[test]
    fn text_original_must_load_as_text() {
        judge_text_original("originals/S-0001-v1.html.txt", DOM_TEXT_VIEWER).unwrap();
        let error = judge_text_original("originals/S-0001-v1.html.txt", DOM_TEXT_SNIFFED_AS_HTML)
            .unwrap_err();
        assert!(error.starts_with("originals/S-0001-v1.html.txt: did not load as text"));
        assert!(judge_text_original("originals/x.svg.txt", "").is_err());
    }

    #[test]
    fn browser_args_are_exactly_the_attested_options() {
        let args = browser_args(OsStr::new("/w/netlog.json"));
        let mut expected: Vec<String> = EDGE_FLAGS.iter().map(|f| (*f).to_owned()).collect();
        expected.insert(1, "--dump-dom".to_owned());
        expected.insert(2, "--log-net-log=/w/netlog.json".to_owned());
        let got: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(got, expected);
        for banned in [
            "--allow-file-access-from-files",
            "--no-sandbox",
            "--user-data-dir",
        ] {
            assert!(!got.iter().any(|a| a.starts_with(banned)), "{banned}");
        }
        assert_eq!(
            options_text(),
            expected.join(" ").replace("/w/netlog.json", "<file>")
        );
    }

    #[cfg(unix)]
    fn fake_browser(dir: &Path, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt as _;
        let path = dir.join("fake-browser");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn load_passes_browser_args_and_reads_the_results() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = dir.path().join("fixture.json");
        fs::write(&fixture, NETLOG_FILE_PAGE_HTTPS).unwrap();
        let script = format!(
            "#!/bin/sh\nfor a in \"$@\"; do case \"$a\" in --log-net-log=*) cp '{}' \"${{a#--log-net-log=}}\";; esac; done\nprintf '%s\\n' \"$@\"\n",
            fixture.display()
        );
        let browser = fake_browser(dir.path(), &script);
        let target = dir.path().join("page.html");
        let loaded = load(&browser, &target, dir.path(), "page.html", PAGE_LIMIT).unwrap();
        let mut expected: Vec<String> = browser_args(dir.path().join("netlog.json").as_os_str())
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        expected.push(file_url(&target));
        assert_eq!(loaded.dom.lines().collect::<Vec<_>>(), expected);
        assert_eq!(
            loaded.net.non_file,
            vec!["https://example.invalid/font.woff2".to_owned()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn load_past_the_limit_fails_and_names_the_page() {
        let dir = tempfile::tempdir().unwrap();
        let browser = fake_browser(dir.path(), "#!/bin/sh\nexec sleep 30\n");
        let error = load(
            &browser,
            &dir.path().join("claims.html"),
            dir.path(),
            "claims.html",
            Duration::from_millis(300),
        )
        .err()
        .unwrap();
        assert!(
            error.starts_with("claims.html: not done within its limit of 0 seconds"),
            "{error}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn browser_failure_exit_fails_and_quotes_standard_error() {
        let dir = tempfile::tempdir().unwrap();
        let browser = fake_browser(dir.path(), "#!/bin/sh\necho renderer crashed >&2\nexit 3\n");
        let error = load(
            &browser,
            &dir.path().join("index.html"),
            dir.path(),
            "index.html",
            PAGE_LIMIT,
        )
        .err()
        .unwrap();
        assert!(
            error.starts_with("index.html: the browser exited with"),
            "{error}"
        );
        assert!(error.contains("renderer crashed"), "{error}");
    }

    #[test]
    fn missing_originals_folder_fails_when_the_edition_counts_originals() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("originals");
        let toml = "edition = \"unreleased\"\n[counts]\noriginals = 70\n";
        assert_eq!(originals_count(toml).unwrap(), 70);
        let error = renderable_originals(&missing, 70).unwrap_err();
        assert!(error.contains("counts 70 originals"), "{error}");
        assert_eq!(originals_count(UNRELEASED_TOML).unwrap(), 0);
        assert!(renderable_originals(&missing, 0).unwrap().is_empty());
    }

    #[test]
    fn only_edge_over_the_default_tree_writes_the_attestation() {
        assert_eq!(attestation(BrowserKind::Edge, true), Attestation::Write);
        assert_eq!(
            attestation(BrowserKind::Chromium, true),
            Attestation::NotEdge
        );
        assert_eq!(
            attestation(BrowserKind::Edge, false),
            Attestation::NotDefaultTree
        );
        // Why Chromium writes none: its summary falls outside the section 4 contract.
        let id = parse_version(BrowserKind::Chromium, "Chromium 156.0.8078.79\n").unwrap();
        let text = summary_text(UNRELEASED_TOML, &id, &options_text(), &counts()).unwrap();
        assert!(matches_attestation(&text).is_err(), "{text}");
    }

    fn search_dom(cases: &[(&str, bool)], done: bool, failed: usize) -> String {
        let mut dom = String::from("<html><body><ol id=\"harness-results\">");
        for (id, ok) in cases {
            let text = if *ok {
                "ok".to_owned()
            } else {
                "list: expected [&quot;a&quot;]".to_owned()
            };
            dom.push_str(&format!(
                "<li data-case=\"{id}\" data-ok=\"{ok}\">{id}: {text}</li>"
            ));
        }
        dom.push_str(&format!(
            "</ol><p id=\"harness-summary\" data-done=\"{done}\" data-p95-ms=\"1.000\" data-run=\"{}\" data-failed=\"{failed}\">{} cases, {failed} failed</p></body></html>",
            cases.len(),
            cases.len()
        ));
        dom
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| (*id).to_owned()).collect()
    }

    #[test]
    fn search_page_with_every_case_passing_passes() {
        let dom = search_dom(&[("rk-01", true), ("skeleton_waits_300ms", true)], true, 0);
        let run = judge_case_page("search/goldens.html", "search", &dom, &ids(&["rk-01"])).unwrap();
        assert_eq!(run.cases.len(), 2);
        assert_eq!(run.summary, "2 cases, 0 failed");
    }

    #[test]
    fn a_failed_search_case_fails_and_is_named() {
        let dom = search_dom(&[("rk-01", true), ("rk-s-04", false)], true, 1);
        let failures = judge_case_page("search/goldens.html", "search", &dom, &[]).unwrap_err();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(
            failures[0].contains("search case rk-s-04 failed"),
            "{failures:?}"
        );
        assert!(failures[0].contains("[\"a\"]"), "{failures:?}");
    }

    #[test]
    fn an_unfinished_search_page_fails() {
        let dom = search_dom(&[("rk-01", true)], false, 0);
        let failures = judge_case_page("search/skeleton.html", "search", &dom, &[]).unwrap_err();
        assert!(
            failures.iter().any(|f| f.contains("did not finish")),
            "{failures:?}"
        );
    }

    #[test]
    fn a_required_search_case_that_did_not_run_fails() {
        let dom = search_dom(&[("rk-01", true)], true, 0);
        let failures = judge_case_page(
            "search/skeleton.html",
            "search",
            &dom,
            &ids(&["skeleton_waits_300ms"]),
        )
        .unwrap_err();
        assert_eq!(
            failures,
            vec!["search/skeleton.html: search case skeleton_waits_300ms did not run".to_owned()]
        );
        let failures =
            judge_case_page("search/x.html", "search", "<html></html>", &[]).unwrap_err();
        assert!(failures[0].contains("no #harness-summary"), "{failures:?}");
    }

    #[test]
    fn a_failed_shortcut_case_fails_and_is_named_in_words() {
        let mut dom = search_dom(&[("ctrl_k_opens", true)], true, 1);
        dom = dom.replace(
            "</ol>",
            "<li data-case=\"slash_in_a_text_field_types\" data-ok=\"false\">slash_in_a_text_field_types (slash in a text field types): the palette stays closed</li></ol>",
        );
        let failures = judge_case_page(
            "shortcuts/shortcuts.html",
            "shortcut",
            &dom,
            &ids(&["ctrl_k_opens", "slash_in_a_text_field_types"]),
        )
        .unwrap_err();
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(
            failures[0].starts_with(
                "shortcuts/shortcuts.html: shortcut case slash_in_a_text_field_types failed"
            ),
            "{failures:?}"
        );
        assert!(
            failures[0].contains("slash in a text field types"),
            "{failures:?}"
        );
    }

    #[test]
    fn an_unfinished_shortcut_page_fails_even_with_no_failed_case() {
        // The page --dump-dom wrote before its cases ran (the load event was not held).
        let dom = search_dom(&[("ctrl_k_opens", true)], false, 0);
        let failures = judge_case_page(
            "shortcuts/storage-throws.html",
            "shortcut",
            &dom,
            &ids(&["ctrl_k_opens", "storage_throwing_page_still_works"]),
        )
        .unwrap_err();
        assert_eq!(
            failures,
            vec![
                "shortcuts/storage-throws.html: the shortcut cases did not finish (1 reported)"
                    .to_owned(),
                "shortcuts/storage-throws.html: shortcut case storage_throwing_page_still_works did not run"
                    .to_owned(),
            ]
        );
        let empty = search_dom(&[], false, 0);
        let failures =
            judge_case_page("shortcuts/shortcuts.html", "shortcut", &empty, &[]).unwrap_err();
        assert!(
            failures.iter().any(|f| f.contains("no shortcut case ran")),
            "{failures:?}"
        );
    }

    #[test]
    fn shortcut_pages_load_the_site_scripts_and_require_every_named_case() {
        let dir = tempfile::tempdir().unwrap();
        let harness = dir.path().join("out/harness");
        let pages = prepare_shortcut_pages(&harness).unwrap();
        let modes: Vec<&str> = pages.iter().map(|p| p.mode).collect();
        assert_eq!(modes, ["shortcuts", "storage-throws"]);
        for page in &pages {
            let dir = page.path.parent().unwrap();
            assert_eq!(
                page.path,
                harness.join(format!("shortcuts/{0}/{0}.html", page.mode))
            );
            let html = fs::read_to_string(&page.path).unwrap();
            assert!(
                html.contains(&format!("data-harness=\"{}\"", page.mode)),
                "{html}"
            );
            let order: Vec<usize> = [
                "<script src=\"selftest.js\"></script>",
                "<script src=\"shortcuts.js\" defer></script>",
                "<script src=\"assets/strings.js\" defer></script>",
                "<script src=\"assets/search.js\" defer></script>",
                "<script src=\"assets/app.js\" defer></script>",
            ]
            .iter()
            .map(|tag| html.find(tag).unwrap_or_else(|| panic!("{tag} missing")))
            .collect();
            assert!(order.windows(2).all(|w| w[0] < w[1]), "{order:?}");
            assert_eq!(
                fs::read_to_string(dir.join("assets/app.js")).unwrap(),
                fieldbook_render::site::APP_JS
            );
            assert_eq!(
                fs::read_to_string(dir.join("assets/search.js")).unwrap(),
                fieldbook_render::search_page::SCRIPT
            );
            assert_eq!(
                fs::read_to_string(dir.join("assets/strings.js")).unwrap(),
                fieldbook_core::strings::js()
            );
            assert_eq!(
                fs::read_to_string(dir.join("shortcuts.js")).unwrap(),
                SHORTCUT_CASES_JS
            );
            assert!(dir.join("hold.js").is_file());
            assert!(dir.join(SELFTEST_NAME).is_file());
        }
        // The card's named cases are required, so a case that stops running fails by name.
        for id in [
            "ctrl_k_opens",
            "slash_opens",
            "slash_in_a_text_field_types",
            "esc_closes_and_returns_focus",
            "question_mark_opens_the_shortcuts_dialog",
            "switch_off_stops_single_keys_but_not_ctrl_k",
            "modal_dialogs_contain_focus",
            "palette_one_character_prefix_first",
            "palette_duplicates_appear_once",
            "palette_caps_at_eight",
            "palette_absent_link_enter_does_not_navigate",
            "palette_linked_result_enter_navigates",
            "palette_final_search_row",
        ] {
            assert!(
                pages[0].required.iter().any(|r| r == id),
                "{id} not required"
            );
            assert!(
                SHORTCUT_CASES_JS.contains(&format!("guarded('{id}'")),
                "{id} is not a case of shortcuts.js"
            );
        }
        assert_eq!(
            pages[1].required,
            ids(&["storage_throwing_page_still_works"])
        );
        // The harness opens pages with no virtual time: cases that waited for the load event
        // would run after --dump-dom had written the page.
        assert!(SHORTCUT_CASES_JS.contains("document.addEventListener('DOMContentLoaded'"));
        assert!(!SHORTCUT_CASES_JS.contains("window.addEventListener('load'"));
    }

    fn copy_dir(from: &Path, to: &Path) {
        fs::create_dir_all(to).unwrap();
        for entry in fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let target = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &target);
            } else {
                fs::copy(entry.path(), target).unwrap();
            }
        }
    }

    #[test]
    fn search_pages_load_every_wrapper_and_require_every_golden_case() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        copy_dir(
            &crate::proc::repo_root().join(golden_wrap::GOLDEN_DIR),
            &root.join(golden_wrap::GOLDEN_DIR),
        );
        let harness = root.join("out/harness");
        let pages = prepare_search_pages(root, &harness, None).unwrap();
        let modes: Vec<&str> = pages.iter().map(|p| p.mode).collect();
        assert_eq!(modes, ["goldens", "skeleton", "version"]);

        let goldens = &pages[0];
        let html = fs::read_to_string(&goldens.path).unwrap();
        assert!(html.contains("data-harness=\"goldens\""), "{html}");
        assert!(
            html.contains("<script src=\"selftest.js\"></script>"),
            "{html}"
        );
        assert!(html.contains("src=\"../../golden/search/seed-ranked.json.js\""));
        assert!(html.contains("src=\"../../golden/search/seed-ranked.json.index.js\""));
        assert!(
            !html.contains("golden/json/"),
            "report goldens are not loaded"
        );
        assert!(
            !html.contains("src=\"assets/search-index.js\""),
            "no site index given"
        );
        // Every query-side tokenize case and every ranked and page-view case is required.
        for id in ["rk-s-01", "pv-01", "ordered_hits_render_without_reranking"] {
            assert!(
                goldens.required.iter().any(|r| r == id),
                "{id} not required"
            );
        }
        assert!(
            harness.join("golden/search/seed-ranked.json.js").is_file(),
            "golden_wrap ran"
        );
        let search_js =
            fs::read_to_string(goldens.path.with_file_name("assets").join("search.js")).unwrap();
        assert_eq!(search_js, fieldbook_render::search_page::SCRIPT);

        assert_eq!(pages[1].required, ids(&["skeleton_waits_300ms"]));
        let version = &pages[2];
        let index = fs::read_to_string(
            version
                .path
                .with_file_name("assets")
                .join("search-index.js"),
        )
        .unwrap();
        let payload = index
            .strip_prefix("window.FB_INDEX = ")
            .and_then(|rest| rest.strip_suffix(';'))
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(payload).unwrap();
        assert_eq!(value["v"], 2);
        assert!(value["docs"].as_array().is_some_and(|d| !d.is_empty()));
    }

    #[test]
    fn golden_case_ids_skip_index_side_tokenize_cases_and_other_kinds() {
        let files = vec![
            (
                "search/t.json".to_owned(),
                r#"{"kind":"tokenize","cases":[{"id":"q","side":"query"},{"id":"i","side":"index"}]}"#
                    .to_owned(),
            ),
            (
                "search/r.json".to_owned(),
                r#"{"kind":"ranked","units":[],"cases":[{"id":"r"}]}"#.to_owned(),
            ),
            (
                "ids/x.json".to_owned(),
                r#"{"kind":"ids","cases":[{"id":"x"}]}"#.to_owned(),
            ),
        ];
        assert_eq!(golden_case_ids(&files).unwrap(), ids(&["q", "r"]));
    }
}
