//! Contract tests for the pure chat status shapes (`ai-config-and-keys` step 1).

use fieldbook_core::labels::{LabelId, PUBLIC};
use fieldbook_core::ports::{AdapterStatus, Caps, ConnState, KeyState, LastCall};

#[test]
fn adapter_status_round_trips() {
    let with_last = AdapterStatus {
        port: "model".to_owned(),
        adapter: "openai-compatible@1".to_owned(),
        endpoint: Some("work".to_owned()),
        state: ConnState::Ready,
        allow: vec![PUBLIC, LabelId::parse("np-0007").expect("valid label id")],
        key: KeyState::Set,
        caps: Caps::new(true, true, false, false, 200_000),
        last: Some(LastCall {
            outcome: "ok".to_owned(),
            at: "2026-01-01T09:00:00Z".to_owned(),
            attempts: 1,
        }),
    };

    let json = serde_json::to_string_pretty(&with_last).expect("serialize AdapterStatus");
    assert!(
        json.contains("\"streaming\": false"),
        "AdapterStatus.caps must serialize streaming as false: {json}"
    );
    let parsed: AdapterStatus =
        serde_json::from_str(&json).expect("deserialize AdapterStatus with last");
    assert_eq!(parsed, with_last);

    let without_last = AdapterStatus {
        port: "model".to_owned(),
        adapter: "handoff@1".to_owned(),
        endpoint: Some("handoff".to_owned()),
        state: ConnState::Handoff,
        allow: vec![],
        key: KeyState::NotNeeded,
        caps: Caps::new(false, false, false, false, 0),
        last: None,
    };

    let json_no_last =
        serde_json::to_string(&without_last).expect("serialize AdapterStatus without last");
    assert!(
        !json_no_last.contains("\"last\""),
        "last must be omitted when None: {json_no_last}"
    );
    let parsed_no_last: AdapterStatus =
        serde_json::from_str(&json_no_last).expect("deserialize AdapterStatus without last");
    assert_eq!(parsed_no_last, without_last);
}

#[test]
fn connstate_names_are_stable() {
    let cases = [
        (ConnState::Off, r#"{"kind":"Off"}"#),
        (ConnState::Handoff, r#"{"kind":"Handoff"}"#),
        (ConnState::Ready, r#"{"kind":"Ready"}"#),
        (ConnState::Busy, r#"{"kind":"Busy"}"#),
        (
            ConnState::Unavailable {
                code: "FB0912".to_owned(),
            },
            r#"{"kind":"Unavailable","code":"FB0912"}"#,
        ),
        (
            ConnState::Blocked {
                code: "FB0413".to_owned(),
            },
            r#"{"kind":"Blocked","code":"FB0413"}"#,
        ),
    ];

    for (state, expected) in cases {
        let actual = serde_json::to_string(&state).expect("serialize ConnState");
        assert_eq!(actual, expected, "ConnState wire shape drifted");
        let round: ConnState = serde_json::from_str(&actual).expect("deserialize ConnState");
        assert_eq!(round, state);
    }

    assert_eq!(
        serde_json::to_string(&KeyState::Set).expect("Set"),
        "\"Set\""
    );
    assert_eq!(
        serde_json::to_string(&KeyState::Missing).expect("Missing"),
        "\"Missing\""
    );
    assert_eq!(
        serde_json::to_string(&KeyState::NotNeeded).expect("NotNeeded"),
        "\"NotNeeded\""
    );

    let sample = AdapterStatus {
        port: "model".to_owned(),
        adapter: "openai-compatible@1".to_owned(),
        endpoint: Some("work".to_owned()),
        state: ConnState::Ready,
        allow: vec![PUBLIC],
        key: KeyState::Set,
        caps: Caps::new(true, true, false, false, 200_000),
        last: Some(LastCall {
            outcome: "ok".to_owned(),
            at: "2026-01-01T09:00:00Z".to_owned(),
            attempts: 1,
        }),
    };
    let expected_golden = r#"{
  "port": "model",
  "adapter": "openai-compatible@1",
  "endpoint": "work",
  "state": {
    "kind": "Ready"
  },
  "allow": [
    "public"
  ],
  "key": "Set",
  "caps": {
    "network": true,
    "structured": true,
    "embeddings": false,
    "streaming": false,
    "max_prompt_chars": 200000
  },
  "last": {
    "outcome": "ok",
    "at": "2026-01-01T09:00:00Z",
    "attempts": 1
  }
}"#;
    let rendered = serde_json::to_string_pretty(&sample).expect("render sample AdapterStatus");
    assert_eq!(rendered, expected_golden);
}
