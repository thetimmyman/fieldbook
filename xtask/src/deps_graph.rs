//! Cargo graph measurement facts for `cargo xtask deps --check`.
//!
//! The one Cargo process seam of the dependency gate: `measure` runs
//! `cargo metadata --locked --offline --format-version 1` against the invocation
//! workspace for each graph target of `deny.toml` and each applicable profile
//! (`silent` = default features, `ai` = feature `net` when the real workspace
//! member has it). Every response is decoded by [`crate::deps_metadata`] before a
//! fact is taken, so a malformed or inconsistent document refuses the whole
//! measurement; there is no skip path and no fallback to the compiled checkout.
//!
//! The counting rule is in `docs/reference/dependencies.md`: the active normal
//! closure from the workspace members, unioned over the two graph targets,
//! members excluded and dev and build edges never followed; `xtask` counts toward
//! the transitive cap and never the shipped direct budget, and the uncapped
//! all-target closure is reported beside it.
//!
//! Declaration facts are kept separate from that closure. The direct maps hold
//! every workspace member's raw declared requirement, including an inactive
//! optional or target-specific declaration, and the requirement keeps its
//! operator. A policy adapter that wants a floor strips the operator itself; this
//! collector makes no budget, DEPS or pin decision.

use crate::deps_metadata::{self, Kind, Metadata};
use crate::proc;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The two graph targets of `deny.toml`.
const LINUX: &str = "x86_64-unknown-linux-gnu";
const WINDOWS: &str = "x86_64-pc-windows-msvc";
/// The crates whose own active normal external edges make a direct shipped crate.
const SHIPPED: [&str; 5] = [
    fieldbook_core::strings::binary(),
    "fieldbook-calc",
    "fieldbook-core",
    "fieldbook-render",
    "fieldbook-win",
];

/// The build profile a measurement is taken for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Profile {
    Silent,
    Ai,
}

impl Profile {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Silent => "silent",
            Self::Ai => "ai",
        }
    }
}

/// One profile's measured graph: the active normal closure over the two graph
/// targets, the workspace members' declaration facts, and the uncapped
/// all-target closure.
pub(crate) struct Graph {
    pub(crate) profile: Profile,
    pub(crate) transitive: BTreeSet<(String, String)>,
    pub(crate) shipped_direct: BTreeSet<String>,
    /// Raw declared requirements by crate name, one map per declaration kind.
    /// These are declaration facts, not the active closure: an inactive optional
    /// or target-specific declaration is present and the requirement keeps its
    /// operator.
    pub(crate) normal_direct: BTreeMap<String, BTreeSet<String>>,
    pub(crate) dev_direct: BTreeMap<String, BTreeSet<String>>,
    pub(crate) build_direct: BTreeMap<String, BTreeSet<String>>,
    pub(crate) has_calamine: bool,
    pub(crate) has_zip: bool,
    pub(crate) all_target: BTreeSet<(String, String)>,
}

impl Graph {
    /// Both workbook crates are active shipped normal dependencies, so the silent
    /// profile may use its higher direct ceiling.
    pub(crate) fn workbook(&self) -> bool {
        self.has_calamine && self.has_zip
    }
}

/// The measured graph of every applicable profile. `ai` is `None` when the real
/// workspace member has no `net` feature, which is inapplicable, never a
/// swallowed command failure.
pub(crate) struct Measurements {
    pub(crate) silent: Graph,
    pub(crate) ai: Option<Graph>,
}

/// Measure every applicable profile against the invocation workspace.
pub(crate) fn measure(root: &Path) -> Result<Measurements, String> {
    let silent = graph(root, Profile::Silent)?;
    let ai = if has_net(root)? {
        Some(graph(root, Profile::Ai)?)
    } else {
        None
    };
    Ok(Measurements { silent, ai })
}

/// Collect one profile: the active normal closure per graph target, unioned, and
/// the unfiltered all-target closure.
fn graph(root: &Path, profile: Profile) -> Result<Graph, String> {
    let mut collected = Collected::default();
    for target in [LINUX, WINDOWS] {
        collect(&metadata(root, profile, Some(target))?, &mut collected);
    }
    let mut all = Collected::default();
    collect(&metadata(root, profile, None)?, &mut all);
    Ok(Graph {
        profile,
        transitive: collected.transitive,
        shipped_direct: collected.shipped_direct,
        normal_direct: collected.normal_direct,
        dev_direct: collected.dev_direct,
        build_direct: collected.build_direct,
        has_calamine: collected.has_calamine,
        has_zip: collected.has_zip,
        all_target: all.transitive,
    })
}

/// The AI profile is applicable only when the workspace package `fieldbook`
/// declares `net`; an absent package or feature is inapplicable, never a
/// swallowed failure. A same-named external package is not the workspace member.
fn has_net(root: &Path) -> Result<bool, String> {
    let metadata = metadata(root, Profile::Silent, None)?;
    Ok(metadata
        .workspace_package(fieldbook_core::strings::binary())?
        .is_some_and(|package| package.features.contains_key("net")))
}

/// Run Cargo as the semantic parser and hand its bytes to the decoder unchanged,
/// so a malformed document cannot be normalised into accepted facts.
fn metadata(root: &Path, profile: Profile, target: Option<&str>) -> Result<Metadata, String> {
    let mut args = vec!["metadata", "--locked", "--offline", "--format-version", "1"];
    if let Some(target) = target {
        args.extend(["--filter-platform", target]);
    }
    if profile == Profile::Ai {
        args.extend(["--features", "fieldbook/net"]);
    }
    let output = proc::output("cargo", &args, root)?;
    if !output.status.success() {
        return Err(format!(
            "`cargo metadata` failed in the invocation workspace: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    deps_metadata::decode(&output.stdout)
}

/// The active normal closure and the workspace members' declaration facts for one
/// view of a decoded document.
#[derive(Default)]
struct Collected {
    transitive: BTreeSet<(String, String)>,
    shipped_direct: BTreeSet<String>,
    normal_direct: BTreeMap<String, BTreeSet<String>>,
    dev_direct: BTreeMap<String, BTreeSet<String>>,
    build_direct: BTreeMap<String, BTreeSet<String>>,
    has_calamine: bool,
    has_zip: bool,
}

/// Record one target's declaration facts and active normal closure. The decoder
/// already validated every shape, identity and join, so the document is coherent
/// here and no second validator runs.
fn collect(metadata: &Metadata, collected: &mut Collected) {
    declarations(metadata, collected);
    closure(metadata, collected);
}

/// Every workspace member's raw declared requirement, including an inactive
/// optional or target-specific declaration. A declaration that names another
/// workspace member by Cargo's directory identity is internal, not a pin fact.
/// Name equality alone cannot identify an internal dependency.
fn declarations(metadata: &Metadata, collected: &mut Collected) {
    let members = metadata.members();
    for id in members {
        let package = &metadata.packages()[id];
        for declaration in &package.declarations {
            if declaration.path.as_ref().is_some_and(|path| {
                members.iter().any(|id| {
                    let member = &metadata.packages()[id];
                    member.name == declaration.name
                        && Path::new(&member.manifest_path).parent() == Some(Path::new(path))
                })
            }) {
                continue;
            }
            let map = match declaration.kind {
                Kind::Normal => &mut collected.normal_direct,
                Kind::Dev => &mut collected.dev_direct,
                Kind::Build => &mut collected.build_direct,
            };
            map.entry(declaration.name.clone())
                .or_default()
                .insert(declaration.requirement.clone());
        }
    }
}

/// The active normal closure from the workspace members: a normal resolved edge
/// is followed, its external package is a counted transitive, and a shipped
/// member's active normal external edge is a direct shipped crate. A dev or
/// build edge is never followed and never selects the workbook ceiling.
fn closure(metadata: &Metadata, collected: &mut Collected) {
    let members = metadata.members();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut stack: Vec<&str> = members.iter().map(String::as_str).collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let node = &metadata.nodes()[id];
        let shipped =
            members.contains(id) && SHIPPED.contains(&metadata.packages()[id].name.as_str());
        for edge in &node.edges {
            if !edge.kinds.iter().any(|kind| kind.kind == Kind::Normal) {
                continue;
            }
            let package = &metadata.packages()[&edge.package];
            if !members.contains(&edge.package) {
                collected
                    .transitive
                    .insert((package.name.clone(), package.version.clone()));
                if shipped {
                    collected.shipped_direct.insert(package.name.clone());
                    collected.has_calamine |= package.name == "calamine";
                    collected.has_zip |= package.name == "zip";
                }
            }
            stack.push(edge.package.as_str());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The immutable literal facts live under `tests/fixtures/deps-graph/`. The
    /// path is bound to the crate manifest, never to the process working
    /// directory or this file's location.
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/deps-graph")
            .join(name)
    }

    fn copy(source: &Path, target: &Path) {
        std::fs::create_dir_all(target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let to = target.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy(&entry.path(), &to);
            } else {
                std::fs::copy(entry.path(), to).unwrap();
            }
        }
    }

    /// Atomically claim a unique private scratch root. A guessed path is never
    /// reused or pre-deleted; the caller owns whatever it claims and leaves it.
    fn scratch(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        loop {
            let candidate = std::env::temp_dir().join(format!(
                "fieldbook-deps-graph-{tag}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => return candidate,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("scratch {}: {error}", candidate.display()),
            }
        }
    }

    /// Copy one fixture workspace tree to a fresh private root and measure its
    /// `ws/` member workspace.
    fn measure_fixture(name: &str) -> Measurements {
        let base = scratch(name);
        copy(&fixture(name), &base);
        measure(&base.join("ws")).unwrap()
    }

    /// Render one measured profile as the literal fact lines the fixture holds.
    fn facts(graph: &Graph) -> String {
        let names = |items: Vec<String>| items.join(" ");
        let pairs = |set: &BTreeSet<(String, String)>| {
            names(
                set.iter()
                    .map(|(name, version)| format!("{name}@{version}"))
                    .collect(),
            )
        };
        let keys = |map: &BTreeMap<String, BTreeSet<String>>| names(map.keys().cloned().collect());
        format!(
            "transitive [{}]\nall [{}]\nshipped [{}]\nnormal [{}]\ndev [{}]\nbuild [{}]",
            pairs(&graph.transitive),
            pairs(&graph.all_target),
            names(graph.shipped_direct.iter().cloned().collect()),
            keys(&graph.normal_direct),
            keys(&graph.dev_direct),
            keys(&graph.build_direct),
        )
    }

    fn expected(name: &str) -> String {
        std::fs::read_to_string(fixture("expected").join(name))
            .unwrap()
            .trim_end()
            .to_owned()
    }

    /// The edge kinds, the workspace/external split and the target filter, over
    /// real Cargo fixtures: dev and build edges never count; a Windows-only edge
    /// counts for the Windows target; `xtask` is transitive only. The declaration
    /// maps keep every raw declared requirement, including the optional `netlib`
    /// and the Windows-only `winlib`. The expected facts are literal fixture
    /// files, independent of this test's output.
    #[test]
    fn deps_counts_normal_target_profile_graph() {
        let measured = measure_fixture("graph");
        assert_eq!(measured.silent.profile.name(), "silent");
        assert_eq!(facts(&measured.silent), expected("silent.txt"));
        assert!(
            measured
                .silent
                .normal_direct
                .values()
                .flatten()
                .all(|requirement| requirement.starts_with('^')),
            "a declaration fact must keep the raw requirement operator"
        );
        let ai = measured
            .ai
            .expect("the `net` feature makes the AI profile applicable");
        assert_eq!(ai.profile.name(), "ai");
        assert_eq!(facts(&ai), expected("ai.txt"));
    }

    /// An absent `net` feature on the workspace member is inapplicable, not a
    /// failure.
    #[test]
    fn deps_graph_reports_ai_inapplicable_without_the_net_feature() {
        let measured = measure_fixture("no-net");
        assert!(measured.ai.is_none());
        assert_eq!(facts(&measured.silent), expected("no-net.txt"));
    }

    /// AI binds to the workspace member's own feature: a same-named external
    /// package with `net` must not select it, and the member's `net` must.
    #[test]
    fn deps_graph_binds_ai_to_the_workspace_member_not_a_namesake() {
        let namesake = measure_fixture("namesake-no-net");
        assert!(
            namesake.ai.is_none(),
            "an external namesake must not select the AI profile"
        );
        assert_eq!(facts(&namesake.silent), expected("namesake-no-net.txt"));
        let bound = measure_fixture("workspace-net");
        assert_eq!(facts(&bound.silent), expected("workspace-net-silent.txt"));
        let ai = bound
            .ai
            .expect("the workspace member's `net` feature selects the AI profile");
        assert_eq!(facts(&ai), expected("workspace-net-ai.txt"));
    }

    /// No workspace package named `fieldbook` is inapplicable, not a refusal.
    #[test]
    fn deps_graph_reports_ai_inapplicable_without_a_workspace_package() {
        let measured = measure_fixture("no-fieldbook");
        assert!(measured.ai.is_none());
        assert_eq!(facts(&measured.silent), expected("no-fieldbook.txt"));
    }

    #[test]
    fn external_namesake_declarations_keep_their_raw_pin() {
        for (fixture, name) in [
            ("origin-control", "external-core"),
            ("origin-namesake", "fieldbook-core"),
            ("origin-inactive", "fieldbook-core"),
        ] {
            let measured = measure_fixture(fixture);
            assert_eq!(
                measured.silent.normal_direct.get(name),
                Some(&BTreeSet::from(["^9.0.0".to_owned()])),
                "a nonmember declaration cannot be excluded by package name"
            );
            if fixture == "origin-inactive" {
                assert!(measured.silent.transitive.is_empty());
                assert!(measured.silent.shipped_direct.is_empty());
            }
        }
    }

    /// The workbook ceiling needs both crates as active shipped normal edges: a
    /// dev-only `zip` or a non-shipped `xtask` edge must not select it.
    #[test]
    fn deps_graph_selects_the_workbook_ceiling_only_from_shipped_normal_edges() {
        let positive = measure_fixture("workbook");
        assert!(positive.silent.workbook());
        assert_eq!(facts(&positive.silent), expected("workbook.txt"));
        let negative = measure_fixture("workbook-dev");
        assert!(
            !negative.silent.workbook(),
            "a dev-only or non-shipped zip must not select the workbook ceiling"
        );
        assert_eq!(facts(&negative.silent), expected("workbook-dev.txt"));
        let namesake = measure_fixture("shipped-namesake");
        assert!(
            !namesake.silent.workbook(),
            "an external namesake is not a shipped member"
        );
        assert_eq!(facts(&namesake.silent), expected("shipped-namesake.txt"));
    }

    /// A failed Cargo process is an error, never an empty fact set.
    #[test]
    fn failed_cargo_is_an_error_not_empty_facts() {
        let name = concat!("bro", "ken");
        let base = scratch(name);
        copy(&fixture(name), &base);
        let Err(error) = measure(&base.join("ws")) else {
            panic!("a broken workspace must refuse");
        };
        assert!(error.contains("cargo metadata"), "{error}");
    }
}
