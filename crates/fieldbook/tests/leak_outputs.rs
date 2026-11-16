//! Binary integration tests for real outward surfaces and journal canary absence
//! (`core-policy` steps 17 and 20).

#![expect(
    clippy::disallowed_methods,
    reason = "integration tests write and inspect temporary files and real journal segments"
)]

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use fieldbook::build::{plan_edition, write_edition_outputs};
use fieldbook::io::state;
use fieldbook::{Error, World};
use fieldbook_core::CivilDate;
use fieldbook_core::audit::{
    DestKind, Event, EventPayload, Kind, LabelId, LabelSet, Now, RecordRef, Sha256, StoreId, Via,
    Word, WriteFile, WriteSet, sample_payload,
};
use fieldbook_core::authz::ReadableStores;
use fieldbook_core::check::{Host, Model, Options, Severity, Tree, load};
use fieldbook_core::egress::{Decision, Delivery, Dest, LabelledView, Shipped, Transfer, check};
use fieldbook_core::labels::{ItemRef, LabelGraph, LabelNode, Ledger, PUBLIC, effective_item};
use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};
use fieldbook_core::schema::Register;
use fieldbook_render::day_one;
use fieldbook_render::map;
use fieldbook_render::meta::{BuildInfo, BuildProfile};
use fieldbook_render::site::Site;
use serde::Deserialize;
use zip::ZipArchive;

use support::temp_copy;
use support::world::TestWorld;

use fieldbook::io::journal::RestoreExportRequest;
use fieldbook::restore::{RestoreOptions, run_restore};
use fieldbook_core::backup::{BackupToml, PlannedFile, RestorePlan, manifest_sha256};
use fieldbook_core::egress::{Operation, Target, hash_tree_manifest};

const POLICY_TOML: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
const RECORDS_TOML: &str = include_str!("../../../tests/fixtures/leak/records.toml");
const MINI_CORPUS_TOML: &str =
    include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

/// One executable outward surface row in [`SURFACES`].
pub struct Surface {
    /// Stable surface identifier matching [`BUILT_SINKS`].
    pub name: &'static str,
    /// Executable harness that invokes the real sink, checks binding/refusal, and verifies
    /// permitted and forbidden canaries on the emitted bytes.
    pub run: fn(),
}

/// Built outward sinks in `fieldbook` covered by [`SURFACES`].
const BUILT_SINKS: &[&str] = &[
    "atomic_outside_writer",
    "journal_holds_ids_and_no_canary",
    "edition",
    "page_packs",
    "backup",
    "day_one_binder",
    "registers_workbook",
    "register_csvs",
    "restore",
    "template_workbooks",
    "desk_pages",
];

/// Registered real-output surfaces (`core-policy` step 17).
pub const SURFACES: &[Surface] = &[
    Surface {
        name: "atomic_outside_writer",
        run: run_atomic_outside_writer,
    },
    Surface {
        name: "journal_holds_ids_and_no_canary",
        run: run_journal_holds_ids_and_no_canary,
    },
    Surface {
        name: "edition",
        run: run_edition_surface,
    },
    Surface {
        name: "page_packs",
        run: run_page_packs_surface,
    },
    Surface {
        name: "backup",
        run: run_backup_surface,
    },
    Surface {
        name: "day_one_binder",
        run: run_day_one_binder_surface,
    },
    Surface {
        name: "registers_workbook",
        run: run_registers_workbook_surface,
    },
    Surface {
        name: "register_csvs",
        run: run_register_csvs_surface,
    },
    Surface {
        name: "restore",
        run: run_restore_surface,
    },
    Surface {
        name: "template_workbooks",
        run: run_template_workbooks_surface,
    },
    Surface {
        name: "desk_pages",
        run: run_desk_pages_surface,
    },
];

#[derive(Debug, Deserialize)]
struct LeakFixture {
    record: Vec<PlantedRecord>,
}

#[derive(Debug, Clone, Deserialize)]
struct PlantedRecord {
    id: String,
    store: StoreId,
    label: String,
    canary: String,
}

fn load_fixture() -> LeakFixture {
    toml::from_str(RECORDS_TOML).unwrap_or_else(|e| panic!("parse records.toml: {e}"))
}

fn default_now() -> Now {
    let d = CivilDate::from_ymd(2026, 11, 1).unwrap_or_else(|e| panic!("{e}"));
    Now::new(d, 3600).unwrap_or_else(|e| panic!("{e}"))
}

fn corpus_mini() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini")
}

fn insert_register_headers(tree: &mut Tree) {
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
}

fn build_model_from_records(records: &[PlantedRecord]) -> Model {
    let mut tree = Tree::new();
    tree.insert_dir("corpus");
    tree.insert_dir("corpus/pages");
    tree.insert_dir("corpus/sources");
    tree.insert_dir("corpus/sources/public");
    tree.insert_dir("overlay");
    tree.insert_dir("overlay/records");
    tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
    tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
    insert_register_headers(&mut tree);

    for rec in records {
        let path = format!("{}/records/{}.toml", rec.store.as_str(), rec.id);
        let content = format!(
            "id = \"{}\"\nhandling = \"{}\"\ncanary = \"{}\"\n",
            rec.id, rec.label, rec.canary
        );
        tree.insert_bytes(path, content.into_bytes());
    }

    let today = CivilDate::from_ymd(2026, 3, 1).unwrap_or_else(|e| panic!("{e}"));
    load(&Host::new(tree, today), &Options::new(false)).0
}

fn run_atomic_outside_writer() {
    let fixture = load_fixture();
    let model = build_model_from_records(&fixture.record);

    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let tree = fieldbook::io::read_tree(tmp.path()).unwrap_or_else(|e| panic!("{e}"));
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let out_dir_str = out_tmp.path().to_string_lossy().replace('\\', "/");

    // Replace the export path prefix in the fixture policy with `out_tmp.path()` so the real
    // atomic outside writer writes into `out_tmp`.
    let policy_toml = POLICY_TOML.replace("/srv/company/exports", &out_dir_str);
    let policy = Policy::parse(&policy_toml, None).unwrap_or_else(|e| panic!("{e}"));
    let state = PolicyState::Loaded(policy);
    let eff = EffectivePolicy::new(&state, &OwnerSettings::unset());

    let out_file = out_tmp.path().join("export-pack.txt");
    let dest = Dest::Export {
        path: out_file.to_string_lossy().into_owned(),
    };
    let now = default_now();
    let shipped = Shipped::Hours(1);

    // 1. Select view using a selection token `(None, None)`.
    let Decision::Allowed(sel_token) = check(&eff, &dest, None, None, now, shipped) else {
        panic!("expected export selection Allowed");
    };
    let readable = ReadableStores::new(vec![
        StoreId::Corpus,
        StoreId::Overlay,
        StoreId::Working,
        StoreId::State,
    ]);
    let view = LabelledView::new(&model, &sel_token, &readable);
    let rendered = view.render_summary(&dest);

    // 2. Bind the exact rendered payload and check again.
    let bid3 = LabelId::parse("bid-0003").unwrap_or_else(|e| panic!("{e}"));
    let labels = LabelSet::from_ids([PUBLIC, bid3]);
    let delivery = Delivery {
        entries: vec![Transfer::write_path(out_file.clone(), rendered.as_bytes())],
        markings: None,
    };
    let Decision::Allowed(allowed) =
        check(&eff, &dest, Some(&labels), Some(&delivery), now, shipped)
    else {
        panic!("expected delivery-bound Allowed");
    };

    let ws = WriteSet {
        labels,
        files: vec![WriteFile::new(
            "export-pack.txt",
            Sha256::digest(rendered.as_bytes()).as_str().to_owned(),
        )],
        affected: vec![RecordRef::parse("corpus/S-0001").unwrap_or_else(|e| panic!("{e}"))],
    };
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("context", &ws, &allowed, &out_file)
        .unwrap_or_else(|e| panic!("{e}"));

    // 3. Attempting to write a forbidden canary payload (`ctl-0002` / `np-0007` / `marked`) or
    //    using the selection token at the atomic outside door MUST be refused with OutOfScope.
    let sel_attempt = world.write_outside(&sel_token, &audited, &out_file, rendered.as_bytes());
    assert!(
        matches!(sel_attempt, Err(Error::OutOfScope(_))),
        "selection token must be refused at write_outside"
    );
    for rec in &fixture.record {
        if rec.label != "public" && rec.label != "bid-0003" {
            let unapproved_bytes = format!("leaked:{}\n", rec.canary);
            let res =
                world.write_outside(&allowed, &audited, &out_file, unapproved_bytes.as_bytes());
            assert!(
                matches!(res, Err(Error::OutOfScope(_))),
                "unapproved payload for label `{}` must be refused by write_outside, got {res:?}",
                rec.label
            );
        }
    }
    assert!(
        !out_file.exists(),
        "refused writes must not create the output file"
    );

    // 4. Write the approved bytes through the real atomic outside writer and inspect disk bytes.
    world
        .write_outside(&allowed, &audited, &out_file, rendered.as_bytes())
        .unwrap_or_else(|e| panic!("{e}"));
    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));

    let disk_text = std::fs::read_to_string(&out_file).unwrap_or_else(|e| panic!("{e}"));
    for rec in &fixture.record {
        let should_appear = rec.label == "public" || rec.label == "bid-0003";
        assert_eq!(
            disk_text.contains(&rec.canary),
            should_appear,
            "canary `{}` (label `{}`) presence on disk must be {should_appear}",
            rec.canary,
            rec.label
        );
    }
}

fn run_journal_holds_ids_and_no_canary() {
    let fixture = load_fixture();
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let tree = fieldbook::io::read_tree(tmp.path()).unwrap_or_else(|e| panic!("{e}"));
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    let journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));

    let mut affected_refs: Vec<RecordRef> = fixture
        .record
        .iter()
        .map(|r| {
            RecordRef::parse(&format!("{}/{}", r.store.as_str(), r.id))
                .unwrap_or_else(|e| panic!("{e}"))
        })
        .collect();
    affected_refs.sort();

    // Emit every Kind in Kind::EGRESS through the real binary Journal and verify the on-disk
    // journal segment holds record IDs and zero canaries.
    let seg_path = world.state_dir().join("audit/journal.jsonl");
    for kind in Kind::EGRESS {
        let next_seq = std::fs::read_to_string(&seg_path)
            .unwrap_or_else(|e| panic!("{e}"))
            .lines()
            .filter(|l| !l.is_empty())
            .count() as u64
            + 1;
        let mut payload = sample_payload(kind, next_seq);
        match &mut payload {
            EventPayload::WriteBegin {
                affected,
                dest,
                path_sha256,
                policy,
                file_hashes,
                ..
            } => {
                *affected = affected_refs.clone();
                *file_hashes = None;
                *dest = Some(DestKind::Export);
                *path_sha256 = Some(Sha256::digest(b"/srv/company/exports/out.txt"));
                *policy = Some(Sha256::digest(POLICY_TOML.as_bytes()));
            }
            EventPayload::WriteEnd {
                affected,
                dest,
                path_sha256,
                policy,
                ..
            } => {
                *affected = affected_refs.clone();
                *dest = Some(DestKind::Export);
                *path_sha256 = Some(Sha256::digest(b"/srv/company/exports/out.txt"));
                *policy = Some(Sha256::digest(POLICY_TOML.as_bytes()));
            }
            EventPayload::AiBegin { context_ids, .. }
            | EventPayload::AiPack { context_ids, .. } => {
                *context_ids = affected_refs.clone();
            }
            EventPayload::McpCall { result_ids, .. } => {
                *result_ids = affected_refs.clone();
            }
            EventPayload::RestoreBegin { dest, policy, .. } => {
                *dest = Some(DestKind::Export);
                *policy = Some(Sha256::digest(POLICY_TOML.as_bytes()));
            }
            _ => {}
        }
        journal
            .append_payload(payload)
            .unwrap_or_else(|e| panic!("append {kind}: {e}"));
    }

    let raw_journal = std::fs::read_to_string(world.state_dir().join("audit/journal.jsonl"))
        .unwrap_or_else(|e| panic!("{e}"));

    let mut seen_egress = BTreeSet::new();
    for line in raw_journal.lines().filter(|l| !l.is_empty()) {
        let (ev, _) = Event::parse_line(line).unwrap_or_else(|e| panic!("parse journal line: {e}"));
        if ev.kind().is_egress() {
            seen_egress.insert(ev.kind().as_str().to_owned());
        }
    }
    let expected_egress: BTreeSet<String> =
        Kind::EGRESS.iter().map(|k| k.as_str().to_owned()).collect();
    assert_eq!(seen_egress, expected_egress);

    for rec in &fixture.record {
        assert!(
            raw_journal.contains(&rec.id),
            "journal must hold record id `{}`",
            rec.id
        );
        assert!(
            !raw_journal.contains(&rec.canary),
            "journal must never contain canary `{}`",
            rec.canary
        );
    }
}

struct TestProvenanceGraph {
    nodes: BTreeMap<ItemRef, LabelNode>,
    ledger: Ledger,
}

impl LabelGraph for TestProvenanceGraph {
    fn node(&self, item: &ItemRef) -> Option<LabelNode> {
        self.nodes.get(item).cloned()
    }

    fn ledger(&self, _store: StoreId) -> &Ledger {
        &self.ledger
    }
}

/// The delivered home and area pages show the permitted records and counts. `run_edition_surface`
/// then requires the plan with forbidden pages and rows added to equal these bytes exactly.
fn build_home_and_areas_counts_use_the_permitted_view(edition: &Path) {
    let read = |rel: &str| {
        std::fs::read_to_string(edition.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
    };
    let index = read("index.html");
    for shown in [
        "5 pages",
        "4 pages",
        "1 of 2 supported",
        "0 of 3 supported",
        "Q-001",
        "href=\"a/cmp.html\"",
        "href=\"a/est.html\"",
        "id=\"areas\"",
    ] {
        assert!(index.contains(shown), "index.html must show `{shown}`");
    }
    assert!(read("a/cmp.html").contains(
        "aria-label=\"1 of 2 supported: 1 supported, 1 unchecked, 0 contradicted, 0 stale, 0 unresolved\""
    ));
    assert!(read("a/est.html").contains(
        "aria-label=\"0 of 3 supported: 0 supported, 3 unchecked, 0 contradicted, 0 stale, 0 unresolved\""
    ));
}

/// The delivered About page and manifest count only the permitted view. `run_edition_surface`
/// then requires the plan with forbidden pages and rows added to equal these bytes exactly.
fn build_about_counts_use_the_permitted_view(edition: &Path) {
    let about = std::fs::read_to_string(edition.join("about.html"))
        .unwrap_or_else(|e| panic!("about.html: {e}"));
    for shown in [
        "<dt>Pages</dt><dd class=\"mono\">9</dd>",
        "<dt>Claims</dt><dd class=\"mono\">5</dd>",
        "<dt>Originals</dt><dd class=\"mono\">2</dd>",
        "This edition holds public only.",
    ] {
        assert!(about.contains(shown), "about.html must show `{shown}`");
    }
    let toml = std::fs::read_to_string(edition.join("edition.toml"))
        .unwrap_or_else(|e| panic!("edition.toml: {e}"));
    assert!(toml.contains("\n[counts]\npages = 9\n"), "{toml}");
    assert!(toml.contains("\nclaims = 5\n"), "{toml}");
}

fn run_edition_surface() {
    let fixture = load_fixture();
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let tree = fieldbook::io::read_tree(tmp.path()).unwrap_or_else(|e| panic!("{e}"));
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));

    let mut tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
    // An in-memory tree keeps the bytes of the stored original, so the plan ships it.
    tree.insert_bytes(
        "corpus/sources/public/S-0002-v2.txt",
        std::fs::read(tmp.path().join("corpus/sources/public/S-0002-v2.txt"))
            .unwrap_or_else(|e| panic!("{e}")),
    );
    // The same tree with the kept-row cells that name the canaries added below: the permitted
    // view keeps those cells verbatim, so this tree is what the mixed tree must equal. Its
    // references dangle (FB0207), and nothing else may be reported.
    let mut cells_tree = tree.clone();
    support::name_canaries_from_kept_rows(&mut cells_tree);
    let (model, report) = load(&Host::new(tree, world.today()), &Options::new(false));
    assert!(report.is_ok(), "corpus-mini must load cleanly");
    let (cells_model, cells_report) =
        load(&Host::new(cells_tree, world.today()), &Options::new(false));
    support::assert_only_dangling_references(&cells_report);

    let eff = EffectivePolicy::floor(false);
    let now = default_now();
    let shipped = Shipped::Hours(1);

    let Decision::Allowed(sel_token) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
        panic!("expected edition selection Allowed");
    };
    let view = LabelledView::new(&model, &sel_token, &ReadableStores::corpus());

    let mut plan = plan_edition(
        &view,
        &BuildInfo::default(),
        out_tmp.path(),
        &eff,
        &LabelSet::public(),
        now,
        shipped,
    )
    .unwrap_or_else(|e| panic!("plan_edition failed: {e:?}"))
    .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

    // Verify persisted output metadata keeps the input ItemRefs needed by `labels::effective_item`,
    // and reopening after source relabelling re-evaluates them.
    assert!(
        !plan.input_items.is_empty(),
        "edition plan must record input ItemRefs for effective_item"
    );
    let edition_item = ItemRef::new(StoreId::Corpus, "edition-output");
    let mut nodes = BTreeMap::new();
    for item in &plan.input_items {
        nodes.insert(item.clone(), LabelNode::new(LabelSet::public(), Vec::new()));
    }
    nodes.insert(
        edition_item.clone(),
        LabelNode::new(LabelSet::public(), plan.input_items.clone()),
    );
    let initial_graph = TestProvenanceGraph {
        nodes: nodes.clone(),
        ledger: Ledger::default(),
    };
    assert_eq!(
        effective_item(&initial_graph, &edition_item),
        LabelSet::public()
    );

    // Relabel one source input to a non-public label (`np-0007`) and confirm `effective_item`
    // re-evaluates the persisted `input_items` on reopen.
    let np7 = LabelId::parse("np-0007").unwrap_or_else(|e| panic!("{e}"));
    let first_input = plan.input_items[0].clone();
    nodes.insert(
        first_input,
        LabelNode::new(LabelSet::from_ids([PUBLIC, np7.clone()]), Vec::new()),
    );
    let relabelled_graph = TestProvenanceGraph {
        nodes,
        ledger: Ledger::default(),
    };
    let relabelled_eff = effective_item(&relabelled_graph, &edition_item);
    assert!(
        relabelled_eff.contains(&np7),
        "reopening after source relabelling must propagate np-0007 via effective_item"
    );
    assert!(
        matches!(
            check(
                &eff,
                &Dest::Edition,
                Some(&relabelled_eff),
                Some(&plan.delivery),
                now,
                shipped
            ),
            Decision::Denied(_)
        ),
        "Dest::Edition must deny relabelled effective_item set"
    );

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
        .unwrap_or_else(|e| panic!("{e}"));

    // Negative plants at the final sink (`write_edition_outputs`):
    // 1. Planting any non-public canary into a planned file after approval must be refused.
    let orig_site = plan.site.clone();
    for rec in &fixture.record {
        if rec.label != "public" {
            let mut mutated_site = Site::new();
            for (rel, bytes) in orig_site.files() {
                if rel == "index.html" {
                    let mut planted = bytes.clone();
                    planted.extend_from_slice(rec.canary.as_bytes());
                    mutated_site.insert(rel, planted);
                } else {
                    mutated_site.insert(rel, bytes.clone());
                }
            }
            plan.site = mutated_site;
            let res = write_edition_outputs(&plan, &audited);
            assert!(
                matches!(res, Err(Error::OutOfScope(_))),
                "mutated bytes with canary for `{}` must be refused at write_edition_outputs",
                rec.label
            );
        }
    }
    plan.site = orig_site;
    assert!(
        !plan.edition_dir.join("index.html").exists(),
        "refused byte-mutated write must not leave index.html on disk"
    );

    // 2. Changing the target directory path after approval must be refused.
    let rogue_dir = plan.out_dir.join("rogue-edition");
    let orig_edition_dir = std::mem::replace(&mut plan.edition_dir, rogue_dir.clone());
    let rogue_res = write_edition_outputs(&plan, &audited);
    plan.edition_dir = orig_edition_dir;
    assert!(
        matches!(rogue_res, Err(Error::OutOfScope(_))),
        "mutated target path must be refused at write_edition_outputs"
    );
    assert!(
        !rogue_dir.exists(),
        "refused target-mutated write must not create rogue directory"
    );

    // 2b. Changing the bytes of a shipped original after approval must be refused, and must
    //     leave neither folder on disk.
    assert!(
        plan.originals.contains_key("S-0002-v2.txt"),
        "the edition plan must carry the stored original"
    );
    let orig_originals = plan.originals.clone();
    for rec in &fixture.record {
        if rec.label != "public" {
            let mut mutated = orig_originals.clone();
            mutated
                .get_mut("S-0002-v2.txt")
                .unwrap_or_else(|| panic!("S-0002-v2.txt"))
                .extend_from_slice(rec.canary.as_bytes());
            plan.originals = mutated;
            let res = write_edition_outputs(&plan, &audited);
            assert!(
                matches!(res, Err(Error::OutOfScope(_))),
                "mutated original with canary for `{}` must be refused at write_edition_outputs",
                rec.label
            );
        }
    }
    plan.originals = orig_originals;
    assert!(
        !plan.originals_dir.exists() && !plan.edition_dir.exists(),
        "a refused original must leave neither folder on disk"
    );

    // 3. Deliver the approved public shell through the real sink and verify disk outputs.
    write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));

    assert!(plan.marker_path.is_file(), ".fieldbook-out must exist");
    let nav_base =
        fieldbook_render::nav::build(&view, &fieldbook_render::site::planned_paths(&view));
    let mut mixed_tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
    mixed_tree.insert_bytes(
        "corpus/pages/estimating/pricing/est-forbidden-canary.md",
        b"+++\nid = \"est-forbidden-canary\"\ntitle = \"Forbidden pricing canary\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSecret.\n\n## The rule\n\nSecret.\n\n## How it works\n\nSecret.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSecret.\n\n## Never confuse\n\nSecret.\n\n## Company calibration\n\nSecret.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-03-01: Secret.\n"
            .to_vec(),
    );
    support::add_canary_register_rows(&mut mixed_tree);
    // The left-out source also has a stored original; the permitted view must not ship it.
    mixed_tree.insert_bytes(
        "corpus/sources/public/S-0002-v2.txt",
        std::fs::read(tmp.path().join("corpus/sources/public/S-0002-v2.txt"))
            .unwrap_or_else(|e| panic!("{e}")),
    );
    support::set_register_cell(
        &mut mixed_tree,
        "sources",
        "S-0099",
        "location",
        "S-0099-v1.txt",
    );
    let canary_original = b"Forbidden original canary, made up for tests.\n";
    support::set_register_cell(
        &mut mixed_tree,
        "sources",
        "S-0099",
        "sha256",
        &fieldbook_core::hash::sha256_hex(canary_original),
    );
    mixed_tree.insert_bytes(
        "corpus/sources/public/S-0099-v1.txt",
        canary_original.to_vec(),
    );
    // The canaries are well formed: the loader reports only the repository-Corpus handling of
    // the `np-0007` source and page (FB0410) and the canary claim left on the version that the
    // kept `S-0001` supersedes (FB0222), and still builds the model the view must filter.
    let (mixed_model, mixed_report) =
        load(&Host::new(mixed_tree, world.today()), &Options::new(false));
    support::assert_report_codes(
        &mixed_report,
        &[
            ("FB0410", Severity::Error),
            ("FB0410", Severity::Error),
            ("FB0222", Severity::Warning),
        ],
    );
    let mixed_view = LabelledView::new(&mixed_model, &sel_token, &ReadableStores::corpus());
    let nav_mixed = fieldbook_render::nav::build(
        &mixed_view,
        &fieldbook_render::site::planned_paths(&mixed_view),
    );
    assert_eq!(
        nav_mixed, nav_base,
        "edition navigation counts and destinations must reflect only the permitted view"
    );

    let index_disk = std::fs::read_to_string(plan.edition_dir.join("index.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(index_disk.contains("<nav aria-label=\"Sections\">"));
    assert!(index_disk.contains("<details class=\"fold nav-more\">"));
    assert!(!index_disk.contains("class=\"not-available\""));
    build_home_and_areas_counts_use_the_permitted_view(&plan.edition_dir);
    build_about_counts_use_the_permitted_view(&plan.edition_dir);

    let concept_disk = std::fs::read_to_string(plan.edition_dir.join("p/cmp-system-review.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    let steps_disk = std::fs::read_to_string(plan.edition_dir.join("p/cmp-review-steps.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        concept_disk.contains("Synthetic business-system review and withholding overview"),
        "permitted page title must reach edition output"
    );
    assert!(
        concept_disk.contains(
            "Synthetic reviewers check control operation and record open items before billing runs."
        ),
        "permitted page body must reach edition output"
    );
    assert!(
        steps_disk.contains("class=\"plink\" href=\"cmp-system-review.html\""),
        "permitted inter-page PageLink must reach edition output"
    );

    // Verify all 5 register pages and ids.html are delivered with permitted rows, IDs, and counts.
    let claims_disk = std::fs::read_to_string(plan.edition_dir.join("claims.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    let claims_part_disk = std::fs::read_to_string(plan.edition_dir.join("claims/CMP-1.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    let sources_disk = std::fs::read_to_string(plan.edition_dir.join("sources.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    let questions_disk = std::fs::read_to_string(plan.edition_dir.join("questions.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    let conflicts_disk = std::fs::read_to_string(plan.edition_dir.join("conflicts.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    let glossary_disk = std::fs::read_to_string(plan.edition_dir.join("glossary.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    // `ids.html` is an index; the ids are on the group pages and the claims part pages under `ids/`.
    let ids_disk = [
        "ids/pages.html",
        "ids/sources.html",
        "ids/versions.html",
        "ids/claims-CMP-1.html",
        "ids/questions.html",
        "ids/conflicts.html",
        "ids/terms.html",
    ]
    .iter()
    .map(|page| {
        std::fs::read_to_string(plan.edition_dir.join(page)).unwrap_or_else(|e| panic!("{e}"))
    })
    .collect::<String>();

    assert!(
        claims_part_disk.contains("<tr id=\"CL-CMP-0001\"")
            && claims_disk.contains("<span class=\"chip mono\">5 rows</span>"),
        "permitted claim rows must reach claims/CMP-1.html and the count claims.html"
    );
    assert!(
        sources_disk.contains("<tr id=\"S-0002-v2\"")
            && sources_disk.contains("<span class=\"chip mono\">4 rows</span>"),
        "permitted source rows and count must reach sources.html"
    );
    assert!(
        questions_disk.contains("<tr id=\"Q-001\"")
            && questions_disk.contains("<span class=\"chip mono\">2 rows</span>"),
        "permitted question rows and count must reach questions.html"
    );
    assert!(
        conflicts_disk.contains("<tr id=\"XC-01\"")
            && conflicts_disk.contains("<span class=\"chip mono\">1 row</span>"),
        "permitted conflict rows and count must reach conflicts.html"
    );
    assert!(
        glossary_disk.contains("<tr id=\"system_review\"")
            && glossary_disk.contains("<span class=\"chip mono\">4 rows</span>"),
        "permitted glossary rows and count must reach glossary.html"
    );
    assert!(
        ids_disk.contains("data-id=\"cmp-system-review\"")
            && ids_disk.contains("data-id=\"S-0002\"")
            && ids_disk.contains("data-id=\"S-0002-v2\"")
            && ids_disk.contains("data-id=\"CL-CMP-0001\"")
            && ids_disk.contains("data-id=\"Q-001\"")
            && ids_disk.contains("data-id=\"XC-01\"")
            && ids_disk.contains("data-id=\"system_review\""),
        "permitted IDs across all 7 groups must reach the ids/ pages"
    );

    // The delivered search index holds the permitted page, section and term units only.
    let index_js = std::fs::read_to_string(plan.edition_dir.join("assets/search-index.js"))
        .unwrap_or_else(|e| panic!("{e}"));
    let index_json: serde_json::Value = serde_json::from_str(
        index_js
            .strip_prefix("window.FB_INDEX = ")
            .and_then(|s| s.strip_suffix(';'))
            .unwrap_or_else(|| panic!("the index must be `window.FB_INDEX = <json>;`")),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let index_docs = index_json["docs"]
        .as_array()
        .unwrap_or_else(|| panic!("docs"));
    for permitted in ["cmp-system-review", "cmp-system-review#purpose"] {
        assert!(
            index_docs.iter().any(|d| d[0] == permitted),
            "permitted unit `{permitted}` must reach assets/search-index.js"
        );
    }
    assert!(
        index_docs
            .iter()
            .all(|d| d[1] == "page" || d[1] == "clause" || d[1] == "term"),
        "assets/search-index.js holds page, section and term units only"
    );

    let mixed_out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let mixed_plan = plan_edition(
        &mixed_view,
        &BuildInfo::default(),
        mixed_out_tmp.path(),
        &eff,
        &LabelSet::public(),
        now,
        shipped,
    )
    .unwrap_or_else(|e| panic!("plan_edition mixed failed: {e:?}"))
    .unwrap_or_else(|d| panic!("plan_edition mixed denied: {d:?}"));
    assert!(
        !mixed_plan
            .site
            .files()
            .contains_key("p/est-forbidden-canary.html"),
        "forbidden page must be absent from mixed edition plan"
    );
    let mixed_index = String::from_utf8_lossy(
        mixed_plan
            .site
            .files()
            .get("assets/search-index.js")
            .unwrap_or_else(|| panic!("the mixed plan must hold assets/search-index.js")),
    )
    .into_owned();
    for forbidden in [
        "est-forbidden-canary",
        "Forbidden pricing canary",
        "np-0007",
    ] {
        assert!(
            !mixed_index.contains(forbidden),
            "forbidden `{forbidden}` must not reach assets/search-index.js"
        );
    }
    let cells_view = LabelledView::new(&cells_model, &sel_token, &ReadableStores::corpus());
    assert_eq!(cells_view.left_out(), 0);
    let cells_out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let cells_plan = plan_edition(
        &cells_view,
        &BuildInfo::default(),
        cells_out_tmp.path(),
        &eff,
        &LabelSet::public(),
        now,
        shipped,
    )
    .unwrap_or_else(|e| panic!("plan_edition cells failed: {e:?}"))
    .unwrap_or_else(|d| panic!("plan_edition cells denied: {d:?}"));
    assert_eq!(
        mixed_plan.site.files(),
        cells_plan.site.files(),
        "forbidden rows, IDs, and counts must be completely absent from mixed edition outputs, edition.toml and its fingerprint included"
    );
    // The cells base differs from the plain base only by the kept-row cells that name left-out
    // records; every page but the manifest must match the plain base, as on main.
    support::assert_plain_site_equals_cells_site(plan.site.files(), cells_plan.site.files());
    assert_eq!(
        mixed_plan.originals, cells_plan.originals,
        "forbidden rows must not change the shipped originals"
    );
    for (name, bytes) in &mixed_plan.originals {
        assert!(
            !String::from_utf8_lossy(bytes).contains("Forbidden original canary"),
            "a left-out original must not be shipped as originals/{name}"
        );
    }
    for (rel, bytes) in mixed_plan.site.files() {
        let s = String::from_utf8_lossy(bytes);
        for forbidden_needle in [
            "Forbidden pricing canary",
            "est-forbidden-canary.html",
            "S-0099",
            "Forbidden source canary",
            "CL-CMP-0099",
            "Forbidden claim canary statement.",
            "XC-99",
            "Forbidden conflict canary topic",
            "forbidden_term_canary",
            "Forbidden term canary",
        ] {
            assert!(
                !s.contains(forbidden_needle),
                "forbidden canary `{forbidden_needle}` must not appear in {rel}"
            );
        }
    }

    // The source pages and the originals reach disk from the permitted view only.
    for vid in ["S-0001-v1", "S-0002-v1", "S-0002-v2", "S-0003-v1"] {
        assert!(
            plan.edition_dir.join(format!("s/{vid}.html")).is_file(),
            "permitted source page s/{vid}.html must reach edition output"
        );
    }
    assert!(!plan.edition_dir.join("s/S-0099-v1.html").exists());
    assert!(
        mixed_plan
            .site
            .files()
            .keys()
            .all(|k| !k.starts_with("s/S-0099")),
        "a left-out source must have no page"
    );
    assert_eq!(
        mixed_plan.originals, plan.originals,
        "a left-out source must ship no original"
    );
    for (name, expected_bytes) in &plan.originals {
        let disk_bytes =
            std::fs::read(plan.originals_dir.join(name)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&disk_bytes, expected_bytes, "mismatch on originals/{name}");
        let disk_str = String::from_utf8_lossy(&disk_bytes);
        for rec in &fixture.record {
            if rec.label != "public" {
                assert!(
                    !disk_str.contains(&rec.canary),
                    "forbidden canary `{}` must not appear in originals/{name}",
                    rec.canary
                );
            }
        }
    }

    for (rel, expected_bytes) in plan.site.files() {
        let disk_path = plan.edition_dir.join(rel);
        let disk_bytes = std::fs::read(&disk_path).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(&disk_bytes, expected_bytes, "mismatch on {rel}");
        let disk_str = String::from_utf8_lossy(&disk_bytes);
        for rec in &fixture.record {
            if rec.label != "public" {
                assert!(
                    !disk_str.contains(&rec.canary),
                    "forbidden canary `{}` must not appear in {rel}",
                    rec.canary
                );
            }
        }
    }
    check_delivered_map_is_public_and_bound();
}

/// The text of the `<pre>` of the context pack of an edition page, unescaped for the characters
/// the pack surface asserts on.
fn pack_pre(html: &str) -> &str {
    let start = html
        .find("<details class=\"context-pack\"><summary>context pack</summary><pre>")
        .unwrap_or_else(|| panic!("a page without a context pack"));
    let rest = &html[start..];
    let from = rest.find("<pre>").unwrap_or_else(|| panic!("<pre>")) + "<pre>".len();
    let to = rest.find("</pre>").unwrap_or_else(|| panic!("</pre>"));
    &rest[from..to]
}

/// The page packs of the edition: the real handler builds them for every page from the permitted
/// view only, the plan binds their bytes, and the final sink refuses a pack that changed.
fn run_page_packs_surface() {
    const FORBIDDEN_MARKER: &str = "Forbidden pack marker text";
    const FORBIDDEN_TITLE: &str = "Forbidden pack canary page";
    const PERMITTED_QUOTE: &str = "Synthetic compliance reviews occur every 30 days.";

    let fixture = load_fixture();
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let tree = fieldbook::io::read_tree(tmp.path()).unwrap_or_else(|e| panic!("{e}"));
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));

    // A page the edition selection leaves out, under the title of the page under test, so that a
    // pack built from anything but the permitted view would rank it as a related unit.
    let mut tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
    let forbidden_page = format!(
        "+++\nid = \"cmp-forbidden-pack\"\ntitle = \"Synthetic business-system review and withholding overview {FORBIDDEN_TITLE}\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\n{FORBIDDEN_MARKER}.\n\n## The rule\n\nSecret.\n\n## How it works\n\nSecret.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSecret.\n\n## Never confuse\n\nSecret.\n\n## Company calibration\n\nSecret.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-03-01: Secret.\n"
    );
    tree.insert_bytes(
        "corpus/pages/compliance/business-systems/cmp-forbidden-pack.md",
        forbidden_page.into_bytes(),
    );
    // The loader reports the repository-Corpus handling of the `np-0007` page (FB0410) and still
    // builds the model that the view must filter.
    let (model, _report) = load(&Host::new(tree, world.today()), &Options::new(false));

    let eff = EffectivePolicy::floor(false);
    let now = default_now();
    let shipped = Shipped::Hours(1);
    let Decision::Allowed(sel_token) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
        panic!("expected edition selection Allowed");
    };
    let view = LabelledView::new(&model, &sel_token, &ReadableStores::corpus());
    assert!(
        view.pages()
            .iter()
            .all(|p| p.id.as_str() != "cmp-forbidden-pack"),
        "the edition selection leaves the np-0007 page out"
    );

    let mut plan = plan_edition(
        &view,
        &BuildInfo::default(),
        out_tmp.path(),
        &eff,
        &LabelSet::public(),
        now,
        shipped,
    )
    .unwrap_or_else(|e| panic!("plan_edition failed: {e:?}"))
    .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

    // Every page carries a pack: the forbidden text is in none, the permitted quote is in the
    // pack of the page that cites it, and each ends with the fixed line and no count.
    let mut packs = 0usize;
    for (rel, bytes) in plan.site.files() {
        if !(rel.starts_with("p/") && rel.ends_with(".html")) {
            continue;
        }
        let html = String::from_utf8_lossy(bytes);
        let pre = pack_pre(&html);
        packs += 1;
        for forbidden in [
            FORBIDDEN_MARKER,
            FORBIDDEN_TITLE,
            "cmp-forbidden-pack",
            "np-0007",
        ] {
            assert!(
                !pre.contains(forbidden),
                "forbidden `{forbidden}` reached the pack of {rel}"
            );
        }
        let last = pre.trim_end().lines().last().unwrap_or_default();
        assert_eq!(
            last, "Some records were omitted.",
            "the pack of {rel} ends with the fixed line"
        );
        assert!(!last.chars().any(|c| c.is_ascii_digit()));
        assert!(pre.contains("\nLabels: public\n"), "{rel}");
    }
    assert_eq!(packs, view.pages().len(), "one pack on every page");
    let concept = String::from_utf8_lossy(
        plan.site
            .files()
            .get("p/cmp-system-review.html")
            .unwrap_or_else(|| panic!("p/cmp-system-review.html")),
    )
    .into_owned();
    assert!(
        pack_pre(&concept).contains(&PERMITTED_QUOTE.replace('"', "&#34;")),
        "the permitted claim quote must reach the pack of the page that cites it"
    );

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
        .unwrap_or_else(|e| panic!("{e}"));

    // Negative plants at the final sink: a canary planted into a pack after approval is refused,
    // and so is a changed target.
    let orig_site = plan.site.clone();
    for rec in &fixture.record {
        if rec.label != "public" {
            let mut mutated = Site::new();
            for (rel, bytes) in orig_site.files() {
                if rel == "p/cmp-system-review.html" {
                    let html = String::from_utf8_lossy(bytes).replace(
                        "</pre></details>",
                        &format!("{}</pre></details>", rec.canary),
                    );
                    mutated.insert(rel, html.into_bytes());
                } else {
                    mutated.insert(rel, bytes.clone());
                }
            }
            plan.site = mutated;
            let res = write_edition_outputs(&plan, &audited);
            assert!(
                matches!(res, Err(Error::OutOfScope(_))),
                "a canary for `{}` planted in a pack must be refused at write_edition_outputs",
                rec.label
            );
        }
    }
    plan.site = orig_site;
    assert!(
        !plan.edition_dir.join("p/cmp-system-review.html").exists(),
        "a refused write must not leave a page on disk"
    );
    let rogue_dir = plan.out_dir.join("rogue-packs");
    let orig_edition_dir = std::mem::replace(&mut plan.edition_dir, rogue_dir.clone());
    let rogue = write_edition_outputs(&plan, &audited);
    plan.edition_dir = orig_edition_dir;
    assert!(
        matches!(rogue, Err(Error::OutOfScope(_))) && !rogue_dir.exists(),
        "a changed target must be refused at write_edition_outputs"
    );

    // The approved packs reach disk as planned, and no forbidden canary is on disk.
    write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));
    let disk = std::fs::read_to_string(plan.edition_dir.join("p/cmp-system-review.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(disk, concept, "the delivered page is the planned page");
    for rec in &fixture.record {
        if rec.label != "public" {
            assert!(!disk.contains(&rec.canary), "{}", rec.canary);
        }
    }
}

fn run_backup_surface() {
    let fixture = load_fixture();
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let tree = fieldbook::io::read_tree(tmp.path()).unwrap_or_else(|e| panic!("{e}"));
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    let target_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let target_path = target_tmp.path().to_path_buf();

    // 1. Setup clean state directory with settings and journal
    let state_mini_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/state-mini");
    let state_dir = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state_dir).unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir_all(state_dir.join("audit")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::copy(
        state_mini_path.join("settings.toml"),
        state_dir.join("settings.toml"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    std::fs::copy(
        state_mini_path.join("audit/journal.jsonl"),
        state_dir.join("audit/journal.jsonl"),
    )
    .unwrap_or_else(|e| panic!("{e}"));

    // Add target to settings.toml
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_path.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_dir.join("settings.toml"), settings_text)
        .unwrap_or_else(|e| panic!("{e}"));

    let eff = EffectivePolicy::floor(false);
    let options = fieldbook::backup::BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };

    // 2. Destination / payload binding checks:
    let now = default_now();
    let shipped = Shipped::Hours(1);
    let dest = Dest::Backup {
        path: target_path.to_string_lossy().into_owned(),
        company_storage: false,
    };
    let test_file = target_path.join("binding-test.txt");
    let delivery = Delivery::new(
        vec![Transfer::write_path(&test_file, b"approved-bytes")],
        None,
    );
    let Decision::Allowed(allowed) = check(
        &eff,
        &dest,
        Some(&LabelSet::public()),
        Some(&delivery),
        now,
        shipped,
    ) else {
        panic!("expected backup delivery Allowed");
    };
    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let policy_sha = Sha256::parse(allowed.policy_hash()).unwrap_or_else(|e| panic!("{e}"));
    let _prep = journal
        .prepare_backup_begin(&LabelSet::public(), &policy_sha, 1)
        .unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_backup(&allowed, &target_path, "folder", 1)
        .unwrap_or_else(|e| panic!("{e}"));

    // Check payload mutation is refused:
    let mut bad_bytes = b"approved-bytes".to_vec();
    bad_bytes.extend_from_slice(b"mutated");
    let res_mut = world.write_outside(&allowed, &audited, &test_file, &bad_bytes);
    assert!(
        matches!(res_mut, Err(Error::OutOfScope(_))),
        "mutated bytes must be refused by write_outside"
    );

    // Check target path mutation is refused:
    let rogue_file = target_path.join("rogue.txt");
    let res_rogue = world.write_outside(&allowed, &audited, &rogue_file, b"approved-bytes");
    assert!(
        matches!(res_rogue, Err(Error::OutOfScope(_))),
        "mutated target path must be refused by write_outside"
    );

    // Check input item refs and reopening after relabelling:
    let backup_item = ItemRef::new(StoreId::Corpus, "backup-output");
    let source_item = ItemRef::new(StoreId::Corpus, "sources/public/S-0002-v2.txt");
    let mut nodes = BTreeMap::new();
    nodes.insert(
        source_item.clone(),
        LabelNode::new(LabelSet::public(), Vec::new()),
    );
    nodes.insert(
        backup_item.clone(),
        LabelNode::new(LabelSet::public(), vec![source_item.clone()]),
    );
    let graph = TestProvenanceGraph {
        nodes: nodes.clone(),
        ledger: Ledger::default(),
    };
    assert_eq!(effective_item(&graph, &backup_item), LabelSet::public());

    let np7 = LabelId::parse("np-0007").unwrap_or_else(|e| panic!("{e}"));
    nodes.insert(
        source_item,
        LabelNode::new(LabelSet::from_ids([PUBLIC, np7.clone()]), Vec::new()),
    );
    let relabelled_graph = TestProvenanceGraph {
        nodes,
        ledger: Ledger::default(),
    };
    let relabelled_eff = effective_item(&relabelled_graph, &backup_item);
    assert!(relabelled_eff.contains(&np7));
    assert!(
        matches!(
            check(
                &eff,
                &dest,
                Some(&relabelled_eff),
                Some(&delivery),
                now,
                shipped
            ),
            Decision::Denied(_)
        ),
        "Dest::Backup must deny relabelled effective_item set"
    );

    // End the journal transaction from our binding test so the lock is released
    journal
        .end_backup(
            audited,
            Word::new_static("ok"),
            1,
            14,
            Sha256::digest(b"manifest"),
            1,
            Sha256::digest(b"head"),
            Sha256::digest(b"fp"),
            Word::new_static("folder"),
        )
        .unwrap_or_else(|e| panic!("{e}"));

    // 3. Exercise real backup handler on clean tree:
    let outcome = fieldbook::backup::run_backup(&world, &eff, &options);
    assert_eq!(
        outcome.code, 0,
        "backup on clean tree must succeed: {}",
        outcome.stderr
    );
    let snap_dir = target_path.join("fieldbook-backup-0001");
    assert!(snap_dir.is_dir(), "snapshot 1 directory must exist");
    assert!(snap_dir.join("backup.toml").is_file());
    assert!(snap_dir.join("SHA256SUMS").is_file());
    assert!(target_path.join("LATEST").is_file());

    // 4. Exercise forbidden canary refusal:
    // Plant a forbidden canary page in the corpus
    let forbidden_page = tmp
        .path()
        .join("corpus/pages/estimating/pricing/forbidden-canary.md");
    let canary_content = "+++\nid = \"forbidden-canary\"\ntitle = \"Forbidden canary\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nSecret canary np-0007.\n";
    std::fs::write(&forbidden_page, canary_content).unwrap_or_else(|e| panic!("{e}"));

    let outcome_forbidden = fieldbook::backup::run_backup(&world, &eff, &options);
    assert_eq!(
        outcome_forbidden.code, 2,
        "backup with forbidden canary must be refused with code 2 (FB0901)"
    );
    assert!(
        outcome_forbidden.stderr.contains("FB0901"),
        "stderr must report FB0901: {}",
        outcome_forbidden.stderr
    );
    assert!(
        !target_path.join("fieldbook-backup-0002").exists(),
        "refused backup must not create fieldbook-backup-0002"
    );

    // Verify disk of successful snapshot 1 does not contain any forbidden canary
    let sums_text =
        std::fs::read_to_string(snap_dir.join("SHA256SUMS")).unwrap_or_else(|e| panic!("{e}"));
    assert!(!sums_text.contains("forbidden-canary"));
    for rec in &fixture.record {
        if rec.label != "public" && rec.label != "owner-supplied" {
            assert!(!sums_text.contains(&rec.canary));
        }
    }
}

fn run_day_one_binder_surface() {
    const FORBIDDEN_MARKER: &str = "Forbidden binder marker text";
    const FORBIDDEN_TITLE: &str = "Forbidden binder canary page";
    const FORBIDDEN_BRIEF_MARKER: &str = "Forbidden binder brief marker";
    const FORBIDDEN_BRIEF_TITLE: &str = "Forbidden binder brief canary";

    let fixture = load_fixture();
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));

    let mut tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
    let forbidden_concept = format!(
        "+++\nid = \"cmp-forbidden-binder\"\ntitle = \"Synthetic compliance {FORBIDDEN_TITLE}\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\n{FORBIDDEN_MARKER}.\n\n## The rule\n\nSecret.\n\n## How it works\n\nSecret.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSecret.\n\n## Never confuse\n\nSecret.\n\n## Company calibration\n\nSecret.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-03-01: Secret.\n"
    );
    tree.insert_bytes(
        "corpus/pages/compliance/business-systems/cmp-forbidden-binder.md",
        forbidden_concept.into_bytes(),
    );

    let forbidden_brief = format!(
        "+++\nid = \"est-forbidden-brief\"\ntitle = \"Synthetic decision brief on {FORBIDDEN_BRIEF_TITLE}\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"brief\"\nstage = 3\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nvolatility = \"V2\"\n+++\n\n## Question\n\nSynthetic brief question?\n\n## Recommendation\n\n{FORBIDDEN_BRIEF_MARKER}.\n\n## Evidence\n\n- Synthetic brief evidence.\n\n## Options\n\n- Synthetic option.\n\n## Unknowns\n\n- Synthetic unknown.\n\n## Next action\n\nSynthetic next action.\n"
    );
    tree.insert_bytes(
        "corpus/pages/estimating/pricing/est-forbidden-brief.md",
        forbidden_brief.into_bytes(),
    );

    let (model, _report) = load(&Host::new(tree, world.today()), &Options::new(false));

    let eff = EffectivePolicy::floor(false);
    let now = default_now();
    let shipped = Shipped::Hours(1);
    let Decision::Allowed(sel_token) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
        panic!("expected edition selection Allowed");
    };
    let view = LabelledView::new(&model, &sel_token, &ReadableStores::corpus());
    assert!(
        view.pages()
            .iter()
            .all(|p| p.id.as_str() != "cmp-forbidden-binder"
                && p.id.as_str() != "est-forbidden-brief"),
        "the edition selection leaves the forbidden pages out"
    );

    let mut plan = plan_edition(
        &view,
        &BuildInfo::default(),
        out_tmp.path(),
        &eff,
        &LabelSet::public(),
        now,
        shipped,
    )
    .unwrap_or_else(|e| panic!("plan_edition failed: {e:?}"))
    .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

    let binder_path = day_one::OUTPUT_PATH;

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
        .unwrap_or_else(|e| panic!("{e}"));

    // Negative plants at the final sink: for every non-public fixture canary, plant it into the
    // planned day-one.html bytes and require write_edition_outputs to return OutOfScope and leave
    // no day-one.html on disk.
    let orig_site = plan.site.clone();
    for rec in &fixture.record {
        if rec.label != "public" {
            let mut mutated = Site::new();
            for (rel, bytes) in orig_site.files() {
                if rel == binder_path {
                    let mut planted = bytes.clone();
                    planted.extend_from_slice(rec.canary.as_bytes());
                    mutated.insert(rel, planted);
                } else {
                    mutated.insert(rel, bytes.clone());
                }
            }
            plan.site = mutated;
            let res = write_edition_outputs(&plan, &audited);
            assert!(
                matches!(res, Err(Error::OutOfScope(_))),
                "a canary for `{}` planted in day-one.html must be refused at write_edition_outputs",
                rec.label
            );
        }
    }
    plan.site = orig_site;
    assert!(
        !plan.edition_dir.join(binder_path).exists(),
        "a refused write must not leave day-one.html on disk"
    );

    // Deliver the approved plan outputs and complete the export journal transaction.
    write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));

    let delivered_bytes =
        std::fs::read(plan.edition_dir.join(binder_path)).unwrap_or_else(|e| panic!("{e}"));
    let delivered = String::from_utf8(delivered_bytes.clone()).unwrap_or_else(|e| panic!("{e}"));

    let planned_bytes = plan
        .site
        .files()
        .get(binder_path)
        .unwrap_or_else(|| panic!("missing planned {binder_path}"));
    assert_eq!(
        &delivered_bytes, planned_bytes,
        "the delivered binder must equal the planned day-one.html bytes"
    );

    for title in [
        "Synthetic decision brief on forward pricing rate review window",
        "Synthetic compliance onboarding checklist across two windows",
        "Synthetic reference table of pricing rate windows and placeholders",
        "Synthetic business-system review and withholding overview",
    ] {
        assert!(
            delivered.contains(title),
            "permitted canary title `{title}` must appear in day-one.html"
        );
    }

    let pos_brief = delivered
        .find("data-page-id=\"est-rate-window-brief\"")
        .unwrap_or_else(|| panic!("est-rate-window-brief missing in binder"));
    let pos_st1 = delivered
        .find("data-page-id=\"cmp-onboarding-checklist\"")
        .unwrap_or_else(|| panic!("cmp-onboarding-checklist missing in binder"));
    let pos_st2_ref = delivered
        .find("data-page-id=\"est-rate-reference\"")
        .unwrap_or_else(|| panic!("est-rate-reference missing in binder"));
    let pos_st2_sys = delivered
        .find("data-page-id=\"cmp-system-review\"")
        .unwrap_or_else(|| panic!("cmp-system-review missing in binder"));

    assert!(
        pos_brief < pos_st1,
        "brief must precede stage 1 (brief: {pos_brief}, stage 1: {pos_st1})"
    );
    assert!(
        pos_st1 < pos_st2_ref && pos_st1 < pos_st2_sys,
        "stage 1 must precede stage 2 (stage 1: {pos_st1}, stage 2 ref: {pos_st2_ref}, stage 2 sys: {pos_st2_sys})"
    );

    assert!(
        !delivered.contains("data-page-id=\"cmp-review-steps\""),
        "page of another stage (cmp-review-steps) must have no article in binder"
    );

    for forbidden in [
        FORBIDDEN_TITLE,
        FORBIDDEN_MARKER,
        FORBIDDEN_BRIEF_TITLE,
        FORBIDDEN_BRIEF_MARKER,
        "cmp-forbidden-binder",
        "est-forbidden-brief",
        "np-0007",
    ] {
        assert!(
            !delivered.contains(forbidden),
            "forbidden string `{forbidden}` reached the binder"
        );
    }
    for rec in &fixture.record {
        if rec.label != "public" {
            assert!(
                !delivered.contains(&rec.canary),
                "non-public fixture canary `{}` reached the binder",
                rec.canary
            );
        }
    }

    assert!(
        !delivered.contains("left out"),
        "the text `left out` must be absent from the binder"
    );
    if let Some(idx) = delivered.find("omitted") {
        let after = &delivered[idx + "omitted".len()..];
        let digits = after.chars().take(10).any(|c| c.is_ascii_digit());
        assert!(!digits, "omitted followed by digits found in binder");
    }
}

struct MixedEditionDelivery {
    _src_tmp: tempfile::TempDir,
    _out_tmp: tempfile::TempDir,
    pub edition_dir: PathBuf,
    pub view: LabelledView,
}

fn deliver_mixed_edition(info: &BuildInfo) -> MixedEditionDelivery {
    let src_tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(src_tmp.path());
    let mut tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
    support::add_canary_register_rows(&mut tree);
    let (model, _report) = load(&Host::new(tree, world.today()), &Options::new(false));
    let eff = EffectivePolicy::floor(false);
    let now = default_now();
    let shipped = Shipped::Hours(1);
    let Decision::Allowed(sel_token) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
        panic!("expected edition selection Allowed");
    };
    let view = LabelledView::new(&model, &sel_token, &ReadableStores::corpus());
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let plan = plan_edition(
        &view,
        info,
        out_tmp.path(),
        &eff,
        &LabelSet::public(),
        now,
        shipped,
    )
    .unwrap_or_else(|e| panic!("plan_edition failed: {e:?}"))
    .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
        .unwrap_or_else(|e| panic!("{e}"));

    write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));

    MixedEditionDelivery {
        _src_tmp: src_tmp,
        _out_tmp: out_tmp,
        edition_dir: plan.edition_dir,
        view,
    }
}

fn unescape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            while let Some(&next_c) = chars.peek() {
                if next_c == ';' {
                    chars.next();
                    break;
                }
                entity.push(chars.next().unwrap_or_else(|| unreachable!()));
                if entity.len() > 10 {
                    break;
                }
            }
            match entity.as_str() {
                "amp" => out.push('&'),
                "lt" => out.push('<'),
                "gt" => out.push('>'),
                "quot" => out.push('"'),
                "apos" => out.push('\''),
                _ => {
                    out.push('&');
                    out.push_str(&entity);
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn extract_attr<'a>(tag: &'a str, attr: &str) -> Option<&'a str> {
    for prefix in [
        format!(" {attr}=\""),
        format!("\t{attr}=\""),
        format!("\n{attr}=\""),
    ] {
        if let Some(pos) = tag.find(&prefix) {
            let start = pos + prefix.len();
            let rest = &tag[start..];
            let end = rest.find('"')?;
            return Some(&rest[..end]);
        }
    }
    None
}

fn parse_cell_ref(r: &str) -> Option<(usize, usize)> {
    let col_end = r.find(|c: char| c.is_ascii_digit())?;
    let (col_letters, row_digits) = r.split_at(col_end);
    let row_num: usize = row_digits.parse().ok()?;
    if row_num == 0 || col_letters.is_empty() {
        return None;
    }
    let mut col_idx = 0usize;
    for ch in col_letters.chars() {
        if !ch.is_ascii_uppercase() {
            return None;
        }
        col_idx = col_idx
            .checked_mul(26)?
            .checked_add(ch as usize - 'A' as usize + 1)?;
    }
    Some((col_idx - 1, row_num - 1))
}

fn parse_workbook_sheet_names(workbook_xml: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut search_from = 0;
    while let Some(start) = workbook_xml[search_from..].find("<sheet ") {
        let tag_start = search_from + start;
        let Some(tag_len) = workbook_xml[tag_start..].find('>') else {
            break;
        };
        let tag = &workbook_xml[tag_start..tag_start + tag_len + 1];
        if let Some(name) = extract_attr(tag, "name") {
            names.push(unescape_xml(name));
        }
        search_from = tag_start + tag_len + 1;
    }
    names
}

fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut strings = Vec::new();
    let mut search_from = 0;
    while let Some(si_rel) = xml[search_from..].find("<si") {
        let start = search_from + si_rel;
        let Some(si_end) = xml[start..].find("</si>") else {
            break;
        };
        let si_content = &xml[start..start + si_end + 5];
        search_from = start + si_end + 5;

        let mut text = String::new();
        let mut t_search = 0;
        while let Some(t_open_rel) = si_content[t_search..].find("<t") {
            let t_open = t_search + t_open_rel;
            let Some(t_open_end) = si_content[t_open..].find('>') else {
                break;
            };
            let text_start = t_open + t_open_end + 1;
            let Some(t_close) = si_content[text_start..].find("</t>") else {
                break;
            };
            let raw_text = &si_content[text_start..text_start + t_close];
            text.push_str(&unescape_xml(raw_text));
            t_search = text_start + t_close + 4;
        }
        strings.push(text);
    }
    strings
}

fn workbook_sheets(bytes: &[u8]) -> BTreeMap<String, Vec<Vec<String>>> {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(&mut cursor).unwrap_or_else(|e| panic!("open zip archive: {e}"));

    let workbook_xml = {
        let mut f = archive
            .by_name("xl/workbook.xml")
            .unwrap_or_else(|e| panic!("xl/workbook.xml: {e}"));
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s)
            .unwrap_or_else(|e| panic!("read xl/workbook.xml: {e}"));
        s
    };
    let sheet_names = parse_workbook_sheet_names(&workbook_xml);

    let shared_strings = if let Ok(mut f) = archive.by_name("xl/sharedStrings.xml") {
        let mut s = String::new();
        std::io::Read::read_to_string(&mut f, &mut s)
            .unwrap_or_else(|e| panic!("read xl/sharedStrings.xml: {e}"));
        parse_shared_strings(&s)
    } else {
        Vec::new()
    };

    let mut sheets = BTreeMap::new();
    for (i, name) in sheet_names.into_iter().enumerate() {
        let sheet_path = format!("xl/worksheets/sheet{}.xml", i + 1);
        let mut f = archive
            .by_name(&sheet_path)
            .unwrap_or_else(|e| panic!("{sheet_path} exists: {e}"));
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut f, &mut xml)
            .unwrap_or_else(|e| panic!("read {sheet_path}: {e}"));

        let mut sheet_cells = BTreeMap::new();
        let mut search_from = 0;
        while let Some(c_rel) = xml[search_from..].find("<c ") {
            let c_start = search_from + c_rel;
            let Some(open_tag_end_rel) = xml[c_start..].find('>') else {
                break;
            };
            let open_tag_end = c_start + open_tag_end_rel;
            let open_tag = &xml[c_start..=open_tag_end];

            let cell_xml = if open_tag.ends_with("/>") {
                search_from = open_tag_end + 1;
                open_tag
            } else {
                let Some(close_rel) = xml[open_tag_end..].find("</c>") else {
                    break;
                };
                let close_end = open_tag_end + close_rel + 4;
                let full = &xml[c_start..close_end];
                search_from = close_end;
                full
            };

            let cell_ref = extract_attr(open_tag, "r")
                .unwrap_or_else(|| panic!("{name}: missing cell r attribute"));

            // A cell element whose `t` attribute is not `s`, or that holds an `<f>` element,
            // must panic with a message naming sheet and cell (`claims!C2`).
            if cell_xml.contains("<f>")
                || cell_xml.contains("<f ")
                || extract_attr(open_tag, "t") != Some("s")
            {
                panic!("{name}!{cell_ref}: cell t attribute is not s or holds formula");
            }

            let val = if let Some(v_start) = cell_xml.find("<v>") {
                let after_v = &cell_xml[v_start + 3..];
                let v_end = after_v
                    .find("</v>")
                    .unwrap_or_else(|| panic!("{name}!{cell_ref}: missing </v>"));
                let idx_str = &after_v[..v_end];
                let idx: usize = idx_str.parse().unwrap_or_else(|e| {
                    panic!("{name}!{cell_ref}: invalid shared string index `{idx_str}`: {e}")
                });
                shared_strings
                    .get(idx)
                    .unwrap_or_else(|| {
                        panic!("{name}!{cell_ref}: shared string index {idx} out of range")
                    })
                    .clone()
            } else {
                String::new()
            };

            let (col_idx, row_idx) = parse_cell_ref(cell_ref)
                .unwrap_or_else(|| panic!("{name}!{cell_ref}: invalid cell coordinate"));

            sheet_cells.insert((row_idx, col_idx), val);
        }

        let (num_rows, num_cols) = if sheet_cells.is_empty() {
            (0, 0)
        } else {
            let max_r = sheet_cells.keys().map(|(r, _)| *r).max().unwrap_or(0);
            let max_c = sheet_cells.keys().map(|(_, c)| *c).max().unwrap_or(0);
            (max_r + 1, max_c + 1)
        };
        let mut rows = vec![vec![String::new(); num_cols]; num_rows];
        for ((r, c), val) in sheet_cells {
            rows[r][c] = val;
        }

        sheets.insert(name, rows);
    }

    sheets
}

fn core_xml(bytes: &[u8]) -> String {
    let mut cursor = std::io::Cursor::new(bytes);
    let mut archive =
        ZipArchive::new(&mut cursor).unwrap_or_else(|e| panic!("open zip archive: {e}"));
    let mut core_file = archive
        .by_name("docProps/core.xml")
        .unwrap_or_else(|e| panic!("docProps/core.xml: {e}"));
    let mut s = String::new();
    std::io::Read::read_to_string(&mut core_file, &mut s)
        .unwrap_or_else(|e| panic!("read docProps/core.xml: {e}"));
    s
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => fields.push(std::mem::take(&mut cur)),
            (other, _) => cur.push(other),
        }
    }
    fields.push(cur);
    fields
}

const FORBIDDEN_REGISTER_STRINGS: &[&str] = &[
    "S-0099",
    "CL-CMP-0099",
    "XC-99",
    "Forbidden source canary",
    "Forbidden claim canary statement.",
    "Forbidden conflict canary topic",
    "forbidden_term_canary",
    "Forbidden term canary",
    "np-0007",
];

fn run_registers_workbook_surface() {
    let delivery = deliver_mixed_edition(&BuildInfo::default());
    let wb_path = delivery.edition_dir.join(fieldbook_core::strings::PATHS[0]);
    let wb_bytes =
        std::fs::read(&wb_path).unwrap_or_else(|e| panic!("read {}: {e}", wb_path.display()));
    let sheets = workbook_sheets(&wb_bytes);

    let mut expected_sheet_names =
        ["sources", "claims", "questions", "conflicts", "glossary"].to_vec();
    expected_sheet_names.sort();
    let actual_names: Vec<&str> = sheets.keys().map(String::as_str).collect();
    assert_eq!(
        actual_names, expected_sheet_names,
        "sheet names are exactly the five register names"
    );

    for &reg in &Register::ALL {
        let reg_name = fieldbook_render::xlsx::sheet_name(reg);
        let sheet_rows = sheets
            .get(reg_name)
            .unwrap_or_else(|| panic!("sheet {reg_name} exists"));
        assert!(!sheet_rows.is_empty(), "sheet {reg_name} has header row");
        let expected_header: Vec<&str> = reg.columns().iter().map(|col| col.name).collect();
        let actual_header: Vec<&str> = sheet_rows[0].iter().map(String::as_str).collect();
        assert_eq!(
            actual_header, expected_header,
            "header mismatch for {reg_name}"
        );
    }

    let fixture = load_fixture();
    let non_public_canaries: Vec<&str> = fixture
        .record
        .iter()
        .filter(|r| r.label != "public")
        .map(|r| r.canary.as_str())
        .collect();

    for (name, rows) in &sheets {
        for (r_idx, row) in rows.iter().enumerate() {
            for (c_idx, cell) in row.iter().enumerate() {
                for forbidden in FORBIDDEN_REGISTER_STRINGS {
                    assert!(
                        !cell.contains(forbidden),
                        "sheet {name} row {r_idx} col {c_idx} contains forbidden `{forbidden}`: `{cell}`"
                    );
                }
                for canary in &non_public_canaries {
                    assert!(
                        !cell.contains(canary),
                        "sheet {name} row {r_idx} col {c_idx} contains non-public canary `{canary}`: `{cell}`"
                    );
                }
            }
        }
    }

    let check_has_cell = |sheet: &str, needle: &str| {
        let rows = sheets.get(sheet).unwrap_or_else(|| panic!("sheet {sheet}"));
        assert!(
            rows.iter().any(|r| r.iter().any(|c| c == needle)),
            "permitted canary `{needle}` missing from sheet {sheet}"
        );
    };
    check_has_cell("sources", "S-0002");
    check_has_cell("claims", "CL-CMP-0001");
    check_has_cell("questions", "Q-001");
    check_has_cell("conflicts", "XC-01");
    check_has_cell("glossary", "system_review");

    assert_eq!(sheets["sources"].len() - 1, 4, "sources data-row count");
    assert_eq!(sheets["claims"].len() - 1, 5, "claims data-row count");
    assert_eq!(sheets["questions"].len() - 1, 2, "questions data-row count");
    assert_eq!(sheets["conflicts"].len() - 1, 1, "conflicts data-row count");
    assert_eq!(sheets["glossary"].len() - 1, 4, "glossary data-row count");

    {
        let rows = &sheets["claims"];
        let supersedes_idx = rows[0]
            .iter()
            .position(|h| h == "supersedes")
            .unwrap_or_else(|| panic!("supersedes column in claims"));
        let row = rows
            .iter()
            .find(|r| r.first().map(String::as_str) == Some("CL-EST-0002"))
            .unwrap_or_else(|| panic!("CL-EST-0002 row in claims"));
        assert!(
            row[supersedes_idx].is_empty(),
            "kept claim CL-EST-0002 supersedes cell must be empty, got `{}`",
            row[supersedes_idx]
        );
    }
    {
        let rows = &sheets["sources"];
        let supersedes_idx = rows[0]
            .iter()
            .position(|h| h == "supersedes")
            .unwrap_or_else(|| panic!("supersedes column in sources"));
        let row = rows
            .iter()
            .find(|r| r.first().map(String::as_str) == Some("S-0001"))
            .unwrap_or_else(|| panic!("S-0001 row in sources"));
        assert!(
            row[supersedes_idx].is_empty(),
            "kept source S-0001 supersedes cell must be empty, got `{}`",
            row[supersedes_idx]
        );
    }
    {
        let rows = &sheets["glossary"];
        let superseded_by_idx = rows[0]
            .iter()
            .position(|h| h == "superseded_by")
            .unwrap_or_else(|| panic!("superseded_by column in glossary"));
        let row = rows
            .iter()
            .find(|r| r.first().map(String::as_str) == Some("rate_window"))
            .unwrap_or_else(|| panic!("rate_window row in glossary"));
        assert!(
            row[superseded_by_idx].is_empty(),
            "kept term rate_window superseded_by cell must be empty, got `{}`",
            row[superseded_by_idx]
        );
    }

    for (name, rows) in &sheets {
        for (r_idx, row) in rows.iter().enumerate() {
            for (c_idx, cell) in row.iter().enumerate() {
                assert!(
                    !cell.contains("omitted"),
                    "sheet {name} row {r_idx} col {c_idx} contains `omitted`: `{cell}`"
                );
                assert!(
                    !cell.contains("Left out"),
                    "sheet {name} row {r_idx} col {c_idx} contains `Left out`: `{cell}`"
                );
            }
        }
    }

    let omission_line = delivery.view.left_out_line(&Dest::Edition);
    assert_eq!(
        omission_line, "Some records were omitted.",
        "edition omission line mismatch"
    );
    assert!(
        !omission_line.chars().any(|c| c.is_ascii_digit()),
        "omission line must not contain digits"
    );
}

fn run_register_csvs_surface() {
    let delivery = deliver_mixed_edition(&BuildInfo::default());
    let wb_bytes = std::fs::read(delivery.edition_dir.join(fieldbook_core::strings::PATHS[0]))
        .unwrap_or_else(|e| panic!("read data/registers.xlsx: {e}"));
    let sheets = workbook_sheets(&wb_bytes);

    let fixture = load_fixture();
    let non_public_canaries: Vec<&str> = fixture
        .record
        .iter()
        .filter(|r| r.label != "public")
        .map(|r| r.canary.as_str())
        .collect();

    let registers_info = [
        (Register::Sources, "sources", 4, "S-0002"),
        (Register::Claims, "claims", 5, "CL-CMP-0001"),
        (Register::Questions, "questions", 2, "Q-001"),
        (Register::Conflicts, "conflicts", 1, "XC-01"),
        (Register::Glossary, "glossary", 4, "system_review"),
    ];

    for (reg, reg_name, expected_count, permitted_canary) in registers_info {
        let csv_path = delivery.edition_dir.join(format!("data/{}.csv", reg_name));
        let bytes =
            std::fs::read(&csv_path).unwrap_or_else(|e| panic!("read {}: {e}", csv_path.display()));

        assert!(
            bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
            "CSV for {reg_name} must start with UTF-8 BOM"
        );

        for (i, &b) in bytes.iter().enumerate() {
            if b == b'\n' {
                assert!(
                    i > 0 && bytes[i - 1] == b'\r',
                    "Every LF must be preceded by CR in {reg_name} at byte {i}"
                );
            }
        }

        let text = std::str::from_utf8(&bytes[3..])
            .unwrap_or_else(|e| panic!("invalid UTF-8 in {reg_name}: {e}"));

        for forbidden in FORBIDDEN_REGISTER_STRINGS {
            assert!(
                !text.contains(forbidden),
                "CSV for {reg_name} contains forbidden string `{forbidden}`"
            );
        }
        for canary in &non_public_canaries {
            assert!(
                !text.contains(canary),
                "CSV for {reg_name} contains non-public canary `{canary}`"
            );
        }

        let lines: Vec<&str> = text
            .strip_suffix("\r\n")
            .unwrap_or_else(|| panic!("CSV for {reg_name} must end with CRLF"))
            .split("\r\n")
            .collect();
        assert!(
            !lines.is_empty(),
            "CSV for {reg_name} must have header line"
        );

        let expected_header = reg
            .columns()
            .iter()
            .map(|col| col.name)
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(
            lines[0], expected_header,
            "header line mismatch in CSV for {reg_name}"
        );

        let data_lines = &lines[1..];
        assert_eq!(
            data_lines.len(),
            expected_count,
            "data line count mismatch for {reg_name}"
        );

        let parsed_data_rows: Vec<Vec<String>> =
            data_lines.iter().map(|line| parse_csv_line(line)).collect();

        assert!(
            parsed_data_rows
                .iter()
                .any(|row| row.first().map(String::as_str) == Some(permitted_canary)),
            "CSV for {reg_name} must have `{permitted_canary}` as the first cell of one line"
        );

        let wb_rows = sheets
            .get(reg_name)
            .unwrap_or_else(|| panic!("sheet {reg_name}"));
        let wb_data_rows = &wb_rows[1..];
        assert_eq!(
            &parsed_data_rows, wb_data_rows,
            "CSV data rows must equal workbook rows cell for cell for {reg_name}"
        );
    }
}

fn rewrite_restore_meta<F>(snap_dir: &Path, mutate: F)
where
    F: FnOnce(&mut BackupToml),
{
    let toml_path = snap_dir.join("backup.toml");
    let sums_path = snap_dir.join("SHA256SUMS");
    let toml_text = std::fs::read_to_string(&toml_path).unwrap_or_else(|e| panic!("{e}"));
    let sums_text = std::fs::read_to_string(&sums_path).unwrap_or_else(|e| panic!("{e}"));
    let mut info = BackupToml::parse(&toml_text).unwrap_or_else(|e| panic!("{e}"));
    mutate(&mut info);
    let new_manifest = manifest_sha256(&info, &sums_text).unwrap_or_else(|e| panic!("{e}"));
    info.manifest_sha256 = new_manifest.as_str().to_string();
    let rewritten = toml::to_string_pretty(&info).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(&toml_path, rewritten).unwrap_or_else(|e| panic!("{e}"));
}

fn read_journal_kinds(state_dir: &Path) -> Vec<String> {
    let audit_dir = state_dir.join("audit");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&audit_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("jsonl") {
                files.push(p);
            }
        }
    }
    files.sort();
    let mut kinds = Vec::new();
    for f in files {
        let content = std::fs::read_to_string(&f).unwrap_or_else(|e| panic!("{e}"));
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let (ev, _) =
                Event::parse_line(trimmed).unwrap_or_else(|e| panic!("parse journal line: {e}"));
            kinds.push(ev.kind().as_str().to_string());
        }
    }
    kinds
}

fn dir_contains_bytes(dir: &Path, needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file()
                && let Ok(bytes) = std::fs::read(&path)
                && bytes.windows(needle.len()).any(|w| w == needle)
            {
                return true;
            }
        }
    }
    false
}

fn run_restore_surface() {
    let fixture = load_fixture();
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let tree = fieldbook::io::read_tree(tmp.path()).unwrap_or_else(|e| panic!("{e}"));
    if let Ok(cat) = fieldbook_core::manifest::SupportCatalogue::from_public_source(
        &tree,
        "1111111111111111111111111111111111111111",
    ) {
        let (_, cat_sha) = cat.to_toml_bytes_and_sha256();
        world.set_support_catalogue(cat, cat_sha);
    }
    let target_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let target_path = target_tmp.path().to_path_buf();

    // 1. Setup clean state directory with settings and journal
    let state_mini_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/state-mini");
    let state_dir = tmp.path().join(".fieldbook");
    std::fs::create_dir_all(&state_dir).unwrap_or_else(|e| panic!("{e}"));
    std::fs::create_dir_all(state_dir.join("audit")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::copy(
        state_mini_path.join("settings.toml"),
        state_dir.join("settings.toml"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    std::fs::copy(
        state_mini_path.join("audit/journal.jsonl"),
        state_dir.join("audit/journal.jsonl"),
    )
    .unwrap_or_else(|e| panic!("{e}"));

    // Add target to settings.toml
    let settings_text = format!(
        "role = \"lead\"\n\n[[backup.target]]\nid = \"main\"\nkind = \"folder\"\npath = \"{}\"\n",
        target_path.to_string_lossy().replace('\\', "/")
    );
    std::fs::write(state_dir.join("settings.toml"), settings_text)
        .unwrap_or_else(|e| panic!("{e}"));

    let eff = EffectivePolicy::floor(false);

    // 2. Destination / payload binding checks
    let binding_scratch = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let binding_dest = binding_scratch
        .path()
        .canonicalize()
        .unwrap_or_else(|e| panic!("{e}"))
        .join("binding-dest");
    let test_file = binding_dest.join("binding-test.txt");
    let approved_bytes = b"approved-restore-bytes";
    let test_sha = Sha256::digest(approved_bytes).as_str().to_string();
    let test_len = approved_bytes.len() as u64;

    let planned_files = vec![PlannedFile {
        path: test_file.clone(),
        sha256: test_sha.clone(),
        bytes: test_len,
    }];
    let plan = RestorePlan {
        destination: binding_dest.clone(),
        roots: Vec::new(),
    };
    let labels = LabelSet::public();
    let request = RestoreExportRequest {
        snapshot: 1,
        mode: "real",
        labels: &labels,
        plan: &plan,
        files: &planned_files,
        create_destination: true,
    };

    let (dir_sha, dir_bytes) = hash_tree_manifest(&[]);
    let transfer_dir = Transfer {
        target: Target::Path(binding_dest.clone()),
        operation: Operation::CreateDir,
        sha256: dir_sha,
        bytes: dir_bytes,
    };
    let transfer_file = Transfer {
        target: Target::Path(test_file.clone()),
        operation: Operation::Write,
        sha256: test_sha,
        bytes: test_len,
    };
    let matching_transfers = vec![transfer_dir, transfer_file];
    let delivery = Delivery::new(matching_transfers, None);

    let dest = Dest::Export {
        path: binding_dest.to_string_lossy().into_owned(),
    };
    let now = default_now();
    let shipped = Shipped::Never;

    let Decision::Allowed(allowed) =
        check(&eff, &dest, Some(&labels), Some(&delivery), now, shipped)
    else {
        panic!("expected restore export delivery Allowed");
    };

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));

    // Selection-only token (no Delivery) passed to begin_restore_export is Err and appends no restore.begin
    let Decision::Allowed(selection_token) = check(&eff, &dest, Some(&labels), None, now, shipped)
    else {
        panic!("expected selection Allowed");
    };
    let kinds_before_sel = read_journal_kinds(world.state_dir());
    let res_sel = journal.begin_restore_export(&request, &selection_token);
    assert!(
        res_sel.is_err(),
        "selection-only token must be refused by begin_restore_export"
    );
    let kinds_after_sel = read_journal_kinds(world.state_dir());
    assert_eq!(
        kinds_before_sel, kinds_after_sel,
        "selection-only token must append no journal events"
    );

    let audited = journal
        .begin_restore_export(&request, &allowed)
        .unwrap_or_else(|e| panic!("{e}"));

    // Check payload mutation is refused:
    // (i) the right path with mutated bytes
    let mut bad_bytes = approved_bytes.to_vec();
    bad_bytes.extend_from_slice(b"-mutated");
    let res_mut = world.write_outside(&allowed, &audited, &test_file, &bad_bytes);
    assert!(
        matches!(res_mut, Err(Error::OutOfScope(_))),
        "mutated bytes must be refused by write_outside"
    );

    // (ii) a different path with the approved bytes
    let rogue_file = binding_dest.join("rogue.txt");
    let res_rogue = world.write_outside(&allowed, &audited, &rogue_file, approved_bytes);
    assert!(
        matches!(res_rogue, Err(Error::OutOfScope(_))),
        "mutated target path must be refused by write_outside"
    );

    // End restore to release the lock
    journal
        .end_restore(audited, 1, "real", "ok", None)
        .unwrap_or_else(|e| panic!("{e}"));

    // Check input item refs and reopening after relabelling:
    let restore_item = ItemRef::new(StoreId::Corpus, "restore-output");
    let source_item = ItemRef::new(StoreId::Corpus, "sources/public/S-0002-v2.txt");
    let mut nodes = BTreeMap::new();
    nodes.insert(
        source_item.clone(),
        LabelNode::new(LabelSet::public(), Vec::new()),
    );
    nodes.insert(
        restore_item.clone(),
        LabelNode::new(LabelSet::public(), vec![source_item.clone()]),
    );
    let graph = TestProvenanceGraph {
        nodes: nodes.clone(),
        ledger: Ledger::default(),
    };
    assert_eq!(effective_item(&graph, &restore_item), LabelSet::public());

    let np7 = LabelId::parse("np-0007").unwrap_or_else(|e| panic!("{e}"));
    nodes.insert(
        source_item,
        LabelNode::new(LabelSet::from_ids([PUBLIC, np7.clone()]), Vec::new()),
    );
    let relabelled_graph = TestProvenanceGraph {
        nodes,
        ledger: Ledger::default(),
    };
    let relabelled_eff = effective_item(&relabelled_graph, &restore_item);
    assert!(relabelled_eff.contains(&np7));
    assert!(
        matches!(
            check(
                &eff,
                &dest,
                Some(&relabelled_eff),
                Some(&delivery),
                now,
                shipped
            ),
            Decision::Denied(_)
        ),
        "Dest::Export must deny relabelled effective_item set"
    );

    // 3. Permitted canary: plant a public page under the temp corpus before backup
    let public_canary = fixture
        .record
        .iter()
        .find(|r| r.label == "public")
        .unwrap_or_else(|| panic!("missing public canary in records.toml"))
        .canary
        .clone();
    let permitted_page = tmp
        .path()
        .join("corpus/pages/estimating/pricing/permitted-canary.md");
    let canary_content = format!(
        "+++\nid = \"permitted-canary\"\ntitle = \"Permitted canary\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-03-01\nhandling = [\"public\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\nPermitted canary {public_canary}.\n"
    );
    std::fs::write(&permitted_page, canary_content).unwrap_or_else(|e| panic!("{e}"));

    // Make snapshot 1 with the real run_backup
    let backup_options = fieldbook::backup::BackupOptions {
        target: Some("main".to_string()),
        to: None,
        dry_run: false,
    };
    let outcome_backup = fieldbook::backup::run_backup(&world, &eff, &backup_options);
    assert_eq!(
        outcome_backup.code, 0,
        "backup failed: {}",
        outcome_backup.stderr
    );
    let snap_dir = target_path.join("fieldbook-backup-0001");
    assert!(snap_dir.is_dir(), "snapshot 1 directory must exist");

    // Restore to fresh outside folder
    let restore_scratch = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let dest_ok = restore_scratch.path().join("restored-ok");
    assert!(!dest_ok.exists());

    let outcome_restore_ok = run_restore(
        &world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_ok.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        outcome_restore_ok.code, 0,
        "restore --to must succeed with exit 0: {}",
        outcome_restore_ok.stderr
    );
    assert!(dest_ok.is_dir(), "restored destination must exist");

    // Restored folder bytes contain the permitted canary and NO canary of any record whose label is not public/owner-supplied
    assert!(
        dir_contains_bytes(&dest_ok, public_canary.as_bytes()),
        "restored folder must contain permitted public canary `{public_canary}`"
    );
    for rec in &fixture.record {
        if rec.label != "public" && rec.label != "owner-supplied" {
            assert!(
                !dir_contains_bytes(&dest_ok, rec.canary.as_bytes()),
                "restored folder must NOT contain canary `{}` ({})",
                rec.canary,
                rec.label
            );
        }
    }

    // 4. Forbidden canary: rewrite snapshot backup.toml labels to include np-0007
    rewrite_restore_meta(&snap_dir, |info| {
        info.labels.push("np-0007".to_string());
        info.labels.sort();
        info.labels.dedup();
    });
    let dest_forbidden = restore_scratch.path().join("restored-forbidden");
    assert!(!dest_forbidden.exists());

    let kinds_before_forbidden = read_journal_kinds(world.state_dir());
    let outcome_forbidden = run_restore(
        &world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_forbidden.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        outcome_forbidden.code, 2,
        "restore with forbidden canary np-0007 must exit with code 2: {}",
        outcome_forbidden.stderr
    );
    assert!(
        outcome_forbidden.stderr.contains("FB0901"),
        "stderr must contain FB0901: {}",
        outcome_forbidden.stderr
    );
    assert!(
        !dest_forbidden.exists(),
        "forbidden destination folder must NOT exist"
    );
    let kinds_after_forbidden = read_journal_kinds(world.state_dir());
    let new_kinds_forbidden = &kinds_after_forbidden[kinds_before_forbidden.len()..];
    assert!(
        new_kinds_forbidden.iter().any(|k| k == "refuse"),
        "journal must gain a refuse event for forbidden canary restore attempt"
    );
    assert!(
        !new_kinds_forbidden.iter().any(|k| k == "restore.begin"),
        "journal must gain NO restore.begin for forbidden canary restore attempt"
    );

    // Also an unlabelled snapshot label set refuses the same way (exit 2, FB0901, nothing written)
    rewrite_restore_meta(&snap_dir, |info| {
        info.labels = vec!["unlabelled".to_string()];
    });
    let dest_unlabelled = restore_scratch.path().join("restored-unlabelled");
    assert!(!dest_unlabelled.exists());

    let kinds_before_unlabelled = read_journal_kinds(world.state_dir());
    let outcome_unlabelled = run_restore(
        &world,
        &eff,
        &RestoreOptions {
            snapshot: 1,
            to: Some(dest_unlabelled.clone()),
            drill: false,
            target: None,
            from: None,
        },
    );
    assert_eq!(
        outcome_unlabelled.code, 2,
        "restore with unlabelled snapshot must exit with code 2: {}",
        outcome_unlabelled.stderr
    );
    assert!(
        outcome_unlabelled.stderr.contains("FB0901"),
        "stderr must contain FB0901: {}",
        outcome_unlabelled.stderr
    );
    assert!(
        !dest_unlabelled.exists(),
        "unlabelled destination folder must NOT exist"
    );
    let kinds_after_unlabelled = read_journal_kinds(world.state_dir());
    let new_kinds_unlabelled = &kinds_after_unlabelled[kinds_before_unlabelled.len()..];
    assert!(
        new_kinds_unlabelled.iter().any(|k| k == "refuse"),
        "journal must gain a refuse event for unlabelled restore attempt"
    );
    assert!(
        !new_kinds_unlabelled.iter().any(|k| k == "restore.begin"),
        "journal must gain NO restore.begin for unlabelled restore attempt"
    );
}

fn run_template_workbooks_surface() {
    let delivery = deliver_mixed_edition(&BuildInfo::default());
    let fixture = load_fixture();
    let non_public_canaries: Vec<&str> = fixture
        .record
        .iter()
        .filter(|r| r.label != "public")
        .map(|r| r.canary.as_str())
        .collect();

    for spec in fieldbook_render::templates_xlsx::TEMPLATES {
        let wb_path = delivery
            .edition_dir
            .join(format!("templates/{}.xlsx", spec.name));
        let wb_bytes =
            std::fs::read(&wb_path).unwrap_or_else(|e| panic!("read {}: {e}", wb_path.display()));
        let sheets = workbook_sheets(&wb_bytes);

        // (a) each workbook has exactly two sheets, named `Read me` and the data sheet
        let mut expected_sheet_names = ["Read me", spec.sheet].to_vec();
        expected_sheet_names.sort();
        let actual_names: Vec<&str> = sheets.keys().map(String::as_str).collect();
        assert_eq!(
            actual_names, expected_sheet_names,
            "sheet names mismatch for template `{}`",
            spec.name
        );

        // (b) the `Read me` sheet cells include the text `Synthetic example rows: replace them.`
        // and a cell containing `Save as CSV UTF-8`
        let readme_rows = sheets
            .get("Read me")
            .unwrap_or_else(|| panic!("missing `Read me` sheet in template `{}`", spec.name));
        let has_synthetic_rows_text = readme_rows.iter().any(|r| {
            r.iter()
                .any(|c| c.contains("Synthetic example rows: replace them."))
        });
        assert!(
            has_synthetic_rows_text,
            "`Read me` sheet in template `{}` must include `Synthetic example rows: replace them.`",
            spec.name
        );
        let has_save_as_csv = readme_rows
            .iter()
            .any(|r| r.iter().any(|c| c.contains("Save as CSV UTF-8")));
        assert!(
            has_save_as_csv,
            "`Read me` sheet in template `{}` must contain a cell with `Save as CSV UTF-8`",
            spec.name
        );

        // (c) the data sheet's header row equals the header row of the matching
        // `fieldbook_render::templates_xlsx::TEMPLATES` spec, and it has exactly 3 data rows,
        // each of which has a cell starting with `Synthetic example:` and a first cell starting
        // with `example-`
        let data_rows = sheets.get(spec.sheet).unwrap_or_else(|| {
            panic!(
                "missing data sheet `{}` in template `{}`",
                spec.sheet, spec.name
            )
        });
        assert!(
            !data_rows.is_empty(),
            "data sheet `{}` in template `{}` has no header row",
            spec.sheet,
            spec.name
        );
        let expected_header: Vec<&str> = spec.columns.iter().map(|col| col.header).collect();
        let actual_header: Vec<&str> = data_rows[0].iter().map(String::as_str).collect();
        assert_eq!(
            actual_header, expected_header,
            "header mismatch for data sheet `{}` in template `{}`",
            spec.sheet, spec.name
        );

        let data_rows_slice = &data_rows[1..];
        assert_eq!(
            data_rows_slice.len(),
            3,
            "data sheet `{}` in template `{}` must have exactly 3 data rows",
            spec.sheet,
            spec.name
        );

        for (row_idx, row) in data_rows_slice.iter().enumerate() {
            let first_cell = row.first().unwrap_or_else(|| {
                panic!(
                    "empty data row {row_idx} in `{}` in template `{}`",
                    spec.sheet, spec.name
                )
            });
            assert!(
                first_cell.starts_with("example-"),
                "first cell of data row {row_idx} in `{}` in template `{}` must start with `example-`, got `{first_cell}`",
                spec.sheet,
                spec.name
            );
            let has_synthetic_example = row
                .iter()
                .any(|cell| cell.starts_with("Synthetic example:"));
            assert!(
                has_synthetic_example,
                "data row {row_idx} in `{}` in template `{}` must have a cell starting with `Synthetic example:`",
                spec.sheet, spec.name
            );
        }

        // (d) no cell of any of the three workbooks contains any restricted (non-`public` label)
        // canary from the fixture or any string of FORBIDDEN_REGISTER_STRINGS
        for (sheet_name, rows) in &sheets {
            for (r_idx, row) in rows.iter().enumerate() {
                for (c_idx, cell) in row.iter().enumerate() {
                    for forbidden in FORBIDDEN_REGISTER_STRINGS {
                        assert!(
                            !cell.contains(forbidden),
                            "template `{}` sheet `{sheet_name}` row {r_idx} col {c_idx} contains forbidden `{forbidden}`: `{cell}`",
                            spec.name
                        );
                    }
                    for canary in &non_public_canaries {
                        assert!(
                            !cell.contains(canary),
                            "template `{}` sheet `{sheet_name}` row {r_idx} col {c_idx} contains non-public canary `{canary}`: `{cell}`",
                            spec.name
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn output_surfaces_cover_built_sinks() {
    let mut surface_names = Vec::new();
    let mut unique = BTreeSet::new();
    for s in SURFACES {
        assert!(
            unique.insert(s.name),
            "duplicate surface row `{}` in SURFACES",
            s.name
        );
        surface_names.push(s.name);
    }
    assert_eq!(
        surface_names, BUILT_SINKS,
        "SURFACES must match BUILT_SINKS inventory"
    );
}

#[test]
fn real_outputs_keep_permitted_canaries_and_refuse_others() {
    for surface in SURFACES {
        (surface.run)();
    }
}

#[test]
fn journal_holds_ids_and_no_canary() {
    run_journal_holds_ids_and_no_canary();
}

#[test]
fn delivered_binder_filters_articles() {
    run_day_one_binder_surface();
}

#[test]
fn delivered_workbook_and_csvs_filter_rows() {
    run_registers_workbook_surface();
    run_register_csvs_surface();
}

#[test]
fn delivered_templates_are_public_and_complete() {
    run_template_workbooks_surface();
}

fn desk_add_report_json(labels: &str, file_canary: &str, source_id: &str) -> String {
    format!(
        concat!(
            "{{",
            "\"report_version\":1,\"verb\":\"add\",\"dir\":\"incoming\",",
            "\"labels\":{labels},",
            "\"inputs\":[{{\"store\":\"corpus\",\"id\":\"{source_id}\"}}],",
            "\"files\":[{{\"name\":\"{file_canary}\",\"result\":\"added\",\"id\":\"{source_id}\",",
            "\"sha256\":\"{sha}\",\"bytes\":42,\"needs_extraction\":false}}],",
            "\"counts\":{{\"added\":1,\"present\":0,\"refused\":0,\"needs_extraction\":0}},",
            "\"exit\":0",
            "}}"
        ),
        labels = labels,
        file_canary = file_canary,
        source_id = source_id,
        sha = "1".repeat(64),
    )
}

fn run_desk_pages_surface() {
    const PERMITTED_DESK_CANARY: &str = "Permitted desk import file canary.txt";
    const FORBIDDEN_DESK_CANARY: &str = "Forbidden desk import file canary.txt";

    // 1. Forbidden non-State label (`np-0007`) on a fresh state folder is refused before writing.
    let fresh_tmp = temp_copy(&repo_root_from_manifest().join("tests/fixtures/corpus-mini"));
    let fresh_state = fresh_tmp.path().join("state");
    std::fs::create_dir_all(fresh_state.join("reports")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(fresh_state.join("settings.toml"), "role = \"lead\"\n")
        .unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        fresh_state.join("reports/last-add.json"),
        desk_add_report_json(
            "[\"public\",\"np-0007\"]",
            FORBIDDEN_DESK_CANARY,
            "S-0001-v1",
        ),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let fresh_world = TestWorld::with_state(fresh_tmp.path(), &fresh_state);
    let refused_fresh = fieldbook::run(&["status".to_owned(), "--page".to_owned()], &fresh_world);
    assert_eq!(refused_fresh.code, 3, "stderr: {}", refused_fresh.stderr);
    assert!(refused_fresh.stderr.contains("FB0412"));
    assert!(
        !fresh_state.join("pages").exists(),
        "refused desk write must not create state/pages"
    );

    // 2. Permitted State label (`owner-supplied` + `public` input) writes `pages/import.html`
    //    containing the permitted canary and keeps journal segments free of file-name canaries.
    let tmp = temp_copy(&repo_root_from_manifest().join("tests/fixtures/corpus-mini"));
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(state_dir.join("reports")).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(state_dir.join("settings.toml"), "role = \"lead\"\n")
        .unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(
        state_dir.join("reports/last-add.json"),
        desk_add_report_json("[\"owner-supplied\"]", PERMITTED_DESK_CANARY, "S-0001-v1"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let world = TestWorld::with_state(tmp.path(), &state_dir);
    let ok = fieldbook::run(&["status".to_owned(), "--page".to_owned()], &world);
    assert_eq!(ok.code, 0, "stderr: {}", ok.stderr);
    let import_html = std::fs::read_to_string(state_dir.join("pages/import.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(import_html.contains(PERMITTED_DESK_CANARY));
    assert!(!import_html.contains(FORBIDDEN_DESK_CANARY));
    let audit_dir = state_dir.join("audit");
    for entry in std::fs::read_dir(&audit_dir).unwrap_or_else(|e| panic!("{e}")) {
        let path = entry.unwrap_or_else(|e| panic!("{e}")).path();
        if path.extension().is_some_and(|ext| ext == "jsonl") {
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{e}"));
            assert!(
                !text.contains(PERMITTED_DESK_CANARY) && !text.contains(FORBIDDEN_DESK_CANARY),
                "journal segment must hold no desk canary: {path:?}"
            );
        }
    }

    // 3. Input provenance re-evaluation: relabelling S-0001 to `public;np-0007` between the
    //    command's initial read and its exclusive write lease refuses and preserves `import.html`.
    std::fs::write(
        state_dir.join("reports/last-add.json"),
        desk_add_report_json("[\"public\"]", FORBIDDEN_DESK_CANARY, "S-0001-v1"),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let sources_csv = tmp.path().join("corpus/registers/sources.csv");
    world.set_before_write_lease(move || {
        let csv = std::fs::read_to_string(&sources_csv).unwrap_or_else(|e| panic!("{e}"));
        let updated = csv
            .lines()
            .map(|line| {
                if line.starts_with("S-0001,") {
                    line.replacen(",public,", ",public|np-0007,", 1)
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&sources_csv, format!("{updated}\n")).unwrap_or_else(|e| panic!("{e}"));
    });
    let relabelled = fieldbook::run(&["status".to_owned(), "--page".to_owned()], &world);
    assert_eq!(relabelled.code, 3, "stderr: {}", relabelled.stderr);
    assert!(relabelled.stderr.contains("FB0412"));
    let preserved = std::fs::read_to_string(state_dir.join("pages/import.html"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(preserved, import_html);
    assert!(!preserved.contains(FORBIDDEN_DESK_CANARY));

    // 4. Negative sink plant: `StateScope` (`write_file`) refuses any path outside `state/`.
    let lease = world.write_lease().unwrap_or_else(|e| panic!("{e}"));
    let journal = state::open(&world, &state_dir, Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .state_scope(&lease)
        .unwrap_or_else(|e| panic!("{e}"));
    let outside = tmp.path().join("outside-desk.html");
    let Err(err) = world.write_file(&audited, &outside, FORBIDDEN_DESK_CANARY.as_bytes()) else {
        panic!("StateScope must refuse writes outside state_dir");
    };
    assert!(
        matches!(err, Error::OutOfScope(_)),
        "expected OutOfScope, got: {err:?}"
    );
    assert!(!outside.exists());
}

fn repo_root_from_manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn delivered_desk_pages_admit_and_bind() {
    run_desk_pages_surface();
}

#[test]
fn delivered_workbook_keeps_full_epoch() {
    let epoch = 1_772_366_400;
    let date = CivilDate::from_epoch_seconds(epoch).unwrap_or_else(|e| panic!("{e}"));
    let info = BuildInfo::new(Some(date), None, BuildProfile::Silent, Some(epoch));
    assert!(!info.is_released());

    let delivery = deliver_mixed_edition(&info);
    let toml_str = std::fs::read_to_string(delivery.edition_dir.join("edition.toml"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        toml_str.contains("edition = \"unreleased\""),
        "delivered edition.toml must say `edition = \"unreleased\"`, got:\n{toml_str}"
    );

    let wb_bytes = std::fs::read(delivery.edition_dir.join(fieldbook_core::strings::PATHS[0]))
        .unwrap_or_else(|e| panic!("{e}"));
    let core = core_xml(&wb_bytes);
    assert!(
        core.contains(
            "<dcterms:created xsi:type=\"dcterms:W3CDTF\">2026-03-01T12:00:00Z</dcterms:created>"
        ),
        "docProps/core.xml must contain full epoch instant, got:\n{core}"
    );

    let default_info = BuildInfo::default();
    assert!(!default_info.is_released());
    let default_delivery = deliver_mixed_edition(&default_info);

    let default_toml = std::fs::read_to_string(default_delivery.edition_dir.join("edition.toml"))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        default_toml.contains("edition = \"unreleased\""),
        "delivered default edition.toml must say `edition = \"unreleased\"`, got:\n{default_toml}"
    );

    let default_wb = std::fs::read(
        default_delivery
            .edition_dir
            .join(fieldbook_core::strings::PATHS[0]),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let default_core = core_xml(&default_wb);
    assert!(
        default_core.contains("2000-01-01T00:00:00Z"),
        "default docProps/core.xml must contain 2000-01-01T00:00:00Z, got:\n{default_core}"
    );
}

fn check_delivered_map_is_public_and_bound() {
    const SYNTHETIC_CANARY: &str = "<!-- synthetic-app-map-canary: made up for tests -->";
    const FORBIDDEN_BRIEF_TITLE: &str = "Synthetic forbidden map brief canary";

    let fixture = load_fixture();
    let tmp = temp_copy(&corpus_mini());
    let world = TestWorld::new(tmp.path());
    let out_tmp = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));

    let mut tree = world.read_tree().unwrap_or_else(|e| panic!("{e}"));
    let forbidden_brief = format!(
        "+++\nid = \"est-forbidden-map-brief\"\ntitle = \"{FORBIDDEN_BRIEF_TITLE}\"\narea = \"est\"\nsubarea = \"pricing\"\ntype = \"brief\"\nstage = 3\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"public\", \"np-0007\"]\nterms = []\nquestions = []\nvolatility = \"V2\"\n+++\n\n## Question\n\nSynthetic brief question?\n\n## Recommendation\n\nSynthetic recommendation.\n\n## Evidence\n\n- Synthetic brief evidence.\n\n## Options\n\n- Synthetic option.\n\n## Unknowns\n\n- Synthetic unknown.\n\n## Next action\n\nSynthetic next action.\n"
    );
    tree.insert_bytes(
        "corpus/pages/estimating/pricing/est-forbidden-map-brief.md",
        forbidden_brief.into_bytes(),
    );

    let (model, _report) = load(&Host::new(tree, world.today()), &Options::new(false));
    let eff = EffectivePolicy::floor(false);
    let now = default_now();
    let shipped = Shipped::Hours(1);
    let Decision::Allowed(sel_token) = check(&eff, &Dest::Edition, None, None, now, shipped) else {
        panic!("expected edition selection Allowed");
    };
    let view = LabelledView::new(&model, &sel_token, &ReadableStores::corpus());

    let mut plan = plan_edition(
        &view,
        &BuildInfo::default(),
        out_tmp.path(),
        &eff,
        &LabelSet::public(),
        now,
        shipped,
    )
    .unwrap_or_else(|e| panic!("plan_edition failed: {e:?}"))
    .unwrap_or_else(|d| panic!("plan_edition denied: {d:?}"));

    let map_path = map::OUTPUT_PATH;
    assert!(
        plan.site.files().contains_key(map_path),
        "plan_edition must include {map_path}"
    );

    let mut journal =
        state::open(&world, world.state_dir(), Via::Cli).unwrap_or_else(|e| panic!("{e}"));
    let audited = journal
        .begin_export("build", &plan.write_set, &plan.allowed, &plan.out_dir)
        .unwrap_or_else(|e| panic!("{e}"));

    let orig_site = plan.site.clone();
    let mut canaries = vec![("synthetic", SYNTHETIC_CANARY)];
    for rec in &fixture.record {
        if rec.label != "public" {
            canaries.push((rec.label.as_str(), rec.canary.as_str()));
        }
    }
    for (label, canary) in canaries {
        let mut mutated = Site::new();
        for (rel, bytes) in orig_site.files() {
            if rel == map_path {
                let mut planted = bytes.clone();
                planted.extend_from_slice(canary.as_bytes());
                mutated.insert(rel, planted);
            } else {
                mutated.insert(rel, bytes.clone());
            }
        }
        plan.site = mutated;
        let res = write_edition_outputs(&plan, &audited);
        assert!(
            matches!(res, Err(Error::OutOfScope(_))),
            "a canary for `{label}` planted in {map_path} must be refused at write_edition_outputs"
        );
    }
    plan.site = orig_site;
    assert!(
        !plan.edition_dir.join(map_path).exists(),
        "a refused write must not leave {map_path} on disk"
    );

    write_edition_outputs(&plan, &audited).unwrap_or_else(|e| panic!("{e}"));
    journal
        .end(audited, Word::new_static("ok"))
        .unwrap_or_else(|e| panic!("{e}"));

    let delivered_bytes =
        std::fs::read(plan.edition_dir.join(map_path)).unwrap_or_else(|e| panic!("{e}"));
    let planned_bytes = plan
        .site
        .files()
        .get(map_path)
        .unwrap_or_else(|| panic!("missing planned {map_path}"));
    assert_eq!(
        &delivered_bytes, planned_bytes,
        "delivered {map_path} must equal the planned bytes"
    );

    let delivered = String::from_utf8(delivered_bytes).unwrap_or_else(|e| panic!("{e}"));
    assert!(delivered.contains("<h1>How Fieldbook fits together</h1>"));
    assert!(
        !delivered.contains(FORBIDDEN_BRIEF_TITLE)
            && !delivered.contains("est-forbidden-map-brief"),
        "forbidden page must be absent from delivered {map_path}"
    );
}

#[test]
fn delivered_map_is_public_and_bound() {
    check_delivered_map_is_public_and_bound();
}
