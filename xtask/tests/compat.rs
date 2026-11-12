//! Integration tests for `cargo xtask compat --base FILE` (`docs/reference/compatibility.md`).

#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    reason = "integration test builds synthetic git repositories and invokes the xtask binary"
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use fieldbook_core::cases::case_set_sha256_entries;
use fieldbook_core::hash::sha256_hex;

static COUNTER: AtomicU64 = AtomicU64::new(1);

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| panic!("xtask lives under repo root"))
        .to_path_buf()
}

fn xtask_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_xtask"))
}

/// The calculation and run-schema bindings of a manifest, without any adapter or template binding.
fn legacy_manifest(text: &str) -> String {
    let mut parts = text.split("[[binding]]\n");
    let header = parts.next().unwrap_or_default();
    let mut out = header.to_owned();
    for block in parts {
        if block.starts_with("kind = \"calculation\"") || block.starts_with("kind = \"run-schema\"")
        {
            out.push_str("[[binding]]\n");
            out.push_str(block);
        }
    }
    out
}

/// Insert `block` into `manifest` keeping bindings sorted by (kind, port, id).
fn insert_binding_sorted(manifest: &str, block: &str) -> String {
    fn key(b: &str) -> (u8, String, String) {
        let field = |name: &str| {
            b.lines()
                .find_map(|l| l.strip_prefix(name).and_then(|r| r.strip_prefix(" = \"")))
                .and_then(|r| r.strip_suffix('"'))
                .unwrap_or_default()
                .to_owned()
        };
        let rank = match field("kind").as_str() {
            "adapter" => 0,
            "calculation" => 1,
            "run-schema" => 2,
            "schema-keyword" => 3,
            _ => 4,
        };
        (rank, field("port"), field("id"))
    }
    let mut parts = manifest.split("[[binding]]\n");
    let header = parts.next().unwrap_or_default().to_owned();
    let mut blocks: Vec<String> = parts
        .map(|b| format!("[[binding]]\n{}", b.trim_end_matches('\n')))
        .collect();
    blocks.push(format!(
        "[[binding]]\n{}",
        block
            .trim_start_matches("[[binding]]\n")
            .trim_end_matches('\n')
    ));
    blocks.sort_by_key(|b| key(b.strip_prefix("[[binding]]\n").unwrap_or(b)));
    let mut out = header;
    for b in blocks {
        out.push_str(&b);
        out.push_str("\n\n");
    }
    out.trim_end_matches('\n').to_owned() + "\n"
}

fn copy_dir_all(src: &Path, dst: &Path) {
    if !src.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(dst);
    if let Ok(entries) = std::fs::read_dir(src) {
        for entry in entries.flatten() {
            let path = entry.path();
            let dest = dst.join(entry.file_name());
            if path.is_dir() {
                copy_dir_all(&path, &dest);
            } else if path.is_file() {
                let _ = std::fs::copy(&path, &dest);
            }
        }
    }
}

type CompatSpec<'a> = (&'a str, &'a str, &'a [(String, String)]);

struct TempRepo {
    dir: PathBuf,
}

struct BindingSpec<'a> {
    kind: &'a str,
    port: Option<&'a str>,
    id: &'a str,
    runner: &'a str,
    dir: &'a str,
    impl_path: &'a str,
    cases: &'a [&'a str],
}

impl TempRepo {
    fn new(label: &str) -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "fieldbook-compat-test-{label}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("create temp repo dir: {e}"));
        Self { dir }
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
                std::fs::create_dir_all(parent)
                    .unwrap_or_else(|e| panic!("create parent dirs: {e}"));
            }
            std::fs::copy(&from, &to).unwrap_or_else(|e| panic!("copy {rel}: {e}"));
        }
        copy_dir_all(&src.join("compat"), &self.dir.join("compat"));
        copy_dir_all(
            &src.join("crates/fieldbook-core/src/versions"),
            &self.dir.join("crates/fieldbook-core/src/versions"),
        );
        copy_dir_all(
            &src.join("crates/fieldbook-core/src/ports/templates"),
            &self.dir.join("crates/fieldbook-core/src/ports/templates"),
        );
        copy_dir_all(
            &src.join("crates/fieldbook/src/io/versions"),
            &self.dir.join("crates/fieldbook/src/io/versions"),
        );
        copy_dir_all(
            &src.join("xtask/src/versions"),
            &self.dir.join("xtask/src/versions"),
        );
        // The synthetic scenarios below build on the calculation bindings alone; the real
        // adapter and template bindings are replayed by `published_adapters_and_templates_replay`.
        let legacy = legacy_manifest(&self.read("compat/published.toml"));
        self.write("compat/published.toml", &legacy);
    }

    /// Restore the workspace manifest and its registry so that coverage of every Built row is checked.
    fn use_real_manifest(&self) {
        let src = repo_root();
        self.write(
            "compat/published.toml",
            &std::fs::read_to_string(src.join("compat/published.toml"))
                .unwrap_or_else(|e| panic!("read real manifest: {e}")),
        );
        self.write(
            "crates/fieldbook/src/adapters/registry.rs",
            &std::fs::read_to_string(src.join("crates/fieldbook/src/adapters/registry.rs"))
                .unwrap_or_else(|e| panic!("read registry: {e}")),
        );
    }

    fn init_git(&self) {
        self.git(&["init", "-q"]);
        self.git(&["config", "user.name", "Synthetic Test"]);
        self.git(&["config", "user.email", "synthetic@example.invalid"]);
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.dir)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap_or_else(|e| panic!("git runs: {e}"));
        assert!(
            out.status.success(),
            "git {args:?} failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn commit_all(&self, msg: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg, "--allow-empty"]);
        self.git(&["rev-parse", "HEAD"])
    }

    fn tag(&self, tag: &str) -> String {
        self.git(&["tag", "-f", tag]);
        self.git(&["rev-parse", &format!("refs/tags/{tag}^{{commit}}")])
    }

    fn write(&self, rel: &str, content: &str) {
        let path = self.dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("create parent: {e}"));
        }
        std::fs::write(&path, content).unwrap_or_else(|e| panic!("write file: {e}"));
    }

    fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.join(rel)).unwrap_or_else(|e| panic!("read file: {e}"))
    }

    fn sha256(&self, rel: &str) -> String {
        let bytes = std::fs::read(self.dir.join(rel)).unwrap_or_else(|e| panic!("read bytes: {e}"));
        sha256_hex(&bytes)
    }

    fn compute_case_set_sha(&self) -> String {
        let mut pairs = Vec::new();
        for name in [
            "calc-01.toml",
            "calc-02.toml",
            "calc-03.toml",
            "calc-04.toml",
            "calc-90.toml",
        ] {
            let text = self.read(&format!("crates/fieldbook-calc/tests/cases/{name}"));
            pairs.push((name, text));
        }
        let refs: Vec<(&str, &str)> = pairs.iter().map(|(n, c)| (*n, c.as_str())).collect();
        case_set_sha256_entries(&refs)
    }

    fn compatibility_files_from_published(&self) -> Vec<(String, String)> {
        let pub_text = self.read("compat/published.toml");
        let parsed: toml::Value =
            toml::from_str(&pub_text).unwrap_or_else(|e| panic!("valid published.toml: {e}"));
        let mut map = BTreeMap::new();
        map.insert(
            "compat/published.toml".to_owned(),
            self.sha256("compat/published.toml"),
        );
        if let Some(bindings) = parsed.get("binding").and_then(toml::Value::as_array) {
            for b in bindings {
                if let Some(files) = b.get("files").and_then(toml::Value::as_array) {
                    for f in files {
                        let p = f["path"]
                            .as_str()
                            .unwrap_or_else(|| panic!("path string"))
                            .to_owned();
                        let s = f["sha256"]
                            .as_str()
                            .unwrap_or_else(|| panic!("sha256 string"))
                            .to_owned();
                        map.insert(p, s);
                    }
                }
            }
        }
        map.into_iter().collect()
    }

    fn write_release_baseline(
        &self,
        compat: Option<CompatSpec<'_>>,
        tool: Option<(&str, &str)>,
        edition: Option<(&str, &str)>,
    ) {
        let dummy_manifest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        let mut out = String::from("baseline_schema = 1\n\n[compatibility]\n");
        if let Some((tag, commit, files)) = compat {
            out.push_str(&format!(
                "kind = \"release\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\narchive_manifest_sha256 = \"{dummy_manifest}\"\n\n"
            ));
            for (path, sha) in files {
                out.push_str(&format!(
                    "[[compatibility.file]]\npath = \"{path}\"\nsha256 = \"{sha}\"\n\n"
                ));
            }
        } else {
            out.push_str("kind = \"empty\"\n\n");
        }

        out.push_str("[tool]\n");
        if let Some((tag, commit)) = tool {
            out.push_str(&format!(
                "kind = \"release\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\narchive_manifest_sha256 = \"{dummy_manifest}\"\n\n"
            ));
        } else {
            out.push_str("kind = \"empty\"\n\n");
        }

        out.push_str("[edition]\n");
        if let Some((tag, commit)) = edition {
            out.push_str(&format!(
                "kind = \"release\"\ntag = \"{tag}\"\ncommit = \"{commit}\"\narchive_manifest_sha256 = \"{dummy_manifest}\"\n"
            ));
        } else {
            out.push_str("kind = \"empty\"\n");
        }

        self.write("release-baseline.toml", &out);
    }

    fn refresh_published_hashes(&self) {
        let pub_text = self.read("compat/published.toml");
        let mut doc: toml::Value =
            toml::from_str(&pub_text).unwrap_or_else(|e| panic!("parse published.toml: {e}"));
        if let Some(bindings) = doc.get_mut("binding").and_then(toml::Value::as_array_mut) {
            for b in bindings {
                if let Some(files) = b.get_mut("files").and_then(toml::Value::as_array_mut) {
                    for f in files {
                        let path = f["path"]
                            .as_str()
                            .unwrap_or_else(|| panic!("path string"))
                            .to_owned();
                        let sha = self.sha256(&path);
                        f.as_table_mut()
                            .unwrap_or_else(|| panic!("file table"))
                            .insert("sha256".to_owned(), toml::Value::String(sha));
                    }
                }
            }
        }
        let formatted = toml::to_string_pretty(&doc)
            .unwrap_or_else(|e| panic!("serialize published.toml: {e}"));
        self.write("compat/published.toml", &formatted);
    }

    fn format_binding(&self, spec: &BindingSpec<'_>) -> String {
        let mut s = String::new();
        s.push_str("[[binding]]\n");
        s.push_str(&format!("kind = \"{}\"\n", spec.kind));
        if let Some(p) = spec.port {
            s.push_str(&format!("port = \"{p}\"\n"));
        }
        s.push_str(&format!("id = \"{}\"\n", spec.id));
        s.push_str(&format!("runner = \"{}\"\n\n", spec.runner));

        // contract
        let contract_path = format!("{}/contract.toml", spec.dir);
        s.push_str("[[binding.files]]\nrole = \"contract\"\n");
        s.push_str(&format!("path = \"{contract_path}\"\n"));
        s.push_str(&format!("sha256 = \"{}\"\n\n", self.sha256(&contract_path)));

        // expected
        for case in spec.cases {
            let exp_path = format!("{}/{case}.out.txt", spec.dir);
            s.push_str("[[binding.files]]\nrole = \"expected\"\n");
            s.push_str(&format!("case = \"{case}\"\n"));
            s.push_str(&format!("path = \"{exp_path}\"\n"));
            s.push_str(&format!("sha256 = \"{}\"\n\n", self.sha256(&exp_path)));
        }

        // implementation
        s.push_str("[[binding.files]]\nrole = \"implementation\"\n");
        s.push_str(&format!("path = \"{}\"\n", spec.impl_path));
        s.push_str(&format!("sha256 = \"{}\"\n\n", self.sha256(spec.impl_path)));

        // input
        for case in spec.cases {
            let in_path = format!("{}/{case}.in.toml", spec.dir);
            s.push_str("[[binding.files]]\nrole = \"input\"\n");
            s.push_str(&format!("case = \"{case}\"\n"));
            s.push_str(&format!("path = \"{in_path}\"\n"));
            s.push_str(&format!("sha256 = \"{}\"\n\n", self.sha256(&in_path)));
        }

        s
    }

    fn run_compat(&self, base_arg: &str) -> Output {
        Command::new(xtask_bin())
            .args(["compat", "--base", base_arg])
            .current_dir(&self.dir)
            .output()
            .unwrap_or_else(|e| panic!("run xtask compat: {e}"))
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn first_release_has_explicit_empty_base() {
    let repo = TempRepo::new("first-empty");
    repo.copy_from_workspace();
    let out = repo.run_compat("release-baseline.toml");
    assert!(
        out.status.success(),
        "expected empty baseline to pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Omitting [edition] table is rejected by strict baseline schema.
    repo.write(
        "release-baseline.toml",
        "baseline_schema = 1\n\n[compatibility]\nkind = \"empty\"\n\n[tool]\nkind = \"empty\"\n",
    );
    let out_missing = repo.run_compat("release-baseline.toml");
    assert!(!out_missing.status.success());
}

#[test]
fn missing_base_is_an_error() {
    let repo = TempRepo::new("missing-base");
    repo.copy_from_workspace();
    repo.init_git();
    repo.commit_all("initial");

    // 1. Non-existent baseline file.
    let out = repo.run_compat("nonexistent-baseline.toml");
    assert!(!out.status.success());

    // 2. Declared release tag does not exist in git history (never silently treated as empty).
    let files = repo.compatibility_files_from_published();
    repo.write_release_baseline(
        Some(("v0.1.0", "0123456789abcdef0123456789abcdef01234567", &files)),
        Some(("v0.1.0", "0123456789abcdef0123456789abcdef01234567")),
        None,
    );
    let out_unavail = repo.run_compat("release-baseline.toml");
    assert!(!out_unavail.status.success());
    let stderr = String::from_utf8_lossy(&out_unavail.stderr);
    assert!(stderr.contains("v0.1.0"), "unexpected stderr: {stderr}");
}

#[test]
fn old_calculation_and_cases_cannot_change_together() {
    let repo = TempRepo::new("old-calc-change");
    repo.copy_from_workspace();
    repo.init_git();
    repo.commit_all("v0.1.0 release");
    let commit = repo.tag("v0.1.0");
    let files = repo.compatibility_files_from_published();
    repo.write_release_baseline(
        Some(("v0.1.0", &commit, &files)),
        Some(("v0.1.0", &commit)),
        None,
    );

    let clean = repo.run_compat("release-baseline.toml");
    assert!(
        clean.status.success(),
        "clean baseline should pass: {}",
        String::from_utf8_lossy(&clean.stderr)
    );

    // Change both the old calculation implementation and its expected output together, and update published.toml hashes.
    let mut impl_src = repo.read("crates/fieldbook-calc/src/versions/build_up_v1.rs");
    impl_src.push_str("\n// COMPAT_TAMPER: changed old calculation\n");
    repo.write(
        "crates/fieldbook-calc/src/versions/build_up_v1.rs",
        &impl_src,
    );

    let mut out_txt = repo.read("compat/calculation/build_up@1/calc-01.out.txt");
    out_txt = out_txt.replace("2849.00", "2849.01");
    repo.write("compat/calculation/build_up@1/calc-01.out.txt", &out_txt);
    repo.refresh_published_hashes();

    let tampered = repo.run_compat("release-baseline.toml");
    assert!(
        !tampered.status.success(),
        "changing old calculation and expected output together must fail"
    );
}

#[test]
fn old_prompt_and_golden_cannot_change_together() {
    let repo = TempRepo::new("old-prompt-change");
    repo.copy_from_workspace();

    // Add a synthetic template binding to compat/published.toml before tagging v0.1.0.
    repo.write(
        "compat/template/ask-answer@1/contract.toml",
        "kind = \"template\"\nid = \"ask-answer@1\"\nsynthetic = true\n",
    );
    repo.write(
        "crates/fieldbook-core/src/ports/templates/ask-answer@1.txt",
        "Prompt v1: {{question}}\n",
    );
    repo.write(
        "compat/template/ask-answer@1/q-01.in.toml",
        "question = \"What is a rate pool?\"\n",
    );
    repo.write(
        "compat/template/ask-answer@1/q-01.out.txt",
        "Prompt v1: What is a rate pool?\n",
    );

    let mut pub_toml = repo.read("compat/published.toml");
    pub_toml.push_str(&format!(
        r#"
[[binding]]
kind = "template"
id = "ask-answer@1"
runner = "template"

[[binding.files]]
role = "contract"
path = "compat/template/ask-answer@1/contract.toml"
sha256 = "{}"

[[binding.files]]
role = "expected"
case = "q-01"
path = "compat/template/ask-answer@1/q-01.out.txt"
sha256 = "{}"

[[binding.files]]
role = "implementation"
path = "crates/fieldbook-core/src/ports/templates/ask-answer@1.txt"
sha256 = "{}"

[[binding.files]]
role = "input"
case = "q-01"
path = "compat/template/ask-answer@1/q-01.in.toml"
sha256 = "{}"
"#,
        repo.sha256("compat/template/ask-answer@1/contract.toml"),
        repo.sha256("compat/template/ask-answer@1/q-01.out.txt"),
        repo.sha256("crates/fieldbook-core/src/ports/templates/ask-answer@1.txt"),
        repo.sha256("compat/template/ask-answer@1/q-01.in.toml"),
    ));
    repo.write("compat/published.toml", &pub_toml);

    repo.init_git();
    repo.commit_all("v0.1.0 with template");
    let commit = repo.tag("v0.1.0");
    let files = repo.compatibility_files_from_published();
    repo.write_release_baseline(
        Some(("v0.1.0", &commit, &files)),
        Some(("v0.1.0", &commit)),
        None,
    );

    assert!(repo.run_compat("release-baseline.toml").status.success());

    // Now change both the prompt template and its expected golden together and refresh published.toml.
    repo.write(
        "crates/fieldbook-core/src/ports/templates/ask-answer@1.txt",
        "Prompt v1 edited: {{question}}\n",
    );
    repo.write(
        "compat/template/ask-answer@1/q-01.out.txt",
        "Prompt v1 edited: What is a rate pool?\n",
    );
    repo.refresh_published_hashes();

    let res = repo.run_compat("release-baseline.toml");
    assert!(
        !res.status.success(),
        "changing old prompt and golden together must fail"
    );
}

#[test]
fn new_version_keeps_old_vectors() {
    let repo = TempRepo::new("new-version");
    repo.copy_from_workspace();

    // Baseline v0.1.0 starts with build_up@1 and canonical-run@1.
    let full_pub = repo.read("compat/published.toml");
    let without_children: String = full_pub
        .split("[[binding]]")
        .filter(|chunk| !chunk.contains("id = \"children_sum@1\""))
        .collect::<Vec<_>>()
        .join("[[binding]]");
    repo.write("compat/published.toml", &without_children);
    repo.init_git();
    repo.commit_all("v0.1.0");
    let commit = repo.tag("v0.1.0");
    let files_v1 = repo.compatibility_files_from_published();

    // Now add the compiled version children_sum@1 and a versioned template ask-answer@2
    // alongside the existing bindings without changing build_up@1 or canonical-run@1.
    repo.write(
        "compat/template/ask-answer@2/contract.toml",
        "kind = \"template\"\nid = \"ask-answer@2\"\nsynthetic = true\n",
    );
    repo.write(
        "crates/fieldbook-core/src/ports/templates/ask-answer@2.txt",
        "Prompt v2: {{question}}\n",
    );
    repo.write(
        "compat/template/ask-answer@2/q-01.in.toml",
        "question = \"What is a rate pool?\"\n",
    );
    repo.write(
        "compat/template/ask-answer@2/q-01.out.txt",
        "Prompt v2: What is a rate pool?\n",
    );
    let template_binding = format!(
        r#"
[[binding]]
kind = "template"
id = "ask-answer@2"
runner = "template"

[[binding.files]]
role = "contract"
path = "compat/template/ask-answer@2/contract.toml"
sha256 = "{}"

[[binding.files]]
role = "expected"
case = "q-01"
path = "compat/template/ask-answer@2/q-01.out.txt"
sha256 = "{}"

[[binding.files]]
role = "implementation"
path = "crates/fieldbook-core/src/ports/templates/ask-answer@2.txt"
sha256 = "{}"

[[binding.files]]
role = "input"
case = "q-01"
path = "compat/template/ask-answer@2/q-01.in.toml"
sha256 = "{}"
"#,
        repo.sha256("compat/template/ask-answer@2/contract.toml"),
        repo.sha256("compat/template/ask-answer@2/q-01.out.txt"),
        repo.sha256("crates/fieldbook-core/src/ports/templates/ask-answer@2.txt"),
        repo.sha256("compat/template/ask-answer@2/q-01.in.toml"),
    );
    repo.write(
        "compat/published.toml",
        &format!("{full_pub}{template_binding}"),
    );
    repo.write_release_baseline(
        Some(("v0.1.0", &commit, &files_v1)),
        Some(("v0.1.0", &commit)),
        None,
    );

    let out = repo.run_compat("release-baseline.toml");
    assert!(
        out.status.success(),
        "adding new bindings while keeping archived bindings unchanged should pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Adding a calculation binding that is absent from compiled dispatch (`build_up@2`) must refuse.
    let v1_impl = repo.read("crates/fieldbook-calc/src/versions/build_up_v1.rs");
    repo.write(
        "crates/fieldbook-calc/src/versions/build_up_v2.rs",
        &v1_impl,
    );
    repo.write(
        "compat/calculation/build_up@2/contract.toml",
        "kind = \"calculation\"\nid = \"build_up@2\"\n",
    );
    let in_01 = repo
        .read("compat/calculation/build_up@1/calc-01.in.toml")
        .replace("build_up@1", "build_up@2");
    let out_01 = repo
        .read("compat/calculation/build_up@1/calc-01.out.txt")
        .replace("fn 10:build_up@1", "fn 10:build_up@2");
    let in_90 = repo
        .read("compat/calculation/build_up@1/calc-90.in.toml")
        .replace("build_up@1", "build_up@2");
    let out_90 = repo.read("compat/calculation/build_up@1/calc-90.out.txt");
    repo.write("compat/calculation/build_up@2/calc-01.in.toml", &in_01);
    repo.write("compat/calculation/build_up@2/calc-01.out.txt", &out_01);
    repo.write("compat/calculation/build_up@2/calc-90.in.toml", &in_90);
    repo.write("compat/calculation/build_up@2/calc-90.out.txt", &out_90);
    let uncompiled_calc = format!(
        r#"[[binding]]
kind = "calculation"
id = "build_up@2"
runner = "calculation"

[[binding.files]]
role = "contract"
path = "compat/calculation/build_up@2/contract.toml"
sha256 = "{}"

[[binding.files]]
role = "expected"
case = "calc-01"
path = "compat/calculation/build_up@2/calc-01.out.txt"
sha256 = "{}"

[[binding.files]]
role = "expected"
case = "calc-90"
path = "compat/calculation/build_up@2/calc-90.out.txt"
sha256 = "{}"

[[binding.files]]
role = "implementation"
path = "crates/fieldbook-calc/src/versions/build_up_v2.rs"
sha256 = "{}"

[[binding.files]]
role = "input"
case = "calc-01"
path = "compat/calculation/build_up@2/calc-01.in.toml"
sha256 = "{}"

[[binding.files]]
role = "input"
case = "calc-90"
path = "compat/calculation/build_up@2/calc-90.in.toml"
sha256 = "{}"

[[binding]]
kind = "calculation"
id = "children_sum@1""#,
        repo.sha256("compat/calculation/build_up@2/contract.toml"),
        repo.sha256("compat/calculation/build_up@2/calc-01.out.txt"),
        repo.sha256("compat/calculation/build_up@2/calc-90.out.txt"),
        repo.sha256("crates/fieldbook-calc/src/versions/build_up_v2.rs"),
        repo.sha256("compat/calculation/build_up@2/calc-01.in.toml"),
        repo.sha256("compat/calculation/build_up@2/calc-90.in.toml"),
    );
    repo.write(
        "compat/published.toml",
        &full_pub.replacen(
            "[[binding]]\nkind = \"calculation\"\nid = \"children_sum@1\"",
            &uncompiled_calc,
            1,
        ),
    );
    assert!(
        !repo.run_compat("release-baseline.toml").status.success(),
        "calculation binding absent from compiled CALC_FNS dispatch must refuse"
    );
}

#[test]
fn changed_cases_with_new_epoch_pass() {
    let repo = TempRepo::new("cases-new-epoch");
    repo.copy_from_workspace();
    repo.init_git();
    repo.commit_all("v0.1.0");
    let commit = repo.tag("v0.1.0");
    let files = repo.compatibility_files_from_published();
    repo.write_release_baseline(
        Some(("v0.1.0", &commit, &files)),
        Some(("v0.1.0", &commit)),
        None,
    );

    // Modify calc-02.toml and bump epoch from 1 to 2.
    let calc_02 = repo.read("crates/fieldbook-calc/tests/cases/calc-02.toml");
    repo.write(
        "crates/fieldbook-calc/tests/cases/calc-02.toml",
        &format!("{calc_02}# synthetic case comment\n"),
    );
    let new_sha = repo.compute_case_set_sha();
    let fns = repo.read("crates/fieldbook-calc/src/fns.rs").replace(
        "pub const CALC_EPOCH: u32 = 1;",
        "pub const CALC_EPOCH: u32 = 2;",
    );
    repo.write("crates/fieldbook-calc/src/fns.rs", &fns);
    repo.write(
        "crates/fieldbook-calc/tests/cases/EPOCH",
        &format!("2 {}\n", &new_sha[..16]),
    );

    let pub_toml = repo.read("compat/published.toml");
    let old_sha = pub_toml
        .lines()
        .find(|l| l.starts_with("case_set_sha256"))
        .unwrap_or_else(|| panic!("case_set_sha256 line"))
        .split('"')
        .nth(1)
        .unwrap_or_else(|| panic!("case_set_sha256 value"))
        .to_owned();
    let updated_pub = pub_toml
        .replace("calculation_epoch = 1", "calculation_epoch = 2")
        .replace(&old_sha, &new_sha);
    repo.write("compat/published.toml", &updated_pub);

    let out = repo.run_compat("release-baseline.toml");
    assert!(
        out.status.success(),
        "changed cases with bumped epoch should pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn changed_cases_require_a_new_epoch() {
    let repo = TempRepo::new("cases-same-epoch");
    repo.copy_from_workspace();
    repo.init_git();
    repo.commit_all("v0.1.0");
    let commit = repo.tag("v0.1.0");
    let files = repo.compatibility_files_from_published();
    repo.write_release_baseline(
        Some(("v0.1.0", &commit, &files)),
        Some(("v0.1.0", &commit)),
        None,
    );

    // Modify calc-02.toml and update digest in EPOCH and published.toml, but keep epoch = 1.
    let calc_02 = repo.read("crates/fieldbook-calc/tests/cases/calc-02.toml");
    repo.write(
        "crates/fieldbook-calc/tests/cases/calc-02.toml",
        &format!("{calc_02}# synthetic case comment\n"),
    );
    let new_sha = repo.compute_case_set_sha();
    repo.write(
        "crates/fieldbook-calc/tests/cases/EPOCH",
        &format!("1 {}\n", &new_sha[..16]),
    );
    let pub_toml = repo.read("compat/published.toml");
    let old_sha = pub_toml
        .lines()
        .find(|l| l.starts_with("case_set_sha256"))
        .unwrap_or_else(|| panic!("case_set_sha256 line"))
        .split('"')
        .nth(1)
        .unwrap_or_else(|| panic!("case_set_sha256 value"))
        .to_owned();
    repo.write(
        "compat/published.toml",
        &pub_toml.replace(&old_sha, &new_sha),
    );

    let out = repo.run_compat("release-baseline.toml");
    assert!(
        !out.status.success(),
        "changed cases with unchanged epoch must fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("minimum permitted epoch: 2"),
        "expected minimum permitted epoch in stderr, got: {stderr}"
    );
}

#[test]
fn published_manifest_round_trips() {
    let repo = TempRepo::new("round-trip");
    repo.copy_from_workspace();

    let original = repo.read("compat/published.toml");
    let value: toml::Value =
        toml::from_str(&original).unwrap_or_else(|e| panic!("parse TOML: {e}"));
    let bindings = value["binding"]
        .as_array()
        .unwrap_or_else(|| panic!("binding array"));
    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[0]["id"].as_str(), Some("build_up@1"));
    assert_eq!(bindings[1]["id"].as_str(), Some("children_sum@1"));
    assert_eq!(bindings[2]["id"].as_str(), Some("canonical-run@1"));

    repo.refresh_published_hashes();
    let out = repo.run_compat("release-baseline.toml");
    assert!(
        out.status.success(),
        "round-tripped published.toml should pass: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn malformed_duplicate_or_missing_binding_refuses() {
    let repo = TempRepo::new("malformed-refuses");
    repo.copy_from_workspace();
    let clean_pub = repo.read("compat/published.toml");

    // 1. Missing binding for a registered CALC_FNS row (`children_sum@1` removed).
    let without_children: String = clean_pub
        .split("[[binding]]")
        .filter(|chunk| !chunk.contains("id = \"children_sum@1\""))
        .collect::<Vec<_>>()
        .join("[[binding]]");
    repo.write("compat/published.toml", &without_children);
    assert!(
        !repo.run_compat("release-baseline.toml").status.success(),
        "missing CALC_FNS binding must fail"
    );

    // 2. Duplicate binding identity.
    let dup = format!(
        "{clean_pub}\n[[binding]]\n{}",
        clean_pub
            .split("[[binding]]")
            .nth(1)
            .unwrap_or_else(|| panic!("first binding block"))
    );
    repo.write("compat/published.toml", &dup);
    assert!(
        !repo.run_compat("release-baseline.toml").status.success(),
        "duplicate binding must fail"
    );

    // 3. Shared registry file (`crates/fieldbook-calc/src/fns.rs`) bound as implementation.
    let shared_path = clean_pub.replace(
        "crates/fieldbook-calc/src/versions/build_up_v1.rs",
        "crates/fieldbook-calc/src/fns.rs",
    );
    repo.write("compat/published.toml", &shared_path);
    assert!(
        !repo.run_compat("release-baseline.toml").status.success(),
        "binding shared fns.rs must fail"
    );

    // 4. Malformed relative path with `..`.
    let bad_path = clean_pub.replace(
        "compat/calculation/build_up@1/contract.toml",
        "compat/calculation/../build_up@1/contract.toml",
    );
    repo.write("compat/published.toml", &bad_path);
    assert!(
        !repo.run_compat("release-baseline.toml").status.success(),
        "parent segment in path must fail"
    );
}

#[test]
fn historical_vectors_use_current_version_dispatch() {
    let repo = TempRepo::new("dispatch");
    repo.copy_from_workspace();
    let clean_pub = repo.read("compat/published.toml");

    // 1. Tampering with the current version implementation causes historical vector execution to fail.
    let orig_impl = repo.read("crates/fieldbook-calc/src/versions/build_up_v1.rs");
    repo.write(
        "crates/fieldbook-calc/src/versions/build_up_v1.rs",
        &format!("{orig_impl}\n// COMPAT_TAMPER\n"),
    );
    repo.refresh_published_hashes();
    let tampered = repo.run_compat("release-baseline.toml");
    assert!(
        !tampered.status.success(),
        "tampered current implementation must fail vector execution"
    );
    repo.write(
        "crates/fieldbook-calc/src/versions/build_up_v1.rs",
        &orig_impl,
    );
    repo.write("compat/published.toml", &clean_pub);

    // 2. Modifying an expected output by one cent fails vector execution through current dispatch.
    let orig_out = repo.read("compat/calculation/build_up@1/calc-01.out.txt");
    repo.write(
        "compat/calculation/build_up@1/calc-01.out.txt",
        &orig_out.replace("2849.00", "2849.01"),
    );
    repo.refresh_published_hashes();
    assert!(
        !repo.run_compat("release-baseline.toml").status.success(),
        "altered expected vector output must fail current dispatch"
    );
    repo.write("compat/calculation/build_up@1/calc-01.out.txt", &orig_out);
    repo.write("compat/published.toml", &clean_pub);

    // 3. Unknown adapter runner refuses (never skipped).
    repo.write("compat/adapter/mock@1/contract.toml", "id = \"mock@1\"\n");
    repo.write("compat/adapter/mock@1/impl.rs", "// mock\n");
    repo.write("compat/adapter/mock@1/c1.in.toml", "x = 1\n");
    repo.write("compat/adapter/mock@1/c1.out.txt", "1\n");
    let adapter_block = format!(
        r#"[[binding]]
kind = "adapter"
port = "model"
id = "mock@1"
runner = "adapter"

[[binding.files]]
role = "contract"
path = "compat/adapter/mock@1/contract.toml"
sha256 = "{}"

[[binding.files]]
role = "expected"
case = "c1"
path = "compat/adapter/mock@1/c1.out.txt"
sha256 = "{}"

[[binding.files]]
role = "implementation"
path = "compat/adapter/mock@1/impl.rs"
sha256 = "{}"

[[binding.files]]
role = "input"
case = "c1"
path = "compat/adapter/mock@1/c1.in.toml"
sha256 = "{}"

[[binding]]
kind = "calculation"
id = "build_up@1""#,
        repo.sha256("compat/adapter/mock@1/contract.toml"),
        repo.sha256("compat/adapter/mock@1/c1.out.txt"),
        repo.sha256("compat/adapter/mock@1/impl.rs"),
        repo.sha256("compat/adapter/mock@1/c1.in.toml"),
    );
    repo.write(
        "compat/published.toml",
        &clean_pub.replacen(
            "[[binding]]\nkind = \"calculation\"\nid = \"build_up@1\"",
            &adapter_block,
            1,
        ),
    );
    assert!(
        !repo.run_compat("release-baseline.toml").status.success(),
        "unregistered adapter runner must refuse"
    );
}

#[test]
fn baseline_streams_remain_independent() {
    // Sequence 1: raw edition-001 first, then first tool v0.1.0.
    let repo_a = TempRepo::new("streams-a");
    repo_a.copy_from_workspace();
    let full_pub = repo_a.read("compat/published.toml");
    repo_a.init_git();
    repo_a.commit_all("edition-001 release");
    let ed_commit = repo_a.tag("edition-001");
    let files_ed = repo_a.compatibility_files_from_published();

    // After edition-001: compatibility = edition-001, tool = empty, edition = edition-001.
    repo_a.write_release_baseline(
        Some(("edition-001", &ed_commit, &files_ed)),
        None,
        Some(("edition-001", &ed_commit)),
    );
    assert!(repo_a.run_compat("release-baseline.toml").status.success());

    // Now v0.1.0 tool release adds template ask-answer@1 and advances compatibility & tool while preserving edition-001.
    repo_a.write(
        "compat/template/ask-answer@1/contract.toml",
        "kind = \"template\"\nid = \"ask-answer@1\"\nsynthetic = true\n",
    );
    repo_a.write(
        "crates/fieldbook-core/src/ports/templates/ask-answer@1.txt",
        "Prompt v1: {{question}}\n",
    );
    repo_a.write(
        "compat/template/ask-answer@1/q-01.in.toml",
        "question = \"What is a rate pool?\"\n",
    );
    repo_a.write(
        "compat/template/ask-answer@1/q-01.out.txt",
        "Prompt v1: What is a rate pool?\n",
    );
    let template_binding = format!(
        r#"
[[binding]]
kind = "template"
id = "ask-answer@1"
runner = "template"

[[binding.files]]
role = "contract"
path = "compat/template/ask-answer@1/contract.toml"
sha256 = "{}"

[[binding.files]]
role = "expected"
case = "q-01"
path = "compat/template/ask-answer@1/q-01.out.txt"
sha256 = "{}"

[[binding.files]]
role = "implementation"
path = "crates/fieldbook-core/src/ports/templates/ask-answer@1.txt"
sha256 = "{}"

[[binding.files]]
role = "input"
case = "q-01"
path = "compat/template/ask-answer@1/q-01.in.toml"
sha256 = "{}"
"#,
        repo_a.sha256("compat/template/ask-answer@1/contract.toml"),
        repo_a.sha256("compat/template/ask-answer@1/q-01.out.txt"),
        repo_a.sha256("crates/fieldbook-core/src/ports/templates/ask-answer@1.txt"),
        repo_a.sha256("compat/template/ask-answer@1/q-01.in.toml"),
    );
    repo_a.write(
        "compat/published.toml",
        &format!("{full_pub}{template_binding}"),
    );
    repo_a.commit_all("v0.1.0 release");
    let v1_commit = repo_a.tag("v0.1.0");
    let files_v1 = repo_a.compatibility_files_from_published();
    repo_a.write_release_baseline(
        Some(("v0.1.0", &v1_commit, &files_v1)),
        Some(("v0.1.0", &v1_commit)),
        Some(("edition-001", &ed_commit)),
    );
    assert!(repo_a.run_compat("release-baseline.toml").status.success());

    // Sequence 2: tool v0.1.0 -> edition-002 -> tool v0.2.0.
    repo_a.commit_all("edition-002 release");
    let ed2_commit = repo_a.tag("edition-002");
    repo_a.write_release_baseline(
        Some(("edition-002", &ed2_commit, &files_v1)),
        Some(("v0.1.0", &v1_commit)),
        Some(("edition-002", &ed2_commit)),
    );
    assert!(repo_a.run_compat("release-baseline.toml").status.success());

    repo_a.commit_all("v0.2.0 release");
    let v2_commit = repo_a.tag("v0.2.0");
    repo_a.write_release_baseline(
        Some(("v0.2.0", &v2_commit, &files_v1)),
        Some(("v0.2.0", &v2_commit)),
        Some(("edition-002", &ed2_commit)),
    );
    assert!(repo_a.run_compat("release-baseline.toml").status.success());

    // Overwriting [tool] with an edition tag or declaring a missing predecessor refuses.
    repo_a.write_release_baseline(
        Some(("v0.2.0", &v2_commit, &files_v1)),
        Some(("edition-002", &ed2_commit)),
        Some(("edition-002", &ed2_commit)),
    );
    assert!(
        !repo_a.run_compat("release-baseline.toml").status.success(),
        "overwriting tool stream with edition tag must fail"
    );

    repo_a.write_release_baseline(
        Some(("v0.2.0", &v2_commit, &files_v1)),
        Some(("v0.9.9", &v2_commit)),
        Some(("edition-002", &ed2_commit)),
    );
    assert!(
        !repo_a.run_compat("release-baseline.toml").status.success(),
        "missing tool predecessor tag must fail"
    );
}

#[test]
fn stale_manifest_case_digest_refuses() {
    let repo = TempRepo::new("stale-digest");
    repo.copy_from_workspace();

    // Update calc-02.toml and EPOCH on disk, leaving compat/published.toml with the stale case_set_sha256.
    let calc_02 = repo.read("crates/fieldbook-calc/tests/cases/calc-02.toml");
    repo.write(
        "crates/fieldbook-calc/tests/cases/calc-02.toml",
        &format!("{calc_02}# updated case\n"),
    );
    let new_sha = repo.compute_case_set_sha();
    repo.write(
        "crates/fieldbook-calc/tests/cases/EPOCH",
        &format!("1 {}\n", &new_sha[..16]),
    );

    let out = repo.run_compat("release-baseline.toml");
    assert!(
        !out.status.success(),
        "stale case_set_sha256 in compat/published.toml must refuse"
    );
}

#[test]
fn manifest_epoch_must_match_compiled_epoch() {
    let repo = TempRepo::new("epoch-mismatch");
    repo.copy_from_workspace();

    // Set calculation_epoch = 2 in published.toml while compiled CALC_EPOCH in fns.rs is 1.
    let pub_toml = repo
        .read("compat/published.toml")
        .replace("calculation_epoch = 1", "calculation_epoch = 2");
    repo.write("compat/published.toml", &pub_toml);
    let epoch_file = repo
        .read("crates/fieldbook-calc/tests/cases/EPOCH")
        .replacen("1 ", "2 ", 1);
    repo.write("crates/fieldbook-calc/tests/cases/EPOCH", &epoch_file);

    let out = repo.run_compat("release-baseline.toml");
    assert!(
        !out.status.success(),
        "mismatch between published.calculation_epoch and compiled CALC_EPOCH must refuse"
    );
}

#[test]
fn epoch_increase_preserves_historical_vectors() {
    let repo = TempRepo::new("epoch-preserve");
    repo.copy_from_workspace();

    // Establish baseline v0.1.0 at calculation_epoch = 2 so archived replay vectors carry epoch = 2
    // (distinct from both compiled default 1 and post-bump 3).
    let fns_v1 = repo.read("crates/fieldbook-calc/src/fns.rs").replace(
        "pub const CALC_EPOCH: u32 = 1;",
        "pub const CALC_EPOCH: u32 = 2;",
    );
    repo.write("crates/fieldbook-calc/src/fns.rs", &fns_v1);
    let sha_v1 = repo.compute_case_set_sha();
    repo.write(
        "crates/fieldbook-calc/tests/cases/EPOCH",
        &format!("2 {}\n", &sha_v1[..16]),
    );
    for rel in [
        "compat/calculation/build_up@1/calc-01.in.toml",
        "compat/calculation/build_up@1/calc-90.in.toml",
        "compat/calculation/children_sum@1/calc-04.in.toml",
        "compat/calculation/children_sum@1/calc-91.in.toml",
    ] {
        let updated = repo
            .read(rel)
            .replace("calculation_epoch = 1", "calculation_epoch = 2");
        repo.write(rel, &updated);
    }
    for rel in [
        "compat/calculation/build_up@1/calc-01.out.txt",
        "compat/calculation/children_sum@1/calc-04.out.txt",
    ] {
        let updated = repo.read(rel).replace("epoch 1\n", "epoch 2\n");
        repo.write(rel, &updated);
    }
    let pub_v1 = repo
        .read("compat/published.toml")
        .replace("calculation_epoch = 1", "calculation_epoch = 2");
    repo.write("compat/published.toml", &pub_v1);
    repo.refresh_published_hashes();

    repo.init_git();
    repo.commit_all("v0.1.0 at epoch 2");
    let commit = repo.tag("v0.1.0");
    let files_v1 = repo.compatibility_files_from_published();
    repo.write_release_baseline(
        Some(("v0.1.0", &commit, &files_v1)),
        Some(("v0.1.0", &commit)),
        None,
    );

    // Bump epoch to 3 after a case-set change while keeping the archived epoch-2 vectors untouched.
    let calc_02 = repo.read("crates/fieldbook-calc/tests/cases/calc-02.toml");
    repo.write(
        "crates/fieldbook-calc/tests/cases/calc-02.toml",
        &format!("{calc_02}# epoch 3 case note\n"),
    );
    let sha_v2 = repo.compute_case_set_sha();
    let fns_v2 = repo.read("crates/fieldbook-calc/src/fns.rs").replace(
        "pub const CALC_EPOCH: u32 = 2;",
        "pub const CALC_EPOCH: u32 = 3;",
    );
    repo.write("crates/fieldbook-calc/src/fns.rs", &fns_v2);
    repo.write(
        "crates/fieldbook-calc/tests/cases/EPOCH",
        &format!("3 {}\n", &sha_v2[..16]),
    );
    let pub_v2 = repo
        .read("compat/published.toml")
        .replace("calculation_epoch = 2", "calculation_epoch = 3")
        .replace(&sha_v1, &sha_v2);
    repo.write("compat/published.toml", &pub_v2);

    let out = repo.run_compat("release-baseline.toml");
    assert!(
        out.status.success(),
        "historical vectors with pinned epoch 2 must still pass at epoch 3: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn published_adapters_and_templates_replay() {
    let root = repo_root();
    let pub_text = std::fs::read_to_string(root.join("compat/published.toml"))
        .unwrap_or_else(|e| panic!("read published.toml: {e}"));
    let parsed: toml::Value =
        toml::from_str(&pub_text).unwrap_or_else(|e| panic!("parse published.toml: {e}"));
    let bindings = parsed
        .get("binding")
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("bindings array"));

    let required = [
        ("adapter", Some("extractor"), "plain@1"),
        ("adapter", Some("extractor"), "html-strip@1"),
        ("adapter", Some("extractor"), "pdftotext-layout@1"),
        ("adapter", Some("backup"), "folder@1"),
        ("adapter", Some("store"), "folder@1"),
        ("adapter", Some("authz"), "policy-readers@1"),
        ("template", None, "ask@1"),
    ];

    for (kind, port, id) in required {
        let present = bindings.iter().any(|b| {
            b.get("kind").and_then(toml::Value::as_str) == Some(kind)
                && b.get("id").and_then(toml::Value::as_str) == Some(id)
                && b.get("port").and_then(toml::Value::as_str) == port
        });
        assert!(
            present,
            "manifest must contain binding for {kind} {port:?} {id}"
        );
    }

    let status = Command::new(xtask_bin())
        .args(["compat", "--base", "release-baseline.toml"])
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| panic!("run xtask compat: {e}"));
    assert!(
        status.status.success(),
        "compat check on real repo must succeed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&status.stdout),
        String::from_utf8_lossy(&status.stderr)
    );
}

#[test]
fn missing_adapter_or_template_runner_refuses() {
    let repo = TempRepo::new("missing-runner-refuses");
    repo.copy_from_workspace();
    repo.use_real_manifest();
    repo.init_git();
    repo.commit_all("baseline v0.1.0");

    let real_pub = repo.read("compat/published.toml");
    let first = repo.run_compat("release-baseline.toml");
    assert!(
        first.status.success(),
        "the real manifest replays in the fixture: {}",
        String::from_utf8_lossy(&first.stderr)
    );

    // 1. Adapter binding of an unknown id refuses with an error naming the binding.
    repo.write(
        "compat/adapter/extractor/unknown@1/contract.toml",
        "kind = \"adapter\"\nport = \"extractor\"\nid = \"unknown@1\"\n",
    );
    repo.write("compat/adapter/extractor/unknown@1/impl.rs", "// impl\n");
    repo.write(
        "compat/adapter/extractor/unknown@1/c1.in.toml",
        "hex = \"\"\n",
    );
    repo.write("compat/adapter/extractor/unknown@1/c1.out.txt", "ok\n");
    let unknown_adapter = repo.format_binding(&BindingSpec {
        kind: "adapter",
        port: Some("extractor"),
        id: "unknown@1",
        runner: "adapter",
        dir: "compat/adapter/extractor/unknown@1",
        impl_path: "compat/adapter/extractor/unknown@1/impl.rs",
        cases: &["c1"],
    });
    repo.write(
        "compat/published.toml",
        &insert_binding_sorted(&real_pub, &unknown_adapter),
    );
    let res = repo.run_compat("release-baseline.toml");
    assert!(!res.status.success());
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("unknown adapter binding `adapter:extractor:unknown@1` in runner dispatch"),
        "error must be the runner refusal naming the unknown adapter binding: {stderr}"
    );

    // 2. Template binding of an unknown id without `synthetic` refuses and names the binding.
    repo.write(
        "compat/template/unknown@1/contract.toml",
        "kind = \"template\"\nid = \"unknown@1\"\n",
    );
    repo.write("compat/template/unknown@1/impl.txt", "template\n");
    repo.write("compat/template/unknown@1/c1.in.toml", "x = 1\n");
    repo.write("compat/template/unknown@1/c1.out.txt", "out\n");
    let unknown_template = repo.format_binding(&BindingSpec {
        kind: "template",
        port: None,
        id: "unknown@1",
        runner: "template",
        dir: "compat/template/unknown@1",
        impl_path: "compat/template/unknown@1/impl.txt",
        cases: &["c1"],
    });
    repo.write(
        "compat/published.toml",
        &insert_binding_sorted(&real_pub, &unknown_template),
    );
    let res = repo.run_compat("release-baseline.toml");
    assert!(!res.status.success());
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("unknown template binding `template:unknown@1` in runner dispatch"),
        "error must be the runner refusal naming the unknown template binding: {stderr}"
    );

    // 3. Registry coverage fails when a Built row has no binding in the manifest.
    let without_plain: String = {
        let mut parts = real_pub.split("[[binding]]\n");
        let mut out = parts.next().unwrap_or_default().to_owned();
        for block in parts {
            if block.contains("port = \"extractor\"\nid = \"plain@1\"") {
                continue;
            }
            out.push_str("[[binding]]\n");
            out.push_str(block);
        }
        out
    };
    assert_ne!(without_plain, real_pub, "the plain@1 binding was removed");
    repo.write("compat/published.toml", &without_plain);
    let res = repo.run_compat("release-baseline.toml");
    assert!(!res.status.success());
    let stderr = String::from_utf8_lossy(&res.stderr);
    assert!(
        stderr.contains("ADAPTERS adapter `(extractor, plain@1)`"),
        "coverage check must name the Built row without a binding: {stderr}"
    );
}
