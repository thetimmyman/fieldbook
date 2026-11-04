//! Streaming SHA-256 digest (`hash::Sha256Stream`).
//!
//! **Responsibility.** Wrap [`sha2::Sha256`] and return the digest as lowercase hex
//! through [`crate::hex::lower`], so the binary can hash an original while it reads it.

use sha2::{Digest, Sha256};

use crate::hex;

/// Streaming SHA-256 hasher whose [`Self::finish`] returns 64 lowercase hexadecimal characters.
#[derive(Debug, Clone, Default)]
pub struct Sha256Stream {
    inner: Sha256,
}

impl Sha256Stream {
    /// Start a new SHA-256 stream.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Sha256::new(),
        }
    }

    /// Feed `bytes` into the digest.
    pub fn update(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    /// Complete the digest and return the 64-character lowercase hex string.
    #[must_use]
    pub fn finish(self) -> String {
        let digest = self.inner.finalize();
        hex::lower(&digest)
    }
}

/// Compute the 64-character lowercase hex SHA-256 digest of `bytes` in one call.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut stream = Sha256Stream::new();
    stream.update(bytes);
    stream.finish()
}

#[cfg(test)]
mod tests {
    use super::{Sha256Stream, sha256_hex};

    #[test]
    fn empty_stream_hashes_to_the_empty_sha256() {
        let s = Sha256Stream::new();
        assert_eq!(
            s.finish(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn chunked_updates_equal_single_update() {
        let mut s = Sha256Stream::new();
        s.update(b"alpha-");
        s.update(b"beta");
        assert_eq!(s.finish(), sha256_hex(b"alpha-beta"));
    }
}
