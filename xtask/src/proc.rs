//! Running other programs.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// The repository root: the parent of the `xtask` crate, wherever the command runs from.
pub fn repo_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf)
}

fn describe(program: &str, args: &[&str]) -> String {
    format!("`{program} {}`", args.join(" "))
}

/// Run `program` in `dir`, returning its output whatever the exit code.
pub fn output(program: &str, args: &[&str], dir: &Path) -> Result<Output, String> {
    Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .map_err(|error| format!("cannot run {}: {error}", describe(program, args)))
}

/// Run `program` in `dir` and return its standard output. Fails when the program is
/// missing or exits non-zero; the error carries the program's standard error.
pub fn capture(program: &str, args: &[&str], dir: &Path) -> Result<String, String> {
    let out = output(program, args, dir)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "{} failed: {}",
            describe(program, args),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// Run `program` in `dir` with its output going to ours. True when it exits zero.
pub fn run_visible(program: &str, args: &[&str], dir: &Path) -> Result<bool, String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|error| format!("cannot run {}: {error}", describe(program, args)))?;
    Ok(status.success())
}

/// Run git in `dir` and return its standard output.
pub fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    capture("git", args, dir)
}
