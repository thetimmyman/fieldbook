//! Policy parser and schema-1 validator (`Policy::parse`, `FB0413`, `ADR 0004 §3`,
//! `docs/reference/policy.md#rules-the-loader-enforces`).

use std::collections::BTreeSet;
use std::fmt;

use crate::audit::DestKind;
use crate::check::Code;
use crate::date::CivilDate;
use crate::hash::sha256_hex;
use crate::labels::{
    BUILTINS, LabelAllow, LabelId, LabelSet, LabelTable, MARKED, MarkingRow, MarkingRules,
    OWNER_SUPPLIED, PUBLIC, StoreId, StoreRule, UNLABELLED,
};

use super::floor::FloorKind;
use super::schema::{RawDestination, RawPolicy, RawStore};

/// Maximum policy file size in bytes (1 MiB).
pub const MAX_POLICY_BYTES: usize = 1_048_576;

/// Allowed destination kinds in `[[destination]]` rows (`git`, `edition`, and `audit` are fixed).
pub const LOADER_DEST_KINDS: [DestKind; 8] = [
    DestKind::Model,
    DestKind::Handoff,
    DestKind::Mcp,
    DestKind::Bridge,
    DestKind::Export,
    DestKind::Backup,
    DestKind::Print,
    DestKind::Clipboard,
];

/// Error returned when `Policy::parse` rejects a policy file (`FB0413`).
///
/// Names the rule, the key path (such as `destination[2].allow`), and the opaque [`LabelId`]
/// when applicable. Never stores or formats a label `title`, `banner`, or `match` text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyError {
    /// Always [`Code::Fb0413`].
    pub code: Code,
    /// Rule identifier / short description from the loader table.
    pub rule: &'static str,
    /// Key path in the policy file (e.g. `destination[2].allow`).
    pub key_path: String,
    /// Opaque label id associated with the failure, when applicable.
    pub label_id: Option<LabelId>,
}

impl PolicyError {
    /// Creates a new [`PolicyError`] (`FB0413`).
    #[must_use]
    pub fn new(rule: &'static str, key_path: impl Into<String>, label_id: Option<LabelId>) -> Self {
        Self {
            code: Code::Fb0413,
            rule,
            key_path: key_path.into(),
            label_id,
        }
    }

    /// Returns the diagnostic code (`FB0413`).
    #[must_use]
    pub const fn code(&self) -> Code {
        self.code
    }

    /// Returns the rule description.
    #[must_use]
    pub const fn rule(&self) -> &'static str {
        self.rule
    }

    /// Returns the TOML key path that failed validation.
    #[must_use]
    pub fn key_path(&self) -> &str {
        &self.key_path
    }

    /// Returns the offending label id, if one applies.
    #[must_use]
    pub const fn label_id(&self) -> Option<&LabelId> {
        self.label_id.as_ref()
    }
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.label_id {
            Some(id) => write!(
                f,
                "{}: {} at `{}` (label `{}`)",
                self.code.id(),
                self.rule,
                self.key_path,
                id.as_str()
            ),
            None => write!(
                f,
                "{}: {} at `{}`",
                self.code.id(),
                self.rule,
                self.key_path
            ),
        }
    }
}

impl std::error::Error for PolicyError {}

/// A validated `[[label]]` row in a loaded [`Policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyLabel {
    /// Opaque company label id (`[a-z]{2,4}-[0-9]{4}`).
    pub id: LabelId,
    /// Human-readable title (never journaled).
    pub title: String,
    /// Optional banner printed on pages, exports, and prints.
    pub banner: Option<String>,
    /// Whether holding this label requires a store `readers` group.
    pub restricted: bool,
}

/// A validated `[[source.rule]]` prefix mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySourceRule {
    /// Key prefix with `/` separator.
    pub prefix: String,
    /// Labels added when `prefix` matches.
    pub labels: LabelSet,
}

/// A validated `[[source.map]]` native classification mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySourceMap {
    /// Trimmed native classification text.
    pub native: String,
    /// Labels added when `native` matches.
    pub labels: LabelSet,
}

/// A validated `[[source]]` row in a loaded [`Policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicySource {
    /// System identifier in `systems.toml`.
    pub id: String,
    /// Canonical absolute path prefix or `"drop"`.
    pub root: String,
    /// Default labels applied to all items from this source.
    pub default: LabelSet,
    /// Prefix rules (`[[source.rule]]`).
    pub rules: Vec<PolicySourceRule>,
    /// Native classification map rows (`[[source.map]]`).
    pub maps: Vec<PolicySourceMap>,
}

/// A validated `[[destination]]` row in a [`Policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyDestination {
    /// Destination kind.
    pub kind: DestKind,
    /// Allowed labels (`LabelAllow`; may be empty).
    pub allow: LabelAllow,
    /// Endpoint name (`model` required, `handoff` optional).
    pub name: Option<String>,
    /// Endpoint base URL (`model` required).
    pub url: Option<String>,
    /// Inclusive expiry date.
    pub expires: Option<CivilDate>,
    /// Maximum call-record retention in days (`model` only).
    pub keep_calls_days_max: Option<u32>,
    /// Change or approval reference.
    pub reference: Option<String>,
    /// Approved MCP client name SHA-256 digests (`mcp` only).
    pub clients: Option<Vec<String>>,
    /// Approved absolute path prefixes (`export` and `backup` only).
    pub paths: Option<Vec<String>>,
    /// Whether this backup row requires `company_storage = true` (true for floor B backup).
    pub company_storage_only: bool,
}

/// The validated `[audit]` configuration of a [`Policy`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyAudit {
    /// Absolute path to the folder `audit export` writes to.
    pub sink: Option<String>,
    /// Maximum age in hours of the last `audit.export` for non-public model calls.
    pub max_unshipped_hours: Option<u64>,
    /// Whether a non-public model call requires a fresh export (`true` by default).
    pub require_fresh_export: bool,
}

impl Default for PolicyAudit {
    fn default() -> Self {
        Self {
            sink: None,
            max_unshipped_hours: None,
            require_fresh_export: true,
        }
    }
}

/// A parsed and validated company policy or built-in floor policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub(crate) revision: u64,
    pub(crate) hash: String,
    pub(crate) labels: Vec<PolicyLabel>,
    pub(crate) label_table: LabelTable,
    pub(crate) markings: MarkingRules,
    pub(crate) sources: Vec<PolicySource>,
    pub(crate) stores: Vec<StoreRule>,
    pub(crate) explicit_stores: BTreeSet<StoreId>,
    pub(crate) destinations: Vec<PolicyDestination>,
    pub(crate) audit: PolicyAudit,
    pub(crate) floor_kind: Option<FloorKind>,
}

impl Policy {
    /// Parses and validates raw bytes against policy schema 1 (`FB0413`).
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] if `bytes` exceeds 1 MiB, is not valid UTF-8, or fails any
    /// schema-1 loader rule.
    pub fn parse_bytes(bytes: &[u8], recorded_revision: Option<u64>) -> Result<Self, PolicyError> {
        if bytes.len() > MAX_POLICY_BYTES {
            return Err(PolicyError::new("file exceeds 1 MiB limit", "policy", None));
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            return Err(PolicyError::new("text is not valid UTF-8", "policy", None));
        };
        Self::parse(text, recorded_revision)
    }

    /// Parses and validates `text` against policy schema 1 (`FB0413`).
    ///
    /// - Rejects inputs larger than 1 MiB or containing UTF-8 replacement characters (`\u{FFFD}`).
    /// - Computes the policy SHA-256 digest over `text.as_bytes()` as given, including any leading
    ///   U+FEFF byte order mark.
    /// - Skips at most one leading U+FEFF before parsing; a second U+FEFF fails with `FB0413`.
    /// - Validates every rule in the loader table order of `docs/reference/policy.md`.
    ///
    /// # Errors
    ///
    /// Returns [`PolicyError`] (`FB0413`) on any schema or loader rule violation.
    pub fn parse(text: &str, recorded_revision: Option<u64>) -> Result<Self, PolicyError> {
        if text.len() > MAX_POLICY_BYTES {
            return Err(PolicyError::new("file exceeds 1 MiB limit", "policy", None));
        }
        if text.contains('\u{fffd}') {
            return Err(PolicyError::new("text is not valid UTF-8", "policy", None));
        }

        let hash = sha256_hex(text.as_bytes());
        let body = text.strip_prefix('\u{feff}').unwrap_or(text);
        if body.starts_with('\u{feff}') || body.contains('\u{feff}') {
            return Err(PolicyError::new(
                "second byte order mark is invalid",
                "policy",
                None,
            ));
        }

        // First check for unknown keys/tables or syntax errors without ever echoing values.
        let top_table: toml::Table = toml::from_str(body)
            .map_err(|_| PolicyError::new("invalid TOML syntax", "policy", None))?;
        let unknown_path = find_unknown_key_path(&top_table);

        let raw: RawPolicy = match toml::from_str(body) {
            Ok(raw) => raw,
            Err(_) => {
                let path = unknown_path.unwrap_or_else(|| "policy".to_owned());
                return Err(PolicyError::new(
                    "unknown key, table, or invalid structure",
                    path,
                    None,
                ));
            }
        };

        // Pre-scan defined label IDs that are syntactically valid opaque company IDs so rule 4
        // (undefined label in allow/holds/default/labels) can be checked in table order before rule 5.
        let mut candidate_defined = BTreeSet::new();
        for raw_lbl in &raw.label {
            if let Some(id_str) = raw_lbl.id.as_ref().and_then(toml::Value::as_str)
                && let Ok(id) = LabelId::parse(id_str)
                && !id.is_builtin()
            {
                candidate_defined.insert(id);
            }
        }
        let is_known_id = |id: &LabelId| id.is_builtin() || candidate_defined.contains(id);

        // Rule 1: `unlabelled` or `marked` in any `allow`.
        for (i, dest) in raw.destination.iter().enumerate() {
            let allow_path = format!("destination[{i}].allow");
            let items = extract_string_array(dest.allow.as_ref(), &allow_path)?;
            for item in items {
                if item == "unlabelled" {
                    return Err(PolicyError::new(
                        "unlabelled is not permitted in allow",
                        allow_path,
                        Some(UNLABELLED),
                    ));
                }
                if item == "marked" {
                    return Err(PolicyError::new(
                        "marked is not permitted in allow",
                        allow_path,
                        Some(MARKED),
                    ));
                }
            }
        }

        // Rule 2: A destination of kind `git`, `edition` or `audit` (or any unknown kind).
        let mut dest_kinds = Vec::with_capacity(raw.destination.len());
        for (i, dest) in raw.destination.iter().enumerate() {
            let kind_path = format!("destination[{i}].kind");
            let kind_str = extract_req_str(dest.kind.as_ref(), &kind_path)?;
            let Some(dk) = DestKind::parse(kind_str) else {
                return Err(PolicyError::new(
                    "unknown destination kind",
                    kind_path,
                    None,
                ));
            };
            if matches!(dk, DestKind::Git | DestKind::Edition | DestKind::Audit) {
                return Err(PolicyError::new(
                    "fixed destination kind is not permitted in policy",
                    kind_path,
                    None,
                ));
            }
            dest_kinds.push(dk);
        }

        // Rule 3: A store row for `corpus` (also check `readers`, `changers`, or `hold` on `corpus` in rule 7).
        for (i, st) in raw.store.iter().enumerate() {
            let id_path = format!("store[{i}].id");
            let id_str = extract_req_str(st.id.as_ref(), &id_path)?;
            if id_str == "corpus" {
                if st.readers.is_some() {
                    return Err(PolicyError::new(
                        "readers, changers and hold are invalid on corpus",
                        format!("store[{i}].readers"),
                        None,
                    ));
                }
                if st.changers.is_some() {
                    return Err(PolicyError::new(
                        "readers, changers and hold are invalid on corpus",
                        format!("store[{i}].changers"),
                        None,
                    ));
                }
                if st.hold.is_some() {
                    return Err(PolicyError::new(
                        "readers, changers and hold are invalid on corpus",
                        format!("store[{i}].hold"),
                        None,
                    ));
                }
                return Err(PolicyError::new(
                    "a store row for corpus is invalid",
                    id_path,
                    None,
                ));
            }
        }

        // Rule 4: An `allow`, `holds`, `default` or `labels` id that is not in the table or the built-ins.
        for (i, dest) in raw.destination.iter().enumerate() {
            let allow_path = format!("destination[{i}].allow");
            let items = extract_string_array(dest.allow.as_ref(), &allow_path)?;
            for s in items {
                let id = parse_label_id(s, &allow_path)?;
                if !is_known_id(&id) {
                    return Err(PolicyError::new(
                        "undefined label in allow",
                        allow_path,
                        Some(id),
                    ));
                }
            }
        }
        for (i, st) in raw.store.iter().enumerate() {
            let holds_path = format!("store[{i}].holds");
            for s in extract_string_array(st.holds.as_ref(), &holds_path)? {
                let id = parse_label_id(s, &holds_path)?;
                if !is_known_id(&id) {
                    return Err(PolicyError::new(
                        "undefined label in holds",
                        holds_path,
                        Some(id),
                    ));
                }
            }
            if let Some(def_val) = &st.default {
                let def_path = format!("store[{i}].default");
                let items = extract_nonempty_string_array(Some(def_val), &def_path)?;
                for s in items {
                    let id = parse_label_id(s, &def_path)?;
                    if !is_known_id(&id) {
                        return Err(PolicyError::new(
                            "undefined label in default",
                            def_path,
                            Some(id),
                        ));
                    }
                }
            }
        }
        for (i, mk) in raw.marking.iter().enumerate() {
            let lbl_path = format!("marking[{i}].labels");
            let items = extract_nonempty_string_array(mk.labels.as_ref(), &lbl_path)?;
            for s in items {
                let id = parse_label_id(s, &lbl_path)?;
                if !is_known_id(&id) {
                    return Err(PolicyError::new(
                        "undefined label in marking labels",
                        lbl_path,
                        Some(id),
                    ));
                }
            }
        }
        for (i, src) in raw.source.iter().enumerate() {
            let def_path = format!("source[{i}].default");
            for s in extract_nonempty_string_array(src.default.as_ref(), &def_path)? {
                let id = parse_label_id(s, &def_path)?;
                if !is_known_id(&id) {
                    return Err(PolicyError::new(
                        "undefined label in source default",
                        def_path,
                        Some(id),
                    ));
                }
            }
            for (j, r) in src.rule.iter().enumerate() {
                let r_path = format!("source[{i}].rule[{j}].labels");
                for s in extract_nonempty_string_array(r.labels.as_ref(), &r_path)? {
                    let id = parse_label_id(s, &r_path)?;
                    if !is_known_id(&id) {
                        return Err(PolicyError::new(
                            "undefined label in source rule labels",
                            r_path,
                            Some(id),
                        ));
                    }
                }
            }
            for (j, m) in src.map.iter().enumerate() {
                let m_path = format!("source[{i}].map[{j}].labels");
                for s in extract_nonempty_string_array(m.labels.as_ref(), &m_path)? {
                    let id = parse_label_id(s, &m_path)?;
                    if !is_known_id(&id) {
                        return Err(PolicyError::new(
                            "undefined label in source map labels",
                            m_path,
                            Some(id),
                        ));
                    }
                }
            }
        }

        // Rule 5: A company label id that is not opaque, or is a built-in; a duplicate id in
        // `[[label]]`, `[[store]]` or `[[source]]`. Also validate `[[label]]` fields.
        let mut seen_labels = BTreeSet::new();
        let mut restricted_labels = BTreeSet::new();
        let mut parsed_labels = Vec::with_capacity(raw.label.len());
        for (i, raw_lbl) in raw.label.iter().enumerate() {
            let id_path = format!("label[{i}].id");
            let id_str = extract_req_str(raw_lbl.id.as_ref(), &id_path)?;
            let Ok(id) = LabelId::parse(id_str) else {
                return Err(PolicyError::new(
                    "company label id must be opaque ([a-z]{2,4}-[0-9]{4})",
                    id_path,
                    None,
                ));
            };
            if id.is_builtin() {
                return Err(PolicyError::new(
                    "built-in label id cannot be redefined in [[label]]",
                    id_path,
                    Some(id),
                ));
            }
            if !seen_labels.insert(id.clone()) {
                return Err(PolicyError::new(
                    "duplicate label id in [[label]]",
                    id_path,
                    Some(id),
                ));
            }
            let title_path = format!("label[{i}].title");
            let title_str = extract_req_str(raw_lbl.title.as_ref(), &title_path)?;
            validate_short_text(title_str, &title_path, 1, 80)?;

            let banner = match &raw_lbl.banner {
                Some(v) => {
                    let banner_path = format!("label[{i}].banner");
                    let b_str = extract_req_str(Some(v), &banner_path)?;
                    validate_short_text(b_str, &banner_path, 1, 80)?;
                    Some(b_str.to_owned())
                }
                None => None,
            };

            let restricted = match &raw_lbl.restricted {
                Some(v) => extract_bool(v, &format!("label[{i}].restricted"))?,
                None => false,
            };
            if restricted {
                restricted_labels.insert(id.clone());
            }
            parsed_labels.push(PolicyLabel {
                id,
                title: title_str.to_owned(),
                banner,
                restricted,
            });
        }

        let mut seen_stores = BTreeSet::new();
        for (i, st) in raw.store.iter().enumerate() {
            let id_path = format!("store[{i}].id");
            let id_str = extract_req_str(st.id.as_ref(), &id_path)?;
            let Some(store_id) = StoreId::from_root_name(id_str) else {
                return Err(PolicyError::new("unknown store id", id_path, None));
            };
            if !seen_stores.insert(store_id) {
                return Err(PolicyError::new(
                    "duplicate store id in [[store]]",
                    id_path,
                    None,
                ));
            }
        }

        let mut seen_sources = BTreeSet::new();
        for (i, src) in raw.source.iter().enumerate() {
            let id_path = format!("source[{i}].id");
            let id_str = extract_req_str(src.id.as_ref(), &id_path)?;
            if id_str.is_empty()
                || !id_str
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            {
                return Err(PolicyError::new("invalid source id", id_path, None));
            }
            if !seen_sources.insert(id_str.to_owned()) {
                return Err(PolicyError::new(
                    "duplicate source id in [[source]]",
                    id_path,
                    None,
                ));
            }
        }

        // Rule 6: `owner-supplied` in a `default`, a `[[source]]` row, a `[[marking]]` row, or the
        // `holds` of any store but `state`.
        for (i, st) in raw.store.iter().enumerate() {
            let id_path = format!("store[{i}].id");
            let id_str = extract_req_str(st.id.as_ref(), &id_path)?;
            let Some(store_id) = StoreId::from_root_name(id_str) else {
                return Err(PolicyError::new("unknown store id", id_path, None));
            };
            let holds_path = format!("store[{i}].holds");
            let holds_strs = extract_string_array(st.holds.as_ref(), &holds_path)?;
            if store_id != StoreId::State && holds_strs.contains(&"owner-supplied") {
                return Err(PolicyError::new(
                    "owner-supplied is refused in holds of any store but state",
                    holds_path,
                    Some(OWNER_SUPPLIED),
                ));
            }
            if let Some(def_val) = &st.default {
                let def_path = format!("store[{i}].default");
                let def_strs = extract_nonempty_string_array(Some(def_val), &def_path)?;
                if def_strs.contains(&"owner-supplied") {
                    return Err(PolicyError::new(
                        "owner-supplied is refused in store default",
                        def_path,
                        Some(OWNER_SUPPLIED),
                    ));
                }
            }
        }
        for (i, mk) in raw.marking.iter().enumerate() {
            let lbl_path = format!("marking[{i}].labels");
            let lbl_strs = extract_nonempty_string_array(mk.labels.as_ref(), &lbl_path)?;
            if lbl_strs.contains(&"owner-supplied") {
                return Err(PolicyError::new(
                    "owner-supplied is refused in [[marking]]",
                    lbl_path,
                    Some(OWNER_SUPPLIED),
                ));
            }
        }
        for (i, src) in raw.source.iter().enumerate() {
            let def_path = format!("source[{i}].default");
            let def_strs = extract_nonempty_string_array(src.default.as_ref(), &def_path)?;
            if def_strs.contains(&"owner-supplied") {
                return Err(PolicyError::new(
                    "owner-supplied is refused in [[source]]",
                    def_path,
                    Some(OWNER_SUPPLIED),
                ));
            }
            for (j, r) in src.rule.iter().enumerate() {
                let r_path = format!("source[{i}].rule[{j}].labels");
                let r_strs = extract_nonempty_string_array(r.labels.as_ref(), &r_path)?;
                if r_strs.contains(&"owner-supplied") {
                    return Err(PolicyError::new(
                        "owner-supplied is refused in [[source.rule]]",
                        r_path,
                        Some(OWNER_SUPPLIED),
                    ));
                }
            }
            for (j, m) in src.map.iter().enumerate() {
                let m_path = format!("source[{i}].map[{j}].labels");
                let m_strs = extract_nonempty_string_array(m.labels.as_ref(), &m_path)?;
                if m_strs.contains(&"owner-supplied") {
                    return Err(PolicyError::new(
                        "owner-supplied is refused in [[source.map]]",
                        m_path,
                        Some(OWNER_SUPPLIED),
                    ));
                }
            }
        }

        // Rule 7: A restricted label in the `holds` of a store with no `readers`;
        // `readers`, `changers` or `hold` on `corpus`.
        // Rule 8: A `default` that is not a subset of `holds`.
        // Rule 9: A store `default` or global `[[marking]].labels` that normalizes to public-only.
        let mut parsed_stores = Vec::with_capacity(raw.store.len());
        for (i, st) in raw.store.iter().enumerate() {
            parsed_stores.push(validate_store_row(i, st, &restricted_labels)?);
        }

        let mut parsed_markings = Vec::with_capacity(raw.marking.len());
        for (i, mk) in raw.marking.iter().enumerate() {
            let match_path = format!("marking[{i}].match");
            let match_str = extract_req_str(mk.matched.as_ref(), &match_path)?;
            validate_short_text(match_str, &match_path, 3, 80)?;
            if match_str.trim().is_empty() {
                return Err(PolicyError::new(
                    "marking match cannot be whitespace-only",
                    match_path,
                    None,
                ));
            }

            let lbl_path = format!("marking[{i}].labels");
            let lbl_strs = extract_nonempty_string_array(mk.labels.as_ref(), &lbl_path)?;
            let mut ids = Vec::with_capacity(lbl_strs.len());
            for s in lbl_strs {
                let id = parse_label_id(s, &lbl_path)?;
                if id == UNLABELLED || id == MARKED {
                    return Err(PolicyError::new(
                        "unlabelled and marked are refused in [[marking]].labels",
                        lbl_path,
                        Some(id),
                    ));
                }
                ids.push(id);
            }
            let set = LabelSet::from_ids(ids);
            if set.is_public() {
                return Err(PolicyError::new(
                    "global [[marking]].labels cannot normalize to public-only",
                    lbl_path,
                    Some(PUBLIC),
                ));
            }
            parsed_markings.push(MarkingRow::new(match_str.to_owned(), set));
        }

        // Rule 10: A `[[source]]` row with no `root`, or a relative `root`.
        let mut parsed_sources = Vec::with_capacity(raw.source.len());
        for (i, src) in raw.source.iter().enumerate() {
            let id_path = format!("source[{i}].id");
            let id_str = extract_req_str(src.id.as_ref(), &id_path)?.to_owned();
            let root_path = format!("source[{i}].root");
            let root_str = extract_req_str(src.root.as_ref(), &root_path)?;
            if root_str != "drop" && canonical_path_components(root_str).is_none() {
                return Err(PolicyError::new(
                    "source root must be an absolute path or 'drop'",
                    root_path,
                    None,
                ));
            }
            let def_path = format!("source[{i}].default");
            let def_ids = parse_label_id_vec(
                &extract_nonempty_string_array(src.default.as_ref(), &def_path)?,
                &def_path,
            )?;
            let default = LabelSet::from_ids(def_ids);

            let mut rules = Vec::with_capacity(src.rule.len());
            for (j, r) in src.rule.iter().enumerate() {
                let p_path = format!("source[{i}].rule[{j}].prefix");
                let prefix = extract_req_str(r.prefix.as_ref(), &p_path)?;
                if prefix.is_empty() {
                    return Err(PolicyError::new(
                        "source rule prefix cannot be empty",
                        p_path,
                        None,
                    ));
                }
                let l_path = format!("source[{i}].rule[{j}].labels");
                let l_ids = parse_label_id_vec(
                    &extract_nonempty_string_array(r.labels.as_ref(), &l_path)?,
                    &l_path,
                )?;
                rules.push(PolicySourceRule {
                    prefix: prefix.to_owned(),
                    labels: LabelSet::from_ids(l_ids),
                });
            }

            let mut maps = Vec::with_capacity(src.map.len());
            for (j, m) in src.map.iter().enumerate() {
                let n_path = format!("source[{i}].map[{j}].native");
                let native = extract_req_str(m.native.as_ref(), &n_path)?.trim();
                if native.is_empty() {
                    return Err(PolicyError::new(
                        "source map native text cannot be empty",
                        n_path,
                        None,
                    ));
                }
                let l_path = format!("source[{i}].map[{j}].labels");
                let l_ids = parse_label_id_vec(
                    &extract_nonempty_string_array(m.labels.as_ref(), &l_path)?,
                    &l_path,
                )?;
                maps.push(PolicySourceMap {
                    native: native.to_owned(),
                    labels: LabelSet::from_ids(l_ids),
                });
            }

            parsed_sources.push(PolicySource {
                id: id_str,
                root: root_str.to_owned(),
                default,
                rules,
                maps,
            });
        }

        // Rule 11: A non-public label in an `mcp` or `clipboard` row.
        for (i, dest) in raw.destination.iter().enumerate() {
            let dk = dest_kinds[i];
            if matches!(dk, DestKind::Mcp | DestKind::Clipboard) {
                let allow_path = format!("destination[{i}].allow");
                let items = extract_string_array(dest.allow.as_ref(), &allow_path)?;
                for s in items {
                    let id = parse_label_id(s, &allow_path)?;
                    if id != PUBLIC {
                        return Err(PolicyError::new(
                            "mcp and clipboard rows allow public only",
                            allow_path,
                            Some(id),
                        ));
                    }
                }
            }
        }

        // Rule 13 & Rule 12: `[audit]` table validation (`require_fresh_export` boolean check,
        // then `model` row with non-public label needing `[audit]` `sink` and `max_unshipped_hours`).
        let parsed_audit = validate_audit_table(raw.audit.as_ref())?;
        for (i, dest) in raw.destination.iter().enumerate() {
            if dest_kinds[i] == DestKind::Model {
                let allow_path = format!("destination[{i}].allow");
                let items = extract_string_array(dest.allow.as_ref(), &allow_path)?;
                let has_non_public = items.iter().any(|&s| s != "public");
                if has_non_public && parsed_audit.require_fresh_export {
                    if parsed_audit.sink.is_none() {
                        return Err(PolicyError::new(
                            "model row with a non-public label requires audit.sink",
                            "audit.sink",
                            None,
                        ));
                    }
                    if parsed_audit.max_unshipped_hours.is_none() {
                        return Err(PolicyError::new(
                            "model row with a non-public label requires audit.max_unshipped_hours",
                            "audit.max_unshipped_hours",
                            None,
                        ));
                    }
                }
            }
        }

        // Rules 14, 15, 16: destination row field validations (`url`, `paths`, `clients`, `expires`, etc.).
        let mut parsed_destinations = Vec::with_capacity(raw.destination.len());
        for (i, dest) in raw.destination.iter().enumerate() {
            parsed_destinations.push(validate_destination_row(i, dest_kinds[i], dest)?);
        }

        // Rule 17: `example = true`.
        if let Some(ex_val) = &raw.example {
            let is_example = extract_bool(ex_val, "example")?;
            if is_example {
                return Err(PolicyError::new(
                    "example policy file cannot be loaded",
                    "example",
                    None,
                ));
            }
        }

        // Rule 18: Unknown `policy_schema` (or any remaining unknown key).
        if let Some(path) = unknown_path {
            return Err(PolicyError::new("unknown key or table", path, None));
        }
        let schema_val = raw
            .policy_schema
            .as_ref()
            .and_then(toml::Value::as_integer)
            .ok_or_else(|| {
                PolicyError::new("policy_schema must be integer 1", "policy_schema", None)
            })?;
        if schema_val != 1 {
            return Err(PolicyError::new(
                "unsupported policy_schema (expected 1)",
                "policy_schema",
                None,
            ));
        }

        // Rule 19: `revision` at least 1 and not lower than `recorded_revision`.
        let rev_int = raw
            .revision
            .as_ref()
            .and_then(toml::Value::as_integer)
            .ok_or_else(|| {
                PolicyError::new("revision must be a positive integer", "revision", None)
            })?;
        let Ok(revision) = u64::try_from(rev_int) else {
            return Err(PolicyError::new(
                "revision must be at least 1",
                "revision",
                None,
            ));
        };
        if revision == 0 {
            return Err(PolicyError::new(
                "revision must be at least 1",
                "revision",
                None,
            ));
        }
        if let Some(recorded) = recorded_revision
            && revision < recorded
        {
            return Err(PolicyError::new(
                "revision is lower than recorded revision",
                "revision",
                None,
            ));
        }

        let label_table = LabelTable::with(
            parsed_labels.iter().map(|l| l.id.clone()),
            restricted_labels,
        );

        Ok(Self {
            revision,
            hash,
            labels: parsed_labels,
            label_table,
            markings: MarkingRules::with_rows(parsed_markings),
            sources: parsed_sources,
            stores: parsed_stores,
            explicit_stores: seen_stores,
            destinations: parsed_destinations,
            audit: parsed_audit,
            floor_kind: None,
        })
    }

    /// Returns the policy revision number (`0` for the built-in floor).
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the 64-character lowercase hex SHA-256 of the policy.
    #[must_use]
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Returns the defined `[[label]]` rows.
    #[must_use]
    pub fn labels(&self) -> &[PolicyLabel] {
        &self.labels
    }

    /// Looks up a `[[label]]` row by its [`LabelId`].
    #[must_use]
    pub fn label(&self, id: &LabelId) -> Option<&PolicyLabel> {
        self.labels.iter().find(|l| &l.id == id)
    }

    /// Returns the [`LabelTable`] of built-in and policy-defined labels.
    #[must_use]
    pub const fn label_table(&self) -> &LabelTable {
        &self.label_table
    }

    /// Returns the [`MarkingRules`] built from this policy's `[[marking]]` rows.
    #[must_use]
    pub fn marking_rules(&self) -> MarkingRules {
        self.markings.clone()
    }

    /// Looks up a `[[source]]` row by `system_id`.
    #[must_use]
    pub fn source(&self, system_id: &str) -> Option<&PolicySource> {
        self.sources.iter().find(|s| s.id == system_id)
    }

    /// Looks up the explicit `[[store]]` row for `store`, if the policy defines one.
    #[must_use]
    pub fn store_row(&self, store: &StoreId) -> Option<&StoreRule> {
        if self.explicit_stores.contains(store) {
            self.stores.iter().find(|r| &r.store == store)
        } else {
            None
        }
    }

    /// Returns the store rules of this policy.
    #[must_use]
    pub fn stores(&self) -> &[StoreRule] {
        &self.stores
    }

    /// Returns the destination rows of this policy.
    #[must_use]
    pub fn destinations(&self) -> &[PolicyDestination] {
        &self.destinations
    }

    /// Returns the `[audit]` configuration of this policy.
    #[must_use]
    pub const fn audit(&self) -> &PolicyAudit {
        &self.audit
    }
}

fn validate_store_row(
    i: usize,
    st: &RawStore,
    restricted_labels: &BTreeSet<LabelId>,
) -> Result<StoreRule, PolicyError> {
    let id_path = format!("store[{i}].id");
    let id_str = extract_req_str(st.id.as_ref(), &id_path)?;
    let Some(store_id) = StoreId::from_root_name(id_str) else {
        return Err(PolicyError::new("unknown store id", id_path, None));
    };

    let holds_path = format!("store[{i}].holds");
    let holds_strs = extract_string_array(st.holds.as_ref(), &holds_path)?;
    let holds_ids = parse_label_id_vec(&holds_strs, &holds_path)?;
    let holds = LabelAllow::from_ids(holds_ids);

    let readers = match &st.readers {
        Some(v) => {
            let r_path = format!("store[{i}].readers");
            let s = extract_req_str(Some(v), &r_path)?;
            validate_short_text(s, &r_path, 1, 128)?;
            Some(s.to_owned())
        }
        None => None,
    };

    let changers = match &st.changers {
        Some(v) => {
            let c_path = format!("store[{i}].changers");
            let s = extract_req_str(Some(v), &c_path)?;
            validate_short_text(s, &c_path, 1, 128)?;
            Some(s.to_owned())
        }
        None => None,
    };

    let hold = match &st.hold {
        Some(v) => extract_bool(v, &format!("store[{i}].hold"))?,
        None => false,
    };

    // Rule 7: A restricted label in `holds` requires `readers`.
    for id in holds.ids() {
        if restricted_labels.contains(id) && readers.is_none() {
            return Err(PolicyError::new(
                "restricted label in holds requires store readers",
                &holds_path,
                Some(id.clone()),
            ));
        }
    }

    // Rule 8 & Rule 9: `default` must be a subset of `holds` and cannot normalize to `public`-only.
    let default = match &st.default {
        Some(def_val) => {
            let def_path = format!("store[{i}].default");
            let def_strs = extract_nonempty_string_array(Some(def_val), &def_path)?;
            let def_ids = parse_label_id_vec(&def_strs, &def_path)?;
            for id in &def_ids {
                if !holds.contains(id) {
                    return Err(PolicyError::new(
                        "store default must be a subset of holds",
                        def_path,
                        Some(id.clone()),
                    ));
                }
            }
            let set = LabelSet::from_ids(def_ids);
            if set.is_public() {
                return Err(PolicyError::new(
                    "store default cannot normalize to public-only",
                    def_path,
                    Some(PUBLIC),
                ));
            }
            set
        }
        None => LabelSet::unlabelled(),
    };

    Ok(StoreRule {
        store: store_id,
        holds,
        default,
        readers,
        changers,
        hold,
    })
}

fn validate_audit_table(raw: Option<&super::schema::RawAudit>) -> Result<PolicyAudit, PolicyError> {
    let Some(audit) = raw else {
        return Ok(PolicyAudit::default());
    };

    // Rule 13: `require_fresh_export` must be a boolean when present.
    let require_fresh_export = match &audit.require_fresh_export {
        Some(val) => extract_bool(val, "audit.require_fresh_export")?,
        None => true,
    };

    let sink = match &audit.sink {
        Some(val) => {
            let s = extract_req_str(Some(val), "audit.sink")?;
            if canonical_path_components(s).is_none() {
                return Err(PolicyError::new(
                    "audit.sink must be an absolute path",
                    "audit.sink",
                    None,
                ));
            }
            Some(s.to_owned())
        }
        None => {
            if require_fresh_export {
                return Err(PolicyError::new(
                    "audit.sink is required when [audit] is present and require_fresh_export is true",
                    "audit.sink",
                    None,
                ));
            }
            None
        }
    };

    let max_unshipped_hours = match &audit.max_unshipped_hours {
        Some(val) => {
            let n = val.as_integer().ok_or_else(|| {
                PolicyError::new(
                    "audit.max_unshipped_hours must be a positive integer",
                    "audit.max_unshipped_hours",
                    None,
                )
            })?;
            let Ok(hours) = u64::try_from(n) else {
                return Err(PolicyError::new(
                    "audit.max_unshipped_hours must be at least 1",
                    "audit.max_unshipped_hours",
                    None,
                ));
            };
            if hours == 0 {
                return Err(PolicyError::new(
                    "audit.max_unshipped_hours must be at least 1",
                    "audit.max_unshipped_hours",
                    None,
                ));
            }
            Some(hours)
        }
        None => None,
    };

    Ok(PolicyAudit {
        sink,
        max_unshipped_hours,
        require_fresh_export,
    })
}

fn validate_destination_row(
    i: usize,
    kind: DestKind,
    dest: &RawDestination,
) -> Result<PolicyDestination, PolicyError> {
    let allow_path = format!("destination[{i}].allow");
    let allow_strs = extract_string_array(dest.allow.as_ref(), &allow_path)?;
    let allow_ids = parse_label_id_vec(&allow_strs, &allow_path)?;
    let allow = LabelAllow::from_ids(allow_ids);

    // Rule 14: `model` row needs an `https` or loopback `url` (and `name`); `url` invalid on other kinds.
    let url = match &dest.url {
        Some(val) => {
            let u_path = format!("destination[{i}].url");
            if kind != DestKind::Model {
                return Err(PolicyError::new(
                    "url is only valid on model rows",
                    u_path,
                    None,
                ));
            }
            let u_str = extract_req_str(Some(val), &u_path)?;
            if !is_https_or_loopback_url(u_str) {
                return Err(PolicyError::new(
                    "model url must be https or loopback http",
                    u_path,
                    None,
                ));
            }
            Some(u_str.to_owned())
        }
        None => {
            if kind == DestKind::Model {
                return Err(PolicyError::new(
                    "model row requires url",
                    format!("destination[{i}].url"),
                    None,
                ));
            }
            None
        }
    };

    // `name`: required on `model`, optional on `handoff`, invalid on other kinds.
    let name = match &dest.name {
        Some(val) => {
            let n_path = format!("destination[{i}].name");
            if !matches!(kind, DestKind::Model | DestKind::Handoff) {
                return Err(PolicyError::new(
                    "name is only valid on model and handoff rows",
                    n_path,
                    None,
                ));
            }
            let n_str = extract_req_str(Some(val), &n_path)?;
            validate_short_text(n_str, &n_path, 1, 64)?;
            Some(n_str.to_owned())
        }
        None => {
            if kind == DestKind::Model {
                return Err(PolicyError::new(
                    "model row requires name",
                    format!("destination[{i}].name"),
                    None,
                ));
            }
            None
        }
    };

    // Rule 15: `paths` only on `export` and `backup` (and required there); each must be a valid
    // Windows or POSIX absolute path without `.` or `..`.
    let paths = match &dest.paths {
        Some(val) => {
            let p_path = format!("destination[{i}].paths");
            if !matches!(kind, DestKind::Export | DestKind::Backup) {
                return Err(PolicyError::new(
                    "paths is only valid on export and backup rows",
                    p_path,
                    None,
                ));
            }
            let items = extract_nonempty_string_array(Some(val), &p_path)?;
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                if canonical_path_components(item).is_none() {
                    return Err(PolicyError::new(
                        "destination path must be an absolute path without '..'",
                        p_path,
                        None,
                    ));
                }
                out.push(item.to_owned());
            }
            Some(out)
        }
        None => {
            if matches!(kind, DestKind::Export | DestKind::Backup) {
                return Err(PolicyError::new(
                    "export and backup rows require paths",
                    format!("destination[{i}].paths"),
                    None,
                ));
            }
            None
        }
    };

    // Rule 16: `clients` only on `mcp`.
    let clients = match &dest.clients {
        Some(val) => {
            let c_path = format!("destination[{i}].clients");
            if kind != DestKind::Mcp {
                return Err(PolicyError::new(
                    "clients is only valid on mcp rows",
                    c_path,
                    None,
                ));
            }
            let items = extract_nonempty_string_array(Some(val), &c_path)?;
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                if item.len() != 64
                    || !item
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                {
                    return Err(PolicyError::new(
                        "client hash must be 64 lowercase hex digits",
                        c_path,
                        None,
                    ));
                }
                out.push(item.to_owned());
            }
            Some(out)
        }
        None => None,
    };

    // `expires`: must be an unquoted TOML local date.
    let expires = match &dest.expires {
        Some(val) => {
            let e_path = format!("destination[{i}].expires");
            let toml::Value::Datetime(dt) = val else {
                return Err(PolicyError::new(
                    "expires must be an unquoted TOML local date",
                    e_path,
                    None,
                ));
            };
            if dt.time.is_some() || dt.offset.is_some() {
                return Err(PolicyError::new(
                    "expires must be a local date without time or offset",
                    e_path,
                    None,
                ));
            }
            let Some(d) = dt.date else {
                return Err(PolicyError::new("expires is missing date", e_path, None));
            };
            let date = CivilDate::from_ymd(d.year, d.month, d.day).map_err(|_| {
                PolicyError::new("expires is not a valid calendar date", e_path, None)
            })?;
            Some(date)
        }
        None => None,
    };

    // `keep_calls_days_max`: only on `model`, integer >= 0.
    let keep_calls_days_max = match &dest.keep_calls_days_max {
        Some(val) => {
            let k_path = format!("destination[{i}].keep_calls_days_max");
            if kind != DestKind::Model {
                return Err(PolicyError::new(
                    "keep_calls_days_max is only valid on model rows",
                    k_path,
                    None,
                ));
            }
            let n = val.as_integer().ok_or_else(|| {
                PolicyError::new("keep_calls_days_max must be an integer", &k_path, None)
            })?;
            let days = u32::try_from(n)
                .map_err(|_| PolicyError::new("keep_calls_days_max must be >= 0", &k_path, None))?;
            Some(days)
        }
        None => None,
    };

    // `ref`: up to 80 code points, no control character.
    let reference = match &dest.reference {
        Some(val) => {
            let r_path = format!("destination[{i}].ref");
            let s = extract_req_str(Some(val), &r_path)?;
            validate_short_text(s, &r_path, 1, 80)?;
            Some(s.to_owned())
        }
        None => None,
    };

    Ok(PolicyDestination {
        kind,
        allow,
        name,
        url,
        expires,
        keep_calls_days_max,
        reference,
        clients,
        paths,
        company_storage_only: false,
    })
}

/// Parses an absolute Windows drive (`C:\...`, `C:/...`), UNC (`\\server\share\...`), or POSIX
/// (`/...`) path into normalized whole components, rejecting relative paths, empty paths, and
/// `.` or `..` components.
#[must_use]
pub fn canonical_path_components(raw: &str) -> Option<Vec<String>> {
    if raw.is_empty() || raw.chars().any(char::is_control) {
        return None;
    }
    let bytes = raw.as_bytes();
    let (prefix, rest, windows_path) = if let Some(trimmed) = raw.strip_prefix("\\\\") {
        let mut parts = trimmed.split(['\\', '/']);
        let server = parts.next().filter(|s| !s.is_empty())?;
        let share = parts.next().filter(|s| !s.is_empty())?;
        if server == "." || server == ".." || share == "." || share == ".." {
            return None;
        }
        let consumed = 2 + server.len() + 1 + share.len();
        let tail = raw.get(consumed..).unwrap_or("");
        (
            vec![format!(
                "\\\\{}\\{}",
                server.to_ascii_lowercase(),
                share.to_ascii_lowercase()
            )],
            tail,
            true,
        )
    } else if let Some(stripped) = raw.strip_prefix('/') {
        (vec!["/".to_owned()], stripped, false)
    } else if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
    {
        let drive = (bytes[0].to_ascii_lowercase() as char).to_string() + ":";
        (vec![drive], &raw[3..], true)
    } else {
        return None;
    };

    let mut comps = prefix;
    if windows_path {
        for seg in rest.split(['/', '\\']) {
            if seg.is_empty() {
                continue;
            }
            if seg == "." || seg == ".." {
                return None;
            }
            comps.push(seg.to_ascii_lowercase());
        }
    } else {
        for seg in rest.split(['/', '\\']) {
            if seg.is_empty() {
                continue;
            }
            if seg == "." || seg == ".." {
                return None;
            }
            comps.push(seg.to_owned());
        }
    }
    Some(comps)
}

/// Returns `true` when `target` is inside `prefix` on whole canonical path components.
#[must_use]
pub fn path_matches_prefix(target: &str, prefix: &str) -> bool {
    let Some(t_comps) = canonical_path_components(target) else {
        return false;
    };
    let Some(p_comps) = canonical_path_components(prefix) else {
        return false;
    };
    t_comps.len() >= p_comps.len() && t_comps[..p_comps.len()] == p_comps[..]
}

fn is_https_or_loopback_url(url: &str) -> bool {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return false;
    }
    let lower = url.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://") {
        let host = rest.split(['/', ':', '?', '#']).next().unwrap_or("");
        return !host.is_empty();
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        for loopback in ["127.0.0.1", "localhost", "[::1]"] {
            if let Some(after) = rest.strip_prefix(loopback)
                && (after.is_empty() || after.starts_with([':', '/', '?', '#']))
            {
                return true;
            }
        }
    }
    false
}

fn extract_req_str<'a>(
    val: Option<&'a toml::Value>,
    key_path: &str,
) -> Result<&'a str, PolicyError> {
    val.and_then(toml::Value::as_str)
        .ok_or_else(|| PolicyError::new("expected string value", key_path, None))
}

fn extract_bool(val: &toml::Value, key_path: &str) -> Result<bool, PolicyError> {
    val.as_bool()
        .ok_or_else(|| PolicyError::new("expected boolean value", key_path, None))
}

fn extract_string_array<'a>(
    val: Option<&'a toml::Value>,
    key_path: &str,
) -> Result<Vec<&'a str>, PolicyError> {
    let arr = val
        .and_then(toml::Value::as_array)
        .ok_or_else(|| PolicyError::new("expected array of strings", key_path, None))?;
    let mut out = Vec::with_capacity(arr.len());
    for item in arr {
        let s = item
            .as_str()
            .ok_or_else(|| PolicyError::new("expected string element in array", key_path, None))?;
        out.push(s);
    }
    Ok(out)
}

fn extract_nonempty_string_array<'a>(
    val: Option<&'a toml::Value>,
    key_path: &str,
) -> Result<Vec<&'a str>, PolicyError> {
    let out = extract_string_array(val, key_path)?;
    if out.is_empty() {
        return Err(PolicyError::new(
            "label array must not be empty",
            key_path,
            None,
        ));
    }
    Ok(out)
}

fn parse_label_id(raw: &str, key_path: &str) -> Result<LabelId, PolicyError> {
    for builtin in BUILTINS {
        if builtin.as_str() == raw {
            return Ok(builtin);
        }
    }
    LabelId::parse(raw)
        .map_err(|_| PolicyError::new("invalid or undefined label id", key_path, None))
}

fn parse_label_id_vec(items: &[&str], key_path: &str) -> Result<Vec<LabelId>, PolicyError> {
    let mut out = Vec::with_capacity(items.len());
    for &s in items {
        out.push(parse_label_id(s, key_path)?);
    }
    Ok(out)
}

fn validate_short_text(
    text: &str,
    key_path: &str,
    min_chars: usize,
    max_chars: usize,
) -> Result<(), PolicyError> {
    let count = text.chars().count();
    if count < min_chars || count > max_chars {
        return Err(PolicyError::new(
            "text length out of bounds",
            key_path,
            None,
        ));
    }
    if text.chars().any(char::is_control) {
        return Err(PolicyError::new(
            "control characters are not permitted",
            key_path,
            None,
        ));
    }
    Ok(())
}

fn find_unknown_key_path(top: &toml::Table) -> Option<String> {
    const TOP_KEYS: &[&str] = &[
        "policy_schema",
        "revision",
        "example",
        "label",
        "marking",
        "source",
        "store",
        "destination",
        "audit",
    ];
    for k in top.keys() {
        if !TOP_KEYS.contains(&k.as_str()) {
            return Some(k.clone());
        }
    }
    if let Some(arr) = top.get("label").and_then(toml::Value::as_array) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(tbl) = item.as_table() {
                for k in tbl.keys() {
                    if !matches!(k.as_str(), "id" | "title" | "banner" | "restricted") {
                        return Some(format!("label[{i}].{k}"));
                    }
                }
            }
        }
    }
    if let Some(arr) = top.get("marking").and_then(toml::Value::as_array) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(tbl) = item.as_table() {
                for k in tbl.keys() {
                    if !matches!(k.as_str(), "match" | "labels") {
                        return Some(format!("marking[{i}].{k}"));
                    }
                }
            }
        }
    }
    if let Some(arr) = top.get("source").and_then(toml::Value::as_array) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(tbl) = item.as_table() {
                for k in tbl.keys() {
                    if !matches!(k.as_str(), "id" | "root" | "default" | "rule" | "map") {
                        return Some(format!("source[{i}].{k}"));
                    }
                }
            }
        }
    }
    if let Some(arr) = top.get("store").and_then(toml::Value::as_array) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(tbl) = item.as_table() {
                for k in tbl.keys() {
                    if !matches!(
                        k.as_str(),
                        "id" | "holds" | "default" | "readers" | "changers" | "hold"
                    ) {
                        return Some(format!("store[{i}].{k}"));
                    }
                }
            }
        }
    }
    if let Some(arr) = top.get("destination").and_then(toml::Value::as_array) {
        for (i, item) in arr.iter().enumerate() {
            if let Some(tbl) = item.as_table() {
                for k in tbl.keys() {
                    if !matches!(
                        k.as_str(),
                        "kind"
                            | "allow"
                            | "name"
                            | "url"
                            | "expires"
                            | "keep_calls_days_max"
                            | "ref"
                            | "clients"
                            | "paths"
                    ) {
                        return Some(format!("destination[{i}].{k}"));
                    }
                }
            }
        }
    }
    if let Some(tbl) = top.get("audit").and_then(toml::Value::as_table) {
        for k in tbl.keys() {
            if !matches!(
                k.as_str(),
                "sink" | "max_unshipped_hours" | "require_fresh_export"
            ) {
                return Some(format!("audit.{k}"));
            }
        }
    }
    None
}
