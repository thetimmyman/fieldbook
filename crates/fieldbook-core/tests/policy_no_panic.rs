//! No-panic fuzz/mutation tests for `Seen::parse`, `FileFact::from_bytes`, and `policy::resolve`
//! (`policy-load` step 12).

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::path::PathBuf;

use fieldbook_core::audit::Sha256;
use fieldbook_core::policy::{FileFact, FileObservation, Seen, WriterWhy, resolve};
use rng::Rng;

const FIXTURE_BYTES: &[u8] = include_bytes!("../../../tests/fixtures/policy/policy.toml");

fn corpus_inputs() -> Vec<Vec<u8>> {
    let mut inputs = Vec::new();
    let mut prng = Rng::new(1);
    for _ in 0..200 {
        let len = usize::try_from(prng.next_u64() % 129).unwrap_or(0);
        inputs.push(prng.bytes(len));
    }

    let sample_seen = Seen {
        revision: 3,
        hash: Sha256::digest(FIXTURE_BYTES),
        audit_sink: Some(PathBuf::from("/srv/company/audit")),
    };
    let seen_bytes = sample_seen.to_text().into_bytes();

    for base in [seen_bytes.as_slice(), FIXTURE_BYTES] {
        inputs.push(base.to_vec());
        for i in 0..base.len() {
            let mut flipped = base.to_vec();
            flipped[i] ^= 0xFF;
            inputs.push(flipped);

            let mut deleted = Vec::with_capacity(base.len() - 1);
            deleted.extend_from_slice(&base[..i]);
            deleted.extend_from_slice(&base[i + 1..]);
            inputs.push(deleted);
        }
    }
    inputs
}

#[test]
fn seen_never_panics_on_random_and_mutated_bytes() {
    for bytes in corpus_inputs() {
        let fact = FileFact::from_bytes(&bytes);
        if let FileFact::Text(text) = fact {
            let parsed = Seen::parse(&text);
            let _ = parsed.is_some();
        }
    }
}

#[test]
fn resolve_never_panics_on_random_and_mutated_bytes() {
    let sample_seen = Seen {
        revision: 3,
        hash: Sha256::digest(FIXTURE_BYTES),
        audit_sink: Some(PathBuf::from("/srv/company/audit")),
    };

    for bytes in corpus_inputs() {
        for writers in [Ok(()), Err(WriterWhy::FileOther)] {
            for seen in [None, Some(&sample_seen)] {
                for has_company_store in [false, true] {
                    let obs = FileObservation::from_bytes(&bytes, writers);
                    let resolved = resolve(obs, seen, has_company_store);
                    let _ = resolved.observation_hash();
                }
            }
        }
    }
}
