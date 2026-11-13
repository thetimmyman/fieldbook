//! Prompt template tests, snapshot verification, and contract checks (`ai-kernel` step 14).

#![expect(
    clippy::disallowed_methods,
    reason = "template snapshot tests read fixture, golden, and reference doc files"
)]

use std::error::Error;
use std::path::Path;

use fieldbook_core::labels::{ItemRef, LabelId, LabelSet, OWNER_SUPPLIED, StoreId};
use fieldbook_core::ports::{
    Limits, ModelRequest, Part, PartKind, Purpose, RequestError, TEMPLATES, Template,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    template: String,
    #[serde(default)]
    part: Vec<FixturePart>,
}

#[derive(Deserialize)]
struct FixturePart {
    kind: String,
    #[serde(default)]
    labels: Vec<String>,
    text: String,
    #[serde(default)]
    ids: Vec<FixtureItemRef>,
}

#[derive(Deserialize)]
struct FixtureItemRef {
    store: String,
    id: String,
}

#[test]
fn templates_have_snapshot_and_doc_row() -> Result<(), Box<dyn Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = manifest_dir.join("../../tests");
    let ports_doc_path = manifest_dir.join("../../docs/reference/ports.md");
    let ports_doc = std::fs::read_to_string(&ports_doc_path)?;

    let start_marker = "<!-- generated:ports -->";
    let end_marker = "<!-- /generated:ports -->";
    let start_idx = ports_doc
        .find(start_marker)
        .ok_or("missing <!-- generated:ports --> marker")?;
    let end_idx = ports_doc
        .find(end_marker)
        .ok_or("missing <!-- /generated:ports --> marker")?;
    let generated_section = &ports_doc[start_idx..end_idx];

    for def in TEMPLATES {
        let golden_path = tests_dir
            .join("golden/prompts")
            .join(format!("{}.txt", def.id));
        assert!(
            golden_path.is_file(),
            "missing golden file: {golden_path:?}"
        );

        let fixture_path = tests_dir
            .join("fixtures/ports/templates")
            .join(format!("{}.toml", def.id));
        assert!(
            fixture_path.is_file(),
            "missing fixture file: {fixture_path:?}"
        );

        assert!(
            generated_section.contains(def.id),
            "ports.md missing row for template: {}",
            def.id
        );
    }
    Ok(())
}

#[test]
fn every_template_renders_its_golden() -> Result<(), Box<dyn Error>> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tests_dir = manifest_dir.join("../../tests");

    for def in TEMPLATES {
        let fixture_path = tests_dir
            .join("fixtures/ports/templates")
            .join(format!("{}.toml", def.id));
        let fixture_toml = std::fs::read_to_string(&fixture_path)?;
        let fixture: Fixture = toml::from_str(&fixture_toml)?;

        let template = Template::new(&fixture.template)?;
        let mut parts = Vec::new();
        for fp in fixture.part {
            let kind = match fp.kind.as_str() {
                "rules" => PartKind::Rules,
                "history" => PartKind::History,
                "context" => PartKind::Context,
                "question" => PartKind::Question,
                "subject" => PartKind::Subject,
                other => return Err(format!("unknown part kind: {other}").into()),
            };

            let mut label_ids = Vec::new();
            for l in fp.labels {
                label_ids.push(LabelId::parse(&l)?);
            }
            let labels = LabelSet::from_ids(label_ids);

            let mut ids = Vec::new();
            for ir in fp.ids {
                let store = StoreId::from_root_name(&ir.store)
                    .ok_or_else(|| format!("unknown store root: {}", ir.store))?;
                ids.push(ItemRef::new(store, ir.id));
            }
            parts.push(Part::new(kind, labels, fp.text, ids));
        }

        let req = ModelRequest::new(
            template.def().purpose,
            template,
            parts,
            None,
            Limits::new(1024, 30),
        )?;

        let mut rendered = req.system();
        rendered.push_str("--- prompt ---\n");
        rendered.push_str(&req.prompt());

        let golden_path = tests_dir
            .join("golden/prompts")
            .join(format!("{}.txt", def.id));
        let expected = std::fs::read_to_string(&golden_path)?;

        assert_eq!(
            rendered, expected,
            "golden mismatch for template {}",
            def.id
        );
    }
    Ok(())
}

#[test]
fn ask_history_is_data_before_context() -> Result<(), Box<dyn Error>> {
    let hist_id = ItemRef::new(StoreId::Corpus, "CL-AAA-0001");
    let hist_label = LabelId::parse("np-0007")?;

    // Two complete turns of conversation
    let p_hist1 = Part::new(
        PartKind::History,
        LabelSet::from_ids([hist_label.clone()]),
        "User: What is a widget crate?\nAssistant: A container for widgets.",
        vec![hist_id.clone()],
    );
    let p_hist2 = Part::new(
        PartKind::History,
        LabelSet::owner_supplied(),
        "User: How many fit inside?\nAssistant: Exactly twelve.",
        Vec::new(),
    );

    let ctx_id = ItemRef::new(StoreId::Corpus, "CL-AAA-0002");
    let p_context = Part::new(
        PartKind::Context,
        LabelSet::public(),
        "cite as: [@CL-AAA-0002] Widget storage standards.",
        vec![ctx_id.clone()],
    );

    let p_question = Part::owner_typed(PartKind::Question, "How many widgets fit in a crate?");

    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![p_hist1, p_hist2, p_context, p_question],
        None,
        Limits::new(1024, 30),
    )?;

    let prompt = req.prompt();

    // Verify quoting of History turns
    assert!(
        prompt.contains("> User: What is a widget crate?\n> Assistant: A container for widgets.")
    );
    assert!(prompt.contains("> User: How many fit inside?\n> Assistant: Exactly twelve."));

    // Verify ordering: History before Context before Question
    let hist_pos = prompt
        .find("-----BEGIN HISTORY")
        .ok_or("missing HISTORY block")?;
    let ctx_pos = prompt
        .find("-----BEGIN CONTEXT")
        .ok_or("missing CONTEXT block")?;
    let q_pos = prompt
        .find("-----BEGIN QUESTION")
        .ok_or("missing QUESTION block")?;

    assert!(hist_pos < ctx_pos);
    assert!(ctx_pos < q_pos);

    // Verify Question block content
    assert!(prompt.contains(
        "-----BEGIN QUESTION-----\nHow many widgets fit in a crate?\n-----END QUESTION-----\n"
    ));

    // Verify labels union
    let labels = req.labels();
    assert!(labels.contains(&hist_label));
    assert!(labels.contains(&OWNER_SUPPLIED));

    // Verify input references
    let input_refs = req.input_refs();
    assert_eq!(input_refs, vec![hist_id, ctx_id]);

    Ok(())
}

#[test]
fn unknown_template_id_is_refused() {
    assert!(Template::new("nope@1").is_err());
    assert!(Template::new("ask@2").is_err());
}

#[test]
fn purpose_mismatch_is_refused() -> Result<(), Box<dyn Error>> {
    let template = Template::new("ask@1")?;
    let err = ModelRequest::new(
        Purpose::Classify,
        template,
        Vec::new(),
        None,
        Limits::new(100, 10),
    );
    assert!(matches!(err, Err(RequestError::PurposeMismatch { .. })));
    Ok(())
}

#[test]
fn part_not_in_template_is_refused() -> Result<(), Box<dyn Error>> {
    let template = Template::new("ask@1")?;
    let subject_part = Part::new(
        PartKind::Subject,
        LabelSet::public(),
        "Subject data",
        Vec::new(),
    );
    let err = ModelRequest::new(
        Purpose::Ask,
        template,
        vec![subject_part],
        None,
        Limits::new(100, 10),
    );
    assert_eq!(err, Err(RequestError::PartNotInTemplate(PartKind::Subject)));
    Ok(())
}
