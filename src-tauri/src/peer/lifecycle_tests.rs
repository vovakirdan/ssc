use super::{
    crypto::{CryptoCtx, KeyExchange},
    data_channel::{confirm, receive},
    protocol::{chunks, KIND_CHUNK, KIND_READY},
    state::{lock, Phase, ReceivedMessage, Session},
};
use std::sync::Arc;

fn keyed_session(generation: u64) -> (Arc<Session>, CryptoCtx) {
    let session = Session::new(generation, format!("session-{generation}"), None);
    let local = KeyExchange::new().unwrap();
    let peer = KeyExchange::new().unwrap();
    let local_public = local.public_key();
    let peer_public = peer.public_key();
    lock(&session.data).crypto = Some(local.finish(peer_public).unwrap());
    (session, peer.finish(local_public).unwrap())
}

fn hello(public_key: [u8; 32]) -> Vec<u8> {
    let mut frame = vec![2, 0];
    frame.extend_from_slice(&public_key);
    frame
}

#[tokio::test]
async fn late_key_callback_after_close_cannot_restore_secrets() {
    let session = Session::new(201, "closed-handshake".into(), None);
    lock(&session.data).exchange = Some(KeyExchange::new().unwrap());
    let peer = KeyExchange::new().unwrap();
    let callback_bytes = hello(peer.public_key());
    session.close().await;

    let error = receive(&session, &callback_bytes).unwrap_err();
    assert_eq!(error.code, "session_closed");
    assert_eq!(session.snapshot().phase, Phase::Closed);
    let data = lock(&session.data);
    assert!(data.exchange.is_none());
    assert!(data.crypto.is_none());
}

#[tokio::test]
async fn retired_callbacks_cannot_change_replacement_session() {
    let (retired, mut old_peer) = keyed_session(202);
    lock(&retired.data).local_ready = true;
    retired.close().await;
    let (replacement, _) = keyed_session(203);
    let expected_sas = lock(&replacement.data).crypto.as_ref().unwrap().sas.clone();

    let delayed_ready = old_peer.encrypt(KIND_READY, &[]).unwrap();
    assert_eq!(
        receive(&retired, &delayed_ready).unwrap_err().code,
        "session_closed"
    );
    assert_eq!(confirm(&retired).await.unwrap_err().code, "session_closed");
    retired.close().await;

    assert_eq!(replacement.snapshot().phase, Phase::Verification);
    let replacement_data = lock(&replacement.data);
    assert!(!replacement_data.closed);
    assert!(!replacement_data.peer_ready);
    assert!(!replacement_data.local_ready);
    assert_eq!(replacement_data.crypto.as_ref().unwrap().sas, expected_sas);
}

#[tokio::test]
async fn invalid_key_callback_can_be_closed_without_retaining_key_exchange() {
    let session = Session::new(204, "invalid-key".into(), None);
    lock(&session.data).exchange = Some(KeyExchange::new().unwrap());
    assert_eq!(
        receive(&session, &hello([0; 32])).unwrap_err().code,
        "invalid_key"
    );
    // The production callback closes the session on any receive error.
    session.close().await;
    let data = lock(&session.data);
    assert!(data.closed);
    assert!(data.exchange.is_none());
    assert!(data.crypto.is_none());
    assert!(!data.local_ready);
    assert!(!data.peer_ready);
}

#[tokio::test]
async fn repeated_close_clears_message_and_readiness_state_exactly_once() {
    let (session, _) = keyed_session(205);
    let parts = chunks(&"x".repeat(20_000)).unwrap();
    {
        let mut data = lock(&session.data);
        data.local_ready = true;
        data.peer_ready = true;
        data.transport_connected = true;
        data.inbox.push(ReceivedMessage {
            generation: 205,
            id: "message-1".into(),
            text: "private text".into(),
        });
        data.reassembler.push(&parts[0]).unwrap();
    }
    assert_eq!(session.snapshot().phase, Phase::Connected);
    session.close().await;
    let epoch = lock(&session.data).recovery_epoch;
    session.close().await;
    assert_eq!(session.snapshot().phase, Phase::Closed);
    let mut data = lock(&session.data);
    assert_eq!(data.recovery_epoch, epoch);
    assert!(data.crypto.is_none());
    assert!(data.inbox.is_empty());
    assert!(!data.local_ready && !data.peer_ready);
    // An abandoned second part cannot complete the old message after cleanup.
    assert!(data.reassembler.push(&parts[1]).is_err());
}

#[test]
fn authenticated_chat_before_local_or_peer_verification_is_rejected() {
    for (generation, local_ready, peer_ready) in
        [(206, false, true), (207, true, false), (208, false, false)]
    {
        let (session, mut peer) = keyed_session(generation);
        {
            let mut data = lock(&session.data);
            data.local_ready = local_ready;
            data.peer_ready = peer_ready;
        }
        let frame = peer
            .encrypt(KIND_CHUNK, &chunks("premature private message").unwrap()[0])
            .unwrap();
        assert_eq!(receive(&session, &frame).unwrap_err().code, "not_ready");
        assert!(lock(&session.data).inbox.is_empty());
    }
}

#[test]
fn ready_frame_with_payload_does_not_enable_peer() {
    let (session, mut peer) = keyed_session(209);
    let invalid_ready = peer.encrypt(KIND_READY, b"unexpected").unwrap();
    assert_eq!(
        receive(&session, &invalid_ready).unwrap_err().code,
        "ready_payload"
    );
    assert!(!lock(&session.data).peer_ready);
    assert!(lock(&session.data).inbox.is_empty());
}

#[tokio::test]
async fn recovered_session_survives_late_expiry_and_current_expiry_cleans_up() {
    let s = Session::new(501, "recovery".into(), None);
    let epoch = s.start_recovery().unwrap();
    s.recovered();
    s.expire_recovery(epoch).await;
    assert!(s.active());
    let epoch = s.start_recovery().unwrap();
    s.expire_recovery(epoch).await;
    assert!(!s.active());
    assert!(s.snapshot().phase == Phase::Closed);
}
