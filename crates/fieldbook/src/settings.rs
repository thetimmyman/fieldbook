//! Pure fail-closed parser for the AI tables of `settings.toml` (`ai-config-and-keys` steps 3–5).
//!
//! Reads `[model]`, `[model.handoff]`, `[model.endpoint.<name>]`, `[search]`, `[mcp]`, and
//! `[chat]` into typed values, leaving keys owned by other cards (`role`, `[asked]`, `[backup]`)
//! untouched. Any violation returns `FB0900` with a 1-based line number, the offending key, and
//! an actionable help line. Never reads environment variables or stores a secret value.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use fieldbook_core::audit::{EndpointName, ModelId};
use fieldbook_core::check::Code;
use fieldbook_core::labels::{LabelAllow, LabelId, MARKED, UNLABELLED};
use fieldbook_core::policy::OwnerSettings;

use crate::adapters::registry::{Port, Row, State};
use crate::io::keys::is_valid_key_env_name;

/// Default `[model] retries`.
pub const DEFAULT_RETRIES: u32 = 2;
/// Maximum allowed `[model] retries`.
pub const MAX_RETRIES: u32 = 10;
/// Default `[model] max_calls_per_run`.
pub const DEFAULT_MAX_CALLS_PER_RUN: u32 = 100;
/// Default `[model] max_out_tokens`.
pub const DEFAULT_MAX_OUT_TOKENS: u32 = 1500;
/// Default `[model] keep_calls`.
pub const DEFAULT_KEEP_CALLS: bool = true;
/// Default `[model] keep_days`.
pub const DEFAULT_KEEP_DAYS: u32 = 30;
/// Default `[model.endpoint.<name>] timeout_s`.
pub const DEFAULT_TIMEOUT_S: u32 = 60;
/// Maximum allowed `[model.endpoint.<name>] timeout_s`.
pub const MAX_TIMEOUT_S: u32 = 600;
/// Default `[chat] idle_minutes`.
pub const DEFAULT_IDLE_MINUTES: u32 = 30;
/// Maximum allowed `[chat] idle_minutes`.
pub const MAX_IDLE_MINUTES: u32 = 1440;

/// Help text for any secret-looking key name (`NS-07`, `FB0900`).
pub const HELP_SECRET_KEY: &str =
    "put the key in an environment variable and name it with `key_env`";

/// Help text for policy-replaced keys (`approved_for`, `default`, `fallback`, `[model.route]`, `company_approved`).
pub const HELP_POLICY_APPROVES: &str =
    "the company's policy approves endpoints; narrow with allow and list routes in order";

/// Top-level keys owned by other readers (`role`, `[asked]`, `[backup]`, `[[backup.target]]`).
pub const OWNED_TOP_LEVEL: &[&str] = &["role", "asked", "backup"];

/// One documented row in [`KEY_TABLE`], rendered into `docs/reference/ports.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeySpec {
    /// Table header path (e.g. `"[model]"` or `"[model.endpoint.<name>]"`).
    pub table: &'static str,
    /// Key name inside `table`.
    pub key: &'static str,
    /// Type description.
    pub ty: &'static str,
    /// Default value description.
    pub default: &'static str,
    /// Adapter row id that reads this key, or `"-"` when read by core settings/routing.
    pub adapter: &'static str,
    /// Whether this key has a corresponding narrow-only merge rule in `fieldbook_core::policy::merge::KEY_TABLE`.
    pub meets_policy: bool,
}

/// Authoritative table of every AI settings key (`ai-config-and-keys` step 5).
pub const KEY_TABLE: &[KeySpec] = &[
    KeySpec {
        table: "[model]",
        key: "order",
        ty: "list of strings",
        default: "[\"handoff\"]",
        adapter: "-",
        meets_policy: true,
    },
    KeySpec {
        table: "[model]",
        key: "retries",
        ty: "integer (1..=10)",
        default: "2",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model]",
        key: "max_calls_per_run",
        ty: "integer (>= 1)",
        default: "100",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model]",
        key: "max_out_tokens",
        ty: "integer (>= 1)",
        default: "1500",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model]",
        key: "keep_calls",
        ty: "boolean",
        default: "true",
        adapter: "-",
        meets_policy: true,
    },
    KeySpec {
        table: "[model]",
        key: "keep_days",
        ty: "integer (>= 0)",
        default: "30",
        adapter: "-",
        meets_policy: true,
    },
    KeySpec {
        table: "[model]",
        key: "proxy",
        ty: "\"env\" or \"none\"",
        default: "\"env\"",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model.handoff]",
        key: "allow",
        ty: "list of label ids",
        default: "none (no narrowing)",
        adapter: "handoff@1",
        meets_policy: true,
    },
    KeySpec {
        table: "[model.handoff]",
        key: "assistant_label",
        ty: "string",
        default: "none",
        adapter: "handoff@1",
        meets_policy: false,
    },
    KeySpec {
        table: "[model.endpoint.<name>]",
        key: "adapter",
        ty: "string (Model adapter id)",
        default: "required",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model.endpoint.<name>]",
        key: "base_url",
        ty: "string (URL with host)",
        default: "required",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model.endpoint.<name>]",
        key: "model",
        ty: "string (ModelId)",
        default: "required",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model.endpoint.<name>]",
        key: "allow",
        ty: "list of label ids",
        default: "none (accepts nothing)",
        adapter: "-",
        meets_policy: true,
    },
    KeySpec {
        table: "[model.endpoint.<name>]",
        key: "timeout_s",
        ty: "integer (1..=600)",
        default: "60",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model.endpoint.<name>]",
        key: "key_env",
        ty: "string (env var name)",
        default: "none",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[model.endpoint.<name>]",
        key: "auth",
        ty: "\"bearer\" or \"api-key\"",
        default: "\"bearer\"",
        adapter: "-",
        meets_policy: false,
    },
    KeySpec {
        table: "[search]",
        key: "semantic",
        ty: "boolean",
        default: "false",
        adapter: "semantic@1",
        meets_policy: false,
    },
    KeySpec {
        table: "[mcp]",
        key: "allow",
        ty: "list of label ids",
        default: "none (no narrowing)",
        adapter: "-",
        meets_policy: true,
    },
    KeySpec {
        table: "[chat]",
        key: "idle_minutes",
        ty: "integer (1..=1440)",
        default: "30",
        adapter: "-",
        meets_policy: false,
    },
];

/// Returns all known settings key names from [`KEY_TABLE`] for registry drift checks.
#[must_use]
pub fn known_key_names() -> Vec<&'static str> {
    KEY_TABLE.iter().map(|spec| spec.key).collect()
}

/// HTTP proxy mode (`[model] proxy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyMode {
    /// Respect standard environment proxy variables (`"env"`).
    #[default]
    Env,
    /// Bypass proxy environment variables (`"none"`).
    None,
}

impl ProxyMode {
    /// Returns the canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::None => "none",
        }
    }
}

/// Endpoint authentication header scheme (`[model.endpoint.<name>] auth`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AuthKind {
    /// `Authorization: Bearer <key>` (`"bearer"`).
    #[default]
    Bearer,
    /// API-key header (`"api-key"`).
    ApiKey,
}

impl AuthKind {
    /// Returns the canonical wire string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bearer => "bearer",
            Self::ApiKey => "api-key",
        }
    }
}

/// Parsed `[model.handoff]` table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HandoffSettings {
    /// Optional owner narrowing allow list (`None` means no narrowing).
    pub allow: Option<LabelAllow>,
    /// Optional human display label for the external browser assistant.
    pub assistant_label: Option<String>,
}

/// Parsed `[model.endpoint.<name>]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EndpointSettings {
    /// Resolved built Model adapter identifier (`<name>@<n>`).
    pub adapter: String,
    /// Endpoint base URL (with host, no userinfo).
    pub base_url: String,
    /// Model identifier sent to the endpoint.
    pub model: ModelId,
    /// Optional owner allow list (`None` means the endpoint accepts nothing).
    pub allow: Option<LabelAllow>,
    /// Request timeout in seconds (`1..=600`, default `60`).
    pub timeout_s: u32,
    /// Name of the environment variable holding the API key (never the key value).
    pub key_env: Option<String>,
    /// Authentication header scheme (`bearer` or `api-key`).
    pub auth: AuthKind,
}

/// Parsed `[model]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSettings {
    /// Route preference order (`handoff` and configured endpoint names).
    pub order: Vec<String>,
    /// Retry limit (`1..=10`, default `2`).
    pub retries: u32,
    /// Maximum model calls per run (`>= 1`, default `100`).
    pub max_calls_per_run: u32,
    /// Maximum output tokens per call (`>= 1`, default `1500`).
    pub max_out_tokens: u32,
    /// Whether the owner enables call-record retention (narrowed by policy).
    pub keep_calls: bool,
    /// Call-record retention days (narrowed by policy).
    pub keep_days: u32,
    /// Proxy selection (`env` or `none`).
    pub proxy: ProxyMode,
    /// `[model.handoff]` settings.
    pub handoff: HandoffSettings,
    /// Configured `[model.endpoint.<name>]` tables, keyed by validated [`EndpointName`].
    pub endpoints: BTreeMap<EndpointName, EndpointSettings>,
}

impl Default for ModelSettings {
    fn default() -> Self {
        Self {
            order: vec!["handoff".to_owned()],
            retries: DEFAULT_RETRIES,
            max_calls_per_run: DEFAULT_MAX_CALLS_PER_RUN,
            max_out_tokens: DEFAULT_MAX_OUT_TOKENS,
            keep_calls: DEFAULT_KEEP_CALLS,
            keep_days: DEFAULT_KEEP_DAYS,
            proxy: ProxyMode::Env,
            handoff: HandoffSettings::default(),
            endpoints: BTreeMap::new(),
        }
    }
}

/// Parsed `[search]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchSettings {
    /// Whether semantic retrieval (`semantic@1`) is enabled (`false` by default).
    pub semantic: bool,
}

/// Parsed `[mcp]` table.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct McpSettings {
    /// Optional owner narrowing allow list (`None` means no narrowing).
    pub allow: Option<LabelAllow>,
}

/// Parsed `[chat]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatSettings {
    /// Idle timeout in minutes (`1..=1440`, default `30`).
    pub idle_minutes: u32,
}

impl Default for ChatSettings {
    fn default() -> Self {
        Self {
            idle_minutes: DEFAULT_IDLE_MINUTES,
        }
    }
}

/// All AI settings parsed from `settings.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    /// `[model]`, `[model.handoff]`, and `[model.endpoint.<name>]`.
    pub model: ModelSettings,
    /// `[search]`.
    pub search: SearchSettings,
    /// `[mcp]`.
    pub mcp: McpSettings,
    /// `[chat]`.
    pub chat: ChatSettings,
}

/// One `FB0900` configuration error produced by [`Settings::parse`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsError {
    /// Always [`Code::Fb0900`].
    pub code: Code,
    /// 1-based line number in `settings.toml` (always `>= 1`).
    pub line: usize,
    /// Offending key or table path.
    pub key: String,
    /// Short explanation of why the key or value was refused.
    pub message: String,
    /// Actionable remediation guidance.
    pub help: &'static str,
}

impl SettingsError {
    fn new(
        line: usize,
        key: impl Into<String>,
        message: impl Into<String>,
        help: &'static str,
    ) -> Self {
        Self {
            code: Code::Fb0900,
            line: line.max(1),
            key: key.into(),
            message: message.into(),
            help,
        }
    }
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error[{}]: line {}: `{}`: {}\n  help: {}",
            self.code.id(),
            self.line,
            self.key,
            self.message,
            self.help
        )
    }
}

impl std::error::Error for SettingsError {}

impl Settings {
    /// Converts these parsed settings into the [`OwnerSettings`] merged by `core-policy`.
    ///
    /// An endpoint with no `allow` key has no entry in `OwnerSettings.endpoints` and accepts
    /// nothing. Absent `[model.handoff] allow` and `[mcp] allow` map to `None` (no narrowing).
    #[must_use]
    pub fn owner(&self) -> OwnerSettings {
        let mut endpoints = BTreeMap::new();
        for (name, ep) in &self.model.endpoints {
            if let Some(allow) = &ep.allow {
                endpoints.insert(name.as_str().to_owned(), allow.clone());
            }
        }
        OwnerSettings {
            endpoints,
            handoff: self.model.handoff.allow.clone(),
            mcp: self.mcp.allow.clone(),
            keep_calls: Some(self.model.keep_calls),
            keep_days: Some(self.model.keep_days),
            order: self.model.order.clone(),
        }
    }

    /// Parses the AI tables of `settings.toml` purely against `rows`.
    ///
    /// # Errors
    ///
    /// Returns every `FB0900` [`SettingsError`] found, each with a 1-based line number.
    pub fn parse(text: &str, rows: &[Row]) -> Result<Self, Vec<SettingsError>> {
        let lines = LineMap::scan(text);

        let table: toml::Table = match toml::from_str(text) {
            Ok(t) => t,
            Err(err) => {
                let line = err
                    .span()
                    .map_or(1, |span| byte_offset_to_line(text, span.start));
                return Err(vec![SettingsError::new(
                    line,
                    "settings.toml",
                    format!("invalid TOML syntax: {}", err.message()),
                    "fix the TOML syntax in settings.toml",
                )]);
            }
        };

        let mut errors = Vec::new();
        let mut settings = Self::default();

        for (key, value) in &table {
            if OWNED_TOP_LEVEL.contains(&key.as_str()) {
                continue;
            }
            match key.as_str() {
                "model" => {
                    if let Some(model_tbl) = value.as_table() {
                        parse_model_table(
                            model_tbl,
                            rows,
                            &lines,
                            &mut settings.model,
                            &mut errors,
                        );
                    } else {
                        errors.push(SettingsError::new(
                            lines.line_for("", "model"),
                            "model",
                            "`model` must be a table",
                            "declare `[model]` as a TOML table",
                        ));
                    }
                }
                "search" => {
                    if let Some(search_tbl) = value.as_table() {
                        parse_search_table(
                            search_tbl,
                            rows,
                            &lines,
                            &mut settings.search,
                            &mut errors,
                        );
                    } else {
                        errors.push(SettingsError::new(
                            lines.line_for("", "search"),
                            "search",
                            "`search` must be a table",
                            "declare `[search]` as a TOML table",
                        ));
                    }
                }
                "mcp" => {
                    if let Some(mcp_tbl) = value.as_table() {
                        parse_mcp_table(mcp_tbl, &lines, &mut settings.mcp, &mut errors);
                    } else {
                        errors.push(SettingsError::new(
                            lines.line_for("", "mcp"),
                            "mcp",
                            "`mcp` must be a table",
                            "declare `[mcp]` as a TOML table",
                        ));
                    }
                }
                "chat" => {
                    if let Some(chat_tbl) = value.as_table() {
                        parse_chat_table(chat_tbl, &lines, &mut settings.chat, &mut errors);
                    } else {
                        errors.push(SettingsError::new(
                            lines.line_for("", "chat"),
                            "chat",
                            "`chat` must be a table",
                            "declare `[chat]` as a TOML table",
                        ));
                    }
                }
                other => {
                    let line = lines.line_for_top(other);
                    push_rejected_key(other, other, value.is_table(), line, &mut errors);
                }
            }
        }

        if errors.is_empty() {
            Ok(settings)
        } else {
            Err(errors)
        }
    }
}

fn is_secret_key_name(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "key" | "api_key" | "token" | "secret" | "password"
    )
}

fn is_policy_replaced_key(key: &str) -> bool {
    matches!(key, "approved_for" | "default" | "fallback")
}

fn push_rejected_key(
    key_name: &str,
    full_path: &str,
    is_table: bool,
    line: usize,
    errors: &mut Vec<SettingsError>,
) {
    if is_secret_key_name(key_name) {
        errors.push(SettingsError::new(
            line,
            full_path,
            "secret-looking key name is not allowed in settings.toml",
            HELP_SECRET_KEY,
        ));
    } else if is_policy_replaced_key(key_name) {
        errors.push(SettingsError::new(
            line,
            full_path,
            "this key was replaced by the company policy and `[model] order`",
            HELP_POLICY_APPROVES,
        ));
    } else if key_name == "company_approved" {
        errors.push(SettingsError::new(
            line,
            full_path,
            "unknown key `company_approved`; settings.toml cannot approve endpoints",
            HELP_POLICY_APPROVES,
        ));
    } else if is_table {
        errors.push(SettingsError::new(
            line,
            full_path,
            "unknown table in settings.toml",
            "remove the unknown table or check docs/reference/ports.md for allowed keys",
        ));
    } else {
        errors.push(SettingsError::new(
            line,
            full_path,
            "unknown key in settings.toml",
            "remove the unknown key or check docs/reference/ports.md for allowed keys",
        ));
    }
}

fn parse_model_table(
    tbl: &toml::Table,
    rows: &[Row],
    lines: &LineMap,
    out: &mut ModelSettings,
    errors: &mut Vec<SettingsError>,
) {
    let mut raw_order: Option<(usize, &toml::Value)> = None;
    let mut declared_endpoints: BTreeSet<String> = BTreeSet::new();

    for (key, value) in tbl {
        let full = format!("model.{key}");
        let line = lines.line_for("model", key);
        match key.as_str() {
            "order" => {
                raw_order = Some((line, value));
            }
            "retries" => {
                if let Some(v) = parse_bounded_u32(
                    value,
                    &full,
                    line,
                    1,
                    MAX_RETRIES,
                    "retries must be an integer from 1 to 10",
                    errors,
                ) {
                    out.retries = v;
                }
            }
            "max_calls_per_run" => {
                if let Some(v) = parse_bounded_u32(
                    value,
                    &full,
                    line,
                    1,
                    u32::MAX,
                    "max_calls_per_run must be a positive integer",
                    errors,
                ) {
                    out.max_calls_per_run = v;
                }
            }
            "max_out_tokens" => {
                if let Some(v) = parse_bounded_u32(
                    value,
                    &full,
                    line,
                    1,
                    u32::MAX,
                    "max_out_tokens must be a positive integer",
                    errors,
                ) {
                    out.max_out_tokens = v;
                }
            }
            "keep_calls" => match value.as_bool() {
                Some(b) => out.keep_calls = b,
                None => errors.push(SettingsError::new(
                    line,
                    full,
                    "expected a boolean for `keep_calls`",
                    "set `keep_calls = true` or `keep_calls = false`",
                )),
            },
            "keep_days" => {
                if let Some(v) = parse_bounded_u32(
                    value,
                    &full,
                    line,
                    0,
                    u32::MAX,
                    "keep_days must be a non-negative integer",
                    errors,
                ) {
                    out.keep_days = v;
                }
            }
            "proxy" => match value.as_str() {
                Some("env") => out.proxy = ProxyMode::Env,
                Some("none") => out.proxy = ProxyMode::None,
                _ => errors.push(SettingsError::new(
                    line,
                    full,
                    "invalid `proxy` mode",
                    "set `proxy` to \"env\" or \"none\"",
                )),
            },
            "handoff" => {
                if let Some(handoff_tbl) = value.as_table() {
                    parse_handoff_table(handoff_tbl, lines, &mut out.handoff, errors);
                } else {
                    errors.push(SettingsError::new(
                        line,
                        full,
                        "`model.handoff` must be a table",
                        "declare `[model.handoff]` as a TOML table",
                    ));
                }
            }
            "endpoint" => {
                if let Some(endpoints_tbl) = value.as_table() {
                    for (ep_name, ep_val) in endpoints_tbl {
                        declared_endpoints.insert(ep_name.clone());
                        let table_path = format!("model.endpoint.{ep_name}");
                        let ep_line = lines.line_for_table(&table_path).unwrap_or(line);
                        if !is_valid_endpoint_name(ep_name) {
                            let msg = if ep_name == "handoff" {
                                "endpoint name `handoff` is reserved"
                            } else {
                                "endpoint name must match `[a-z][a-z0-9-]{0,31}`"
                            };
                            errors.push(SettingsError::new(
                                ep_line,
                                &table_path,
                                msg,
                                "choose a lowercase endpoint name starting with a letter (not `handoff`)",
                            ));
                            continue;
                        }
                        let Some(ep_tbl) = ep_val.as_table() else {
                            errors.push(SettingsError::new(
                                ep_line,
                                &table_path,
                                "endpoint entry must be a table",
                                "declare `[model.endpoint.<name>]` as a TOML table",
                            ));
                            continue;
                        };
                        if let Some(ep) =
                            parse_endpoint_table(&table_path, ep_line, ep_tbl, rows, lines, errors)
                            && let Ok(valid_name) = EndpointName::parse(ep_name)
                        {
                            out.endpoints.insert(valid_name, ep);
                        }
                    }
                } else {
                    errors.push(SettingsError::new(
                        line,
                        full,
                        "`model.endpoint` must be a table",
                        "declare endpoints as `[model.endpoint.<name>]` tables",
                    ));
                }
            }
            "route" => {
                let route_line = lines.line_for_table("model.route").unwrap_or(line);
                errors.push(SettingsError::new(
                    route_line,
                    "model.route",
                    "`[model.route]` was replaced by the company policy and `[model] order`",
                    HELP_POLICY_APPROVES,
                ));
            }
            other => {
                push_rejected_key(other, &full, value.is_table(), line, errors);
            }
        }
    }

    if let Some((order_line, order_val)) = raw_order {
        let Some(arr) = order_val.as_array() else {
            errors.push(SettingsError::new(
                order_line,
                "model.order",
                "expected an array of route names for `order`",
                "set `order = [\"handoff\"]` or list configured endpoint names",
            ));
            return;
        };
        let mut parsed_order = Vec::with_capacity(arr.len());
        let mut seen = BTreeSet::new();
        for item in arr {
            let Some(name) = item.as_str() else {
                errors.push(SettingsError::new(
                    order_line,
                    "model.order",
                    "every `order` entry must be a string",
                    "list configured endpoint names and/or \"handoff\"",
                ));
                continue;
            };
            if !seen.insert(name.to_owned()) {
                errors.push(SettingsError::new(
                    order_line,
                    "model.order",
                    format!("duplicate route `{name}` in `order`"),
                    "list each route at most once in `[model] order`",
                ));
                continue;
            }
            if name != "handoff" && !declared_endpoints.contains(name) {
                errors.push(SettingsError::new(
                    order_line,
                    "model.order",
                    format!("route `{name}` in `order` names no configured endpoint"),
                    "configure `[model.endpoint.<name>]` or remove the entry from `order`",
                ));
                continue;
            }
            parsed_order.push(name.to_owned());
        }
        out.order = parsed_order;
    }
}

fn parse_handoff_table(
    tbl: &toml::Table,
    lines: &LineMap,
    out: &mut HandoffSettings,
    errors: &mut Vec<SettingsError>,
) {
    for (key, value) in tbl {
        let full = format!("model.handoff.{key}");
        let line = lines.line_for("model.handoff", key);
        match key.as_str() {
            "allow" => {
                if let Some(allow) = parse_allow_list(value, &full, line, errors) {
                    out.allow = Some(allow);
                }
            }
            "assistant_label" => match value.as_str() {
                Some(s) if !s.is_empty() && !s.chars().any(char::is_control) => {
                    out.assistant_label = Some(s.to_owned());
                }
                _ => errors.push(SettingsError::new(
                    line,
                    full,
                    "invalid `assistant_label`",
                    "set `assistant_label` to a non-empty string without control characters",
                )),
            },
            other => {
                push_rejected_key(other, &full, value.is_table(), line, errors);
            }
        }
    }
}

fn parse_endpoint_table(
    table_path: &str,
    table_line: usize,
    tbl: &toml::Table,
    rows: &[Row],
    lines: &LineMap,
    errors: &mut Vec<SettingsError>,
) -> Option<EndpointSettings> {
    let mut adapter: Option<String> = None;
    let mut base_url: Option<String> = None;
    let mut model: Option<ModelId> = None;
    let mut allow: Option<LabelAllow> = None;
    let mut timeout_s: u32 = DEFAULT_TIMEOUT_S;
    let mut key_env: Option<String> = None;
    let mut auth = AuthKind::Bearer;
    let mut had_error = false;

    for (key, value) in tbl {
        let full = format!("{table_path}.{key}");
        let line = lines.line_for(table_path, key);
        match key.as_str() {
            "adapter" => match value.as_str() {
                Some(raw) => match resolve_built_model_adapter(raw, rows) {
                    Some(resolved) => adapter = Some(resolved.to_owned()),
                    None => {
                        had_error = true;
                        errors.push(SettingsError::new(
                            line,
                            full,
                            format!("model adapter `{raw}` is not a built Model adapter"),
                            "use a built Model adapter from `docs/reference/ports.md`",
                        ));
                    }
                },
                None => {
                    had_error = true;
                    errors.push(SettingsError::new(
                        line,
                        full,
                        "expected a string for `adapter`",
                        "set `adapter` to a built Model adapter id",
                    ));
                }
            },
            "base_url" => match value.as_str() {
                Some(url) => match validate_base_url(url) {
                    Ok(()) => base_url = Some(url.to_owned()),
                    Err((msg, help)) => {
                        had_error = true;
                        errors.push(SettingsError::new(line, full, msg, help));
                    }
                },
                None => {
                    had_error = true;
                    errors.push(SettingsError::new(
                        line,
                        full,
                        "expected a string URL for `base_url`",
                        "set `base_url` to a URL with a host and no user information",
                    ));
                }
            },
            "model" => match value.as_str().and_then(|s| ModelId::parse(s).ok()) {
                Some(id) => model = Some(id),
                None => {
                    had_error = true;
                    errors.push(SettingsError::new(
                        line,
                        full,
                        "invalid model identifier",
                        "set `model` to a valid model id (`[A-Za-z0-9._:@/-]{1,64}`)",
                    ));
                }
            },
            "allow" => {
                if let Some(parsed) = parse_allow_list(value, &full, line, errors) {
                    allow = Some(parsed);
                } else {
                    had_error = true;
                }
            }
            "timeout_s" => {
                if let Some(v) = parse_bounded_u32(
                    value,
                    &full,
                    line,
                    1,
                    MAX_TIMEOUT_S,
                    "timeout_s must be an integer from 1 to 600",
                    errors,
                ) {
                    timeout_s = v;
                } else {
                    had_error = true;
                }
            }
            "key_env" => match value.as_str() {
                Some(var_name) if is_valid_key_env_name(var_name) => {
                    key_env = Some(var_name.to_owned());
                }
                _ => {
                    had_error = true;
                    errors.push(SettingsError::new(
                        line,
                        full,
                        "invalid environment variable name for `key_env`",
                        "name `key_env` matching `[A-Z][A-Z0-9_]{2,63}`",
                    ));
                }
            },
            "auth" => match value.as_str() {
                Some("bearer") => auth = AuthKind::Bearer,
                Some("api-key") => auth = AuthKind::ApiKey,
                _ => {
                    had_error = true;
                    errors.push(SettingsError::new(
                        line,
                        full,
                        "invalid `auth` value",
                        "set `auth` to \"bearer\" or \"api-key\"",
                    ));
                }
            },
            other => {
                had_error = true;
                push_rejected_key(other, &full, value.is_table(), line, errors);
            }
        }
    }

    if adapter.is_none() && !tbl.contains_key("adapter") {
        had_error = true;
        errors.push(SettingsError::new(
            table_line,
            format!("{table_path}.adapter"),
            "missing required key `adapter`",
            "set `adapter` to a built Model adapter id",
        ));
    }
    if base_url.is_none() && !tbl.contains_key("base_url") {
        had_error = true;
        errors.push(SettingsError::new(
            table_line,
            format!("{table_path}.base_url"),
            "missing required key `base_url`",
            "set `base_url` to the endpoint URL",
        ));
    }
    if model.is_none() && !tbl.contains_key("model") {
        had_error = true;
        errors.push(SettingsError::new(
            table_line,
            format!("{table_path}.model"),
            "missing required key `model`",
            "set `model` to the model identifier",
        ));
    }

    if had_error {
        return None;
    }

    Some(EndpointSettings {
        adapter: adapter?,
        base_url: base_url?,
        model: model?,
        allow,
        timeout_s,
        key_env,
        auth,
    })
}

fn parse_search_table(
    tbl: &toml::Table,
    rows: &[Row],
    lines: &LineMap,
    out: &mut SearchSettings,
    errors: &mut Vec<SettingsError>,
) {
    for (key, value) in tbl {
        let full = format!("search.{key}");
        let line = lines.line_for("search", key);
        match key.as_str() {
            "semantic" => match value.as_bool() {
                Some(false) => out.semantic = false,
                Some(true) => {
                    let has_semantic = rows.iter().any(|r| {
                        r.port == Port::Index && r.id == "semantic@1" && r.state == State::Built
                    });
                    if has_semantic {
                        out.semantic = true;
                    } else {
                        errors.push(SettingsError::new(
                            line,
                            full,
                            "`search.semantic = true` requires the built `semantic@1` Index adapter",
                            "set `semantic = false` until `semantic@1` is built",
                        ));
                    }
                }
                None => errors.push(SettingsError::new(
                    line,
                    full,
                    "expected a boolean for `semantic`",
                    "set `semantic = true` or `semantic = false`",
                )),
            },
            other => {
                push_rejected_key(other, &full, value.is_table(), line, errors);
            }
        }
    }
}

fn parse_mcp_table(
    tbl: &toml::Table,
    lines: &LineMap,
    out: &mut McpSettings,
    errors: &mut Vec<SettingsError>,
) {
    for (key, value) in tbl {
        let full = format!("mcp.{key}");
        let line = lines.line_for("mcp", key);
        match key.as_str() {
            "allow" => {
                if let Some(allow) = parse_allow_list(value, &full, line, errors) {
                    out.allow = Some(allow);
                }
            }
            other => {
                push_rejected_key(other, &full, value.is_table(), line, errors);
            }
        }
    }
}

fn parse_chat_table(
    tbl: &toml::Table,
    lines: &LineMap,
    out: &mut ChatSettings,
    errors: &mut Vec<SettingsError>,
) {
    for (key, value) in tbl {
        let full = format!("chat.{key}");
        let line = lines.line_for("chat", key);
        match key.as_str() {
            "idle_minutes" => {
                if let Some(v) = parse_bounded_u32(
                    value,
                    &full,
                    line,
                    1,
                    MAX_IDLE_MINUTES,
                    "idle_minutes must be an integer from 1 to 1440",
                    errors,
                ) {
                    out.idle_minutes = v;
                }
            }
            other => {
                push_rejected_key(other, &full, value.is_table(), line, errors);
            }
        }
    }
}

fn parse_bounded_u32(
    value: &toml::Value,
    full_key: &str,
    line: usize,
    min: u32,
    max: u32,
    help: &'static str,
    errors: &mut Vec<SettingsError>,
) -> Option<u32> {
    let Some(i) = value.as_integer() else {
        errors.push(SettingsError::new(
            line,
            full_key,
            "expected an integer value",
            help,
        ));
        return None;
    };
    let Ok(n) = u32::try_from(i) else {
        errors.push(SettingsError::new(
            line,
            full_key,
            format!("value {i} is outside the allowed range {min}..={max}"),
            help,
        ));
        return None;
    };
    if n < min || n > max {
        errors.push(SettingsError::new(
            line,
            full_key,
            format!("value {n} is outside the allowed range {min}..={max}"),
            help,
        ));
        return None;
    }
    Some(n)
}

fn parse_allow_list(
    value: &toml::Value,
    full_key: &str,
    line: usize,
    errors: &mut Vec<SettingsError>,
) -> Option<LabelAllow> {
    let Some(arr) = value.as_array() else {
        errors.push(SettingsError::new(
            line,
            full_key,
            "expected a list of label ids for `allow`",
            "list label ids such as `[\"public\"]` or opaque ids `[a-z]{2,4}-[0-9]{4}`",
        ));
        return None;
    };

    let mut ids = Vec::with_capacity(arr.len());
    let mut ok = true;
    for elem in arr {
        let Some(s) = elem.as_str() else {
            ok = false;
            errors.push(SettingsError::new(
                line,
                full_key,
                "every entry in `allow` must be a string label id",
                "use built-in label ids (`public`, `owner-supplied`) or opaque ids `[a-z]{2,4}-[0-9]{4}`",
            ));
            continue;
        };
        let Ok(id) = LabelId::parse(s) else {
            ok = false;
            errors.push(SettingsError::new(
                line,
                full_key,
                format!("`{s}` is not a valid label id"),
                "use built-in label ids (`public`, `owner-supplied`) or opaque ids `[a-z]{2,4}-[0-9]{4}`",
            ));
            continue;
        };
        if id == UNLABELLED || id == MARKED {
            ok = false;
            errors.push(SettingsError::new(
                line,
                full_key,
                format!("`{}` cannot appear in `allow`", id.as_str()),
                "nothing may allow `unlabelled` or `marked`",
            ));
            continue;
        }
        ids.push(id);
    }

    if ok {
        Some(LabelAllow::from_ids(ids))
    } else {
        None
    }
}

fn is_valid_endpoint_name(name: &str) -> bool {
    if name == "handoff" {
        return false;
    }
    let bytes = name.as_bytes();
    if bytes.is_empty() || bytes.len() > 32 || !bytes[0].is_ascii_lowercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Resolves a configured `adapter` string (`name@n` or unversioned `name`) against built `Model` rows.
#[must_use]
pub fn resolve_built_model_adapter(spec: &str, rows: &[Row]) -> Option<&'static str> {
    if spec.contains('@') {
        rows.iter()
            .find(|r| r.port == Port::Model && r.state == State::Built && r.id == spec)
            .map(|r| r.id)
    } else {
        let mut best: Option<(u32, &'static str)> = None;
        for row in rows {
            if row.port != Port::Model || row.state != State::Built {
                continue;
            }
            if let Some((base, ver_str)) = row.id.split_once('@')
                && base == spec
                && let Ok(ver) = ver_str.parse::<u32>()
                && best.is_none_or(|(best_v, _)| ver > best_v)
            {
                best = Some((ver, row.id));
            }
        }
        best.map(|(_, id)| id)
    }
}

fn validate_base_url(url: &str) -> Result<(), (&'static str, &'static str)> {
    if url.is_empty() || url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err((
            "base_url must be a valid URL with a host",
            "provide a URL like `https://models.example.test/v1` with no whitespace",
        ));
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err((
            "base_url must include a scheme and host",
            "provide a URL like `https://models.example.test/v1`",
        ));
    };
    let mut scheme_chars = scheme.chars();
    let Some(first) = scheme_chars.next() else {
        return Err((
            "base_url scheme must not be empty",
            "provide a URL like `https://models.example.test/v1`",
        ));
    };
    if !first.is_ascii_alphabetic()
        || !scheme_chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return Err((
            "base_url has an invalid URL scheme",
            "provide a URL like `https://models.example.test/v1`",
        ));
    }
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() {
        return Err((
            "base_url must include a host",
            "provide a URL like `https://models.example.test/v1`",
        ));
    }
    if authority.contains('@') {
        return Err((
            "base_url must not contain user information",
            HELP_SECRET_KEY,
        ));
    }
    let host = if let Some(bracketed) = authority.strip_prefix('[') {
        let Some((ipv6, after)) = bracketed.split_once(']') else {
            return Err((
                "base_url has an unclosed IPv6 host bracket",
                "provide a valid host in `base_url`",
            ));
        };
        if !after.is_empty() {
            let Some(port_str) = after.strip_prefix(':') else {
                return Err((
                    "base_url has an invalid host/port",
                    "provide a valid host and optional `:port` in `base_url`",
                ));
            };
            if port_str.is_empty() || port_str.parse::<u16>().is_err() {
                return Err((
                    "base_url has an invalid port",
                    "provide a port from 1 to 65535",
                ));
            }
        }
        ipv6
    } else if let Some((h, port_str)) = authority.rsplit_once(':') {
        if port_str.is_empty() || port_str.parse::<u16>().is_err() {
            return Err((
                "base_url has an invalid port",
                "provide a port from 1 to 65535",
            ));
        }
        h
    } else {
        authority
    };

    if host.is_empty() {
        return Err((
            "base_url must include a host",
            "provide a URL like `https://models.example.test/v1`",
        ));
    }
    Ok(())
}

fn byte_offset_to_line(text: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(text.len());
    text.as_bytes()
        .iter()
        .take(clamped)
        .filter(|&&b| b == b'\n')
        .count()
        + 1
}

/// Pure, panic-free 1-based line scanner for TOML table headers and keys.
struct LineMap {
    tables: BTreeMap<String, usize>,
    keys: BTreeMap<(String, String), usize>,
}

impl LineMap {
    fn scan(text: &str) -> Self {
        let mut tables = BTreeMap::new();
        let mut keys = BTreeMap::new();
        let mut current_table = String::new();

        for (idx, raw_line) in text.lines().enumerate() {
            let line_no = idx + 1;
            let trimmed = strip_toml_comment(raw_line).trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(after_open) = trimmed.strip_prefix('[') {
                let inner = if let Some(arr_inner) = after_open.strip_prefix('[') {
                    arr_inner.rsplit_once("]]").map(|(head, _)| head)
                } else {
                    after_open.rsplit_once(']').map(|(head, _)| head)
                };
                if let Some(header) = inner {
                    let segments = split_toml_key_path(header);
                    if !segments.is_empty() {
                        current_table = segments.join(".");
                        tables.entry(current_table.clone()).or_insert(line_no);
                        // Also record ancestor prefixes so e.g. `[model.endpoint.work]` records `model` and `model.endpoint`.
                        for prefix_len in 1..segments.len() {
                            let prefix = segments[..prefix_len].join(".");
                            tables.entry(prefix).or_insert(line_no);
                        }
                    }
                }
                continue;
            }

            if let Some((lhs, _)) = split_key_value_line(trimmed) {
                let segments = split_toml_key_path(lhs);
                if segments.is_empty() {
                    continue;
                }
                let mut full_segments: Vec<String> = if current_table.is_empty() {
                    Vec::new()
                } else {
                    current_table.split('.').map(str::to_owned).collect()
                };
                full_segments.extend(segments);
                if let Some((leaf, parent_slice)) = full_segments.split_last() {
                    let parent = parent_slice.join(".");
                    keys.entry((parent.clone(), leaf.clone()))
                        .or_insert(line_no);
                    // Record dotted prefixes as tables too (e.g. `model.route.x = 1`).
                    for prefix_len in 1..full_segments.len() {
                        let prefix = full_segments[..prefix_len].join(".");
                        tables.entry(prefix).or_insert(line_no);
                    }
                }
            }
        }

        Self { tables, keys }
    }

    fn line_for_table(&self, table: &str) -> Option<usize> {
        self.tables.get(table).copied()
    }

    fn line_for_top(&self, key: &str) -> usize {
        self.keys
            .get(&(String::new(), key.to_owned()))
            .or_else(|| self.tables.get(key))
            .copied()
            .unwrap_or(1)
    }

    fn line_for(&self, table: &str, key: &str) -> usize {
        let dotted = if table.is_empty() {
            key.to_owned()
        } else {
            format!("{table}.{key}")
        };
        self.keys
            .get(&(table.to_owned(), key.to_owned()))
            .or_else(|| self.tables.get(&dotted))
            .or_else(|| self.tables.get(table))
            .copied()
            .unwrap_or(1)
    }
}

fn strip_toml_comment(line: &str) -> &str {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '#' if !in_double && !in_single => return &line[..idx],
            _ => {}
        }
    }
    line
}

fn split_key_value_line(line: &str) -> Option<(&str, &str)> {
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;
    for (idx, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '=' if !in_double && !in_single => return Some((&line[..idx], &line[idx + 1..])),
            _ => {}
        }
    }
    None
}

fn split_toml_key_path(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_double = false;
    let mut in_single = false;
    let mut escaped = false;

    for ch in raw.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double => escaped = true,
            '"' if !in_single => in_double = !in_double,
            '\'' if !in_double => in_single = !in_single,
            '.' if !in_double && !in_single => {
                let seg = cur.trim();
                if !seg.is_empty() {
                    out.push(seg.to_owned());
                }
                cur.clear();
            }
            other => cur.push(other),
        }
    }
    let seg = cur.trim();
    if !seg.is_empty() {
        out.push(seg.to_owned());
    }
    out
}
