#[test]
fn synthetic_required_check_control() {
    let in_actions = std::env::var("GITHUB_ACTIONS").unwrap_or_default() == "true";
    assert!(!in_actions, "synthetic required-check control");
}
