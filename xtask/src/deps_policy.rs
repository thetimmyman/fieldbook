//! Complete dependency-policy evaluation over located inputs and measured graphs.
//! The command owns I/O and activation; this module returns reports and findings.
use crate::deps_budget::{deps3, summary};
use crate::deps_graph::{Graph, Measurements};
use crate::deps_inputs::{Fact, Inputs};
use crate::deps_references::Row;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const PINS: &str = "Pins";
const DEV_ONLY: &str = "Test-only and dev-only crates";
const TOOLS: &str = "Developer tools";
const ACTIONS: &str = "CI actions";
const AI_INAPPLICABLE: &str = "deps: ai not applicable (fieldbook feature `net` absent)";

const TOOL_SECTIONS: [&str; 6] = ["cargo", "gitleaks", "poppler", "toolchain", "lock", "fuzz"];

pub(crate) struct Evaluation {
    pub(crate) report: Vec<String>,
    pub(crate) findings: Vec<String>,
}

pub(crate) fn evaluate(measurements: &Measurements, inputs: &Inputs) -> Result<Evaluation, String> {
    let mut findings = Vec::new();
    deps1(
        &measurements.silent,
        measurements.ai.as_ref(),
        inputs,
        &mut findings,
    )?;
    deps2(inputs, &mut findings)?;
    deps3(
        &measurements.silent,
        measurements.ai.as_ref(),
        &mut findings,
    );
    deps4(inputs, &mut findings);
    deps5(inputs, &mut findings)?;
    findings.sort();
    let mut report = vec![summary(&measurements.silent)];
    report.push(match &measurements.ai {
        Some(graph) => summary(graph),
        None => AI_INAPPLICABLE.to_owned(),
    });
    Ok(Evaluation { report, findings })
}

fn rows_by<'a>(rows: &'a [Row], table: &str) -> BTreeMap<&'a str, &'a Row> {
    rows.iter()
        .filter(|row| row.table == table)
        .map(|row| (row.identity.as_str(), row))
        .collect()
}

fn merge<'a>(
    into: &mut BTreeMap<String, BTreeSet<String>>,
    maps: impl Iterator<Item = (&'a String, &'a BTreeSet<String>)>,
) {
    for (name, requirements) in maps {
        into.entry(name.clone())
            .or_default()
            .extend(requirements.iter().cloned());
    }
}

fn deps1(
    silent: &Graph,
    ai: Option<&Graph>,
    inputs: &Inputs,
    findings: &mut Vec<String>,
) -> Result<(), String> {
    let pins = rows_by(&inputs.references.rows, PINS);
    let dev_only = rows_by(&inputs.references.rows, DEV_ONLY);
    let mut direct = BTreeMap::<String, BTreeSet<String>>::new();
    let mut dev = BTreeMap::<String, BTreeSet<String>>::new();
    for graph in std::iter::once(silent).chain(ai) {
        merge(
            &mut direct,
            graph.normal_direct.iter().chain(graph.build_direct.iter()),
        );
        merge(&mut dev, graph.dev_direct.iter());
    }
    for (name, requirements) in direct {
        let declared = requirement_pins(&name, &requirements)?;
        match pins.get(name.as_str()) {
            None => findings.push(format!("DEPS-1 {name}: no Pins row")),
            Some(row) => compare(findings, &name, &declared, row)?,
        }
    }
    for (name, requirements) in dev {
        let declared = requirement_pins(&name, &requirements)?;
        match pins
            .get(name.as_str())
            .or_else(|| dev_only.get(name.as_str()))
        {
            None => findings.push(format!("DEPS-1 {name}: no Test-only or Pins row")),
            Some(row) => compare(findings, &name, &declared, row)?,
        }
    }
    Ok(())
}

fn deps2(inputs: &Inputs, findings: &mut Vec<String>) -> Result<(), String> {
    let pins = rows_by(&inputs.references.rows, PINS);
    for fact in &inputs.workspace {
        if fact.section != "workspace.dependencies" {
            continue;
        }
        let name = crate_name(fact)?;
        let Some(declared) = fact
            .value
            .as_str()
            .or_else(|| fact.value.get("version").and_then(Value::as_str))
        else {
            findings.push(format!("DEPS-2 {name}: workspace pin has no version"));
            continue;
        };
        let pin = requirement_pin(name, declared)?;
        match pins.get(name) {
            None => findings.push(format!("DEPS-2 {name}: no Pins row")),
            Some(row) => {
                let recorded = row_pin(name, row)?;
                if pin != recorded {
                    findings.push(format!(
                        "DEPS-2 {name}: workspace pin {pin} differs from row {recorded}"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn crate_name(fact: &Fact) -> Result<&str, String> {
    let Value::Object(fields) = &fact.value else {
        return Ok(fact.key.as_str());
    };
    match fields.get("package") {
        None => Ok(fact.key.as_str()),
        Some(Value::String(name)) if !name.is_empty() => Ok(name.as_str()),
        Some(_) => Err(format!(
            "{}:{}: workspace pin `{}` has no usable package name",
            fact.source.path.display(),
            fact.source.line,
            fact.key
        )),
    }
}

fn deps4(inputs: &Inputs, findings: &mut Vec<String>) {
    let actions = rows_by(&inputs.references.rows, ACTIONS);
    for reference in &inputs.references.workflows {
        let text = reference.reference.as_str();
        let Some((name, sha)) = action_parts(text) else {
            findings.push(format!("DEPS-4 {text}: not owner/repo@<40 hex>"));
            continue;
        };
        match actions.get(name) {
            None => findings.push(format!("DEPS-4 {text}: no CI actions row")),
            Some(row) => {
                let recorded = row.cells.get(2).map_or("", |cell| unbacktick(cell));
                if recorded != sha {
                    findings.push(format!("DEPS-4 {text}: SHA differs from row {recorded}"));
                }
            }
        }
    }
}

fn deps5(inputs: &Inputs, findings: &mut Vec<String>) -> Result<(), String> {
    let tools = rows_by(&inputs.references.rows, TOOLS);
    for fact in &inputs.tools {
        let Some((name, pin)) = tool_pin(fact)? else {
            continue;
        };
        match tools.get(name) {
            None => findings.push(format!("DEPS-5 {name}: no Developer tools row")),
            Some(row) => {
                let recorded = row_pin(name, row)?;
                if pin != recorded {
                    findings.push(format!(
                        "DEPS-5 {name}: pin {pin} differs from row {recorded}"
                    ));
                }
            }
        }
    }
    Ok(())
}

fn tool_pin(fact: &Fact) -> Result<Option<(&str, &str)>, String> {
    if !TOOL_SECTIONS.contains(&fact.section.as_str()) {
        return Err(format!(
            "{}:{}: unsupported tool section `{}`",
            fact.source.path.display(),
            fact.source.line,
            fact.section
        ));
    }
    let Some(row) = tool_row(&fact.section, &fact.key) else {
        return Ok(None);
    };
    let Value::String(pin) = &fact.value else {
        return Err(format!(
            "{}:{}: tool pin is not a string",
            fact.source.path.display(),
            fact.source.line
        ));
    };
    Ok(Some((row, pin)))
}

fn tool_row<'a>(section: &'a str, key: &'a str) -> Option<&'a str> {
    match (section, key) {
        ("cargo", name) => Some(name),
        ("gitleaks" | "poppler", "version") => Some(section),
        ("toolchain", "channel") => Some("Rust toolchain"),
        ("lock", "nightly") => Some("Rust nightly"),
        ("fuzz", "toolchain") => Some("Rust nightly for fuzzing"),
        _ => None,
    }
}

// Cargo normalizes direct build metadata; workspace declarations stay literal.
fn compare(findings: &mut Vec<String>, name: &str, pins: &[&str], row: &Row) -> Result<(), String> {
    let recorded = row_pin(name, row)?;
    let release = without_build(recorded);
    for pin in pins {
        if without_build(pin) != release {
            findings.push(format!(
                "DEPS-1 {name}: pin {pin} differs from row {recorded}"
            ));
        }
    }
    Ok(())
}

fn requirement_pin<'a>(name: &str, requirement: &'a str) -> Result<&'a str, String> {
    let pin = requirement
        .strip_prefix('^')
        .or_else(|| requirement.strip_prefix('='))
        .unwrap_or(requirement);
    if version(pin) {
        Ok(pin)
    } else {
        Err(format!(
            "{name}: unsupported declared requirement `{requirement}`"
        ))
    }
}

fn requirement_pins<'a>(
    name: &str,
    requirements: &'a BTreeSet<String>,
) -> Result<Vec<&'a str>, String> {
    requirements
        .iter()
        .map(|requirement| requirement_pin(name, requirement))
        .collect()
}

fn version(text: &str) -> bool {
    let plain = text
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'+'));
    if !plain {
        return false;
    }
    let (release, build) = match text.split_once('+') {
        Some((release, build)) => (release, Some(build)),
        None => (text, None),
    };
    if build.is_some_and(|build| !identifiers(build, false)) {
        return false;
    }
    let (core, pre) = match release.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (release, None),
    };
    if pre.is_some_and(|pre| !identifiers(pre, true)) {
        return false;
    }
    core_parts(core)
}

fn core_parts(core: &str) -> bool {
    let mut seen = 0;
    for part in core.split('.') {
        seen += 1;
        let numeric = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        if seen > 3 || !numeric || (part.len() > 1 && part.starts_with('0')) {
            return false;
        }
        if part.parse::<u64>().is_err() {
            return false;
        }
    }
    true
}

fn identifiers(text: &str, prerelease: bool) -> bool {
    text.split('.').all(|identifier| {
        let shaped = !identifier.is_empty()
            && identifier
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-');
        let leading_zero = prerelease
            && identifier.len() > 1
            && identifier.starts_with('0')
            && identifier.bytes().all(|b| b.is_ascii_digit());
        shaped && !leading_zero
    })
}

fn without_build(pin: &str) -> &str {
    pin.split_once('+').map_or(pin, |(release, _)| release)
}

fn row_pin<'a>(name: &str, row: &'a Row) -> Result<&'a str, String> {
    let cell = row
        .cells
        .get(1)
        .ok_or_else(|| format!("{name}: row has no pin cell"))?;
    let pin = cell_pin(cell).ok_or_else(|| format!("{name}: row has no pin token"))?;
    if matches!(row.table.as_str(), PINS | DEV_ONLY) && !version(pin) {
        return Err(format!("{name}: invalid crate pin `{cell}`"));
    }
    Ok(pin)
}

fn cell_pin(cell: &str) -> Option<&str> {
    let token = unbacktick(cell).split_whitespace().next()?;
    let plain = token
        .bytes()
        .all(|b| b.is_ascii_graphic() && b != b'`' && b != b'|');
    plain.then_some(token)
}

fn unbacktick(cell: &str) -> &str {
    cell.strip_prefix('`')
        .and_then(|inner| inner.strip_suffix('`'))
        .unwrap_or(cell)
}

fn action_parts(reference: &str) -> Option<(&str, &str)> {
    let (name, sha) = reference.split_once('@')?;
    let mut parts = name.split('/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    if parts.next().is_some() || owner.is_empty() || repo.is_empty() {
        return None;
    }
    if sha.len() != 40
        || !sha
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    Some((name, sha))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deps_budget::build::{edges, graph, refs};
    use crate::deps_graph::Profile;
    use std::path::Path;

    fn inputs() -> Inputs {
        crate::deps_inputs::read(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/deps-policy/clean"),
        )
        .unwrap()
    }
    fn measured() -> Measurements {
        Measurements {
            silent: graph(
                Profile::Silent,
                &[("alpha", "^1.0.0"), ("beta", "^2.0.0")],
                2,
            ),
            ai: None,
        }
    }
    #[test]
    fn complete_evaluation_has_a_clean_report() {
        let result = evaluate(&measured(), &inputs()).unwrap();
        assert!(result.findings.is_empty());
        assert_eq!(
            result.report,
            [
                "deps: silent direct 2/12 transitive 2/80 all-target 0",
                "deps: ai not applicable (fieldbook feature `net` absent)"
            ]
        );
    }
    #[test]
    fn catalog_faults_preserve_both_direct_and_workspace_checks() {
        let mut input = inputs();
        input.references.rows.retain(|row| row.identity != "alpha");
        let result = evaluate(&measured(), &input).unwrap();
        assert_eq!(
            result.findings,
            ["DEPS-1 alpha: no Pins row", "DEPS-2 alpha: no Pins row"]
        );
        let mut measured = measured();
        measured
            .silent
            .normal_direct
            .insert("beta".into(), BTreeSet::from(["^3.0.0".into()]));
        assert!(
            evaluate(&measured, &inputs())
                .unwrap()
                .findings
                .contains(&"DEPS-1 beta: pin 3.0.0 differs from row 2.0.0".into())
        );
    }
    #[test]
    fn budgets_run_for_both_profiles() {
        let declared = edges(18);
        let measured = Measurements {
            silent: graph(Profile::Silent, &refs(&declared[..13]), 81),
            ai: Some(graph(Profile::Ai, &refs(&declared), 101)),
        };
        let result = evaluate(&measured, &inputs()).unwrap();
        for expected in [
            "DEPS-3 silent: direct shipped crates 13 exceed 12",
            "DEPS-3 silent: transitive crates 81 exceed 80",
            "DEPS-3 ai: direct shipped crates 18 exceed 17",
            "DEPS-3 ai: transitive crates 101 exceed 100",
        ] {
            assert!(result.findings.contains(&expected.into()));
        }
        assert_eq!(result.report.len(), 2);
    }
    #[test]
    fn action_and_tool_faults_are_both_reported() {
        let mut input = inputs();
        input.references.workflows[0].reference = "actions/checkout@v7".into();
        input
            .tools
            .iter_mut()
            .find(|fact| fact.key == "just")
            .unwrap()
            .value = Value::String("1.59.0".into());
        let result = evaluate(&measured(), &input).unwrap();
        assert_eq!(
            result.findings,
            [
                "DEPS-4 actions/checkout@v7: not owner/repo@<40 hex>",
                "DEPS-5 just: pin 1.59.0 differs from row 1.58.0"
            ]
        );
    }
    #[test]
    fn aliases_join_the_canonical_crate_and_build_metadata_matches_the_release() {
        let mut input = inputs();
        let pin = input
            .workspace
            .iter_mut()
            .find(|fact| fact.section == "workspace.dependencies")
            .unwrap();
        pin.key = "renamed".into();
        pin.value = serde_json::json!({"package":"alpha","version":"1.0.0"});
        input
            .references
            .rows
            .iter_mut()
            .find(|row| row.identity == "beta")
            .unwrap()
            .cells[1] = "2.0.0+example".into();
        assert!(evaluate(&measured(), &input).unwrap().findings.is_empty());
    }
    #[test]
    fn unsupported_unused_pins_and_missing_rows_cannot_mask_refusal() {
        for missing_row in [false, true] {
            let mut input = inputs();
            let mut measured = measured();
            if missing_row {
                input.references.rows.retain(|row| row.identity != "alpha");
                measured
                    .silent
                    .normal_direct
                    .insert("alpha".into(), BTreeSet::from(["~1.0.0".into()]));
            } else {
                input
                    .workspace
                    .iter_mut()
                    .find(|fact| fact.section == "workspace.dependencies")
                    .unwrap()
                    .value = Value::String("~1.0.0".into());
            }
            assert!(
                evaluate(&measured, &input)
                    .err()
                    .unwrap()
                    .contains("unsupported declared requirement `~1.0.0`")
            );
        }
    }
    #[test]
    fn malformed_crate_catalog_pins_refuse_instead_of_becoming_mismatch_findings() {
        let mut input = inputs();
        input
            .references
            .rows
            .iter_mut()
            .find(|row| row.identity == "beta")
            .unwrap()
            .cells[1] = "not-a-version".into();
        assert!(
            evaluate(&measured(), &input)
                .err()
                .unwrap()
                .contains("invalid crate pin")
        );
    }
}
