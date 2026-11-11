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
use fieldbook_render::meta::BuildInfo;
use fieldbook_render::site::Site;
use serde::Deserialize;

use support::temp_copy;
use support::world::TestWorld;

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
        "<dt>Originals</dt><dd class=\"mono\">1</dd>",
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
    assert!(index_disk.contains("class=\"not-available\""));
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
    let ids_disk = std::fs::read_to_string(plan.edition_dir.join("ids.html"))
        .unwrap_or_else(|e| panic!("{e}"));

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
        "permitted IDs across all 7 groups must reach ids.html"
    );

    // The delivered search index holds the permitted page and section units only.
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
            .all(|d| d[1] == "page" || d[1] == "clause"),
        "assets/search-index.js holds page and section units only"
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
