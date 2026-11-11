#![cfg(unix)]
#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "xtask hook integration tests start processes and read synthetic repositories"
)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Stdio};

fn init(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let result = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .output()?;
    if !result.status.success() {
        return Err("repository initialization failed".into());
    }
    Ok(())
}

#[test]
fn pre_push_does_not_redirect_foreign_git_to_the_pushed_repository() {
    let root = std::env::temp_dir().join(format!("fieldbook-hook-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    init(&root).expect("pushed repository");
    init(&root.join("foreign")).expect("foreign repository");
    let bin = root.join("bin");
    std::fs::create_dir(&bin).expect("command directory");
    let just = bin.join("just");
    std::fs::write(
        &just,
        "#!/bin/sh\nset -eu\ntest \"$1\" = scan\nactual=$(git -C foreign rev-parse --show-toplevel)\ntest \"$actual\" = \"$(pwd)/foreign\"\nprintf '%s' \"$actual\" > observed-root\n",
    )
    .expect("check command");
    std::fs::set_permissions(&just, std::fs::Permissions::from_mode(0o755))
        .expect("executable command");
    let inherited_path = std::env::var_os("PATH").expect("PATH");
    let path =
        std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited_path)))
            .expect("command path");
    let hook = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.githooks/pre-push");
    let before = std::fs::read(root.join(".git/HEAD")).expect("initial HEAD");
    let result = Command::new("bash")
        .arg(hook)
        .current_dir(&root)
        .env("PATH", path)
        .env("GIT_DIR", root.join(".git"))
        .env("GIT_WORK_TREE", &root)
        .env("GIT_INDEX_FILE", root.join(".git/index"))
        .env_remove("FIELDBOOK_DENYLIST")
        .stdin(Stdio::null())
        .output()
        .expect("pre-push hook");
    assert!(
        result.status.success(),
        "hook failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(root.join("observed-root")).expect("observed foreign root"),
        root.join("foreign").to_string_lossy()
    );
    assert_eq!(std::fs::read(root.join(".git/HEAD")).expect("HEAD"), before);
    assert!(!root.join(".git/index").exists());
    std::fs::remove_dir_all(root).expect("cleanup");
}
