//! Peak-memory integration test for `fieldbook backup` on Linux.
//!
//! Spawns the real `fieldbook` binary so `/proc/<pid>/status` `VmHWM` reflects
//! only the backup process and not the Rust test harness or sibling tests.

#![cfg(target_os = "linux")]
#![expect(
    clippy::expect_used,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "integration test spawns the compiled fieldbook binary and samples /proc/<pid>/status"
)]

mod support;

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use fieldbook_core::hash::sha256_hex;
use fieldbook_core::manifest::SupportCatalogue;

const MIB: u64 = 1024 * 1024;
const FILE_COUNT: usize = 10;
const FILE_SIZE_BYTES: usize = 20 * 1024 * 1024;
const BOUND_MULTIPLE: u64 = 4;
const HEADROOM_BYTES: u64 = 64 * MIB;
const MAX_DURATION: Duration = Duration::from_secs(60);

struct BackupFixture {
    tree_dir: tempfile::TempDir,
    state_dir: PathBuf,
    target_dir: tempfile::TempDir,
    cat_dir: tempfile::TempDir,
    cat_file: PathBuf,
    cat_sha: String,
    home_dir: tempfile::TempDir,
}

fn setup_fixture(large_files: bool) -> BackupFixture {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_mini = repo.join("tests/fixtures/corpus-mini");
    let state_mini = repo.join("tests/fixtures/state-mini");

    let tree_dir = support::temp_copy(&corpus_mini);
    let state_dir = tree_dir.path().join(".fieldbook");
    support::copy_dir_recursive(&state_mini, &state_dir).expect("copy state-mini into .fieldbook");

    let target_dir = tempfile::tempdir().expect("create target tempdir");
    let settings_path = state_dir.join("settings.toml");
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_dir.path().to_string_lossy().replace('\\', "/")
    );
    std::fs::write(&settings_path, settings_text).expect("write settings.toml");

    if large_files {
        let chunk = vec![0x42u8; FILE_SIZE_BYTES];
        let chunk_sha = sha256_hex(&chunk);
        let sources_dir = tree_dir.path().join("corpus/sources/public");
        let sources_csv = tree_dir.path().join("corpus/registers/sources.csv");
        let mut sources_file = OpenOptions::new()
            .append(true)
            .open(&sources_csv)
            .expect("open sources.csv");
        for i in 0..FILE_COUNT {
            let sid = format!("S-900{i}");
            let fname = format!("{sid}-v1.bin");
            std::fs::write(sources_dir.join(&fname), &chunk).expect("write 20 MiB source");
            writeln!(
                sources_file,
                "{sid},1,synthetic-perf,\"Large {i}\",PerfBoard,primary_law,primary_evidence,A,\"Synthetic\",synthetic_perf,reviewed,{fname},{chunk_sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,active,,V2,public,Note,Purpose,,,,,"
            )
            .expect("append sources.csv row");
        }
    }

    let tree = fieldbook::io::read_tree(tree_dir.path()).expect("read tree");
    let cat =
        SupportCatalogue::from_public_source(&tree, "1111111111111111111111111111111111111111")
            .expect("build support catalogue");
    let (cat_bytes, cat_sha) = cat.to_toml_bytes_and_sha256();

    let cat_dir = tempfile::tempdir().expect("create support catalogue dir");
    let cat_file = cat_dir.path().join("corpus-support.toml");
    std::fs::write(&cat_file, &cat_bytes).expect("write support catalogue");

    let home_dir = tempfile::tempdir().expect("create home dir");

    BackupFixture {
        tree_dir,
        state_dir,
        target_dir,
        cat_dir,
        cat_file,
        cat_sha,
        home_dir,
    }
}

fn vmhwm_kb(status: &str) -> Option<u64> {
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let trimmed = rest.trim();
            let num = trimmed
                .strip_suffix("kB")
                .or_else(|| trimmed.strip_suffix("kb"))
                .unwrap_or(trimmed)
                .trim();
            if let Ok(val) = num.parse::<u64>() {
                return Some(val);
            }
        }
    }
    None
}

fn run_raw_verify(fix: &BackupFixture, bin: &str) {
    let output = Command::new(bin)
        .current_dir(fix.tree_dir.path())
        .env("FIELDBOOK_STATE", &fix.state_dir)
        .env("HOME", fix.home_dir.path())
        .args(["backup", "verify", "--raw", "."])
        .arg("--support-catalogue")
        .arg(&fix.cat_file)
        .args(["--expect-support-sha256", &fix.cat_sha])
        .output()
        .expect("run fieldbook backup verify --raw");
    assert_eq!(
        output.status.code(),
        Some(0),
        "raw verify failed ({:?}):\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_backup_and_measure_vmhwm(fix: &BackupFixture, bin: &str) -> (u64, Duration) {
    let start = Instant::now();
    let mut child = Command::new(bin)
        .current_dir(fix.tree_dir.path())
        .env("FIELDBOOK_STATE", &fix.state_dir)
        .env("HOME", fix.home_dir.path())
        .args(["backup", "--target", "main"])
        .spawn()
        .expect("spawn fieldbook backup");

    let status_path = format!("/proc/{}/status", child.id());
    let mut peak_kb: u64 = 0;

    let exit_status = loop {
        if let Ok(text) = std::fs::read_to_string(&status_path)
            && let Some(kb) = vmhwm_kb(&text)
        {
            peak_kb = peak_kb.max(kb);
        }
        match child.try_wait().expect("try_wait child") {
            Some(status) => break status,
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };

    if let Ok(text) = std::fs::read_to_string(&status_path)
        && let Some(kb) = vmhwm_kb(&text)
    {
        peak_kb = peak_kb.max(kb);
    }

    let elapsed = start.elapsed();
    assert_eq!(
        exit_status.code(),
        Some(0),
        "fieldbook backup failed with exit {:?}",
        exit_status.code()
    );
    assert!(peak_kb > 0, "failed to observe non-zero VmHWM for pid");
    assert!(
        fix.target_dir.path().join("fieldbook-backup-0001").is_dir(),
        "snapshot 1 directory missing"
    );
    (peak_kb, elapsed)
}

#[test]
fn backup_peak_memory_is_bounded() {
    let bin = env!("CARGO_BIN_EXE_fieldbook");
    let _ = &Path::new(bin);

    // Baseline measurement on unmodified corpus-mini.
    let base_fix = setup_fixture(false);
    let _keep_base_cat = &base_fix.cat_dir;
    run_raw_verify(&base_fix, bin);
    let (baseline_kb, base_elapsed) = run_backup_and_measure_vmhwm(&base_fix, bin);

    // 200 MiB measurement: 10 x 20 MiB files.
    let large_fix = setup_fixture(true);
    let _keep_large_cat = &large_fix.cat_dir;
    run_raw_verify(&large_fix, bin);
    let (peak_kb, large_elapsed) = run_backup_and_measure_vmhwm(&large_fix, bin);

    let baseline_bytes = baseline_kb.saturating_mul(1024);
    let peak_bytes = peak_kb.saturating_mul(1024);
    let bound_bytes = baseline_bytes + BOUND_MULTIPLE * (FILE_SIZE_BYTES as u64) + HEADROOM_BYTES;

    println!(
        "backup_peak_memory_is_bounded: baseline={} KiB ({:.2} MiB, {:?}), peak={} KiB ({:.2} MiB, {:?}), bound={} KiB ({:.2} MiB)",
        baseline_kb,
        baseline_bytes as f64 / MIB as f64,
        base_elapsed,
        peak_kb,
        peak_bytes as f64 / MIB as f64,
        large_elapsed,
        bound_bytes / 1024,
        bound_bytes as f64 / MIB as f64,
    );

    assert!(
        large_elapsed <= MAX_DURATION,
        "200 MiB backup took {large_elapsed:?}, exceeding {MAX_DURATION:?}"
    );
    assert!(
        peak_bytes <= bound_bytes,
        "peak memory {peak_bytes} bytes ({:.2} MiB) exceeded bound {bound_bytes} bytes ({:.2} MiB) (baseline {baseline_bytes} bytes)",
        peak_bytes as f64 / MIB as f64,
        bound_bytes as f64 / MIB as f64,
    );
}
