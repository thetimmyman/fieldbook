//! Git work-tree guard (`git_guard`), folder placement rules (`placement`), and `.fieldbook/`
//! ignore check (`fieldbook_is_ignored`)
//! (`docs/decisions/0004-company-boundary.md#4-stores-and-admission`, `FB0415`).

use serde::{Deserialize, Serialize};

use crate::check::Code;

use super::id::{OWNER_SUPPLIED, PUBLIC};
use super::set::LabelSet;
use super::store::{Refusal, RefuseReason};

/// Pure facts about whether a target path sits inside a Git work tree and whether it is inside
/// the verified, ignored Home `state` folder (or its restore drill).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GitContext {
    /// Whether the path has a `.git` ancestor.
    pub under_git: bool,
    /// Whether the path is inside the Home preset's resolved `state` root (including its drill)
    /// and `.fieldbook/` is verified to be ignored by the checkout.
    pub ignored_home_state: bool,
}

impl GitContext {
    /// Outside any Git work tree.
    #[must_use]
    pub const fn outside() -> Self {
        Self {
            under_git: false,
            ignored_home_state: false,
        }
    }

    /// Inside a Git work tree (not in ignored Home state).
    #[must_use]
    pub const fn in_git() -> Self {
        Self {
            under_git: true,
            ignored_home_state: false,
        }
    }

    /// Inside a Git work tree within verified ignored Home `state` (or its drill).
    #[must_use]
    pub const fn ignored_home_state() -> Self {
        Self {
            under_git: true,
            ignored_home_state: true,
        }
    }
}

/// Pure guard refusing non-public data under a `.git` ancestor (`FB0415`, reason `git`).
///
/// - Outside Git (`!context.under_git`), all label sets pass.
/// - Under Git (`context.under_git`), `public` is permitted, and `owner-supplied` is permitted
///   only when `context.ignored_home_state` is `true`. Any other label is refused with `FB0415`
///   and reason `git`.
pub fn git_guard(labels: &LabelSet, context: GitContext) -> Result<(), Refusal> {
    if !context.under_git {
        return Ok(());
    }

    let allowed = labels.ids().iter().all(|id| {
        if *id == PUBLIC {
            true
        } else if *id == OWNER_SUPPLIED {
            context.ignored_home_state
        } else {
            false
        }
    });

    if allowed {
        Ok(())
    } else {
        Err(Refusal {
            code: Code::Fb0415,
            reason: RefuseReason::Git,
            labels: labels.clone(),
        })
    }
}

/// Operating-system facts about a folder (`World::place(path)`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PlaceFacts {
    /// Whether the folder or an ancestor is a reparse point, junction, or symbolic link.
    pub reparse: bool,
    /// Whether the folder carries cloud-sync attributes or is registered as a sync root.
    pub sync_root: bool,
    /// Whether the folder resides on a network drive or UNC share.
    pub network: bool,
}

impl PlaceFacts {
    /// Local folder with no reparse point, sync root, or network drive.
    #[must_use]
    pub const fn local() -> Self {
        Self {
            reparse: false,
            sync_root: false,
            network: false,
        }
    }
}

/// Role of the folder being checked by [`placement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PlaceRole {
    /// A live store root (`corpus`, `overlay`, `working`).
    LiveStore,
    /// The `state` folder.
    State,
    /// A sync drop folder (`state\drop\...` or a stream's `path`).
    Drop,
    /// An export destination path.
    ExportTarget,
    /// A backup target folder.
    BackupTarget,
}

/// Classified folder kind returned by [`placement`] for journaling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaceKind {
    /// Local non-synced folder.
    Local,
    /// Network drive or UNC share.
    Network,
    /// Cloud sync root.
    SyncRoot,
    /// Reparse point or junction.
    Reparse,
}

/// Input to [`placement`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Whether the machine is on the Laptop preset (`true`) or Home/CI (`false`).
    pub laptop: bool,
    /// Whether the folder lies under a `.git` ancestor.
    pub under_git: bool,
    /// Which role the folder serves.
    pub role: PlaceRole,
    /// Operating-system facts about the folder (`World::place`).
    pub facts: PlaceFacts,
    /// Whether a synced/reparse export or backup target was explicitly declared.
    pub declared: bool,
}

fn refuse_place() -> Refusal {
    Refusal {
        code: Code::Fb0415,
        reason: RefuseReason::Place,
        labels: LabelSet::unlabelled(),
    }
}

const fn classify_facts(facts: PlaceFacts) -> PlaceKind {
    if facts.sync_root {
        PlaceKind::SyncRoot
    } else if facts.reparse {
        PlaceKind::Reparse
    } else if facts.network {
        PlaceKind::Network
    } else {
        PlaceKind::Local
    }
}

/// Judges whether a folder with the given [`Placement`] is permitted (`FB0415`, reason `place`).
///
/// Never inspects a folder's name; decisions depend only on `laptop`, `under_git`, `role`,
/// `facts`, and `declared`.
pub fn placement(input: Placement) -> Result<PlaceKind, Refusal> {
    match input.role {
        PlaceRole::LiveStore | PlaceRole::State => {
            if input.laptop && input.under_git {
                return Err(refuse_place());
            }
            if input.facts.reparse || input.facts.sync_root || input.facts.network {
                return Err(refuse_place());
            }
            Ok(PlaceKind::Local)
        }
        PlaceRole::Drop => {
            if input.under_git || input.facts.sync_root || input.facts.reparse {
                return Err(refuse_place());
            }
            Ok(classify_facts(input.facts))
        }
        PlaceRole::ExportTarget | PlaceRole::BackupTarget => {
            if input.under_git {
                return Err(refuse_place());
            }
            if (input.facts.sync_root || input.facts.reparse) && !input.declared {
                return Err(refuse_place());
            }
            Ok(classify_facts(input.facts))
        }
    }
}

/// Pure check that `.fieldbook/` is ignored by the given `.gitignore` text on the Home layout.
#[must_use]
pub fn fieldbook_is_ignored(ignore_text: &str) -> bool {
    let mut ignored = false;
    for raw_line in ignore_text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if matches!(
            line,
            ".fieldbook"
                | ".fieldbook/"
                | "/.fieldbook"
                | "/.fieldbook/"
                | ".fieldbook/**"
                | "/.fieldbook/**"
        ) {
            ignored = true;
        } else if line.starts_with('!') {
            ignored = false;
        }
    }
    ignored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::labels::id::LabelId;

    fn id(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("{e}"))
    }

    // control: GD-01
    #[test]
    fn a_company_label_under_git_is_refused() {
        let set = LabelSet::from_ids([id("np-0007")]);
        let err = git_guard(&set, GitContext::in_git()).unwrap_err();
        assert_eq!(err.code, Code::Fb0415);
        assert_eq!(err.reason, RefuseReason::Git);
        assert_eq!(err.labels, set);

        // Even inside ignored_home_state, a company label is refused under Git.
        let err_home = git_guard(&set, GitContext::ignored_home_state()).unwrap_err();
        assert_eq!(err_home.code, Code::Fb0415);
        assert_eq!(err_home.reason, RefuseReason::Git);
    }

    #[test]
    fn public_may_sit_under_git() {
        assert_eq!(git_guard(&LabelSet::public(), GitContext::in_git()), Ok(()));
        assert_eq!(
            git_guard(&LabelSet::public(), GitContext::ignored_home_state()),
            Ok(())
        );
    }

    #[test]
    fn owner_supplied_requires_ignored_home_state() {
        let owner = LabelSet::owner_supplied();
        // Verified ignored Home State and its restore drill pass.
        assert_eq!(git_guard(&owner, GitContext::ignored_home_state()), Ok(()));

        // Unignored State, Corpus, or export/backup targets under Git refuse owner-supplied.
        let err = git_guard(&owner, GitContext::in_git()).unwrap_err();
        assert_eq!(err.code, Code::Fb0415);
        assert_eq!(err.reason, RefuseReason::Git);
    }

    #[test]
    fn nothing_is_refused_outside_git() {
        for set in [
            LabelSet::public(),
            LabelSet::unlabelled(),
            LabelSet::marked(),
            LabelSet::owner_supplied(),
            LabelSet::from_ids([id("np-0007"), id("bid-0003")]),
        ] {
            assert_eq!(git_guard(&set, GitContext::outside()), Ok(()));
        }
    }

    #[test]
    fn a_laptop_store_under_git_is_refused() {
        for role in [PlaceRole::LiveStore, PlaceRole::State] {
            let err = placement(Placement {
                laptop: true,
                under_git: true,
                role,
                facts: PlaceFacts::local(),
                declared: false,
            })
            .unwrap_err();
            assert_eq!(err.code, Code::Fb0415);
            assert_eq!(err.reason, RefuseReason::Place);
        }
    }

    // control: GD-01
    #[test]
    fn a_laptop_store_in_a_sync_root_is_refused() {
        for role in [PlaceRole::LiveStore, PlaceRole::State] {
            let err = placement(Placement {
                laptop: true,
                under_git: false,
                role,
                facts: PlaceFacts {
                    reparse: false,
                    sync_root: true,
                    network: false,
                },
                declared: true,
            })
            .unwrap_err();
            assert_eq!(err.code, Code::Fb0415);
            assert_eq!(err.reason, RefuseReason::Place);
        }
    }

    #[test]
    fn a_laptop_store_behind_a_junction_is_refused() {
        for role in [PlaceRole::LiveStore, PlaceRole::State] {
            let err = placement(Placement {
                laptop: true,
                under_git: false,
                role,
                facts: PlaceFacts {
                    reparse: true,
                    sync_root: false,
                    network: false,
                },
                declared: true,
            })
            .unwrap_err();
            assert_eq!(err.code, Code::Fb0415);
            assert_eq!(err.reason, RefuseReason::Place);
        }
    }

    #[test]
    fn a_laptop_store_on_a_network_drive_is_refused() {
        for role in [PlaceRole::LiveStore, PlaceRole::State] {
            let err = placement(Placement {
                laptop: true,
                under_git: false,
                role,
                facts: PlaceFacts {
                    reparse: false,
                    sync_root: false,
                    network: true,
                },
                declared: true,
            })
            .unwrap_err();
            assert_eq!(err.code, Code::Fb0415);
            assert_eq!(err.reason, RefuseReason::Place);
        }
    }

    #[test]
    fn a_drop_folder_may_be_on_the_network_but_not_a_sync_root() {
        assert_eq!(
            placement(Placement {
                laptop: true,
                under_git: false,
                role: PlaceRole::Drop,
                facts: PlaceFacts::local(),
                declared: false,
            }),
            Ok(PlaceKind::Local)
        );
        assert_eq!(
            placement(Placement {
                laptop: true,
                under_git: false,
                role: PlaceRole::Drop,
                facts: PlaceFacts {
                    reparse: false,
                    sync_root: false,
                    network: true,
                },
                declared: false,
            }),
            Ok(PlaceKind::Network)
        );
        let err_sync = placement(Placement {
            laptop: true,
            under_git: false,
            role: PlaceRole::Drop,
            facts: PlaceFacts {
                reparse: false,
                sync_root: true,
                network: false,
            },
            declared: true,
        })
        .unwrap_err();
        assert_eq!(err_sync.code, Code::Fb0415);
        assert_eq!(err_sync.reason, RefuseReason::Place);

        let err_git = placement(Placement {
            laptop: true,
            under_git: true,
            role: PlaceRole::Drop,
            facts: PlaceFacts::local(),
            declared: false,
        })
        .unwrap_err();
        assert_eq!(err_git.code, Code::Fb0415);
    }

    #[test]
    fn a_sync_root_backup_target_is_valid_only_when_declared() {
        let sync_facts = PlaceFacts {
            reparse: false,
            sync_root: true,
            network: false,
        };
        let err = placement(Placement {
            laptop: true,
            under_git: false,
            role: PlaceRole::BackupTarget,
            facts: sync_facts,
            declared: false,
        })
        .unwrap_err();
        assert_eq!(err.code, Code::Fb0415);
        assert_eq!(err.reason, RefuseReason::Place);

        assert_eq!(
            placement(Placement {
                laptop: true,
                under_git: false,
                role: PlaceRole::BackupTarget,
                facts: sync_facts,
                declared: true,
            }),
            Ok(PlaceKind::SyncRoot)
        );
    }

    #[test]
    fn a_folder_named_onedrive_that_is_local_is_not_refused() {
        // Folder names are never inspected; a folder with local PlaceFacts is accepted.
        let _folder_name = "C:\\Users\\ synthetic \\OneDrive\\Fieldbook";
        assert_eq!(
            placement(Placement {
                laptop: true,
                under_git: false,
                role: PlaceRole::LiveStore,
                facts: PlaceFacts::local(),
                declared: false,
            }),
            Ok(PlaceKind::Local)
        );
    }

    #[test]
    fn the_home_layout_may_sit_under_git() {
        for role in [PlaceRole::LiveStore, PlaceRole::State] {
            assert_eq!(
                placement(Placement {
                    laptop: false,
                    under_git: true,
                    role,
                    facts: PlaceFacts::local(),
                    declared: false,
                }),
                Ok(PlaceKind::Local)
            );
        }
    }

    #[test]
    fn home_state_must_be_ignored() {
        assert!(fieldbook_is_ignored(
            "# comment\ntarget/\n.fieldbook/\nout/\n"
        ));
        assert!(fieldbook_is_ignored("/.fieldbook/\n"));
        assert!(!fieldbook_is_ignored("target/\nout/\n"));
        assert!(!fieldbook_is_ignored(".fieldbook/\n!.fieldbook/\n"));
        assert!(!fieldbook_is_ignored(".fieldbook/**\n!**/call.json\n"));
    }
}
