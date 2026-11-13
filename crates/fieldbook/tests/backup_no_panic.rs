//! No-panic fuzz/property test over `BackupToml::parse` and `BackupSettings::parse` (`backup-snapshot` step 11).

#[path = "../../../tests/support/rng.rs"]
mod rng;

mod backup_no_panic {
    use fieldbook::io::state::BackupSettings;
    use fieldbook_core::backup::BackupToml;

    use super::rng::Rng;

    const VALID_BACKUP_TOML: &str = r#"backup_version = 1
tool_version = "0.1.0"
snapshot = 1
created_utc = "2026-03-01T12:00:00Z"
labels = ["public"]
roots = ["corpus", "state"]
files = 10
bytes = 2048
fingerprint = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
journal_seq = 1
journal_head = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
journal_state = "ok"
manifest_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
"#;

    const VALID_SETTINGS_TOML: &str = r#"[backup]
default = "main"
warn_after_days = 7
keep_last = 7
keep_weekly = 4

[[backup.target]]
id = "main"
kind = "folder"
path = "/var/backups"
include = ["all"]
company_storage = false
"#;

    #[test]
    fn parser_never_panics_on_200_seeded_inputs() {
        assert!(BackupToml::parse(VALID_BACKUP_TOML).is_ok());
        assert!(BackupSettings::parse(VALID_SETTINGS_TOML).is_ok());

        let mut rng = Rng::new(1);
        for _ in 0..200 {
            let len = usize::try_from(rng.next_u64() % 129).unwrap_or(0);
            let bytes = rng.bytes(len);
            let text = String::from_utf8_lossy(&bytes);
            let _ = BackupToml::parse(&text);
            let _ = BackupSettings::parse(&text);
        }

        let backup_bytes = VALID_BACKUP_TOML.as_bytes();
        for i in 0..backup_bytes.len() {
            let mut flipped = backup_bytes.to_vec();
            flipped[i] ^= 0x5a;
            let flipped_text = String::from_utf8_lossy(&flipped);
            let _ = BackupToml::parse(&flipped_text);
            let _ = BackupSettings::parse(&flipped_text);

            let mut deleted = backup_bytes.to_vec();
            deleted.remove(i);
            let deleted_text = String::from_utf8_lossy(&deleted);
            let _ = BackupToml::parse(&deleted_text);
            let _ = BackupSettings::parse(&deleted_text);
        }

        let settings_bytes = VALID_SETTINGS_TOML.as_bytes();
        for i in 0..settings_bytes.len() {
            let mut flipped = settings_bytes.to_vec();
            flipped[i] ^= 0x5a;
            let flipped_text = String::from_utf8_lossy(&flipped);
            let _ = BackupToml::parse(&flipped_text);
            let _ = BackupSettings::parse(&flipped_text);

            let mut deleted = settings_bytes.to_vec();
            deleted.remove(i);
            let deleted_text = String::from_utf8_lossy(&deleted);
            let _ = BackupToml::parse(&deleted_text);
            let _ = BackupSettings::parse(&deleted_text);
        }
    }
}
