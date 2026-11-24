//! Release-build policy guarantees and CI/recipe drift checks (`policy-load` steps 7, 8, 11, 15).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "release and workflow tests inspect Cargo manifests, justfile, ci.yml, and CLI flags"
)]

use std::path::{Path, PathBuf};

// control: PL-03
#[cfg(not(feature = "test-hooks"))]
#[test]
fn a_release_build_has_no_policy_or_program_data_flag() {
    let world = ReleaseFakeWorld::default();

    for bad_args in [
        &["--policy", "x", "policy", "show"][..],
        &["--program-data", "x", "policy", "show"][..],
    ] {
        let argv: Vec<String> = bad_args.iter().map(|s| (*s).to_owned()).collect();
        let outcome = fieldbook::run(&argv, &world);
        assert_eq!(outcome.code, 2, "expected exit 2 for {bad_args:?}");
        assert!(
            outcome.stderr.contains("unexpected argument"),
            "expected clap unknown-argument error for {bad_args:?}, got:\n{}",
            outcome.stderr
        );
    }

    let mut cmd = <fieldbook::Cli as clap::CommandFactory>::command();
    let help = cmd.render_long_help().to_string();
    assert!(!help.contains("--policy"), "help lists --policy:\n{help}");
    assert!(
        !help.contains("--program-data"),
        "help lists --program-data:\n{help}"
    );
}

#[test]
fn the_check_recipe_runs_the_all_features_builds() {
    let justfile = include_str!("../../../justfile");
    let check_section = justfile
        .split("\ncheck:")
        .nth(1)
        .expect("check recipe in justfile");
    assert!(
        check_section.contains("cargo nextest run -p fieldbook --locked --all-features"),
        "justfile check recipe is missing `cargo nextest run -p fieldbook --locked --all-features`"
    );
    assert!(
        check_section.contains("cargo clippy --workspace --all-targets --locked --all-features"),
        "justfile check recipe is missing `cargo clippy --workspace --all-targets --locked --all-features`"
    );
}

fn collect_cargo_tomls(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_cargo_tomls(&path, out);
        } else if path.file_name().and_then(|s| s.to_str()) == Some("Cargo.toml") {
            out.push(path);
        }
    }
}

#[test]
fn only_the_fieldbook_manifest_names_test_hooks() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let allowed = repo_root
        .join("crates/fieldbook/Cargo.toml")
        .canonicalize()
        .unwrap();

    let mut manifests = Vec::new();
    collect_cargo_tomls(&repo_root.join("crates"), &mut manifests);
    collect_cargo_tomls(&repo_root.join("xtask"), &mut manifests);

    for manifest in manifests {
        let canon = manifest.canonicalize().unwrap();
        let text = std::fs::read_to_string(&canon).unwrap();
        if canon == allowed {
            assert!(
                text.contains("test-hooks"),
                "crates/fieldbook/Cargo.toml must declare test-hooks"
            );
        } else {
            assert!(
                !text.contains("test-hooks"),
                "manifest {} must not name `test-hooks`",
                manifest.display()
            );
        }
    }
}

#[test]
fn windows_policy_features_are_executed() {
    let ci_yml = include_str!("../../../.github/workflows/ci.yml");

    let windows_section = ci_yml
        .split("\n  windows:\n")
        .nth(1)
        .expect("windows job in ci.yml")
        .split("\n  ci-ok:\n")
        .next()
        .expect("windows job before ci-ok");

    assert!(
        windows_section.contains("cargo nextest run --workspace --exclude xtask --locked"),
        "windows job must retain default-feature nextest invocation"
    );
    assert!(
        windows_section.contains("cargo xtask policy-tests"),
        "windows job must invoke `cargo xtask policy-tests` wrapper"
    );
    assert!(
        !windows_section.contains("cargo nextest run --locked -p fieldbook --all-features")
            && !windows_section.contains("cargo nextest run -p fieldbook --locked --all-features"),
        "windows job must use `cargo xtask policy-tests` wrapper rather than an unprovisioned direct --all-features call"
    );

    let ci_ok_section = ci_yml
        .split("\n  ci-ok:\n")
        .nth(1)
        .expect("ci-ok job in ci.yml");
    assert!(
        ci_ok_section.contains("needs: [rust, corpus, changes, windows]"),
        "ci-ok must depend on windows job"
    );
}

#[cfg(not(feature = "test-hooks"))]
struct ReleaseFakeWorld {
    cwd: PathBuf,
    tree_root: PathBuf,
    state_dir: PathBuf,
}

#[cfg(not(feature = "test-hooks"))]
impl Default for ReleaseFakeWorld {
    fn default() -> Self {
        Self {
            cwd: PathBuf::from("/repo"),
            tree_root: PathBuf::from("/repo"),
            state_dir: PathBuf::from("/repo/.fieldbook"),
        }
    }
}

#[cfg(not(feature = "test-hooks"))]
impl fieldbook::World for ReleaseFakeWorld {
    fn read_tree(&self) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
        Ok(fieldbook_core::check::Tree::new())
    }
    fn write_lease(&self) -> Result<fieldbook::StoreLease, fieldbook::WorldError> {
        Err(fieldbook::WorldError::new(&self.state_dir, "unsupported"))
    }
    fn read_tree_under(
        &self,
        _lease: &fieldbook::StoreLease,
    ) -> Result<fieldbook_core::check::Tree, fieldbook::WorldError> {
        Ok(fieldbook_core::check::Tree::new())
    }
    fn read_file(&self, _path: &Path) -> Result<Option<Vec<u8>>, fieldbook::WorldError> {
        Ok(None)
    }
    fn write_file(
        &self,
        _audited: &fieldbook::Audited,
        path: &Path,
        _bytes: &[u8],
    ) -> Result<fieldbook_core::audit::Sha256, fieldbook::WorldError> {
        Err(fieldbook::WorldError::new(path, "unsupported"))
    }
    fn write_outside(
        &self,
        _allowed: &fieldbook_core::egress::Allowed,
        _audited: &fieldbook::Audited,
        path: &Path,
        _bytes: &[u8],
    ) -> Result<fieldbook_core::audit::Sha256, fieldbook::WorldError> {
        Err(fieldbook::WorldError::new(path, "unsupported"))
    }
    fn has_company_store(&self) -> bool {
        false
    }
    fn today(&self) -> fieldbook_core::CivilDate {
        fieldbook_core::CivilDate::from_ymd(2026, 3, 1).unwrap()
    }
    fn now(&self) -> fieldbook_core::audit::Now {
        fieldbook_core::audit::Now::new(self.today(), 43_200).unwrap()
    }
    fn principal(&self) -> fieldbook_core::audit::Principal {
        fieldbook_core::audit::Principal::local()
    }
    fn cwd(&self) -> &Path {
        &self.cwd
    }
    fn tree_root(&self) -> &Path {
        &self.tree_root
    }
    fn state_dir(&self) -> &Path {
        &self.state_dir
    }
    fn stdin_is_terminal(&self) -> bool {
        false
    }
    fn read_line(&self) -> String {
        String::new()
    }
    fn prompt(&self, _flag: &str) -> Result<(), fieldbook::WorldError> {
        Ok(())
    }
    fn place(&self, _path: &Path) -> fieldbook::PlaceFacts {
        fieldbook::PlaceFacts::local()
    }
    fn support_facts(&self) -> fieldbook::SupportFacts {
        fieldbook::SupportFacts::default()
    }
    fn reset_drill(&self, _audited: &fieldbook::Audited) -> Result<(), fieldbook::WorldError> {
        Err(fieldbook::WorldError::OutOfScope(
            self.state_dir.join("drill"),
        ))
    }
    fn remove_drill(&self, _audited: &fieldbook::Audited) -> Result<(), fieldbook::WorldError> {
        Err(fieldbook::WorldError::OutOfScope(
            self.state_dir.join("drill"),
        ))
    }
    fn store_id(&self) -> &'static str {
        "memory@0"
    }
    fn read_maintenance(&self) -> Result<String, fieldbook::WorldError> {
        Ok(String::new())
    }
    fn read_state_summary(
        &self,
        _today: fieldbook_core::CivilDate,
    ) -> Result<fieldbook_core::status::StateSummary, fieldbook::WorldError> {
        Ok(fieldbook_core::status::StateSummary::default())
    }
}
