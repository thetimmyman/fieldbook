//! Hosted deploy-job script integration tests (`hosted-workflow`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "Hosted deploy-job script integration tests run bash scripts and inspect temp trees"
)]

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

const SYNTHETIC_HOST: &str = "fb-synthetic.example.invalid";
const SYNTHETIC_ACCOUNT: &str = "synthetic-account-0001";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn require_bash() {
    let bash_ok = Command::new("bash")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !bash_ok {
        panic!("bash is required for the deploy script tests");
    }
}

fn run_deploy_script(
    script_name: &str,
    work_dir: &Path,
    env_vars: &[(&str, &str)],
) -> (ExitStatus, String) {
    let script_path = repo_root().join("deploy").join(script_name);
    let mut cmd = Command::new("bash");
    cmd.arg(&script_path)
        .current_dir(work_dir)
        .env_remove("FIELDBOOK_GIT_SHA")
        .env_remove("FIELDBOOK_HOST")
        .env_remove("CLOUDFLARE_ACCOUNT_ID");
    for &(key, val) in env_vars {
        cmd.env(key, val);
    }
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("failed to run {}: {e}", script_path.display()));
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");
    (output.status, combined)
}

// control: HO-06
#[test]
fn identifier_script_fails_when_tree_contains_host_value() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("deployed-config.txt"),
        format!("endpoint: https://{SYNTHETIC_HOST}/api\n"),
    )
    .expect("write test file");

    let (status, output) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[
            ("FIELDBOOK_HOST", SYNTHETIC_HOST),
            ("CLOUDFLARE_ACCOUNT_ID", SYNTHETIC_ACCOUNT),
        ],
    );

    assert!(
        !status.success(),
        "expected non-zero exit when host value is present, got {status:?}, output: {output}"
    );
    assert!(
        output.contains("FIELDBOOK_HOST"),
        "expected output to name FIELDBOOK_HOST, got: {output}"
    );
    assert!(
        !output.contains(SYNTHETIC_HOST),
        "output must not contain host value: {output}"
    );
    assert!(
        !output.contains(&SYNTHETIC_HOST.to_ascii_uppercase()),
        "output must not contain upper-case host value: {output}"
    );
}

// control: HO-06
#[test]
fn identifier_script_fails_when_tree_contains_uppercase_host_value() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("upper-host.txt"),
        format!("HOST={}\n", SYNTHETIC_HOST.to_ascii_uppercase()),
    )
    .expect("write test file");

    let (status, output) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[
            ("FIELDBOOK_HOST", SYNTHETIC_HOST),
            ("CLOUDFLARE_ACCOUNT_ID", SYNTHETIC_ACCOUNT),
        ],
    );

    assert!(
        !status.success(),
        "expected non-zero exit when uppercase host value is present, got {status:?}, output: {output}"
    );
    assert!(
        output.contains("FIELDBOOK_HOST"),
        "expected output to name FIELDBOOK_HOST, got: {output}"
    );
    assert!(
        !output.contains(SYNTHETIC_HOST),
        "output must not contain lowercase host value: {output}"
    );
    assert!(
        !output.contains(&SYNTHETIC_HOST.to_ascii_uppercase()),
        "output must not contain uppercase host value: {output}"
    );
}

// control: HO-06
#[test]
fn identifier_script_fails_when_tree_contains_account_value() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("account.txt"),
        format!("account_id = \"{SYNTHETIC_ACCOUNT}\"\n"),
    )
    .expect("write test file");

    let (status, output) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[
            ("FIELDBOOK_HOST", SYNTHETIC_HOST),
            ("CLOUDFLARE_ACCOUNT_ID", SYNTHETIC_ACCOUNT),
        ],
    );

    assert!(
        !status.success(),
        "expected non-zero exit when account value is present, got {status:?}, output: {output}"
    );
    assert!(
        output.contains("CLOUDFLARE_ACCOUNT_ID"),
        "expected output to name CLOUDFLARE_ACCOUNT_ID, got: {output}"
    );
    assert!(
        !output.contains(SYNTHETIC_ACCOUNT),
        "output must not contain account value: {output}"
    );
    assert!(
        !output.contains(&SYNTHETIC_ACCOUNT.to_ascii_uppercase()),
        "output must not contain uppercase account value: {output}"
    );
}

// control: HO-06
#[test]
fn identifier_script_excludes_git_directory() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    let git_dir = tmp.path().join(".git");
    std::fs::create_dir_all(&git_dir).expect("create .git dir");
    std::fs::write(
        git_dir.join("config"),
        format!("url = https://{SYNTHETIC_HOST}/{SYNTHETIC_ACCOUNT}.git\n"),
    )
    .expect("write .git/config");
    std::fs::write(
        tmp.path().join("tracked-file.txt"),
        "clean file content without any identifier values\n",
    )
    .expect("write clean file");

    let (status, output) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[
            ("FIELDBOOK_HOST", SYNTHETIC_HOST),
            ("CLOUDFLARE_ACCOUNT_ID", SYNTHETIC_ACCOUNT),
        ],
    );

    assert!(
        status.success(),
        "expected exit 0 when identifier values only exist under .git, got {status:?}, output: {output}"
    );
    assert!(
        output.contains("identifiers: none in the tree"),
        "expected 'identifiers: none in the tree', got: {output}"
    );
    assert!(
        !output.contains(SYNTHETIC_HOST),
        "output must not contain host value: {output}"
    );
    assert!(
        !output.contains(SYNTHETIC_ACCOUNT),
        "output must not contain account value: {output}"
    );
}

// control: HO-06
#[test]
fn identifier_script_passes_on_clean_tree() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        tmp.path().join("index.html"),
        "<!doctype html><html><body>Fieldbook</body></html>\n",
    )
    .expect("write clean html");
    std::fs::write(
        tmp.path().join("styles.css"),
        "body { font-family: sans-serif; }\n",
    )
    .expect("write clean css");

    let (status, output) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[
            ("FIELDBOOK_HOST", SYNTHETIC_HOST),
            ("CLOUDFLARE_ACCOUNT_ID", SYNTHETIC_ACCOUNT),
        ],
    );

    assert!(
        status.success(),
        "expected exit 0 on clean tree, got {status:?}, output: {output}"
    );
    assert!(
        output.contains("identifiers: none in the tree"),
        "expected 'identifiers: none in the tree', got: {output}"
    );
}

// control: HO-06
#[test]
fn identifier_script_fails_when_required_env_empty_or_unset() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("clean.txt"), "clean file content\n").expect("write clean file");

    let (status_empty_host, output_empty_host) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[
            ("FIELDBOOK_HOST", ""),
            ("CLOUDFLARE_ACCOUNT_ID", SYNTHETIC_ACCOUNT),
        ],
    );
    assert!(
        !status_empty_host.success(),
        "expected non-zero exit when FIELDBOOK_HOST is empty, got {status_empty_host:?}, output: {output_empty_host}"
    );
    assert!(
        output_empty_host.contains("FIELDBOOK_HOST"),
        "expected output to name FIELDBOOK_HOST when empty, got: {output_empty_host}"
    );

    let (status_unset_account, output_unset_account) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[("FIELDBOOK_HOST", SYNTHETIC_HOST)],
    );
    assert!(
        !status_unset_account.success(),
        "expected non-zero exit when CLOUDFLARE_ACCOUNT_ID is unset, got {status_unset_account:?}, output: {output_unset_account}"
    );
    assert!(
        output_unset_account.contains("CLOUDFLARE_ACCOUNT_ID"),
        "expected output to name CLOUDFLARE_ACCOUNT_ID when unset, got: {output_unset_account}"
    );
}

#[cfg(unix)]
// control: HO-06
#[test]
fn identifier_script_fails_when_search_tool_errors() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("clean.txt"), "clean file content\n").expect("write clean file");

    let bin = tempfile::tempdir().expect("tempdir for fake grep");
    let grep_path = bin.path().join("grep");
    std::fs::write(&grep_path, "#!/bin/sh\nexit 2\n").expect("write fake grep script");

    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&grep_path, std::fs::Permissions::from_mode(0o755))
            .expect("chmod fake grep script");
    }

    let orig_path = std::env::var("PATH").unwrap_or_default();
    let custom_path = format!("{}:{}", bin.path().display(), orig_path);

    let (status, output) = run_deploy_script(
        "check-no-identifiers.sh",
        tmp.path(),
        &[
            ("PATH", &custom_path),
            ("FIELDBOOK_HOST", SYNTHETIC_HOST),
            ("CLOUDFLARE_ACCOUNT_ID", SYNTHETIC_ACCOUNT),
        ],
    );

    assert!(
        !status.success(),
        "expected non-zero exit when grep errors, got {status:?}, output: {output}"
    );
    assert!(
        output.contains("FIELDBOOK_HOST"),
        "expected output to name the variable FIELDBOOK_HOST, got: {output}"
    );
}

// control: HO-07
#[test]
fn unreleased_script_fails_when_page_lacks_unreleased_build_text() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    let hosted_dir = tmp.path().join("out/hosted");
    std::fs::create_dir_all(&hosted_dir).expect("create out/hosted dir");
    std::fs::write(
        hosted_dir.join("index.html"),
        "<!doctype html><html><head><title>Fieldbook</title></head><body><h1>Production Edition</h1></body></html>\n",
    )
    .expect("write index.html without unreleased build text");

    let (status, output) = run_deploy_script("check-unreleased.sh", tmp.path(), &[]);

    assert!(
        !status.success(),
        "expected non-zero exit when page lacks 'unreleased build', got {status:?}, output: {output}"
    );
}

// control: HO-07
#[test]
fn unreleased_script_fails_when_index_html_missing() {
    require_bash();

    let tmp_no_dir = tempfile::tempdir().expect("tempdir");
    let (status_no_dir, output_no_dir) =
        run_deploy_script("check-unreleased.sh", tmp_no_dir.path(), &[]);
    assert!(
        !status_no_dir.success(),
        "expected non-zero exit when out/hosted does not exist, got {status_no_dir:?}, output: {output_no_dir}"
    );

    let tmp_with_dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp_with_dir.path().join("out/hosted")).expect("create out/hosted dir");
    let (status_with_dir, output_with_dir) =
        run_deploy_script("check-unreleased.sh", tmp_with_dir.path(), &[]);
    assert!(
        !status_with_dir.success(),
        "expected non-zero exit when index.html is missing inside out/hosted, got {status_with_dir:?}, output: {output_with_dir}"
    );
}

// control: HO-07
#[test]
fn unreleased_script_passes_when_page_contains_unreleased_build() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    let hosted_dir = tmp.path().join("out/hosted");
    std::fs::create_dir_all(&hosted_dir).expect("create out/hosted dir");
    std::fs::write(
        hosted_dir.join("index.html"),
        "<!doctype html><html><body><p>Unreleased Build</p></body></html>\n",
    )
    .expect("write index.html with unreleased build text");

    let (status, output) = run_deploy_script("check-unreleased.sh", tmp.path(), &[]);

    assert!(
        status.success(),
        "expected exit 0 when page carries unreleased build text, got {status:?}, output: {output}"
    );
    assert!(
        output.contains("unreleased build: ok"),
        "expected 'unreleased build: ok', got: {output}"
    );
}

// control: HO-07
#[test]
fn unreleased_script_fails_when_git_sha_env_is_set() {
    require_bash();
    let tmp = tempfile::tempdir().expect("tempdir");
    let hosted_dir = tmp.path().join("out/hosted");
    std::fs::create_dir_all(&hosted_dir).expect("create out/hosted dir");
    std::fs::write(
        hosted_dir.join("index.html"),
        "<!doctype html><html><body><p>Unreleased Build</p></body></html>\n",
    )
    .expect("write index.html with unreleased build text");

    let (status_sha, output_sha) = run_deploy_script(
        "check-unreleased.sh",
        tmp.path(),
        &[("FIELDBOOK_GIT_SHA", "abc")],
    );
    assert!(
        !status_sha.success(),
        "expected non-zero exit when FIELDBOOK_GIT_SHA is set to 'abc', got {status_sha:?}, output: {output_sha}"
    );

    let (status_empty_sha, output_empty_sha) = run_deploy_script(
        "check-unreleased.sh",
        tmp.path(),
        &[("FIELDBOOK_GIT_SHA", "")],
    );
    assert!(
        !status_empty_sha.success(),
        "expected non-zero exit when FIELDBOOK_GIT_SHA is set to empty string, got {status_empty_sha:?}, output: {output_empty_sha}"
    );
}
