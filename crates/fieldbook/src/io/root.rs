//! Canonical input-path hashing (`input_path_sha256`) and Git work-tree / ignored Home State
//! context resolution (`git_context`) (`add-new-and-edition`).

#![expect(
    clippy::disallowed_methods,
    reason = "io::root canonicalizes input paths and inspects .git and .gitignore on disk"
)]

use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf};

use fieldbook_core::audit::Sha256;
use fieldbook_core::hash::Sha256Stream;
use fieldbook_core::labels::{GitContext, fieldbook_is_ignored};

use crate::World;
use crate::io::Error;

const INPUT_PATH_DOMAIN: &[u8] = b"input-path 1\n";

/// Compute the lossless canonical input-path digest (`input-path 1\n`, length-framed OS tag
/// `unix` or `windows`, then length-framed raw path bytes: Unix `OsStr` bytes or Windows
/// UTF-16LE code units).
///
/// # Errors
///
/// Returns [`Error`] if `path` does not exist or cannot be canonicalized.
pub fn input_path_sha256(path: &Path) -> Result<Sha256, Error> {
    let canonical = std::fs::canonicalize(path).map_err(|err| Error::new(path, err.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        Ok(hash_encoded_input_path(
            b"unix",
            canonical.as_os_str().as_bytes(),
        ))
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        let units: Vec<u16> = canonical.as_os_str().encode_wide().collect();
        let bytes = encode_utf16le_units(&units);
        Ok(hash_encoded_input_path(b"windows", &bytes))
    }
    #[cfg(not(any(unix, windows)))]
    {
        Ok(hash_encoded_input_path(
            b"unix",
            canonical.as_os_str().to_string_lossy().as_bytes(),
        ))
    }
}

#[must_use]
fn hash_encoded_input_path(os_tag: &[u8], path_bytes: &[u8]) -> Sha256 {
    let mut hasher = Sha256Stream::new();
    hasher.update(INPUT_PATH_DOMAIN);
    update_framed(&mut hasher, os_tag);
    update_framed(&mut hasher, path_bytes);
    let hex = hasher.finish();
    Sha256::parse(&hex).unwrap_or_else(|_| unreachable!())
}

fn update_framed(hasher: &mut Sha256Stream, bytes: &[u8]) {
    let prefix = format!("{}:", bytes.len());
    hasher.update(prefix.as_bytes());
    hasher.update(bytes);
}

#[cfg(any(windows, test))]
#[must_use]
fn encode_utf16le_units(units: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(units.len() * 2);
    for &u in units {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

/// Resolve canonical existing ancestors and remaining components of `target` (relative to `base`
/// when `target` is relative).
fn resolve_canonical_with_tail(base: &Path, target: &Path) -> Result<PathBuf, Error> {
    let joined = if target.is_absolute() {
        target.to_path_buf()
    } else {
        base.join(target)
    };

    let mut cur = joined.as_path();
    let mut tail: Vec<OsString> = Vec::new();

    loop {
        match std::fs::canonicalize(cur) {
            Ok(mut resolved) => {
                for part in tail.into_iter().rev() {
                    if part == OsStr::new(".") {
                        continue;
                    } else if part == OsStr::new("..") {
                        let _ = resolved.pop();
                    } else {
                        resolved.push(part);
                    }
                }
                return Ok(resolved);
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                if let Some(name) = cur.file_name() {
                    tail.push(name.to_os_string());
                    if let Some(parent) = cur.parent() {
                        cur = parent;
                        continue;
                    }
                }
                return Ok(normalize_lexical(&joined));
            }
            Err(err) => return Err(Error::new(target, err.to_string())),
        }
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
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

/// Resolve the [`GitContext`] of `destination` against `world`.
///
/// - Resolves the canonical target and its existing ancestors.
/// - Sets `under_git = true` if any ancestor of the resolved target contains `.git`.
/// - Sets `ignored_home_state = true` only when `under_git` is `true`, `world.has_company_store()`
///   is `false` (Home preset), the resolved state root is `<canonical_tree_root>/.fieldbook`,
///   `<canonical_tree_root>/.gitignore` passes [`fieldbook_is_ignored`], and the resolved target
///   is contained on whole path components inside that resolved state root (including `.fieldbook/drill`).
///
/// # Errors
///
/// Returns [`Error`] if canonicalizing an existing ancestor or reading an existing `.gitignore`
/// fails with an I/O error.
pub fn git_context(world: &dyn World, destination: &Path) -> Result<GitContext, Error> {
    let resolved_dest = resolve_canonical_with_tail(world.cwd(), destination)?;
    let under_git = resolved_dest
        .ancestors()
        .any(|anc| std::fs::symlink_metadata(anc.join(".git")).is_ok());

    if !under_git || world.has_company_store() {
        return Ok(GitContext {
            under_git,
            ignored_home_state: false,
        });
    }

    let canonical_tree_root = resolve_canonical_with_tail(world.cwd(), world.tree_root())?;
    let canonical_state_dir = resolve_canonical_with_tail(world.cwd(), world.state_dir())?;
    let expected_home_state = canonical_tree_root.join(".fieldbook");

    if canonical_state_dir != expected_home_state {
        return Ok(GitContext {
            under_git: true,
            ignored_home_state: false,
        });
    }

    if !resolved_dest.starts_with(&canonical_state_dir) {
        return Ok(GitContext {
            under_git: true,
            ignored_home_state: false,
        });
    }

    let gitignore_path = canonical_tree_root.join(".gitignore");
    let ignore_text = match std::fs::read_to_string(&gitignore_path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GitContext {
                under_git: true,
                ignored_home_state: false,
            });
        }
        Err(err) => return Err(Error::new(&gitignore_path, err.to_string())),
    };

    let ignored_home_state = fieldbook_is_ignored(&ignore_text);
    Ok(GitContext {
        under_git: true,
        ignored_home_state,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fieldbook_core::CivilDate;
    use fieldbook_core::audit::{Now, Principal, Via};
    use fieldbook_core::check::Tree;

    use crate::StoreLease;
    use crate::io::{Audited, atomic};

    struct TestDiskWorld {
        root: PathBuf,
        state_dir: PathBuf,
        company_store: bool,
    }

    impl TestDiskWorld {
        fn new(root: &Path) -> Self {
            Self {
                root: root.to_path_buf(),
                state_dir: root.join(".fieldbook"),
                company_store: false,
            }
        }
    }

    impl World for TestDiskWorld {
        fn read_tree(&self) -> Result<Tree, Error> {
            Ok(Tree::new())
        }

        fn write_lease(&self) -> Result<StoreLease, Error> {
            crate::io::journal::acquire_write_lease(&self.root, &self.state_dir)
        }

        fn read_tree_under(&self, _lease: &StoreLease) -> Result<Tree, Error> {
            Ok(Tree::new())
        }

        fn write_file(
            &self,
            audited: &Audited,
            path: &Path,
            bytes: &[u8],
        ) -> Result<Sha256, Error> {
            atomic::write_file(audited, path, bytes)
        }

        fn has_company_store(&self) -> bool {
            self.company_store
        }

        fn today(&self) -> CivilDate {
            CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("{e}"))
        }

        fn now(&self) -> Now {
            Now::new(self.today(), 43_200).unwrap_or_else(|e| panic!("{e}"))
        }

        fn principal(&self) -> Principal {
            Principal::local()
        }

        fn cwd(&self) -> &Path {
            &self.root
        }

        fn tree_root(&self) -> &Path {
            &self.root
        }

        fn state_dir(&self) -> &Path {
            &self.state_dir
        }
    }

    #[test]
    fn lossless_input_path_encoding_preserves_non_utf8_and_surrogates() {
        let unix_a = hash_encoded_input_path(b"unix", b"/tmp/a\xff\xfe.txt");
        let unix_b = hash_encoded_input_path(b"unix", b"/tmp/a\xff\xfd.txt");
        assert_ne!(
            unix_a, unix_b,
            "distinct non-UTF-8 Unix byte sequences must not collide"
        );

        let win_a = encode_utf16le_units(&[0x0043, 0x003a, 0x005c, 0xd800, 0x0061]);
        let win_b = encode_utf16le_units(&[0x0043, 0x003a, 0x005c, 0xd801, 0x0061]);
        let hash_win_a = hash_encoded_input_path(b"windows", &win_a);
        let hash_win_b = hash_encoded_input_path(b"windows", &win_b);
        assert_ne!(
            hash_win_a, hash_win_b,
            "distinct unpaired UTF-16LE surrogates must not collide"
        );
        assert_ne!(
            hash_encoded_input_path(b"unix", &win_a),
            hash_encoded_input_path(b"windows", &win_a),
            "OS tag must separate unix and windows digests"
        );
    }

    #[test]
    fn input_path_sha256_canonicalizes_existing_file() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let world = TestDiskWorld::new(tmp.path());
        let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
        let journal = crate::io::state::open(&world, world.state_dir(), Via::Cli)
            .unwrap_or_else(|e| panic!("{e}"));
        let audited = journal
            .state_scope(&lease)
            .unwrap_or_else(|e| panic!("{e}"));

        let file_path = tmp.path().join("sub").join("note.txt");
        world
            .write_file(&audited, &file_path, b"hello\n")
            .unwrap_or_else(|e| panic!("{e}"));

        let dot_path = tmp.path().join("sub").join(".").join("note.txt");
        let d1 = input_path_sha256(&file_path).unwrap_or_else(|e| panic!("{e}"));
        let d2 = input_path_sha256(&dot_path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(d1, d2);
    }

    #[test]
    fn git_context_checks_containment_prefix_ignore_and_sibling() {
        let tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let mut world = TestDiskWorld::new(tmp.path());
        let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
        let journal = crate::io::state::open(&world, world.state_dir(), Via::Cli)
            .unwrap_or_else(|e| panic!("{e}"));
        let audited = journal
            .state_scope(&lease)
            .unwrap_or_else(|e| panic!("{e}"));

        // Outside Git initially.
        let ctx_outside = git_context(&world, &tmp.path().join(".fieldbook/reports/last-add.json"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_outside, GitContext::outside());

        // Create .git marker file.
        world
            .write_file(
                &audited,
                &tmp.path().join(".git/HEAD"),
                b"ref: refs/heads/main\n",
            )
            .unwrap_or_else(|e| panic!("{e}"));

        // Under Git, but .gitignore is missing -> unignored State.
        let ctx_unignored =
            git_context(&world, &tmp.path().join(".fieldbook/reports/last-add.json"))
                .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_unignored, GitContext::in_git());

        // Write .gitignore ignoring .fieldbook/.
        world
            .write_file(&audited, &tmp.path().join(".gitignore"), b".fieldbook/\n")
            .unwrap_or_else(|e| panic!("{e}"));

        // Canonical containment inside .fieldbook and .fieldbook/drill.
        let ctx_state = git_context(&world, &tmp.path().join(".fieldbook/reports/last-add.json"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_state, GitContext::ignored_home_state());

        let ctx_drill = git_context(&world, &tmp.path().join(".fieldbook/drill/corpus"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_drill, GitContext::ignored_home_state());

        // False prefix match (.fieldbook-other) must not be treated as ignored_home_state.
        let ctx_prefix = git_context(&world, &tmp.path().join(".fieldbook-other/file.txt"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_prefix, GitContext::in_git());

        // Sibling target outside .fieldbook.
        let ctx_sibling = git_context(&world, &tmp.path().join("corpus/pages/fnd/a.md"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_sibling, GitContext::in_git());

        // Laptop / company store never grants ignored_home_state.
        world.company_store = true;
        let ctx_laptop = git_context(&world, &tmp.path().join(".fieldbook/reports/last-add.json"))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_laptop, GitContext::in_git());

        // Custom state folder outside <tree_root>/.fieldbook never grants ignored_home_state.
        world.company_store = false;
        world.state_dir = tmp.path().join("custom-state");
        let ctx_custom = git_context(
            &world,
            &tmp.path().join("custom-state/reports/last-add.json"),
        )
        .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(ctx_custom, GitContext::in_git());
    }
}
