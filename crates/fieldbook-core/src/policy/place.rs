//! Policy file placement guard (`policy::place`, `FB0414`) and laptop folder flag guard
//! (`flags_allowed`, `FB0419`) (`policy-load`, `ADR 0004 §3`,
//! `docs/reference/policy.md#where-it-is-read-from`).

use std::fmt;
use std::path::{Path, PathBuf};

use crate::check::Code;
use crate::labels::{LabelSet, Refusal, RefuseReason};

/// Where the program looks for a policy file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Source {
    /// Home and CI default: no policy file is read and the floor applies.
    #[default]
    None,
    /// Machine-level policy file under `%ProgramData%\Fieldbook\` (`Fieldbook/policy.toml`).
    Machine(PathBuf),
    /// Test-hook `--policy FILE` override (`#[cfg(feature = "test-hooks")]`).
    Flag(PathBuf),
}

impl Source {
    /// Returns the filesystem path of the policy source, if any.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::None => None,
            Self::Machine(p) | Self::Flag(p) => Some(p.as_path()),
        }
    }
}

/// Pure facts describing where a candidate policy file sits relative to editable roots.
///
/// Distinct from [`crate::labels::placement::Placement`]; the module path distinguishes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Whether the policy path lies inside the data root (e.g. `state`, `overlay`, `working`).
    pub in_data_root: bool,
    /// Whether the policy path lies inside the Corpus root.
    pub in_corpus: bool,
    /// Whether the policy path has a `.git` ancestor.
    pub under_git: bool,
    /// Whether a `.git` ancestor is permitted because this is a test build reading a fixture.
    pub fixture_ok: bool,
}

/// Why [`place`] refused a policy file path (`FB0414`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PlaceWhy {
    /// Inside the data folder (`in_data_root`).
    DataRoot,
    /// Inside the Corpus (`in_corpus`).
    Corpus,
    /// Inside a Git work tree (`under_git && !fixture_ok`).
    GitWorkTree,
}

impl PlaceWhy {
    /// Returns the diagnostic code (`FB0414`).
    #[must_use]
    pub const fn code(self) -> Code {
        Code::Fb0414
    }

    /// Returns the location noun phrase used in the `FB0414` refusal diagnostic.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DataRoot => "the data folder",
            Self::Corpus => "the Corpus",
            Self::GitWorkTree => "a git work tree",
        }
    }

    /// Formats the `FB0414` refusal line for `path`.
    #[must_use]
    pub fn format_refusal(self, path: &Path) -> String {
        format!(
            "error[FB0414]: the policy file {} is in {}, where the owner can edit it; the policy is read from a place only IT can write",
            path.display(),
            self.as_str()
        )
    }
}

impl fmt::Display for PlaceWhy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::error::Error for PlaceWhy {}

/// Judges whether a policy file with [`Placement`] `p` may be opened (`FB0414`).
///
/// - `in_data_root` and `in_corpus` are refused on every layout.
/// - `under_git` is refused unless `fixture_ok` is `true` (set only in test builds).
///
/// # Errors
///
/// Returns [`PlaceWhy`] when the policy path sits in an owner-editable tree.
pub const fn place(p: &Placement) -> Result<(), PlaceWhy> {
    if p.in_data_root {
        return Err(PlaceWhy::DataRoot);
    }
    if p.in_corpus {
        return Err(PlaceWhy::Corpus);
    }
    if p.under_git && !p.fixture_ok {
        return Err(PlaceWhy::GitWorkTree);
    }
    Ok(())
}

/// Command-line folder override flags checked by [`flags_allowed`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Flags {
    /// Optional `--state` folder flag.
    pub state: Option<PathBuf>,
    /// Optional `--overlay` folder flag.
    pub overlay: Option<PathBuf>,
    /// Optional `--working` folder flag.
    pub working: Option<PathBuf>,
}

/// Fixed laptop root folders against which [`flags_allowed`] validates [`Flags`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaptopFolders {
    /// Fixed laptop `state` folder.
    pub state: PathBuf,
    /// Fixed laptop `overlay` folder.
    pub overlay: PathBuf,
    /// Fixed laptop `working` folder.
    pub working: PathBuf,
}

/// Refuses (`FB0419`) a `--state`, `--overlay`, or `--working` flag that names a folder other
/// than the fixed laptop folder of the same name when `laptop` is `true`, and allows every flag
/// when `laptop` is `false`.
///
/// # Errors
///
/// Returns [`Refusal`] (`FB0419`, reason `place`) when `laptop` is `true` and any present flag
/// differs from the corresponding path in `fixed`.
pub fn flags_allowed(laptop: bool, fixed: &LaptopFolders, flags: &Flags) -> Result<(), Refusal> {
    if !laptop {
        return Ok(());
    }
    let state_bad = flags.state.as_ref().is_some_and(|p| p != &fixed.state);
    let overlay_bad = flags.overlay.as_ref().is_some_and(|p| p != &fixed.overlay);
    let working_bad = flags.working.as_ref().is_some_and(|p| p != &fixed.working);
    if state_bad || overlay_bad || working_bad {
        return Err(Refusal {
            code: Code::Fb0419,
            reason: RefuseReason::Place,
            labels: LabelSet::unlabelled(),
        });
    }
    Ok(())
}
