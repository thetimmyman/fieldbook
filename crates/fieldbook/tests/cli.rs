//! The built program, run as a process.

#![expect(
    clippy::disallowed_types,
    reason = "these tests exist to start the real binary"
)]

use std::error::Error;
use std::process::{Command, Output};

fn fieldbook(args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_fieldbook"))
        .args(args)
        .output()
}

#[test]
fn version_prints_name_version_and_identity() -> Result<(), Box<dyn Error>> {
    let output = fieldbook(&["--version"])?;
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout)?;
    assert_eq!(stdout, format!("{}\n", fieldbook::version_line()));
    assert!(stdout.starts_with("fieldbook "));
    Ok(())
}

#[test]
fn unknown_arguments_print_usage_and_exit_two() -> Result<(), Box<dyn Error>> {
    for args in [&[][..], &["check"], &["--version", "extra"]] {
        let output = fieldbook(args)?;
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8(output.stderr)?.starts_with("usage: fieldbook"));
    }
    Ok(())
}
