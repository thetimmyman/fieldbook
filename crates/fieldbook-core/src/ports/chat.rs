//! Pure chat and AI status shapes (`ai-config-and-keys`, AD-01, AD-15).
//!
//! No I/O. Later cards extend this module with `SendPreview`, `Scan`, `Transport`, `ChatCaps`,
//! `StreamState`, `ErrKind`, `ApiError`, `AnswerCard`, `SendReply`, and `NoteReply`.

use serde::{Deserialize, Serialize};

use crate::labels::LabelId;
use crate::ports::Caps;

/// Connection and policy readiness state of an adapter (`ai-config-and-keys` step 1).
///
/// `Unavailable` records an availability failure (such as a missing `net` build feature, `FB0912`).
/// `Blocked` records a policy failure (`FB0413`, `FB0918`, or `FB0901`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ConnState {
    /// Adapter row is not built yet (`State::Later`).
    Off,
    /// Copy-paste Handoff adapter (`handoff@1`).
    Handoff,
    /// Adapter is built, permitted by policy, and ready to accept calls.
    Ready,
    /// Adapter is currently handling an in-flight call.
    Busy,
    /// Adapter cannot run because of an availability condition (e.g. `FB0912`).
    Unavailable {
        /// Four-digit `FBxxxx` diagnostic code.
        code: String,
    },
    /// Adapter is denied by the company policy or owner narrowing (`FB0413`, `FB0918`, or `FB0901`).
    Blocked {
        /// Four-digit `FBxxxx` diagnostic code.
        code: String,
    },
}

impl ConnState {
    /// Returns the stable wire variant name (`Off`, `Handoff`, `Ready`, `Busy`, `Unavailable`, or `Blocked`).
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Handoff => "Handoff",
            Self::Ready => "Ready",
            Self::Busy => "Busy",
            Self::Unavailable { .. } => "Unavailable",
            Self::Blocked { .. } => "Blocked",
        }
    }

    /// Returns the diagnostic code carried by [`Self::Unavailable`] or [`Self::Blocked`], if any.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Unavailable { code } | Self::Blocked { code } => Some(code.as_str()),
            Self::Off | Self::Handoff | Self::Ready | Self::Busy => None,
        }
    }
}

/// Presence state of an endpoint's API key variable (never holds key text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyState {
    /// The configured environment variable is set to a non-empty value.
    Set,
    /// The configured environment variable is unset or empty.
    Missing,
    /// This adapter or endpoint does not require an environment key.
    NotNeeded,
}

impl KeyState {
    /// Returns the canonical lowercase label for plain-text status display.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Missing => "missing",
            Self::NotNeeded => "not-needed",
        }
    }
}

/// Summary of the newest completed `ai.call` recorded for an endpoint in the audit journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastCall {
    /// Closed outcome word (`ok`, `failed`, `invalid`, `truncated`, or `cancelled`).
    pub outcome: String,
    /// UTC event timestamp (`YYYY-MM-DDTHH:MM:SSZ`) of the completed `ai.call`.
    pub at: String,
    /// Number of transport attempts recorded on the `ai.call` event.
    pub attempts: u64,
}

/// Reported status of one port adapter or configured model endpoint (`ai status --json` and chat header chip).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterStatus {
    /// Architectural port name (`model`, `extractor`, `admit`, `backup`, `authz`, `store`, ...).
    pub port: String,
    /// Adapter registry identifier (`<name>@<n>`).
    pub adapter: String,
    /// Endpoint name (or `"handoff"`) for Model port entries; absent for non-model ports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Connection and policy readiness state.
    pub state: ConnState,
    /// Effective allowed label identifiers; empty when the policy or owner allows nothing.
    pub allow: Vec<LabelId>,
    /// Key presence state (never a key value).
    pub key: KeyState,
    /// Declared runtime capabilities of the adapter (`streaming` is `false` until a streaming adapter lands).
    pub caps: Caps,
    /// Newest completed `ai.call` for this endpoint, omitted when none is recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<LastCall>,
}
