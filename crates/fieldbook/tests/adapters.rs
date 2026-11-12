//! Integration and contract tests for port adapters and adapter registry drift (`AD-01`, `ADR 0004 §11`).

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::print_stdout,
    reason = "integration tests assert adapter contract conformance and drift invariants"
)]

mod support;

#[path = "../../../tests/support/extractor_suite.rs"]
mod extractor_suite;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fieldbook::adapters::registry::{ADAPTERS, Caps, Port, Row, State};
use fieldbook::io::Disk;
use fieldbook_core::audit::{Actor, Principal, Role, Sha256, Via};
use fieldbook_core::authz::{Act, Authz, AuthzDecision, PolicyReaders, Scope};
use fieldbook_core::extraction::methods::{METHODS, Method};
use fieldbook_core::extraction::{Extractor, HtmlStrip, Plain, thin_page_numbers};
use fieldbook_core::labels::StoreId;
use fieldbook_core::policy::Policy;

struct FolderTargetFactory;

impl support::backup_suite::BackupFactory for FolderTargetFactory {
    fn make<'a>(
        &self,
        world: &'a support::world::TestWorld,
        audited: &'a fieldbook::io::Audited,
        allowed: &'a fieldbook_core::egress::Allowed,
        target: &'a Path,
    ) -> Box<dyn fieldbook_core::backup::BackupTarget + 'a> {
        Box::new(fieldbook::io::backup_folder::FolderTarget::new(
            world, audited, allowed, target,
        ))
    }
}

fn sample_actor() -> Actor {
    Actor {
        principal: Principal::local(),
        role: Role::Owner,
        via: Via::Cli,
    }
}

fn make_policy_readers<'a>(
    stores: &[(StoreId, Option<&str>, Option<&str>)],
    groups: &'a dyn fieldbook_core::authz::Groups,
) -> Box<dyn Authz + 'a> {
    let mut policy_text = String::from("policy_schema = 1\nrevision = 1\n");
    for &(store, readers, changers) in stores {
        if store == StoreId::Corpus {
            continue;
        }
        policy_text.push_str("\n[[store]]\n");
        policy_text.push_str(&format!("id = \"{}\"\n", store.as_str()));
        policy_text.push_str("holds = [\"public\", \"unlabelled\"]\n");
        if let Some(r) = readers {
            policy_text.push_str(&format!("readers = \"{r}\"\n"));
        }
        if let Some(c) = changers {
            policy_text.push_str(&format!("changers = \"{c}\"\n"));
        }
    }
    let policy = Policy::parse(&policy_text, None).expect("valid policy text");
    Box::new(PolicyReaders::new(&policy, groups))
}

#[test]
fn contract_extractor_plain_1() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/extractor/plain@1");
    let input = std::fs::read(fix_dir.join("input.txt")).expect("read input.txt");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");

    let expected_method = expect
        .get("method")
        .and_then(toml::Value::as_str)
        .expect("method");
    let expected_pages = usize::try_from(
        expect
            .get("pages")
            .and_then(toml::Value::as_integer)
            .expect("pages"),
    )
    .expect("positive pages");

    let extractor = Plain;
    let extraction = extractor.extract(&input).expect("extract plain");
    assert_eq!(extraction.method, expected_method);
    assert_eq!(extraction.pages.len(), expected_pages);
    assert_eq!(thin_page_numbers(&extraction.pages), Vec::<u32>::new());

    let vectors = extractor_suite::text_vectors();
    extractor_suite::extractor_contract(&extractor, &vectors, &|ext| {
        if ext.method == fieldbook_core::extraction::methods::PLAIN {
            Ok(())
        } else {
            Err(format!(
                "expected method {}, got {}",
                fieldbook_core::extraction::methods::PLAIN,
                ext.method
            ))
        }
    });
}

#[test]
fn contract_extractor_html_strip_1() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/extractor/html-strip@1");
    let input = std::fs::read(fix_dir.join("input.html")).expect("read input.html");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");

    let expected_method = expect
        .get("method")
        .and_then(toml::Value::as_str)
        .expect("method");
    let expected_pages = usize::try_from(
        expect
            .get("pages")
            .and_then(toml::Value::as_integer)
            .expect("pages"),
    )
    .expect("positive pages");

    let extractor = HtmlStrip;
    let extraction = extractor.extract(&input).expect("extract html-strip");
    assert_eq!(extraction.method, expected_method);
    assert_eq!(extraction.pages.len(), expected_pages);
    assert_eq!(thin_page_numbers(&extraction.pages), Vec::<u32>::new());

    let vectors = extractor_suite::text_vectors();
    extractor_suite::extractor_contract(&extractor, &vectors, &|ext| {
        if ext.method == fieldbook_core::extraction::methods::HTML_STRIP {
            Ok(())
        } else {
            Err(format!(
                "expected method {}, got {}",
                fieldbook_core::extraction::methods::HTML_STRIP,
                ext.method
            ))
        }
    });
}

#[test]
fn contract_backup_folder_1() {
    assert_eq!(fieldbook::io::backup_folder::FolderTarget::ID, "folder@1");

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/backup/folder@1");
    let input = std::fs::read(fix_dir.join("input.txt")).expect("read input.txt");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");
    let expected_sha = expect
        .get("sha256")
        .and_then(toml::Value::as_str)
        .expect("sha256");

    let actual_sha = Sha256::digest(&input);
    assert_eq!(actual_sha.to_string(), expected_sha);

    support::backup_suite::backup_target_contract(&FolderTargetFactory);
}

#[test]
fn contract_store_folder_1() {
    support::store_suite::store_suite(&|p| Disk::with_state(p, p.join(".fieldbook")));

    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/store/folder@1");
    let input = std::fs::read(fix_dir.join("input.txt")).expect("read input.txt");
    let expect_toml =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");
    let expect: toml::Table = toml::from_str(&expect_toml).expect("parse expect.toml");
    let expected_path = expect
        .get("path")
        .and_then(toml::Value::as_str)
        .expect("path");
    let expected_sha = expect
        .get("sha256")
        .and_then(toml::Value::as_str)
        .expect("sha256");

    let actual_sha = Sha256::digest(&input);
    assert_eq!(actual_sha.to_string(), expected_sha);

    let tmp = tempfile::tempdir().expect("create tempdir");
    std::fs::create_dir_all(tmp.path().join("corpus")).expect("create corpus");
    std::fs::create_dir_all(tmp.path().join(".fieldbook")).expect("create .fieldbook");
    let disk = Disk::with_state(tmp.path(), tmp.path().join(".fieldbook"));

    let written_sha = support::store_suite::write_store_file(&disk, expected_path, &input)
        .expect("write_store_file");
    assert_eq!(written_sha.to_string(), expected_sha);
}

#[test]
fn contract_authz_policy_readers_1() {
    support::authz_suite::authz_suite(&make_policy_readers);

    // Replay fixture
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fix_dir = repo_root.join("tests/fixtures/ports/authz/policy-readers@1");
    let input_str = std::fs::read_to_string(fix_dir.join("input.toml")).expect("read input.toml");
    let expect_str =
        std::fs::read_to_string(fix_dir.join("expect.toml")).expect("read expect.toml");

    let input_table: toml::Table = toml::from_str(&input_str).expect("parse input.toml");
    let expect_table: toml::Table = toml::from_str(&expect_str).expect("parse expect.toml");

    let mut groups = support::authz_suite::ScriptedGroups::new();
    if let Some(grp_tbl) = input_table.get("groups").and_then(toml::Value::as_table) {
        for (k, v) in grp_tbl {
            let ans = match v {
                toml::Value::Boolean(b) => Some(*b),
                toml::Value::String(s) if s == "unreadable" => None,
                _ => panic!("unexpected group answer: {v:?}"),
            };
            groups = groups.with_answer(k, ans);
        }
    }

    let mut store_specs = Vec::new();
    if let Some(store_arr) = input_table.get("store").and_then(toml::Value::as_array) {
        for st in store_arr {
            let id_str = st
                .get("id")
                .and_then(toml::Value::as_str)
                .expect("store id");
            let store_id = match id_str {
                "corpus" => StoreId::Corpus,
                "overlay" => StoreId::Overlay,
                "working" => StoreId::Working,
                "state" => StoreId::State,
                other => panic!("unknown store id {other}"),
            };
            let readers = st.get("readers").and_then(toml::Value::as_str);
            let changers = st.get("changers").and_then(toml::Value::as_str);
            store_specs.push((store_id, readers, changers));
        }
    }

    let authz = make_policy_readers(&store_specs, &groups);
    let actor = sample_actor();

    for (store_name, acts_val) in expect_table {
        let store_id = match store_name.as_str() {
            "corpus" => StoreId::Corpus,
            "overlay" => StoreId::Overlay,
            "working" => StoreId::Working,
            "state" => StoreId::State,
            other => panic!("unknown store in expect: {other}"),
        };
        let acts_tbl = acts_val.as_table().expect("store acts table");
        let scope = Scope::for_store(store_id);

        for (act_name, expected_val) in acts_tbl {
            let act = match act_name.as_str() {
                "read" => Act::Read,
                "propose" => Act::Propose,
                "accept" => Act::Accept,
                "export" => Act::Export,
                "admin" => Act::Admin,
                other => panic!("unknown act {other}"),
            };
            let expected_str = expected_val.as_str().expect("expected str");
            let decision = authz.allow(&actor, act, &scope);
            let actual_str = match decision {
                AuthzDecision::Allow => "allow".to_string(),
                AuthzDecision::Deny(word) => format!("deny:{}", word.as_str()),
            };
            assert_eq!(
                actual_str, expected_str,
                "store {store_name} act {act_name} failed fixture expectation"
            );
        }
    }
}

#[test]
fn caps_streaming_is_false_for_built_adapters() {
    for row in ADAPTERS {
        if row.state == State::Built {
            assert!(
                !row.caps().streaming,
                "{}: built adapter must have caps().streaming == false",
                row.id
            );
        }
    }

    // Local helper that takes Caps: verify it fails on synthetic row with streaming == true
    fn check_streaming(state: State, caps: Caps) -> Result<(), &'static str> {
        if state == State::Built && caps.streaming {
            return Err("streaming must be false for built adapter");
        }
        Ok(())
    }

    let streaming_caps = Caps::new(false, false, false, true, 0);
    assert!(check_streaming(State::Built, streaming_caps).is_err());
}

fn heading_anchor(port: Port, id: &str) -> String {
    let raw = format!("{} {}", port.as_str(), id);
    raw.to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .replace(' ', "-")
}

fn extract_backticked(line: &str) -> Option<&str> {
    let start = line.find('`')? + 1;
    let end = line[start..].find('`')? + start;
    Some(&line[start..end])
}

fn check_ports_md_tables(ports_md: &str, rows: &[Row]) -> Result<(), String> {
    let marker_begin = "<!-- generated:ports -->";
    let marker_end = "<!-- /generated:ports -->";
    let Some(start) = ports_md.find(marker_begin) else {
        return Err("marker <!-- generated:ports --> not found in ports.md".to_owned());
    };
    let Some(end) = ports_md.find(marker_end) else {
        return Err("marker <!-- /generated:ports --> not found in ports.md".to_owned());
    };
    let generated_section = &ports_md[start + marker_begin.len()..end];

    let mut table_ids = BTreeSet::new();

    for section in generated_section.split("\n## ") {
        let first_line = section.lines().next().unwrap_or("").trim();
        if first_line.to_ascii_lowercase().starts_with("templates") {
            continue;
        }
        for line in section.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with('|')
                && trimmed.contains('`')
                && let Some(id) = extract_backticked(trimmed)
            {
                table_ids.insert(id.to_owned());
            }
        }
    }

    for row in rows {
        if !table_ids.contains(row.id) {
            return Err(format!(
                "registry id {} missing from ports.md generated tables",
                row.id
            ));
        }
    }

    for id in &table_ids {
        if !rows.iter().any(|r| r.id == id.as_str()) {
            return Err(format!(
                "ports.md generated tables contain unknown id {}",
                id
            ));
        }
    }

    Ok(())
}

fn extract_string_literal(s: &str) -> Option<&str> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"')? + start;
    Some(&s[start..end])
}

fn find_const_id(content: &str) -> Option<&str> {
    let idx = content.find("const ID:")?;
    let rest = &content[idx..];
    let start = rest.find('"')? + 1;
    let end = rest[start..].find('"')? + start;
    Some(&rest[start..end])
}

fn collect_rs_files(
    dir: &Path,
    krate_name: &str,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) != Some("tests") {
                collect_rs_files(&path, krate_name, out)?;
            }
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs")
            && path.file_name().and_then(|n| n.to_str()) != Some("doubles.rs")
            && !path.components().any(|c| c.as_os_str() == "tests")
        {
            out.push((path, krate_name.to_owned()));
        }
    }
    Ok(())
}

fn scan_impls_in_file(
    file_path: &Path,
    crate_name: &str,
    content: &str,
    rows: &[Row],
) -> Result<(), String> {
    let mut search_idx = 0;
    while let Some(impl_pos) = content[search_idx..].find("impl") {
        let actual_impl_pos = search_idx + impl_pos;
        search_idx = actual_impl_pos + 4;

        // Check word boundary
        if actual_impl_pos > 0 {
            let prev = content.as_bytes()[actual_impl_pos - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' {
                continue;
            }
        }
        let after_impl = &content[search_idx..];
        let Some(first_char) = after_impl.chars().next() else {
            break;
        };
        if first_char.is_ascii_alphanumeric() || first_char == '_' {
            continue;
        }

        let Some(open_brace_rel) = after_impl.find('{') else {
            break;
        };
        let header = &after_impl[..open_brace_rel];
        if header.contains(';') {
            continue;
        }

        let Some(for_rel) = header.find(" for ") else {
            continue;
        };

        let trait_part = header[..for_rel].trim();
        let type_part = header[for_rel + 5..].trim();

        let trait_name = trait_part
            .split('<')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .last()
            .unwrap_or("");
        let type_name = type_part
            .split('<')
            .next()
            .unwrap_or("")
            .split_whitespace()
            .next()
            .unwrap_or("");

        let port = match trait_name {
            "Extractor" => Port::Extractor,
            "Authz" => Port::Authz,
            "BackupTarget" => Port::Backup,
            "World" => {
                if type_name == "Disk" {
                    Port::Store
                } else {
                    continue;
                }
            }
            _ => continue,
        };

        // Extract impl body
        let body_start = search_idx + open_brace_rel + 1;
        let mut depth = 1;
        let mut body_end = body_start;
        for (i, b) in content[body_start..].bytes().enumerate() {
            if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
                if depth == 0 {
                    body_end = body_start + i;
                    break;
                }
            }
        }
        let impl_body = &content[body_start..body_end];

        let id = if port == Port::Store {
            "folder@1".to_string()
        } else if let Some(fn_id_pos) = impl_body.find("fn id(") {
            let fn_rest = &impl_body[fn_id_pos..];
            let Some(fn_brace_rel) = fn_rest.find('{') else {
                return Err(format!("{}: fn id missing body", file_path.display()));
            };
            let fn_body_start = fn_id_pos + fn_brace_rel + 1;
            let mut fn_depth = 1;
            let mut fn_body_end = fn_body_start;
            for (i, b) in impl_body[fn_body_start..].bytes().enumerate() {
                if b == b'{' {
                    fn_depth += 1;
                } else if b == b'}' {
                    fn_depth -= 1;
                    if fn_depth == 0 {
                        fn_body_end = fn_body_start + i;
                        break;
                    }
                }
            }
            let fn_body = &impl_body[fn_body_start..fn_body_end];

            if let Some(lit) = extract_string_literal(fn_body) {
                lit.to_string()
            } else if fn_body.contains("PLAIN_ID") {
                "plain@1".to_string()
            } else if fn_body.contains("HTML_STRIP_ID") {
                "html-strip@1".to_string()
            } else if fn_body.contains("PDFTOTEXT_LAYOUT_ID") {
                "pdftotext-layout@1".to_string()
            } else if let Some(lit) = find_const_id(content) {
                lit.to_string()
            } else {
                return Err(format!(
                    "{}: unresolvable id in fn id() for impl {} for {}",
                    file_path.display(),
                    trait_name,
                    type_name
                ));
            }
        } else if let Some(lit) = find_const_id(content) {
            lit.to_string()
        } else {
            return Err(format!(
                "{}: unresolvable id for impl {} for {}",
                file_path.display(),
                trait_name,
                type_name
            ));
        };

        let row = rows.iter().find(|r| r.port == port && r.id == id);
        let Some(row) = row else {
            return Err(format!(
                "{}: impl for trait {} with id {} has no registry row",
                file_path.display(),
                trait_name,
                id
            ));
        };

        if row.krate != crate_name {
            return Err(format!(
                "{}: row crate {} does not match file crate {}",
                file_path.display(),
                row.krate,
                crate_name
            ));
        }
    }

    Ok(())
}

fn scan_sources(repo: &Path, rows: &[Row]) -> Result<(), String> {
    let mut files = Vec::new();
    let crates_dir = repo.join("crates");
    if crates_dir.is_dir()
        && let Ok(entries) = std::fs::read_dir(&crates_dir)
    {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let krate_name = entry.file_name().to_string_lossy().to_string();
                let src_dir = entry.path().join("src");
                if src_dir.is_dir() {
                    collect_rs_files(&src_dir, &krate_name, &mut files)?;
                }
            }
        }
    }

    let xtask_src = repo.join("xtask/src");
    if xtask_src.is_dir() {
        collect_rs_files(&xtask_src, "xtask", &mut files)?;
    }

    for (file_path, krate_name) in files {
        let content = std::fs::read_to_string(&file_path)
            .map_err(|e| format!("{}: {e}", file_path.display()))?;

        let content_to_scan = if let Some(idx) = content.find("#[cfg(test)]") {
            &content[..idx]
        } else {
            &content[..]
        };

        scan_impls_in_file(&file_path, &krate_name, content_to_scan, rows)?;
    }

    Ok(())
}

/// Pure registry drift verification function.
pub fn registry_drift(
    rows: &[Row],
    repo: &Path,
    ports_md: &str,
    known_keys: Option<&[&str]>,
) -> Result<(), String> {
    // 1. Scan source files for trait implementations
    scan_sources(repo, rows)?;

    // 2. Check generated tables in ports.md
    check_ports_md_tables(ports_md, rows)?;

    // 3. For each row, check contracts, fixtures, and documentation headings
    for row in rows {
        if let Some(keys) = known_keys {
            for key in row.settings_keys {
                if !keys.contains(key) {
                    return Err(format!("{}: unknown settings key {key}", row.id));
                }
            }
        }

        if row.state == State::Built {
            // Check contract test function exists
            let contract_fn = format!("fn {}(", row.contract_name());
            let test_file = if row.krate == "xtask" {
                repo.join("xtask/src/extract.rs")
            } else {
                repo.join("crates/fieldbook/tests/adapters.rs")
            };

            let test_content = std::fs::read_to_string(&test_file)
                .map_err(|e| format!("read {}: {e}", test_file.display()))?;
            if !test_content.contains(&contract_fn) {
                return Err(format!(
                    "row {}: contract test {} not found in {}",
                    row.id,
                    contract_fn,
                    test_file.display()
                ));
            }

            // Check fixture folder
            let fix_dir = repo.join(row.fixtures);
            if !fix_dir.is_dir() {
                return Err(format!(
                    "row {}: fixtures dir {} does not exist",
                    row.id, row.fixtures
                ));
            }
            if !fix_dir.join("expect.toml").is_file() {
                return Err(format!(
                    "row {}: missing expect.toml in {}",
                    row.id, row.fixtures
                ));
            }
            let entries = std::fs::read_dir(&fix_dir).map_err(|e| e.to_string())?;
            let has_input = entries
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with("input."));
            if !has_input {
                return Err(format!(
                    "row {}: missing input.* file in {}",
                    row.id, row.fixtures
                ));
            }

            // Check doc heading and anchor
            let expected_heading = format!("### {} {}", row.port.as_str(), row.id);
            if !ports_md.contains(&expected_heading) {
                return Err(format!(
                    "row {}: heading '{}' not found in ports.md",
                    row.id, expected_heading
                ));
            }
            let anchor = heading_anchor(row.port, row.id);
            if anchor != row.doc {
                return Err(format!(
                    "row {}: expected doc anchor {}, got {}",
                    row.id, anchor, row.doc
                ));
            }
        }
    }

    Ok(())
}

fn synthetic_ports_md(rows: &[Row]) -> String {
    let mut out = String::from("# Ports and Adapters\n\n<!-- generated:ports -->\n");
    for &port in &Port::ALL {
        let port_rows: Vec<&Row> = rows.iter().filter(|r| r.port == port).collect();
        if !port_rows.is_empty() {
            out.push_str(&format!("\n## {}\n\n", port.as_str()));
            out.push_str("| ID | Default | State |\n|---|---|---|\n");
            for r in port_rows {
                out.push_str(&format!(
                    "| `{}` | {} | {} |\n",
                    r.id,
                    r.default,
                    r.state.as_str()
                ));
            }
        }
    }
    out.push_str("\n<!-- /generated:ports -->\n\n");
    for r in rows {
        out.push_str(&format!(
            "### {} {}\n\nDocumentation text.\n\n",
            r.port.as_str(),
            r.id
        ));
    }
    out
}

#[test]
fn adapters_match_registry_docs_and_fixtures() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ports_md =
        std::fs::read_to_string(repo_root.join("docs/reference/ports.md")).expect("read ports.md");
    registry_drift(ADAPTERS, &repo_root, &ports_md, None)
        .expect("adapters should match registry docs and fixtures");
}

#[test]
fn drift_passes_on_real_repo_with_synthetic_ports_md() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ports_md = synthetic_ports_md(ADAPTERS);
    registry_drift(ADAPTERS, &repo_root, &ports_md, Some(&[]))
        .expect("drift check should pass on real repo with synthetic ports_md");
}

#[test]
fn drift_names_a_planted_adapter_file() {
    let tmp = tempfile::tempdir().expect("create tempdir");
    let planted_dir = tmp.path().join("crates/fieldbook/src/adapters");
    std::fs::create_dir_all(&planted_dir).expect("create planted dir");
    let planted_file = planted_dir.join("planted.rs");
    std::fs::write(
        &planted_file,
        b"pub struct Planted;\n\nimpl Extractor for Planted {\n    fn id(&self) -> &'static str {\n        \"planted@1\"\n    }\n}\n",
    )
    .expect("write planted.rs");

    let ports_md = synthetic_ports_md(ADAPTERS);
    let res = registry_drift(ADAPTERS, tmp.path(), &ports_md, None);
    assert!(res.is_err(), "planted adapter must be rejected");
    let err = res.unwrap_err();
    assert!(
        err.contains("planted.rs"),
        "error message must name planted.rs: {err}"
    );
}

#[test]
fn drift_fails_when_a_row_is_missing() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let missing_rows: Vec<Row> = ADAPTERS
        .iter()
        .copied()
        .filter(|r| r.id != "plain@1")
        .collect();
    let ports_md = synthetic_ports_md(&missing_rows);
    let res = registry_drift(&missing_rows, &repo_root, &ports_md, None);
    assert!(res.is_err(), "missing plain@1 row must fail drift check");
    let err = res.unwrap_err();
    assert!(
        err.contains("plain@1"),
        "error must name missing row plain@1: {err}"
    );
}

#[test]
fn drift_fails_on_unknown_settings_key() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut custom_rows = ADAPTERS.to_vec();
    custom_rows[0].settings_keys = &["unknown_key"];
    let ports_md = synthetic_ports_md(&custom_rows);
    let res = registry_drift(&custom_rows, &repo_root, &ports_md, Some(&["known_key"]));
    assert!(res.is_err(), "unknown settings key must fail drift check");
    let err = res.unwrap_err();
    assert!(
        err.contains("unknown_key"),
        "error must name unknown setting: {err}"
    );
}

#[test]
fn drift_fails_on_wrong_crate() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut custom_rows = ADAPTERS.to_vec();
    custom_rows[0].krate = "wrong-crate";
    let ports_md = synthetic_ports_md(&custom_rows);
    let res = registry_drift(&custom_rows, &repo_root, &ports_md, None);
    assert!(res.is_err(), "wrong crate must fail drift check");
    let err = res.unwrap_err();
    assert!(
        err.contains("wrong-crate"),
        "error must name wrong crate: {err}"
    );
}

/// Pure helper for verifying that extraction methods match registered extractors and documentation.
pub fn check_methods_match_extractors(
    methods: &[Method],
    rows: &[Row],
    repo: &Path,
    corpus_format_md: &str,
) -> Result<(), String> {
    let extraction_section = corpus_format_md
        .split("### Extraction")
        .nth(1)
        .ok_or_else(|| "### Extraction section not found in corpus-format.md".to_owned())?;
    let key_table = extraction_section
        .split("| Key | Meaning |")
        .nth(1)
        .ok_or_else(|| "Key | Meaning table not found in corpus-format.md".to_owned())?
        .split("\n\n")
        .next()
        .unwrap_or("");

    for m in methods {
        if m.name == "supplied" {
            continue;
        }
        let Some(extractor_id) = m.extractor else {
            return Err(format!("{}: extractor id is None", m.name));
        };
        let is_registered = rows
            .iter()
            .any(|r| r.port == Port::Extractor && r.id == extractor_id);
        if !is_registered {
            return Err(format!(
                "{}: extractor {} not found in ADAPTERS",
                m.name, extractor_id
            ));
        }

        let fix_path = repo.join(m.fixture);
        if !fix_path.exists() {
            return Err(format!("{}: fixture {} does not exist", m.name, m.fixture));
        }

        for &key in m.keys {
            let pat1 = format!("`{key}`");
            let pat2 = format!("`[{key}]`");
            if !key_table.contains(&pat1) && !key_table.contains(&pat2) {
                return Err(format!(
                    "{}: key {} not found in Key | Meaning table",
                    m.name, key
                ));
            }
        }
    }

    for r in rows {
        if r.port == Port::Extractor && r.id != "staged@1" {
            let is_named = methods.iter().any(|m| m.extractor == Some(r.id));
            if !is_named {
                return Err(format!(
                    "{}: registered Extractor row not named by any METHODS row",
                    r.id
                ));
            }
        }
    }

    Ok(())
}

#[test]
fn methods_match_extractors() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_format_md =
        std::fs::read_to_string(repo_root.join("docs/reference/corpus-format.md"))
            .expect("read corpus-format.md");

    check_methods_match_extractors(METHODS, ADAPTERS, &repo_root, &corpus_format_md)
        .expect("methods must match extractors");
}

#[test]
fn methods_fails_on_unregistered_extractor() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let corpus_format_md =
        std::fs::read_to_string(repo_root.join("docs/reference/corpus-format.md"))
            .expect("read corpus-format.md");

    let mut custom_methods = METHODS.to_vec();
    custom_methods[0].extractor = Some("unregistered@1");

    let res =
        check_methods_match_extractors(&custom_methods, ADAPTERS, &repo_root, &corpus_format_md);
    assert!(
        res.is_err(),
        "unregistered extractor must fail methods check"
    );
    let err = res.unwrap_err();
    assert!(
        err.contains("unregistered@1"),
        "error must name unregistered extractor: {err}"
    );
}
