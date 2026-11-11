//! Label-leak matrix (`core-policy` step 10, control `LB-01`).
//!
//! Runs the synthetic policy (`tests/fixtures/policy/policy.toml`) and planted canary
//! records (`tests/fixtures/leak/records.toml`) across every destination kind (`DEST_KINDS`),
//! asserting that each canary appears in the output view if and only if `egress::check`
//! allows the label.

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "integration test assertions"
)]

mod leak_matrix {
    use std::collections::BTreeSet;

    use fieldbook_core::assist::page_pack;
    use fieldbook_core::audit::{
        Actor, DestKind, EventEnvelope, Kind, Now, Principal, Role, Via, ZERO_HASH, sample_payload,
        seal_body,
    };
    use fieldbook_core::authz::{Groups, PolicyReaders, ReadableStores, readable};
    use fieldbook_core::check::{Code, Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{DEST_KINDS, Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::ids::PageId;
    use fieldbook_core::labels::{
        LabelAllow, LabelContext, LabelId, LabelSet, MarkingRules, OWNER_SUPPLIED, PUBLIC, StoreId,
        StoreRule, admit, for_text,
    };
    use fieldbook_core::policy::{EffectivePolicy, FloorKind, OwnerSettings, Policy, PolicyState};
    use fieldbook_core::schema::Register;
    use serde::Deserialize;

    const FIXTURE_POLICY: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
    const LEAK_RECORDS_TOML: &str = include_str!("../../../tests/fixtures/leak/records.toml");
    const MINI_CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    #[derive(Debug, Clone, Deserialize)]
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

    /// One output path in the pure label-leak matrix (`OUTPUT_PATHS`).
    pub struct OutputPath {
        /// Identifier of the output path.
        pub name: &'static str,
        /// Destination kind governed by this column.
        pub kind: DestKind,
        /// Pure rendering function from a [`LabelledView`] to text.
        pub render: fn(&LabelledView, &Dest) -> String,
    }

    fn render_view_text(view: &LabelledView, dest: &Dest) -> String {
        view.render_summary(dest)
    }

    /// The public page whose context pack the `page_pack_*` rows build, and the title that every
    /// planted page shares so that its pack ranks them all as related units.
    const PACK_PAGE: &str = "cmp-leak-fixture";
    const PACK_TITLE: &str = "Leak matrix fixture page";
    /// The default `[assistant] pack_budget_chars`.
    const PACK_BUDGET: usize = 12_000;

    fn render_page_pack_text(view: &LabelledView, dest: &Dest) -> String {
        page_pack(view, dest, &PageId::parse(PACK_PAGE).unwrap(), PACK_BUDGET).to_text()
    }

    /// Const table of output paths exercised by the label-leak matrix (`core-policy` step 10).
    pub const OUTPUT_PATHS: &[OutputPath] = &[
        OutputPath {
            name: "view:handoff",
            kind: DestKind::Handoff,
            render: render_view_text,
        },
        OutputPath {
            name: "view:model",
            kind: DestKind::Model,
            render: render_view_text,
        },
        OutputPath {
            name: "view:mcp",
            kind: DestKind::Mcp,
            render: render_view_text,
        },
        OutputPath {
            name: "view:bridge",
            kind: DestKind::Bridge,
            render: render_view_text,
        },
        OutputPath {
            name: "view:export",
            kind: DestKind::Export,
            render: render_view_text,
        },
        OutputPath {
            name: "view:backup",
            kind: DestKind::Backup,
            render: render_view_text,
        },
        OutputPath {
            name: "view:print",
            kind: DestKind::Print,
            render: render_view_text,
        },
        OutputPath {
            name: "view:clipboard",
            kind: DestKind::Clipboard,
            render: render_view_text,
        },
        OutputPath {
            name: "view:git",
            kind: DestKind::Git,
            render: render_view_text,
        },
        OutputPath {
            name: "view:edition",
            kind: DestKind::Edition,
            render: render_view_text,
        },
        OutputPath {
            name: "view:audit",
            kind: DestKind::Audit,
            render: render_view_text,
        },
        // The page context pack (`page-context-packs`): the public edition view that the site
        // build packs, and the handoff view that the same builder serves later.
        OutputPath {
            name: "page_pack_edition",
            kind: DestKind::Edition,
            render: render_page_pack_text,
        },
        OutputPath {
            name: "page_pack_handoff",
            kind: DestKind::Handoff,
            render: render_page_pack_text,
        },
    ];

    fn lid(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("valid label `{s}`: {e}"))
    }

    fn now_2026_11_01() -> Now {
        Now::new(CivilDate::parse("2026-11-01").unwrap(), 3600).unwrap()
    }

    fn fixture_records() -> Vec<PlantedRecord> {
        let parsed: LeakFixture =
            toml::from_str(LEAK_RECORDS_TOML).expect("valid tests/fixtures/leak/records.toml");
        parsed.record
    }

    fn dest_for_kind(kind: DestKind) -> Dest {
        match kind {
            DestKind::Handoff => Dest::Handoff,
            DestKind::Model => Dest::Model {
                name: "approved-gateway".to_owned(),
                base_url: "https://ai-gateway.example/v1".to_owned(),
            },
            DestKind::Mcp => Dest::Mcp {
                client: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_owned(),
            },
            DestKind::Bridge => Dest::Bridge,
            DestKind::Export => Dest::Export {
                path: "/srv/company/exports/out.txt".into(),
            },
            DestKind::Backup => Dest::Backup {
                path: "/srv/company/backups/snap".into(),
                company_storage: true,
            },
            DestKind::Print => Dest::Print,
            DestKind::Clipboard => Dest::Clipboard,
            DestKind::Git => Dest::Git,
            DestKind::Edition => Dest::Edition,
            DestKind::Audit => Dest::Audit,
        }
    }

    fn loaded_effective_policy() -> (Policy, EffectivePolicy) {
        let p = Policy::parse(FIXTURE_POLICY, None).expect("fixture policy parses");
        let mut owner = OwnerSettings::unset();
        owner.endpoints.insert(
            "approved-gateway".to_owned(),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED, lid("bid-0003")]),
        );
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p.clone()), &owner);
        (p, eff)
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

    /// A page that carries `canary` as its only text, under the title every pack row ranks.
    fn canary_page(id: &str, title: &str, handling: &str, canary: &str) -> (String, Vec<u8>) {
        let md = format!(
            "+++\nid = \"{id}\"\ntitle = \"{title}\"\narea = \"cmp\"\nsubarea = \"business-systems\"\ntype = \"concept\"\nstage = 1\nreadiness = \"SYNTHETIC-NOW\"\nstatus = \"draft\"\nas_of = 2026-02-01\nhandling = [{handling}]\nterms = []\nquestions = []\nconflicts = []\nvolatility = \"V2\"\n+++\n\n{canary}\n"
        );
        (
            format!("corpus/pages/compliance/business-systems/{id}.md"),
            md.into_bytes(),
        )
    }

    fn build_model_from_records(records: &[PlantedRecord], include_marked: bool) -> Model {
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

        // The page the pack rows build, and one page per planted label holding its canary.
        let (path, bytes) =
            canary_page(PACK_PAGE, PACK_TITLE, "\"public\"", "Public fixture text.");
        tree.insert_dir("corpus/pages/compliance");
        tree.insert_dir("corpus/pages/compliance/business-systems");
        tree.insert_bytes(path, bytes);
        for (idx, rec) in records.iter().enumerate() {
            if rec.label == "marked" {
                continue;
            }
            let id = format!("cmp-leak-{idx}");
            let (path, bytes) =
                canary_page(&id, PACK_TITLE, &format!("\"{}\"", rec.label), &rec.canary);
            tree.insert_bytes(path, bytes);
        }

        for rec in records {
            if rec.label == "marked" && !include_marked {
                continue;
            }
            let path = format!("{}/records/{}.toml", rec.store.as_str(), rec.id);
            let content = format!(
                "id = \"{}\"\nhandling = \"{}\"\ncanary = \"{}\"\n",
                rec.id, rec.label, rec.canary
            );
            tree.insert_bytes(path, content.into_bytes());
        }

        let today = CivilDate::parse("2026-03-01").unwrap();
        load(&Host::new(tree, today), &Options::new(false)).0
    }

    fn all_readable(policy: &Policy) -> ReadableStores {
        struct InAllGroups;
        impl Groups for InAllGroups {
            fn in_group(&self, _group: &str) -> Option<bool> {
                Some(true)
            }
        }
        let adapter = PolicyReaders::new(policy, &InAllGroups);
        let actor = Actor {
            principal: Principal::parse("local").unwrap(),
            role: Role::Owner,
            via: Via::Cli,
        };
        readable(
            &adapter,
            &actor,
            &[
                StoreId::Corpus,
                StoreId::Overlay,
                StoreId::Working,
                StoreId::State,
            ],
        )
    }

    #[test]
    fn leak_matrix_covers_every_label() {
        let (policy, _) = loaded_effective_policy();
        let records = fixture_records();

        // Every label of the fixture policy has a planted record, plus `public` and `marked`.
        for defined in policy.labels() {
            assert!(
                records.iter().any(|r| r.label == defined.id.as_str()),
                "missing planted record for policy label `{}`",
                defined.id
            );
        }
        assert!(
            records.iter().any(|r| r.label == "public"),
            "missing planted public record"
        );
        assert!(
            records.iter().any(|r| r.label == "marked"),
            "missing planted marked record"
        );

        // Every planted canary is unique and non-empty.
        let mut seen_canaries = BTreeSet::new();
        for rec in &records {
            assert!(!rec.canary.is_empty());
            assert!(
                seen_canaries.insert(rec.canary.clone()),
                "duplicate canary `{}`",
                rec.canary
            );
        }
    }

    #[test]
    fn the_matrix_has_a_column_for_every_dest_kind() {
        for spec in DEST_KINDS {
            assert!(
                OUTPUT_PATHS.iter().any(|p| p.kind == spec.kind),
                "OUTPUT_PATHS is missing a column for destination kind `{}`",
                spec.kind.as_str()
            );
        }
        // One `view:` column per kind, and the page pack rows beside them.
        assert_eq!(
            OUTPUT_PATHS
                .iter()
                .filter(|p| p.name.starts_with("view:"))
                .count(),
            DEST_KINDS.len()
        );
        assert!(OUTPUT_PATHS.iter().any(|p| p.name == "page_pack_edition"));
        assert!(OUTPUT_PATHS.iter().any(|p| p.name == "page_pack_handoff"));
        assert_eq!(OUTPUT_PATHS.len(), DEST_KINDS.len() + 2);
    }

    // control: LB-01
    #[test]
    fn canary_appears_only_where_allowed() {
        let (policy, eff) = loaded_effective_policy();
        let records = fixture_records();
        let model = build_model_from_records(&records, false);
        let stores = all_readable(&policy);

        // Include a mixed record (`public` + `ctl-0002`) so replacing the subset check by an overlap check fails.
        let model_with_mixed = {
            let mut tree = model.tree().clone();
            let (path, bytes) = canary_page(
                "cmp-leak-mixed",
                PACK_TITLE,
                "\"public\", \"ctl-0002\"",
                "CANARY_MIXED_PUBLIC_CTL_0002",
            );
            tree.insert_bytes(path, bytes);
            tree.insert_bytes(
                "overlay/records/fnd-leak-mixed-ctl.toml",
                b"id = \"fnd-leak-mixed-ctl\"\nhandling = [\"public\", \"ctl-0002\"]\ncanary = \"CANARY_MIXED_PUBLIC_CTL_0002\"\n",
            );
            load(
                &Host::new(tree, CivilDate::parse("2026-03-01").unwrap()),
                &Options::new(false),
            )
            .0
        };

        for path in OUTPUT_PATHS {
            let dest = dest_for_kind(path.kind);
            let sel = check(&eff, &dest, None, None, now_2026_11_01(), Shipped::Hours(1));
            let Decision::Allowed(sel_token) = sel else {
                panic!("expected selection Allowed for {:?}", path.kind);
            };
            let view = LabelledView::new(&model_with_mixed, &sel_token, &stores);
            let rendered = (path.render)(&view, &dest);

            for rec in records.iter().filter(|r| r.label != "marked") {
                let label_set = LabelSet::from_ids([lid(&rec.label)]);
                let decision = check(
                    &eff,
                    &dest,
                    Some(&label_set),
                    None,
                    now_2026_11_01(),
                    Shipped::Hours(1),
                );
                let allowed = decision.is_allowed();
                let present = rendered.contains(&rec.canary);
                assert_eq!(
                    present,
                    allowed,
                    "path `{}` (kind `{}`) for label `{}`: canary present={present}, check allowed={allowed}",
                    path.name,
                    path.kind.as_str(),
                    rec.label
                );
            }

            // Mixed `public` + `ctl-0002` must be denied everywhere `ctl-0002` is denied, and its canary must never appear.
            let mixed_set = LabelSet::from_ids([PUBLIC, lid("ctl-0002")]);
            let mixed_allowed = check(
                &eff,
                &dest,
                Some(&mixed_set),
                None,
                now_2026_11_01(),
                Shipped::Hours(1),
            )
            .is_allowed();
            assert!(!mixed_allowed);
            assert!(
                !rendered.contains("CANARY_MIXED_PUBLIC_CTL_0002"),
                "mixed public+ctl-0002 canary leaked on {}",
                path.name
            );
        }
    }

    #[test]
    fn a_label_allowed_nowhere_never_appears() {
        let (policy, eff) = loaded_effective_policy();
        let records = fixture_records();
        let ctl_rec = records
            .iter()
            .find(|r| r.label == "ctl-0002")
            .expect("ctl-0002 record");
        let model = build_model_from_records(&records, false);
        let stores = all_readable(&policy);

        for path in OUTPUT_PATHS {
            let dest = dest_for_kind(path.kind);
            let dec = check(
                &eff,
                &dest,
                Some(&LabelSet::from_ids([lid("ctl-0002")])),
                None,
                now_2026_11_01(),
                Shipped::Hours(1),
            );
            assert!(
                dec.is_denied(),
                "ctl-0002 must be denied on {:?}",
                path.kind
            );

            let Decision::Allowed(sel_token) =
                check(&eff, &dest, None, None, now_2026_11_01(), Shipped::Hours(1))
            else {
                panic!("expected selection token");
            };
            let view = LabelledView::new(&model, &sel_token, &stores);
            let rendered = (path.render)(&view, &dest);
            assert!(
                !rendered.contains(&ctl_rec.canary),
                "ctl-0002 canary appeared on `{}`",
                path.name
            );
        }
    }

    #[test]
    fn the_reply_to_a_tool_client_is_identical_with_0_and_3_restricted_records() {
        let (policy, eff) = loaded_effective_policy();
        let records = fixture_records();
        let public_only: Vec<PlantedRecord> = records
            .iter()
            .filter(|r| r.label == "public")
            .cloned()
            .collect();
        // 0 restricted records vs 3 restricted company records (`np-0007`, `bid-0003`, `ctl-0002`).
        let model_0 = build_model_from_records(&public_only, false);
        let model_3 = build_model_from_records(&records, false);
        let stores = all_readable(&policy);

        // 1. `mcp` reply is byte-for-byte equal with 0 and 3 restricted records.
        let mcp_dest = dest_for_kind(DestKind::Mcp);
        let Decision::Allowed(mcp_tok_0) = check(
            &eff,
            &mcp_dest,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let Decision::Allowed(mcp_tok_3) = check(
            &eff,
            &mcp_dest,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view_mcp_0 = LabelledView::new(&model_0, &mcp_tok_0, &stores);
        let view_mcp_3 = LabelledView::new(&model_3, &mcp_tok_3, &stores);
        assert_eq!(
            view_mcp_0.render_summary(&mcp_dest).as_bytes(),
            view_mcp_3.render_summary(&mcp_dest).as_bytes()
        );

        // 2. `model` reply (restricted to `public` so the 3 company records are omitted) is also byte-for-byte equal.
        let model_dest = dest_for_kind(DestKind::Model);
        let Decision::Allowed(mod_tok_0) = check(
            &eff,
            &model_dest,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let Decision::Allowed(mod_tok_3) = check(
            &eff,
            &model_dest,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let pub_allow = LabelAllow::from_ids([PUBLIC]);
        let view_mod_0 = LabelledView::new(&model_0, &mod_tok_0, &stores).restrict(&pub_allow);
        let view_mod_3 = LabelledView::new(&model_3, &mod_tok_3, &stores).restrict(&pub_allow);
        assert_eq!(
            view_mod_0.render_summary(&model_dest).as_bytes(),
            view_mod_3.render_summary(&model_dest).as_bytes()
        );
    }

    #[test]
    fn a_backup_with_a_canary_in_working_site_or_a_stray_file_is_refused_or_labelled() {
        let (policy, eff) = loaded_effective_policy();
        let rules = policy.marking_rules();
        let backup_dest = dest_for_kind(DestKind::Backup);

        // 1. Working content with a marking (`NP-0007 NDA`) gets labelled `np-0007` by `for_text`,
        //    and since `working` only holds `public`, `unlabelled`, `bid-0003`, `admit` refuses it.
        let working_rule = policy.store_rule(&StoreId::Working);
        let working_labels = for_text(
            "Working note containing NP-0007 NDA terms CANARY_WORKING_1",
            &rules,
            &LabelContext::store(&working_rule),
        );
        assert!(working_labels.contains(&lid("np-0007")));
        assert!(admit(&working_labels, &working_rule).is_err());

        // 2. Derived site content in `state` with a controlled canary (`ctl-0002`) is refused by backup egress.
        let site_labels = LabelSet::from_ids([lid("ctl-0002")]);
        let site_dec = check(
            &eff,
            &backup_dest,
            Some(&site_labels),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        assert!(site_dec.is_denied());

        // 3. A stray file with no source/store default and no marking gets `unlabelled`, which is refused by loaded-policy backup (`FB0901`).
        let stray_labels = for_text(
            "Stray file with CANARY_STRAY_99",
            &rules,
            &LabelContext::none(),
        );
        assert_eq!(stray_labels, LabelSet::unlabelled());
        let stray_dec = check(
            &eff,
            &backup_dest,
            Some(&stray_labels),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match stray_dec {
            Decision::Denied(d) => assert_eq!(d.code, Code::Fb0901),
            Decision::Allowed(_) => {
                panic!("stray unlabelled file must be refused under loaded policy")
            }
        }
    }

    #[test]
    fn a_marked_canary_held_at_floor_b_reaches_no_destination() {
        let records = fixture_records();
        let marked_rec = records
            .iter()
            .find(|r| r.label == "marked")
            .expect("planted marked record");
        let marked_set = LabelSet::marked();

        // 1. With a company store and no policy (Floor B), `overlay`, `working`, and `state` admit `marked`.
        let floor_b_policy = Policy::floor(FloorKind::B);
        for store in [StoreId::Overlay, StoreId::Working, StoreId::State] {
            let rule: StoreRule = floor_b_policy.store_rule(&store);
            assert!(
                admit(&marked_set, &rule).is_ok(),
                "Floor B store `{store}` must admit `marked`"
            );
        }

        // 2. `egress::check` denies `marked` with FB0901 for every destination kind.
        let eff_b = EffectivePolicy::floor(true);
        let model = build_model_from_records(&records, true);
        let stores = ReadableStores::new([
            StoreId::Corpus,
            StoreId::Overlay,
            StoreId::Working,
            StoreId::State,
        ]);

        for spec in DEST_KINDS {
            let dest = dest_for_kind(spec.kind);
            let dec = check(
                &eff_b,
                &dest,
                Some(&marked_set),
                None,
                now_2026_11_01(),
                Shipped::Hours(0),
            );
            match dec {
                Decision::Denied(d) => {
                    assert_eq!(
                        d.code,
                        Code::Fb0901,
                        "expected FB0901 for `marked` on {:?}",
                        spec.kind
                    );
                }
                Decision::Allowed(_) => {
                    panic!("`marked` must be denied for destination {:?}", spec.kind)
                }
            }

            // 3. No view for any kind holds the `marked` canary.
            if let Decision::Allowed(sel_token) = check(
                &eff_b,
                &dest,
                None,
                None,
                now_2026_11_01(),
                Shipped::Hours(0),
            ) {
                let view = LabelledView::new(&model, &sel_token, &stores);
                let text = view.render_summary(&dest);
                assert!(
                    !text.contains(&marked_rec.canary),
                    "marked canary leaked in view for {:?}",
                    spec.kind
                );
            }
        }

        // 4. Pure journal serialization holds the id and no canary.
        let actor = Actor {
            principal: Principal::parse("local").unwrap(),
            role: Role::Owner,
            via: Via::Cli,
        };
        let env = EventEnvelope::from_actor(&actor, StoreId::Corpus);
        for (idx, kind) in Kind::EGRESS.iter().enumerate() {
            let seq = (idx + 1) as u64;
            let payload = sample_payload(*kind, seq);
            let body = payload.render_body(seq, now_2026_11_01(), &env);
            let (line, _) = seal_body(&body, ZERO_HASH);
            assert!(
                !line.contains(&marked_rec.canary),
                "journal line for {kind:?} must never contain marked canary"
            );
        }

        // 5. And the snapshot label decision at Floor B backup refuses `marked` (`FB0901`).
        let backup_company = Dest::Backup {
            path: "/srv/company/backups/snap".into(),
            company_storage: true,
        };
        let snap_dec = check(
            &eff_b,
            &backup_company,
            Some(&marked_set),
            None,
            now_2026_11_01(),
            Shipped::Hours(0),
        );
        match snap_dec {
            Decision::Denied(d) => assert_eq!(d.code, Code::Fb0901),
            Decision::Allowed(_) => panic!("snapshot backup at Floor B must refuse `marked`"),
        }

        // Also verify `for_text` marks text containing `COMPANY CONFIDENTIAL` as `marked`.
        let detected = for_text(
            &format!("COMPANY CONFIDENTIAL {}", marked_rec.canary),
            &MarkingRules::builtin(),
            &LabelContext::none(),
        );
        assert_eq!(detected, LabelSet::marked());
    }
}
