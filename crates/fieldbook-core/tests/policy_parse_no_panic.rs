//! No-panic fuzz/property test over `Policy::parse` (`core-policy` step 2).

#[path = "../../../tests/support/rng.rs"]
mod rng;

mod policy_parse_no_panic {
    use fieldbook_core::policy::Policy;

    use super::rng::Rng;

    const FIXTURE_POLICY: &str = include_str!("../../../tests/fixtures/policy/policy.toml");

    #[test]
    fn parser_never_panics_on_200_seeded_inputs() {
        let mut rng = Rng::new(1);
        for _ in 0..200 {
            let len = usize::try_from(rng.next_u64() % 129).unwrap_or(0);
            let bytes = rng.bytes(len);
            let text = String::from_utf8_lossy(&bytes);
            let _ = Policy::parse(&text, None);
            let _ = Policy::parse(&text, Some(3));
        }

        let fixture_bytes = FIXTURE_POLICY.as_bytes();
        for i in 0..fixture_bytes.len() {
            let mut flipped = fixture_bytes.to_vec();
            flipped[i] ^= 0x5a;
            let flipped_text = String::from_utf8_lossy(&flipped);
            let _ = Policy::parse(&flipped_text, None);
            let _ = Policy::parse(&flipped_text, Some(3));

            let mut deleted = fixture_bytes.to_vec();
            deleted.remove(i);
            let deleted_text = String::from_utf8_lossy(&deleted);
            let _ = Policy::parse(&deleted_text, None);
            let _ = Policy::parse(&deleted_text, Some(3));
        }
    }
}
