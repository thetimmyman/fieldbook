//! Egress destination catalog, decision order, delivery binding, `LabelledView`, and `Authz`
//! integration tests (`core-policy` steps 6–9, 15, 16, 18).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod egress {
    use fieldbook_core::audit::{DestKind, Now};
    use fieldbook_core::authz::ReadableStores;
    use fieldbook_core::check::{Code, Host, Model, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{
        DEST_KINDS, Decision, Delivery, Dest, LabelledView, Operation, Reason, RowSource, Shipped,
        Target, Transfer, check, covers, shipped,
    };
    use fieldbook_core::labels::{
        LabelAllow, LabelId, LabelSet, OWNER_SUPPLIED, PUBLIC, StoreId, UNLABELLED, markings_in,
    };
    use fieldbook_core::policy::{
        ClosedWhy, EffectivePolicy, FloorKind, OwnerSettings, Policy, PolicyState,
    };
    use fieldbook_core::schema::Register;

    const FIXTURE_POLICY: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
    const POLICY_REF_MD: &str = include_str!("../../../docs/reference/policy.md");
    const MINI_CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    fn lid(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("valid label `{s}`: {e}"))
    }

    fn now_2026_11_01() -> Now {
        Now::new(CivilDate::parse("2026-11-01").unwrap(), 3600).unwrap()
    }

    fn loaded_effective() -> EffectivePolicy {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let mut owner = OwnerSettings::unset();
        owner.endpoints.insert(
            "approved-gateway".to_owned(),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED, lid("bid-0003")]),
        );
        EffectivePolicy::new(&PolicyState::Loaded(p), &owner)
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

    fn build_synthetic_model() -> Model {
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

        // Add two sources to `sources.csv`: S-0042 (public) and S-0043 (np-0007 via ledger/handling).
        let sources_header = tree
            .get_bytes("corpus/registers/sources.csv")
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .unwrap();
        let row_pub = "S-0042,1,FAR 15,48 CFR Part 15,NARA,primary_law,primary_evidence,A,Public domain,far_15,reviewed,S-0042-v1.txt,0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef,2025-10,2025-10-15,2025-11-01,2025-11-02,active,,V1,public,Public note,Purpose,,,,,\n";
        let row_np = "S-0043,1,Vendor NDA,Supplier NDA Quote,Vendor,primary_law,primary_evidence,A,Proprietary,vendor_nda,reviewed,S-0043-v1.txt,abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789,2025-10,2025-10-15,2025-11-01,2025-11-02,active,,V1,np-0007,Restricted NDA note,Purpose,,,,,\n";
        tree.insert_bytes(
            "corpus/registers/sources.csv",
            format!("{sources_header}{row_pub}{row_np}").into_bytes(),
        );
        tree.insert_bytes("corpus/sources/public/S-0042-v1.txt", b"Public source body");
        tree.insert_bytes(
            "corpus/sources/public/S-0043-v1.txt",
            b"Restricted NDA body",
        );

        // Add two claims to `claims.csv`: CL-CMP-0001 pointing at S-0042-v1, and CL-CMP-0002 pointing at S-0043-v1.
        let claims_header = tree
            .get_bytes("corpus/registers/claims.csv")
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .unwrap();
        let claim_pub = "CL-CMP-0001,cmp,fact,Public requirement statement,2025-11-01,S-0042,1,text_section,sec. 15.4,Public source body,false,owner,unchecked,pending_company_confirmation,proposed,observed,V1,,,,,,,,,,,,,,,,,,\n";
        let claim_np = "CL-CMP-0002,cmp,fact,Restricted requirement statement,2025-11-01,S-0043,1,text_section,sec. 1.1,Restricted NDA body,false,owner,unchecked,pending_company_confirmation,proposed,observed,V1,,,,,,,,,,,,,,,,,,\n";
        tree.insert_bytes(
            "corpus/registers/claims.csv",
            format!("{claims_header}{claim_pub}{claim_np}").into_bytes(),
        );

        // Add overlay record in `overlay/records/rec-bid.toml` with `bid-0003` and `overlay/records/rec-np.toml` with `np-0007`.
        tree.insert_bytes(
            "overlay/records/rec-bid.toml",
            b"handling = \"bid-0003\"\nbody = \"BID_RECORD_BODY\"\n",
        );
        tree.insert_bytes(
            "overlay/records/rec-np.toml",
            b"handling = \"np-0007\"\nbody = \"NP_RECORD_BODY\"\n",
        );

        let today = CivilDate::parse("2026-03-01").unwrap();
        load(&Host::new(tree, today), &Options::new(false)).0
    }

    // -------------------------------------------------------------------------
    // Step 6: DEST_KINDS & Dest tests
    // -------------------------------------------------------------------------

    #[test]
    fn dest_kinds_match_policy_docs_and_matrix() {
        let section = POLICY_REF_MD
            .split("## Destination kinds")
            .nth(1)
            .and_then(|s| s.split("## The decision").next())
            .expect("Destination kinds section in docs/reference/policy.md");

        let mut doc_rows = Vec::new();
        for line in section.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with('|') || !trimmed.ends_with('|') {
                continue;
            }
            let cells: Vec<&str> = trimmed[1..trimmed.len() - 1]
                .split('|')
                .map(str::trim)
                .collect();
            if cells.len() != 7 || cells[0] == "Kind" || cells[0].starts_with("---") {
                continue;
            }
            let advisory = cells[0].contains("(advisory)");
            let kind_word = cells[0]
                .split_whitespace()
                .next()
                .unwrap()
                .trim_matches('`');
            let kind = DestKind::parse(kind_word).unwrap();
            let source = if cells[1].starts_with("Policy row") {
                RowSource::PolicyRow
            } else {
                RowSource::Fixed
            };
            let shows_left_out = match cells[2] {
                "Yes" => Some(true),
                "No" => Some(false),
                "n/a" => None,
                other => panic!("unexpected Left-out count shown cell: {other}"),
            };
            doc_rows.push((kind, source, advisory, shows_left_out));
        }

        assert_eq!(doc_rows.len(), DEST_KINDS.len());
        for (spec, (doc_kind, doc_source, doc_adv, doc_left)) in DEST_KINDS.iter().zip(&doc_rows) {
            assert_eq!(spec.kind, *doc_kind);
            assert_eq!(spec.source, *doc_source);
            assert_eq!(spec.advisory, *doc_adv);
            assert_eq!(spec.shows_left_out_count, *doc_left);

            // Check loader acceptance matches `RowSource`.
            let snippet = format!(
                "policy_schema = 1\nrevision = 1\n[[destination]]\nkind = \"{}\"\nallow = [\"public\"]\n",
                spec.kind.as_str()
            );
            if spec.source == RowSource::Fixed {
                let err = Policy::parse(&snippet, None).unwrap_err();
                assert_eq!(err.code, Code::Fb0413);
            } else {
                // PolicyRow kinds don't fail on `kind` itself.
                if let Err(err) = Policy::parse(&snippet, None) {
                    assert_ne!(err.key_path, "destination[0].kind");
                }
            }
        }
    }

    #[test]
    fn fixed_rows_allow_public_only() {
        let eff = loaded_effective();
        for dest in [Dest::Git, Dest::Edition, Dest::Audit] {
            assert_eq!(eff.allow_set(&dest), LabelAllow::from_ids([PUBLIC]));
            let ok = check(
                &eff,
                &dest,
                Some(&LabelSet::public()),
                None,
                now_2026_11_01(),
                Shipped::Hours(0),
            );
            assert!(matches!(ok, Decision::Allowed(_)));

            let denied = check(
                &eff,
                &dest,
                Some(&LabelSet::from_ids([lid("bid-0003")])),
                None,
                now_2026_11_01(),
                Shipped::Hours(0),
            );
            match denied {
                Decision::Denied(d) => assert_eq!(d.code, Code::Fb0901),
                Decision::Allowed(_) => panic!("fixed row {dest:?} must deny company label"),
            }
        }
    }

    #[test]
    fn advisory_kinds_are_handoff_print_and_clipboard() {
        let adv: Vec<DestKind> = DEST_KINDS
            .iter()
            .filter(|s| s.advisory)
            .map(|s| s.kind)
            .collect();
        assert_eq!(
            adv,
            vec![DestKind::Handoff, DestKind::Print, DestKind::Clipboard]
        );
    }

    // -------------------------------------------------------------------------
    // Step 7 & Step 16: egress::check decision order & delivery binding tests
    // -------------------------------------------------------------------------

    #[test]
    fn closed_denies_first() {
        let eff = EffectivePolicy::new(
            &PolicyState::Closed(ClosedWhy::Invalid),
            &OwnerSettings::unset(),
        );
        // Even with an unknown label and no row, step 1 (Closed -> FB0413) wins first.
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::from_ids([lid("unk-9999")])),
            None,
            now_2026_11_01(),
            Shipped::Never,
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0413);
                assert_eq!(d.reason, Reason::Closed);
            }
            Decision::Allowed(_) => panic!("Closed state must deny first"),
        }
    }

    // control: LB-03
    #[test]
    fn unlabelled_is_denied_even_if_a_row_lists_it() {
        // Construct a floor-A policy and force `unlabelled` into its handoff row in memory;
        // step 2 of `check` must still deny `unlabelled` with FB0901 / `Reason::Unlabelled`.
        let mut floor_a = Policy::floor(FloorKind::A);
        floor_a.set_dest_allow(
            DestKind::Handoff,
            LabelAllow::from_ids([PUBLIC, UNLABELLED, OWNER_SUPPLIED]),
        );
        let eff = EffectivePolicy::new(&PolicyState::Loaded(floor_a), &OwnerSettings::unset());
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::unlabelled()),
            None,
            now_2026_11_01(),
            Shipped::Hours(0),
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0901);
                assert_eq!(d.reason, Reason::Unlabelled);
            }
            Decision::Allowed(_) => panic!("unlabelled must be denied in step 2"),
        }
    }

    #[test]
    fn floor_b_backup_takes_unlabelled_only_to_the_recorded_target() {
        let floor_b = EffectivePolicy::floor(true);
        let recorded = Dest::Backup {
            path: "/srv/company/backups/snap".into(),
            company_storage: true,
        };
        let unrecorded = Dest::Backup {
            path: "/srv/company/backups/snap".into(),
            company_storage: false,
        };

        // 1. Allowed on Floor B backup with company_storage = true.
        let ok = check(
            &floor_b,
            &recorded,
            Some(&LabelSet::unlabelled()),
            None,
            now_2026_11_01(),
            Shipped::Never,
        );
        assert!(matches!(ok, Decision::Allowed(_)));

        // 2. Denied (FB0901 / Reason::Unlabelled) when company_storage = false.
        match check(
            &floor_b,
            &unrecorded,
            Some(&LabelSet::unlabelled()),
            None,
            now_2026_11_01(),
            Shipped::Never,
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0901);
                assert_eq!(d.reason, Reason::Unlabelled);
            }
            Decision::Allowed(_) => panic!("unrecorded backup target must deny unlabelled"),
        }

        // 2b. Public backup with company_storage = false is also denied (FB0918 / Reason::NoRow) on Floor B.
        match check(
            &floor_b,
            &unrecorded,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Never,
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0918);
                assert_eq!(d.reason, Reason::NoRow);
            }
            Decision::Allowed(_) => panic!("unrecorded backup target must deny public on Floor B"),
        }

        // 3. Denied (FB0901) for every other destination kind at Floor B.
        for other in [
            Dest::Handoff,
            Dest::Print,
            Dest::Clipboard,
            Dest::Bridge,
            Dest::Mcp {
                client: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_owned(),
            },
            Dest::Export {
                path: "/srv/company/exports/out".into(),
            },
        ] {
            match check(
                &floor_b,
                &other,
                Some(&LabelSet::unlabelled()),
                None,
                now_2026_11_01(),
                Shipped::Never,
            ) {
                Decision::Denied(d) => assert_eq!(d.code, Code::Fb0901),
                Decision::Allowed(_) => panic!("other dest {other:?} must deny unlabelled"),
            }
        }

        // 4. Denied (FB0901 / Reason::Unlabelled) under a loaded policy even with company_storage = true.
        let mut loaded_pol = Policy::parse(FIXTURE_POLICY, None).unwrap();
        loaded_pol.set_dest_allow(
            DestKind::Backup,
            LabelAllow::from_ids([PUBLIC, UNLABELLED, OWNER_SUPPLIED]),
        );
        let loaded =
            EffectivePolicy::new(&PolicyState::Loaded(loaded_pol), &OwnerSettings::unset());
        match check(
            &loaded,
            &recorded,
            Some(&LabelSet::unlabelled()),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0901);
                assert_eq!(d.reason, Reason::Unlabelled);
            }
            Decision::Allowed(_) => panic!("loaded policy must deny unlabelled on backup"),
        }

        // 5. `marked` is denied on Floor B recorded backup too.
        match check(
            &floor_b,
            &recorded,
            Some(&LabelSet::marked()),
            None,
            now_2026_11_01(),
            Shipped::Never,
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0901);
                assert_eq!(d.reason, Reason::Marked);
            }
            Decision::Allowed(_) => panic!("marked must be denied on Floor B backup"),
        }
    }

    #[test]
    fn an_unknown_label_is_denied() {
        let eff = loaded_effective();
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::from_ids([lid("unk-9999")])),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0901);
                assert_eq!(d.reason, Reason::Label);
            }
            Decision::Allowed(_) => panic!("unknown label must be denied"),
        }
    }

    #[test]
    fn no_row_is_fb0918() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "missing-endpoint".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        match check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0918);
                assert_eq!(d.reason, Reason::NoRow);
            }
            Decision::Allowed(_) => panic!("missing row must be FB0918"),
        }
    }

    #[test]
    fn an_expired_row_is_fb0918() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let after_expiry = Now::new(CivilDate::parse("2027-07-01").unwrap(), 0).unwrap();
        match check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            after_expiry,
            Shipped::Hours(1),
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0918);
                assert_eq!(d.reason, Reason::Expired);
            }
            Decision::Allowed(_) => panic!("expired row must be FB0918"),
        }
    }

    #[test]
    fn the_row_applies_through_its_last_day() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let last_day = Now::new(CivilDate::parse("2027-06-30").unwrap(), 86_399).unwrap();
        let dec = check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            last_day,
            Shipped::Hours(1),
        );
        assert!(matches!(dec, Decision::Allowed(_)));
    }

    #[test]
    fn a_different_url_is_fb0918() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://other-gateway.example/v1".to_owned(),
        };
        match check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0918);
                assert_eq!(d.reason, Reason::Url);
            }
            Decision::Allowed(_) => panic!("mismatched URL must be FB0918"),
        }
    }

    #[test]
    fn an_unlisted_client_is_fb0918() {
        let eff = loaded_effective();
        let dest = Dest::Mcp {
            client: "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".to_owned(),
        };
        match check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0918);
                assert_eq!(d.reason, Reason::Client);
            }
            Decision::Allowed(_) => panic!("unlisted client must be FB0918"),
        }
    }

    #[test]
    fn a_path_outside_every_prefix_is_fb0918() {
        let eff = loaded_effective();
        let dest = Dest::Export {
            path: "/srv/other/out.html".into(),
        };
        match check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0918);
                assert_eq!(d.reason, Reason::NoRow);
            }
            Decision::Allowed(_) => panic!("path outside prefix must be FB0918"),
        }
    }

    #[test]
    fn export_rows_union() {
        let extra = format!(
            "{FIXTURE_POLICY}\n[[destination]]\nkind = \"export\"\nallow = [\"np-0007\"]\npaths = [\"/srv/company/exports/sub\"]\n"
        );
        let p = Policy::parse(&extra, None).unwrap();
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &OwnerSettings::unset());
        let dest = Dest::Export {
            path: "/srv/company/exports/sub/pack.html".into(),
        };
        let combined = LabelSet::from_ids([lid("bid-0003"), lid("np-0007")]);
        let dec = check(
            &eff,
            &dest,
            Some(&combined),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        assert!(matches!(dec, Decision::Allowed(_)));
    }

    #[test]
    fn a_label_outside_the_row_is_fb0901() {
        let eff = loaded_effective();
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::from_ids([lid("ctl-0002")])),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0901);
                assert_eq!(d.reason, Reason::Label);
            }
            Decision::Allowed(_) => panic!("label outside allow must be FB0901"),
        }
    }

    // control: EG-01
    #[test]
    fn the_subset_test_needs_every_label() {
        let eff = loaded_effective();
        // `Bridge` allows `public` and `bid-0003`, but NOT `np-0007`.
        // A set carrying both `bid-0003` (allowed) and `np-0007` (forbidden) must be denied.
        let both = LabelSet::from_ids([lid("bid-0003"), lid("np-0007")]);
        let dec = check(
            &eff,
            &Dest::Bridge,
            Some(&both),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0901);
                assert_eq!(d.reason, Reason::Label);
            }
            Decision::Allowed(_) => panic!("subset test must require every label in the set"),
        }
    }

    #[test]
    fn a_marking_outside_the_request_is_fb0902() {
        let eff = loaded_effective();
        let text = b"Text containing NP-0007 NDA marking";
        let rules = eff.marking_rules();
        let markings = markings_in(std::str::from_utf8(text).unwrap(), &rules);
        let delivery = Delivery {
            entries: vec![Transfer::reply("handoff", text)],
            markings,
        };
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::from_ids([lid("bid-0003")])),
            Some(&delivery),
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0902);
                assert_eq!(d.reason, Reason::Mismatch);
            }
            Decision::Allowed(_) => panic!("marking outside request labels must be FB0902"),
        }
    }

    #[test]
    fn a_plain_text_has_no_mismatch() {
        let eff = loaded_effective();
        let text = b"Plain text with no banner at all.";
        let rules = eff.marking_rules();
        let markings = markings_in(std::str::from_utf8(text).unwrap(), &rules);
        let delivery = Delivery {
            entries: vec![Transfer::reply("handoff", text)],
            markings,
        };
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::public()),
            Some(&delivery),
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        assert!(matches!(dec, Decision::Allowed(_)));
    }

    #[test]
    fn the_owner_list_narrows_the_decision() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let mut owner = OwnerSettings::unset();
        owner.handoff = Some(LabelAllow::from_ids([PUBLIC]));
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &owner);

        // Policy allows `np-0007` to handoff, but owner narrowed handoff to `public`.
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::from_ids([lid("np-0007")])),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Denied(d) => assert_eq!(d.code, Code::Fb0901),
            Decision::Allowed(_) => panic!("owner narrowing must deny np-0007"),
        }
    }

    // control: EG-02
    #[test]
    fn a_stale_export_stops_a_company_model_call() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let dec = check(
            &eff,
            &dest,
            Some(&LabelSet::from_ids([lid("bid-0003")])),
            None,
            now_2026_11_01(),
            Shipped::Hours(25),
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0925);
                assert_eq!(d.reason, Reason::Shipped);
            }
            Decision::Allowed(_) => panic!("stale export must deny company model call with FB0925"),
        }
    }

    #[test]
    fn no_export_ever_stops_a_company_model_call() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let dec = check(
            &eff,
            &dest,
            Some(&LabelSet::from_ids([lid("bid-0003")])),
            None,
            now_2026_11_01(),
            Shipped::Never,
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0925);
                assert_eq!(d.reason, Reason::Shipped);
            }
            Decision::Allowed(_) => panic!("Shipped::Never must deny company model call"),
        }
    }

    #[test]
    fn with_the_requirement_off_a_lapsed_export_allows_the_call() {
        let text = format!("{FIXTURE_POLICY}require_fresh_export = false\n");
        let p = Policy::parse(&text, None).unwrap();
        let expected_hash = p.hash().to_owned();
        let mut owner = OwnerSettings::unset();
        owner.endpoints.insert(
            "approved-gateway".to_owned(),
            LabelAllow::from_ids([PUBLIC, lid("bid-0003")]),
        );
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &owner);
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let labels = LabelSet::from_ids([lid("bid-0003")]);
        let dec = check(
            &eff,
            &dest,
            Some(&labels),
            None,
            now_2026_11_01(),
            Shipped::Never,
        );
        match dec {
            Decision::Allowed(tok) => {
                assert_eq!(tok.kind(), DestKind::Model);
                assert_eq!(tok.labels(), Some(&labels));
                assert_eq!(tok.policy_hash(), expected_hash);
            }
            Decision::Denied(d) => {
                panic!("expected Allowed when require_fresh_export=false, got {d:?}")
            }
        }
    }

    #[test]
    fn a_public_model_call_ignores_the_export_age() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let dec = check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Never,
        );
        assert!(matches!(dec, Decision::Allowed(_)));
    }

    #[test]
    fn shipped_is_never_with_no_export() {
        assert_eq!(shipped(None, now_2026_11_01()), Shipped::Never);
    }

    // control: EG-02
    #[test]
    fn shipped_counts_whole_hours_since_the_last_export() {
        let last = Now::new(CivilDate::parse("2026-11-01").unwrap(), 0).unwrap();
        // 2 hours and 59 minutes and 59 seconds -> rounds down to 2 whole hours.
        let now = Now::new(CivilDate::parse("2026-11-01").unwrap(), 2 * 3600 + 3599).unwrap();
        assert_eq!(shipped(Some(last), now), Shipped::Hours(2));
    }

    #[test]
    fn covers_agrees_with_check() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let bid = LabelSet::from_ids([lid("bid-0003")]);
        let ctl = LabelSet::from_ids([lid("ctl-0002")]);

        assert!(covers(
            &eff,
            &dest,
            &bid,
            now_2026_11_01(),
            Shipped::Hours(2)
        ));
        assert!(!covers(
            &eff,
            &dest,
            &bid,
            now_2026_11_01(),
            Shipped::Hours(25)
        ));
        assert!(!covers(
            &eff,
            &dest,
            &ctl,
            now_2026_11_01(),
            Shipped::Hours(2)
        ));
    }

    #[test]
    fn allowed_carries_kind_labels_and_hash() {
        let eff = loaded_effective();
        let labels = LabelSet::from_ids([lid("bid-0003")]);
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&labels),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Allowed(tok) => {
                assert_eq!(tok.kind(), DestKind::Handoff);
                assert_eq!(tok.labels(), Some(&labels));
                assert_eq!(tok.policy_hash(), eff.hash());
            }
            Decision::Denied(d) => panic!("expected Allowed, got {d:?}"),
        }
    }

    #[test]
    fn a_denial_holds_no_title() {
        let eff = loaded_effective();
        let dec = check(
            &eff,
            &Dest::Handoff,
            Some(&LabelSet::from_ids([lid("ctl-0002")])),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Denied(d) => {
                let dbg = format!("{d:?}");
                assert!(!dbg.contains("Export-controlled technical data"));
                assert!(!dbg.contains("CTL-0002: controlled"));
            }
            Decision::Allowed(_) => panic!("expected Denied"),
        }
    }

    #[test]
    fn selection_needs_no_invented_labels() {
        let eff = loaded_effective();
        let dec = check(
            &eff,
            &Dest::Handoff,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Allowed(tok) => {
                assert!(tok.labels().is_none());
                assert!(!tok.has_delivery());
                assert!(tok.permitted().contains(&lid("np-0007")));
            }
            Decision::Denied(d) => panic!("expected selection Allowed, got {d:?}"),
        }
    }

    #[test]
    fn preflight_cannot_write() {
        let eff = loaded_effective();
        let dest = Dest::Export {
            path: "/srv/company/exports/out.txt".into(),
        };
        let dec = check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        let Decision::Allowed(tok) = dec else {
            panic!("expected preflight Allowed");
        };
        assert!(!tok.permits_transfer(
            &Target::Path("/srv/company/exports/out.txt".into()),
            &Operation::Write,
            b"hello"
        ));
    }

    #[test]
    fn delivery_without_labels_is_mismatch() {
        let eff = loaded_effective();
        let delivery = Delivery {
            entries: vec![Transfer::reply("handoff", b"body")],
            markings: None,
        };
        let dec = check(
            &eff,
            &Dest::Handoff,
            None,
            Some(&delivery),
            now_2026_11_01(),
            Shipped::Hours(1),
        );
        match dec {
            Decision::Denied(d) => {
                assert_eq!(d.code, Code::Fb0902);
                assert_eq!(d.reason, Reason::Mismatch);
            }
            Decision::Allowed(_) => panic!("(None, Some(delivery)) must be FB0902 mismatch"),
        }
    }

    #[test]
    fn empty_permission_selection_is_empty() {
        let floor_b = EffectivePolicy::floor(true);
        let dest = Dest::Model {
            name: "any".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let dec = check(
            &floor_b,
            &dest,
            None,
            None,
            now_2026_11_01(),
            Shipped::Never,
        );
        let Decision::Allowed(tok) = dec else {
            panic!("expected selection token");
        };
        assert!(tok.permitted().is_empty());
    }

    #[test]
    fn selection_token_cannot_write() {
        let eff = loaded_effective();
        let dest = Dest::Export {
            path: "/srv/company/exports/out.txt".into(),
        };
        let Decision::Allowed(tok) =
            check(&eff, &dest, None, None, now_2026_11_01(), Shipped::Hours(1))
        else {
            panic!("expected selection token");
        };
        assert!(!tok.permits_transfer(
            &Target::Path("/srv/company/exports/out.txt".into()),
            &Operation::Write,
            b"hello"
        ));
    }

    #[test]
    fn changed_body_token_reuse_is_refused() {
        let eff = loaded_effective();
        let dest = Dest::Export {
            path: "/srv/company/exports/out.txt".into(),
        };
        let target = Target::Path("/srv/company/exports/out.txt".into());
        let delivery = Delivery {
            entries: vec![Transfer::write_path(
                "/srv/company/exports/out.txt",
                b"approved-bytes",
            )],
            markings: None,
        };
        let Decision::Allowed(tok) = check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            Some(&delivery),
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected delivery-bound token");
        };
        assert!(tok.permits_transfer(&target, &Operation::Write, b"approved-bytes"));
        assert!(!tok.permits_transfer(&target, &Operation::Write, b"tampered-bytes"));
    }

    #[test]
    fn cross_path_token_reuse_is_refused() {
        let eff = loaded_effective();
        let dest = Dest::Export {
            path: "/srv/company/exports/a.txt".into(),
        };
        let delivery = Delivery {
            entries: vec![Transfer::write_path("/srv/company/exports/a.txt", b"bytes")],
            markings: None,
        };
        let Decision::Allowed(tok) = check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            Some(&delivery),
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected delivery-bound token");
        };
        assert!(!tok.permits_transfer(
            &Target::Path("/srv/company/exports/b.txt".into()),
            &Operation::Write,
            b"bytes"
        ));
    }

    #[test]
    fn cross_endpoint_token_reuse_is_refused() {
        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let delivery = Delivery {
            entries: vec![Transfer::post_endpoint(
                "https://ai-gateway.example/v1",
                b"prompt",
            )],
            markings: None,
        };
        let Decision::Allowed(tok) = check(
            &eff,
            &dest,
            Some(&LabelSet::public()),
            Some(&delivery),
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected delivery-bound token");
        };
        assert!(tok.permits_transfer(
            &Target::Endpoint("https://ai-gateway.example/v1".to_owned()),
            &Operation::Post,
            b"prompt"
        ));
        assert!(!tok.permits_transfer(
            &Target::Endpoint("https://other-gateway.example/v1".to_owned()),
            &Operation::Post,
            b"prompt"
        ));

        // Delivery binding to an endpoint that spoofs a host prefix or path prefix is refused at `check` (FB0902).
        for bad_ep in [
            "https://ai-gateway.example.attacker.invalid/v1",
            "https://ai-gateway.example/v10",
            "https://ai-gateway.example/v1/../admin",
        ] {
            let bad_deliv = Delivery {
                entries: vec![Transfer::post_endpoint(bad_ep, b"prompt")],
                markings: None,
            };
            let res = check(
                &eff,
                &dest,
                Some(&LabelSet::public()),
                Some(&bad_deliv),
                now_2026_11_01(),
                Shipped::Hours(1),
            );
            match res {
                Decision::Denied(d) => {
                    assert_eq!(d.code, Code::Fb0902);
                    assert_eq!(d.reason, Reason::Mismatch);
                }
                Decision::Allowed(_) => panic!("endpoint `{bad_ep}` must be refused"),
            }
        }

        // Delivery binding to a filesystem path outside `dest.path` or outside approved policy prefixes is refused (FB0902).
        let export_dest = Dest::Export {
            path: "/srv/company/exports/approved.txt".into(),
        };
        for bad_path in ["/srv/other/outside.txt", "/srv/company/exports/other.txt"] {
            let bad_path_deliv = Delivery {
                entries: vec![Transfer::write_path(bad_path, b"bytes")],
                markings: None,
            };
            let res = check(
                &eff,
                &export_dest,
                Some(&LabelSet::public()),
                Some(&bad_path_deliv),
                now_2026_11_01(),
                Shipped::Hours(1),
            );
            match res {
                Decision::Denied(d) => {
                    assert_eq!(d.code, Code::Fb0902);
                    assert_eq!(d.reason, Reason::Mismatch);
                }
                Decision::Allowed(_) => panic!("transfer path `{bad_path}` must be refused"),
            }
        }
    }

    #[test]
    fn source_body_handling_and_extracted_text_follow_source_version_labels() {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public");
        tree.insert_dir("corpus/sources/text");
        tree.insert_dir("corpus/sources/text/S-0043-v1");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
        insert_register_headers(&mut tree);

        let sources_header = tree
            .get_bytes("corpus/registers/sources.csv")
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .unwrap();
        let row_np = "S-0043,1,Vendor NDA,Supplier NDA Quote,Vendor,primary_law,primary_evidence,A,Proprietary,vendor_nda,reviewed,S-0043-v1.txt,abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789,2025-10,2025-10-15,2025-11-01,2025-11-02,active,,V1,np-0007,Restricted NDA note,Purpose,,,,,\n";
        tree.insert_bytes(
            "corpus/registers/sources.csv",
            format!("{sources_header}{row_np}").into_bytes(),
        );
        // Source original contains a line that looks like inline `handling = "public"` metadata;
        // it must NEVER override the source register's `np-0007` label.
        tree.insert_bytes(
            "corpus/sources/public/S-0043-v1.txt",
            b"handling = \"public\"\nRESTRICTED_ORIGINAL_CANARY\n",
        );
        // Extracted text under `corpus/sources/text/S-0043-v1/p0001.txt` has no inline handling metadata;
        // it must be excluded whenever `S-0043-v1` is excluded.
        tree.insert_bytes(
            "corpus/sources/text/S-0043-v1/p0001.txt",
            b"RESTRICTED_EXTRACTED_PAGE_CANARY\n",
        );

        let model = load(
            &Host::new(tree, CivilDate::parse("2026-03-01").unwrap()),
            &Options::new(false),
        )
        .0;
        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Bridge,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert!(view.sources().is_empty());
        assert!(
            view.tree()
                .get("corpus/sources/public/S-0043-v1.txt")
                .is_none()
        );
        assert!(
            view.tree()
                .get("corpus/sources/text/S-0043-v1/p0001.txt")
                .is_none()
        );
        let summary = view.render_summary(&Dest::Bridge);
        assert!(!summary.contains("RESTRICTED_ORIGINAL_CANARY"));
        assert!(!summary.contains("RESTRICTED_EXTRACTED_PAGE_CANARY"));
    }

    // -------------------------------------------------------------------------
    // Step 8 & Step 18: LabelledView tests
    // -------------------------------------------------------------------------

    #[test]
    fn a_record_outside_the_set_is_left_out_whole() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        // `Bridge` allows `public` and `bid-0003`, NOT `np-0007`.
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Bridge,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let readable = ReadableStores::corpus();
        let view = LabelledView::new(&model, &tok, &readable);
        let summary = view.render_summary(&Dest::Bridge);
        assert!(summary.contains("S-0042-v1"));
        assert!(!summary.contains("S-0043-v1"));
        assert!(!summary.contains("Restricted NDA note"));
    }

    #[test]
    fn left_out_counts_what_was_dropped() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Print,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert!(view.left_out() >= 1);
        assert_eq!(
            view.left_out_line(&Dest::Print),
            format!("Left out (policy): {} records", view.left_out())
        );
    }

    #[test]
    fn a_record_inside_the_set_is_kept() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Handoff,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert_eq!(view.sources().len(), 2);
    }

    #[test]
    fn a_row_that_points_at_a_dropped_record_is_dropped() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        // `Print` allows `public` and `bid-0003`, so `S-0043-v1` (`np-0007`) is dropped;
        // `CL-CMP-0002` (which points at `S-0043-v1`) must also be dropped!
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Print,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert_eq!(view.sources().len(), 1);
        assert_eq!(view.claims().len(), 1);
        assert_eq!(view.claims()[0].value.claim_id.as_str(), "CL-CMP-0001");
    }

    #[test]
    fn a_view_for_public_keeps_only_public() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Clipboard,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert_eq!(view.sources().len(), 1);
        assert_eq!(view.sources()[0].value.source_id.as_str(), "S-0042");
    }

    #[test]
    fn a_store_the_actor_may_not_read_is_left_out() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Handoff,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        // With `ReadableStores::corpus()`, `overlay/records/rec-bid.toml` is left out even though `bid-0003` is permitted.
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert!(view.tree().get("overlay/records/rec-bid.toml").is_none());
    }

    #[test]
    fn the_count_is_shown_only_to_kinds_that_show_it() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Print,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert!(
            view.left_out_line(&Dest::Handoff)
                .starts_with("Left out (policy): ")
        );
        assert!(
            view.left_out_line(&Dest::Print)
                .starts_with("Left out (policy): ")
        );
        assert!(
            view.left_out_line(&Dest::Export {
                path: "/srv/company/exports/out".into()
            })
            .starts_with("Left out (policy): ")
        );
        assert_eq!(
            view.left_out_line(&Dest::Clipboard),
            "Some records were omitted."
        );
        assert_eq!(
            view.left_out_line(&Dest::Bridge),
            "Some records were omitted."
        );
        assert_eq!(
            view.left_out_line(&Dest::Edition),
            "Some records were omitted."
        );
    }

    #[test]
    fn the_model_reply_text_does_not_change_with_the_count() {
        let model_with_extra = build_synthetic_model();
        let mut tree_clean = Tree::new();
        tree_clean.insert_dir("corpus");
        tree_clean.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree_clean.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
        insert_register_headers(&mut tree_clean);
        let sources_header = tree_clean
            .get_bytes("corpus/registers/sources.csv")
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .unwrap();
        let row_pub = "S-0042,1,FAR 15,48 CFR Part 15,NARA,primary_law,primary_evidence,A,Public domain,far_15,reviewed,S-0042-v1.txt,0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef,2025-10,2025-10-15,2025-11-01,2025-11-02,active,,V1,public,Public note,Purpose,,,,,\n";
        tree_clean.insert_bytes(
            "corpus/registers/sources.csv",
            format!("{sources_header}{row_pub}").into_bytes(),
        );
        tree_clean.insert_bytes("corpus/sources/public/S-0042-v1.txt", b"Public source body");
        let claims_header = tree_clean
            .get_bytes("corpus/registers/claims.csv")
            .map(|b| String::from_utf8(b.to_vec()).unwrap())
            .unwrap();
        let claim_pub = "CL-CMP-0001,cmp,fact,Public requirement statement,2025-11-01,S-0042,1,text_section,sec. 15.4,Public source body,false,owner,unchecked,pending_company_confirmation,proposed,observed,V1,,,,,,,,,,,,,,,,,,\n";
        tree_clean.insert_bytes(
            "corpus/registers/claims.csv",
            format!("{claims_header}{claim_pub}").into_bytes(),
        );
        let model_clean = load(
            &Host::new(tree_clean, CivilDate::parse("2026-03-01").unwrap()),
            &Options::new(false),
        )
        .0;

        let eff = loaded_effective();
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let Decision::Allowed(tok1) =
            check(&eff, &dest, None, None, now_2026_11_01(), Shipped::Hours(1))
        else {
            panic!("expected Allowed");
        };
        let Decision::Allowed(tok2) =
            check(&eff, &dest, None, None, now_2026_11_01(), Shipped::Hours(1))
        else {
            panic!("expected Allowed");
        };
        let v1 = LabelledView::new(&model_clean, &tok1, &ReadableStores::corpus());
        let v2 = LabelledView::new(&model_with_extra, &tok2, &ReadableStores::corpus());
        assert_eq!(v1.render_summary(&dest), v2.render_summary(&dest));
    }

    #[test]
    fn a_corpus_only_list_keeps_only_corpus_records() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Handoff,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        for (path, _) in view.tree().iter() {
            assert!(
                path.starts_with("corpus/"),
                "unexpected non-corpus path in corpus-only view: {path}"
            );
        }
    }

    #[test]
    fn filtered_tree_keeps_structure_without_hidden_paths() {
        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/pages");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("corpus/sources");
        tree.insert_dir("corpus/sources/public"); // empty originals root!
        tree.insert_dir("overlay");
        tree.insert_dir("overlay/secret_nested");
        tree.insert_dir("overlay/permitted_nested");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
        insert_register_headers(&mut tree);
        tree.insert_bytes(
            "overlay/secret_nested/canary.toml",
            b"handling = \"ctl-0002\"\nbody = \"FORBIDDEN\"\n",
        );
        tree.insert_bytes(
            "overlay/permitted_nested/allowed.toml",
            b"handling = \"bid-0003\"\nbody = \"PERMITTED\"\n",
        );

        let model = load(
            &Host::new(tree, CivilDate::parse("2026-03-01").unwrap()),
            &Options::new(false),
        )
        .0;

        let eff = loaded_effective();
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Handoff,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };

        // Read both Corpus and Overlay.
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        struct InGroup;
        impl fieldbook_core::authz::Groups for InGroup {
            fn in_group(&self, _: &str) -> Option<bool> {
                Some(true)
            }
        }
        let adapter = fieldbook_core::authz::PolicyReaders::new(&p, &InGroup);
        let actor = fieldbook_core::audit::Actor {
            principal: fieldbook_core::audit::Principal::parse("local").unwrap(),
            role: fieldbook_core::audit::Role::Owner,
            via: fieldbook_core::audit::Via::Cli,
        };
        let readable =
            fieldbook_core::authz::readable(&adapter, &actor, &[StoreId::Corpus, StoreId::Overlay]);

        let view = LabelledView::new(&model, &tok, &readable);
        // Empty originals root remains present.
        assert!(view.tree().contains_dir("corpus/sources/public"));
        // Ancestor of permitted file remains present.
        assert!(view.tree().contains_dir("overlay/permitted_nested"));
        // Arbitrary directory containing only forbidden-label canary is absent!
        assert!(!view.tree().contains_dir("overlay/secret_nested"));
        assert!(
            view.tree()
                .get("overlay/secret_nested/canary.toml")
                .is_none()
        );
    }

    #[test]
    fn mixed_public_permissions_keep_public_units() {
        let model = build_synthetic_model();
        let eff = loaded_effective();
        // `Handoff` permits `public`, `owner-supplied`, `np-0007`, `bid-0003` (a mixed public+company LabelAllow).
        let Decision::Allowed(tok) = check(
            &eff,
            &Dest::Handoff,
            None,
            None,
            now_2026_11_01(),
            Shipped::Hours(1),
        ) else {
            panic!("expected Allowed");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert!(
            view.sources()
                .iter()
                .any(|s| s.value.source_id.as_str() == "S-0042"),
            "public source must remain in mixed public+company view"
        );

        // And `restrict` narrows without LabelSet normalization.
        let narrowed = view.restrict(&LabelAllow::from_ids([PUBLIC]));
        assert_eq!(narrowed.sources().len(), 1);
        assert_eq!(narrowed.sources()[0].value.source_id.as_str(), "S-0042");
    }

    #[test]
    fn empty_view_never_selects_an_item() {
        let model = build_synthetic_model();
        let floor_b = EffectivePolicy::floor(true);
        let dest = Dest::Model {
            name: "any".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        let Decision::Allowed(tok) = check(
            &floor_b,
            &dest,
            None,
            None,
            now_2026_11_01(),
            Shipped::Never,
        ) else {
            panic!("expected selection token");
        };
        let view = LabelledView::new(&model, &tok, &ReadableStores::corpus());
        assert!(view.sources().is_empty());
        assert!(view.claims().is_empty());
        assert!(view.pages().is_empty());
        assert!(view.corpus().is_none());
    }
}

mod authz {
    use fieldbook_core::audit::{Actor, Now, Principal, Role, Via, Word};
    use fieldbook_core::authz::{
        Act, Authz, AuthzDecision, Groups, PolicyReaders, Scope, readable,
    };
    use fieldbook_core::check::{Host, Options, Tree, load};
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, LabelledView, Shipped, check};
    use fieldbook_core::labels::StoreId;
    use fieldbook_core::policy::{EffectivePolicy, OwnerSettings, Policy, PolicyState};
    use fieldbook_core::schema::Register;

    const FIXTURE_POLICY: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
    const DECISION_RS: &str = include_str!("../src/egress/decision.rs");
    const MINI_CORPUS_TOML: &str =
        include_str!("../../../tests/fixtures/corpus-mini/corpus/corpus.toml");

    struct ScriptedGroups(Option<bool>);
    impl Groups for ScriptedGroups {
        fn in_group(&self, _group: &str) -> Option<bool> {
            self.0
        }
    }

    fn sample_actor() -> Actor {
        Actor {
            principal: Principal::parse("local").unwrap(),
            role: Role::Owner,
            via: Via::Cli,
        }
    }

    #[test]
    fn read_is_allowed_when_the_store_names_no_readers() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let groups = ScriptedGroups(Some(false));
        let authz = PolicyReaders::new(&p, &groups);
        // `working` in fixture has no `readers`.
        assert_eq!(
            authz.allow(
                &sample_actor(),
                Act::Read,
                &Scope::for_store(StoreId::Working)
            ),
            AuthzDecision::Allow
        );
    }

    #[test]
    fn read_needs_the_group_when_the_store_names_readers() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let not_in = ScriptedGroups(Some(false));
        let in_grp = ScriptedGroups(Some(true));

        let deny_adapter = PolicyReaders::new(&p, &not_in);
        assert_eq!(
            deny_adapter.allow(
                &sample_actor(),
                Act::Read,
                &Scope::for_store(StoreId::Overlay)
            ),
            AuthzDecision::Deny(Word::new_static("authz"))
        );

        let allow_adapter = PolicyReaders::new(&p, &in_grp);
        assert_eq!(
            allow_adapter.allow(
                &sample_actor(),
                Act::Read,
                &Scope::for_store(StoreId::Overlay)
            ),
            AuthzDecision::Allow
        );
    }

    #[test]
    fn an_unreadable_token_is_denied_for_a_store_with_readers() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let unreadable = ScriptedGroups(None);
        let adapter = PolicyReaders::new(&p, &unreadable);
        assert_eq!(
            adapter.allow(
                &sample_actor(),
                Act::Read,
                &Scope::for_store(StoreId::Overlay)
            ),
            AuthzDecision::Deny(Word::new_static("authz"))
        );
    }

    #[test]
    fn admin_follows_changers_when_named() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let not_in = ScriptedGroups(Some(false));
        let in_grp = ScriptedGroups(Some(true));

        let deny_adapter = PolicyReaders::new(&p, &not_in);
        assert_eq!(
            deny_adapter.allow(
                &sample_actor(),
                Act::Admin,
                &Scope::for_store(StoreId::Overlay)
            ),
            AuthzDecision::Deny(Word::new_static("authz"))
        );

        let allow_adapter = PolicyReaders::new(&p, &in_grp);
        assert_eq!(
            allow_adapter.allow(
                &sample_actor(),
                Act::Admin,
                &Scope::for_store(StoreId::Overlay)
            ),
            AuthzDecision::Allow
        );
    }

    #[test]
    fn admin_is_allowed_when_no_changers_are_named() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let not_in = ScriptedGroups(Some(false));
        let adapter = PolicyReaders::new(&p, &not_in);
        // `working` has no `changers`.
        assert_eq!(
            adapter.allow(
                &sample_actor(),
                Act::Admin,
                &Scope::for_store(StoreId::Working)
            ),
            AuthzDecision::Allow
        );
    }

    #[test]
    fn readable_lists_only_allowed_stores() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let not_in = ScriptedGroups(Some(false));
        let adapter = PolicyReaders::new(&p, &not_in);
        let stores = readable(
            &adapter,
            &sample_actor(),
            &[
                StoreId::Corpus,
                StoreId::Overlay,
                StoreId::Working,
                StoreId::State,
            ],
        );
        // `overlay` and `state` name `readers` in fixture, so only `corpus` and `working` are readable when not in group.
        assert_eq!(stores.stores(), &[StoreId::Corpus, StoreId::Working]);
    }

    #[test]
    fn egress_decision_does_not_call_authz() {
        assert!(
            !DECISION_RS.contains("authz::"),
            "egress/decision.rs must not call authz::"
        );
    }

    #[test]
    fn a_denying_authz_in_a_test_narrows_a_view() {
        struct ScriptedAuthz;
        impl Authz for ScriptedAuthz {
            fn allow(&self, _who: &Actor, _act: Act, on: &Scope) -> AuthzDecision {
                if on.store == StoreId::Corpus {
                    AuthzDecision::Allow
                } else {
                    AuthzDecision::Deny(Word::new_static("authz"))
                }
            }
        }

        let mut tree = Tree::new();
        tree.insert_dir("corpus");
        tree.insert_dir("corpus/registers");
        tree.insert_dir("overlay");
        tree.insert_bytes("corpus/corpus.toml", MINI_CORPUS_TOML.as_bytes());
        tree.insert_bytes("corpus/ASSISTANT.md", b"rules_version = 1\n");
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
        tree.insert_bytes(
            "overlay/records/item.toml",
            b"handling = \"bid-0003\"\nbody = \"OVERLAY_CANARY\"\n",
        );
        let model = load(
            &Host::new(tree, CivilDate::parse("2026-03-01").unwrap()),
            &Options::new(false),
        )
        .0;

        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &OwnerSettings::unset());
        let now = Now::new(CivilDate::parse("2026-11-01").unwrap(), 3600).unwrap();
        let Decision::Allowed(tok) =
            check(&eff, &Dest::Handoff, None, None, now, Shipped::Hours(1))
        else {
            panic!("expected Allowed");
        };

        let r = readable(
            &ScriptedAuthz,
            &sample_actor(),
            &[StoreId::Corpus, StoreId::Overlay],
        );
        let view = LabelledView::new(&model, &tok, &r);
        assert!(
            !view
                .render_summary(&Dest::Handoff)
                .contains("OVERLAY_CANARY")
        );
    }
}
