//! Golden vector tests for every identifier family in `FAMILIES`.

use std::collections::BTreeSet;

use fieldbook_core::ids::{
    AdrId, AreaCode, CaseId, ClaimId, ConflictId, EditionId, FAMILIES, PageId, QuestionId,
    ReleaseId, SourceId, TermId, VersionId, is_error_code, next_claim_id, next_conflict_id,
    next_question_id, next_source_id,
};
use serde::Deserialize;

const GOLDEN_FILES: [(&str, &str); 13] = [
    ("area", include_str!("../../../tests/golden/ids/area.json")),
    ("page", include_str!("../../../tests/golden/ids/page.json")),
    (
        "source",
        include_str!("../../../tests/golden/ids/source.json"),
    ),
    (
        "source_version",
        include_str!("../../../tests/golden/ids/source_version.json"),
    ),
    (
        "claim",
        include_str!("../../../tests/golden/ids/claim.json"),
    ),
    (
        "question",
        include_str!("../../../tests/golden/ids/question.json"),
    ),
    (
        "conflict",
        include_str!("../../../tests/golden/ids/conflict.json"),
    ),
    ("term", include_str!("../../../tests/golden/ids/term.json")),
    ("case", include_str!("../../../tests/golden/ids/case.json")),
    (
        "error_code",
        include_str!("../../../tests/golden/ids/error_code.json"),
    ),
    ("adr", include_str!("../../../tests/golden/ids/adr.json")),
    (
        "edition",
        include_str!("../../../tests/golden/ids/edition.json"),
    ),
    (
        "tool_release",
        include_str!("../../../tests/golden/ids/tool_release.json"),
    ),
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenFile {
    kind: String,
    version: u32,
    family: String,
    cases: Vec<GoldenCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenCase {
    id: String,
    op: String,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    area: Option<String>,
    #[serde(default)]
    existing: Option<Vec<String>>,
    expect: GoldenExpect,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GoldenExpect {
    ok: bool,
    #[serde(default)]
    next: Option<String>,
    #[serde(default)]
    area: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    number: Option<u32>,
}

fn run_parse_case(family: &str, case: &GoldenCase) -> Result<(), String> {
    let Some(input) = case.input.as_deref() else {
        return Err(format!("{}: parse case missing input", case.id));
    };
    match family {
        "area" => {
            let res = AreaCode::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "page" => {
            let res = PageId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
            if let (Ok(parsed), Some(exp_area)) = (&res, case.expect.area.as_deref())
                && parsed.area().as_str() != exp_area
            {
                return Err(format!(
                    "{}: area mismatch: expected {exp_area}, got {}",
                    case.id,
                    parsed.area()
                ));
            }
        }
        "source" => {
            let res = SourceId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "source_version" => {
            let res = VersionId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
            if let Ok(parsed) = &res {
                if let Some(exp_source) = case.expect.source.as_deref()
                    && parsed.source().as_str() != exp_source
                {
                    return Err(format!(
                        "{}: source mismatch: expected {exp_source}, got {}",
                        case.id,
                        parsed.source()
                    ));
                }
                if let Some(exp_num) = case.expect.number
                    && parsed.number() != exp_num
                {
                    return Err(format!(
                        "{}: version number mismatch: expected {exp_num}, got {}",
                        case.id,
                        parsed.number()
                    ));
                }
            }
        }
        "claim" => {
            let res = ClaimId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "question" => {
            let res = QuestionId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "conflict" => {
            let res = ConflictId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "term" => {
            let res = TermId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "case" => {
            let res = CaseId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
            if let Ok(parsed) = &res {
                if let Some(exp_prefix) = case.expect.prefix.as_deref()
                    && parsed.prefix() != exp_prefix
                {
                    return Err(format!(
                        "{}: prefix mismatch: expected {exp_prefix}, got {}",
                        case.id,
                        parsed.prefix()
                    ));
                }
                if let Some(exp_num) = case.expect.number
                    && parsed.number() != exp_num
                {
                    return Err(format!(
                        "{}: number mismatch: expected {exp_num}, got {}",
                        case.id,
                        parsed.number()
                    ));
                }
            }
        }
        "error_code" => {
            let actual = is_error_code(input);
            if actual != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {actual}",
                    case.id, case.expect.ok
                ));
            }
        }
        "adr" => {
            let res = AdrId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "edition" => {
            let res = EditionId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        "tool_release" => {
            let res = ReleaseId::parse(input);
            if res.is_ok() != case.expect.ok {
                return Err(format!(
                    "{}: expected ok={}, got {:?}",
                    case.id, case.expect.ok, res
                ));
            }
        }
        other => return Err(format!("{}: unknown family {other}", case.id)),
    }
    Ok(())
}

fn run_next_case(family: &str, case: &GoldenCase) -> Result<(), String> {
    let Some(existing) = case.existing.as_deref() else {
        return Err(format!("{}: next case missing existing array", case.id));
    };
    let res: Result<String, _> = match family {
        "source" => next_source_id(existing).map(|id| id.to_string()),
        "claim" => {
            let area = case.area.as_deref().unwrap_or("cmp");
            next_claim_id(area, existing).map(|id| id.to_string())
        }
        "question" => next_question_id(existing).map(|id| id.to_string()),
        "conflict" => next_conflict_id(existing).map(|id| id.to_string()),
        other => {
            return Err(format!(
                "{}: family {other} does not support next op",
                case.id
            ));
        }
    };
    if res.is_ok() != case.expect.ok {
        return Err(format!(
            "{}: expected ok={}, got {:?}",
            case.id, case.expect.ok, res
        ));
    }
    if let (Ok(actual), Some(expected_next)) = (&res, case.expect.next.as_deref())
        && actual != expected_next
    {
        return Err(format!(
            "{}: expected next={expected_next}, got {actual}",
            case.id
        ));
    }
    Ok(())
}

#[test]
fn golden_ids_vectors_pass() {
    let mut seen_ids = BTreeSet::new();
    let mut failures = Vec::new();

    for row in &FAMILIES {
        let Some((_, text)) = GOLDEN_FILES.iter().find(|(name, _)| *name == row.name) else {
            failures.push(format!("missing golden file for family {}", row.name));
            continue;
        };
        let file: GoldenFile = match serde_json::from_str(text) {
            Ok(f) => f,
            Err(e) => {
                failures.push(format!("{}: JSON parse error: {e}", row.name));
                continue;
            }
        };
        if file.kind != "ids" || file.version != 1 || file.family != row.name {
            failures.push(format!(
                "{}: invalid golden header (kind={}, version={}, family={})",
                row.name, file.kind, file.version, file.family
            ));
        }
        if row.name == "case" {
            let in_use = ["CALC", "QA", "FC"];
            let reserved = ["EST", "QTE", "SCH", "RTE", "EAC", "EVM", "CTF"];
            let has_in_use = file.cases.iter().any(|c| {
                c.expect.ok
                    && c.expect
                        .prefix
                        .as_deref()
                        .is_some_and(|p| in_use.contains(&p))
            });
            let has_reserved = file.cases.iter().any(|c| {
                c.expect.ok
                    && c.expect
                        .prefix
                        .as_deref()
                        .is_some_and(|p| reserved.contains(&p))
            });
            if !has_in_use || !has_reserved {
                failures.push(
                    "case.json must include a valid case from a prefix in use and one from a reserved prefix"
                        .to_owned(),
                );
            }
        }
        for case in &file.cases {
            let _ = &case.note;
            if !seen_ids.insert(case.id.clone()) {
                failures.push(format!("duplicate golden case id: {}", case.id));
            }
            let outcome = match case.op.as_str() {
                "parse" => run_parse_case(row.name, case),
                "next" => run_next_case(row.name, case),
                other => Err(format!("{}: unknown op {other:?}", case.id)),
            };
            if let Err(err) = outcome {
                failures.push(err);
            }
        }
    }

    assert!(
        failures.is_empty(),
        "golden_ids failures:\n{}",
        failures.join("\n")
    );
}
