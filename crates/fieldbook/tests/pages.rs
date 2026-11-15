//! The desk pages `fieldbook status --page` writes (`AD-17`, `status-command`).

#![expect(
    clippy::expect_used,
    reason = "test helpers build temporary fixtures and inspect their results"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "desk page tests prepare temporary fixture copies and corrupt a snapshot on purpose"
)]

#[path = "../../../tests/support/chip_cases.rs"]
#[expect(
    dead_code,
    reason = "the pages test uses only the chip substitution of the shared cases"
)]
mod chip_cases;

mod support;

use std::path::{Path, PathBuf};

use clap::Parser;
use fieldbook::{Cli, World};
use fieldbook_core::labels::LabelSet;
use fieldbook_core::policy::EffectivePolicy;
use fieldbook_render::csp::CSP_STATIC;

use chip_cases::substitute_chip;
use support::temp_copy;
use support::world::TestWorld;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The page names `PAGES` holds, in order.
const PAGE_FILES: [&str; 3] = ["import.html", "activity.html", "backup.html"];

struct Desk {
    root: tempfile::TempDir,
    target: tempfile::TempDir,
    state: PathBuf,
    world: TestWorld,
}

impl Desk {
    fn page(&self, name: &str) -> String {
        std::fs::read_to_string(self.state.join("pages").join(name))
            .unwrap_or_else(|err| panic!("read {name}: {err}"))
    }
}

fn write_settings(state: &Path, target: &Path, warn_after_days: u32) {
    std::fs::create_dir_all(state).expect("create state folder");
    let text = format!(
        "role = \"lead\"\n\n[backup]\ndefault = \"main\"\nwarn_after_days = {warn_after_days}\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state.join("settings.toml"), text).expect("write settings.toml");
}

/// The saved Import report the pages show, with one added file whose name needs escaping and
/// one refused file.
fn add_report(labels: &str) -> String {
    format!(
        concat!(
            "{{",
            "\"report_version\":1,\"verb\":\"add\",\"dir\":\"incoming\",",
            "\"labels\":{labels},",
            "\"inputs\":[{{\"store\":\"corpus\",\"id\":\"S-0009-v1\"}}],",
            "\"files\":[",
            "{{\"name\":\"Synthetic <script>alert(1)</script> long incoming file name for the desk page canary.txt\",",
            "\"result\":\"added\",\"id\":\"S-0009-v1\",",
            "\"sha256\":\"{added_sha}\",\"bytes\":42,\"needs_extraction\":false}},",
            "{{\"name\":\"refused.txt\",\"result\":\"refused\",",
            "\"sha256\":\"{refused_sha}\",\"bytes\":7,\"needs_extraction\":false,\"code\":\"FB0410\"}}",
            "],",
            "\"counts\":{{\"added\":1,\"present\":0,\"refused\":1,\"needs_extraction\":0}},",
            "\"exit\":1",
            "}}"
        ),
        labels = labels,
        added_sha = "1".repeat(64),
        refused_sha = "2".repeat(64),
    )
}

fn write_add_report(state: &Path, labels: &str) {
    let reports = state.join("reports");
    std::fs::create_dir_all(&reports).expect("create reports folder");
    std::fs::write(reports.join("last-add.json"), add_report(labels)).expect("write last-add.json");
}

/// Builds a fixture state with a journal and an Import report; `backup` adds one verified backup.
fn desk_fixture(backup: bool) -> Desk {
    let root = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let target = tempfile::tempdir().expect("create target folder");
    let state = root.path().join("state");
    std::fs::create_dir_all(&state).expect("create state folder");
    support::state::build_state(&state, &support::state::state_mini_events());
    write_settings(&state, target.path(), 7);

    let world = TestWorld::with_state(root.path(), &state);
    let tree = world.read_tree().expect("read fixture tree");
    let catalogue = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    )
    .expect("the fixture has a public support catalogue");
    let (_, sha) = catalogue.to_toml_bytes_and_sha256();
    world.set_support_catalogue(catalogue, sha);

    if backup {
        let made = fieldbook::run(&["backup".to_owned()], &world);
        assert_eq!(made.code, 0, "backup: {}", made.stderr);
        let verified = fieldbook::run(&["backup".to_owned(), "verify".to_owned()], &world);
        assert_eq!(verified.code, 0, "backup verify: {}", verified.stderr);
    }
    let sources = root.path().join("corpus/registers/sources.csv");
    let text = std::fs::read_to_string(&sources).expect("read sources");
    let row = text
        .lines()
        .find(|line| line.starts_with("S-0001,"))
        .expect("source row");
    std::fs::write(
        &sources,
        format!("{text}{}\n", row.replacen("S-0001,", "S-0009,", 1)),
    )
    .expect("add source");
    let originals = root.path().join("corpus/sources/public");
    std::fs::copy(
        originals.join("S-0001-v1.txt"),
        originals.join("S-0009-v1.txt"),
    )
    .expect("copy synthetic original");
    write_add_report(&state, "[\"public\"]");

    Desk {
        root,
        target,
        state,
        world,
    }
}

fn write_pages(world: &TestWorld) -> fieldbook::Outcome {
    fieldbook::run(&["status".to_owned(), "--page".to_owned()], world)
}

#[test]
fn pages_are_static_and_have_the_csp() {
    let desk = desk_fixture(false);
    let outcome = write_pages(&desk.world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);

    assert!(
        desk.state.join("pages/assets/app.css").is_file(),
        "pages/assets/app.css must be written"
    );
    assert_eq!(
        std::fs::read_to_string(desk.state.join("pages/assets/app.js"))
            .expect("pages/assets/app.js must be written"),
        fieldbook_render::site::APP_JS
    );

    for name in PAGE_FILES {
        let page = desk.page(name);
        assert!(
            page.contains(&format!(
                "Content-Security-Policy\" content=\"{CSP_STATIC}\""
            )),
            "{name} carries CSP_STATIC"
        );
        assert!(!page.contains("style=\""), "{name} has no style attribute");
        assert!(
            page.contains("<script src=\"assets/app.js\" defer></script>"),
            "{name} loads external assets/app.js for copy chips"
        );
        let without_app_js = page.replace("<script src=\"assets/app.js\" defer></script>", "");
        assert!(
            !without_app_js.contains("<script"),
            "{name} has no inline script"
        );
        assert!(
            page.contains("<div class=\"toast\" id=\"fb:toast\" role=\"status\" aria-live=\"polite\" hidden></div>"),
            "{name} includes the copy toast live region"
        );
        assert!(
            page.contains("<div class=\"sr-only\" id=\"fb:sr\" aria-live=\"polite\"></div>"),
            "{name} includes the screen-reader live region"
        );
        assert!(!page.contains("onclick"), "{name} has no event handler");
        assert!(
            page.contains("Written 2026-03-01T12:00:00Z"),
            "{name} names its time"
        );
        assert!(
            page.contains(chip("Write the desk pages")),
            "{name} shows the refresh chip"
        );
        // A table, when the page has one, has a caption and scoped headers.
        if page.contains("<table") {
            assert!(page.contains("<caption>"), "{name} table has a caption");
            assert!(
                page.contains("scope=\"col\""),
                "{name} table has scoped headers"
            );
        }
    }

    let import = desk.page("import.html");
    assert!(
        import.contains("&lt;script&gt;alert(1)&lt;/script&gt;"),
        "the file name is escaped: {import}"
    );
    assert!(!import.contains("<script>alert(1)</script>"));
}

#[test]
fn desk_commands_match_the_parser() {
    let desk = desk_fixture(false);
    assert_eq!(write_pages(&desk.world).code, 0);
    let mut seen = 0usize;
    for name in PAGE_FILES {
        let page = desk.page(name);
        for piece in page.split("<code>").skip(1) {
            let command = piece.split("</code>").next().unwrap_or_default();
            if !command.starts_with("fieldbook ") {
                continue;
            }
            seen += 1;
            let argv = substitute_chip(&unescape(command), &[]);
            assert!(
                Cli::try_parse_from(&argv).is_ok(),
                "the page command `{command}` does not parse"
            );
        }
    }
    assert!(seen >= 3, "the pages show commands: {seen}");
}

fn chip(intent: &str) -> &'static str {
    fieldbook_core::strings::chips()
        .iter()
        .find(|row| row.intent == intent)
        .map_or("", |row| row.chip)
}

fn unescape(text: &str) -> String {
    text.replace("&#60;", "<")
        .replace("&#62;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

#[test]
fn pages_write_nothing_outside_pages() {
    let desk = desk_fixture(false);
    let before = desk.world.written().len();
    assert_eq!(write_pages(&desk.world).code, 0);
    let written = desk.world.written();
    assert!(written.len() > before);
    for path in written.into_iter().skip(before) {
        let allowed = path.starts_with(desk.state.join("pages"))
            || path.starts_with(desk.state.join("assets"));
        assert!(
            allowed,
            "a desk write stayed outside pages/ and assets/: {path:?}"
        );
    }
}

#[test]
fn status_without_page_writes_nothing() {
    let desk = desk_fixture(false);
    let before = desk.world.written().len();
    let outcome = fieldbook::run(&["status".to_owned()], &desk.world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    assert_eq!(desk.world.written().len(), before);
    assert!(!desk.state.join("pages").exists());
}

#[test]
fn missing_sources_give_empty_pages() {
    let root = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let state = root.path().join("state");
    std::fs::create_dir_all(&state).expect("create state folder");
    std::fs::write(state.join("settings.toml"), "role = \"lead\"\n").expect("write settings");
    let world = TestWorld::with_state(root.path(), &state);

    let outcome = write_pages(&world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    let import = std::fs::read_to_string(state.join("pages/import.html")).expect("import page");
    let activity =
        std::fs::read_to_string(state.join("pages/activity.html")).expect("activity page");
    let backup = std::fs::read_to_string(state.join("pages/backup.html")).expect("backup page");
    assert!(import.contains("No add has run"));
    assert!(activity.contains("None recorded"));
    assert!(backup.contains("No backups recorded"));
}

#[test]
fn desk_sources_keep_missing_unavailable_and_failed_distinct() {
    // 1. A source that does not exist has its empty line.
    let root = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    let state = root.path().join("state");
    std::fs::create_dir_all(&state).expect("create state folder");
    std::fs::write(state.join("settings.toml"), "role = \"lead\"\n").expect("write settings");
    let missing_world = TestWorld::with_state(root.path(), &state);
    assert_eq!(write_pages(&missing_world).code, 0);
    let missing = std::fs::read_to_string(state.join("pages/import.html")).expect("import page");
    assert!(missing.contains("No add has run"));
    assert!(!missing.contains("Import report unavailable"));

    // 2. An unreadable report renders its diagnostic and exits 3.
    let reports = state.join("reports");
    std::fs::create_dir_all(&reports).expect("create reports folder");
    std::fs::write(reports.join("last-add.json"), "{ not json").expect("write bad report");
    let unavailable_world = TestWorld::with_state(root.path(), &state);
    let outcome = write_pages(&unavailable_world);
    assert_eq!(outcome.code, 3, "an unreadable source exits 3");
    let unavailable = std::fs::read_to_string(state.join("pages/import.html"))
        .expect("the diagnostic page was written");
    assert!(unavailable.contains("Import report unavailable"));
    assert!(!unavailable.contains("added ·"), "no counts are inferred");

    // 3. A report without labels is unavailable, never a public report.
    std::fs::write(reports.join("last-add.json"), add_report("[]"))
        .expect("write unlabelled report");
    let unlabelled = write_pages(&TestWorld::with_state(root.path(), &state));
    assert_eq!(unlabelled.code, 3);
    let page = std::fs::read_to_string(state.join("pages/import.html")).expect("import page");
    assert!(page.contains("Import report unavailable"));
    assert!(
        page.contains("empty `labels`"),
        "the diagnostic is shown: {page}"
    );

    // 4. A failed verification keeps readable events, labelled unverified.
    let broken_state = root.path().join("broken-state");
    std::fs::create_dir_all(&broken_state).expect("create broken state folder");
    support::state::plant(&broken_state, support::state::Plant::UnclosedBegin);
    std::fs::write(broken_state.join("settings.toml"), "role = \"lead\"\n")
        .expect("write settings");
    let broken_world = TestWorld::with_state(root.path(), &broken_state);
    let outcome = write_pages(&broken_world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    let activity =
        std::fs::read_to_string(broken_state.join("pages/activity.html")).expect("activity page");
    assert!(
        activity.contains("unfinished"),
        "the state is named: {activity}"
    );
    assert!(activity.contains("remain unverified"));
    assert!(
        activity.contains("Showing"),
        "readable events remain visible"
    );
    assert!(
        activity.contains(chip("Verify the audit journal")),
        "the page offers the check, never a repair"
    );
}

#[test]
fn backup_page_shows_failed_reverification() {
    let desk = desk_fixture(true);
    // Corrupt the snapshot's copied data without touching its manifest identity.
    let snapshot = desk.target.path().join("fieldbook-backup-0001");
    let corrupted = corrupt_one_copied_file(&snapshot);
    assert!(corrupted, "the snapshot holds a copied file to corrupt");

    let failed = fieldbook::run(&["backup".to_owned(), "verify".to_owned()], &desk.world);
    assert_eq!(failed.code, 1, "the verification fails: {}", failed.stderr);

    let outcome = write_pages(&desk.world);
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);
    let page = desk.page("backup.html");
    assert!(
        page.contains("<span class=\"chip bad\">failed</span>"),
        "the latest failed verification is shown: {page}"
    );
    assert!(
        page.contains("Last successful verification:"),
        "the historical success is labelled separately: {page}"
    );
    assert!(
        page.contains("Latest recorded verification:"),
        "the latest outcome comes first: {page}"
    );
    assert!(
        !page.contains("Last successful verification: none yet"),
        "a past success is not reported as none: {page}"
    );
}

fn corrupt_one_copied_file(snapshot: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(snapshot) else {
        return false;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if corrupt_one_copied_file(&path) {
                return true;
            }
        } else if path.extension().is_some_and(|ext| ext == "txt") {
            let mut bytes = std::fs::read(&path).unwrap_or_default();
            bytes.push(b'!');
            std::fs::write(&path, bytes).expect("corrupt a copied file");
            return true;
        }
    }
    false
}

#[test]
fn desk_admission_preserves_existing_page() {
    let desk = desk_fixture(false);
    assert_eq!(write_pages(&desk.world).code, 0);
    let before = desk.page("import.html");

    // A report that carries a label the State store does not hold on the home floor.
    write_add_report(&desk.state, "[\"public\",\"np-0007\"]");
    let refused = write_pages(&desk.world);
    assert_eq!(refused.code, 3, "FB0412 exits 3");
    assert!(
        refused.stderr.contains("FB0412"),
        "the refusal names its code: {}",
        refused.stderr
    );
    assert_eq!(
        desk.page("import.html"),
        before,
        "the previous page is preserved"
    );
}

/// The store rule the desk writer admits against: State on the home floor.
#[test]
fn desk_admission_uses_the_state_rule() {
    let policy = EffectivePolicy::floor(false);
    let rule = policy.store_rule(&fieldbook_core::labels::StoreId::State);
    assert!(fieldbook_core::labels::admit(&LabelSet::owner_supplied(), &rule).is_ok());
    assert!(fieldbook_core::labels::admit(&LabelSet::public(), &rule).is_ok());
}

#[test]
fn edition_never_holds_pages() {
    let desk = desk_fixture(false);
    assert_eq!(write_pages(&desk.world).code, 0);
    assert!(desk.state.join("pages/import.html").is_file());

    let out = desk.root.path().join("out");
    let outcome = fieldbook::run(
        &[
            "build".to_owned(),
            "--out".to_owned(),
            out.to_string_lossy().to_string(),
        ],
        &desk.world,
    );
    assert_eq!(outcome.code, 0, "stderr: {}", outcome.stderr);

    let mut found = Vec::new();
    collect_pages(&out, &mut found);
    assert!(found.is_empty(), "an edition holds no desk page: {found:?}");
}

fn collect_pages(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name.to_string_lossy() == "pages")
            {
                found.push(path);
            } else {
                collect_pages(&path, found);
            }
        }
    }
}

#[test]
fn the_desk_pages_match_their_goldens() {
    let desk = desk_fixture(false);
    assert_eq!(write_pages(&desk.world).code, 0);
    assert_eq!(
        desk.page("import.html"),
        include_str!("../../../tests/golden/pages/import.html")
    );
    assert_eq!(
        desk.page("activity.html"),
        include_str!("../../../tests/golden/pages/activity.html")
    );

    // The Backup status golden shows one recorded, verified snapshot.
    let backed = desk_fixture(true);
    assert_eq!(write_pages(&backed.world).code, 0);
    assert_eq!(
        backed.page("backup.html"),
        include_str!("../../../tests/golden/pages/backup.html")
    );
}

#[test]
fn missing_import_provenance_never_renders_content() {
    let desk = desk_fixture(false);
    let path = desk.state.join("reports/last-add.json");
    let report = std::fs::read_to_string(&path).expect("read report");
    std::fs::write(&path, report.replace("S-0009-v1", "S-9999-v1")).expect("write report");
    let result = write_pages(&desk.world);
    assert_eq!(result.code, 3);
    let page = desk.page("import.html");
    assert!(page.contains("unavailable"));
    assert!(!page.contains("long incoming file name"));
}

#[test]
fn unreadable_backup_settings_are_not_reported_as_empty() {
    let desk = desk_fixture(false);
    std::fs::write(
        desk.state.join("settings.toml"),
        "role = 'lead'\n[backup]\ndefault = 123\n",
    )
    .expect("write settings");
    let result = write_pages(&desk.world);
    assert_eq!(result.code, 3);
    let page = desk.page("backup.html");
    assert!(page.contains("Backup status unavailable"));
    assert!(!page.contains("No backups recorded"));
}

#[test]
fn reclassified_import_source_refuses_before_replacing_page() {
    let desk = desk_fixture(false);
    assert_eq!(write_pages(&desk.world).code, 0);
    let before = desk.page("import.html");

    // Reclassify S-0009 between `status`'s initial unlocked read and `write_desk_pages`'s
    // exclusive store lease while `last-add.json` still carries only `["public"]`.
    let sources_path = desk.root.path().join("corpus/registers/sources.csv");
    desk.world.set_before_write_lease(move || {
        let text = std::fs::read_to_string(&sources_path).expect("read sources");
        let updated = text
            .lines()
            .map(|line| {
                if line.starts_with("S-0009,") {
                    line.replacen(",public,", ",public|np-0007,", 1)
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&sources_path, format!("{updated}\n")).expect("relabel S-0009");
    });

    let refused = write_pages(&desk.world);
    assert_eq!(refused.code, 3, "stderr: {}", refused.stderr);
    assert!(
        refused.stderr.contains("FB0412"),
        "expected FB0412 refusal, got: {}",
        refused.stderr
    );
    assert_eq!(
        desk.page("import.html"),
        before,
        "existing import.html must remain untouched"
    );
}

#[test]
fn import_ledger_reclassification_and_broken_ledger_are_enforced() {
    use fieldbook_core::policy::{OwnerSettings, Policy, PolicyState};

    let desk = desk_fixture(false);
    // Loaded policy where State holds `bid-0003` (and NOT `np-0007` or `unlabelled`).
    let fixture_toml = include_str!("../../../tests/fixtures/policy/policy.toml");
    let state_bid_only = fixture_toml.replace(
        "holds = [\"public\", \"unlabelled\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]",
        "holds = [\"public\", \"unlabelled\", \"owner-supplied\", \"bid-0003\"]",
    );
    let policy = Policy::parse(&state_bid_only, None).expect("parse policy");
    desk.world.set_effective_policy(EffectivePolicy::new(
        &PolicyState::Loaded(policy),
        &OwnerSettings::unset(),
    ));

    let overlay_dir = desk.root.path().join("overlay");
    std::fs::create_dir_all(&overlay_dir).expect("create overlay dir");
    std::fs::write(
        overlay_dir.join("labels.csv"),
        "seq,item_id,kind,labels,origin\n1,S-0100-v1,add,bid-0003,intake\n",
    )
    .expect("write initial overlay ledger");

    let report_path = desk.state.join("reports/last-add.json");
    let overlay_report = add_report("[\"bid-0003\"]")
        .replace("\"store\":\"corpus\"", "\"store\":\"overlay\"")
        .replace("S-0009-v1", "S-0100-v1")
        .replace(
            "long incoming file name for the desk page canary.txt",
            "permitted-ledger-canary.txt",
        );
    std::fs::write(&report_path, &overlay_report).expect("write overlay import report");

    let initial = write_pages(&desk.world);
    assert_eq!(initial.code, 0, "stderr: {}", initial.stderr);
    let before = desk.page("import.html");
    assert!(before.contains("permitted-ledger-canary.txt"));

    // Append `np-0007` to `overlay/labels.csv` without modifying `last-add.json` labels.
    std::fs::write(
        overlay_dir.join("labels.csv"),
        "seq,item_id,kind,labels,origin\n1,S-0100-v1,add,bid-0003,intake\n2,S-0100-v1,add,np-0007,person\n",
    )
    .expect("append np-0007 ledger entry");
    std::fs::write(
        &report_path,
        overlay_report.replace("permitted-ledger-canary.txt", "forbidden-ledger-canary.txt"),
    )
    .expect("update report file name");

    let refused = write_pages(&desk.world);
    assert_eq!(refused.code, 3, "stderr: {}", refused.stderr);
    assert!(
        refused.stderr.contains("FB0412"),
        "ledger reclassification must refuse with FB0412, got: {}",
        refused.stderr
    );
    assert_eq!(desk.page("import.html"), before);

    // Restore a clean `overlay/labels.csv`, then corrupt `state/labels.csv`: must fail closed too.
    std::fs::write(
        overlay_dir.join("labels.csv"),
        "seq,item_id,kind,labels,origin\n1,S-0100-v1,add,bid-0003,intake\n",
    )
    .expect("restore clean overlay ledger");
    std::fs::write(desk.state.join("labels.csv"), "bad header\n").expect("write corrupt ledger");

    let broken = write_pages(&desk.world);
    assert_eq!(broken.code, 3, "stderr: {}", broken.stderr);
    assert!(
        broken.stderr.contains("FB0412"),
        "corrupt state/labels.csv must refuse with FB0412, got: {}",
        broken.stderr
    );
    assert_eq!(desk.page("import.html"), before);
}

#[test]
fn loaded_policy_governs_desk_admission() {
    use fieldbook_core::policy::{OwnerSettings, Policy, PolicyState};

    let desk = desk_fixture(false);
    write_add_report(&desk.state, "[\"public\",\"np-0007\"]");

    // Default floor on Home layout does not hold `np-0007` in State.
    let floor_refused = write_pages(&desk.world);
    assert_eq!(floor_refused.code, 3, "stderr: {}", floor_refused.stderr);
    assert!(floor_refused.stderr.contains("FB0412"));

    // Loaded policy whose State store holds `np-0007` admits the desk write.
    let fixture_toml = include_str!("../../../tests/fixtures/policy/policy.toml");
    let allow_policy = Policy::parse(fixture_toml, None).expect("parse fixture policy");
    desk.world.set_effective_policy(EffectivePolicy::new(
        &PolicyState::Loaded(allow_policy),
        &OwnerSettings::unset(),
    ));
    let allowed = write_pages(&desk.world);
    assert_eq!(allowed.code, 0, "stderr: {}", allowed.stderr);
    let before = desk.page("import.html");

    // Narrowing State's `holds` in a loaded policy to omit `np-0007` refuses and preserves pages.
    let deny_toml = fixture_toml.replace(
        "holds = [\"public\", \"unlabelled\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]",
        "holds = [\"public\", \"unlabelled\", \"owner-supplied\", \"bid-0003\"]",
    );
    let deny_policy = Policy::parse(&deny_toml, None).expect("parse narrowed policy");
    desk.world.set_effective_policy(EffectivePolicy::new(
        &PolicyState::Loaded(deny_policy),
        &OwnerSettings::unset(),
    ));
    let denied = write_pages(&desk.world);
    assert_eq!(denied.code, 3, "stderr: {}", denied.stderr);
    assert!(denied.stderr.contains("FB0412"));
    assert_eq!(desk.page("import.html"), before);
}

#[test]
fn desk_git_guard_enforces_public_home_owner_supplied_and_refuses_unlabelled_or_marked() {
    // 1. Public delivery under `.git` succeeds even outside ignored `.fieldbook/`.
    let root = temp_copy(&repo_root().join("tests/fixtures/corpus-mini"));
    std::fs::create_dir_all(root.path().join(".git")).expect("create .git dir");
    let tracked_state = root.path().join("state");
    std::fs::create_dir_all(&tracked_state).expect("create tracked state dir");
    support::state::build_state(&tracked_state, &support::state::state_mini_events());
    std::fs::write(tracked_state.join("settings.toml"), "role = \"lead\"\n")
        .expect("write role-only settings");
    let sources = root.path().join("corpus/registers/sources.csv");
    let src_text = std::fs::read_to_string(&sources).expect("read sources");
    let row = src_text
        .lines()
        .find(|line| line.starts_with("S-0001,"))
        .expect("S-0001 row");
    std::fs::write(
        &sources,
        format!("{src_text}{}\n", row.replacen("S-0001,", "S-0009,", 1)),
    )
    .expect("append S-0009");
    let originals = root.path().join("corpus/sources/public");
    std::fs::copy(
        originals.join("S-0001-v1.txt"),
        originals.join("S-0009-v1.txt"),
    )
    .expect("copy S-0009 original");
    write_add_report(&tracked_state, "[\"public\"]");

    let tracked_world = TestWorld::with_state(root.path(), &tracked_state);
    let pub_ok = write_pages(&tracked_world);
    assert_eq!(
        pub_ok.code, 0,
        "public under .git must pass: {}",
        pub_ok.stderr
    );

    // `owner-supplied` outside ignored `.fieldbook/` under `.git` is refused with FB0415.
    let before_tracked = std::fs::read_to_string(tracked_state.join("pages/import.html"))
        .expect("read public import.html");
    write_add_report(&tracked_state, "[\"owner-supplied\"]");
    let owner_refused = write_pages(&tracked_world);
    assert_eq!(owner_refused.code, 3, "stderr: {}", owner_refused.stderr);
    assert!(
        owner_refused.stderr.contains("FB0415"),
        "expected FB0415, got: {}",
        owner_refused.stderr
    );
    assert_eq!(
        std::fs::read_to_string(tracked_state.join("pages/import.html")).expect("preserved"),
        before_tracked
    );

    // 2. `owner-supplied` inside ignored Home `.fieldbook/` under `.git` succeeds.
    std::fs::write(root.path().join(".gitignore"), "/.fieldbook/\n").expect("write .gitignore");
    let home_state = root.path().join(".fieldbook");
    write_settings(&home_state, root.path().join("backups").as_path(), 7);
    write_add_report(&home_state, "[\"owner-supplied\"]");
    let home_world = TestWorld::new(root.path());
    let home_ok = write_pages(&home_world);
    assert_eq!(
        home_ok.code, 0,
        "owner-supplied in ignored .fieldbook/ must pass: {}",
        home_ok.stderr
    );
    let before_home =
        std::fs::read_to_string(home_state.join("pages/import.html")).expect("read home import");

    // 3a. Forbidden `unlabelled` input under `.git` is refused with FB0415 and preserves pages.
    let updated_sources = std::fs::read_to_string(&sources)
        .expect("read sources")
        .lines()
        .map(|line| {
            if line.starts_with("S-0009,") {
                line.replacen(",public,", ",unlabelled,", 1)
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&sources, format!("{updated_sources}\n")).expect("relabel S-0009 unlabelled");
    let unlabelled_refused = write_pages(&home_world);
    assert_eq!(
        unlabelled_refused.code, 3,
        "stderr: {}",
        unlabelled_refused.stderr
    );
    assert!(
        unlabelled_refused.stderr.contains("FB0415"),
        "expected FB0415 for unlabelled under .git, got: {}",
        unlabelled_refused.stderr
    );
    assert_eq!(
        std::fs::read_to_string(home_state.join("pages/import.html")).expect("preserved"),
        before_home
    );

    // 3b. Forbidden `marked` report on Floor B (`State` admits `marked`, `git_guard` refuses)
    //     exits 3 with FB0415 and preserves existing pages.
    let restored_sources = updated_sources.replacen(",unlabelled,", ",public,", 1);
    std::fs::write(&sources, format!("{restored_sources}\n")).expect("restore S-0009 public");
    write_add_report(&home_state, "[\"marked\"]");
    home_world.set_effective_policy(EffectivePolicy::floor(true));
    let marked_refused = write_pages(&home_world);
    assert_eq!(marked_refused.code, 3, "stderr: {}", marked_refused.stderr);
    assert!(
        marked_refused.stderr.contains("FB0415"),
        "expected FB0415 for marked under .git, got: {}",
        marked_refused.stderr
    );
    assert_eq!(
        std::fs::read_to_string(home_state.join("pages/import.html")).expect("preserved"),
        before_home
    );
}
