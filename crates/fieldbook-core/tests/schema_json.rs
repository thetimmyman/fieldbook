//! Verification and contract tests for `schema_json` module.

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::error::Error;
use std::fmt;

use fieldbook_core::ports::{Field, Schema};
use fieldbook_core::schema_json::{
    FailureCode, JsonValue, NumberError, SCHEMA_KEYWORDS, from_json_schema, integer_from_f64,
    integer_from_u64, to_json_schema, validate,
};
use rng::Rng;
use serde::Deserialize;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};

const SCHEMA_JSON_VECTORS: &str = include_str!("../../../tests/fixtures/schema-json.json");

#[derive(Debug, Deserialize)]
struct FixtureFile {
    keywords: Vec<String>,
    cases: Vec<TestCase>,
}

#[derive(Debug, Deserialize)]
struct TestCase {
    name: String,
    keyword: String,
    schema: serde_json::Value,
    input: String,
    expect: serde_json::Value,
}

#[derive(Debug)]
enum RawError {
    Json(serde_json::Error),
    Number(NumberError),
    DuplicateKey,
}

impl fmt::Display for RawError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(e) => write!(f, "json parse error: {e}"),
            Self::Number(e) => write!(f, "number error: {e}"),
            Self::DuplicateKey => write!(f, "duplicate key in raw input"),
        }
    }
}

impl Error for RawError {}

struct JsonValueVisitor<'a> {
    recorded_error: &'a std::cell::RefCell<Option<RawError>>,
}

impl<'de> Visitor<'de> for JsonValueVisitor<'_> {
    type Value = JsonValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any valid JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
        Ok(JsonValue::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(JsonValue::Integer(v))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        match integer_from_u64(v) {
            Ok(val) => Ok(val),
            Err(e) => {
                *self.recorded_error.borrow_mut() = Some(RawError::Number(e));
                Err(E::custom(e))
            }
        }
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        match integer_from_f64(v) {
            Ok(val) => Ok(val),
            Err(e) => {
                *self.recorded_error.borrow_mut() = Some(RawError::Number(e));
                Err(E::custom(e))
            }
        }
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        Ok(JsonValue::String(v.to_string()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        Ok(JsonValue::String(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(JsonValue::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(JsonValue::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        while let Some(elem) = seq.next_element_seed(JsonValueVisitor {
            recorded_error: self.recorded_error,
        })? {
            out.push(elem);
        }
        Ok(JsonValue::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut entries = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                *self.recorded_error.borrow_mut() = Some(RawError::DuplicateKey);
                return Err(de::Error::custom("duplicate key"));
            }
            let val = map.next_value_seed(JsonValueVisitor {
                recorded_error: self.recorded_error,
            })?;
            entries.push((key, val));
        }
        Ok(JsonValue::Object(entries))
    }
}

impl<'de> de::DeserializeSeed<'de> for JsonValueVisitor<'_> {
    type Value = JsonValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

fn parse_raw(text: &str) -> Result<JsonValue, RawError> {
    let recorded_error = std::cell::RefCell::new(None);
    let mut de = serde_json::Deserializer::from_str(text);
    let visitor = JsonValueVisitor {
        recorded_error: &recorded_error,
    };
    match de.deserialize_any(visitor) {
        Ok(val) => {
            de.end().map_err(RawError::Json)?;
            Ok(val)
        }
        Err(err) => {
            if let Some(specific) = recorded_error.into_inner() {
                Err(specific)
            } else {
                Err(RawError::Json(err))
            }
        }
    }
}

fn json_from_serde_value(v: &serde_json::Value) -> Result<JsonValue, Box<dyn Error>> {
    match v {
        serde_json::Value::Null => Ok(JsonValue::Null),
        serde_json::Value::Bool(b) => Ok(JsonValue::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(JsonValue::Integer(i))
            } else if let Some(u) = n.as_u64() {
                integer_from_u64(u).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else if let Some(f) = n.as_f64() {
                integer_from_f64(f).map_err(|e| Box::new(e) as Box<dyn Error>)
            } else {
                Err("unsupported number representation".into())
            }
        }
        serde_json::Value::String(s) => Ok(JsonValue::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for elem in arr {
                out.push(json_from_serde_value(elem)?);
            }
            Ok(JsonValue::Array(out))
        }
        serde_json::Value::Object(map) => {
            let mut entries = Vec::with_capacity(map.len());
            for (k, val) in map {
                entries.push((k.clone(), json_from_serde_value(val)?));
            }
            entries.sort_by_key(|(k, _)| {
                SCHEMA_KEYWORDS
                    .iter()
                    .position(|kw| kw == k)
                    .unwrap_or(usize::MAX)
            });
            JsonValue::object(entries).map_err(|e| Box::new(e) as Box<dyn Error>)
        }
    }
}

fn run_case(_profile: &str, case: &TestCase) -> Result<(), Box<dyn Error>> {
    let schema_json = json_from_serde_value(&case.schema)?;

    if case.expect.as_str() == Some("ok") {
        let schema = from_json_schema(&schema_json)?;
        let parsed_input = parse_raw(&case.input)?;
        validate(&schema, &parsed_input)?;
        return Ok(());
    }

    if let Some(schema_err_word) = case.expect.get("schema_error").and_then(|v| v.as_str()) {
        match from_json_schema(&schema_json) {
            Err(err) => {
                let msg = err.to_string();
                if !msg.contains(schema_err_word) {
                    return Err(format!(
                        "case '{}': expected schema error mentioning '{schema_err_word}', got '{msg}'",
                        case.name
                    )
                    .into());
                }
                return Ok(());
            }
            Ok(_) => {
                return Err(format!(
                    "case '{}': expected schema error mentioning '{schema_err_word}', but parsed successfully",
                    case.name
                )
                .into());
            }
        }
    }

    if let Some(_parse_err_word) = case.expect.get("parse_error").and_then(|v| v.as_str()) {
        match parse_raw(&case.input) {
            Err(_) => return Ok(()),
            Ok(_) => {
                return Err(format!(
                    "case '{}': expected raw parse error, but parsed successfully",
                    case.name
                )
                .into());
            }
        }
    }

    if let Some(expected_code) = case.expect.get("code").and_then(|v| v.as_str()) {
        let expected_node = case.expect.get("node").and_then(|v| v.as_u64());

        if expected_code == "duplicate" {
            match parse_raw(&case.input) {
                Err(RawError::DuplicateKey) => return Ok(()),
                Ok(val) => {
                    let schema = from_json_schema(&schema_json)?;
                    match validate(&schema, &val) {
                        Err(failure) => {
                            if failure.code.as_str() != "duplicate" {
                                return Err(format!(
                                    "case '{}': expected duplicate code, got '{}'",
                                    case.name,
                                    failure.code.as_str()
                                )
                                .into());
                            }
                            if let Some(node) = expected_node
                                && failure.node as u64 != node
                            {
                                return Err(format!(
                                    "case '{}': expected node {node}, got {}",
                                    case.name, failure.node
                                )
                                .into());
                            }
                            return Ok(());
                        }
                        Ok(()) => {
                            return Err(format!(
                                "case '{}': expected validation failure, but succeeded",
                                case.name
                            )
                            .into());
                        }
                    }
                }
                Err(other) => {
                    return Err(format!(
                        "case '{}': expected duplicate key error, got '{other}'",
                        case.name
                    )
                    .into());
                }
            }
        }

        let parsed_input = parse_raw(&case.input)?;
        let schema = from_json_schema(&schema_json)?;
        match validate(&schema, &parsed_input) {
            Err(failure) => {
                if failure.code.as_str() != expected_code {
                    return Err(format!(
                        "case '{}': expected code '{expected_code}', got '{}'",
                        case.name,
                        failure.code.as_str()
                    )
                    .into());
                }
                if let Some(node) = expected_node
                    && failure.node as u64 != node
                {
                    return Err(format!(
                        "case '{}': expected node {node}, got {}",
                        case.name, failure.node
                    )
                    .into());
                }
                Ok(())
            }
            Ok(()) => Err(format!(
                "case '{}': expected failure '{expected_code}', but validation succeeded",
                case.name
            )
            .into()),
        }
    } else {
        Err(format!(
            "case '{}': unknown expectation shape {:?}",
            case.name, case.expect
        )
        .into())
    }
}

#[test]
fn schema_vectors_pass_for_model_reply_profile() -> Result<(), Box<dyn Error>> {
    let fixture: FixtureFile = serde_json::from_str(SCHEMA_JSON_VECTORS)?;
    for case in &fixture.cases {
        run_case("model_reply", case)?;
    }
    Ok(())
}

#[test]
fn schema_vectors_pass_for_mcp_arguments_profile() -> Result<(), Box<dyn Error>> {
    let fixture: FixtureFile = serde_json::from_str(SCHEMA_JSON_VECTORS)?;
    for case in &fixture.cases {
        run_case("mcp_arguments", case)?;
    }
    Ok(())
}

#[test]
fn schema_keywords_have_vectors() -> Result<(), Box<dyn Error>> {
    let fixture: FixtureFile = serde_json::from_str(SCHEMA_JSON_VECTORS)?;

    for &kw in SCHEMA_KEYWORDS {
        if !fixture.keywords.iter().any(|k| k == kw) {
            return Err(format!(
                "keyword '{kw}' from SCHEMA_KEYWORDS missing from fixture keywords"
            )
            .into());
        }
    }

    for kw in &fixture.keywords {
        if !SCHEMA_KEYWORDS.contains(&kw.as_str()) {
            return Err(format!("fixture keyword '{kw}' missing from SCHEMA_KEYWORDS").into());
        }
    }

    for &kw in SCHEMA_KEYWORDS {
        if !fixture.cases.iter().any(|c| c.keyword == kw) {
            return Err(format!("keyword '{kw}' is not exercised by any test case").into());
        }
    }

    for kw in &fixture.keywords {
        if !fixture.cases.iter().any(|c| c.keyword == kw.as_str()) {
            return Err(format!("fixture keyword '{kw}' is not exercised by any test case").into());
        }
    }

    for case in &fixture.cases {
        if !SCHEMA_KEYWORDS.contains(&case.keyword.as_str()) {
            return Err(format!(
                "case '{}' exercises unknown keyword '{}'",
                case.name, case.keyword
            )
            .into());
        }
    }

    Ok(())
}

#[test]
fn emission_round_trips() -> Result<(), Box<dyn Error>> {
    let fixture: FixtureFile = serde_json::from_str(SCHEMA_JSON_VECTORS)?;
    for case in &fixture.cases {
        if case.expect.as_str() == Some("ok") {
            let doc = json_from_serde_value(&case.schema)?;
            let parsed_schema = from_json_schema(&doc)?;
            let emitted = to_json_schema(&parsed_schema);
            if emitted != doc {
                return Err(format!(
                    "case '{}': emitted schema differs from original doc:\nemitted: {emitted:?}\noriginal: {doc:?}",
                    case.name
                )
                .into());
            }
        }
    }
    Ok(())
}

#[test]
fn integer_conversion_refuses_fractional_out_of_range_and_nonfinite() {
    assert_eq!(integer_from_f64(1.5), Err(NumberError::Fractional));
    assert_eq!(integer_from_f64(-0.5), Err(NumberError::Fractional));
    assert_eq!(integer_from_f64(0.0001), Err(NumberError::Fractional));
    assert_eq!(integer_from_f64(f64::NAN), Err(NumberError::NotFinite));
    assert_eq!(integer_from_f64(f64::INFINITY), Err(NumberError::NotFinite));
    assert_eq!(
        integer_from_f64(f64::NEG_INFINITY),
        Err(NumberError::NotFinite)
    );
    assert_eq!(integer_from_f64(1e30), Err(NumberError::OutOfRange));
    assert_eq!(integer_from_f64(-1e30), Err(NumberError::OutOfRange));
    assert_eq!(integer_from_f64(0.0), Ok(JsonValue::Integer(0)));
    assert_eq!(integer_from_f64(42.0), Ok(JsonValue::Integer(42)));
    assert_eq!(integer_from_f64(-42.0), Ok(JsonValue::Integer(-42)));

    assert_eq!(integer_from_u64(u64::MAX), Err(NumberError::OutOfRange));
    const OVERFLOW_U64: u64 = 9_223_372_036_854_775_808;
    assert_eq!(integer_from_u64(OVERFLOW_U64), Err(NumberError::OutOfRange));
    const MAX_SIGNED_U64: u64 = 9_223_372_036_854_775_807;
    assert_eq!(
        integer_from_u64(MAX_SIGNED_U64),
        Ok(JsonValue::Integer(i64::MAX))
    );
    assert_eq!(integer_from_u64(0), Ok(JsonValue::Integer(0)));
}

#[test]
fn duplicate_keys_are_refused_before_validation() -> Result<(), Box<dyn Error>> {
    let raw = r#"{"name": "a", "name": "b"}"#;
    assert!(parse_raw(raw).is_err());

    let entries = vec![
        ("secret_field".to_string(), JsonValue::Integer(1)),
        ("secret_field".to_string(), JsonValue::Integer(2)),
    ];
    let dup_err = JsonValue::object(entries).unwrap_err();
    assert_eq!(dup_err.key_len, 12);
    assert!(!format!("{dup_err}").contains("secret_field"));

    let hand_built = JsonValue::Object(vec![
        ("a".to_string(), JsonValue::Integer(1)),
        ("a".to_string(), JsonValue::Integer(2)),
    ]);
    let schema = Schema::object(vec![Field::required("a", Schema::integer(0, 10))]);
    let failure = validate(&schema, &hand_built).unwrap_err();
    assert_eq!(failure.code, FailureCode::Duplicate);
    assert_eq!(failure.node, 0);
    Ok(())
}

#[test]
fn failure_display_never_shows_the_input() {
    let schema = Schema::string(1, 5);
    let secret = "SECRET-VALUE";
    let input = JsonValue::String(secret.to_string());
    let failure = validate(&schema, &input).unwrap_err();

    let display = format!("{failure}");
    let debug = format!("{failure:?}");
    let code_display = format!("{}", failure.code);

    assert!(
        !display.contains(secret),
        "Display contained secret input: {display}"
    );
    assert!(
        !debug.contains(secret),
        "Debug contained secret input: {debug}"
    );
    assert!(
        !code_display.contains(secret),
        "FailureCode contained secret input: {code_display}"
    );
}

#[test]
fn validate_never_panics_on_random_bytes() -> Result<(), Box<dyn Error>> {
    let schema_doc = JsonValue::Object(vec![
        ("type".to_string(), JsonValue::String("object".to_string())),
        (
            "properties".to_string(),
            JsonValue::Object(vec![(
                "name".to_string(),
                JsonValue::Object(vec![
                    ("type".to_string(), JsonValue::String("string".to_string())),
                    ("minLength".to_string(), JsonValue::Integer(1)),
                    ("maxLength".to_string(), JsonValue::Integer(10)),
                ]),
            )]),
        ),
        (
            "required".to_string(),
            JsonValue::Array(vec![JsonValue::String("name".to_string())]),
        ),
        ("additionalProperties".to_string(), JsonValue::Bool(false)),
    ]);
    let schema = from_json_schema(&schema_doc)?;

    let mut rng = Rng::new(1);
    for i in 0..200usize {
        let len = i % 65;
        let bytes = rng.bytes(len);
        let text = String::from_utf8_lossy(&bytes);
        if let Ok(val) = parse_raw(&text) {
            let _ = validate(&schema, &val);
        }
    }
    Ok(())
}
