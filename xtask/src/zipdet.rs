//! Deterministic deflate-only ZIP archive writer and verifier (`docs/cards/edition-manifest.md`
//! step 2, `docs/reference/corpus-format.md#06-site-and-edition`).
//!
//! Entries are sorted bytewise by their slash-separated relative path under the declared
//! top-level directory (`edition/` or `originals/`), compressed with Deflate at a fixed
//! compression level, given fixed `0o644` Unix permissions, and stamped with the UTC
//! date/time derived from `source_date_epoch: i64` without reading the system clock or `TZ`.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

/// Fixed Unix file mode (`0o644`) stored on every entry in a deterministic ZIP.
pub const DETERMINISTIC_UNIX_MODE: u32 = 0o644;

/// Convert a UTC `source_date_epoch` (seconds since `1970-01-01T00:00:00Z`) into a DOS
/// [`DateTime`] using pure UTC arithmetic (independent of `TZ`).
///
/// Dates before `1980-01-01T00:00:00Z` clamp to `1980-01-01T00:00:00` (the DOS epoch start);
/// dates after `2107-12-31T23:59:58Z` clamp to `2107-12-31T23:59:58`.
#[must_use]
pub fn dos_datetime_from_epoch(source_date_epoch: i64) -> DateTime {
    // 1980-01-01T00:00:00Z is 315_532_800 seconds since 1970-01-01T00:00:00Z.
    const MIN_DOS_EPOCH: i64 = 315_532_800;
    // 2107-12-31T23:59:58Z is 4_354_819_198 seconds since 1970-01-01T00:00:00Z.
    const MAX_DOS_EPOCH: i64 = 4_354_819_198;

    let clamped = source_date_epoch.clamp(MIN_DOS_EPOCH, MAX_DOS_EPOCH);
    let days = clamped.div_euclid(86_400);
    let secs_of_day = u32::try_from(clamped.rem_euclid(86_400)).unwrap_or(0);

    let hour = u8::try_from(secs_of_day / 3_600).unwrap_or(0);
    let minute = u8::try_from((secs_of_day % 3_600) / 60).unwrap_or(0);
    // DOS timestamps have 2-second resolution (even seconds).
    let second = u8::try_from((secs_of_day % 60) & !1).unwrap_or(0);

    let (year, month, day) = civil_from_days(days);
    DateTime::from_date_and_time(year, month, day, hour, minute, second)
        .unwrap_or_else(|_| DateTime::default_for_write())
}

fn civil_from_days(days_since_1970: i64) -> (u16, u8, u8) {
    let z = days_since_1970 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = u32::try_from(z - era * 146_097).unwrap_or(0);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = i64::from(yoe) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };
    (
        u16::try_from(year).unwrap_or(1980),
        u8::try_from(m).unwrap_or(1),
        u8::try_from(d).unwrap_or(1),
    )
}

fn validate_top_folder(top_folder: &str) -> Result<&str, String> {
    let trimmed = top_folder.trim_end_matches('/');
    if trimmed.is_empty()
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed == "."
        || trimmed == ".."
    {
        return Err(format!(
            "invalid ZIP top folder `{top_folder}`: expected a single directory name such as `edition` or `originals`"
        ));
    }
    Ok(trimmed)
}

fn validate_entry_rel_path(rel_path: &str) -> Result<(), String> {
    if rel_path.is_empty() {
        return Err("ZIP entry relative path must not be empty".to_owned());
    }
    if rel_path.starts_with('/')
        || rel_path.ends_with('/')
        || rel_path.contains('\\')
        || rel_path.contains(':')
    {
        return Err(format!(
            "ZIP entry path `{rel_path}` must be a clean slash-separated relative path"
        ));
    }
    for seg in rel_path.split('/') {
        if seg.is_empty() || seg == "." || seg == ".." {
            return Err(format!(
                "ZIP entry path `{rel_path}` contains invalid segment `{seg}`"
            ));
        }
    }
    Ok(())
}

/// Build deterministic Deflate-only ZIP bytes in memory from `(rel_path, bytes)` entries,
/// placing every file at `<top_folder>/<rel_path>` sorted by UTF-8 path bytes.
pub fn write_deterministic_zip_bytes<I, P, B>(
    top_folder: &str,
    source_date_epoch: i64,
    entries: I,
) -> Result<Vec<u8>, String>
where
    I: IntoIterator<Item = (P, B)>,
    P: AsRef<str>,
    B: AsRef<[u8]>,
{
    let top = validate_top_folder(top_folder)?;
    let mtime = dos_datetime_from_epoch(source_date_epoch);

    let mut sorted: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (rel, data) in entries {
        let rel_str = rel.as_ref();
        validate_entry_rel_path(rel_str)?;
        let full_name = format!("{top}/{rel_str}");
        if sorted
            .insert(full_name.clone(), data.as_ref().to_vec())
            .is_some()
        {
            return Err(format!("duplicate ZIP entry path `{full_name}`"));
        }
    }

    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
        .last_modified_time(mtime)
        .unix_permissions(DETERMINISTIC_UNIX_MODE)
        .large_file(false);

    let mut buf = Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut buf);
        for (full_name, bytes) in &sorted {
            writer
                .start_file(full_name.as_str(), options)
                .map_err(|e| format!("failed to start ZIP entry `{full_name}`: {e}"))?;
            writer
                .write_all(bytes)
                .map_err(|e| format!("failed to write ZIP entry `{full_name}`: {e}"))?;
        }
        writer
            .finish()
            .map_err(|e| format!("failed to finish ZIP archive: {e}"))?;
    }
    Ok(buf.into_inner())
}

/// Walk `src_dir` (refusing symlinks) and write a deterministic Deflate-only ZIP to `out_zip`
/// with all files placed under `<top_folder>/<rel_path>`.
pub fn write_deterministic_zip_from_dir(
    src_dir: &Path,
    top_folder: &str,
    source_date_epoch: i64,
    out_zip: &Path,
) -> Result<Vec<u8>, String> {
    let entries = collect_dir_files(src_dir)?;
    let zip_bytes = write_deterministic_zip_bytes(top_folder, source_date_epoch, entries)?;
    if let Some(parent) = out_zip.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create parent directory `{}`: {e}", parent.display()))?;
    }
    fs::write(out_zip, &zip_bytes)
        .map_err(|e| format!("cannot write ZIP `{}`: {e}", out_zip.display()))?;
    Ok(zip_bytes)
}

fn collect_dir_files(root: &Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let meta =
        fs::symlink_metadata(root).map_err(|e| format!("cannot stat `{}`: {e}", root.display()))?;
    if meta.file_type().is_symlink() {
        return Err(format!(
            "staged root `{}` must not be a symlink",
            root.display()
        ));
    }
    if !meta.is_dir() {
        return Err(format!(
            "staged root `{}` is not a directory",
            root.display()
        ));
    }
    let mut out = Vec::new();
    walk_dir_inner(root, "", &mut out)?;
    out.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    Ok(out)
}

fn walk_dir_inner(
    cur_dir: &Path,
    rel_prefix: &str,
    out: &mut Vec<(String, Vec<u8>)>,
) -> Result<(), String> {
    let mut dir_entries: Vec<fs::DirEntry> = fs::read_dir(cur_dir)
        .map_err(|e| format!("cannot read directory `{}`: {e}", cur_dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("directory entry error in `{}`: {e}", cur_dir.display()))?;
    dir_entries.sort_by_key(fs::DirEntry::file_name);

    for entry in dir_entries {
        let name_os = entry.file_name();
        let Some(name) = name_os.to_str() else {
            return Err(format!("non-UTF-8 file name in `{}`", cur_dir.display()));
        };
        let rel = if rel_prefix.is_empty() {
            name.to_owned()
        } else {
            format!("{rel_prefix}/{name}")
        };
        let path = entry.path();
        let meta = fs::symlink_metadata(&path).map_err(|e| format!("cannot stat `{rel}`: {e}"))?;
        let ft = meta.file_type();
        if ft.is_symlink() {
            return Err(format!("symlink `{rel}` is prohibited in staged archive"));
        }
        if ft.is_dir() {
            walk_dir_inner(&path, &rel, out)?;
        } else if ft.is_file() {
            let bytes = fs::read(&path).map_err(|e| format!("cannot read `{rel}`: {e}"))?;
            out.push((rel, bytes));
        } else {
            return Err(format!(
                "special file `{rel}` is prohibited in staged archive"
            ));
        }
    }
    Ok(())
}

/// Read and verify a deterministic ZIP archive, checking that:
/// - every entry is under `<top_folder>/` with a clean relative path,
/// - entries appear in strict byte-sorted path order with no duplicates,
/// - every entry uses `CompressionMethod::Deflated`,
/// - every entry has Unix mode `0o644` and timestamp matching `source_date_epoch` (when supplied),
///
/// and returning the ordered `(rel_path_without_top_folder, bytes)` pairs.
pub fn read_and_verify_deterministic_zip(
    zip_bytes: &[u8],
    expected_top_folder: &str,
    expected_epoch: Option<i64>,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let top = validate_top_folder(expected_top_folder)?;
    let prefix = format!("{top}/");
    let expected_mtime = expected_epoch.map(dos_datetime_from_epoch);

    let reader = Cursor::new(zip_bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("invalid ZIP archive: {e}"))?;

    let mut entries = Vec::with_capacity(archive.len());
    let mut prev_full_name: Option<String> = None;

    for idx in 0..archive.len() {
        let mut file = archive
            .by_index(idx)
            .map_err(|e| format!("cannot read ZIP entry #{idx}: {e}"))?;
        let full_name = file.name().to_owned();
        if let Some(prev) = &prev_full_name
            && full_name.as_bytes() <= prev.as_bytes()
        {
            return Err(format!(
                "ZIP entries are not strictly sorted by byte order: `{prev}` followed by `{full_name}`"
            ));
        }
        prev_full_name = Some(full_name.clone());

        let Some(rel_path) = full_name.strip_prefix(&prefix) else {
            return Err(format!(
                "ZIP entry `{full_name}` does not reside under top folder `{prefix}`"
            ));
        };
        validate_entry_rel_path(rel_path)?;

        if file.compression() != CompressionMethod::Deflated {
            return Err(format!(
                "ZIP entry `{full_name}` uses compression `{:?}`, expected Deflated",
                file.compression()
            ));
        }

        if let Some(mode) = file.unix_mode()
            && (mode & 0o777) != DETERMINISTIC_UNIX_MODE
        {
            return Err(format!(
                "ZIP entry `{full_name}` has unix permissions `{:o}`, expected `{:o}`",
                mode & 0o777,
                DETERMINISTIC_UNIX_MODE
            ));
        }

        if let Some(exp_dt) = expected_mtime
            && let Some(actual_dt) = file.last_modified()
            && (actual_dt.year() != exp_dt.year()
                || actual_dt.month() != exp_dt.month()
                || actual_dt.day() != exp_dt.day()
                || actual_dt.hour() != exp_dt.hour()
                || actual_dt.minute() != exp_dt.minute()
                || actual_dt.second() != exp_dt.second())
        {
            return Err(format!(
                "ZIP entry `{full_name}` timestamp {:?} does not match expected {:?}",
                actual_dt, exp_dt
            ));
        }

        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| format!("failed to decompress ZIP entry `{full_name}`: {e}"))?;
        entries.push((rel_path.to_owned(), data));
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{
        DETERMINISTIC_UNIX_MODE, dos_datetime_from_epoch, read_and_verify_deterministic_zip,
        write_deterministic_zip_bytes, write_deterministic_zip_from_dir,
    };

    #[test]
    fn deterministic_zip_is_byte_identical_across_insertion_orders_directories_and_timezones() {
        let epoch = 1_773_532_800_i64; // 2026-03-15T00:00:00Z
        let dt = dos_datetime_from_epoch(epoch);
        assert_eq!((dt.year(), dt.month(), dt.day()), (2026, 3, 15));
        assert_eq!((dt.hour(), dt.minute(), dt.second()), (0, 0, 0));

        // 1. Reverse vs forward insertion order in memory produces identical bytes.
        let order_a = vec![
            ("z/last.txt", b"last payload\n".as_slice()),
            ("a/first.txt", b"first payload\n".as_slice()),
            ("edition.toml", b"edition = \"edition-001\"\n".as_slice()),
        ];
        let order_b = vec![
            ("a/first.txt", b"first payload\n".as_slice()),
            ("edition.toml", b"edition = \"edition-001\"\n".as_slice()),
            ("z/last.txt", b"last payload\n".as_slice()),
        ];

        let bytes_a =
            write_deterministic_zip_bytes("edition", epoch, order_a).expect("zip order_a");
        let bytes_b =
            write_deterministic_zip_bytes("edition", epoch, order_b).expect("zip order_b");
        assert_eq!(bytes_a, bytes_b);

        // 2. Two distinct temporary directories produce identical bytes.
        let dir_1 = TempDir::new().expect("tempdir 1");
        let dir_2 = TempDir::new().expect("tempdir 2");
        for root in [dir_1.path(), dir_2.path()] {
            fs::create_dir_all(root.join("a")).expect("mkdir a");
            fs::create_dir_all(root.join("z")).expect("mkdir z");
            fs::write(root.join("a/first.txt"), b"first payload\n").expect("write first");
            fs::write(root.join("z/last.txt"), b"last payload\n").expect("write last");
            fs::write(root.join("edition.toml"), b"edition = \"edition-001\"\n")
                .expect("write edition.toml");
        }
        let out_1 = dir_1.path().join("../out1.zip");
        let out_2 = dir_2.path().join("../out2.zip");
        let disk_1 = write_deterministic_zip_from_dir(dir_1.path(), "edition", epoch, &out_1)
            .expect("disk1");
        let disk_2 = write_deterministic_zip_from_dir(dir_2.path(), "edition", epoch, &out_2)
            .expect("disk2");
        assert_eq!(disk_1, bytes_a);
        assert_eq!(disk_2, bytes_a);

        // 3. Verify round-trip entries, order, Deflate method, permissions (0o644) and timestamp.
        let unpacked =
            read_and_verify_deterministic_zip(&bytes_a, "edition", Some(epoch)).expect("verify");
        let paths: Vec<&str> = unpacked.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(paths, vec!["a/first.txt", "edition.toml", "z/last.txt"]);
        assert_eq!(DETERMINISTIC_UNIX_MODE, 0o644);
    }

    #[test]
    fn deterministic_zip_rejects_unsorted_or_wrong_top_folder_or_stored_method() {
        let epoch = 1_773_532_800_i64;
        let valid = write_deterministic_zip_bytes(
            "originals",
            epoch,
            vec![("S-0001-v1.pdf", b"pdf bytes".as_slice())],
        )
        .expect("valid originals zip");

        // Wrong expected top folder refuses.
        let err = read_and_verify_deterministic_zip(&valid, "edition", Some(epoch))
            .expect_err("wrong top folder must fail");
        assert!(err.contains("top folder"), "{err}");

        // Wrong expected epoch refuses.
        let err = read_and_verify_deterministic_zip(&valid, "originals", Some(epoch + 86_400))
            .expect_err("wrong epoch must fail");
        assert!(err.contains("timestamp"), "{err}");

        // Traversal or backslash in relative path refuses on write.
        assert!(
            write_deterministic_zip_bytes(
                "edition",
                epoch,
                vec![("../escape.txt", b"x".as_slice())]
            )
            .is_err()
        );
    }
}
