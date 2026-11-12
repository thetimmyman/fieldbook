//! Unit and property tests for pure AI ports types (`ai-kernel` step 10).

#[path = "../../../tests/support/rng.rs"]
mod rng;

use std::error::Error;

use fieldbook_core::audit::{Event, Sha256};
use fieldbook_core::labels::{ItemRef, LabelId, LabelSet, StoreId};
use fieldbook_core::ports::{
    Fb0917, HandoffId, HandoffTicket, Limits, ModelRequest, Part, PartKind, ProvenanceRef, Purpose,
    Template, verify_provenance,
};

const SAMPLE_AI_CALL: &str = r#"{"seq":10,"time":"2026-03-01T12:00:00Z","kind":"ai.call","actor":"local","role":"lead","store":"corpus","via":"cli","tx":9,"outcome":"ok","attempts":1,"output_sha256":"5acd3c636094184230526091a7fa1b53dbcd20ddc1de6fcadbc4d9233ff12633","tokens_in":120,"tokens_out":45,"hash":"52eb0b0216adb04f1b3c90b4a4b3563fbd18c0f2d1325d5a81b40dd5be5176a7"}"#;

const SAMPLE_AI_PASTE: &str = r#"{"seq":12,"time":"2026-03-01T12:00:00Z","kind":"ai.paste","actor":"local","role":"lead","store":"corpus","via":"cli","pack_seq":11,"output_sha256":"bf9a21db73f9fa27ed0ba82e591c3510be2df6c4c71093a4a5c892a41d76f9e0","labels":["public"],"hash":"9bd71de3c435e431135ceb60ea4d5b83a643b371dc405e903be45b6d18727141"}"#;

#[test]
fn request_labels_are_the_union_of_parts() -> Result<(), Box<dyn Error>> {
    let label_a = LabelId::parse("np-0007")?;
    let label_b = LabelId::parse("bid-0003")?;
    let set_a = LabelSet::from_ids([label_a.clone()]);
    let set_b = LabelSet::from_ids([label_b.clone()]);

    let part_a = Part::new(PartKind::Context, set_a, "Context A", Vec::new());
    let part_b = Part::new(PartKind::Question, set_b, "Question B", Vec::new());

    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![part_a, part_b],
        None,
        Limits::new(100, 10),
    )?;

    let labels = req.labels();
    assert!(labels.contains(&label_a));
    assert!(labels.contains(&label_b));
    Ok(())
}

#[test]
fn an_owner_pasted_part_carries_owner_supplied() {
    let part = Part::owner_typed(PartKind::Question, "User question text");
    assert_eq!(part.labels(), &LabelSet::owner_supplied());
}

#[test]
fn a_part_with_no_label_makes_the_request_unlabelled() -> Result<(), Box<dyn Error>> {
    let part_unlabelled = Part::new(
        PartKind::Context,
        LabelSet::unlabelled(),
        "Context",
        Vec::new(),
    );
    let part_rules = Part::rules("Rules text");

    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![part_rules, part_unlabelled],
        None,
        Limits::new(100, 10),
    )?;

    assert!(req.labels().is_unresolved());
    Ok(())
}

#[test]
fn request_has_no_way_to_drop_a_label() -> Result<(), Box<dyn Error>> {
    let label = LabelId::parse("ctl-0002")?;
    let set = LabelSet::from_ids([label.clone()]);
    let part = Part::new(PartKind::Context, set, "Context with label", Vec::new());

    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![part],
        None,
        Limits::new(100, 10),
    )?;

    assert!(req.labels().contains(&label));
    Ok(())
}

#[test]
fn parts_render_in_fixed_order() -> Result<(), Box<dyn Error>> {
    let p_question = Part::new(
        PartKind::Question,
        LabelSet::owner_supplied(),
        "The Question?",
        Vec::new(),
    );
    let p_context = Part::new(
        PartKind::Context,
        LabelSet::public(),
        "The Context.",
        Vec::new(),
    );
    let p_history = Part::new(
        PartKind::History,
        LabelSet::owner_supplied(),
        "User: Earlier turn\nAssistant: Prior answer",
        Vec::new(),
    );
    let p_rules = Part::rules("Specific Rules.");

    // Supply parts out of order: Question, Context, History, Rules
    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![p_question, p_context, p_history, p_rules],
        None,
        Limits::new(100, 10),
    )?;

    let prompt = req.prompt();
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

    let sys = req.system();
    assert!(sys.contains("Specific Rules."));
    Ok(())
}

#[test]
fn empty_history_has_no_delimiter() -> Result<(), Box<dyn Error>> {
    let p_context = Part::new(PartKind::Context, LabelSet::public(), "Context", Vec::new());
    let p_question = Part::new(
        PartKind::Question,
        LabelSet::owner_supplied(),
        "The question text",
        Vec::new(),
    );

    // Case 1: no History part
    let req1 = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![p_context.clone(), p_question.clone()],
        None,
        Limits::new(100, 10),
    )?;
    assert!(!req1.prompt().contains("HISTORY"));

    // Case 2: History part with only whitespace
    let p_hist_blank = Part::new(
        PartKind::History,
        LabelSet::public(),
        "   \n\t  \n",
        Vec::new(),
    );
    let req2 = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![p_hist_blank, p_context, p_question],
        None,
        Limits::new(100, 10),
    )?;
    assert!(!req2.prompt().contains("HISTORY"));
    Ok(())
}

#[test]
fn history_labels_and_ids_are_kept() -> Result<(), Box<dyn Error>> {
    let id = ItemRef::new(StoreId::Corpus, "CL-AAA-0001");
    let label = LabelId::parse("np-0007")?;
    let p_hist = Part::new(
        PartKind::History,
        LabelSet::from_ids([label.clone()]),
        "User: turn\nAssistant: answer",
        vec![id.clone()],
    );

    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![p_hist],
        None,
        Limits::new(100, 10),
    )?;

    assert!(req.labels().contains(&label));
    assert_eq!(req.input_refs(), vec![id.clone()]);
    assert_eq!(req.context_refs(), vec![id]);
    Ok(())
}

#[test]
fn input_refs_keep_equal_ids_in_different_stores() -> Result<(), Box<dyn Error>> {
    let r_corpus = ItemRef::new(StoreId::Corpus, "SO-0001");
    let r_overlay = ItemRef::new(StoreId::Overlay, "SO-0001");

    let p1 = Part::new(
        PartKind::Context,
        LabelSet::public(),
        "Text 1",
        vec![r_corpus.clone()],
    );
    let p2 = Part::new(
        PartKind::Context,
        LabelSet::public(),
        "Text 2",
        vec![r_overlay.clone()],
    );

    let req = ModelRequest::new(
        Purpose::Ask,
        Template::new("ask@1")?,
        vec![p1, p2],
        None,
        Limits::new(100, 10),
    )?;

    let refs = req.input_refs();
    assert_eq!(refs.len(), 2);
    assert!(refs.contains(&r_corpus));
    assert!(refs.contains(&r_overlay));
    Ok(())
}

#[test]
fn ticket_round_trips() -> Result<(), Box<dyn Error>> {
    let id = HandoffId::parse("H-0001")?;
    assert_eq!(id.format(), "H-0001");
    assert_eq!(id.value(), 1);

    let pack_sha = Sha256::digest(b"pack");
    let ticket = HandoffTicket::new(id, pack_sha.clone());
    assert_eq!(ticket.id.format(), "H-0001");
    assert_eq!(ticket.pack_sha256, pack_sha);
    Ok(())
}

#[test]
fn a_ticket_with_a_bad_form_is_refused() {
    let bad = [
        "H-12", "h-0001", "H-00001", "H-000a", "H-0001 ", "", "H-0000",
    ];
    for input in bad {
        assert!(
            HandoffId::parse(input).is_err(),
            "expected refusal for: {input:?}"
        );
    }
}

#[test]
fn ticket_parse_never_panics_on_random_and_mutated_bytes() {
    let mut r = rng::Rng::new(1);
    for len in 0..=64 {
        for _ in 0..200 {
            let raw = r.bytes(len);
            let text = String::from_utf8_lossy(&raw);
            let _ = HandoffId::parse(&text);
        }
    }

    let base = b"H-0001";
    for idx in 0..base.len() {
        let mut flipped = base.to_vec();
        flipped[idx] ^= 0xFF;
        let s = String::from_utf8_lossy(&flipped);
        let _ = HandoffId::parse(&s);

        let mut deleted = base.to_vec();
        deleted.remove(idx);
        let s = String::from_utf8_lossy(&deleted);
        let _ = HandoffId::parse(&s);
    }
}

#[test]
fn provenance_resolves_only_a_matching_call() -> Result<(), Box<dyn Error>> {
    let (call_event, _) = Event::parse_line(SAMPLE_AI_CALL)?;
    let (paste_event, _) = Event::parse_line(SAMPLE_AI_PASTE)?;

    let call_sha =
        Sha256::parse("5acd3c636094184230526091a7fa1b53dbcd20ddc1de6fcadbc4d9233ff12633")?;
    let paste_sha =
        Sha256::parse("bf9a21db73f9fa27ed0ba82e591c3510be2df6c4c71093a4a5c892a41d76f9e0")?;

    let ref_call = ProvenanceRef::new(10, call_sha.clone());
    let ref_paste = ProvenanceRef::new(12, paste_sha.clone());

    // Matches
    assert!(verify_provenance([&call_event], &ref_call).is_ok());
    assert!(verify_provenance([&paste_event], &ref_paste).is_ok());
    assert!(verify_provenance([&call_event, &paste_event], &ref_call).is_ok());
    assert!(verify_provenance([&call_event, &paste_event], &ref_paste).is_ok());

    // Mismatched sequence number
    let wrong_seq = ProvenanceRef::new(11, call_sha.clone());
    assert_eq!(verify_provenance([&call_event], &wrong_seq), Err(Fb0917));

    // Mismatched output hash
    let wrong_hash = ProvenanceRef::new(10, paste_sha);
    assert_eq!(verify_provenance([&call_event], &wrong_hash), Err(Fb0917));

    // Empty events iterator
    assert_eq!(verify_provenance([], &ref_call), Err(Fb0917));

    Ok(())
}
