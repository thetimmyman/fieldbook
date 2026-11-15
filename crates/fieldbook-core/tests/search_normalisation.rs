use fieldbook_core::search::normalise;

#[test]
fn normalisation_is_idempotent_after_case_mapping_and_soft_hyphen_removal() {
    for (input, expected) in [
        ("J\u{030c}", "\u{01f0}"),
        ("\u{0130}\u{0316}", "i\u{0316}\u{0307}"),
        ("e\u{00ad}\u{0301}", "\u{00e9}"),
    ] {
        let once = normalise(input);
        assert_eq!(once, expected);
        assert_eq!(normalise(&once), once);
    }
}
