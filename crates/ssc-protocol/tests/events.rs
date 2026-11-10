use base64::{engine::general_purpose::STANDARD, Engine as _};
use ssc_protocol::{
    crypto::{CryptoCtx, VERSION},
    decode_event, encode_event,
    events::{
        ATTACHMENT_CHUNK_BYTES, MAX_ATTACHMENT_BYTES, MAX_EVENT_FRAME_BYTES, MAX_EVENT_JSON_BYTES,
    },
    handshake::{Handshake, Role},
    message::{chunks_with_id, Reassembler},
    AttachmentKind, ByeReason, ChatEvent, Session, KIND_EVENT,
};
fn id() -> String {
    "07".repeat(16)
}
fn shapes() -> Vec<ChatEvent> {
    vec![
        ChatEvent::Text {
            id: id(),
            text: "Привет 🔒".into(),
        },
        ChatEvent::AttachmentOffer {
            id: id(),
            name: "файл.dat".into(),
            mime: "application/octet-stream".into(),
            size: MAX_ATTACHMENT_BYTES,
            sha256: "ab".repeat(32),
            kind: AttachmentKind::File,
        },
        ChatEvent::AttachmentAccept { id: id() },
        ChatEvent::AttachmentReject { id: id() },
        ChatEvent::AttachmentChunk {
            id: id(),
            offset: MAX_ATTACHMENT_BYTES - 3,
            data: STANDARD.encode([0, 255, 128]),
        },
        ChatEvent::AttachmentProgress {
            id: id(),
            received: MAX_ATTACHMENT_BYTES,
        },
        ChatEvent::AttachmentComplete { id: id() },
        ChatEvent::AttachmentCancel { id: id() },
        ChatEvent::Receipt { id: id() },
        ChatEvent::Reaction {
            target: id(),
            emoji: Some("❤️".into()),
        },
        ChatEvent::Reaction {
            target: id(),
            emoji: None,
        },
        ChatEvent::Bye {
            reason: ByeReason::Expired,
        },
        ChatEvent::Capabilities {
            audio: vec![
                "audio/webm;codecs=opus".into(),
                "audio/mp4".into(),
                "audio/wav".into(),
            ],
        },
    ]
}
fn ready() -> (Session, Session) {
    let (mut a, mut b) = unverified();
    let ar = a.confirm().unwrap();
    let br = b.confirm().unwrap();
    a.receive(&br).unwrap();
    b.receive(&ar).unwrap();
    (a, b)
}
fn unverified() -> (Session, Session) {
    let mut a = Session::new("events", true, [1; 32], [3; 32]).unwrap();
    let mut b = Session::new("events", false, [2; 32], [4; 32]).unwrap();
    let ac = a.start().unwrap().remove(0);
    let bc = b.start().unwrap().remove(0);
    let ar = a.receive(&bc).unwrap().outgoing.remove(0);
    let br = b.receive(&ac).unwrap().outgoing.remove(0);
    a.receive(&br).unwrap();
    b.receive(&ar).unwrap();
    (a, b)
}
fn with_raw_peer(verified: bool) -> (Session, CryptoCtx) {
    let mut a = Session::new("raw-events", true, [1; 32], [3; 32]).unwrap();
    let mut b = Handshake::from_entropy("raw-events", Role::Responder, [2; 32], [4; 32]);
    let ac = a.start().unwrap().remove(0);
    let bc = b.start().unwrap().remove(0);
    let ar = a.receive(&bc).unwrap().outgoing.remove(0);
    let br = b.receive(&ac).unwrap().outgoing.remove(0);
    a.receive(&br).unwrap();
    let mut crypto = b.receive(&ar).unwrap().crypto.unwrap();
    if verified {
        a.confirm().unwrap();
        a.receive(&crypto.encrypt(1, &[]).unwrap()).unwrap();
    }
    (a, crypto)
}
#[test]
fn all_event_shapes_roundtrip_and_keep_fixed_snake_case_wire_tags() {
    for event in shapes() {
        let bytes = encode_event(&event).unwrap();
        assert_eq!(decode_event(&bytes).unwrap(), event);
        assert!(bytes.len() + 26 <= MAX_EVENT_FRAME_BYTES);
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(value["type"].is_string());
    }
    for kind in [
        AttachmentKind::Photo,
        AttachmentKind::Video,
        AttachmentKind::Audio,
        AttachmentKind::File,
    ] {
        let event = ChatEvent::AttachmentOffer {
            id: id(),
            name: "empty".into(),
            mime: "application/octet-stream".into(),
            size: 0,
            sha256: "00".repeat(32),
            kind,
        };
        assert_eq!(decode_event(&encode_event(&event).unwrap()).unwrap(), event);
    }
    assert!(encode_event(&ChatEvent::Capabilities { audio: vec![] }).is_ok());
}
#[test]
fn malformed_types_unknown_fields_duplicate_fields_and_missing_nullable_emoji_are_rejected() {
    for value in [
        format!(r#"{{"type":"receipt","id":"{}","extra":true}}"#, id()),
        format!(r#"{{"type":"receipt","id":"{}","id":"{}"}}"#, id(), id()),
        format!(r#"{{"type":"reaction","target":"{}"}}"#, id()),
        format!(
            r#"{{"type":"attachment_progress","id":"{}","received":-1}}"#,
            id()
        ),
        format!(
            r#"{{"type":"attachment_progress","id":"{}","received":0.5}}"#,
            id()
        ),
        format!(
            r#"{{"type":"attachment_accept","id":"{}","payload":{{}}}}"#,
            id()
        ),
        r#"{"type":"bye","reason":"disconnect"}"#.into(),
        r#"{"type":"unknown"}"#.into(),
        r#"[]"#.into(),
    ] {
        assert!(
            decode_event(value.as_bytes()).is_err(),
            "accepted malformed event"
        );
    }
    assert!(encode_event(&ChatEvent::Receipt { id: "A".repeat(32) }).is_err());
}
#[test]
fn semantic_limits_cover_metadata_chunks_progress_and_reaction_whitelist() {
    let offer =
        |name: String, mime: String, size: u64, sha256: String| ChatEvent::AttachmentOffer {
            id: id(),
            name,
            mime,
            size,
            sha256,
            kind: AttachmentKind::File,
        };
    assert!(encode_event(&offer(
        "🔒".repeat(256),
        "x".repeat(128),
        MAX_ATTACHMENT_BYTES,
        "ab".repeat(32)
    ))
    .is_ok());
    for event in [
        offer(
            format!("{}a", "🔒".repeat(256)),
            "x".into(),
            1,
            "ab".repeat(32),
        ),
        offer("ok".into(), "é".into(), 1, "ab".repeat(32)),
        offer("ok".into(), "x".repeat(129), 1, "ab".repeat(32)),
        offer(
            "ok".into(),
            "x".into(),
            MAX_ATTACHMENT_BYTES + 1,
            "ab".repeat(32),
        ),
        offer("ok".into(), "x".into(), 1, "AB".repeat(32)),
        offer("x\n".into(), "x".into(), 1, "ab".repeat(32)),
        offer("../unsafe".into(), "x".into(), 1, "ab".repeat(32)),
        offer("a\\b".into(), "x".into(), 1, "ab".repeat(32)),
        ChatEvent::AttachmentProgress {
            id: id(),
            received: MAX_ATTACHMENT_BYTES + 1,
        },
        ChatEvent::Reaction {
            target: id(),
            emoji: Some("💣".into()),
        },
    ] {
        assert!(encode_event(&event).is_err());
    }
    let payload = STANDARD.encode(vec![7; ATTACHMENT_CHUNK_BYTES]);
    assert!(encode_event(&ChatEvent::AttachmentChunk {
        id: id(),
        offset: MAX_ATTACHMENT_BYTES - ATTACHMENT_CHUNK_BYTES as u64,
        data: payload
    })
    .is_ok());
    for (offset, data) in [
        (MAX_ATTACHMENT_BYTES, STANDARD.encode([1])),
        (u64::MAX, STANDARD.encode([1])),
        (0, STANDARD.encode(vec![0; ATTACHMENT_CHUNK_BYTES + 1])),
        (0, String::new()),
        (0, "AP-A".into()),
        (0, "YQ".into()),
        (0, "YQ==\n".into()),
    ] {
        assert!(encode_event(&ChatEvent::AttachmentChunk {
            id: id(),
            offset,
            data
        })
        .is_err());
    }
    for emoji in ["👍", "❤️", "😂", "😮", "😢", "🔥"] {
        assert!(encode_event(&ChatEvent::Reaction {
            target: id(),
            emoji: Some(emoji.into())
        })
        .is_ok());
    }
    for audio in [
        vec!["audio/ogg".into()],
        vec!["audio/wav".into(), "audio/wav".into()],
        vec!["audio/wav".into(); 4],
    ] {
        assert!(encode_event(&ChatEvent::Capabilities { audio }).is_err());
    }
}
#[test]
fn event_frame_limit_includes_aead_overhead_and_large_text_still_uses_old_chunks() {
    let base = encode_event(&ChatEvent::Text {
        id: id(),
        text: "x".into(),
    })
    .unwrap()
    .len();
    let text = "x".repeat(1 + MAX_EVENT_JSON_BYTES - base);
    let event = ChatEvent::Text { id: id(), text };
    let (mut a, mut b) = ready();
    let frame = a.send_event(event.clone()).unwrap();
    assert_eq!(frame.len(), MAX_EVENT_FRAME_BYTES);
    assert_eq!(b.receive(&frame).unwrap().event, Some(event));
    assert!(decode_event(&vec![b' '; MAX_EVENT_JSON_BYTES + 1]).is_err());
    let text = "🔒".repeat(60_000);
    assert!(text.len() > 64 * 1024);
    let frames = a.send_text(&text, [7; 16]).unwrap();
    let mut result = None;
    for frame in frames {
        let received = b.receive(&frame).unwrap();
        result = received.event.or(result);
    }
    assert_eq!(result, Some(ChatEvent::Text { id: id(), text }));
}
#[test]
fn events_require_both_explicit_confirmations_and_received_early_events_retire_keys() {
    let (mut a, mut b) = unverified();
    assert!(a.send_event(ChatEvent::Receipt { id: id() }).is_err());
    let ar = a.confirm().unwrap();
    b.receive(&ar).unwrap();
    assert!(b.send_event(ChatEvent::Receipt { id: id() }).is_err());
    let (mut a, mut peer) = with_raw_peer(false);
    let frame = peer
        .encrypt(
            KIND_EVENT,
            &encode_event(&ChatEvent::Receipt { id: id() }).unwrap(),
        )
        .unwrap();
    assert_eq!(
        a.receive(&frame).err().unwrap().code,
        "verification_required"
    );
    assert!(a.sas().is_none());
    assert_eq!(a.confirm().unwrap_err().code, "session_closed");
}
#[test]
fn authenticated_invalid_events_tampering_replay_and_v3_frames_fail_closed() {
    let (mut a, mut peer) = with_raw_peer(true);
    let invalid = format!(r#"{{"type":"receipt","id":"{}","extra":true}}"#, id());
    let frame = peer.encrypt(KIND_EVENT, invalid.as_bytes()).unwrap();
    assert_eq!(a.receive(&frame).err().unwrap().code, "invalid_event");
    assert!(a.sas().is_none());
    for changed in [0, 1, 9, 10, 35] {
        let (mut a, mut b) = ready();
        let mut frame = a.send_event(ChatEvent::Receipt { id: id() }).unwrap();
        frame[changed] ^= 1;
        assert!(b.receive(&frame).is_err());
        assert!(b.sas().is_none());
    }
    let (mut a, mut b) = ready();
    let frame = a.send_event(ChatEvent::Receipt { id: id() }).unwrap();
    assert_eq!(frame[0], VERSION);
    b.receive(&frame).unwrap();
    assert_eq!(b.receive(&frame).err().unwrap().code, "replay");
    let (mut a, mut peer) = with_raw_peer(true);
    let mut frame = peer
        .encrypt(
            KIND_EVENT,
            &encode_event(&ChatEvent::Receipt { id: id() }).unwrap(),
        )
        .unwrap();
    frame[0] = 3;
    assert!(a.receive(&frame).is_err());
    assert!(a.sas().is_none());
}
#[test]
fn bye_is_terminal_but_attachment_cancel_keeps_the_chat_usable() {
    let (mut a, mut b) = ready();
    let cancel = ChatEvent::AttachmentCancel { id: id() };
    assert_eq!(
        b.receive(&a.send_event(cancel.clone()).unwrap())
            .unwrap()
            .event,
        Some(cancel)
    );
    assert!(a.sas().is_some());
    assert!(b.sas().is_some());
    let bye = ChatEvent::Bye {
        reason: ByeReason::Exit,
    };
    let frame = a.send_event(bye.clone()).unwrap();
    assert!(a.sas().is_none());
    assert_eq!(b.receive(&frame).unwrap().event, Some(bye));
    assert!(b.sas().is_none());
    assert_eq!(
        a.send_event(ChatEvent::Receipt { id: id() })
            .unwrap_err()
            .code,
        "session_closed"
    );
    assert!(b.receive(&frame).is_err());
}
#[test]
fn reassembly_exposes_the_exact_sender_id_without_breaking_string_api() {
    let text = "Юникод".repeat(5_000);
    let parts = chunks_with_id(&text, [7; 16]).unwrap();
    let mut receiver = Reassembler::default();
    let mut complete = None;
    for part in &parts {
        complete = receiver.push_with_id(part).unwrap().or(complete);
    }
    let complete = complete.unwrap();
    assert_eq!(complete.id, id());
    assert_eq!(complete.text, text);
    let mut legacy = Reassembler::default();
    let mut result = None;
    for part in parts {
        result = legacy.push(&part).unwrap().or(result);
    }
    assert_eq!(result, Some(text));
}
#[test]
fn debug_and_errors_never_include_event_payloads() {
    let event = ChatEvent::Text {
        id: id(),
        text: "PRIVATE_MESSAGE".into(),
    };
    assert!(!format!("{event:?}").contains("PRIVATE_MESSAGE"));
    let error = decode_event(b"{PRIVATE_MESSAGE").unwrap_err();
    assert!(!error.to_string().contains("PRIVATE_MESSAGE"));
}

#[test]
fn v3_commitment_cannot_start_or_downgrade_a_v4_session() {
    let mut session = Session::new("events", true, [1; 32], [2; 32]).unwrap();
    let mut old = vec![3, 0, 1];
    old.extend_from_slice(&[7; 32]);
    assert_eq!(
        session.receive(&old).err().unwrap().code,
        "handshake_context"
    );
    assert!(session.sas().is_none());
    assert_eq!(session.start().unwrap_err().code, "session_closed");
}
