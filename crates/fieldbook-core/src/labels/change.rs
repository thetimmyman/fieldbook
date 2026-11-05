//! Rules governing who may add a label (`AddContext`, `may_add`, `FB0822`)
//! (`docs/decisions/0004-company-boundary.md#2-labels`).

use serde::{Deserialize, Serialize};

use crate::check::Code;

use super::id::LabelId;
use super::set::LabelSet;
use super::store::{Refusal, RefuseReason};

/// Machine layout preset (`Home` or `Laptop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    /// Home or CI checkout preset.
    Home,
    /// Company laptop preset.
    Laptop,
}

/// Declared actor role from `settings.toml` (`owner`, `lead`, or `agent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeclaredRole {
    /// Owner role (`"owner"`).
    Owner,
    /// Lead role (`"lead"`).
    Lead,
    /// Agent role (`"agent"`).
    Agent,
}

impl DeclaredRole {
    /// Parses a role word (`"owner"`, `"lead"`, `"agent"`).
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "owner" => Some(Self::Owner),
            "lead" => Some(Self::Lead),
            "agent" => Some(Self::Agent),
            _ => None,
        }
    }

    /// Returns the wire spelling of this role.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Lead => "lead",
            Self::Agent => "agent",
        }
    }
}

/// Context for a `label add` authorization check ([`may_add`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddContext {
    /// Layout preset (`Home` checks `role`; `Laptop` ignores `role`).
    pub preset: Preset,
    /// Declared role from `settings.toml`.
    pub role: DeclaredRole,
    /// Whether standard input is an interactive console.
    pub input_console: bool,
    /// Whether standard output is an interactive console.
    pub output_console: bool,
    /// Whether the typed confirmation named the exact number of items.
    pub confirmed_count: bool,
    /// Whether the target store has `hold = true`.
    pub hold: bool,
    /// `Some(true)` / `Some(false)` when the store row names `changers` (from the logon token);
    /// `None` when the store row names no `changers` group.
    pub in_changers: Option<bool>,
}

/// Returns `true` when `confirmation` names the non-zero item `count` as a standalone decimal token.
#[must_use]
pub fn confirmation_names_count(confirmation: &str, count: usize) -> bool {
    if count == 0 {
        return false;
    }
    let expected = count.to_string();
    confirmation
        .split(|c: char| !c.is_ascii_digit())
        .any(|tok| tok == expected)
}

fn refuse_add(label: &LabelId) -> Refusal {
    Refusal {
        code: Code::Fb0822,
        reason: RefuseReason::Label,
        labels: LabelSet::from_ids([label.clone()]),
    }
}

/// Checks whether a caller in `context` may add `label` via `label add` (`FB0822`).
///
/// Refuses:
/// - Any built-in label (`public`, `unlabelled`, `marked`, `owner-supplied`).
/// - A caller without an interactive console on both input and output.
/// - A confirmation that did not name the item count.
/// - A store with `hold = true`.
/// - A logon token outside the store's `changers` group (`in_changers == Some(false)`).
/// - On the `Home` preset, the `agent` role (`Preset::Laptop` ignores the declared role).
pub fn may_add(context: &AddContext, label: &LabelId) -> Result<(), Refusal> {
    if label.is_builtin() {
        return Err(refuse_add(label));
    }
    if !context.input_console || !context.output_console {
        return Err(refuse_add(label));
    }
    if !context.confirmed_count {
        return Err(refuse_add(label));
    }
    if context.hold {
        return Err(refuse_add(label));
    }
    if context.in_changers == Some(false) {
        return Err(refuse_add(label));
    }
    if context.preset == Preset::Home && context.role == DeclaredRole::Agent {
        return Err(refuse_add(label));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::id::{MARKED, OWNER_SUPPLIED, PUBLIC, UNLABELLED};

    fn id(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
    }

    fn valid_home_context() -> AddContext {
        AddContext {
            preset: Preset::Home,
            role: DeclaredRole::Owner,
            input_console: true,
            output_console: true,
            confirmed_count: true,
            hold: false,
            in_changers: None,
        }
    }

    #[test]
    fn a_person_at_a_console_may_add_a_real_label() {
        let ctx = valid_home_context();
        assert_eq!(may_add(&ctx, &id("np-0007")), Ok(()));

        let lead_ctx = AddContext {
            role: DeclaredRole::Lead,
            in_changers: Some(true),
            ..valid_home_context()
        };
        assert_eq!(may_add(&lead_ctx, &id("bid-0003")), Ok(()));
    }

    // control: LB-02
    #[test]
    fn a_pipe_on_input_is_refused() {
        let ctx = AddContext {
            input_console: false,
            ..valid_home_context()
        };
        let err = may_add(&ctx, &id("np-0007")).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
        assert_eq!(err.reason, RefuseReason::Label);
    }

    #[test]
    fn redirected_output_is_refused() {
        let ctx = AddContext {
            output_console: false,
            ..valid_home_context()
        };
        let err = may_add(&ctx, &id("np-0007")).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
    }

    #[test]
    fn the_confirmation_must_name_the_count() {
        assert!(confirmation_names_count("yes 3 items", 3));
        assert!(!confirmation_names_count("yes", 3));
        assert!(!confirmation_names_count("yes 13 items", 3));

        let ctx = AddContext {
            confirmed_count: confirmation_names_count("yes", 3),
            ..valid_home_context()
        };
        let err = may_add(&ctx, &id("np-0007")).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
    }

    #[test]
    fn the_role_word_is_ignored_on_the_laptop_preset() {
        let ctx = AddContext {
            preset: Preset::Laptop,
            role: DeclaredRole::Agent,
            ..valid_home_context()
        };
        assert_eq!(may_add(&ctx, &id("np-0007")), Ok(()));
    }

    #[test]
    fn at_home_the_agent_role_is_refused() {
        let ctx = AddContext {
            preset: Preset::Home,
            role: DeclaredRole::Agent,
            ..valid_home_context()
        };
        let err = may_add(&ctx, &id("np-0007")).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
    }

    #[test]
    fn a_token_outside_changers_is_refused() {
        let ctx = AddContext {
            in_changers: Some(false),
            ..valid_home_context()
        };
        let err = may_add(&ctx, &id("np-0007")).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
    }

    #[test]
    fn a_store_on_hold_is_refused() {
        let ctx = AddContext {
            hold: true,
            ..valid_home_context()
        };
        let err = may_add(&ctx, &id("np-0007")).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
    }

    // control: LB-02
    #[test]
    fn no_person_may_add_public() {
        let ctx = valid_home_context();
        let err = may_add(&ctx, &PUBLIC).unwrap_err();
        assert_eq!(err.code, Code::Fb0822);
        assert_eq!(err.reason, RefuseReason::Label);
    }

    #[test]
    fn no_person_may_add_unlabelled_marked_or_owner_supplied() {
        let ctx = valid_home_context();
        for builtin in [UNLABELLED, MARKED, OWNER_SUPPLIED] {
            let err = may_add(&ctx, &builtin).unwrap_err();
            assert_eq!(err.code, Code::Fb0822);
        }
    }
}
