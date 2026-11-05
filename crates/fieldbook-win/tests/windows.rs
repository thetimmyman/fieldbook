//! Windows system-call tests for `fieldbook-win`.

#![cfg(windows)]
#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "the test makes a junction and prints the logon count"
)]

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use std::process::Command;

use fieldbook_win::{
    Folder, Place, WriterKind, in_group, interactive_logons, known_folder, mark_not_indexed, place,
    principal, writers,
};

#[test]
fn known_folders_are_absolute_existing_folders() {
    for folder in [Folder::ProgramData, Folder::LocalAppData] {
        let path = known_folder(folder).expect("known folder resolves");
        assert!(path.is_absolute(), "{path:?} must be absolute");
        assert!(path.is_dir(), "{path:?} must exist and be a directory");
    }
}

#[test]
fn the_principal_is_a_domain_and_a_name() {
    let p = principal().expect("principal resolves");
    let (domain, name) = p.split_once('\\').expect("principal has DOMAIN\\name form");
    assert!(!domain.is_empty(), "domain is non-empty in {p}");
    assert!(!name.is_empty(), "name is non-empty in {p}");
}

#[test]
fn in_group_knows_everyone_and_not_a_missing_group() {
    assert_eq!(in_group("Everyone"), Ok(true));
    assert!(
        in_group("FieldbookMissingGroup_9f8e7d6c5b4a").is_err(),
        "an unresolved group name must be Err, not Ok(false)"
    );
}

#[test]
fn in_group_accepts_a_sid_string() {
    assert_eq!(in_group("S-1-1-0"), Ok(true));
}

#[test]
fn in_group_rejects_a_malformed_sid() {
    assert!(
        in_group("S-1-not-a-sid").is_err(),
        "a malformed SID string must be Err"
    );
}

#[test]
fn in_group_accepts_an_account_name() {
    assert_eq!(in_group("Everyone"), Ok(true));
}

#[test]
fn interactive_logons_gives_a_count() {
    let count = interactive_logons().expect("interactive_logons succeeds");
    println!("interactive_logons: {count}");
}

#[test]
fn a_plain_temporary_folder_has_no_place_facts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let plain = Place {
        reparse: false,
        sync_root: false,
        network: false,
    };
    assert_eq!(place(tmp.path()), Ok(plain));
    let missing_child = tmp.path().join("does-not-exist").join("nested");
    assert_eq!(place(&missing_child), Ok(plain));
}

#[test]
fn a_junction_is_a_reparse_point() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("target");
    let link = tmp.path().join("junction");
    fs::create_dir(&target).expect("create target dir");
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(&target)
        .status()
        .expect("cmd /C mklink /J starts");
    assert!(status.success(), "mklink /J succeeded");

    let link_place = place(&link).expect("place on junction");
    assert!(link_place.reparse, "junction must report reparse = true");
    assert!(!link_place.network, "local junction is not network");

    let child_under_junction = link.join("uncreated-child");
    let child_place = place(&child_under_junction).expect("place on child under junction");
    assert!(
        child_place.reparse,
        "child below a junction must report reparse = true"
    );
}

#[test]
fn a_unc_path_is_a_network_path() {
    let unc = Path::new(r"\\server\share\folder");
    let p = place(unc).expect("place on synthetic UNC path");
    assert!(p.network, "UNC path must report network = true");
}

#[test]
fn a_verbatim_local_path_is_not_a_network_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let canonical = fs::canonicalize(tmp.path()).expect("canonicalize tempdir");
    let s = canonical.to_string_lossy();
    assert!(
        s.starts_with(r"\\?\"),
        "canonical local path {s} starts with verbatim prefix"
    );
    let p = place(&canonical).expect("place on verbatim local path");
    assert!(
        !p.network,
        "verbatim local path {s} must not be treated as a network path"
    );
}

#[test]
fn a_verbatim_unc_path_is_a_network_path() {
    let unc = Path::new(r"\\?\UNC\server\share\folder");
    let p = place(unc).expect("place on synthetic verbatim UNC path");
    assert!(p.network, "verbatim UNC path must report network = true");
}

#[test]
fn the_hosts_file_is_written_by_administrators_and_system_only() {
    let hosts = Path::new(r"C:\Windows\System32\drivers\etc\hosts");
    let list = writers(hosts).expect("writers on hosts file");
    assert!(!list.is_empty(), "hosts file has at least one writer");
    for w in &list {
        assert!(
            matches!(w, WriterKind::Administrators | WriterKind::System),
            "unexpected writer {w:?} on hosts file: {list:?}"
        );
    }
}

#[test]
fn a_file_the_user_can_write_lists_an_other_writer() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("writable.txt");
    fs::write(&file, b"hello").expect("write file");
    let list = writers(&file).expect("writers on user file");
    assert!(
        list.contains(&WriterKind::Other)
            || list.contains(&WriterKind::Administrators)
            || list.contains(&WriterKind::System),
        "user file lists writers: {list:?}"
    );
    // Grant an explicit allow ACE to Everyone (`*S-1-1-0`) so a non-admin/non-system writer is present
    // even when the CI runner executes as ContainerAdministrator or Administrator.
    let status = Command::new("icacls")
        .arg(&file)
        .args(["/grant", "*S-1-1-0:(W)"])
        .status()
        .expect("icacls runs");
    assert!(status.success(), "icacls /grant Everyone succeeded");
    let after = writers(&file).expect("writers after granting Everyone write");
    assert!(
        after.contains(&WriterKind::Other),
        "expected WriterKind::Other in {after:?}"
    );
}

#[test]
fn mark_not_indexed_sets_the_flag() {
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
    const FILE_ATTRIBUTE_NOT_CONTENT_INDEXED: u32 = 0x2000;

    let tmp = tempfile::tempdir().expect("tempdir");
    let folder = tmp.path().join("unindexed");
    fs::create_dir(&folder).expect("create folder");
    mark_not_indexed(&folder).expect("mark_not_indexed succeeds");

    let attrs = fs::metadata(&folder).expect("metadata").file_attributes();
    assert_ne!(
        attrs & FILE_ATTRIBUTE_NOT_CONTENT_INDEXED,
        0,
        "FILE_ATTRIBUTE_NOT_CONTENT_INDEXED (0x2000) must be set in {attrs:#x}"
    );
    assert_ne!(
        attrs & FILE_ATTRIBUTE_DIRECTORY,
        0,
        "FILE_ATTRIBUTE_DIRECTORY (0x10) must be preserved in {attrs:#x}"
    );
}
