//! Corpus root discovery, in-memory [`Tree`] loader and [`Disk`] implementation of [`crate::World`].

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "io::tree reads the Corpus tree, working directory and clock for Disk"
)]

use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};

use fieldbook_core::CivilDate;
use fieldbook_core::audit::{Now, Principal};
use fieldbook_core::check::Tree;
use fieldbook_core::egress::Allowed;
use fieldbook_core::hash::Sha256Stream;

use crate::World;
use crate::io::Audited;
use crate::io::root::Roots;
use crate::io::store_lease::StoreLease;

/// An I/O error raised while reading or writing the Corpus tree or working directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    /// An underlying filesystem read or write error.
    Io {
        /// The filesystem path whose operation failed.
        path: PathBuf,
        /// Human-readable error description.
        message: String,
    },
    /// A target file is held open in another program during an atomic rename.
    InUse(PathBuf),
    /// A target path is outside the folder(s) the write operation is permitted to touch.
    OutOfScope(PathBuf),
}

/// Alias for [`WorldError`].
pub type Error = WorldError;

impl WorldError {
    /// Construct a [`WorldError::Io`] for `path` and `message`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::Io {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Construct a [`WorldError::Io`] for `path` and `message`.
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self::new(path, message)
    }

    /// Construct a [`WorldError::InUse`] for `path`.
    #[must_use]
    pub fn in_use(path: impl Into<PathBuf>) -> Self {
        Self::InUse(path.into())
    }

    /// Return the path associated with this error.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::Io { path, .. } | Self::InUse(path) | Self::OutOfScope(path) => path.as_path(),
        }
    }
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } => write!(f, "{}: {message}", path.display()),
            Self::InUse(path) => write!(
                f,
                "{} is open in another program; close it and run the command again",
                path.display()
            ),
            Self::OutOfScope(path) => write!(
                f,
                "refused: {} is outside the folder this write may touch",
                path.display()
            ),
        }
    }
}

impl std::error::Error for WorldError {}

/// Read all bytes of `path` from disk, rejecting non-regular files.
pub fn read_file(path: &Path) -> Result<Vec<u8>, WorldError> {
    let meta = std::fs::metadata(path).map_err(|err| WorldError::new(path, err.to_string()))?;
    if !meta.is_file() {
        return Err(WorldError::new(path, "not a regular file"));
    }
    std::fs::read(path).map_err(|err| WorldError::new(path, err.to_string()))
}

/// Walk up from `start` to the nearest ancestor containing a `corpus/` directory, even if
/// `corpus/corpus.toml` is missing; return `start` itself if no ancestor contains `corpus/`.
#[must_use]
pub fn find_root(start: &Path) -> PathBuf {
    for dir in start.ancestors() {
        if dir.join("corpus").is_dir() {
            return dir.to_path_buf();
        }
    }
    start.to_path_buf()
}

/// Read every directory (including empty ones) and every file under `<root>/corpus/` into a
/// [`Tree`], skipping `.git` and streaming files under `corpus/sources/public/` into
/// `Entry::Original` hashes via [`Sha256Stream`].
///
/// If `<root>/corpus` does not exist, returns an empty [`Tree`].
pub fn read_tree(root: &Path) -> Result<Tree, WorldError> {
    let corpus_dir = root.join("corpus");
    let meta = match std::fs::symlink_metadata(&corpus_dir) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Tree::new()),
        Err(err) => return Err(WorldError::new(&corpus_dir, err.to_string())),
    };
    if !meta.is_dir() {
        return Err(WorldError::new(&corpus_dir, "not a directory"));
    }

    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    read_dir_recursive(&corpus_dir, "corpus", &mut tree)?;
    Ok(tree)
}

fn read_dir_recursive(dir: &Path, rel_prefix: &str, tree: &mut Tree) -> Result<(), WorldError> {
    let entries_iter =
        std::fs::read_dir(dir).map_err(|err| WorldError::new(dir, err.to_string()))?;

    let mut entries = Vec::new();
    for item in entries_iter {
        let entry = item.map_err(|err| WorldError::new(dir, err.to_string()))?;
        entries.push(entry);
    }
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let rel_path = format!("{rel_prefix}/{name}");
        let file_type = entry
            .file_type()
            .map_err(|err| WorldError::new(&path, err.to_string()))?;

        if file_type.is_dir() {
            tree.insert_dir(&rel_path);
            read_dir_recursive(&path, &rel_path, tree)?;
        } else if file_type.is_file() {
            if rel_path.starts_with("corpus/sources/public/") {
                let (sha256, len) = hash_original_file(&path)?;
                tree.insert_original(rel_path, sha256, len);
            } else {
                let bytes =
                    std::fs::read(&path).map_err(|err| WorldError::new(&path, err.to_string()))?;
                tree.insert_bytes(rel_path, bytes);
            }
        }
    }
    Ok(())
}

fn hash_original_file(path: &Path) -> Result<(String, u64), WorldError> {
    let mut file =
        std::fs::File::open(path).map_err(|err| WorldError::new(path, err.to_string()))?;
    let mut hasher = Sha256Stream::new();
    let mut buf = [0u8; 8192];
    let mut total_len = 0u64;
    loop {
        let count = file
            .read(&mut buf)
            .map_err(|err| WorldError::new(path, err.to_string()))?;
        if count == 0 {
            break;
        }
        hasher.update(&buf[..count]);
        total_len = total_len.saturating_add(count as u64);
    }
    Ok((hasher.finish(), total_len))
}

/// Real filesystem and system-clock implementation of [`World`].
#[derive(Debug, Clone)]
pub struct Disk {
    cwd: PathBuf,
    tree_root: PathBuf,
    state_dir: PathBuf,
    roots: Roots,
}

impl Disk {
    /// Create a [`Disk`] holding bounded [`Roots`].
    #[must_use]
    pub fn new(roots: Roots) -> Self {
        let cwd = roots.corpus.path().to_path_buf();
        let tree_root = find_root(&cwd);
        let state_dir = roots.state.path().to_path_buf();
        Self {
            cwd,
            tree_root,
            state_dir,
            roots,
        }
    }

    /// Create a [`Disk`] rooted at `cwd` with an explicit `state_dir`.
    #[must_use]
    pub fn with_state(cwd: impl Into<PathBuf>, state_dir: impl Into<PathBuf>) -> Self {
        let cwd = cwd.into();
        let tree_root = find_root(&cwd);
        let state_dir = state_dir.into();
        let roots = Roots::at_home(&tree_root, &state_dir);
        Self {
            cwd,
            tree_root,
            state_dir,
            roots,
        }
    }

    /// Capture the process working directory (and `FIELDBOOK_STATE` if set) once and construct a [`Disk`].
    pub fn current() -> Result<Self, WorldError> {
        let cwd = std::env::current_dir()
            .map_err(|err| WorldError::new(PathBuf::from("."), err.to_string()))?;
        let tree_root = find_root(&cwd);
        let env_state = std::env::var_os("FIELDBOOK_STATE").filter(|v| !v.is_empty());
        let state_dir =
            crate::io::state::resolve(&tree_root, &cwd, env_state.as_deref().map(Path::new));
        let roots = Roots::at_home(&tree_root, &state_dir);
        Ok(Self {
            cwd,
            tree_root,
            state_dir,
            roots,
        })
    }

    /// Return the bounded [`Roots`] held by this [`Disk`].
    #[must_use]
    pub const fn roots(&self) -> &Roots {
        &self.roots
    }

    /// Return the state directory bound to this [`Disk`].
    #[must_use]
    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    /// Return facts about public support provenance: retained verified catalogues and completed public writes.
    #[must_use]
    pub fn support_facts(&self) -> fieldbook_core::manifest::SupportFacts {
        use crate::io::journal::read_valid_events_prefix;
        use fieldbook_core::audit::kind::{BackupVerified, EventPayload};
        use fieldbook_core::audit::{CorpusPath, Sha256, StoreId};
        use fieldbook_core::manifest::{
            SupportFacts, is_lower_hex, verify_support_catalogue_bytes,
        };
        use fieldbook_core::schema::PageType;
        use std::collections::{BTreeMap, BTreeSet};

        let events = read_valid_events_prefix(&self.state_dir);

        // 1. Collect verified catalogue SHAs from successful backup.verify raw events
        let mut verified_catalogue_shas = BTreeSet::new();
        for event in &events {
            if let EventPayload::BackupVerify {
                outcome,
                verified:
                    BackupVerified::Raw {
                        public_support_sha256: Some(sha),
                        ..
                    },
            } = &event.payload
                && outcome.as_str() == "ok"
            {
                let hex = sha.as_str().to_ascii_lowercase();
                if is_lower_hex(&hex, 64) {
                    verified_catalogue_shas.insert(hex);
                }
            }
        }

        // 2. Load and verify retained catalogues
        let mut catalogues = Vec::new();
        let pub_support_dir = self.state_dir.join("public-support");
        for sha in &verified_catalogue_shas {
            let cat_path = pub_support_dir.join(format!("{sha}.toml"));
            if cat_path.is_file()
                && let Ok(bytes) = std::fs::read(&cat_path)
                && let Ok(cat) = verify_support_catalogue_bytes(&bytes, sha)
            {
                catalogues.push((cat, sha.clone()));
            }
        }

        // 3. Collect candidate support paths
        let mut candidate_paths: BTreeSet<String> = BTreeSet::new();

        // 3a. Known root support files and regression/rules
        for p in &[
            "AGENTS.md",
            fieldbook_core::strings::paths()[6],
            "README.md",
            "corpus.toml",
            "changes.md",
            "scan-allow.toml",
            "eval/regression.toml",
            "pipeline/rules.toml",
        ] {
            candidate_paths.insert((*p).to_owned());
            candidate_paths.insert(format!("corpus/{p}"));
        }

        // 3b. Registered page templates
        for ty in PageType::ALL {
            let p = format!("templates/{}.md", ty.as_str());
            candidate_paths.insert(p.clone());
            candidate_paths.insert(format!("corpus/{p}"));
        }

        // 3c. Paths from retained verified catalogues
        for (cat, _) in &catalogues {
            for entry in &cat.files {
                candidate_paths.insert(entry.path.clone());
                candidate_paths.insert(format!("corpus/{}", entry.path));
            }
        }

        // 3d. Scan tree on disk for any present files
        collect_disk_support_paths(&self.tree_root, &mut candidate_paths);

        // Precompute path_sha256 for all candidate paths
        let mut path_sha_to_candidates: BTreeMap<Sha256, Vec<String>> = BTreeMap::new();
        for cand in candidate_paths {
            if let Ok(canon) = CorpusPath::parse(&cand) {
                let p_sha = Sha256::digest(canon.as_str().as_bytes());
                path_sha_to_candidates.entry(p_sha).or_default().push(cand);
            }
        }

        // 4. Trace completed public writes from journal events
        let mut public_writes: BTreeMap<String, String> = BTreeMap::new();
        let mut open_begin = None;

        for event in &events {
            if event.envelope.store != StoreId::Corpus {
                continue;
            }
            match &event.payload {
                EventPayload::WriteBegin {
                    verb,
                    labels,
                    file_hashes: Some(hashes),
                    affected,
                    dest: None,
                    ..
                } => {
                    open_begin = Some((verb, labels, affected.as_slice(), hashes.as_slice()));
                }
                EventPayload::WriteEnd {
                    verb,
                    labels,
                    affected,
                    outcome,
                    dest: None,
                    ..
                } => {
                    if let Some((begin_verb, _begin_labels, begin_affected, hashes)) =
                        open_begin.take()
                        && begin_verb == verb
                        && begin_affected == affected.as_slice()
                        && outcome.as_str() == "ok"
                        && labels.is_public()
                    {
                        for entry in hashes {
                            let sha_hex = entry.sha256.as_str().to_ascii_lowercase();
                            public_writes.insert(entry.path_sha256.to_string(), sha_hex.clone());
                            if let Some(candidates) = path_sha_to_candidates.get(&entry.path_sha256)
                            {
                                for cand in candidates {
                                    let clean = cand.strip_prefix("corpus/").unwrap_or(cand);
                                    public_writes.insert(clean.to_owned(), sha_hex.clone());
                                    public_writes
                                        .insert(format!("corpus/{clean}"), sha_hex.clone());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        SupportFacts::new(catalogues, public_writes)
    }
}

fn collect_disk_support_paths(tree_root: &Path, paths: &mut std::collections::BTreeSet<String>) {
    let mut check_dir = |dir: &Path, prefix: &str| {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type()
                    && ft.is_file()
                {
                    let name = entry.file_name();
                    let name_str = name.to_string_lossy();
                    let rel = if prefix.is_empty() {
                        name_str.to_string()
                    } else {
                        format!("{prefix}/{name_str}")
                    };
                    if fieldbook_core::manifest::classify_support_path(&rel).is_ok() {
                        paths.insert(rel.clone());
                        paths.insert(format!("corpus/{rel}"));
                    }
                }
            }
        }
    };

    check_dir(tree_root, "");
    check_dir(&tree_root.join("corpus"), "");
    check_dir(&tree_root.join("templates"), "templates");
    check_dir(&tree_root.join("corpus/templates"), "templates");
    check_dir(
        &tree_root.join("templates/workbooks"),
        "templates/workbooks",
    );
    check_dir(
        &tree_root.join("corpus/templates/workbooks"),
        "templates/workbooks",
    );
    check_dir(&tree_root.join("eval"), "eval");
    check_dir(&tree_root.join("corpus/eval"), "eval");
    check_dir(&tree_root.join("pipeline"), "pipeline");
    check_dir(&tree_root.join("corpus/pipeline"), "pipeline");
}

impl World for Disk {
    fn read_tree(&self) -> Result<Tree, WorldError> {
        crate::io::journal::read_tree_coherent(&self.tree_root, &self.state_dir)
    }

    fn read_file(&self, path: &Path) -> Result<Option<Vec<u8>>, WorldError> {
        match std::fs::symlink_metadata(path) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(WorldError::new(path, err.to_string())),
            Ok(meta) => {
                if !meta.is_file() {
                    return Err(WorldError::new(path, "not a regular file"));
                }
                std::fs::read(path)
                    .map(Some)
                    .map_err(|err| WorldError::new(path, err.to_string()))
            }
        }
    }

    fn write_lease(&self) -> Result<StoreLease, WorldError> {
        crate::io::journal::acquire_write_lease(&self.tree_root, &self.state_dir)
    }

    fn read_tree_under(&self, lease: &StoreLease) -> Result<Tree, WorldError> {
        crate::io::journal::read_tree_under_lease(&self.tree_root, &self.state_dir, lease)
    }

    fn write_file(
        &self,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<fieldbook_core::audit::Sha256, WorldError> {
        Disk::write_file(self, audited, path, bytes)
    }

    fn write_outside(
        &self,
        allowed: &Allowed,
        audited: &Audited,
        path: &Path,
        bytes: &[u8],
    ) -> Result<fieldbook_core::audit::Sha256, WorldError> {
        Disk::write_outside(self, allowed, audited, path, bytes)
    }

    fn has_company_store(&self) -> bool {
        Disk::has_company_store(self)
    }

    fn stdin_is_terminal(&self) -> bool {
        Disk::stdin_is_terminal(self)
    }

    fn read_line(&self) -> String {
        Disk::read_line(self)
    }

    fn prompt(&self, flag: &str) -> Result<(), WorldError> {
        Disk::prompt(self, flag)
    }

    fn today(&self) -> CivilDate {
        self.now().date
    }

    fn now(&self) -> Now {
        let secs = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            Ok(dur) => dur.as_secs(),
            Err(_) => 0,
        };
        Now::from_unix_secs(secs)
            .unwrap_or_else(|_| Now::new(CivilDate::MIN, 0).unwrap_or_else(|_| unreachable!()))
    }

    fn principal(&self) -> Principal {
        crate::io::state::os_principal()
    }

    fn cwd(&self) -> &Path {
        &self.cwd
    }

    fn tree_root(&self) -> &Path {
        &self.tree_root
    }

    fn state_dir(&self) -> &Path {
        &self.state_dir
    }

    fn place(&self, path: &Path) -> fieldbook_core::labels::PlaceFacts {
        Disk::place(self, path)
    }

    fn support_facts(&self) -> fieldbook_core::manifest::SupportFacts {
        Disk::support_facts(self)
    }

    fn reset_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        Disk::reset_drill(self, audited)
    }

    fn remove_drill(&self, audited: &Audited) -> Result<(), WorldError> {
        Disk::remove_drill(self, audited)
    }

    fn read_maintenance(&self) -> Result<String, WorldError> {
        Disk::read_maintenance(self)
    }

    fn read_state_summary(
        &self,
        today: fieldbook_core::CivilDate,
    ) -> Result<fieldbook_core::status::StateSummary, WorldError> {
        Disk::read_state_summary(self, today)
    }

    /// Return the `folder@1` Store adapter id: this world writes through a folder store.
    fn store_id(&self) -> &'static str {
        "folder@1"
    }
}
