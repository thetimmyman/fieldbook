//! Policy file I/O: placement resolution (`resolve_place`, `FB0414`), bounded reader
//! (`PolicySource::read`, `PolicySource::read_draft`), recorded revision persistence
//! (`seen_path`, `read_seen`, `write_seen`), owner narrowing settings (`owner_settings`),
//! and company-store detection (`company_store_beside`) (`policy-load` step 5).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "io::policy is the single authorized reader of the policy file and its placement"
)]

use std::ffi::OsString;
use std::fmt;
use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

use fieldbook_core::check::Code;
use fieldbook_core::labels::{LabelSet, Refusal, RefuseReason};
pub use fieldbook_core::policy::Source;
use fieldbook_core::policy::{
    FileObservation, OVER_LIMIT_PREFIX_BYTES, OwnerSettings, PlaceWhy, Placement, Seen,
    place as check_place, writer_check,
};

use crate::World;
use crate::io::{Audited, WorldError};

/// Placement refusal raised before a policy file is opened (`FB0414`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaceRefusal {
    /// The policy path as given.
    pub path: PathBuf,
    /// Why placement failed (`DataRoot`, `Corpus`, or `GitWorkTree`).
    pub why: PlaceWhy,
}

impl PlaceRefusal {
    /// Returns the diagnostic code (`FB0414`).
    #[must_use]
    pub const fn code(&self) -> Code {
        Code::Fb0414
    }

    /// Converts this placement refusal into a core [`Refusal`] (`FB0414`, reason `place`).
    #[must_use]
    pub fn to_refusal(&self) -> Refusal {
        Refusal {
            code: Code::Fb0414,
            reason: RefuseReason::Place,
            labels: LabelSet::unlabelled(),
        }
    }
}

impl fmt::Display for PlaceRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.why.format_refusal(&self.path))
    }
}

impl std::error::Error for PlaceRefusal {}

/// The single reader of a company policy file (`PolicySource::read` and `PolicySource::read_draft`).
pub struct PolicySource;

impl PolicySource {
    /// Discovers the policy [`Source`] from `world`, `laptop`, and optional test-hook CLI flags.
    #[must_use]
    pub fn discover_with_flags(
        world: &dyn World,
        laptop: bool,
        policy_flag: Option<&Path>,
        program_data_flag: Option<&Path>,
    ) -> Source {
        if let Some(p) = policy_flag {
            let abs = if p.is_absolute() {
                p.to_path_buf()
            } else {
                world.cwd().join(p)
            };
            return Source::Flag(abs);
        }
        if let Some(pd) = program_data_flag {
            let abs = if pd.is_absolute() {
                pd.to_path_buf()
            } else {
                world.cwd().join(pd)
            };
            return Source::Machine(abs.join("Fieldbook").join("policy.toml"));
        }
        Self::discover(world, laptop)
    }

    /// Discovers the policy [`Source`] from `world` and `laptop`.
    #[must_use]
    pub fn discover(world: &dyn World, laptop: bool) -> Source {
        #[cfg(feature = "test-hooks")]
        {
            if let Some(pd) = world.program_data() {
                if pd.extension().and_then(|s| s.to_str()) == Some("toml") {
                    return Source::Flag(pd);
                }
                return Source::Machine(pd.join("Fieldbook").join("policy.toml"));
            }
        }
        if laptop && let Some(pd) = world.program_data() {
            return Source::Machine(pd.join("Fieldbook").join("policy.toml"));
        }
        Source::None
    }

    /// Opens and reads at most `1 MiB + 1` bytes from `source`, after verifying placement
    /// (`resolve_place`, `FB0414`) and querying `world.writers(path)` (`FB0418`).
    ///
    /// # Errors
    ///
    /// Returns [`PlaceRefusal`] (`FB0414`) before opening the file if `source` lies inside the
    /// data root, the Corpus, or a Git work tree.
    pub fn read(world: &dyn World, source: &Source) -> Result<FileObservation, PlaceRefusal> {
        Self::read_in(world, source, world.state_dir(), world.tree_root())
    }

    /// Like [`PolicySource::read`] with explicit `state` and `corpus` paths.
    ///
    /// # Errors
    ///
    /// Returns [`PlaceRefusal`] (`FB0414`) before opening the file if `source` lies inside `state`,
    /// `corpus`, or a Git work tree.
    pub fn read_in(
        world: &dyn World,
        source: &Source,
        state: &Path,
        corpus: &Path,
    ) -> Result<FileObservation, PlaceRefusal> {
        let Some(path) = source.path() else {
            return Ok(FileObservation::absent());
        };
        resolve_place(world, path, state, corpus)?;

        let exists = std::fs::symlink_metadata(path).is_ok() || world.exists(path);
        if !exists {
            return Ok(FileObservation::absent());
        }

        let writers = writer_check(&world.writers(path));
        match read_bounded_bytes(path) {
            Ok(Some(bytes)) => Ok(FileObservation::from_bytes(&bytes, writers)),
            Ok(None) => Ok(FileObservation::absent()),
            Err(()) => Ok(FileObservation::unreadable(writers)),
        }
    }

    /// Reads a draft policy file for `fieldbook policy check FILE` in any build, skipping
    /// `resolve_place` and the writer check.
    #[must_use]
    pub fn read_draft(path: &Path) -> FileObservation {
        match read_bounded_bytes(path) {
            Ok(Some(bytes)) => FileObservation::from_bytes(&bytes, Ok(())),
            Ok(None) => FileObservation::absent(),
            Err(()) => FileObservation::unreadable(Ok(())),
        }
    }
}

fn read_bounded_bytes(path: &Path) -> Result<Option<Vec<u8>>, ()> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    let mut buf = Vec::new();
    let limit = OVER_LIMIT_PREFIX_BYTES as u64;
    if file.take(limit).read_to_end(&mut buf).is_err() {
        return Err(());
    }
    Ok(Some(buf))
}

/// Canonicalizes `path` against `cwd`; when `path` does not exist, canonicalizes its nearest
/// existing ancestor folder and appends the remaining components so links into the data root or
/// Corpus are resolved even for an absent policy path.
#[must_use]
pub fn canonicalize_nearest(path: &Path, cwd: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    };

    // Normalize lexical `.` and `..` components only after trying full canonicalize first.
    if let Ok(canon) = std::fs::canonicalize(&abs) {
        return canon;
    }

    let mut cur = abs.as_path();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if let Ok(canon_base) = std::fs::canonicalize(cur) {
            let mut out = canon_base;
            for part in tail.into_iter().rev() {
                if part == "." {
                    continue;
                } else if part == ".." {
                    let _ = out.pop();
                } else {
                    out.push(part);
                }
            }
            return out;
        }
        match (cur.file_name(), cur.parent()) {
            (Some(name), Some(parent)) if parent != cur => {
                tail.push(name.to_os_string());
                cur = parent;
            }
            _ => break,
        }
    }

    lexical_clean(&abs)
}

fn lexical_clean(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(p) => out.push(p.as_os_str()),
            Component::RootDir => out.push(comp.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    out
}

fn starts_with_whole_components(path: &Path, root: &Path) -> bool {
    let path_comps: Vec<_> = path.components().collect();
    let root_comps: Vec<_> = root.components().collect();
    if root_comps.is_empty() || path_comps.len() < root_comps.len() {
        return false;
    }
    for (a, b) in path_comps.iter().zip(root_comps.iter()) {
        if cfg!(windows) {
            if !a
                .as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
            {
                return false;
            }
        } else if a != b {
            return false;
        }
    }
    true
}

/// Resolves the canonical placement of `path` relative to `state`, `corpus`, and `.git`
/// ancestors (`World::exists`), passing the pure [`Placement`] facts to [`check_place`] (`FB0414`).
///
/// # Errors
///
/// Returns [`PlaceRefusal`] (`FB0414`) when `path` lies inside the data root, the Corpus, or a
/// Git work tree (unless `cfg!(feature = "test-hooks")` permits a checkout fixture).
pub fn resolve_place(
    world: &dyn World,
    path: &Path,
    state: &Path,
    corpus: &Path,
) -> Result<(), PlaceRefusal> {
    let canon_path = canonicalize_nearest(path, world.cwd());
    let canon_state = canonicalize_nearest(state, world.cwd());
    let canon_corpus = canonicalize_nearest(corpus, world.cwd());

    let in_data_root = starts_with_whole_components(&canon_path, &canon_state);
    let in_corpus = starts_with_whole_components(&canon_path, &canon_corpus);

    let mut under_git = false;
    let start_dir = canon_path.parent().unwrap_or(&canon_path);
    for anc in start_dir.ancestors() {
        if anc.as_os_str().is_empty() {
            continue;
        }
        let git_entry = anc.join(".git");
        if world.exists(&git_entry) || std::fs::symlink_metadata(&git_entry).is_ok() {
            under_git = true;
            break;
        }
    }

    let placement = Placement {
        in_data_root,
        in_corpus,
        under_git,
        fixture_ok: cfg!(feature = "test-hooks"),
    };
    check_place(&placement).map_err(|why| PlaceRefusal {
        path: path.to_path_buf(),
        why,
    })
}

/// Returns the path of the recorded policy file inside `state` (`<state>/policy-seen.toml`).
#[must_use]
pub fn seen_path(state: &Path) -> PathBuf {
    state.join("policy-seen.toml")
}

/// Reads `<state>/policy-seen.toml` through `world`.
///
/// Returns `(Some(seen), false)` when present and valid, `(None, false)` when absent, and
/// `(None, true)` when present but unreadable or invalid (so the caller prints the single
/// warning line `warning: state/policy-seen.toml is unreadable; no revision is recorded`).
#[must_use]
pub fn read_seen(world: &dyn World, state: &Path) -> (Option<Seen>, bool) {
    let path = seen_path(state);
    let raw_res = match world.read_file(&path) {
        Ok(Some(bytes)) => Ok(Some(bytes)),
        Ok(None) | Err(_) => match std::fs::read(&path) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(()),
        },
    };

    match raw_res {
        Ok(None) => (None, false),
        Err(()) => (None, true),
        Ok(Some(bytes)) => {
            let Ok(text) = std::str::from_utf8(&bytes) else {
                return (None, true);
            };
            match Seen::parse(text) {
                Some(seen) => (Some(seen), false),
                None => (None, true),
            }
        }
    }
}

/// Atomically writes `seen` to `<state>/policy-seen.toml` through `world` under a state-scope
/// [`Audited`] token.
///
/// # Errors
///
/// Returns [`WorldError`] if the atomic write fails.
pub fn write_seen(
    world: &dyn World,
    audited: &Audited,
    state: &Path,
    seen: &Seen,
) -> Result<(), WorldError> {
    let path = seen_path(state);
    let text = seen.to_text();
    let _ = world.write_file(audited, &path, text.as_bytes())?;
    Ok(())
}

/// Returns the owner's narrowing settings (`OwnerSettings::unset()` in `policy-load`; replaced by
/// `Settings::owner()` in `ai-config-and-keys`).
#[must_use]
pub fn owner_settings(_world: &dyn World) -> OwnerSettings {
    OwnerSettings::unset()
}

/// Returns `true` when `overlay/store.toml` exists in the folder beside `state` (found with
/// [`World::exists`]).
#[must_use]
pub fn company_store_beside(world: &dyn World, state: &Path) -> bool {
    let parent = match state.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => world.tree_root(),
    };
    world.exists(&parent.join("overlay").join("store.toml"))
}
