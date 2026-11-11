//! Page context pack tests (`page-context-packs` step 7, controls `HG-01` and `HG-02`).
//!
//! The fixture is `tests/fixtures/corpus-mini` plus synthetic records whose `handling` is
//! `np-0007`, built under the synthetic policy `tests/fixtures/policy/policy.toml`.

#![expect(clippy::unwrap_used, reason = "integration test helpers")]
#![expect(
    clippy::disallowed_methods,
    reason = "tests read the corpus-mini fixture"
)]

mod pack {
    use std::path::Path;

    use fieldbook_core::assist::{ContextPack, PackBlock, RELATED_LIMIT, page_pack};
    use fieldbook_core::audit::Now;
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::ids::PageId;
    use fieldbook_core::labels::{LabelAllow, LabelId, OWNER_SUPPLIED, PUBLIC};
    use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};

    const FIXTURE_POLICY: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
    const TODAY_RAW: &str = include_str!("../../../tests/fixtures/corpus-mini/TODAY");
    const ASSISTANT_MD: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/ASSISTANT.md");

    /// The default budget of `[assistant] pack_budget_chars` (`HG-02`).
    const BUDGET: usize = 12_000;

    /// The page under test and its title.
    const PAGE: &str = "cmp-system-review";
    const TITLE: &str = "Synthetic business-system review and withholding overview";

    /// A page that cites one claim of a conflict and nothing else.
    const SINGLE_PAGE: &str = "cmp-single-cite";

    /// Markers carried only by the `np-0007` records.
    const MARKER_PAGE: &str = "zzmarkerpagenp7";
    const MARKER_SOURCE: &str = "zzmarkersourcenp7";
    const MARKER_CLAIM: &str = "zzmarkerclaimnp7";
    /// The marker of the held version's text.
    const MARKER_HELD: &str = "zzmarkerheldtext";

    fn lid(s: &str) -> LabelId {
        LabelId::parse(s).unwrap()
    }

    fn today() -> CivilDate {
        CivilDate::parse(TODAY_RAW.trim()).unwrap()
    }

    /// The fixed `Now`: the fixture's `TODAY` at 43 200 seconds.
    fn now() -> Now {
        Now::new(today(), 43_200).unwrap()
    }

    fn insert_all(tree: &mut Tree, dir: &Path, rel: &str) {
        tree.insert_dir(rel);
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            let child = format!("{rel}/{name}");
            if entry.file_type().unwrap().is_dir() {
                insert_all(tree, &entry.path(), &child);
            } else {
                tree.insert_bytes(child, std::fs::read(entry.path()).unwrap());
            }
        }
    }

    /// One CSV row for `header`, with `cells` and every other column empty.
    fn csv_row(header: &str, cells: &[(&str, &str)]) -> String {
        header
            .split(',')
            .map(|col| {
                let value = cells
                    .iter()
                    .find(|(c, _)| *c == col)
                    .map_or("", |(_, v)| *v);
                if value.contains(',') {
                    format!("\"{value}\"")
                } else {
                    value.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join(",")
    }

    /// A concept page file with the given id, title, handling and Purpose text.
    fn concept_page(id: &str, title: &str, handling: &str, purpose: &str, cite: &str) -> String {
        format!(
            "+++\nid = \"{id}\"\ntitle = \"{title}\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [\"{handling}\"]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n## Purpose\n\n{purpose}\n\n## The rule\n\nSynthetic rule text {cite}.\n\n## How it works\n\nSynthetic mechanism.\n\n## Worked example\n\nSynthetic example. The numbers are made up.\n\nSynthetic walk-through.\n\n## Never confuse\n\nSynthetic confusion.\n\n## Company calibration\n\nSynthetic calibration.\n\n## Sources\n\n## Change note\n\n- NEW_KNOWLEDGE, 2026-02-01: Synthetic.\n"
        )
    }

    /// Which synthetic `np-0007` records the fixture holds.
    #[derive(Clone, Copy)]
    struct Plant {
        np_page: bool,
        np_source: bool,
        np_claim: bool,
        held_version: bool,
    }

    const PLAIN: Plant = Plant {
        np_page: false,
        np_source: false,
        np_claim: false,
        held_version: false,
    };

    fn fixture_tree(plant: Plant) -> Tree {
        let corpus =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/corpus-mini/corpus");
        let mut tree = Tree::new();
        insert_all(&mut tree, &corpus, "corpus");

        // Both claims name `XC-01` as their conflict, and the opposing one is contradicted, so
        // a pack excludes it by default.
        let claims = std::fs::read_to_string(corpus.join("registers/claims.csv")).unwrap();
        let claims_header = claims.lines().next().unwrap().to_owned();
        let mut claims_out: Vec<String> = claims
            .lines()
            .map(|line| {
                if line.starts_with("CL-CMP-0001,") {
                    line.replace(
                        ",owner,,,,,Synthetic owner",
                        ",owner,,,XC-01,,Synthetic owner",
                    )
                } else if line.starts_with("CL-CMP-0002,") {
                    line.replace(
                        ",unchecked,pending_company_confirmation,",
                        ",contradicted,pending_company_confirmation,",
                    )
                    .replace(
                        ",,Q-001,Synthetic unchecked",
                        ",XC-01,Q-001,Synthetic unchecked",
                    )
                } else {
                    line.to_owned()
                }
            })
            .collect();

        let single = concept_page(
            SINGLE_PAGE,
            "Lone claim cite probe",
            "public",
            "Synthetic page that relies on one claim only.",
            "[@CL-CMP-0001]",
        );
        tree.insert_bytes(
            format!("corpus/pages/compliance/business-systems/{SINGLE_PAGE}.md"),
            single.into_bytes(),
        );

        let sources = std::fs::read_to_string(corpus.join("registers/sources.csv")).unwrap();
        let sources_header = sources.lines().next().unwrap().to_owned();
        let mut sources_out: Vec<String> = sources.lines().map(str::to_owned).collect();

        if plant.np_page {
            let page = concept_page(
                "cmp-np-canary",
                TITLE,
                "np-0007",
                &format!("Restricted text {MARKER_PAGE}."),
                "[@CL-CMP-0001]",
            );
            tree.insert_bytes(
                "corpus/pages/compliance/business-systems/cmp-np-canary.md",
                page.into_bytes(),
            );
        }
        if plant.np_source || plant.np_claim {
            sources_out.push(csv_row(
                &sources_header,
                &[
                    ("source_id", "S-0006"),
                    ("version", "1"),
                    ("tag", "synthetic-np"),
                    ("title", &format!("{TITLE} {MARKER_SOURCE}")),
                    ("publisher", "Synthetic Board"),
                    ("source_kind", "primary_law"),
                    ("source_role", "primary_evidence"),
                    ("authority_tier", "A"),
                    ("licence", "Synthetic, no restrictions"),
                    ("location", "metadata-only: made up for tests"),
                    ("sha256", &"6".repeat(64)),
                    ("as_of", "2026-02-01"),
                    ("retrieved", "2026-02-01"),
                    ("lifecycle", "active"),
                    ("volatility", "V2"),
                    ("handling", "np-0007"),
                    ("note", "Restricted fixture note."),
                    ("purpose", "Synthetic purpose for tests"),
                ],
            ));
        }
        if plant.np_claim {
            claims_out.push(csv_row(
                &claims_header,
                &[
                    ("claim_id", "CL-CMP-0003"),
                    ("area", "cmp"),
                    ("kind", "fact"),
                    (
                        "statement",
                        &format!("Synthetic restricted statement {MARKER_CLAIM}."),
                    ),
                    ("as_of", "2026-02-01"),
                    ("source_id", "S-0006"),
                    ("version", "1"),
                    ("locator_kind", "text_section"),
                    ("locator", "sec. 1"),
                    ("quote", "Synthetic restricted quote."),
                    ("consequential", "false"),
                    ("drafted_by", "codex"),
                    ("verification", "unchecked"),
                    ("applicability", "pending_company_confirmation"),
                    ("adoption", "proposed"),
                    ("data_origin", "observed"),
                    ("volatility", "V2"),
                ],
            ));
        }
        if plant.held_version {
            // `S-0002` version 2 is held; its text opens with the page's title, so an index read
            // from the raw tree would rank it first.
            for line in &mut sources_out {
                if line.starts_with("S-0002,2,") {
                    *line = line.replace(",active,S-0002-v1,", ",quarantined,S-0002-v1,");
                }
            }
            tree.insert_bytes(
                "corpus/sources/public/S-0002-v2.txt",
                format!("{TITLE} {MARKER_HELD}\n").into_bytes(),
            );
        }
        tree.insert_bytes(
            "corpus/registers/claims.csv",
            format!("{}\n", claims_out.join("\n")).into_bytes(),
        );
        tree.insert_bytes(
            "corpus/registers/sources.csv",
            format!("{}\n", sources_out.join("\n")).into_bytes(),
        );
        tree
    }

    fn load_model(plant: Plant) -> Model {
        load(
            &Host::new(fixture_tree(plant), today()),
            &Options::new(false),
        )
        .0
    }

    fn effective() -> EffectivePolicy {
        let policy = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let mut owner = OwnerSettings::unset();
        owner.endpoints.insert(
            "approved-gateway".to_owned(),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED, lid("bid-0003")]),
        );
        EffectivePolicy::new(&PolicyState::Loaded(policy), &owner)
    }

    fn model_dest() -> Dest {
        Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        }
    }

    /// The selection view for `dest`, narrowed by `permissions` when given.
    fn view_for(model: &Model, dest: &Dest, permissions: Option<&[LabelId]>) -> LabelledView {
        let Decision::Allowed(token) =
            check(&effective(), dest, None, None, now(), Shipped::Hours(1))
        else {
            panic!("selection for {dest:?} must be allowed");
        };
        let view = LabelledView::new(model, &token, &ReadableStores::corpus());
        match permissions {
            Some(ids) => view.restrict(&LabelAllow::from_ids(ids.iter().cloned())),
            None => view,
        }
    }

    fn pack_of(view: &LabelledView, dest: &Dest, page: &str, budget: usize) -> ContextPack {
        page_pack(view, dest, &PageId::parse(page).unwrap(), budget)
    }

    fn last_line(text: &str) -> &str {
        text.trim_end_matches('\n').lines().last().unwrap()
    }

    fn block_ids(blocks: &[PackBlock]) -> Vec<String> {
        blocks.iter().map(|b| b.id.clone()).collect()
    }

    /// Ids of the blocks of `pack` whose text holds `marker`.
    fn blocks_holding<'a>(pack: &'a ContextPack, marker: &str) -> Vec<&'a str> {
        pack.blocks()
            .iter()
            .filter(|b| b.text.contains(marker))
            .map(|b| b.id.as_str())
            .collect()
    }

    // -------------------------------------------------------------------------
    // The label rule
    // -------------------------------------------------------------------------

    #[test]
    fn a_marker_outside_the_allowed_set_never_reaches_a_pack() {
        let model = load_model(Plant {
            np_page: true,
            np_source: true,
            ..PLAIN
        });
        let view = view_for(&model, &Dest::Handoff, Some(&[PUBLIC]));
        let pack = pack_of(&view, &Dest::Handoff, PAGE, BUDGET);
        let text = pack.to_text();

        for marker in [MARKER_PAGE, MARKER_SOURCE] {
            let leaked = blocks_holding(&pack, marker);
            assert!(
                leaked.is_empty(),
                "the marker `{marker}` of an `np-0007` record reached blocks {leaked:?}"
            );
            assert!(!text.contains(marker), "marker `{marker}` is in the text");
        }
        assert_eq!(last_line(&text), "Left out (policy): 2 records");
    }

    #[test]
    fn a_record_inside_the_allowed_set_is_present() {
        let model = load_model(Plant {
            np_page: true,
            np_source: true,
            ..PLAIN
        });
        let view = view_for(&model, &Dest::Handoff, Some(&[PUBLIC, lid("np-0007")]));
        let pack = pack_of(&view, &Dest::Handoff, PAGE, BUDGET);
        let text = pack.to_text();

        assert!(
            text.contains(MARKER_PAGE) || text.contains(MARKER_SOURCE),
            "an allowed `np-0007` record is missing from the pack:\n{text}"
        );
        assert_eq!(last_line(&text), "Left out (policy): 0 records");
        assert!(
            text.lines()
                .any(|l| l.starts_with("Labels: ") && l.contains("np-0007")),
            "the Labels line names the label of an included record:\n{text}"
        );
    }

    // control: HG-01
    #[test]
    fn a_model_pack_ends_with_the_fixed_line_and_no_digit() {
        let plain = load_model(PLAIN);
        let restricted = load_model(Plant {
            np_page: true,
            np_source: true,
            np_claim: true,
            ..PLAIN
        });
        let model = model_dest();
        for dest in [&model, &Dest::Edition] {
            let mut lines = Vec::new();
            for (name, m, want) in [("0 left out", &plain, 0), ("3 left out", &restricted, 3)] {
                let view = view_for(m, dest, None);
                assert_eq!(view.left_out(), want, "{name} for {dest:?}");
                let text = pack_of(&view, dest, PAGE, BUDGET).to_text();
                let last = last_line(&text).to_owned();
                assert_eq!(last, "Some records were omitted.", "{name} for {dest:?}");
                assert!(
                    !last.chars().any(|c| c.is_ascii_digit()),
                    "a digit in the last line for {dest:?}: {last}"
                );
                lines.push(last);
            }
            assert_eq!(lines[0], lines[1]);
        }

        // A destination the owner reads shows the count.
        let view = view_for(&restricted, &Dest::Handoff, Some(&[PUBLIC]));
        let text = pack_of(&view, &Dest::Handoff, PAGE, BUDGET).to_text();
        assert_eq!(last_line(&text), "Left out (policy): 3 records");
    }

    // -------------------------------------------------------------------------
    // Shape of the text
    // -------------------------------------------------------------------------

    #[test]
    fn assistant_md_comes_first_then_the_labels_line() {
        let model = load_model(PLAIN);
        let view = view_for(&model, &Dest::Edition, None);
        let text = pack_of(&view, &Dest::Edition, PAGE, BUDGET).to_text();

        assert!(
            text.starts_with(ASSISTANT_MD),
            "ASSISTANT.md is first, verbatim"
        );
        let rest = &text[ASSISTANT_MD.len()..];
        assert!(
            rest.starts_with("\nLabels: public\n\n--- page cmp-system-review ---\n"),
            "a blank line, the Labels line, a blank line, then the page block:\n{rest}"
        );
        assert!(text.ends_with("Some records were omitted.\n"));
        assert!(!text.ends_with("\n\n"));
        assert!(!text.contains('\r'), "LF only");
    }

    #[test]
    fn every_claim_block_has_a_cite_as_line() {
        let model = load_model(PLAIN);
        let view = view_for(&model, &Dest::Edition, None);
        let pack = pack_of(&view, &Dest::Edition, PAGE, BUDGET);

        let claims: Vec<&PackBlock> = pack.blocks().iter().filter(|b| b.kind == "claim").collect();
        assert!(claims.len() >= 2, "the page cites two claims");
        for block in claims {
            let want = format!("\ncite as: [@{}]", block.id);
            assert!(
                block.text.contains(&want),
                "claim block {} has no cite as line:\n{}",
                block.id,
                block.text
            );
        }
    }

    #[test]
    fn the_page_and_its_cited_claims_come_first() {
        let model = load_model(PLAIN);
        let view = view_for(&model, &Dest::Edition, None);
        let pack = pack_of(&view, &Dest::Edition, PAGE, BUDGET);
        let ids = block_ids(pack.blocks());

        // `CL-CMP-0001` brings its conflict row and its opponent, which is the second cited claim.
        assert_eq!(&ids[..4], [PAGE, "CL-CMP-0001", "XC-01", "CL-CMP-0002"]);
        let related = ids.len() - 4;
        assert!(
            related <= RELATED_LIMIT + 1,
            "at most the top {RELATED_LIMIT} related units"
        );
        let unique: std::collections::BTreeSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len(), "blocks are de-duplicated: {ids:?}");
    }

    #[test]
    fn two_runs_give_equal_text() {
        let model = load_model(PLAIN);
        let view = view_for(&model, &Dest::Edition, None);
        let first = pack_of(&view, &Dest::Edition, PAGE, BUDGET).to_text();
        let second = pack_of(
            &view_for(&load_model(PLAIN), &Dest::Edition, None),
            &Dest::Edition,
            PAGE,
            BUDGET,
        )
        .to_text();
        assert_eq!(first.as_bytes(), second.as_bytes());
    }

    // -------------------------------------------------------------------------
    // The budget and the closure
    // -------------------------------------------------------------------------

    #[test]
    fn a_small_budget_drops_whole_blocks_and_lists_them() {
        let model = load_model(PLAIN);
        let view = view_for(&model, &Dest::Edition, None);
        let full = pack_of(&view, &Dest::Edition, PAGE, BUDGET);
        let everything = block_ids(full.blocks());
        assert!(full.left_out_budget().is_empty());
        assert!(!full.to_text().contains("Left out (budget)"));

        // Room for the preamble, the page and a little more.
        let budget = ASSISTANT_MD.chars().count() + full.blocks()[0].text.chars().count() + 40;
        let small = pack_of(&view, &Dest::Edition, PAGE, budget);
        assert!(small.blocks().len() < full.blocks().len());
        for block in small.blocks() {
            let original = full.blocks().iter().find(|b| b.id == block.id).unwrap();
            assert_eq!(block.text, original.text, "kept blocks are whole");
        }
        let mut seen = block_ids(small.blocks());
        seen.extend(block_ids(small.left_out_budget()));
        assert_eq!(seen, everything, "every block is kept or listed, in order");

        let text = small.to_text();
        let heading = text.find("\nLeft out (budget)\n").unwrap();
        let last = text.rfind("\nSome records were omitted.\n").unwrap();
        let first_dropped = &small.left_out_budget()[0];
        let entry = format!("- {} {}", first_dropped.kind, first_dropped.id);
        let at = text.find(&entry).unwrap();
        assert!(
            heading < at && at < last,
            "the list is before the last line"
        );
        let last_block = text.rfind("\n--- ").unwrap();
        assert!(last_block < heading, "the list follows the last block");
    }

    #[test]
    fn a_selected_claim_brings_its_conflict_and_opponent() {
        let model = load_model(PLAIN);
        let view = view_for(&model, &Dest::Edition, None);
        // The page cites only `CL-CMP-0001`; `CL-CMP-0002` opposes it in `XC-01` and is
        // `contradicted`, which a pack excludes by default.
        let pack = pack_of(&view, &Dest::Edition, SINGLE_PAGE, BUDGET);
        let ids = block_ids(pack.blocks());
        let claim = ids.iter().position(|i| i == "CL-CMP-0001").unwrap();
        assert_eq!(
            ids[claim + 1],
            "XC-01",
            "the conflict row follows its claim"
        );
        assert_eq!(ids[claim + 2], "CL-CMP-0002", "then the opposing claim");
        let opponent = &pack.blocks()[claim + 2];
        assert!(
            opponent
                .text
                .contains("Status: contradicted (conflict XC-01)"),
            "{}",
            opponent.text
        );
        assert!(opponent.text.contains("\ncite as: [@CL-CMP-0002]"));
    }

    #[test]
    fn over_budget_drops_claim_with_its_conflict() {
        let model = load_model(PLAIN);
        let view = view_for(&model, &Dest::Edition, None);
        let full = pack_of(&view, &Dest::Edition, SINGLE_PAGE, BUDGET);
        let size = |id: &str| {
            full.blocks()
                .iter()
                .find(|b| b.id == id)
                .map(|b| b.text.chars().count() + 2)
                .unwrap()
        };
        // Fits the preamble, the page and the claim, but not the conflict row and the opponent.
        let budget = ASSISTANT_MD.chars().count() + size(SINGLE_PAGE) + size("CL-CMP-0001");
        let pack = pack_of(&view, &Dest::Edition, SINGLE_PAGE, budget);

        assert_eq!(block_ids(pack.blocks()), [SINGLE_PAGE]);
        let dropped = block_ids(pack.left_out_budget());
        assert_eq!(&dropped[..3], ["CL-CMP-0001", "XC-01", "CL-CMP-0002"]);

        let text = pack.to_text();
        let last_block = text.rfind("\n--- ").unwrap();
        let heading = text.find("\nLeft out (budget)\n").unwrap();
        let claim = text
            .find("- claim CL-CMP-0001\n- conflict XC-01\n- claim CL-CMP-0002\n")
            .unwrap();
        let last = text.rfind("\nSome records were omitted.\n").unwrap();
        assert!(last_block < heading && heading < claim && claim < last);
        assert!(!text.contains("--- claim CL-CMP-0001 ---"));
    }

    // -------------------------------------------------------------------------
    // Held versions
    // -------------------------------------------------------------------------

    #[test]
    fn held_version_not_in_a_pack() {
        let model = load_model(Plant {
            held_version: true,
            ..PLAIN
        });
        let dests = [model_dest(), Dest::Edition, Dest::Handoff];
        for dest in &dests {
            let view = view_for(&model, dest, None);
            let pack = pack_of(&view, dest, PAGE, BUDGET);
            let leaked = blocks_holding(&pack, MARKER_HELD);
            assert!(
                leaked.is_empty(),
                "held text reached blocks {leaked:?} of a pack for {dest:?}"
            );
            assert!(!pack.to_text().contains(MARKER_HELD));
        }
    }
}
