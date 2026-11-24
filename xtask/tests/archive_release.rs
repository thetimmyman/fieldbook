#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration test builds synthetic git repositories and invokes the xtask binary"
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use fieldbook_core::audit::Now;
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Entry, Host, Options, Tree, load};
use fieldbook_core::date::CivilDate;
use fieldbook_core::edition::{BuildInfo, BuildProfile, EditionInfo};
use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check as egress_check};
use fieldbook_core::hash::sha256_hex;
use fieldbook_core::ids::VersionId;
use fieldbook_core::manifest::{SupportCatalogue, shipped_corpus, write_sums};
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_core::schema::Register;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

static COUNTER: AtomicU64 = AtomicU64::new(1);

const MINI_CORPUS_TOML: &str = include_str!("../../tests/fixtures/corpus-mini/corpus/corpus.toml");

const EMPTY_BASELINES: &str = "\
baseline_schema = 1

[compatibility]
kind = \"empty\"

[tool]
kind = \"empty\"

[edition]
kind = \"empty\"
";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("xtask lives under repo root"))
        .to_path_buf()
}

fn xtask_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_xtask"))
}

fn run_xtask(args: &[&str], cwd: &Path, repo_root_override: Option<&Path>) -> Output {
    let mut cmd = Command::new(xtask_bin());
    cmd.args(args).current_dir(cwd);
    if let Some(root) = repo_root_override {
        cmd.env("XTASK_REPO_ROOT", root);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("run xtask {args:?}: {e}"))
}

fn assert_success(out: &Output, context: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "{context} expected exit 0, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status
    );
}

fn assert_failure(out: &Output, context: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "{context} expected non-zero exit, got {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        out.status
    );
}

fn assert_stdout_contains(out: &Output, needle: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stdout.contains(needle),
        "expected stdout to contain `{needle}`\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn assert_stderr_contains(out: &Output, needle: &str) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(needle),
        "expected stderr to contain `{needle}`\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

fn assert_stderr_contains_any(out: &Output, needles: &[&str]) {
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    let found = needles.iter().any(|n| stderr.contains(n));
    assert!(
        found,
        "expected stderr to contain one of {needles:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
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

fn dos_datetime_from_epoch(source_date_epoch: i64) -> DateTime {
    const MIN_DOS_EPOCH: i64 = 315_532_800;
    const MAX_DOS_EPOCH: i64 = 4_354_819_198;

    let clamped = source_date_epoch.clamp(MIN_DOS_EPOCH, MAX_DOS_EPOCH);
    let days = clamped.div_euclid(86_400);
    let secs_of_day = u32::try_from(clamped.rem_euclid(86_400)).unwrap_or(0);

    let hour = u8::try_from(secs_of_day / 3_600).unwrap_or(0);
    let minute = u8::try_from((secs_of_day % 3_600) / 60).unwrap_or(0);
    let second = u8::try_from((secs_of_day % 60) & !1).unwrap_or(0);

    let (year, month, day) = civil_from_days(days);
    DateTime::from_date_and_time(year, month, day, hour, minute, second)
        .unwrap_or_else(|_| DateTime::default_for_write())
}

fn write_deterministic_zip_bytes<I, P, B>(
    top_folder: &str,
    source_date_epoch: i64,
    entries: I,
) -> Result<Vec<u8>, String>
where
    I: IntoIterator<Item = (P, B)>,
    P: AsRef<str>,
    B: AsRef<[u8]>,
{
    let top = top_folder.trim_end_matches('/');
    let mtime = dos_datetime_from_epoch(source_date_epoch);

    let mut sorted: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (rel, data) in entries {
        let rel_str = rel.as_ref();
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
        .unix_permissions(0o644)
        .large_file(false);

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut writer = ZipWriter::new(&mut buf);
        for (full_name, bytes) in &sorted {
            writer
                .start_file(full_name.as_str(), options)
                .map_err(|e| format!("failed to start ZIP entry `{full_name}`: {e}"))?;
            std::io::Write::write_all(&mut writer, bytes)
                .map_err(|e| format!("failed to write ZIP entry `{full_name}`: {e}"))?;
        }
        writer
            .finish()
            .map_err(|e| format!("failed to finish ZIP archive: {e}"))?;
    }
    Ok(buf.into_inner())
}

fn valid_summary_text(tag: &str) -> String {
    format!(
        "edition: {tag}\n\
         browser: Microsoft Edge 130.0.0.0\n\
         options: --headless=new --dump-dom --log-net-log=<file> --disable-background-networking\n\
         pages loaded: 2\n\
         text-renamed originals loaded: 1\n\
         requests started by those loads: 12\n\
         requests to file: URLs: 12\n\
         requests to any other scheme or host: 0\n\
         self-test: pass\n\
         negative page, requests blocked: 1\n\
         negative page, requests reported: 1\n\
         verdict: pass\n"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryKind {
    Raw,
    Site,
}

#[derive(Clone)]
struct SyntheticSourceSpec {
    source_id: &'static str,
    version: u32,
    lifecycle: &'static str,
    supersedes: &'static str,
    note: &'static str,
    ship_payload: bool,
}

fn default_source_specs() -> Vec<SyntheticSourceSpec> {
    vec![SyntheticSourceSpec {
        source_id: "S-0001",
        version: 1,
        lifecycle: "active",
        supersedes: "",
        note: "Synthetic active source.",
        ship_payload: true,
    }]
}

fn build_synthetic_edition_set(
    out_dir: &Path,
    edition_num: &str,
    commit_sha: &str,
    kind: DeliveryKind,
    sources: &[SyntheticSourceSpec],
    summary_override: Option<&str>,
) {
    fs::create_dir_all(out_dir).unwrap_or_else(|e| panic!("mkdir edition set: {e}"));
    let epoch = 1_773_532_800_i64;
    let today = CivilDate::from_ymd(2026, 3, 15).unwrap_or_else(|e| panic!("date: {e}"));
    let assistant_name = fieldbook_core::strings::paths()[6];

    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
    tree.insert_bytes(format!("corpus/{assistant_name}"), b"rules_version = 1\n");
    tree.insert_bytes(
        "corpus/changes.md",
        format!(
            "# Corpus changes\n\nSynthetic changes log.\n\n## edition-{edition_num}\n\n- Added: synthetic.\n"
        )
        .into_bytes(),
    );
    tree.insert_dir("corpus/registers");
    for reg in Register::ALL {
        let mut header = String::new();
        for (idx, col) in reg.columns().iter().enumerate() {
            if idx > 0 {
                header.push(',');
            }
            header.push_str(col.name);
        }
        header.push('\n');
        tree.insert_bytes(
            format!("corpus/registers/{}", reg.file_name()),
            header.into_bytes(),
        );
    }

    let mut sources_csv = String::from(
        "source_id,version,tag,title,publisher,source_kind,source_role,authority_tier,licence,source_family,family_basis,location,sha256,published,effective_from,as_of,retrieved,lifecycle,supersedes,volatility,handling,note,purpose,origin_system,origin_stream,origin_key,doc_kind,series\n",
    );
    let mut quarantined = BTreeSet::new();
    let mut orig_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();

    for (idx, spec) in sources.iter().enumerate() {
        let vid_str = format!("{}-v{}", spec.source_id, spec.version);
        let vid = VersionId::parse(&vid_str).unwrap_or_else(|e| panic!("valid vid: {e}"));
        let orig_bytes = format!("original payload for {vid_str}\n").into_bytes();
        let orig_sha = sha256_hex(&orig_bytes);
        let tag_slug = format!("syn-{}", idx + 1);
        let family = format!("syn_{}", idx + 1);
        sources_csv.push_str(&format!(
            "{},{},{tag_slug},Synthetic Source {vid_str},Synthetic Board,primary_law,primary_evidence,A,Synthetic,{family},reviewed,{vid_str}.txt,{orig_sha},2026-01-10,2026-01-15,2026-02-01,2026-02-01,{},{},V2,public,{},Baseline,,,,,\n",
            spec.source_id, spec.version, spec.lifecycle, spec.supersedes, spec.note
        ));
        if spec.lifecycle == "quarantined" {
            quarantined.insert(vid.clone());
        }
        if spec.ship_payload {
            tree.insert(
                format!("corpus/sources/public/{vid_str}.txt"),
                Entry::Original {
                    sha256: orig_sha.clone(),
                    len: orig_bytes.len() as u64,
                },
            );
            let page_txt = format!("extracted text for {vid_str}\n").into_bytes();
            let page_sha = sha256_hex(&page_txt);
            let ext_toml = format!(
                "tool = \"plain\"\ntool_version = \"1\"\nmethod = \"plain\"\npage_count = 1\nthin_pages = []\noriginal_sha256 = \"{orig_sha}\"\n\n[pages]\np0001 = \"{page_sha}\"\n"
            );
            tree.insert_bytes(
                format!("corpus/sources/text/{vid_str}/extraction.toml"),
                ext_toml.into_bytes(),
            );
            tree.insert_bytes(format!("corpus/sources/text/{vid_str}/p0001.txt"), page_txt);
            orig_files.insert(format!("{vid_str}.txt"), orig_bytes);
        }
    }

    tree.insert_bytes("corpus/registers/sources.csv", sources_csv.into_bytes());
    tree.insert_bytes(
        "corpus/pages/compliance/business-systems/cmp-sample.md",
        b"+++\nid = \"cmp-sample\"\ntitle = \"Synthetic Title\"\ntype = \"concept\"\narea = \"cmp\"\nsubarea = \"business-systems\"\nstage = 3\nstatus = \"draft\"\nreadiness = \"SYNTHETIC-NOW\"\nas_of = 2026-02-01\nhandling = [\"public\"]\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSynthetic.\n\n## The rule\n\nSynthetic.\n\n## How it works\n\nSynthetic.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\n## Never confuse\n\nSynthetic.\n\n## Company calibration\n\nSynthetic.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n",
    );
    tree.insert_bytes("corpus/eval/regression.toml", b"case = []\n");

    let (model, _) = load(&Host::new(tree, today), &Options::default());
    let eff = EffectivePolicy::floor(false);
    let now = Now::new(today, 0).unwrap_or_else(|e| panic!("now: {e}"));
    let Decision::Allowed(allowed) =
        egress_check(&eff, &Dest::Edition, None, None, now, Shipped::Hours(0))
    else {
        panic!("edition allowed");
    };
    let view = LabelledView::new(&model, &allowed, &ReadableStores::corpus());
    let shipped =
        shipped_corpus(&view, &quarantined).unwrap_or_else(|e| panic!("shipped_corpus: {e}"));
    let tag_str = format!("edition-{edition_num}");
    let build_info = BuildInfo::new(
        Some(today),
        Some(commit_sha.to_owned()),
        BuildProfile::Silent,
        Some(epoch),
    );
    let ed_info = EditionInfo::from(&view, &shipped, Some(&tag_str), &build_info, "0.1.0", 1)
        .unwrap_or_else(|e| panic!("EditionInfo: {e}"));
    let (support_bytes, support_sha) = SupportCatalogue::from_public_source(&view, commit_sha)
        .unwrap_or_else(|e| panic!("SupportCatalogue: {e}"))
        .to_toml_bytes_and_sha256();

    let mut ed_files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for (p, _) in &shipped {
        ed_files.insert(
            p.clone(),
            view.tree()
                .get_bytes(p)
                .unwrap_or_else(|| panic!("bytes for {p}"))
                .to_vec(),
        );
    }
    ed_files.insert(assistant_name.to_owned(), b"rules_version = 1\n".to_vec());
    ed_files.insert("README.txt".to_owned(), b"Synthetic README\n".to_vec());
    ed_files.insert(
        "TRANSFER-STATEMENT.txt".to_owned(),
        b"Synthetic transfer statement\n".to_vec(),
    );
    ed_files.insert("edition.toml".to_owned(), ed_info.to_toml().into_bytes());
    ed_files.insert(
        "evidence/corpus-support.toml".to_owned(),
        support_bytes.clone(),
    );
    ed_files.insert(
        "intake/ai-development-statement.md".to_owned(),
        b"# AI\n".to_vec(),
    );
    ed_files.insert("intake/data-handling.md".to_owned(), b"# Data\n".to_vec());
    ed_files.insert(
        "intake/integrity-guide.md".to_owned(),
        b"# Integrity\n".to_vec(),
    );
    ed_files.insert(
        "intake/network-silence.md".to_owned(),
        b"# Network\n".to_vec(),
    );

    match kind {
        DeliveryKind::Raw => {
            let check_json = serde_json::json!({
                "report_version": 1,
                "strict": true,
                "errors": 0,
                "diagnostics": [],
                "today": "2026-03-15"
            });
            let check_bytes =
                serde_json::to_vec(&check_json).unwrap_or_else(|e| panic!("check json: {e}"));
            let check_sha = sha256_hex(&check_bytes);
            let reg_json = serde_json::json!({
                "passed": true,
                "critical_misses": 0,
                "cases": 1,
                "critical_cases": 0,
                "edition": tag_str,
                "commit": commit_sha,
                "check_sha256": check_sha,
                "today": "2026-03-15"
            });
            let reg_bytes =
                serde_json::to_vec(&reg_json).unwrap_or_else(|e| panic!("reg json: {e}"));

            ed_files.insert(
                "corpus/eval/regression.toml".to_owned(),
                b"case = []\n".to_vec(),
            );
            ed_files.insert("evidence/check.json".to_owned(), check_bytes);
            ed_files.insert("evidence/regression.json".to_owned(), reg_bytes);
        }
        DeliveryKind::Site => {
            let summary =
                summary_override.map_or_else(|| valid_summary_text(&tag_str), |s| s.to_owned());
            ed_files.insert(
                "intake/network-silence-summary.txt".to_owned(),
                summary.into_bytes(),
            );
            ed_files.insert("index.html".to_owned(), b"<!doctype html>\n".to_vec());
            ed_files.insert("about.html".to_owned(), b"<!doctype html>\n".to_vec());
            ed_files.insert(
                fieldbook_core::strings::paths()[3].to_owned(),
                b"<!doctype html>\n".to_vec(),
            );
            for data in [
                fieldbook_core::strings::PATHS[0],
                "data/conflicts.csv",
                "data/glossary.csv",
                "data/questions.csv",
                "data/sources.csv",
                "templates/discovery-log.xlsx",
                "templates/assumption-register.xlsx",
                "templates/90-day-plan.xlsx",
                "templates/decision-package.xlsx",
            ] {
                ed_files.insert(data.to_owned(), b"synthetic\n".to_vec());
            }
        }
    }

    let ed_sums = write_sums(
        &ed_files
            .iter()
            .map(|(p, b)| (p.clone(), sha256_hex(b)))
            .collect::<Vec<_>>(),
    );
    ed_files.insert("SHA256SUMS".to_owned(), ed_sums.into_bytes());

    let orig_sums = write_sums(
        &orig_files
            .iter()
            .map(|(p, b)| (p.clone(), sha256_hex(b)))
            .collect::<Vec<_>>(),
    );
    orig_files.insert("SHA256SUMS".to_owned(), orig_sums.into_bytes());

    let ed_zip_name = format!("fieldbook-edition-{edition_num}.zip");
    let orig_zip_name = format!("fieldbook-originals-{edition_num}.zip");
    let ed_zip_bytes = write_deterministic_zip_bytes("edition", epoch, &ed_files)
        .unwrap_or_else(|e| panic!("ed zip: {e}"));
    let orig_zip_bytes = write_deterministic_zip_bytes("originals", epoch, &orig_files)
        .unwrap_or_else(|e| panic!("orig zip: {e}"));

    fs::write(out_dir.join(&ed_zip_name), &ed_zip_bytes)
        .unwrap_or_else(|e| panic!("write ed zip: {e}"));
    fs::write(out_dir.join(&orig_zip_name), &orig_zip_bytes)
        .unwrap_or_else(|e| panic!("write orig zip: {e}"));

    let outer_sums = write_sums(&[
        (ed_zip_name, sha256_hex(&ed_zip_bytes)),
        (orig_zip_name, sha256_hex(&orig_zip_bytes)),
    ]);
    fs::write(out_dir.join("SHA256SUMS"), outer_sums)
        .unwrap_or_else(|e| panic!("write outer sums: {e}"));
    fs::write(out_dir.join("corpus-support.toml"), &support_bytes)
        .unwrap_or_else(|e| panic!("write support: {e}"));
    let support_sha_line = write_sums(&[("corpus-support.toml".to_owned(), support_sha)]);
    fs::write(out_dir.join("corpus-support.sha256"), support_sha_line)
        .unwrap_or_else(|e| panic!("write support.sha256: {e}"));
}

fn build_synthetic_evidence_dir(
    dir: &Path,
    baseline_toml: &str,
    mode_toml: Option<&str>,
    vector_payload: &[u8],
    identity: (&str, &str),
) -> PathBuf {
    fs::create_dir_all(dir.join("compat/calc")).unwrap_or_else(|e| panic!("mkdir: {e}"));
    let contract_bytes = b"# synthetic contract\n";
    let impl_bytes = b"// synthetic impl\n";
    let input_bytes = vector_payload;
    let expected_bytes = b"expected = 1\n";

    fs::write(dir.join("compat/calc/contract.md"), contract_bytes)
        .unwrap_or_else(|e| panic!("write: {e}"));
    fs::write(dir.join("compat/calc/impl.rs"), impl_bytes).unwrap_or_else(|e| panic!("write: {e}"));
    fs::write(dir.join("compat/calc/in.toml"), input_bytes)
        .unwrap_or_else(|e| panic!("write: {e}"));
    fs::write(dir.join("compat/calc/out.toml"), expected_bytes)
        .unwrap_or_else(|e| panic!("write: {e}"));

    let published_toml = format!(
        "compat_schema = 1\ncalculation_epoch = 1\ncase_set_sha256 = \"{}\"\n\n[[binding]]\nkind = \"calculation\"\nid = \"build_up@1\"\nrunner = \"calculation\"\nfiles = [\n  {{ role = \"contract\", path = \"compat/calc/contract.md\", sha256 = \"{}\" }},\n  {{ role = \"expected\", case = \"calc-01\", path = \"compat/calc/out.toml\", sha256 = \"{}\" }},\n  {{ role = \"implementation\", path = \"compat/calc/impl.rs\", sha256 = \"{}\" }},\n  {{ role = \"input\", case = \"calc-01\", path = \"compat/calc/in.toml\", sha256 = \"{}\" }},\n]\n",
        "11".repeat(32),
        sha256_hex(contract_bytes),
        sha256_hex(expected_bytes),
        sha256_hex(impl_bytes),
        sha256_hex(input_bytes),
    );
    fs::write(dir.join("compat/published.toml"), published_toml)
        .unwrap_or_else(|e| panic!("write: {e}"));
    fs::write(dir.join("release-baseline.toml"), baseline_toml)
        .unwrap_or_else(|e| panic!("write: {e}"));
    if let Some(mode) = mode_toml {
        fs::write(dir.join("edition-release.toml"), mode).unwrap_or_else(|e| panic!("write: {e}"));
    }
    fs::write(
        dir.join("release-identity.toml"),
        identity_toml(identity.0, identity.1),
    )
    .unwrap_or_else(|e| panic!("write: {e}"));
    dir.to_path_buf()
}

/// The canonical `release-identity.toml` the tag check writes for `tag` at `commit`.
fn identity_toml(tag: &str, commit: &str) -> String {
    format!("identity_schema = 1\ntag = \"{tag}\"\ncommit = \"{commit}\"\n")
}

fn build_synthetic_tool_package(pkg_dir: &Path, tag: &str, commit: &str) {
    fs::create_dir_all(pkg_dir).unwrap_or_else(|e| panic!("mkdir pkg: {e}"));
    let version = tag.strip_prefix('v').unwrap_or(tag);
    let desc = format!("tag = \"{tag}\"\ncommit = \"{commit}\"\n");
    fs::write(pkg_dir.join("package.toml"), desc)
        .unwrap_or_else(|e| panic!("write package.toml: {e}"));

    let script = format!("#!/bin/sh\necho \"fieldbook {version} ({commit})\"\n");
    let script_path = pkg_dir.join(fieldbook_core::strings::binary());
    fs::write(&script_path, &script).unwrap_or_else(|e| panic!("write fieldbook: {e}"));

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script_path)
            .unwrap_or_else(|e| panic!("stat fieldbook: {e}"))
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap_or_else(|e| panic!("chmod fieldbook: {e}"));
    }

    let script_bytes = fs::read(&script_path).unwrap_or_else(|e| panic!("read fieldbook: {e}"));
    let script_sha = sha256_hex(&script_bytes);
    let sums = format!("{script_sha}  fieldbook\n");
    fs::write(pkg_dir.join("SHA256SUMS"), sums).unwrap_or_else(|e| panic!("write SHA256SUMS: {e}"));
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap_or_else(|e| panic!("create_dir_all: {e}"));
    for entry in fs::read_dir(from).unwrap_or_else(|e| panic!("read_dir: {e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
        let target = to.join(entry.file_name());
        let ft = entry
            .file_type()
            .unwrap_or_else(|e| panic!("file_type: {e}"));
        if ft.is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap_or_else(|e| panic!("copy: {e}"));
        }
    }
}

fn collect_dir_hashes(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
    if !dir.exists() {
        return;
    }
    for entry in fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir: {e}")) {
        let entry = entry.unwrap_or_else(|e| panic!("entry: {e}"));
        let path = entry.path();
        let ft = entry
            .file_type()
            .unwrap_or_else(|e| panic!("file_type: {e}"));
        if ft.is_dir() {
            collect_dir_hashes(&path, base, out);
        } else if ft.is_file() {
            let rel = path
                .strip_prefix(base)
                .unwrap_or_else(|e| panic!("strip_prefix: {e}"))
                .to_str()
                .unwrap_or_else(|| panic!("utf8 path"))
                .replace('\\', "/");
            let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read: {e}"));
            out.insert(rel, sha256_hex(&bytes));
        }
    }
}

fn extract_bound_paths_from_published(published_str: &str) -> Vec<String> {
    let value: toml::Value =
        toml::from_str(published_str).unwrap_or_else(|e| panic!("parse published.toml: {e}"));
    let mut paths = Vec::new();
    if let Some(bindings) = value.get("binding").and_then(|b| b.as_array()) {
        for binding in bindings {
            if let Some(files) = binding.get("files").and_then(|f| f.as_array()) {
                for file in files {
                    if let Some(path) = file.get("path").and_then(|p| p.as_str()) {
                        paths.push(path.to_owned());
                    }
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(label: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "fieldbook-archive-test-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create temp repo dir: {e}"));
        Self { dir }
    }

    fn init(&self) {
        self.git(&["init", "-q", "-b", "main"]);
        self.git(&["config", "user.name", "Synthetic Test"]);
        self.git(&["config", "user.email", "synthetic@example.invalid"]);
        self.git(&["config", "commit.gpgsign", "false"]);
        self.git(&["config", "tag.gpgsign", "false"]);
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap_or_else(|e| panic!("git runs: {e}"));
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "git {args:?} failed:\nstdout: {stdout}\nstderr: {stderr}"
        );
        stdout.trim().to_owned()
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| panic!("create parent: {e}"));
        }
        fs::write(&path, content).unwrap_or_else(|e| panic!("write file: {e}"));
    }

    fn commit_all(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg, "--allow-empty"]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn set_origin_main(&self, sha: &str) {
        self.git(&["update-ref", "refs/remotes/origin/main", sha]);
    }

    fn tag(&self, name: &str) {
        self.git(&["tag", name]);
    }

    fn checkout(&self, name: &str) {
        self.git(&["checkout", "-q", name]);
    }

    fn copy_from_workspace(&self) {
        let src = repo_root();
        let files = [
            "release-baseline.toml",
            "compat/published.toml",
            "compat/calculation/build_up@1/contract.toml",
            "compat/calculation/build_up@1/calc-01.in.toml",
            "compat/calculation/build_up@1/calc-01.out.txt",
            "compat/calculation/build_up@1/calc-90.in.toml",
            "compat/calculation/build_up@1/calc-90.out.txt",
            "compat/calculation/children_sum@1/contract.toml",
            "compat/calculation/children_sum@1/calc-04.in.toml",
            "compat/calculation/children_sum@1/calc-04.out.txt",
            "compat/calculation/children_sum@1/calc-91.in.toml",
            "compat/calculation/children_sum@1/calc-91.out.txt",
            "compat/run-schema/canonical-run@1/contract.toml",
            "compat/run-schema/canonical-run@1/calc-01.in.toml",
            "compat/run-schema/canonical-run@1/calc-01.out.txt",
            "crates/fieldbook-calc/src/fns.rs",
            "crates/fieldbook-calc/src/versions/build_up_v1.rs",
            "crates/fieldbook-calc/src/versions/children_sum_v1.rs",
            "crates/fieldbook-calc/src/versions/run_v1.rs",
            "crates/fieldbook-calc/tests/cases/EPOCH",
            "crates/fieldbook-calc/tests/cases/calc-01.toml",
            "crates/fieldbook-calc/tests/cases/calc-02.toml",
            "crates/fieldbook-calc/tests/cases/calc-03.toml",
            "crates/fieldbook-calc/tests/cases/calc-04.toml",
            "crates/fieldbook-calc/tests/cases/calc-90.toml",
        ];
        for rel in files {
            let from = src.join(rel);
            let to = self.dir.join(rel);
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent).unwrap_or_else(|e| panic!("create parent dirs: {e}"));
            }
            fs::copy(&from, &to).unwrap_or_else(|e| panic!("copy {rel}: {e}"));
        }

        for dir in [
            "compat",
            "crates/fieldbook-core/src/versions",
            "crates/fieldbook-core/src/ports/templates",
            "crates/fieldbook/src/io/versions",
            "xtask/src/versions",
        ] {
            copy_tree(&src.join(dir), &self.dir.join(dir));
        }
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// =========================================================================
// Integration tests
// =========================================================================

#[test]
fn archive_round_trip() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let ed_dir = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let ev_dir = tmp.path().join("ev-001");
    build_synthetic_evidence_dir(
        &ev_dir,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );

    let archives_root = tmp.path().join("archives");
    let base_file = ev_dir.join("release-baseline.toml");

    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out, "archive-release");

    let arch_dir = archives_root.join(format!("edition-001-{}", &commit[..12]));
    assert!(arch_dir.is_dir());

    let restore_dir = tmp.path().join("restore");
    let arch_str = arch_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &["verify-archive", arch_str, "--restore-to", restore_str],
        tmp.path(),
        None,
    );
    assert_success(&out_verify, "verify-archive");
    assert_stdout_contains(&out_verify, "explicit empty baseline");

    let restored_html = fs::read(restore_dir.join("current/edition/index.html"))
        .unwrap_or_else(|e| panic!("read restored index.html: {e}"));
    assert_eq!(restored_html, b"<!doctype html>\n");

    // Re-running archive-release with identical inputs succeeds.
    let out_again = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(
        &out_again,
        "re-running archive-release with identical inputs",
    );
}

#[test]
fn changed_archive_byte_fails() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let ed_dir = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let ev_dir = tmp.path().join("ev-001");
    build_synthetic_evidence_dir(
        &ev_dir,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );

    let archives_root = tmp.path().join("archives");
    let base_file = ev_dir.join("release-baseline.toml");

    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out, "archive-release");

    let arch_dir = archives_root.join(format!("edition-001-{}", &commit[..12]));
    let zip_path = arch_dir.join("fieldbook-edition-001.zip");
    let mut zip_bytes = fs::read(&zip_path).unwrap_or_else(|e| panic!("read zip: {e}"));
    assert!(zip_bytes.len() > 50);
    zip_bytes[50] ^= 0x55;
    fs::write(&zip_path, &zip_bytes).unwrap_or_else(|e| panic!("write tampered zip: {e}"));

    let restore_dir = tmp.path().join("restore");
    let arch_str = arch_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &["verify-archive", arch_str, "--restore-to", restore_str],
        tmp.path(),
        None,
    );
    assert_failure(&out_verify, "verify-archive with tampered zip");
    // The archive-manifest refusal itself, not a later check that happens to notice the zip.
    assert_stderr_contains(
        &out_verify,
        "archived file `fieldbook-edition-001.zip` sha256",
    );
    assert_stderr_contains(&out_verify, "differs from `archive.toml`");
}

#[test]
fn site_archive_refuses_a_support_digest_naming_other_bytes() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let ed_dir = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );
    let ev_dir = tmp.path().join("ev-001");
    build_synthetic_evidence_dir(
        &ev_dir,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );
    let base_file = ev_dir.join("release-baseline.toml");
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let archive = |edition: &Path, to: &Path| {
        run_xtask(
            &[
                "archive-release",
                "--kind",
                "site",
                "--edition",
                edition.to_str().unwrap_or_else(|| panic!("utf8")),
                "--evidence",
                ev_str,
                "--base",
                base_str,
                "--to",
                to.to_str().unwrap_or_else(|| panic!("utf8")),
            ],
            tmp.path(),
            None,
        )
    };

    // Every prerequisite is valid: the untouched set archives.
    let control = archive(&ed_dir, &tmp.path().join("archives-control"));
    assert_success(&control, "archive-release of the untouched set");

    // The one change: the set's corpus-support.sha256 names other bytes.
    let wrong = tmp.path().join("ed-001-wrong-digest");
    copy_tree(&ed_dir, &wrong);
    fs::write(
        wrong.join("corpus-support.sha256"),
        format!("{}  corpus-support.toml\n", "0".repeat(64)),
    )
    .unwrap_or_else(|e| panic!("write digest: {e}"));
    let to = tmp.path().join("archives");
    let out = archive(&wrong, &to);
    assert_failure(&out, "a support digest that names other bytes");
    assert_stderr_contains(
        &out,
        "`corpus-support.sha256` does not match `<sha256>  corpus-support.toml\\n` of `corpus-support.toml`",
    );
    assert!(
        !to.join(format!("edition-001-{}", &commit[..12])).exists(),
        "nothing is archived"
    );
}

#[test]
fn missing_previous_set_blocks_delivery() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let ed_dir = tmp.path().join("ed-002");
    build_synthetic_edition_set(
        &ed_dir,
        "002",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let baseline = format!(
        "baseline_schema = 1\n\n\
         [compatibility]\n\
         kind = \"empty\"\n\n\
         [tool]\n\
         kind = \"empty\"\n\n\
         [edition]\n\
         kind = \"release\"\n\
         tag = \"edition-001\"\n\
         commit = \"{}\"\n\
         archive_manifest_sha256 = \"{}\"\n",
        "1".repeat(40),
        "2".repeat(64)
    );
    let ev_dir = tmp.path().join("ev-002");
    build_synthetic_evidence_dir(
        &ev_dir,
        &baseline,
        None,
        b"vector = 1\n",
        ("edition-002", &commit),
    );

    let archives_root = tmp.path().join("archives");
    let base_file = ev_dir.join("release-baseline.toml");

    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out, "archive-release");

    let arch_dir = archives_root.join(format!("edition-002-{}", &commit[..12]));
    let restore_dir = tmp.path().join("restore");
    let arch_str = arch_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &["verify-archive", arch_str, "--restore-to", restore_str],
        tmp.path(),
        None,
    );
    assert_failure(&out_verify, "verify-archive without --previous");
    assert_stderr_contains_any(&out_verify, &["edition-001", "predecessor"]);
}

#[test]
#[cfg(unix)]
fn explicit_first_release_verifies_current_archive() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);

    let pkg_dir = tmp.path().join("pkg-v0.1.0");
    build_synthetic_tool_package(&pkg_dir, "v0.1.0", &commit);

    let ed_dir = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let baseline = format!(
        "baseline_schema = 1\n\n\
         [compatibility]\n\
         kind = \"empty\"\n\n\
         [tool]\n\
         kind = \"empty\"\n\n\
         [edition]\n\
         kind = \"release\"\n\
         tag = \"edition-001\"\n\
         commit = \"{}\"\n\
         archive_manifest_sha256 = \"{}\"\n",
        "1".repeat(40),
        "2".repeat(64)
    );
    let ev_dir = tmp.path().join("ev-t1");
    build_synthetic_evidence_dir(
        &ev_dir,
        &baseline,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );

    let archives_root = tmp.path().join("archives");
    let base_file = ev_dir.join("release-baseline.toml");

    let pkg_str = pkg_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out = run_xtask(
        &[
            "archive-release",
            "--package",
            pkg_str,
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out, "archive-release tool with empty tool baseline");

    let arch_dir = archives_root.join(format!("v0.1.0-{}", &commit[..12]));
    let restore_dir = tmp.path().join("restore");
    let arch_str = arch_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &["verify-archive", arch_str, "--restore-to", restore_str],
        tmp.path(),
        None,
    );
    assert_success(&out_verify, "verify-archive tool without previous");
    assert_stdout_contains(&out_verify, "explicit empty baseline");
}

#[test]
fn missing_previous_is_not_an_empty_baseline() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "b".repeat(40);

    // Part (a): tool binding declares a predecessor and --previous is omitted -> non-zero.
    let pkg_dir = tmp.path().join("pkg-v0.2.0");
    build_synthetic_tool_package(&pkg_dir, "v0.2.0", &commit);

    let ed_dir = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let baseline_pred = format!(
        "baseline_schema = 1\n\n\
         [compatibility]\n\
         kind = \"empty\"\n\n\
         [tool]\n\
         kind = \"release\"\n\
         tag = \"v0.1.0\"\n\
         commit = \"{}\"\n\
         archive_manifest_sha256 = \"{}\"\n\n\
         [edition]\n\
         kind = \"empty\"\n",
        "1".repeat(40),
        "2".repeat(64)
    );
    let ev_dir = tmp.path().join("ev-t2");
    build_synthetic_evidence_dir(
        &ev_dir,
        &baseline_pred,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );

    let archives_root = tmp.path().join("archives");
    let base_file = ev_dir.join("release-baseline.toml");

    let pkg_str = pkg_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_arch_a = run_xtask(
        &[
            "archive-release",
            "--package",
            pkg_str,
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_arch_a, "archive-release tool v0.2.0");

    let arch_a = archives_root.join(format!("v0.2.0-{}", &commit[..12]));
    let restore_a = tmp.path().join("restore-a");
    let arch_a_str = arch_a.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_a_str = restore_a.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify_a = run_xtask(
        &["verify-archive", arch_a_str, "--restore-to", restore_a_str],
        tmp.path(),
        None,
    );
    assert_failure(
        &out_verify_a,
        "omitting --previous when predecessor declared",
    );

    // Part (b): tool binding is explicitly empty but --previous is supplied -> non-zero.
    let commit_c = "c".repeat(40);
    let pkg_c_dir = tmp.path().join("pkg-v0.1.0-c");
    build_synthetic_tool_package(&pkg_c_dir, "v0.1.0", &commit_c);

    let ed_c_dir = tmp.path().join("ed-001-c");
    build_synthetic_edition_set(
        &ed_c_dir,
        "001",
        &commit_c,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let ev_c_dir = tmp.path().join("ev-c");
    build_synthetic_evidence_dir(
        &ev_c_dir,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &commit_c),
    );

    let base_c_file = ev_c_dir.join("release-baseline.toml");
    let pkg_c_str = pkg_c_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ed_c_str = ed_c_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_c_str = ev_c_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_c_str = base_c_file.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_arch_b = run_xtask(
        &[
            "archive-release",
            "--package",
            pkg_c_str,
            "--edition",
            ed_c_str,
            "--evidence",
            ev_c_str,
            "--base",
            base_c_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_arch_b, "archive-release tool v0.1.0 empty baseline");

    let arch_b = archives_root.join(format!("v0.1.0-{}", &commit_c[..12]));
    let restore_b = tmp.path().join("restore-b");
    let arch_b_str = arch_b.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_b_str = restore_b.to_str().unwrap_or_else(|| panic!("utf8"));

    let dummy_prev = tmp.path().join("dummy-prev");
    fs::create_dir_all(&dummy_prev).unwrap_or_else(|e| panic!("mkdir: {e}"));
    let dummy_prev_str = dummy_prev.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify_b = run_xtask(
        &[
            "verify-archive",
            arch_b_str,
            "--restore-to",
            restore_b_str,
            "--previous",
            dummy_prev_str,
        ],
        tmp.path(),
        None,
    );
    assert_failure(
        &out_verify_b,
        "supplying --previous when baseline is explicitly empty",
    );
}

#[test]
fn archive_required_before_delivery() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let ed_dir = tmp.path().join("downloaded-artifacts");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let restore_dir = tmp.path().join("restore");
    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out = run_xtask(
        &["verify-archive", ed_str, "--restore-to", restore_str],
        tmp.path(),
        None,
    );
    assert_failure(&out, "verify-archive without archive.toml");
    assert_stderr_contains(&out, "archive.toml");
}

#[test]
fn existing_different_archive_is_refused() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let ed_dir = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let ev1_dir = tmp.path().join("ev-001-v1");
    build_synthetic_evidence_dir(
        &ev1_dir,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );

    let archives_root = tmp.path().join("archives");
    let base1_file = ev1_dir.join("release-baseline.toml");

    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev1_str = ev1_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base1_str = base1_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out1 = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed_str,
            "--evidence",
            ev1_str,
            "--base",
            base1_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out1, "first archive-release");

    let arch_dir = archives_root.join(format!("edition-001-{}", &commit[..12]));
    let mut files_before = BTreeMap::new();
    collect_dir_hashes(&arch_dir, &arch_dir, &mut files_before);
    assert!(!files_before.is_empty());

    // Evidence 2 has a changed bound vector and updated published.toml hash.
    let ev2_dir = tmp.path().join("ev-001-v2");
    build_synthetic_evidence_dir(
        &ev2_dir,
        EMPTY_BASELINES,
        None,
        b"vector = 2\n",
        ("edition-001", &commit),
    );
    let base2_file = ev2_dir.join("release-baseline.toml");
    let ev2_str = ev2_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base2_str = base2_file.to_str().unwrap_or_else(|| panic!("utf8"));

    let out2 = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed_str,
            "--evidence",
            ev2_str,
            "--base",
            base2_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_failure(&out2, "archive-release with different bytes");
    assert_stderr_contains(&out2, "already exists with different bytes");

    let mut files_after = BTreeMap::new();
    collect_dir_hashes(&arch_dir, &arch_dir, &mut files_after);
    assert_eq!(
        files_before, files_after,
        "first folder must remain unchanged"
    );
}

#[test]
#[cfg(unix)]
fn rollback_uses_archived_bytes() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let c1 = "1".repeat(40);

    let pkg_v1 = tmp.path().join("pkg-v0.1.0");
    build_synthetic_tool_package(&pkg_v1, "v0.1.0", &c1);

    let ed_1 = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_1,
        "001",
        &c1,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let ev_1 = tmp.path().join("ev-1");
    build_synthetic_evidence_dir(
        &ev_1,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &c1),
    );

    let archives_root = tmp.path().join("archives");
    let base1_file = ev_1.join("release-baseline.toml");

    let pkg1_str = pkg_v1.to_str().unwrap_or_else(|| panic!("utf8"));
    let ed1_str = ed_1.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev1_str = ev_1.to_str().unwrap_or_else(|| panic!("utf8"));
    let base1_str = base1_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out1 = run_xtask(
        &[
            "archive-release",
            "--package",
            pkg1_str,
            "--edition",
            ed1_str,
            "--evidence",
            ev1_str,
            "--base",
            base1_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out1, "archive tool v0.1.0");

    let arch_1 = archives_root.join(format!("v0.1.0-{}", &c1[..12]));
    let arch_1_manifest_bytes = fs::read(arch_1.join("archive.toml"))
        .unwrap_or_else(|e| panic!("read v0.1.0 archive.toml: {e}"));
    let arch_1_sha = sha256_hex(&arch_1_manifest_bytes);
    let v1_script_bytes = fs::read(arch_1.join("package/fieldbook"))
        .unwrap_or_else(|e| panic!("read archived fieldbook script: {e}"));

    // Tool v0.2.0 declares v0.1.0 as predecessor.
    let c2 = "2".repeat(40);
    let pkg_v2 = tmp.path().join("pkg-v0.2.0");
    build_synthetic_tool_package(&pkg_v2, "v0.2.0", &c2);

    let ed_2 = tmp.path().join("ed-002");
    build_synthetic_edition_set(
        &ed_2,
        "001",
        &c2,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let baseline_v2 = format!(
        "baseline_schema = 1\n\n\
         [compatibility]\n\
         kind = \"empty\"\n\n\
         [tool]\n\
         kind = \"release\"\n\
         tag = \"v0.1.0\"\n\
         commit = \"{c1}\"\n\
         archive_manifest_sha256 = \"{arch_1_sha}\"\n\n\
         [edition]\n\
         kind = \"empty\"\n"
    );
    let ev_2 = tmp.path().join("ev-2");
    build_synthetic_evidence_dir(
        &ev_2,
        &baseline_v2,
        None,
        b"vector = 2\n",
        ("edition-001", &c2),
    );
    let base2_file = ev_2.join("release-baseline.toml");

    let pkg2_str = pkg_v2.to_str().unwrap_or_else(|| panic!("utf8"));
    let ed2_str = ed_2.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev2_str = ev_2.to_str().unwrap_or_else(|| panic!("utf8"));
    let base2_str = base2_file.to_str().unwrap_or_else(|| panic!("utf8"));

    let out2 = run_xtask(
        &[
            "archive-release",
            "--package",
            pkg2_str,
            "--edition",
            ed2_str,
            "--evidence",
            ev2_str,
            "--base",
            base2_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out2, "archive tool v0.2.0");

    let arch_2 = archives_root.join(format!("v0.2.0-{}", &c2[..12]));
    let restore_dir = tmp.path().join("restore-rollback");
    let arch_2_str = arch_2.to_str().unwrap_or_else(|| panic!("utf8"));
    let arch_1_str = arch_1.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &[
            "verify-archive",
            arch_2_str,
            "--restore-to",
            restore_str,
            "--previous",
            arch_1_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_verify, "verify-archive v0.2.0 with predecessor v0.1.0");
    assert_stdout_contains(&out_verify, "v0.1.0");

    let restored_script = fs::read(restore_dir.join("previous/package/fieldbook"))
        .unwrap_or_else(|e| panic!("read restored v0.1.0 script: {e}"));
    assert_eq!(restored_script, v1_script_bytes);

    // Variant: --previous folder whose archive.toml hash does not equal the declared one fails.
    let arch_1_tampered = tmp.path().join("arch-1-tampered");
    copy_tree(&arch_1, &arch_1_tampered);
    let mut manifest_str = fs::read_to_string(arch_1_tampered.join("archive.toml"))
        .unwrap_or_else(|e| panic!("read archive.toml: {e}"));
    manifest_str.push_str("# tampered hash\n");
    fs::write(arch_1_tampered.join("archive.toml"), manifest_str)
        .unwrap_or_else(|e| panic!("write tampered archive.toml: {e}"));

    let restore_bad = tmp.path().join("restore-bad");
    let bad_prev_str = arch_1_tampered.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_bad_str = restore_bad.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_bad = run_xtask(
        &[
            "verify-archive",
            arch_2_str,
            "--restore-to",
            restore_bad_str,
            "--previous",
            bad_prev_str,
        ],
        tmp.path(),
        None,
    );
    assert_failure(&out_bad, "verify-archive with mismatched previous hash");
}

#[test]
fn missing_declared_edition_predecessor_refuses() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let c1 = "1".repeat(40);

    let ed_1 = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_1,
        "001",
        &c1,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let ev_1 = tmp.path().join("ev-1");
    build_synthetic_evidence_dir(
        &ev_1,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &c1),
    );

    let archives_root = tmp.path().join("archives");
    let base1_file = ev_1.join("release-baseline.toml");

    let ed1_str = ed_1.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev1_str = ev_1.to_str().unwrap_or_else(|| panic!("utf8"));
    let base1_str = base1_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out1 = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed1_str,
            "--evidence",
            ev1_str,
            "--base",
            base1_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out1, "archive site edition-001");

    let arch_1 = archives_root.join(format!("edition-001-{}", &c1[..12]));
    let arch_1_manifest_bytes = fs::read(arch_1.join("archive.toml"))
        .unwrap_or_else(|e| panic!("read 001 archive.toml: {e}"));
    let arch_1_sha = sha256_hex(&arch_1_manifest_bytes);

    let c2 = "2".repeat(40);
    let ed_2 = tmp.path().join("ed-002");
    build_synthetic_edition_set(
        &ed_2,
        "002",
        &c2,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let baseline_2 = format!(
        "baseline_schema = 1\n\n\
         [compatibility]\n\
         kind = \"empty\"\n\n\
         [tool]\n\
         kind = \"empty\"\n\n\
         [edition]\n\
         kind = \"release\"\n\
         tag = \"edition-001\"\n\
         commit = \"{c1}\"\n\
         archive_manifest_sha256 = \"{arch_1_sha}\"\n"
    );
    let ev_2 = tmp.path().join("ev-2");
    build_synthetic_evidence_dir(
        &ev_2,
        &baseline_2,
        None,
        b"vector = 2\n",
        ("edition-002", &c2),
    );
    let base2_file = ev_2.join("release-baseline.toml");

    let ed2_str = ed_2.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev2_str = ev_2.to_str().unwrap_or_else(|| panic!("utf8"));
    let base2_str = base2_file.to_str().unwrap_or_else(|| panic!("utf8"));

    let out2 = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed2_str,
            "--evidence",
            ev2_str,
            "--base",
            base2_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out2, "archive site edition-002");

    let arch_2 = archives_root.join(format!("edition-002-{}", &c2[..12]));
    let arch_2_str = arch_2.to_str().unwrap_or_else(|| panic!("utf8"));
    let arch_1_str = arch_1.to_str().unwrap_or_else(|| panic!("utf8"));

    // (1) Omitting --previous fails.
    let restore_1 = tmp.path().join("restore-1");
    let restore_1_str = restore_1.to_str().unwrap_or_else(|| panic!("utf8"));
    let out_missing = run_xtask(
        &["verify-archive", arch_2_str, "--restore-to", restore_1_str],
        tmp.path(),
        None,
    );
    assert_failure(&out_missing, "omitting --previous");

    // (2) Supplying previous of different tag or hash fails.
    let arch_1_tampered = tmp.path().join("arch-1-tampered");
    copy_tree(&arch_1, &arch_1_tampered);
    let mut manifest_str = fs::read_to_string(arch_1_tampered.join("archive.toml"))
        .unwrap_or_else(|e| panic!("read archive.toml: {e}"));
    manifest_str.push_str("# different hash\n");
    fs::write(arch_1_tampered.join("archive.toml"), manifest_str)
        .unwrap_or_else(|e| panic!("write tampered archive.toml: {e}"));

    let restore_2 = tmp.path().join("restore-2");
    let restore_2_str = restore_2.to_str().unwrap_or_else(|| panic!("utf8"));
    let tampered_str = arch_1_tampered.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_tampered = run_xtask(
        &[
            "verify-archive",
            arch_2_str,
            "--restore-to",
            restore_2_str,
            "--previous",
            tampered_str,
        ],
        tmp.path(),
        None,
    );
    assert_failure(&out_tampered, "supplying previous with wrong hash");

    // (3) Supplying the exact one passes.
    let restore_3 = tmp.path().join("restore-3");
    let restore_3_str = restore_3.to_str().unwrap_or_else(|| panic!("utf8"));
    let out_ok = run_xtask(
        &[
            "verify-archive",
            arch_2_str,
            "--restore-to",
            restore_3_str,
            "--previous",
            arch_1_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_ok, "supplying exact predecessor");
    assert_stdout_contains(&out_ok, "edition-001");
}

#[test]
fn raw_to_site_restores_edition_predecessor_without_tool() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let c1 = "1".repeat(40);

    let raw_ed = tmp.path().join("raw-ed-001");
    build_synthetic_edition_set(
        &raw_ed,
        "001",
        &c1,
        DeliveryKind::Raw,
        &default_source_specs(),
        None,
    );

    let raw_mode = "release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n";
    let raw_ev = tmp.path().join("raw-ev-001");
    build_synthetic_evidence_dir(
        &raw_ev,
        EMPTY_BASELINES,
        Some(raw_mode),
        b"vector = 1\n",
        ("edition-001", &c1),
    );

    let archives_root = tmp.path().join("archives");
    let raw_base_file = raw_ev.join("release-baseline.toml");

    let raw_ed_str = raw_ed.to_str().unwrap_or_else(|| panic!("utf8"));
    let raw_ev_str = raw_ev.to_str().unwrap_or_else(|| panic!("utf8"));
    let raw_base_str = raw_base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_raw = run_xtask(
        &[
            "archive-release",
            "--kind",
            "raw",
            "--edition",
            raw_ed_str,
            "--evidence",
            raw_ev_str,
            "--base",
            raw_base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_raw, "archive raw edition-001");

    let raw_arch = archives_root.join(format!("edition-001-{}", &c1[..12]));
    let raw_arch_toml = fs::read(raw_arch.join("archive.toml"))
        .unwrap_or_else(|e| panic!("read raw archive.toml: {e}"));
    let raw_arch_sha = sha256_hex(&raw_arch_toml);

    // Site edition-002 declares raw edition-001 as predecessor.
    let c2 = "2".repeat(40);
    let site_ed = tmp.path().join("site-ed-002");
    build_synthetic_edition_set(
        &site_ed,
        "002",
        &c2,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let baseline_2 = format!(
        "baseline_schema = 1\n\n\
         [compatibility]\n\
         kind = \"empty\"\n\n\
         [tool]\n\
         kind = \"empty\"\n\n\
         [edition]\n\
         kind = \"release\"\n\
         tag = \"edition-001\"\n\
         commit = \"{c1}\"\n\
         archive_manifest_sha256 = \"{raw_arch_sha}\"\n"
    );

    // Stale descriptor check: retaining raw 001 descriptor for edition-002 is refused.
    let bad_ev = tmp.path().join("bad-ev-002");
    build_synthetic_evidence_dir(
        &bad_ev,
        &baseline_2,
        Some("release_schema = 1\nedition = \"001\"\nkind = \"raw\"\n"),
        b"vector = 2\n",
        ("edition-002", &c2),
    );
    let bad_base_file = bad_ev.join("release-baseline.toml");
    let site_ed_str = site_ed.to_str().unwrap_or_else(|| panic!("utf8"));
    let bad_ev_str = bad_ev.to_str().unwrap_or_else(|| panic!("utf8"));
    let bad_base_str = bad_base_file.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_stale = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            site_ed_str,
            "--evidence",
            bad_ev_str,
            "--base",
            bad_base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_failure(&out_stale, "retaining raw 001 descriptor for edition-002");

    // Clean site edition-002 evidence.
    let site_ev = tmp.path().join("site-ev-002");
    build_synthetic_evidence_dir(
        &site_ev,
        &baseline_2,
        None,
        b"vector = 2\n",
        ("edition-002", &c2),
    );
    let site_base_file = site_ev.join("release-baseline.toml");
    let site_ev_str = site_ev.to_str().unwrap_or_else(|| panic!("utf8"));
    let site_base_str = site_base_file.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_site = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            site_ed_str,
            "--evidence",
            site_ev_str,
            "--base",
            site_base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_site, "archive site edition-002");

    let site_arch = archives_root.join(format!("edition-002-{}", &c2[..12]));
    let restore_dir = tmp.path().join("restore-raw-to-site");
    let site_arch_str = site_arch.to_str().unwrap_or_else(|| panic!("utf8"));
    let raw_arch_str = raw_arch.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &[
            "verify-archive",
            site_arch_str,
            "--restore-to",
            restore_str,
            "--previous",
            raw_arch_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_verify, "verify-archive site current with raw previous");

    assert!(
        restore_dir.join("previous/edition/README.txt").is_file(),
        "<restore>/previous/edition/README.txt must exist"
    );
    assert!(
        !restore_dir.join("current/package").exists(),
        "no package in current"
    );
    assert!(
        !restore_dir.join("previous/package").exists(),
        "no package in previous"
    );
}

#[test]
fn site_evidence_reaches_archive() {
    let repo = Repo::new("site-evidence-archive");
    repo.init();
    repo.copy_from_workspace();
    repo.write("Cargo.toml", "[workspace.package]\nversion = \"0.0.0\"\n");
    repo.write("Cargo.lock", "version = 4\n");
    repo.write(
        "CHANGELOG.md",
        "# Changelog\n\n## Unreleased\n\n- Added: x.\n",
    );
    repo.write("release-baseline.toml", EMPTY_BASELINES);
    repo.write(
        "corpus/changes.md",
        "# Corpus changes\n\n## edition-002\n\n- Added: x.\n",
    );

    let commit = repo.commit_all("edition 002 release");
    repo.set_origin_main(&commit);
    repo.tag("edition-002");
    repo.checkout("edition-002");

    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let evidence_dir = tmp.path().join("evidence-out");
    let ev_str = evidence_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_check = run_xtask(
        &[
            "release-check",
            "edition-002",
            "--base",
            "release-baseline.toml",
            "--evidence-out",
            ev_str,
        ],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_success(&out_check, "release-check edition-002");
    assert!(evidence_dir.join("compat/published.toml").is_file());
    // The tag check emits the identity it verified together with the evidence.
    assert_eq!(
        fs::read_to_string(evidence_dir.join("release-identity.toml"))
            .unwrap_or_else(|e| panic!("read release-identity.toml: {e}")),
        identity_toml("edition-002", &commit)
    );

    let pair_dir = tmp.path().join("pair-002");
    build_synthetic_edition_set(
        &pair_dir,
        "002",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );

    let archives_root = tmp.path().join("archives");
    let pair_str = pair_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_arch = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            pair_str,
            "--evidence",
            ev_str,
            "--base",
            "release-baseline.toml",
            "--to",
            to_str,
        ],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_success(&out_arch, "archive-release site edition-002");

    let arch_dir = archives_root.join(format!("edition-002-{}", &commit[..12]));
    let restore_dir = tmp.path().join("restore-site");
    let arch_str = arch_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &["verify-archive", arch_str, "--restore-to", restore_str],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_success(&out_verify, "verify-archive site edition-002");

    let published_text = fs::read_to_string(arch_dir.join("evidence/compat/published.toml"))
        .unwrap_or_else(|e| panic!("read published.toml: {e}"));
    let bound_paths = extract_bound_paths_from_published(&published_text);
    assert!(!bound_paths.is_empty());
    let mut saw_rs_blob = false;
    for rel in &bound_paths {
        let path = arch_dir.join("evidence").join(rel);
        assert!(
            path.is_file(),
            "bound file `{rel}` must exist in archive evidence"
        );
        if rel.ends_with(".rs") {
            saw_rs_blob = true;
        }
    }
    assert!(
        saw_rs_blob,
        "archive evidence must include bound implementation .rs blob"
    );

    // Negative 1: delete a bound implementation blob -> archive-release non-zero.
    let ev_neg1 = tmp.path().join("ev-neg1");
    copy_tree(&evidence_dir, &ev_neg1);
    let rs_path = ev_neg1.join("crates/fieldbook-calc/src/versions/build_up_v1.rs");
    assert!(rs_path.is_file());
    fs::remove_file(&rs_path).unwrap_or_else(|e| panic!("remove rs blob: {e}"));

    let to_neg1 = tmp.path().join("archives-neg1");
    let ev_neg1_str = ev_neg1.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_neg1_str = to_neg1.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_neg1 = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            pair_str,
            "--evidence",
            ev_neg1_str,
            "--base",
            "release-baseline.toml",
            "--to",
            to_neg1_str,
        ],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_failure(&out_neg1, "deleted implementation blob");

    // Negative 2: change one bound vector's bytes (wrong hash) -> non-zero.
    let ev_neg2 = tmp.path().join("ev-neg2");
    copy_tree(&evidence_dir, &ev_neg2);
    let vec_path = ev_neg2.join("compat/calculation/build_up@1/calc-01.in.toml");
    assert!(vec_path.is_file());
    let mut vec_bytes = fs::read(&vec_path).unwrap_or_else(|e| panic!("read: {e}"));
    vec_bytes.push(b'\n');
    fs::write(&vec_path, vec_bytes).unwrap_or_else(|e| panic!("write: {e}"));

    let to_neg2 = tmp.path().join("archives-neg2");
    let ev_neg2_str = ev_neg2.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_neg2_str = to_neg2.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_neg2 = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            pair_str,
            "--evidence",
            ev_neg2_str,
            "--base",
            "release-baseline.toml",
            "--to",
            to_neg2_str,
        ],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_failure(&out_neg2, "changed bound vector bytes");

    // Negative 3: edition-release.toml in evidence naming edition = "003" -> non-zero.
    let ev_neg3 = tmp.path().join("ev-neg3");
    copy_tree(&evidence_dir, &ev_neg3);
    fs::write(
        ev_neg3.join("edition-release.toml"),
        "release_schema = 1\nedition = \"003\"\nkind = \"site\"\n",
    )
    .unwrap_or_else(|e| panic!("write edition-release.toml: {e}"));

    let to_neg3 = tmp.path().join("archives-neg3");
    let ev_neg3_str = ev_neg3.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_neg3_str = to_neg3.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_neg3 = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            pair_str,
            "--evidence",
            ev_neg3_str,
            "--base",
            "release-baseline.toml",
            "--to",
            to_neg3_str,
        ],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_failure(&out_neg3, "mismatched edition-release.toml edition number");

    // The archived identity is the tag check's, matching the pair and archive.toml.
    assert_eq!(
        fs::read_to_string(arch_dir.join("evidence/release-identity.toml"))
            .unwrap_or_else(|e| panic!("read archived identity: {e}")),
        identity_toml("edition-002", &commit)
    );

    // Negatives 4 and 5: evidence from another tag or another commit -> archive-release refuses
    // before anything is archived.
    let other_commit = if commit.starts_with('0') {
        "1".repeat(40)
    } else {
        "0".repeat(40)
    };
    for (name, tag, ident_commit) in [
        ("tag", "edition-003", commit.as_str()),
        ("commit", "edition-002", other_commit.as_str()),
    ] {
        let ev_neg = tmp.path().join(format!("ev-identity-{name}"));
        copy_tree(&evidence_dir, &ev_neg);
        fs::write(
            ev_neg.join("release-identity.toml"),
            identity_toml(tag, ident_commit),
        )
        .unwrap_or_else(|e| panic!("write identity: {e}"));
        let to_neg = tmp.path().join(format!("archives-identity-{name}"));
        let out_neg = run_xtask(
            &[
                "archive-release",
                "--kind",
                "site",
                "--edition",
                pair_str,
                "--evidence",
                ev_neg.to_str().unwrap_or_else(|| panic!("utf8")),
                "--base",
                "release-baseline.toml",
                "--to",
                to_neg.to_str().unwrap_or_else(|| panic!("utf8")),
            ],
            &repo.dir,
            Some(&repo.dir),
        );
        assert_failure(&out_neg, &format!("evidence identity with another {name}"));
        assert_stderr_contains(&out_neg, "differs from the paired edition");
        assert!(
            !to_neg
                .join(format!("edition-002-{}", &commit[..12]))
                .exists()
        );
    }

    // The Site provider refuses an archive whose retained identity names another tag or
    // commit, even when archive.toml lists the edited bytes.
    for (name, tag, ident_commit) in [
        ("tag", "edition-003", commit.as_str()),
        ("commit", "edition-002", other_commit.as_str()),
    ] {
        let copy = tmp.path().join(format!("arch-identity-{name}"));
        copy_tree(&arch_dir, &copy);
        fs::write(
            copy.join("evidence/release-identity.toml"),
            identity_toml(tag, ident_commit),
        )
        .unwrap_or_else(|e| panic!("write identity: {e}"));
        relist_archive_file(&copy, "evidence/release-identity.toml");
        let restore = tmp.path().join(format!("restore-identity-{name}"));
        let out_verify = run_xtask(
            &[
                "verify-archive",
                copy.to_str().unwrap_or_else(|| panic!("utf8")),
                "--restore-to",
                restore.to_str().unwrap_or_else(|| panic!("utf8")),
            ],
            &repo.dir,
            Some(&repo.dir),
        );
        assert_failure(
            &out_verify,
            &format!("archived identity with another {name}"),
        );
        assert_stderr_contains(&out_verify, "not the archived pair");
    }

    // Evidence without the tag check's identity is refused, and nothing is archived.
    let ev_bare = tmp.path().join("ev-no-identity");
    copy_tree(&evidence_dir, &ev_bare);
    fs::remove_file(ev_bare.join("release-identity.toml"))
        .unwrap_or_else(|e| panic!("remove identity: {e}"));
    let to_bare = tmp.path().join("archives-no-identity");
    let out_bare = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            pair_str,
            "--evidence",
            ev_bare.to_str().unwrap_or_else(|| panic!("utf8")),
            "--base",
            "release-baseline.toml",
            "--to",
            to_bare.to_str().unwrap_or_else(|| panic!("utf8")),
        ],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_failure(&out_bare, "evidence without the identity file");
    assert_stderr_contains(&out_bare, "needs the tag check's identity");
    assert!(
        !to_bare
            .join(format!("edition-002-{}", &commit[..12]))
            .exists()
    );

    // An archive whose identity is removed from the folder and from archive.toml fails the
    // exact-set check.
    let copy_gone = tmp.path().join("arch-no-identity");
    copy_tree(&arch_dir, &copy_gone);
    fs::remove_file(copy_gone.join("evidence/release-identity.toml"))
        .unwrap_or_else(|e| panic!("remove identity: {e}"));
    unlist_archive_file(&copy_gone, "evidence/release-identity.toml");
    let out_gone = run_xtask(
        &[
            "verify-archive",
            copy_gone.to_str().unwrap_or_else(|| panic!("utf8")),
            "--restore-to",
            tmp.path()
                .join("restore-no-identity")
                .to_str()
                .unwrap_or_else(|| panic!("utf8")),
        ],
        &repo.dir,
        Some(&repo.dir),
    );
    assert_failure(&out_gone, "archive without the identity file");
    assert_stderr_contains(&out_gone, "release-identity.toml");
}

/// Drop the `archive.toml` row of `rel`, as if the archive had never listed it.
fn unlist_archive_file(arch_dir: &Path, rel: &str) {
    let manifest_path = arch_dir.join("archive.toml");
    let text = fs::read_to_string(&manifest_path).unwrap_or_else(|e| panic!("read manifest: {e}"));
    let header = format!("[[file]]\npath = \"{rel}\"\n");
    let at = text
        .find(&header)
        .unwrap_or_else(|| panic!("`{rel}` is listed"));
    let after = at + header.len();
    let row_end = text[after..]
        .find("\n\n")
        .map_or(text.len(), |n| after + n + 2);
    let edited = format!("{}{}", &text[..at], &text[row_end..]);
    fs::write(&manifest_path, edited).unwrap_or_else(|e| panic!("write manifest: {e}"));
}

/// Rewrite the `archive.toml` row of `rel` so it names the file's current size and digest; only
/// a provider check can then refuse the edited archive.
fn relist_archive_file(arch_dir: &Path, rel: &str) {
    let bytes = fs::read(arch_dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
    let manifest_path = arch_dir.join("archive.toml");
    let text = fs::read_to_string(&manifest_path).unwrap_or_else(|e| panic!("read manifest: {e}"));
    let row_start = format!("path = \"{rel}\"\n");
    let at = text
        .find(&row_start)
        .unwrap_or_else(|| panic!("`{rel}` is listed"));
    let after = at + row_start.len();
    let row_end = text[after..]
        .find("\n\n")
        .map_or(text.len(), |n| after + n + 1);
    let new_row = format!(
        "size = {}\nsha256 = \"{}\"\n",
        bytes.len(),
        sha256_hex(&bytes)
    );
    let edited = format!("{}{new_row}{}", &text[..after], &text[row_end..]);
    fs::write(&manifest_path, edited).unwrap_or_else(|e| panic!("write manifest: {e}"));
}

#[test]
fn site_archive_refuses_a_failing_browser_summary() {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let ed_dir = tmp.path().join("ed-failing");
    let failing_summary =
        valid_summary_text("edition-001").replace("verdict: pass", "verdict: fail");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        Some(&failing_summary),
    );

    let ev_dir = tmp.path().join("ev-failing");
    build_synthetic_evidence_dir(
        &ev_dir,
        EMPTY_BASELINES,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );

    let archives_root = tmp.path().join("archives");
    let base_file = ev_dir.join("release-baseline.toml");

    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = archives_root.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_arch = run_xtask(
        &[
            "archive-release",
            "--kind",
            "site",
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    assert_success(&out_arch, "archive-release with failing summary succeeds");

    let arch_dir = archives_root.join(format!("edition-001-{}", &commit[..12]));
    let restore_dir = tmp.path().join("restore");
    let arch_str = arch_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore_dir.to_str().unwrap_or_else(|| panic!("utf8"));

    let out_verify = run_xtask(
        &["verify-archive", arch_str, "--restore-to", restore_str],
        tmp.path(),
        None,
    );
    assert_failure(&out_verify, "verify-archive with failing verdict");
    assert_stderr_contains(&out_verify, "verdict");
}

/// Runs `archive-release` for a tool package whose `fieldbook` script or descriptor was altered
/// by `tamper` after the honest builder wrote it.
#[cfg(unix)]
fn archive_tampered_tool(tamper: &dyn Fn(&Path, &str)) -> (tempfile::TempDir, Output, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap_or_else(|e| panic!("tempdir: {e}"));
    let commit = "a".repeat(40);
    let pkg_dir = tmp.path().join("pkg-v0.1.0");
    build_synthetic_tool_package(&pkg_dir, "v0.1.0", &commit);
    tamper(&pkg_dir, &commit);

    let ed_dir = tmp.path().join("ed-001");
    build_synthetic_edition_set(
        &ed_dir,
        "001",
        &commit,
        DeliveryKind::Site,
        &default_source_specs(),
        None,
    );
    let baseline = "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n\n[edition]\nkind = \"empty\"\n";
    let ev_dir = tmp.path().join("ev");
    build_synthetic_evidence_dir(
        &ev_dir,
        baseline,
        None,
        b"vector = 1\n",
        ("edition-001", &commit),
    );

    let base_file = ev_dir.join("release-baseline.toml");
    let to_dir = tmp.path().join("archives");
    let pkg_str = pkg_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ed_str = ed_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let ev_str = ev_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let base_str = base_file.to_str().unwrap_or_else(|| panic!("utf8"));
    let to_str = to_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let out = run_xtask(
        &[
            "archive-release",
            "--package",
            pkg_str,
            "--edition",
            ed_str,
            "--evidence",
            ev_str,
            "--base",
            base_str,
            "--to",
            to_str,
        ],
        tmp.path(),
        None,
    );
    (tmp, out, to_dir)
}

#[test]
#[cfg(unix)]
fn tool_archive_refuses_a_package_that_reports_another_commit() {
    let (tmp, out, to_dir) = archive_tampered_tool(&|pkg, _commit| {
        let other = "b".repeat(40);
        let script = format!("#!/bin/sh\necho \"fieldbook 0.1.0 ({other})\"\n");
        fs::write(pkg.join(fieldbook_core::strings::binary()), &script)
            .unwrap_or_else(|e| panic!("write: {e}"));
        let sums = format!("{}  fieldbook\n", sha256_hex(script.as_bytes()));
        fs::write(pkg.join("SHA256SUMS"), sums).unwrap_or_else(|e| panic!("write: {e}"));
    });
    // The archive step records the bytes; the restore rehearsal is what runs the program.
    assert_success(&out, "archive-release records the package bytes");
    let arch_dir = to_dir.join(format!("v0.1.0-{}", "a".repeat(12)));
    let restore = tmp.path().join("restore");
    let arch_str = arch_dir.to_str().unwrap_or_else(|| panic!("utf8"));
    let restore_str = restore.to_str().unwrap_or_else(|| panic!("utf8"));
    let verify = run_xtask(
        &["verify-archive", arch_str, "--restore-to", restore_str],
        tmp.path(),
        None,
    );
    assert_failure(
        &verify,
        "a package reporting another commit fails the rehearsal",
    );
    assert_stderr_contains(&verify, "smoke");
}

#[test]
#[cfg(unix)]
fn tool_archive_refuses_a_descriptor_for_another_commit() {
    let (_tmp, out, _to) = archive_tampered_tool(&|pkg, _commit| {
        let other = "b".repeat(40);
        fs::write(
            pkg.join("package.toml"),
            format!("tag = \"v0.1.0\"\ncommit = \"{other}\"\n"),
        )
        .unwrap_or_else(|e| panic!("write: {e}"));
    });
    assert_failure(&out, "a descriptor for another commit is not archived");
    assert_stderr_contains(&out, "paired edition commit");
}

#[test]
#[cfg(unix)]
fn tool_archive_refuses_a_retained_identity_of_another_commit() {
    let (tmp, out, to_dir) = archive_tampered_tool(&|_pkg, _commit| {});
    assert_success(&out, "archive-release tool package");
    let arch_dir = to_dir.join(format!("v0.1.0-{}", "a".repeat(12)));
    let restore_ok = tmp.path().join("restore-ok");
    let ok = run_xtask(
        &[
            "verify-archive",
            arch_dir.to_str().unwrap_or_else(|| panic!("utf8")),
            "--restore-to",
            restore_ok.to_str().unwrap_or_else(|| panic!("utf8")),
        ],
        tmp.path(),
        None,
    );
    assert_success(&ok, "the untouched tool archive verifies");

    let copy = tmp.path().join("arch-identity-other");
    copy_tree(&arch_dir, &copy);
    fs::write(
        copy.join("evidence/release-identity.toml"),
        identity_toml("edition-001", &"b".repeat(40)),
    )
    .unwrap_or_else(|e| panic!("write identity: {e}"));
    relist_archive_file(&copy, "evidence/release-identity.toml");
    let restore = tmp.path().join("restore-identity-other");
    let verify = run_xtask(
        &[
            "verify-archive",
            copy.to_str().unwrap_or_else(|| panic!("utf8")),
            "--restore-to",
            restore.to_str().unwrap_or_else(|| panic!("utf8")),
        ],
        tmp.path(),
        None,
    );
    assert_failure(
        &verify,
        "a tool archive retaining another commit's identity",
    );
    assert_stderr_contains(&verify, "not the archived pair");
}
