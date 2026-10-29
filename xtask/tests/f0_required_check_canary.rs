#[test]
fn synthetic_required_check_control() {
    let in_actions = std::env::var_os("GITHUB_ACTIONS").is_some_and(|value| value == "true");
    assert!(!in_actions, "synthetic required-check control");
}
