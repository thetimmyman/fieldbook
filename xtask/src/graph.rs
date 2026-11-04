//! The crate-graph rules, checked over `cargo metadata` (AD-05).
//!
//! The rules are a pure function over the metadata so a synthetic graph can prove
//! that each rule fails. One test runs it over this workspace.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// The shipped crates and the workspace crates each may depend on, in any dependency kind.
const SHIPPED: [(&str, &[&str]); 4] = [
    ("fieldbook-calc", &[]),
    ("fieldbook-core", &["fieldbook-calc"]),
    ("fieldbook-render", &["fieldbook-calc", "fieldbook-core"]),
    (
        fieldbook_core::strings::binary(),
        &["fieldbook-calc", "fieldbook-core", "fieldbook-render"],
    ),
];

/// Crates that bring C code or network access. Reopen only through an owner-reviewed
/// gate card: the AI-gate card for the model client, and the card that adds the first
/// network adapter of the `Pull` port for the connector client (`GuardedPull`, ADR 0004
/// section 8). Each gate names the crates it admits and guards one failure, an
/// unreviewed outbound path; everything else stays banned.
/// `cc` may enter at R2 through `embed-resource` alone. `deny.toml` bans the same list.
const BANNED: [&str; 13] = [
    "cc",
    "cmake",
    "bindgen",
    "pkg-config",
    "openssl-sys",
    "libsqlite3-sys",
    "tokio",
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "native-tls",
    "rustls",
];

/// Every rule that the metadata breaks, as sentences. Empty means the graph is sound.
pub fn violations(meta: &Value) -> Vec<String> {
    let (Some(packages), Some(nodes), Some(members)) = (
        meta["packages"].as_array(),
        meta.pointer("/resolve/nodes").and_then(Value::as_array),
        meta["workspace_members"].as_array(),
    ) else {
        return vec!["the metadata has no packages, resolve or workspace_members".to_owned()];
    };
    let name_of: BTreeMap<&str, &str> = packages
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p["name"].as_str()?)))
        .collect();
    let member_ids: BTreeSet<&str> = members.iter().filter_map(Value::as_str).collect();
    // id -> [(dependency id, true unless the edge is dev-only)]
    let edges: BTreeMap<&str, Vec<(&str, bool)>> = nodes
        .iter()
        .filter_map(|node| {
            let deps = node["deps"].as_array()?;
            let list = deps.iter().filter_map(|dep| {
                let kinds = dep["dep_kinds"].as_array()?;
                let builds =
                    kinds.is_empty() || kinds.iter().any(|k| k["kind"].as_str() != Some("dev"));
                Some((dep["pkg"].as_str()?, builds))
            });
            Some((node["id"].as_str()?, list.collect()))
        })
        .collect();

    let mut out = Vec::new();
    let name = |id: &str| name_of.get(id).copied().unwrap_or("?");
    // Rule 1: the workspace holds the four shipped crates and xtask, and no other crate.
    let member_names: BTreeSet<&str> = member_ids.iter().map(|id| name(id)).collect();
    let want_names: BTreeSet<&str> = SHIPPED
        .iter()
        .map(|(shipped, _)| *shipped)
        .chain(["xtask"])
        .collect();
    if member_names != want_names {
        out.push(format!(
            "the workspace members are {member_names:?}; the architecture allows exactly {want_names:?}"
        ));
    }
    // Rule 2: the edges between workspace crates.
    for (shipped, allowed) in SHIPPED {
        let Some(id) = member_ids.iter().find(|id| name(id) == shipped) else {
            out.push(format!("workspace crate {shipped} is missing"));
            continue;
        };
        let actual: BTreeSet<&str> = edges
            .get(id)
            .into_iter()
            .flatten()
            .filter(|(dep, _)| member_ids.contains(dep))
            .map(|(dep, _)| name(dep))
            .collect();
        let want: BTreeSet<&str> = allowed.iter().copied().collect();
        if actual != want {
            out.push(format!("{shipped} depends on {actual:?} among workspace crates; the layering allows exactly {want:?}"));
        }
    }
    for id in &member_ids {
        if edges
            .get(id)
            .into_iter()
            .flatten()
            .any(|(dep, _)| name(dep) == "xtask")
        {
            out.push(format!(
                "{} depends on xtask, which is development-only",
                name(id)
            ));
        }
    }
    // Rule 3: no banned crate in the normal or build graph of any workspace crate.
    let mut seen: BTreeSet<&str> = member_ids.clone();
    let mut queue: Vec<&str> = member_ids.iter().copied().collect();
    while let Some(id) = queue.pop() {
        for (dep, builds) in edges.get(id).into_iter().flatten() {
            if *builds && seen.insert(dep) {
                queue.push(dep);
            }
        }
    }
    for id in &seen {
        if BANNED.contains(&name(id)) {
            out.push(format!(
                "banned crate {} is in the dependency graph",
                name(id)
            ));
        }
    }
    // Rule 4: at most one version of `zip`, in any kind of dependency.
    let zips: BTreeSet<&str> = packages
        .iter()
        .filter(|p| p["name"].as_str() == Some("zip"))
        .filter_map(|p| p["version"].as_str())
        .collect();
    if zips.len() > 1 {
        out.push(format!(
            "more than one zip version is in the graph: {zips:?}"
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::violations;
    use serde_json::{Value, json};

    const MEMBERS: [&str; 5] = [
        "fieldbook-calc",
        "fieldbook-core",
        "fieldbook-render",
        fieldbook_core::strings::binary(),
        "xtask",
    ];

    /// A metadata document for the workspace: `edges` are (from, to, kind) over member
    /// names and `extra` are further packages as (name, version).
    fn metadata(edges: &[(&str, &str, Option<&str>)], extra: &[(&str, &str)]) -> Value {
        let packages: Vec<Value> = MEMBERS
            .iter()
            .map(|n| (*n, "0.0.0"))
            .chain(extra.iter().copied())
            .map(|(n, v)| json!({"id": format!("{n} {v}"), "name": n, "version": v}))
            .collect();
        let nodes: Vec<Value> = packages.iter().map(|p| {
            let id = p["id"].as_str().unwrap_or_default();
            let from = p["name"].as_str().unwrap_or_default();
            let deps: Vec<Value> = edges.iter().filter(|(f, _, _)| *f == from).map(|(_, to, kind)| {
                let version = packages.iter().find(|q| q["name"] == *to).and_then(|q| q["version"].as_str()).unwrap_or("0.0.0");
                json!({"pkg": format!("{to} {version}"), "dep_kinds": [{"kind": kind, "target": null}]})
            }).collect();
            json!({"id": id, "deps": deps})
        }).collect();
        let ids: Vec<String> = MEMBERS.iter().map(|n| format!("{n} 0.0.0")).collect();
        json!({"packages": packages, "workspace_members": ids, "resolve": {"nodes": nodes}})
    }

    const GOOD: [(&str, &str, Option<&str>); 6] = [
        ("fieldbook-core", "fieldbook-calc", None),
        ("fieldbook-render", "fieldbook-core", None),
        ("fieldbook-render", "fieldbook-calc", None),
        (fieldbook_core::strings::binary(), "fieldbook-render", None),
        (fieldbook_core::strings::binary(), "fieldbook-core", None),
        (fieldbook_core::strings::binary(), "fieldbook-calc", None),
    ];

    #[test]
    fn the_intended_layering_is_accepted() {
        assert_eq!(violations(&metadata(&GOOD, &[])), Vec::<String>::new());
    }

    // control: DP-03
    #[test]
    fn an_extra_workspace_edge_is_rejected() {
        let mut edges = GOOD.to_vec();
        edges.push(("fieldbook-calc", "fieldbook-core", None));
        assert_eq!(violations(&metadata(&edges, &[])).len(), 1);
        let mut dev_edge = GOOD.to_vec();
        dev_edge.push(("fieldbook-core", "fieldbook-render", Some("dev")));
        assert_eq!(violations(&metadata(&dev_edge, &[])).len(), 1);
    }

    // control: NS-01
    #[test]
    fn a_network_crate_is_rejected_unless_only_a_dev_dependency() {
        let mut edges = GOOD.to_vec();
        edges.push(("fieldbook-core", "tokio", None));
        let found = violations(&metadata(&edges, &[("tokio", "1.0.0")]));
        assert_eq!(
            found,
            vec!["banned crate tokio is in the dependency graph".to_owned()]
        );
        let mut dev = GOOD.to_vec();
        dev.push(("fieldbook-core", "tokio", Some("dev")));
        assert!(violations(&metadata(&dev, &[("tokio", "1.0.0")])).is_empty());
    }

    #[test]
    fn a_banned_crate_below_another_crate_is_found() {
        let mut edges = GOOD.to_vec();
        edges.extend([
            ("xtask", "ureq", None),
            ("ureq", "native-tls", Some("build")),
        ]);
        let found = violations(&metadata(
            &edges,
            &[("ureq", "3.0.0"), ("native-tls", "0.2.0")],
        ));
        assert_eq!(found.len(), 2, "{found:?}");
    }

    // control: DP-03
    #[test]
    fn two_zip_versions_are_rejected() {
        let found = violations(&metadata(&GOOD, &[("zip", "8.6.0"), ("zip", "9.0.5")]));
        assert_eq!(found.len(), 1);
        assert!(violations(&metadata(&GOOD, &[("zip", "8.6.0")])).is_empty());
    }

    #[test]
    fn a_fifth_workspace_crate_is_rejected() {
        let mut meta = metadata(&GOOD, &[]);
        meta["workspace_members"]
            .as_array_mut()
            .expect("an array")
            .push(json!("fieldbook-extra 0.0.0"));
        assert!(!violations(&meta).is_empty());
    }

    #[test]
    fn a_shipped_crate_may_not_depend_on_xtask() {
        let mut edges = GOOD.to_vec();
        edges.push((fieldbook_core::strings::binary(), "xtask", None));
        assert!(!violations(&metadata(&edges, &[])).is_empty());
    }

    #[test]
    fn this_workspace_obeys_the_layering() {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../Cargo.toml");
        let output = std::process::Command::new(cargo)
            .args([
                "metadata",
                "--format-version",
                "1",
                "--locked",
                "--manifest-path",
            ])
            .arg(manifest)
            .output()
            .expect("cargo metadata starts");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let meta: Value = serde_json::from_slice(&output.stdout).expect("metadata is JSON");
        let found = violations(&meta);
        assert!(
            found.is_empty(),
            "workspace layering broken (see AD-05):\n  {}",
            found.join("\n  ")
        );
    }
}
