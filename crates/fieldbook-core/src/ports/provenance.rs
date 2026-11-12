//! Provenance reference resolution and handoff tickets (`AD-01`, `ADR 0004 §12`).

use std::fmt;

use crate::audit::{Event, EventPayload, Sha256};

/// Reference to an audited model call or paste in the journal.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProvenanceRef {
    /// 1-based sequence number of the matching event.
    pub call_seq: u64,
    /// SHA-256 digest of the output bytes.
    pub output_sha256: Sha256,
}

impl ProvenanceRef {
    /// Constructs a new [`ProvenanceRef`].
    #[must_use]
    pub const fn new(call_seq: u64, output_sha256: Sha256) -> Self {
        Self {
            call_seq,
            output_sha256,
        }
    }
}

/// Typed handoff identifier in the format `H-nnnn` where `nnnn` is `0001..=9999`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HandoffId(u32);

/// Error returned when parsing a [`HandoffId`] fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandoffIdError;

impl fmt::Display for HandoffIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("handoff identifier must have form H-nnnn with nnnn in 0001..=9999")
    }
}

impl std::error::Error for HandoffIdError {}

impl HandoffId {
    /// Minimum permitted handoff number.
    pub const MIN: u32 = 1;
    /// Maximum permitted handoff number.
    pub const MAX: u32 = 9999;

    /// Constructs a [`HandoffId`] from an integer value if within `1..=9999`.
    #[must_use]
    pub const fn new(n: u32) -> Option<Self> {
        if n >= Self::MIN && n <= Self::MAX {
            Some(Self(n))
        } else {
            None
        }
    }

    /// Returns the underlying integer identifier.
    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Parses a handoff identifier string (`H-nnnn`).
    ///
    /// Accepts exactly `H-` followed by four ASCII decimal digits in `0001..=9999`.
    /// Rejects `H-0000`, leading/trailing spaces, and wrong lengths without panicking.
    ///
    /// # Errors
    ///
    /// Returns [`HandoffIdError`] if `text` is not a valid handoff identifier.
    pub fn parse(text: &str) -> Result<Self, HandoffIdError> {
        let bytes = text.as_bytes();
        if bytes.len() != 6 {
            return Err(HandoffIdError);
        }
        if bytes.get(0..2) != Some(b"H-") {
            return Err(HandoffIdError);
        }
        let Some(digits) = bytes.get(2..6) else {
            return Err(HandoffIdError);
        };
        let mut val: u32 = 0;
        for &b in digits {
            if !b.is_ascii_digit() {
                return Err(HandoffIdError);
            }
            val = val * 10 + u32::from(b - b'0');
        }
        Self::new(val).ok_or(HandoffIdError)
    }

    /// Formats the handoff identifier as `H-nnnn`.
    #[must_use]
    pub fn format(self) -> String {
        format!("H-{:04}", self.0)
    }
}

impl fmt::Display for HandoffId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "H-{:04}", self.0)
    }
}

/// Completed handoff ticket holding a parsed identifier and pack content digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffTicket {
    /// Handoff identifier.
    pub id: HandoffId,
    /// SHA-256 digest of the handoff pack content.
    pub pack_sha256: Sha256,
}

impl HandoffTicket {
    /// Constructs a new [`HandoffTicket`].
    #[must_use]
    pub const fn new(id: HandoffId, pack_sha256: Sha256) -> Self {
        Self { id, pack_sha256 }
    }
}

/// Diagnostic error code FB0917: provenance does not resolve in the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fb0917;

impl fmt::Display for Fb0917 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FB0917: provenance does not resolve in the journal")
    }
}

impl std::error::Error for Fb0917 {}

/// Verifies that a provenance reference resolves to an audited `ai.call` or `ai.paste` event.
///
/// # Errors
///
/// Returns [`Fb0917`] if no event matches the sequence number and output digest.
pub fn verify_provenance<'a>(
    events: impl IntoIterator<Item = &'a Event>,
    r: &ProvenanceRef,
) -> Result<(), Fb0917> {
    for event in events {
        if event.seq == r.call_seq {
            match &event.payload {
                EventPayload::AiCall {
                    output_sha256: Some(out),
                    ..
                } if out == &r.output_sha256 => return Ok(()),
                EventPayload::AiPaste { output_sha256, .. }
                    if output_sha256 == &r.output_sha256 =>
                {
                    return Ok(());
                }
                _ => {}
            }
        }
    }
    Err(Fb0917)
}
