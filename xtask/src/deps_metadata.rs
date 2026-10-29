//! Strict facts from Cargo's format-version 1 metadata, before graph measurement.
//!
//! Cargo remains the manifest and target-condition interpreter. This module checks
//! the required declaration/resolve shapes and every identity join, retaining all
//! declaration and resolved-edge kinds. Unknown unrelated Cargo fields are ignored.
//! Only decoding can construct a Metadata; its collections are exposed read-only.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Kind {
    Normal,
    Dev,
    Build,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Dev => "dev",
            Self::Build => "build",
        }
    }
}

#[derive(Debug)]
pub(crate) struct Declaration {
    pub(crate) name: String,
    pub(crate) requirement: String,
    pub(crate) kind: Kind,
    pub(crate) target: Option<String>,
    pub(crate) path: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Package {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) manifest_path: String,
    pub(crate) features: BTreeMap<String, Vec<String>>,
    pub(crate) declarations: Vec<Declaration>,
}

#[derive(Debug)]
pub(crate) struct Edge {
    pub(crate) package: String,
    pub(crate) kinds: Vec<EdgeKind>,
}

#[derive(Debug)]
pub(crate) struct EdgeKind {
    pub(crate) kind: Kind,
    pub(crate) target: Option<String>,
}

#[derive(Debug)]
pub(crate) struct Node {
    pub(crate) id: String,
    pub(crate) edges: Vec<Edge>,
}

#[derive(Debug)]
pub(crate) struct Metadata {
    members: BTreeSet<String>,
    packages: BTreeMap<String, Package>,
    nodes: BTreeMap<String, Node>,
}

impl Metadata {
    pub(crate) fn members(&self) -> &BTreeSet<String> {
        &self.members
    }

    pub(crate) fn packages(&self) -> &BTreeMap<String, Package> {
        &self.packages
    }

    pub(crate) fn nodes(&self) -> &BTreeMap<String, Node> {
        &self.nodes
    }

    /// Select by workspace identity, never by an external package's matching name.
    pub(crate) fn workspace_package(&self, name: &str) -> Result<Option<&Package>, String> {
        let mut found = self
            .members
            .iter()
            .map(|id| &self.packages[id])
            .filter(|package| package.name == name);
        let package = found.next();
        if found.next().is_some() {
            return Err(format!(
                "cargo metadata: workspace package `{name}` is ambiguous"
            ));
        }
        Ok(package)
    }
}

pub(crate) fn decode(bytes: &[u8]) -> Result<Metadata, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|error| format!("cargo metadata: {error}"))?;
    if value.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("cargo metadata: unsupported format version".to_owned());
    }
    let mut members = BTreeSet::new();
    for id in array(&value, "workspace_members")? {
        let id = id
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "cargo metadata: invalid member id".to_owned())?;
        if !members.insert(id.to_owned()) {
            return Err("cargo metadata: duplicate member id".to_owned());
        }
    }
    let mut packages = BTreeMap::new();
    for value in array(&value, "packages")? {
        let package = package(value)?;
        if packages.insert(package.id.clone(), package).is_some() {
            return Err("cargo metadata: duplicate package id".to_owned());
        }
    }
    let mut nodes = BTreeMap::new();
    let resolve = value
        .get("resolve")
        .ok_or_else(|| "cargo metadata: missing resolve".to_owned())?;
    for value in array(resolve, "nodes")? {
        let node = node(value)?;
        if nodes.insert(node.id.clone(), node).is_some() {
            return Err("cargo metadata: duplicate node id".to_owned());
        }
    }
    for node in nodes.values() {
        if !packages.contains_key(&node.id) {
            return Err(format!("cargo metadata: unresolved node `{}`", node.id));
        }
        for edge in &node.edges {
            if !packages.contains_key(&edge.package) || !nodes.contains_key(&edge.package) {
                return Err(format!(
                    "cargo metadata: unresolved edge `{}`",
                    edge.package
                ));
            }
            let owner = &packages[&node.id];
            let dependency = &packages[&edge.package];
            for kind in &edge.kinds {
                if !owner.declarations.iter().any(|declaration| {
                    declaration.name == dependency.name
                        && declaration.kind == kind.kind
                        && declaration.target == kind.target
                }) {
                    return Err("cargo metadata: edge has no matching declaration".to_owned());
                }
            }
        }
    }
    for id in &members {
        if !packages.contains_key(id) || !nodes.contains_key(id) {
            return Err(format!("cargo metadata: unresolved member `{id}`"));
        }
    }
    Ok(Metadata {
        members,
        packages,
        nodes,
    })
}

fn package(value: &Value) -> Result<Package, String> {
    let features = value
        .get("features")
        .and_then(Value::as_object)
        .ok_or_else(|| "cargo metadata: invalid features".to_owned())?;
    let features = features
        .iter()
        .map(|(name, items)| {
            let items = items
                .as_array()
                .ok_or_else(|| "cargo metadata: feature is not an array".to_owned())?
                .iter()
                .map(|item| {
                    item.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| "cargo metadata: feature item is not text".to_owned())
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok((name.clone(), items))
        })
        .collect::<Result<_, String>>()?;
    let declarations = array(value, "dependencies")?
        .iter()
        .map(|value| {
            Ok(Declaration {
                name: text(value, "name")?.to_owned(),
                requirement: text(value, "req")?.to_owned(),
                kind: kind(value)?,
                target: target(value)?,
                path: match value.get("path") {
                    None | Some(Value::Null) => None,
                    Some(_) => Some(text(value, "path")?.to_owned()),
                },
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(Package {
        id: text(value, "id")?.to_owned(),
        name: text(value, "name")?.to_owned(),
        version: text(value, "version")?.to_owned(),
        manifest_path: text(value, "manifest_path")?.to_owned(),
        features,
        declarations,
    })
}

fn node(value: &Value) -> Result<Node, String> {
    let edges = array(value, "deps")?
        .iter()
        .map(|value| {
            let kinds = array(value, "dep_kinds")?
                .iter()
                .map(|value| {
                    Ok(EdgeKind {
                        kind: kind(value)?,
                        target: target(value)?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if kinds.is_empty() {
                return Err("cargo metadata: edge has no kinds".to_owned());
            }
            Ok(Edge {
                package: text(value, "pkg")?.to_owned(),
                kinds,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(Node {
        id: text(value, "id")?.to_owned(),
        edges,
    })
}

fn kind(value: &Value) -> Result<Kind, String> {
    match value.get("kind") {
        Some(Value::Null) => Ok(Kind::Normal),
        Some(Value::String(kind)) if kind == "normal" => Ok(Kind::Normal),
        Some(Value::String(kind)) if kind == "dev" => Ok(Kind::Dev),
        Some(Value::String(kind)) if kind == "build" => Ok(Kind::Build),
        _ => Err("cargo metadata: invalid dependency kind".to_owned()),
    }
}

fn target(value: &Value) -> Result<Option<String>, String> {
    match value.get("target") {
        Some(Value::Null) => Ok(None),
        Some(Value::String(target)) => Ok(Some(target.clone())),
        _ => Err("cargo metadata: invalid target".to_owned()),
    }
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("cargo metadata: invalid {field}"))
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("cargo metadata: invalid {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::{Path, PathBuf};

    fn fixture(path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/deps-metadata")
            .join(path)
    }

    #[test]
    fn complete_facts_and_workspace_identity_are_retained() {
        let metadata = decode(&std::fs::read(fixture("valid.json")).unwrap()).unwrap();
        let packages: Vec<_> = metadata.packages().values().map(|p| json!({
            "id":p.id,"name":p.name,"version":p.version,"manifest_path":p.manifest_path,"features":p.features,
            "declarations":p.declarations.iter().map(|d| json!([d.name,d.requirement,d.kind.name(),d.target,d.path])).collect::<Vec<_>>()
        })).collect();
        let nodes: Vec<_> = metadata.nodes().values().map(|n| json!({
            "id":n.id,"edges":n.edges.iter().map(|e| json!({"package":e.package,
                "kinds":e.kinds.iter().map(|k|json!([k.kind.name(),k.target])).collect::<Vec<_>>()})).collect::<Vec<_>>()
        })).collect();
        let actual = json!({"members":metadata.members(),"packages":packages,"nodes":nodes});
        let expected: Value =
            serde_json::from_slice(&std::fs::read(fixture("expected.json")).unwrap()).unwrap();
        assert_eq!(actual, expected);
        assert!(
            !metadata
                .workspace_package("fieldbook")
                .unwrap()
                .unwrap()
                .features
                .contains_key("net")
        );
        assert!(metadata.workspace_package("absent").unwrap().is_none());
    }

    #[test]
    fn malformed_or_inconsistent_documents_refuse() {
        let cases: Vec<(String, String)> =
            serde_json::from_slice(&std::fs::read(fixture("errors.json")).unwrap()).unwrap();
        assert!(
            cases.len() >= 51,
            "required independent refusals disappeared"
        );
        for (name, expected) in cases {
            let error = decode(&std::fs::read(fixture(&name)).unwrap()).expect_err(&name);
            assert!(error.contains(&expected), "{name}: {error}");
        }
    }

    #[test]
    fn absent_and_ambiguous_workspace_names_are_distinct() {
        let absent = decode(&std::fs::read(fixture("no-app.json")).unwrap()).unwrap();
        assert!(absent.workspace_package("fieldbook").unwrap().is_none());
        let ambiguous = decode(&std::fs::read(fixture("ambiguous-app.json")).unwrap()).unwrap();
        assert!(
            ambiguous
                .workspace_package("fieldbook")
                .unwrap_err()
                .contains("ambiguous")
        );
    }

    #[test]
    fn bytes_and_single_byte_mutations_do_not_panic() {
        let original = std::fs::read(fixture("valid.json")).unwrap();
        let inspect = |bytes: &[u8]| {
            if let Ok(metadata) = decode(bytes) {
                let _ = metadata.workspace_package("fieldbook");
            }
        };
        let seeded = std::fs::read(fixture("bytes.bin")).unwrap();
        assert_eq!(seeded.len(), 200 * 32);
        for bytes in seeded.as_chunks::<32>().0 {
            inspect(bytes);
        }
        for byte in 0..=u8::MAX {
            inspect(&[byte]);
        }
        for index in 0..original.len() {
            let mut changed = original.clone();
            changed[index] ^= 0xff;
            inspect(&changed);
            changed = original.clone();
            changed.remove(index);
            inspect(&changed);
        }
    }

    #[test]
    fn real_cargo_target_documents_decode() {
        for target in ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"] {
            let output = crate::proc::output(
                "cargo",
                &[
                    "metadata",
                    "--locked",
                    "--offline",
                    "--format-version",
                    "1",
                    "--filter-platform",
                    target,
                ],
                &crate::proc::repo_root(),
            )
            .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let metadata = decode(&output.stdout).unwrap();
            assert_eq!(
                metadata
                    .workspace_package("fieldbook")
                    .unwrap()
                    .unwrap()
                    .name,
                "fieldbook"
            );
        }
    }
}
