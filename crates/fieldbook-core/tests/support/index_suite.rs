//! Shared contract suite for an [`IndexPort`] adapter.
//!
//! Included via `#[path = "support/index_suite.rs"] mod index_suite;` from an adapter's own test
//! file. The same cases run for every adapter of the port.

#[expect(
    clippy::duplicate_mod,
    reason = "shared seeded test RNG is included per integration test binary"
)]
#[path = "../../../../tests/support/rng.rs"]
mod rng;

use fieldbook_core::ports::index::{IndexPort, Query, Scope, Tier};

/// Runs the shared contract over an [`IndexPort`] adapter constructed by `make`.
pub fn index_suite<I: IndexPort>(make: &dyn Fn() -> I) {
    let first = make();
    let second = make();
    assert!(!first.id().is_empty(), "index id must not be empty");

    // The same query gives the same hits from two independently built indexes.
    let query = Query::new("withholding", Scope::all(), 10);
    assert_eq!(
        first.query(&query),
        second.query(&query),
        "{}: same query must give the same hits",
        first.id()
    );

    // An empty query gives no hits and no error.
    let empty = first
        .query(&Query::new("", Scope::all(), 10))
        .unwrap_or_else(|error| panic!("{}: empty query failed: {error}", first.id()));
    assert!(
        empty.hits.is_empty() && empty.total == 0,
        "{}: empty query must give no hits",
        first.id()
    );

    // A random 1 MB query does not panic.
    let mut rng = rng::Rng::new(1);
    let noise = String::from_utf8_lossy(&rng.bytes(1 << 20)).into_owned();
    let _ = first.query(&Query::new(noise, Scope::all(), 10));

    // Exact-tier hits come before ranked hits.
    let exact = first
        .query(&Query::new("S-0001-v1", Scope::all(), 50))
        .unwrap_or_else(|error| panic!("{}: exact query failed: {error}", first.id()));
    let mut saw_ranked = false;
    for hit in &exact.hits {
        match hit.tier {
            Tier::Ranked => saw_ranked = true,
            Tier::Exact => assert!(
                !saw_ranked,
                "{}: exact-tier hit after a ranked hit",
                first.id()
            ),
        }
    }

    // A scope restricts the kinds returned.
    let claim_scope = Scope::new(["claim"])
        .unwrap_or_else(|error| panic!("{}: claim scope: {error}", first.id()));
    let claims = first
        .query(&Query::new("synthetic", claim_scope, 50))
        .unwrap_or_else(|error| panic!("{}: scoped query failed: {error}", first.id()));
    assert!(
        !claims.hits.is_empty(),
        "{}: claim scope must return at least one hit",
        first.id()
    );
    assert!(
        claims.hits.iter().all(|hit| hit.unit.kind == "claim"),
        "{}: claim scope returned another kind",
        first.id()
    );

    // The limit holds while the total counts every scoped match.
    let limited = first
        .query(&Query::new("synthetic", Scope::all(), 2))
        .unwrap_or_else(|error| panic!("{}: limited query failed: {error}", first.id()));
    assert!(
        limited.hits.len() <= 2,
        "{}: limit was not applied",
        first.id()
    );
    assert!(
        limited.total >= limited.hits.len(),
        "{}: total is below the returned hit count",
        first.id()
    );
}
