//! `cargo xtask policy-tests`: run the feature-enabled `fieldbook` policy integration suite under
//! the Windows Administrators-provisioned wrapper (`xtask/policy-tests.ps1`) on Windows, or
//! directly through `cargo nextest` off Windows (`policy-load` steps 15, 16, 17, 19).

use std::path::Path;
use std::process::Command;

/// Executes `cargo xtask policy-tests`.
pub fn command(root: &Path) -> Result<bool, String> {
    let script = root.join("xtask/policy-tests.ps1");
    if !script.is_file() {
        return Err(format!("missing provisioning script: {}", script.display()));
    }

    if cfg!(windows) {
        let status = Command::new("powershell")
            .current_dir(root)
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&script)
            .status()
            .map_err(|err| format!("failed to start powershell for {}: {err}", script.display()))?;
        Ok(status.success())
    } else {
        let status = Command::new("cargo")
            .current_dir(root)
            .args([
                "nextest",
                "run",
                "--locked",
                "-p",
                fieldbook_core::strings::binary(),
                "--all-features",
            ])
            .status()
            .map_err(|err| format!("failed to run cargo nextest: {err}"))?;
        Ok(status.success())
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn provisioning_script_sets_token_owner_protected_dacl_and_cleans_up() {
        let script_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("policy-tests.ps1");
        let text = std::fs::read_to_string(&script_path).expect("read xtask/policy-tests.ps1");
        for needle in [
            "SeRestorePrivilege",
            "SeTakeOwnershipPrivilege",
            "OpenProcessToken",
            "SetTokenInformation",
            "TokenOwner",
            "S-1-5-32-544",
            "S-1-5-18",
            "SetAccessRuleProtection($true, $false)",
            "cargo nextest run --locked -p fieldbook --all-features",
            "finally",
        ] {
            assert!(
                text.contains(needle),
                "xtask/policy-tests.ps1 is missing required provisioning step `{needle}`"
            );
        }
    }
}
