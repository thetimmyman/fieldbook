//! Serde schema structs for `policy.toml` (`policy_schema = 1`, `ADR 0004 §3`,
//! `docs/reference/policy.md#schema-version-1`).

use serde::Deserialize;

/// Raw top-level `policy.toml` table (`policy_schema = 1`).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPolicy {
    /// Schema version (must be `1`).
    pub policy_schema: Option<toml::Value>,
    /// Company revision counter (must be `>= 1` and `>= recorded_revision`).
    pub revision: Option<toml::Value>,
    /// Marks the shipped example file (`example = true` is always refused).
    pub example: Option<toml::Value>,
    /// Company-defined opaque labels (`[[label]]`).
    #[serde(default)]
    pub label: Vec<RawLabel>,
    /// Global text marking rules (`[[marking]]`).
    #[serde(default)]
    pub marking: Vec<RawMarking>,
    /// Company data sources (`[[source]]`).
    #[serde(default)]
    pub source: Vec<RawSource>,
    /// Per-store holding and access rules (`[[store]]`).
    #[serde(default)]
    pub store: Vec<RawStore>,
    /// Outbound destination rules (`[[destination]]`).
    #[serde(default)]
    pub destination: Vec<RawDestination>,
    /// Audit export sink and freshness policy (`[audit]`).
    pub audit: Option<RawAudit>,
}

/// One `[[label]]` row in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLabel {
    /// Opaque company label code (`[a-z]{2,4}-[0-9]{4}`).
    pub id: Option<toml::Value>,
    /// Human-readable title (1..=80 code points, no control characters; never journaled).
    pub title: Option<toml::Value>,
    /// Optional banner printed on pages, exports, and prints (1..=80 code points, no control characters).
    pub banner: Option<toml::Value>,
    /// Whether holding this label requires a store `readers` group.
    pub restricted: Option<toml::Value>,
}

/// One `[[marking]]` row in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawMarking {
    /// Literal text to match (3..=80 characters, case-insensitive, whitespace-collapsed).
    #[serde(rename = "match")]
    pub matched: Option<toml::Value>,
    /// Labels assigned when `match` is found (non-empty, must not normalize to `public`-only).
    pub labels: Option<toml::Value>,
}

/// One `[[source]]` row in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSource {
    /// System identifier in `systems.toml`.
    pub id: Option<toml::Value>,
    /// Canonical absolute path prefix or `"drop"`.
    pub root: Option<toml::Value>,
    /// Default labels applied to all items from this source.
    pub default: Option<toml::Value>,
    /// Prefix rules (`[[source.rule]]`).
    #[serde(default)]
    pub rule: Vec<RawSourceRule>,
    /// Native classification map rows (`[[source.map]]`).
    #[serde(default)]
    pub map: Vec<RawSourceMap>,
}

/// One `[[source.rule]]` row in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSourceRule {
    /// Key prefix with `/` separator.
    pub prefix: Option<toml::Value>,
    /// Labels added when `prefix` matches.
    pub labels: Option<toml::Value>,
}

/// One `[[source.map]]` row in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSourceMap {
    /// Exact trimmed native classification text.
    pub native: Option<toml::Value>,
    /// Labels added when `native` matches.
    pub labels: Option<toml::Value>,
}

/// One `[[store]]` row in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawStore {
    /// Store identifier (`overlay`, `working`, or `state`; `corpus` is invalid).
    pub id: Option<toml::Value>,
    /// Labels the store may hold (`LabelAllow`; may be empty).
    pub holds: Option<toml::Value>,
    /// Optional default labels for notes written in the store (subset of `holds`, never `public`-only).
    pub default: Option<toml::Value>,
    /// Optional Windows group required to open the store.
    pub readers: Option<toml::Value>,
    /// Optional Windows group required to run `label add` on the store.
    pub changers: Option<toml::Value>,
    /// Whether purge and label changes are refused (`default = false`).
    pub hold: Option<toml::Value>,
}

/// One `[[destination]]` row in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawDestination {
    /// Destination kind (`model`, `handoff`, `mcp`, `bridge`, `export`, `backup`, `print`, `clipboard`).
    pub kind: Option<toml::Value>,
    /// Allowed labels (`LabelAllow`; may be empty).
    pub allow: Option<toml::Value>,
    /// Endpoint name (`model` required, `handoff` optional).
    pub name: Option<toml::Value>,
    /// Endpoint base URL (`model` required; `https://` or loopback `http://`).
    pub url: Option<toml::Value>,
    /// Unquoted TOML local date through which the row applies.
    pub expires: Option<toml::Value>,
    /// Maximum days call records may be kept (`model` only, `>= 0`).
    pub keep_calls_days_max: Option<toml::Value>,
    /// Company change/approval reference (1..=80 code points, no control characters; never journaled).
    #[serde(rename = "ref")]
    pub reference: Option<toml::Value>,
    /// Lowercase hex SHA-256 hashes of approved client names (`mcp` only).
    pub clients: Option<toml::Value>,
    /// Canonical path prefixes (`export` and `backup` only).
    pub paths: Option<toml::Value>,
}

/// The `[audit]` table in `policy.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawAudit {
    /// Absolute path to the folder `audit export` writes to.
    pub sink: Option<toml::Value>,
    /// Maximum age in hours of the last `audit.export` for non-public model calls.
    pub max_unshipped_hours: Option<toml::Value>,
    /// Whether a non-public model call requires a fresh export (`default = true`).
    pub require_fresh_export: Option<toml::Value>,
}
