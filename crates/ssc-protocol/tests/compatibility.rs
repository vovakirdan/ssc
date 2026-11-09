use ssc_protocol::{crypto::KeyExchange, invitation, message, Session};
const LEGACY_INVITATION:&str="v3:H4sIAAAAAAAE/6WOzQ5AMBCE32XPDou49MZriAOl1E+Llot4d0P6BrLJZna/ZGYuciTC8iQ4okORoLqRLUGvQXeqH/Q4zYux67Y7f5yg6qV58W/g4/YveNEI4/eGb8bMd0S6xU9aYzrptTUxqEfhOEkBJVRZ3Q9J1GFGwwAAAA==";

#[test]
fn legacy_native_crypto_vector_remains_byte_identical() {
    // Captured from pre-extraction desktop v3 with these seeds and transcript.
    let a = KeyExchange::from_seed([9; 32]);
    let b = KeyExchange::from_seed([17; 32]);
    let ap = a.public_key();
    let bp = b.public_key();
    let mut a = a.finish(bp, &[7; 32], true).unwrap();
    let mut b = b.finish(ap, &[7; 32], false).unwrap();
    assert_eq!(a.sas, "5840165d3e66");
    let frame = a.encrypt(2, "legacy vector 🔒".as_bytes()).unwrap();
    assert_eq!(
        hex::encode(&frame),
        "0302000000000000000174cebbcd4db2355cd9c923145e5f52b3cecbe816479b4a508641ab465760b2d31cb3"
    );
    assert_eq!(b.decrypt(&frame).unwrap().1, "legacy vector 🔒".as_bytes());
}
#[test]
fn legacy_gzip_json_vector_remains_byte_identical() {
    let bundle = invitation::dec_bundle(LEGACY_INVITATION).unwrap();
    assert_eq!(bundle.sdp_payload.id, "connection1");
    assert_eq!(bundle.sdp_payload.ts, 123);
    assert_eq!(invitation::enc_bundle(&bundle).unwrap(), LEGACY_INVITATION);
    assert_eq!(message::chunks_with_id("Привет",[7;16]).unwrap()[0],br#"{"id":"07070707070707070707070707070707","index":0,"total":1,"data":"0J/RgNC40LLQtdGC"}"#);
}
fn connected() -> (Session, Session) {
    let mut a = Session::new("session", true, [1; 32], [11; 32]).unwrap();
    let mut b = Session::new("session", false, [2; 32], [12; 32]).unwrap();
    let ac = a.start().unwrap();
    let bc = b.start().unwrap();
    let ar = a.receive(&bc[0]).unwrap();
    let br = b.receive(&ac[0]).unwrap();
    a.receive(&br.outgoing[0]).unwrap();
    b.receive(&ar.outgoing[0]).unwrap();
    assert!(a.sas().is_some());
    assert_eq!(a.sas(), b.sas());
    (a, b)
}
#[test]
fn browser_engine_requires_two_explicit_confirmations() {
    let (mut a, mut b) = connected();
    assert!(!a.local_ready());
    assert!(!a.peer_ready());
    assert!(a.send_text("x", [0; 16]).is_err());
    let ready = a.confirm().unwrap();
    b.receive(&ready).unwrap();
    assert!(!b.local_ready());
    assert!(b.peer_ready());
    assert!(b.send_text("x", [0; 16]).is_err());
    a.receive(&b.confirm().unwrap()).unwrap();
    let message = "🔒Привет\n".repeat(6000);
    let frames = a.send_text(&message, [7; 16]).unwrap();
    assert!(frames.len() > 1);
    for f in &frames[..frames.len() - 1] {
        assert!(b.receive(f).unwrap().message.is_none());
    }
    assert_eq!(
        b.receive(frames.last().unwrap()).unwrap().message,
        Some(message)
    );
    assert!(a.confirm().is_err());
}
#[test]
fn protocol_errors_retire_browser_keys_and_partial_messages() {
    let (mut a, mut b) = connected();
    let ar = a.confirm().unwrap();
    let br = b.confirm().unwrap();
    a.receive(&br).unwrap();
    b.receive(&ar).unwrap();
    let parts = a
        .send_text(&"x".repeat(message::CHUNK_BYTES + 1), [7; 16])
        .unwrap();
    b.receive(&parts[0]).unwrap();
    assert!(b.receive(&parts[0]).is_err());
    assert!(b.sas().is_none());
    assert!(!b.peer_ready());
    assert!(!b.local_ready());
    assert_eq!(b.receive(&parts[1]).err().unwrap().code, "session_closed");
    assert!(b.start().is_err());
    assert!(b.confirm().is_err());
}
#[test]
fn close_is_terminal_and_does_not_allow_late_callbacks() {
    let (mut a, _) = connected();
    a.close();
    a.close();
    assert!(a.sas().is_none());
    assert!(a.start().is_err());
    assert!(a.confirm().is_err());
    assert!(a.send_text("late", [8; 16]).is_err());
    assert!(Session::new("", true, [1; 32], [2; 32]).is_err());
    assert!(Session::new(&"x".repeat(129), true, [1; 32], [2; 32]).is_err());
}

#[test]
fn complete_native_handshake_matches_fixed_cross_provider_vectors() {
    let expected: serde_json::Value = serde_json::from_str(include_str!("vectors.json")).unwrap();
    let mut a = Session::new("wasm-native-vector", true, [9; 32], [11; 32]).unwrap();
    let mut b = Session::new("wasm-native-vector", false, [17; 32], [19; 32]).unwrap();
    let ac = a.start().unwrap().remove(0);
    let bc = b.start().unwrap().remove(0);
    assert_eq!(hex::encode(&ac), expected["commit_a"]);
    assert_eq!(hex::encode(&bc), expected["commit_b"]);
    let ar = a.receive(&bc).unwrap().outgoing.remove(0);
    let br = b.receive(&ac).unwrap().outgoing.remove(0);
    assert_eq!(hex::encode(&ar), expected["reveal_a"]);
    assert_eq!(hex::encode(&br), expected["reveal_b"]);
    a.receive(&br).unwrap();
    b.receive(&ar).unwrap();
    assert_eq!(a.sas().unwrap(), expected["sas"]);
    let ra = a.confirm().unwrap();
    let rb = b.confirm().unwrap();
    assert_eq!(hex::encode(&ra), expected["ready_a"]);
    assert_eq!(hex::encode(&rb), expected["ready_b"]);
    a.receive(&rb).unwrap();
    b.receive(&ra).unwrap();
    assert_eq!(
        hex::encode(a.send_text("Привет из WASM 🔒", [7; 16]).unwrap().remove(0)),
        expected["text"]
    );
}

#[test]
fn producing_invalid_invitation_is_rejected_before_publication() {
    let mut bundle = invitation::dec_bundle(LEGACY_INVITATION).unwrap();
    bundle.sdp_payload.id.clear();
    assert_eq!(
        invitation::enc_bundle(&bundle).unwrap_err().code,
        "invalid_invitation"
    );
}
