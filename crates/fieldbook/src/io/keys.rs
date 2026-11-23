//! Environment key access (`Key`, `KeyError`, `KeySource`, `EnvKeys`, `ScriptedKeys`)
//! (`ai-config-and-keys` step 2, control NS-07).
//!
//! `EnvKeys` is the single place in the program that calls `std::env::var`.
//! `Key` has no `Display`, no `Clone`, and no `Serialize`, and its `Debug` prints `Key(<redacted>)`.

use std::collections::BTreeMap;
use std::fmt;

/// Secret API key read from an environment variable.
///
/// Intentionally implements neither `Display`, `Clone`, nor `Serialize`. Its `Debug`
/// implementation prints `Key(<redacted>)` so structs holding a `Key` can still derive `Debug`.
pub struct Key(String);

impl Key {
    /// Returns the byte length of the key without exposing any character of its value.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns `true` when the key has zero bytes (never true for a key returned by [`KeySource`]).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns the raw key text for the HTTP authentication header builder.
    ///
    /// Enforced by `reveal_is_called_only_in_io_net` (NS-07): only code under `io/net*`
    /// may call this method.
    #[must_use]
    pub fn reveal(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(<redacted>)")
    }
}

/// Failure when reading a named key from a [`KeySource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    /// The environment variable is not set.
    Unset,
    /// The environment variable is set to the empty string.
    Empty,
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unset => f.write_str("environment variable is not set"),
            Self::Empty => f.write_str("environment variable is empty"),
        }
    }
}

impl std::error::Error for KeyError {}

/// Validates an environment variable name for `key_env`: `[A-Z][A-Z0-9_]{2,63}`.
#[must_use]
pub fn is_valid_key_env_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    if !(3..=64).contains(&bytes.len()) || !bytes[0].is_ascii_uppercase() {
        return false;
    }
    bytes[1..]
        .iter()
        .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

/// Port for reading API keys by environment variable name.
pub trait KeySource {
    /// Look up `name` and return a non-empty [`Key`], or [`KeyError::Unset`] / [`KeyError::Empty`].
    ///
    /// # Errors
    ///
    /// Returns [`KeyError::Unset`] if `name` is absent or [`KeyError::Empty`] if it is empty.
    fn key(&self, name: &str) -> Result<Key, KeyError>;
}

/// Production [`KeySource`] backed by the process environment.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvKeys;

impl KeySource for EnvKeys {
    #[expect(
        clippy::disallowed_methods,
        reason = "the one reader of the environment"
    )]
    fn key(&self, name: &str) -> Result<Key, KeyError> {
        match std::env::var(name) {
            Ok(val) if val.is_empty() => Err(KeyError::Empty),
            Ok(val) => Ok(Key(val)),
            Err(_) => Err(KeyError::Unset),
        }
    }
}

/// In-memory [`KeySource`] test double. Redacts all values in `Debug`.
#[derive(Default, Clone)]
pub struct ScriptedKeys {
    entries: BTreeMap<String, String>,
}

impl ScriptedKeys {
    /// Creates an empty [`ScriptedKeys`] double.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Inserts `name -> value` and returns `self` for chaining.
    #[must_use]
    pub fn with_key(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.entries.insert(name.into(), value.into());
        self
    }

    /// Inserts or replaces `name -> value`.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.entries.insert(name.into(), value.into());
    }
}

impl fmt::Debug for ScriptedKeys {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut map = f.debug_map();
        for name in self.entries.keys() {
            map.entry(name, &"<redacted>");
        }
        map.finish()
    }
}

impl KeySource for ScriptedKeys {
    fn key(&self, name: &str) -> Result<Key, KeyError> {
        match self.entries.get(name) {
            None => Err(KeyError::Unset),
            Some(v) if v.is_empty() => Err(KeyError::Empty),
            Some(v) => Ok(Key(v.clone())),
        }
    }
}
