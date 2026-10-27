//! The `fieldbook` binary. All behaviour lives in the library; see `fieldbook::run`.

fn main() -> std::process::ExitCode {
    fieldbook::io::run_process()
}
