//! Policy parser, floor policy, state, and owner settings merge tests (`core-policy` steps 2–5, 15, 21, 22).

#![expect(clippy::unwrap_used, reason = "integration test helpers")]

mod policy {
    use fieldbook_core::audit::{DestKind, Now};
    use fieldbook_core::check::Code;
    use fieldbook_core::date::CivilDate;
    use fieldbook_core::egress::{Decision, Dest, Shipped, check};
    use fieldbook_core::labels::{
        LabelAllow, LabelContext, LabelId, LabelSet, MARKED, OWNER_SUPPLIED, PUBLIC, StoreId,
        StoreRule, UNLABELLED, admit, for_text,
    };
    use fieldbook_core::policy::{
        ClosedWhy, CutBy, EffectivePolicy, FloorKind, KEY_TABLE, MergeKind, OwnerSettings, Policy,
        PolicyState, merge_list, merge_number, merge_switch, policy_hash,
    };

    const FIXTURE_POLICY: &str = include_str!("../../../tests/fixtures/policy/policy.toml");
    const GOLDEN_FLOOR_HASH: &str = include_str!("../../../tests/golden/policy/floor-hash.txt");
    const POLICY_REF_MD: &str = include_str!("../../../docs/reference/policy.md");

    fn lid(s: &str) -> LabelId {
        LabelId::parse(s).unwrap_or_else(|e| panic!("valid label `{s}`: {e}"))
    }

    fn now_2026_11_01() -> Now {
        Now::new(CivilDate::parse("2026-11-01").unwrap(), 3600).unwrap()
    }

    // -------------------------------------------------------------------------
    // Step 2: Policy::parse tests
    // -------------------------------------------------------------------------

    #[test]
    fn the_fixture_policy_parses() {
        let p = Policy::parse(FIXTURE_POLICY, None).expect("fixture policy must parse");
        assert_eq!(p.revision(), 3);
        assert!(p.label(&lid("np-0007")).is_some());
        assert!(p.label(&lid("bid-0003")).is_some());
        assert!(p.label(&lid("ctl-0002")).is_some());
        assert!(p.source("finance-exports").is_some());
        assert!(p.store_row(&StoreId::Overlay).is_some());
        assert_eq!(p.audit().sink.as_deref(), Some("/srv/company/audit"));
        assert_eq!(p.audit().max_unshipped_hours, Some(24));
        assert!(p.audit().require_fresh_export);
    }

    #[test]
    fn unlabelled_in_allow_is_invalid() {
        let bad = FIXTURE_POLICY.replace(
            "allow = [\"public\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]\nref = \"CHG-0002\"",
            "allow = [\"public\", \"unlabelled\"]\nref = \"CHG-0002\"",
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[1].allow");
        assert_eq!(err.label_id, Some(UNLABELLED));
    }

    #[test]
    fn marked_in_allow_is_invalid() {
        let bad = FIXTURE_POLICY.replace(
            "allow = [\"public\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]\nref = \"CHG-0002\"",
            "allow = [\"public\", \"marked\"]\nref = \"CHG-0002\"",
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[1].allow");
        assert_eq!(err.label_id, Some(MARKED));
    }

    #[test]
    fn a_fixed_kind_is_invalid() {
        for fixed in ["git", "edition", "audit"] {
            let bad = format!(
                "{FIXTURE_POLICY}\n[[destination]]\nkind = \"{fixed}\"\nallow = [\"public\"]\n"
            );
            let err = Policy::parse(&bad, None).unwrap_err();
            assert_eq!(err.code, Code::Fb0413);
            assert!(
                err.key_path.ends_with(".kind"),
                "unexpected path: {}",
                err.key_path
            );
        }
    }

    #[test]
    fn a_store_row_for_corpus_is_invalid() {
        let bad = format!(
            "{FIXTURE_POLICY}\n[[store]]\nid = \"corpus\"\nholds = [\"public\"]\ndefault = [\"np-0007\"]\n"
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert!(err.key_path.ends_with(".id"));
    }

    #[test]
    fn an_undefined_label_is_invalid() {
        // 1. In destination `allow`
        let bad_allow = FIXTURE_POLICY.replace(
            "allow = [\"public\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]\nref = \"CHG-0002\"",
            "allow = [\"public\", \"unk-9999\"]\nref = \"CHG-0002\"",
        );
        let err = Policy::parse(&bad_allow, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[1].allow");
        assert_eq!(err.label_id, Some(lid("unk-9999")));

        // 2. In store `holds`
        let bad_holds = FIXTURE_POLICY.replace(
            "holds = [\"public\", \"unlabelled\", \"np-0007\", \"bid-0003\"]",
            "holds = [\"public\", \"unlabelled\", \"unk-9999\"]",
        );
        let err = Policy::parse(&bad_holds, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "store[0].holds");
        assert_eq!(err.label_id, Some(lid("unk-9999")));

        // 3. In source `default`
        let bad_default = FIXTURE_POLICY.replace(
            "root = \"/srv/company/finance\"\ndefault = [\"bid-0003\"]",
            "root = \"/srv/company/finance\"\ndefault = [\"unk-9999\"]",
        );
        let err = Policy::parse(&bad_default, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "source[0].default");
        assert_eq!(err.label_id, Some(lid("unk-9999")));

        // 4. In marking `labels`
        let bad_labels = FIXTURE_POLICY.replace(
            "match = \"NP-0007 NDA\"\nlabels = [\"np-0007\"]",
            "match = \"NP-0007 NDA\"\nlabels = [\"unk-9999\"]",
        );
        let err = Policy::parse(&bad_labels, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "marking[0].labels");
        assert_eq!(err.label_id, Some(lid("unk-9999")));
    }

    #[test]
    fn a_company_label_must_be_opaque() {
        let bad = FIXTURE_POLICY.replace("id = \"ctl-0002\"", "id = \"internal-use\"");
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "label[2].id");
    }

    #[test]
    fn a_duplicate_or_builtin_id_is_invalid() {
        // Built-in id in [[label]]
        let bad_builtin = FIXTURE_POLICY.replace("id = \"ctl-0002\"", "id = \"public\"");
        let err = Policy::parse(&bad_builtin, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "label[2].id");

        // Duplicate label id
        let bad_dup = FIXTURE_POLICY.replace("id = \"ctl-0002\"", "id = \"np-0007\"");
        let err = Policy::parse(&bad_dup, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "label[2].id");
    }

    #[test]
    fn owner_supplied_is_refused_everywhere_but_state_holds() {
        // In overlay holds -> invalid
        let bad_overlay = FIXTURE_POLICY.replace(
            "id = \"overlay\"\nholds = [\"public\", \"unlabelled\", \"np-0007\", \"bid-0003\"]",
            "id = \"overlay\"\nholds = [\"public\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]",
        );
        let err = Policy::parse(&bad_overlay, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "store[0].holds");
        assert_eq!(err.label_id, Some(OWNER_SUPPLIED));

        // In marking labels -> invalid
        let bad_marking = FIXTURE_POLICY.replace(
            "match = \"NP-0007 NDA\"\nlabels = [\"np-0007\"]",
            "match = \"NP-0007 NDA\"\nlabels = [\"owner-supplied\"]",
        );
        let err = Policy::parse(&bad_marking, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "marking[0].labels");

        // In source default -> invalid
        let bad_source = FIXTURE_POLICY.replace(
            "root = \"/srv/company/finance\"\ndefault = [\"bid-0003\"]",
            "root = \"/srv/company/finance\"\ndefault = [\"owner-supplied\"]",
        );
        let err = Policy::parse(&bad_source, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "source[0].default");

        // In state default -> invalid
        let bad_state_def = FIXTURE_POLICY.replace(
            "id = \"state\"\nholds = [\"public\", \"unlabelled\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]",
            "id = \"state\"\nholds = [\"public\", \"unlabelled\", \"owner-supplied\", \"np-0007\", \"bid-0003\"]\ndefault = [\"owner-supplied\"]",
        );
        let err = Policy::parse(&bad_state_def, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "store[2].default");
    }

    #[test]
    fn a_restricted_label_needs_readers() {
        let bad = FIXTURE_POLICY.replace("readers = 'EXAMPLE\\Fieldbook-Readers'\n", "");
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "store[0].holds");
        assert_eq!(err.label_id, Some(lid("np-0007")));
    }

    #[test]
    fn readers_changers_and_hold_are_invalid_on_corpus() {
        let bad = format!(
            "{FIXTURE_POLICY}\n[[store]]\nid = \"corpus\"\nholds = [\"public\"]\ndefault = [\"np-0007\"]\nreaders = \"CORP\\\\Readers\"\n"
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert!(err.key_path.starts_with("store["));
    }

    #[test]
    fn default_outside_holds_is_invalid() {
        let bad = FIXTURE_POLICY.replace(
            "id = \"working\"\nholds = [\"public\", \"unlabelled\", \"bid-0003\"]",
            "id = \"working\"\nholds = [\"public\", \"unlabelled\", \"bid-0003\"]\ndefault = [\"ctl-0002\"]",
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "store[1].default");
        assert_eq!(err.label_id, Some(lid("ctl-0002")));
    }

    #[test]
    fn a_source_needs_an_absolute_root_or_drop() {
        let bad =
            FIXTURE_POLICY.replace("root = \"/srv/company/finance\"", "root = \"relative/erp\"");
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "source[0].root");

        // `drop` is accepted
        let ok_drop = FIXTURE_POLICY.replace("root = \"/srv/company/finance\"", "root = \"drop\"");
        assert!(Policy::parse(&ok_drop, None).is_ok());
    }

    #[test]
    fn mcp_and_clipboard_allow_public_only() {
        let bad_mcp = FIXTURE_POLICY.replace(
            "kind = \"mcp\"\nallow = [\"public\"]",
            "kind = \"mcp\"\nallow = [\"public\", \"np-0007\"]",
        );
        let err = Policy::parse(&bad_mcp, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[2].allow");
        assert_eq!(err.label_id, Some(lid("np-0007")));

        let bad_clip = FIXTURE_POLICY.replace(
            "kind = \"clipboard\"\nallow = [\"public\"]",
            "kind = \"clipboard\"\nallow = [\"public\", \"np-0007\"]",
        );
        let err = Policy::parse(&bad_clip, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[7].allow");
        assert_eq!(err.label_id, Some(lid("np-0007")));
    }

    #[test]
    fn a_model_row_with_a_company_label_needs_an_audit_sink_and_limit() {
        let no_audit = FIXTURE_POLICY.replace(
            "[audit]\nsink = \"/srv/company/audit\"\nmax_unshipped_hours = 24\n",
            "",
        );
        let err = Policy::parse(&no_audit, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "audit.sink");

        let zero_limit =
            FIXTURE_POLICY.replace("max_unshipped_hours = 24", "max_unshipped_hours = 0");
        let err = Policy::parse(&zero_limit, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "audit.max_unshipped_hours");
    }

    // control: EG-02
    #[test]
    fn a_missing_require_fresh_export_means_on() {
        // Positive: fixture has `[audit]` with no `require_fresh_export` and reads as `true`.
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        assert!(p.audit().require_fresh_export);

        // And the model row of that policy is still invalid without `max_unshipped_hours`.
        let no_limit = FIXTURE_POLICY.replace("max_unshipped_hours = 24\n", "");
        let err = Policy::parse(&no_limit, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "audit.max_unshipped_hours");
    }

    #[test]
    fn with_the_requirement_off_a_model_row_needs_no_sink_or_limit() {
        let off = FIXTURE_POLICY.replace(
            "[audit]\nsink = \"/srv/company/audit\"\nmax_unshipped_hours = 24\n",
            "[audit]\nrequire_fresh_export = false\n",
        );
        let p = Policy::parse(&off, None).expect("policy with require_fresh_export = false");
        assert!(!p.audit().require_fresh_export);
        assert!(p.audit().sink.is_none());
        assert!(p.audit().max_unshipped_hours.is_none());
    }

    #[test]
    fn a_non_boolean_require_fresh_export_is_invalid() {
        let str_val = format!("{FIXTURE_POLICY}require_fresh_export = \"no\"\n");
        let err = Policy::parse(&str_val, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "audit.require_fresh_export");

        let int_val = format!("{FIXTURE_POLICY}require_fresh_export = 0\n");
        let err = Policy::parse(&int_val, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "audit.require_fresh_export");
    }

    #[test]
    fn a_model_row_needs_an_https_or_loopback_url() {
        let bad_http = FIXTURE_POLICY.replace(
            "url = \"https://ai-gateway.example/v1\"",
            "url = \"http://ai-gateway.example/v1\"",
        );
        let err = Policy::parse(&bad_http, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[0].url");

        let ok_loopback = FIXTURE_POLICY.replace(
            "url = \"https://ai-gateway.example/v1\"",
            "url = \"http://127.0.0.1:8080/v1\"",
        );
        assert!(Policy::parse(&ok_loopback, None).is_ok());
    }

    #[test]
    fn paths_only_on_export_and_backup() {
        let bad = FIXTURE_POLICY.replace(
            "kind = \"print\"\nallow = [\"public\", \"owner-supplied\", \"bid-0003\"]",
            "kind = \"print\"\nallow = [\"public\", \"owner-supplied\", \"bid-0003\"]\npaths = [\"/srv/print\"]",
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[6].paths");
    }

    #[test]
    fn a_windows_or_posix_absolute_path_is_valid_on_every_host() {
        let with_win = FIXTURE_POLICY.replace(
            "paths = [\"/srv/company/exports\"]",
            "paths = [\"/srv/company/exports\", \"C:\\\\Exports\\\\Fieldbook\", \"\\\\\\\\fileserver\\\\exports\"]",
        );
        assert!(Policy::parse(&with_win, None).is_ok());
    }

    #[test]
    fn a_relative_or_dotdot_path_is_invalid() {
        let rel = FIXTURE_POLICY.replace(
            "paths = [\"/srv/company/exports\"]",
            "paths = [\"exports\"]",
        );
        let err = Policy::parse(&rel, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[4].paths");

        let dotdot = FIXTURE_POLICY.replace(
            "paths = [\"/srv/company/exports\"]",
            "paths = [\"/srv/company/exports/../secret\"]",
        );
        let err = Policy::parse(&dotdot, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[4].paths");
    }

    #[test]
    fn an_expires_in_quotes_is_invalid() {
        let bad = FIXTURE_POLICY.replace("expires = 2027-06-30", "expires = \"2027-06-30\"");
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[0].expires");
    }

    #[test]
    fn clients_only_on_mcp() {
        let bad = FIXTURE_POLICY.replace(
            "kind = \"bridge\"\nallow = [\"public\", \"bid-0003\"]",
            "kind = \"bridge\"\nallow = [\"public\", \"bid-0003\"]\nclients = [\"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"]",
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "destination[3].clients");
    }

    #[test]
    fn an_example_file_is_invalid() {
        let bad = format!("example = true\n{FIXTURE_POLICY}");
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "example");
    }

    #[test]
    fn unknown_key_table_or_schema_is_invalid() {
        let bad_key = format!("mystery_key = 1\n{FIXTURE_POLICY}");
        let err = Policy::parse(&bad_key, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);

        let bad_schema = FIXTURE_POLICY.replace("policy_schema = 1", "policy_schema = 2");
        let err = Policy::parse(&bad_schema, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "policy_schema");
    }

    #[test]
    fn a_lower_revision_is_invalid() {
        let err = Policy::parse(FIXTURE_POLICY, Some(4)).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "revision");
    }

    #[test]
    fn an_equal_or_higher_revision_is_valid() {
        assert!(Policy::parse(FIXTURE_POLICY, Some(3)).is_ok());
        assert!(Policy::parse(FIXTURE_POLICY, Some(2)).is_ok());
    }

    #[test]
    fn oversize_or_non_utf8_is_invalid() {
        let oversize = "a".repeat((1024 * 1024) + 1);
        let err = Policy::parse(&oversize, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);

        let non_utf8 = [0xff_u8, 0xfe, 0xfd];
        let err = Policy::parse_bytes(&non_utf8, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
    }

    #[test]
    fn a_byte_order_mark_is_skipped_but_hashed() {
        let without_bom = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let with_bom_text = format!("\u{FEFF}{FIXTURE_POLICY}");
        let with_bom = Policy::parse(&with_bom_text, None).unwrap();

        assert_eq!(without_bom.revision(), with_bom.revision());
        assert_eq!(without_bom.labels(), with_bom.labels());
        assert_eq!(without_bom.destinations(), with_bom.destinations());
        assert_eq!(without_bom.stores(), with_bom.stores());
        assert_eq!(without_bom.audit(), with_bom.audit());

        let hash_plain = policy_hash(&PolicyState::Loaded(without_bom));
        let hash_bom = policy_hash(&PolicyState::Loaded(with_bom));
        assert_ne!(hash_plain, hash_bom);

        let expected_bom_hash = fieldbook_core::hash::sha256_hex(with_bom_text.as_bytes());
        assert_eq!(hash_bom, expected_bom_hash);
    }

    #[test]
    fn two_byte_order_marks_are_invalid() {
        let two_boms = format!("\u{FEFF}\u{FEFF}{FIXTURE_POLICY}");
        let err = Policy::parse(&two_boms, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
    }

    #[test]
    fn errors_never_print_a_title() {
        let bad = FIXTURE_POLICY.replace("id = \"ctl-0002\"", "id = \"not-opaque\"");
        let err = Policy::parse(&bad, None).unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("Supplier terms under an NDA"));
        assert!(!msg.contains("NP-0007: restricted"));
        assert!(!msg.contains("Export-controlled technical data"));
        assert!(!msg.contains("CTL-0002: controlled"));
        assert!(!msg.contains("NP-0007 NDA"));
    }

    // -------------------------------------------------------------------------
    // Step 21 & Step 22: Public-only default/marking rejection and empty arrays
    // -------------------------------------------------------------------------

    #[test]
    fn store_default_cannot_grant_public() {
        let bad_explicit = FIXTURE_POLICY.replace(
            "id = \"working\"\nholds = [\"public\", \"unlabelled\", \"bid-0003\"]",
            "id = \"working\"\nholds = [\"public\", \"unlabelled\", \"bid-0003\"]\ndefault = [\"public\"]",
        );
        let err = Policy::parse(&bad_explicit, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "store[1].default");
        assert_eq!(err.label_id, Some(PUBLIC));

        // Valid root-bound public `[[source]]` row still parses.
        let ok_public_source = format!(
            "{FIXTURE_POLICY}\n[[source]]\nid = \"pub-mirror\"\nroot = \"/srv/drop/public\"\ndefault = [\"public\"]\n"
        );
        assert!(Policy::parse(&ok_public_source, None).is_ok());
    }

    #[test]
    fn marking_rule_cannot_grant_public() {
        let bad = FIXTURE_POLICY.replace(
            "match = \"NP-0007 NDA\"\nlabels = [\"np-0007\"]",
            "match = \"NP-0007 NDA\"\nlabels = [\"public\"]",
        );
        let err = Policy::parse(&bad, None).unwrap_err();
        assert_eq!(err.code, Code::Fb0413);
        assert_eq!(err.key_path, "marking[0].labels");
        assert_eq!(err.label_id, Some(PUBLIC));

        // And the labeller independently refuses to turn non-public act into `public`.
        let rules = Policy::parse(FIXTURE_POLICY, None).unwrap().marking_rules();
        let set = for_text("Plain text with no banner", &rules, &LabelContext::none());
        assert_eq!(set, LabelSet::unlabelled());
    }

    #[test]
    fn empty_permission_arrays_parse_and_allow_nothing() {
        let text = r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Synthetic Non-Public"
banner = "SYNTHETIC NON-PUBLIC"

[[store]]
id = "state"
holds = []

[[destination]]
kind = "handoff"
allow = []
"#;
        let p = Policy::parse(text, None).expect("explicit empty allow=[] and holds=[] must parse");
        let state_rule = p.store_rule(&StoreId::State);
        assert!(state_rule.holds.is_empty());

        for set in [
            LabelSet::public(),
            LabelSet::unlabelled(),
            LabelSet::from_ids([lid("np-0007")]),
        ] {
            assert!(admit(&set, &state_rule).is_err());
        }

        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &OwnerSettings::unset());
        assert!(eff.allow_set(&Dest::Handoff).is_empty());
        for set in [
            LabelSet::public(),
            LabelSet::unlabelled(),
            LabelSet::from_ids([lid("np-0007")]),
        ] {
            let dec = check(
                &eff,
                &Dest::Handoff,
                Some(&set),
                None,
                now_2026_11_01(),
                Shipped::Hours(0),
            );
            assert!(matches!(dec, Decision::Denied(_)));
        }

        let bad_default = format!("{text}default = [\"np-0007\"]\n");
        assert!(Policy::parse(&bad_default, None).is_err());

        let missing_holds = r#"
policy_schema = 1
revision = 1
[[store]]
id = "state"
"#;
        assert!(Policy::parse(missing_holds, None).is_err());

        let missing_allow = r#"
policy_schema = 1
revision = 1
[[destination]]
kind = "handoff"
"#;
        assert!(Policy::parse(missing_allow, None).is_err());
    }

    // -------------------------------------------------------------------------
    // Step 3: Floor policy tests
    // -------------------------------------------------------------------------

    #[test]
    fn store_rule_is_the_row_the_floor_or_corpus() {
        let text = r#"
policy_schema = 1
revision = 1

[[label]]
id = "np-0007"
title = "Synthetic Non-Public"
banner = "SYNTHETIC NON-PUBLIC"

[[store]]
id = "overlay"
holds = ["public", "np-0007"]
default = ["np-0007"]
"#;
        let p = Policy::parse(text, None).unwrap();
        let overlay = p.store_rule(&StoreId::Overlay);
        assert!(overlay.holds.contains(&lid("np-0007")));
        assert!(!overlay.holds.contains(&UNLABELLED));

        let working = p.store_rule(&StoreId::Working);
        assert_eq!(working, StoreRule::floor(StoreId::Working, false));
        assert!(!working.holds.contains(&MARKED));

        let corpus = p.store_rule(&StoreId::Corpus);
        assert_eq!(corpus, StoreRule::corpus());
        assert_eq!(corpus.holds, LabelAllow::from_ids([PUBLIC]));
    }

    #[test]
    fn floor_a_allows_public_and_owner_supplied_to_handoff() {
        let eff = EffectivePolicy::floor(false);
        assert_eq!(
            eff.allow_set(&Dest::Handoff),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED])
        );
    }

    #[test]
    fn floor_a_model_row_allows_public_only() {
        let floor_a = Policy::floor(FloorKind::A);
        let model_row = floor_a
            .destinations()
            .iter()
            .find(|d| d.kind == DestKind::Model)
            .unwrap();
        assert_eq!(model_row.allow, LabelAllow::from_ids([PUBLIC]));
    }

    #[test]
    fn floor_a_export_and_backup_allow_public_and_owner_supplied() {
        let eff = EffectivePolicy::floor(false);
        assert_eq!(
            eff.allow_set(&Dest::Export {
                path: "/tmp/out".into()
            }),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED])
        );
        assert_eq!(
            eff.allow_set(&Dest::Backup {
                path: "/tmp/bk".into(),
                company_storage: false,
            }),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED])
        );
    }

    #[test]
    fn floor_b_sends_nothing_to_a_model() {
        let floor_b = Policy::floor(FloorKind::B);
        let model_row = floor_b
            .destinations()
            .iter()
            .find(|d| d.kind == DestKind::Model)
            .unwrap();
        assert!(model_row.allow.is_empty());
    }

    #[test]
    fn floor_b_refuses_export() {
        let eff = EffectivePolicy::floor(true);
        let export_dest = Dest::Export {
            path: "/tmp/out".into(),
        };
        assert!(eff.allow_set(&export_dest).is_empty());
        let dec = check(
            &eff,
            &export_dest,
            Some(&LabelSet::public()),
            None,
            now_2026_11_01(),
            Shipped::Hours(0),
        );
        assert!(matches!(dec, Decision::Denied(_)));
    }

    #[test]
    fn floor_b_stores_hold_marked_and_floor_a_stores_do_not() {
        let floor_a = Policy::floor(FloorKind::A);
        let floor_b = Policy::floor(FloorKind::B);
        for store in [StoreId::Overlay, StoreId::Working, StoreId::State] {
            assert!(!floor_a.store_rule(&store).holds.contains(&MARKED));
            assert!(floor_b.store_rule(&store).holds.contains(&MARKED));
        }
        assert!(!floor_b.store_rule(&StoreId::Corpus).holds.contains(&MARKED));
    }

    #[test]
    fn no_floor_row_allows_marked_to_any_destination() {
        for kind in [FloorKind::A, FloorKind::B] {
            let p = Policy::floor(kind);
            for row in p.destinations() {
                assert!(
                    !row.allow.contains(&MARKED),
                    "floor {kind:?} destination {:?} must not allow `marked`",
                    row.kind
                );
            }
        }
    }

    #[test]
    fn a_loaded_policy_gives_an_unlisted_store_no_marked() {
        let text = r#"
policy_schema = 1
revision = 1
"#;
        let p = Policy::parse(text, None).unwrap();
        for store in [StoreId::Overlay, StoreId::Working, StoreId::State] {
            assert!(!p.store_rule(&store).holds.contains(&MARKED));
        }
    }

    #[test]
    fn floor_b_backup_never_holds_marked() {
        let floor_b = Policy::floor(FloorKind::B);
        let backup = floor_b
            .destinations()
            .iter()
            .find(|d| d.kind == DestKind::Backup)
            .unwrap();
        assert!(!backup.allow.contains(&MARKED));
    }

    #[test]
    fn floor_b_backup_needs_the_recorded_target() {
        let eff = EffectivePolicy::floor(true);
        let unrecorded = Dest::Backup {
            path: "/tmp/bk".into(),
            company_storage: false,
        };
        let recorded = Dest::Backup {
            path: "/tmp/bk".into(),
            company_storage: true,
        };
        assert!(eff.allow_set(&unrecorded).is_empty());
        assert_eq!(
            eff.allow_set(&recorded),
            LabelAllow::from_ids([PUBLIC, UNLABELLED, OWNER_SUPPLIED])
        );
    }

    #[test]
    fn floor_hash_is_the_hash_of_the_text() {
        let expected = GOLDEN_FLOOR_HASH.trim();
        assert_eq!(policy_hash(&PolicyState::Floor(FloorKind::A)), expected);
        assert_eq!(policy_hash(&PolicyState::Floor(FloorKind::B)), expected);
    }

    #[test]
    fn a_loaded_policy_hash_is_the_hash_of_its_bytes() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let expected = fieldbook_core::hash::sha256_hex(FIXTURE_POLICY.as_bytes());
        assert_eq!(policy_hash(&PolicyState::Loaded(p)), expected);
    }

    #[test]
    fn floor_sentence_matches_the_reference() {
        let line = POLICY_REF_MD
            .lines()
            .find_map(|l| l.strip_prefix("> **Floor.** "))
            .expect("docs/reference/policy.md must contain `> **Floor.** `");
        assert_eq!(Policy::floor_sentence(), line);
    }

    #[test]
    fn floor_sentence_groups_kinds_by_label_set() {
        let mut floor_a = Policy::floor(FloorKind::A);
        let floor_b = Policy::floor(FloorKind::B);
        floor_a.set_dest_allow(
            DestKind::Print,
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED]),
        );
        let sentence = Policy::floor_sentence_from(&floor_a, &floor_b);
        assert!(
            sentence.contains(
                "Handoff, exports, backups and print receive `public` and `owner-supplied`."
            ),
            "unexpected sentence: {sentence}"
        );
    }

    #[test]
    fn floor_sentence_changes_when_floor_b_changes() {
        let floor_a = Policy::floor(FloorKind::A);
        let mut floor_b_export = Policy::floor(FloorKind::B);
        floor_b_export.set_dest_allow(
            DestKind::Export,
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED]),
        );
        let s1 = Policy::floor_sentence_from(&floor_a, &floor_b_export);
        assert!(!s1.contains("exports are refused"), "got: {s1}");

        let mut floor_b_working = Policy::floor(FloorKind::B);
        floor_b_working
            .set_store_holds(StoreId::Working, LabelAllow::from_ids([PUBLIC, UNLABELLED]));
        let s2 = Policy::floor_sentence_from(&floor_a, &floor_b_working);
        assert!(
            s2.contains("the stores `overlay` and `state` also hold `marked`"),
            "got: {s2}"
        );
    }

    #[test]
    fn an_empty_permission_intersection_stays_empty() {
        let a = LabelAllow::from_ids([PUBLIC]);
        let b = LabelAllow::from_ids([lid("np-0007")]);
        let inter = merge_list(&a, &b);
        assert!(inter.is_empty());
    }

    #[test]
    fn public_and_company_permissions_both_survive() {
        let a = LabelAllow::from_ids([PUBLIC, lid("np-0007"), lid("bid-0003")]);
        let b = LabelAllow::from_ids([PUBLIC, lid("np-0007")]);
        let inter = merge_list(&a, &b);
        assert!(inter.contains(&PUBLIC));
        assert!(inter.contains(&lid("np-0007")));
        assert!(!inter.contains(&lid("bid-0003")));
    }

    // -------------------------------------------------------------------------
    // Step 4: PolicyState tests
    // -------------------------------------------------------------------------

    #[test]
    fn closed_denies_all_but_audit() {
        for why in [
            ClosedWhy::Absent,
            ClosedWhy::Invalid,
            ClosedWhy::LowerRevision,
            ClosedWhy::Writable,
        ] {
            let eff = EffectivePolicy::new(&PolicyState::Closed(why), &OwnerSettings::unset());
            let audit_dec = check(
                &eff,
                &Dest::Audit,
                None,
                None,
                now_2026_11_01(),
                Shipped::Hours(0),
            );
            assert!(
                matches!(audit_dec, Decision::Allowed(_)),
                "Dest::Audit must be allowed even when Closed({why:?})"
            );

            for dest in [
                Dest::Handoff,
                Dest::Mcp {
                    client: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                        .to_owned(),
                },
                Dest::Bridge,
                Dest::Print,
                Dest::Clipboard,
                Dest::Git,
                Dest::Edition,
                Dest::Export {
                    path: "/srv/company/exports/out.html".into(),
                },
                Dest::Backup {
                    path: "/srv/company/backups/snap".into(),
                    company_storage: true,
                },
                Dest::Model {
                    name: "approved-gateway".to_owned(),
                    base_url: "https://ai-gateway.example/v1".to_owned(),
                },
            ] {
                let dec = check(
                    &eff,
                    &dest,
                    Some(&LabelSet::public()),
                    None,
                    now_2026_11_01(),
                    Shipped::Hours(0),
                );
                match dec {
                    Decision::Denied(d) => assert_eq!(d.code, Code::Fb0413),
                    Decision::Allowed(_) => panic!("Closed({why:?}) must deny {dest:?}"),
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // Step 5: OwnerSettings & EffectivePolicy merge tests
    // -------------------------------------------------------------------------

    #[test]
    fn effective_floor_is_the_floor_of_the_machine() {
        let home = EffectivePolicy::floor(false);
        let laptop = EffectivePolicy::floor(true);

        assert_eq!(
            home.allow_set(&Dest::Handoff),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED])
        );
        assert_eq!(
            laptop.allow_set(&Dest::Handoff),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED])
        );

        let recorded_backup = Dest::Backup {
            path: "/srv/company/backups/snap".into(),
            company_storage: true,
        };
        assert_eq!(
            home.allow_set(&recorded_backup),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED])
        );
        assert_eq!(
            laptop.allow_set(&recorded_backup),
            LabelAllow::from_ids([PUBLIC, UNLABELLED, OWNER_SUPPLIED])
        );
    }

    #[test]
    fn a_list_intersects() {
        let p = LabelAllow::from_ids([PUBLIC, lid("np-0007"), lid("bid-0003")]);
        let o = LabelAllow::from_ids([PUBLIC, lid("bid-0003")]);
        assert_eq!(
            merge_list(&p, &o),
            LabelAllow::from_ids([PUBLIC, lid("bid-0003")])
        );
    }

    // control: PL-02
    #[test]
    fn the_owner_cannot_widen() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let mut owner = OwnerSettings::unset();
        owner.handoff = Some(LabelAllow::from_ids([
            PUBLIC,
            lid("np-0007"),
            lid("bid-0003"),
            lid("ctl-0002"),
        ]));
        owner.endpoints.insert(
            "approved-gateway".to_owned(),
            LabelAllow::from_ids([PUBLIC, lid("np-0007"), lid("bid-0003"), lid("ctl-0002")]),
        );

        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &owner);
        // Policy handoff allows public, owner-supplied, np-0007, bid-0003 (not ctl-0002).
        let handoff_allow = eff.allow_set(&Dest::Handoff);
        assert!(!handoff_allow.contains(&lid("ctl-0002")));

        // Policy model row allows public, owner-supplied, bid-0003 (not np-0007 or ctl-0002).
        let model_allow = eff.allow_set(&Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        });
        assert!(!model_allow.contains(&lid("np-0007")));
        assert!(!model_allow.contains(&lid("ctl-0002")));
    }

    #[test]
    fn absent_handoff_and_mcp_lists_do_not_narrow() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &OwnerSettings::unset());
        assert_eq!(
            eff.allow_set(&Dest::Handoff),
            LabelAllow::from_ids([PUBLIC, OWNER_SUPPLIED, lid("np-0007"), lid("bid-0003")])
        );
        assert_eq!(
            eff.allow_set(&Dest::Mcp {
                client: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                    .to_owned(),
            }),
            LabelAllow::from_ids([PUBLIC])
        );
    }

    #[test]
    fn a_switch_is_and() {
        assert!(merge_switch(true, true));
        assert!(!merge_switch(true, false));
        assert!(!merge_switch(false, true));
        assert!(!merge_switch(false, false));
    }

    #[test]
    fn a_number_is_the_minimum() {
        assert_eq!(merge_number(30, 14), 14);
        assert_eq!(merge_number(7, 14), 7);
        assert_eq!(merge_number(14, 14), 14);
    }

    #[test]
    fn an_endpoint_without_an_owner_list_accepts_nothing() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &OwnerSettings::unset());
        let dest = Dest::Model {
            name: "approved-gateway".to_owned(),
            base_url: "https://ai-gateway.example/v1".to_owned(),
        };
        assert!(eff.allow_set(&dest).is_empty());
    }

    #[test]
    fn explain_names_the_side_that_cut() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let mut owner = OwnerSettings::unset();
        // Policy handoff allows `public`, `owner-supplied`, `np-0007`, `bid-0003` (so policy cut `ctl-0002`).
        // Owner narrows handoff to `public` only (so owner cut `owner-supplied`, `np-0007`, `bid-0003`).
        owner.handoff = Some(LabelAllow::from_ids([PUBLIC]));
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &owner);
        let cuts = eff.explain(&Dest::Handoff);

        let cut_of = |id: &LabelId| {
            cuts.iter()
                .find(|c| &c.label == id)
                .map(|c| c.by)
                .expect("label must be in cuts")
        };
        assert_eq!(cut_of(&lid("np-0007")), CutBy::Owner);
        assert_eq!(cut_of(&lid("bid-0003")), CutBy::Owner);
        assert_eq!(cut_of(&OWNER_SUPPLIED), CutBy::Owner);
        assert_eq!(cut_of(&lid("ctl-0002")), CutBy::Policy);
    }

    #[test]
    fn allow_set_is_the_narrowed_set() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
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
        assert_eq!(
            eff.allow_set(&dest),
            LabelAllow::from_ids([PUBLIC, lid("bid-0003")])
        );
    }

    #[test]
    fn a_banner_comes_from_the_label_row() {
        let p = Policy::parse(FIXTURE_POLICY, None).unwrap();
        let eff = EffectivePolicy::new(&PolicyState::Loaded(p), &OwnerSettings::unset());
        assert_eq!(
            eff.banner(&lid("np-0007")),
            Some("NP-0007: restricted to the named counterparty")
        );
        assert_eq!(eff.banner(&PUBLIC), None);
    }

    #[test]
    fn key_table_matches_the_policy_reference() {
        let section = POLICY_REF_MD
            .split("## Narrow only: the owner's settings")
            .nth(1)
            .and_then(|s| s.split("## Destination kinds").next())
            .expect("Merge section must exist in docs/reference/policy.md");

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
            if cells.len() != 3 || cells[0] == "Settings key" || cells[0].starts_with("---") {
                continue;
            }
            let key = cells[0].trim_matches('`');
            let kind = match cells[1] {
                "List: intersect" => MergeKind::List,
                "Switch: AND" => MergeKind::Switch,
                "Number: minimum" => MergeKind::Number,
                "A word list: the policy decides which entries are usable" => MergeKind::Word,
                other => panic!("unexpected rule in reference table: {other}"),
            };
            doc_rows.push((key, kind, cells[2]));
        }

        assert_eq!(doc_rows.len(), KEY_TABLE.len());
        for (row, (doc_key, doc_kind, doc_side)) in KEY_TABLE.iter().zip(doc_rows) {
            assert_eq!(row.key, doc_key);
            assert_eq!(row.kind, doc_kind);
            assert_eq!(row.policy_side, doc_side);
        }
    }
}
