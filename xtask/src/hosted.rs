//! Hosted edition asset limits check.
//!
//! Enforces Cloudflare Workers static assets limits:
//! - No file strictly over 25 MiB (26,214,400 bytes).
//! - No more than 20,000 files in the deployed assets tree.

use std::path::Path;

pub const MAX_FILE_BYTES: u64 = 25 * 1024 * 1024;
pub const MAX_FILES: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    pub files: usize,
    pub largest: Vec<(u64, String)>,
}

fn walk(
    current_dir: &Path,
    rel_prefix: &str,
    files: &mut Vec<(u64, String)>,
) -> Result<(), String> {
    let read_dir = std::fs::read_dir(current_dir)
        .map_err(|error| format!("cannot read directory `{}`: {error}", current_dir.display()))?;
    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|error| {
            format!(
                "cannot read directory entry in `{}`: {error}",
                current_dir.display()
            )
        })?;
        entries.push(entry);
    }
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let entry_path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();
        let rel_path = if rel_prefix.is_empty() {
            file_name_str.into_owned()
        } else {
            format!("{rel_prefix}/{file_name_str}")
        };

        let meta = std::fs::symlink_metadata(&entry_path).map_err(|error| {
            format!(
                "cannot read metadata for `{}`: {error}",
                entry_path.display()
            )
        })?;
        let file_type = meta.file_type();
        if file_type.is_symlink() {
            return Err(format!("`{}` is a symlink", entry_path.display()));
        }
        if file_type.is_dir() {
            walk(&entry_path, &rel_path, files)?;
        } else if file_type.is_file() {
            files.push((meta.len(), rel_path));
        } else {
            return Err(format!(
                "`{}` is not a regular file or directory",
                entry_path.display()
            ));
        }
    }
    Ok(())
}

fn scan(dir: &Path) -> Result<Scan, String> {
    let root_meta = std::fs::symlink_metadata(dir)
        .map_err(|error| format!("cannot read directory `{}`: {error}", dir.display()))?;
    if root_meta.file_type().is_symlink() {
        return Err(format!("`{}` is a symlink", dir.display()));
    }
    if !root_meta.is_dir() {
        return Err(format!("`{}` is not a directory", dir.display()));
    }

    let mut files = Vec::new();
    walk(dir, "", &mut files)?;
    let file_count = files.len();
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(Scan {
        files: file_count,
        largest: files,
    })
}

fn judge(scan: &Scan) -> Vec<String> {
    let mut findings = Vec::new();
    for (size, path) in &scan.largest {
        if *size > MAX_FILE_BYTES {
            findings.push(format!("file over 25 MiB: {path} ({size} bytes)"));
        }
    }
    if scan.files > MAX_FILES {
        findings.push(format!("too many files: {} (limit 20000)", scan.files));
    }
    findings
}

pub fn command(dir: &Path) -> Result<bool, String> {
    let scan = scan(dir)?;
    let total_bytes: u64 = scan
        .largest
        .iter()
        .fold(0u64, |acc, (size, _)| acc.saturating_add(*size));
    println!(
        "hosted-limits: {} files, total {total_bytes} bytes",
        scan.files
    );
    for (size, path) in scan.largest.iter().take(5) {
        println!("  {size}  {path}");
    }
    let findings = judge(&scan);
    for finding in &findings {
        eprintln!("{finding}");
    }
    Ok(findings.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = loop {
                let candidate = std::env::temp_dir().join(format!(
                    "fieldbook-xtask-hosted-{label}-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&candidate) {
                    Ok(()) => break candidate,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("temporary directory: {error}"),
                }
            };
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn create_file(&self, rel: &str, size: u64) -> PathBuf {
            let path = self.0.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            let file = std::fs::File::create(&path).expect("create file");
            if size > 0 {
                file.set_len(size).expect("set_len");
            }
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn hosted_file_of_exactly_25_mib_passes() {
        let temp = TempDir::new("exact-limit");
        temp.create_file("exact.bin", MAX_FILE_BYTES);

        let scan_result = scan(temp.path()).expect("scan succeeds");
        assert_eq!(scan_result.files, 1);
        let findings = judge(&scan_result);
        assert!(findings.is_empty());

        let result = command(temp.path());
        assert_eq!(result, Ok(true));
    }

    // Negative control: a file of 25 MiB + 1 byte fails and names the file.
    // control: HO-01
    #[test]
    fn hosted_file_over_25_mib_fails_and_finding_names_the_file() {
        let temp = TempDir::new("over-limit");
        temp.create_file("over.bin", MAX_FILE_BYTES + 1);

        let scan_result = scan(temp.path()).expect("scan succeeds");
        let findings = judge(&scan_result);
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0],
            format!("file over 25 MiB: over.bin ({} bytes)", MAX_FILE_BYTES + 1)
        );

        let result = command(temp.path());
        assert_eq!(result, Ok(false));
    }

    #[test]
    fn hosted_file_count_boundary_at_20000_and_20001() {
        let temp = TempDir::new("count-limit");
        for i in 0..20_000 {
            let name = format!("f_{i:05}.txt");
            temp.create_file(&name, 0);
        }

        let scan_result = scan(temp.path()).expect("scan 20000 files");
        assert_eq!(scan_result.files, 20_000);
        let findings = judge(&scan_result);
        assert!(findings.is_empty());
        assert_eq!(command(temp.path()), Ok(true));

        temp.create_file("f_20000.txt", 0);
        let scan_over = scan(temp.path()).expect("scan 20001 files");
        assert_eq!(scan_over.files, 20_001);
        let findings_over = judge(&scan_over);
        assert_eq!(
            findings_over,
            vec!["too many files: 20001 (limit 20000)".to_string()]
        );
        assert_eq!(command(temp.path()), Ok(false));
    }

    #[test]
    fn hosted_five_largest_descending_with_ties_by_path() {
        let temp = TempDir::new("five-largest");
        temp.create_file("tiny.txt", 100);
        temp.create_file("small.txt", 200);
        temp.create_file("b_tie.txt", 500);
        temp.create_file("a_tie.txt", 500);
        temp.create_file("large.txt", 700);
        temp.create_file("huge.txt", 900);
        temp.create_file("medium.txt", 600);

        let scan_result = scan(temp.path()).expect("scan");
        assert_eq!(scan_result.files, 7);

        let expected_top_five = [
            (900, "huge.txt".to_string()),
            (700, "large.txt".to_string()),
            (600, "medium.txt".to_string()),
            (500, "a_tie.txt".to_string()),
            (500, "b_tie.txt".to_string()),
        ];
        assert_eq!(&scan_result.largest[..5], &expected_top_five);

        let result = command(temp.path());
        assert_eq!(result, Ok(true));
    }

    #[test]
    fn hosted_nested_directories_reported_with_forward_slashes() {
        let temp = TempDir::new("nested-dirs");
        temp.create_file("root.txt", 10);
        temp.create_file("sub/nested.txt", 20);
        temp.create_file("sub/deep/inner.txt", 30);

        let scan_result = scan(temp.path()).expect("scan");
        assert_eq!(scan_result.files, 3);

        let paths: Vec<String> = scan_result.largest.iter().map(|(_, p)| p.clone()).collect();
        assert!(paths.contains(&"root.txt".to_string()));
        assert!(paths.contains(&"sub/nested.txt".to_string()));
        assert!(paths.contains(&"sub/deep/inner.txt".to_string()));

        for path in &paths {
            assert!(
                !path.contains('\\'),
                "path should use forward slashes: {path}"
            );
        }
    }

    #[test]
    fn hosted_missing_directory_is_err() {
        let temp = TempDir::new("missing-check");
        let missing = temp.path().join("does-not-exist");
        assert!(scan(&missing).is_err());
        assert!(command(&missing).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn hosted_symlink_is_err() {
        let temp = TempDir::new("symlink-check");
        let target = temp.create_file("target.txt", 10);
        let link = temp.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");

        let err = scan(temp.path()).expect_err("symlink must fail");
        assert!(
            err.contains("link.txt"),
            "error should name the symlink path: {err}"
        );
        assert!(command(temp.path()).is_err());
    }
}
