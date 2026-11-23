//! Recorded highest policy revision (`Seen`, `state/policy-seen.toml`), observed file facts
//! (`FileFact`, `ObservationDigest`, `FileObservation`), and pure policy state resolution
//! (`resolve`, `Resolved`) (`policy-load`, `ADR 0004 §3`, `docs/reference/policy.md#states`).

use std::fmt::Write as _;
use std::path::PathBuf;

use crate::audit::Sha256;
use crate::check::Code;

use super::floor::{FloorKind, policy_hash};
use super::parse::{MAX_POLICY_BYTES, Policy, PolicyError};
use super::state::{ClosedWhy, PolicyState};
use super::writers::WriterWhy;

/// Bounded prefix length hashed for oversized policy files (`1 MiB + 1 = 1,048,577` bytes).
pub const OVER_LIMIT_PREFIX_BYTES: usize = MAX_POLICY_BYTES + 1;

/// Persisted record in `state/policy-seen.toml` tracking the highest loaded policy revision,
/// its SHA-256 hash, and its `[audit].sink` path (`seen_schema = 1`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    /// Highest loaded policy `revision` (`>= 1`).
    pub revision: u64,
    /// Lowercase 64-character SHA-256 hex digest of the loaded policy file bytes.
    pub hash: Sha256,
    /// Recorded `[audit].sink` path from the last loaded policy, preserved across `Closed` runs
    /// so `audit export` continues to function.
    pub audit_sink: Option<PathBuf>,
}

impl Seen {
    /// Parses `state/policy-seen.toml` text (`seen_schema = 1`).
    ///
    /// Returns `None` if `text` is invalid TOML, contains an unknown key or an unknown
    /// `seen_schema`, has a `revision < 1`, or has an invalid `hash`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let table: toml::Table = toml::from_str(text).ok()?;
        for key in table.keys() {
            if !matches!(
                key.as_str(),
                "seen_schema" | "revision" | "hash" | "audit_sink"
            ) {
                return None;
            }
        }
        let schema = table.get("seen_schema")?.as_integer()?;
        if schema != 1 {
            return None;
        }
        let rev_int = table.get("revision")?.as_integer()?;
        let revision = u64::try_from(rev_int).ok().filter(|&r| r >= 1)?;
        let hash_str = table.get("hash")?.as_str()?;
        let hash = Sha256::parse(hash_str).ok()?;
        let audit_sink = match table.get("audit_sink") {
            Some(val) => {
                let s = val.as_str()?;
                if s.is_empty() {
                    return None;
                }
                Some(PathBuf::from(s))
            }
            None => None,
        };
        Some(Self {
            revision,
            hash,
            audit_sink,
        })
    }

    /// Serializes this record in deterministic fixed key order (`seen_schema = 1`).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "seen_schema = 1");
        let _ = writeln!(out, "revision = {}", self.revision);
        let _ = writeln!(out, "hash = {:?}", self.hash.as_str());
        if let Some(sink) = &self.audit_sink {
            let _ = writeln!(out, "audit_sink = {:?}", sink.to_string_lossy());
        }
        out
    }
}

/// Digest of the raw bytes observed at the policy path before UTF-8 decoding or TOML parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationDigest {
    /// Whole-file SHA-256 digest for bounded readable input (`<= 1 MiB`), including invalid UTF-8.
    Whole(Sha256),
    /// Bounded prefix SHA-256 digest for oversized input (`> 1 MiB`), over the first `1,048,577` bytes.
    Prefix {
        /// Number of prefix bytes hashed (`1_048_577`).
        bytes: u64,
        /// SHA-256 digest of the prefix bytes.
        sha256: Sha256,
    },
    /// No file existed at the policy path.
    Absent,
    /// Opening or reading the policy file failed.
    Unreadable,
}

impl ObservationDigest {
    /// Computes the [`ObservationDigest`] of raw readable bytes before UTF-8 decoding.
    ///
    /// Inputs up to [`MAX_POLICY_BYTES`] (`1,048,576`) produce [`Self::Whole`]; larger inputs
    /// produce [`Self::Prefix`] over the first [`OVER_LIMIT_PREFIX_BYTES`] (`1,048,577`) bytes.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.len() > MAX_POLICY_BYTES {
            let prefix = &bytes[..bytes.len().min(OVER_LIMIT_PREFIX_BYTES)];
            Self::Prefix {
                bytes: OVER_LIMIT_PREFIX_BYTES as u64,
                sha256: Sha256::digest(prefix),
            }
        } else {
            Self::Whole(Sha256::digest(bytes))
        }
    }

    /// Encodes the `<file-fact>` line for the `Closed` policy observation hash:
    /// `sha256:<hash>`, `prefix:1048577:sha256:<hash>`, `absent`, or `unreadable`.
    #[must_use]
    pub fn file_fact_string(&self) -> String {
        match self {
            Self::Whole(sha) => format!("sha256:{}", sha.as_str()),
            Self::Prefix { bytes, sha256 } => {
                format!("prefix:{bytes}:sha256:{}", sha256.as_str())
            }
            Self::Absent => "absent".to_owned(),
            Self::Unreadable => "unreadable".to_owned(),
        }
    }
}

/// Classification of the bytes returned by `PolicySource::read`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileFact {
    /// The policy file does not exist.
    Absent,
    /// Opening or reading the policy file failed.
    Unreadable,
    /// The policy file exceeds 1 MiB (`1,048,576` bytes).
    TooLarge,
    /// The policy file is not valid UTF-8 (`utf16 = true` when it starts with `FF FE` or `FE FF`).
    NotUtf8 {
        /// Whether the file begins with a UTF-16 LE (`FF FE`) or UTF-16 BE (`FE FF`) byte order mark.
        utf16: bool,
    },
    /// Valid UTF-8 text up to 1 MiB, with any leading UTF-8 BOM and CRLF line endings preserved.
    Text(String),
}

impl FileFact {
    /// Turns the raw bytes returned by `PolicySource::read` into a [`FileFact`].
    ///
    /// - More than 1 MiB (`1,048,576` bytes) is [`Self::TooLarge`].
    /// - A leading `FF FE` or `FE FF` is [`Self::NotUtf8`] with `utf16: true`.
    /// - Other invalid UTF-8 bytes are [`Self::NotUtf8`] with `utf16: false`.
    /// - Valid UTF-8 bytes become [`Self::Text`] unchanged (preserving UTF-8 BOM and CRLF).
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.len() > MAX_POLICY_BYTES {
            return Self::TooLarge;
        }
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Self::NotUtf8 { utf16: true };
        }
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::Text(text.to_owned()),
            Err(_) => Self::NotUtf8 { utf16: false },
        }
    }
}

/// Complete observation of the policy file: its [`FileFact`], raw byte [`ObservationDigest`],
/// and operating-system [`WriterWhy`] check result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileObservation {
    /// Decoded file fact.
    pub fact: FileFact,
    /// Raw byte observation digest computed before UTF-8 decoding.
    pub digest: ObservationDigest,
    /// Writer check result (`Ok(())` when only Administrators/System can write).
    pub writers: Result<(), WriterWhy>,
}

impl FileObservation {
    /// Builds a [`FileObservation`] from raw file bytes and the writer check result.
    #[must_use]
    pub fn from_bytes(bytes: &[u8], writers: Result<(), WriterWhy>) -> Self {
        Self {
            fact: FileFact::from_bytes(bytes),
            digest: ObservationDigest::from_bytes(bytes),
            writers,
        }
    }

    /// Builds a [`FileObservation`] when no policy file is present.
    #[must_use]
    pub const fn absent() -> Self {
        Self {
            fact: FileFact::Absent,
            digest: ObservationDigest::Absent,
            writers: Ok(()),
        }
    }

    /// Builds a [`FileObservation`] when the policy file exists but could not be opened or read.
    #[must_use]
    pub const fn unreadable(writers: Result<(), WriterWhy>) -> Self {
        Self {
            fact: FileFact::Unreadable,
            digest: ObservationDigest::Unreadable,
            writers,
        }
    }

    /// Builds a [`FileObservation`] from an already-classified [`FileFact`] and writer result.
    #[must_use]
    pub fn from_fact(fact: FileFact, writers: Result<(), WriterWhy>) -> Self {
        let digest = match &fact {
            FileFact::Absent => ObservationDigest::Absent,
            FileFact::Unreadable => ObservationDigest::Unreadable,
            FileFact::TooLarge => ObservationDigest::Prefix {
                bytes: OVER_LIMIT_PREFIX_BYTES as u64,
                sha256: Sha256::digest(&[]),
            },
            FileFact::NotUtf8 { utf16: true } => {
                ObservationDigest::Whole(Sha256::digest(&[0xFF, 0xFE]))
            }
            FileFact::NotUtf8 { utf16: false } => ObservationDigest::Whole(Sha256::digest(&[0xFF])),
            FileFact::Text(text) => ObservationDigest::Whole(Sha256::digest(text.as_bytes())),
        };
        Self {
            fact,
            digest,
            writers,
        }
    }
}

/// Pure result of resolving a [`FileObservation`] against an optional recorded [`Seen`] and
/// `has_company_store`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// Resulting runtime policy state (`Floor`, `Loaded`, or `Closed`).
    pub state: PolicyState,
    /// Journal `policy.change` outcome word (`Some("loaded")`, `Some("absent")`, `Some("invalid")`,
    /// or `None` when remaining on the Floor).
    pub outcome: Option<&'static str>,
    /// The [`Seen`] record to persist (`new` only for `Loaded`; unchanged previous `Seen` for `Closed`).
    pub seen: Option<Seen>,
    /// Short explanation note for non-parse failures (`Unreadable`, `TooLarge`, `NotUtf8`, `Absent`, `Writable`).
    pub note: Option<&'static str>,
    /// Schema-1 parse failure when `Policy::parse` rejected the file (`None` when `note` explains the failure).
    pub finding: Option<PolicyError>,
    /// The original [`FileObservation`], carried unchanged.
    pub observation: FileObservation,
}

impl Resolved {
    /// Computes the observation hash for `policy.change`, `policy show`, and `policy_line`:
    ///
    /// - `Floor`: the floor hash (`Sha256` of [`policy_hash`]).
    /// - `Loaded`: the SHA-256 of the policy file bytes.
    /// - `Closed`: SHA-256 of `closed\n<reason>\n<file-fact>\n<writer-fact>`.
    #[must_use]
    pub fn observation_hash(&self) -> Sha256 {
        match &self.state {
            PolicyState::Floor(_) => {
                let hex = policy_hash(&self.state);
                Sha256::parse(&hex).unwrap_or_else(|_| Sha256::digest(hex.as_bytes()))
            }
            PolicyState::Loaded(policy) => Sha256::parse(policy.hash())
                .unwrap_or_else(|_| Sha256::digest(policy.hash().as_bytes())),
            PolicyState::Closed(why) => closed_observation_hash(*why, &self.observation),
        }
    }

    /// Returns the diagnostic [`Code`] when in [`PolicyState::Closed`] (`FB0418` for `Writable`,
    /// `FB0413` otherwise).
    #[must_use]
    pub const fn closed_code(&self) -> Option<Code> {
        match self.state {
            PolicyState::Closed(ClosedWhy::Writable) => Some(Code::Fb0418),
            PolicyState::Closed(_) => Some(Code::Fb0413),
            PolicyState::Floor(_) | PolicyState::Loaded(_) => None,
        }
    }

    /// Returns the `<why>` explanation string when in [`PolicyState::Closed`]:
    /// `self.note` when present, otherwise `<rule> at <key_path>` from `self.finding`.
    #[must_use]
    pub fn closed_why_text(&self) -> Option<String> {
        let PolicyState::Closed(why) = &self.state else {
            return None;
        };
        if let Some(note) = self.note {
            return Some(note.to_owned());
        }
        if let Some(err) = &self.finding {
            return Some(match err.label_id() {
                Some(id) => format!(
                    "{} at {} (label {})",
                    err.rule(),
                    err.key_path(),
                    id.as_str()
                ),
                None => format!("{} at {}", err.rule(), err.key_path()),
            });
        }
        Some(
            match why {
                ClosedWhy::Absent => "the policy file is missing",
                ClosedWhy::Invalid => "invalid policy file",
                ClosedWhy::LowerRevision => "revision is lower than recorded revision at revision",
                ClosedWhy::Writable => "the policy file is writable by a non-administrator",
            }
            .to_owned(),
        )
    }
}

/// Computes the deterministic SHA-256 digest of `closed\n<reason>\n<file-fact>\n<writer-fact>`.
#[must_use]
pub fn closed_observation_hash(why: ClosedWhy, observation: &FileObservation) -> Sha256 {
    let reason = match why {
        ClosedWhy::Absent => "absent",
        ClosedWhy::Invalid => "invalid",
        ClosedWhy::LowerRevision => "lower_revision",
        ClosedWhy::Writable => "writable",
    };
    let file_fact = observation.digest.file_fact_string();
    let writer_fact = match &observation.writers {
        Ok(()) => "ok",
        Err(w) => w.word(),
    };
    let encoded = format!("closed\n{reason}\n{file_fact}\n{writer_fact}");
    Sha256::digest(encoded.as_bytes())
}

/// Resolves a [`FileObservation`] against an optional recorded [`Seen`] and `has_company_store`.
#[must_use]
pub fn resolve(file: FileObservation, seen: Option<&Seen>, has_company_store: bool) -> Resolved {
    // 1. Absent file: Floor when no previous record exists; Closed(Absent) when a record exists.
    if matches!(file.fact, FileFact::Absent) {
        return match seen {
            None => Resolved {
                state: PolicyState::Floor(FloorKind::for_machine(has_company_store)),
                outcome: None,
                seen: None,
                note: None,
                finding: None,
                observation: file,
            },
            Some(prev) => Resolved {
                state: PolicyState::Closed(ClosedWhy::Absent),
                outcome: Some("absent"),
                seen: Some(prev.clone()),
                note: Some("the policy file is missing"),
                finding: None,
                observation: file,
            },
        };
    }

    // 2. A present file that fails the writer check closes with Closed(Writable) (FB0418).
    if let Err(writer_why) = file.writers {
        return Resolved {
            state: PolicyState::Closed(ClosedWhy::Writable),
            outcome: Some("invalid"),
            seen: seen.cloned(),
            note: Some(writer_why.as_str()),
            finding: None,
            observation: file,
        };
    }

    // 3. Inspect the file fact.
    match &file.fact {
        FileFact::Absent => unreachable!("handled above"),
        FileFact::Unreadable => Resolved {
            state: PolicyState::Closed(ClosedWhy::Invalid),
            outcome: Some("invalid"),
            seen: seen.cloned(),
            note: Some("the file could not be read"),
            finding: None,
            observation: file,
        },
        FileFact::TooLarge => Resolved {
            state: PolicyState::Closed(ClosedWhy::Invalid),
            outcome: Some("invalid"),
            seen: seen.cloned(),
            note: Some("the file is larger than 1 MiB"),
            finding: None,
            observation: file,
        },
        FileFact::NotUtf8 { utf16: true } => Resolved {
            state: PolicyState::Closed(ClosedWhy::Invalid),
            outcome: Some("invalid"),
            seen: seen.cloned(),
            note: Some("the file is UTF-16; save it as UTF-8"),
            finding: None,
            observation: file,
        },
        FileFact::NotUtf8 { utf16: false } => Resolved {
            state: PolicyState::Closed(ClosedWhy::Invalid),
            outcome: Some("invalid"),
            seen: seen.cloned(),
            note: Some("the file is not valid UTF-8"),
            finding: None,
            observation: file,
        },
        FileFact::Text(text) => match Policy::parse(text, seen.map(|s| s.revision)) {
            Ok(policy) => {
                let hash = Sha256::parse(policy.hash())
                    .unwrap_or_else(|_| Sha256::digest(text.as_bytes()));
                let audit_sink = policy.audit().sink.as_ref().map(PathBuf::from);
                let new_seen = Seen {
                    revision: policy.revision(),
                    hash,
                    audit_sink,
                };
                Resolved {
                    state: PolicyState::Loaded(policy),
                    outcome: Some("loaded"),
                    seen: Some(new_seen),
                    note: None,
                    finding: None,
                    observation: file,
                }
            }
            Err(err) => {
                let why = if err.key_path() == "revision"
                    && err.rule() == "revision is lower than recorded revision"
                {
                    ClosedWhy::LowerRevision
                } else {
                    ClosedWhy::Invalid
                };
                Resolved {
                    state: PolicyState::Closed(why),
                    outcome: Some("invalid"),
                    seen: seen.cloned(),
                    note: None,
                    finding: Some(err),
                    observation: file,
                }
            }
        },
    }
}
