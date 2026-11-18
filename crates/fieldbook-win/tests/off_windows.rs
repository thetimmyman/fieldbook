//! Off Windows, every entry point in `fieldbook_win` returns `Err(WinError { call: "not windows", code: 0 })`.

#![cfg(not(windows))]

use std::path::Path;

use fieldbook_win::{
    Folder, WinError, final_path, in_group, interactive_logons, known_folder, mark_not_indexed,
    place, principal, writers,
};

#[test]
fn every_function_is_an_error_off_windows() {
    let want = WinError {
        call: "not windows",
        code: 0,
    };
    let here = Path::new(".");
    assert_eq!(known_folder(Folder::ProgramData), Err(want));
    assert_eq!(known_folder(Folder::LocalAppData), Err(want));
    assert_eq!(principal(), Err(want));
    assert_eq!(in_group("Everyone"), Err(want));
    assert_eq!(interactive_logons(), Err(want));
    assert_eq!(place(here), Err(want));
    assert_eq!(writers(here), Err(want));
    assert_eq!(mark_not_indexed(here), Err(want));
    #[expect(
        clippy::disallowed_types,
        reason = "an open handle is the input of final_path"
    )]
    let file = std::fs::File::open(here).expect("open the current folder");
    assert_eq!(final_path(&file), Err(want));
}
