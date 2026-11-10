use super::{
    connection::new_peer_with_servers,
    crypto::{CryptoCtx, KeyExchange},
    data_channel::{
        confirm, queue_received, receive, send_chat_event_session, wait_send_buffer, BUFFER_HIGH,
        BUFFER_LOW, MAX_INBOX_EVENTS,
    },
    protocol::{encode_event, ChatEvent, KIND_EVENT},
    state::{lock, CloseReason, Phase, Session},
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use ssc_protocol::{AttachmentKind, ByeReason};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

fn keyed(generation: u64) -> (Arc<Session>, CryptoCtx) {
    let s = Session::new(generation, "e".repeat(32), None);
    let a = KeyExchange::new().unwrap();
    let b = KeyExchange::new().unwrap();
    let public_a = a.public_key();
    let public_b = b.public_key();
    lock(&s.data).crypto = Some(a.finish(public_b, &[9; 32], true).unwrap());
    (s, b.finish(public_a, &[9; 32], false).unwrap())
}
fn ready(s: &Session) {
    let mut d = lock(&s.data);
    d.local_ready = true;
    d.peer_ready = true;
    d.transport_connected = true;
}

async fn connected(generation: u64) -> (Arc<Session>, Arc<Session>) {
    let a = Session::new(generation, "e".repeat(32), None);
    let b = Session::new(generation + 1, "e".repeat(32), None);
    new_peer_with_servers(&a, true, vec![]).await.unwrap();
    new_peer_with_servers(&b, false, vec![]).await.unwrap();
    let offer = crate::commands::candidate_api::local_bundle(&a, true)
        .await
        .unwrap();
    crate::commands::candidate_api::remote(&b, super::crypto::dec_bundle(&offer).unwrap())
        .await
        .unwrap();
    let answer = crate::commands::candidate_api::local_bundle(&b, false)
        .await
        .unwrap();
    crate::commands::candidate_api::remote(&a, super::crypto::dec_bundle(&answer).unwrap())
        .await
        .unwrap();
    while lock(&a.data).crypto.is_none() || lock(&b.data).crypto.is_none() {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let sas = lock(&a.data).crypto.as_ref().unwrap().sas.clone();
    assert_eq!(sas, lock(&b.data).crypto.as_ref().unwrap().sas);
    confirm(&a, a.generation, &sas).await.unwrap();
    confirm(&b, b.generation, &sas).await.unwrap();
    while a.snapshot().phase != Phase::Connected || b.snapshot().phase != Phase::Connected {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    (a, b)
}

#[tokio::test]
async fn actual_native_v4_events_stable_text_id_and_authenticated_bye() {
    tokio::time::timeout(Duration::from_secs(30), async {
        let (a, b) = connected(701).await;
        let id = "1".repeat(32);
        let file_id = "2".repeat(32);
        let events = vec![
            ChatEvent::Text {
                id: id.clone(),
                text: "private-native-event 🦀".repeat(5000),
            },
            ChatEvent::AttachmentOffer {
                id: file_id.clone(),
                name: "private-filename.bin".into(),
                mime: "application/octet-stream".into(),
                size: 7,
                sha256: "a".repeat(64),
                kind: AttachmentKind::File,
            },
            ChatEvent::AttachmentAccept {
                id: file_id.clone(),
            },
            ChatEvent::AttachmentChunk {
                id: file_id.clone(),
                offset: 0,
                data: STANDARD.encode(b"payload"),
            },
            ChatEvent::AttachmentProgress {
                id: file_id.clone(),
                received: 7,
            },
            ChatEvent::AttachmentComplete { id: file_id },
            ChatEvent::Reaction {
                target: id.clone(),
                emoji: Some("👍".into()),
            },
            ChatEvent::Receipt { id },
            ChatEvent::Capabilities {
                audio: vec!["audio/webm;codecs=opus".into()],
            },
        ];
        for event in &events {
            send_chat_event_session(&a, a.generation, event.clone())
                .await
                .unwrap();
        }
        while lock(&b.data).event_inbox.len() < events.len() {
            assert!(b.active());
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let drained = b.drain_chat_events(b.generation);
        assert_eq!(drained.generation, b.generation);
        assert_eq!(drained.events, events);
        assert!(lock(&b.data).inbox.is_empty());
        let reply = ChatEvent::Receipt { id: "1".repeat(32) };
        send_chat_event_session(&b, b.generation, reply.clone())
            .await
            .unwrap();
        while lock(&a.data).event_inbox.is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(a.drain_chat_events(a.generation).events, vec![reply]);
        let snapshot = serde_json::to_string(&b.snapshot()).unwrap();
        assert!(
            !snapshot.contains("private-native-event")
                && !snapshot.contains("private-filename")
                && !snapshot.contains("payload")
        );
        a.close_local(ByeReason::Exit).await;
        while b.active() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(a.snapshot().close_reason, Some(CloseReason::LocalLeft));
        assert_eq!(b.snapshot().close_reason, Some(CloseReason::PeerLeft));
        b.close_from_callback();
        assert_eq!(b.snapshot().close_reason, Some(CloseReason::PeerLeft));
        assert!(b.drain_chat_events(b.generation).events.is_empty());
    })
    .await
    .expect("native v4 event/bye transfer timed out");
}

#[tokio::test]
async fn actual_native_expired_bye_is_distinct_from_transport_loss() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let (a, b) = connected(703).await;
        b.close_local(ByeReason::Expired).await;
        while a.active() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(a.snapshot().close_reason, Some(CloseReason::Expired));
        assert_eq!(b.snapshot().close_reason, Some(CloseReason::Expired));
        let loss = Session::new(705, "loss".into(), None);
        loss.close().await;
        assert_eq!(
            loss.snapshot().close_reason,
            Some(CloseReason::ConnectionLost)
        );
    })
    .await
    .expect("native expiry bye timed out");
}

#[tokio::test]
async fn generation_and_readiness_reject_before_nonce_or_queue_mutation() {
    let (s, _) = keyed(706);
    ready(&s);
    let event = ChatEvent::Receipt { id: "3".repeat(32) };
    assert_eq!(
        send_chat_event_session(&s, 705, event.clone())
            .await
            .unwrap_err()
            .code,
        "session_changed"
    );
    lock(&s.data).peer_ready = false;
    assert_eq!(
        send_chat_event_session(&s, 706, event.clone())
            .await
            .unwrap_err()
            .code,
        "not_ready"
    );
    let probe = lock(&s.data)
        .crypto
        .as_mut()
        .unwrap()
        .encrypt(super::protocol::KIND_READY, &[])
        .unwrap();
    assert_eq!(u64::from_be_bytes(probe[2..10].try_into().unwrap()), 1);
    ready(&s);
    queue_received(&s, event.clone()).unwrap();
    assert!(s.drain_chat_events(705).events.is_empty());
    assert_eq!(s.drain_chat_events(706).events, vec![event]);
    s.close().await;
}

#[tokio::test]
async fn authenticated_bye_before_both_ready_cannot_forge_close_reason() {
    let (s, mut peer) = keyed(707);
    let bytes = peer
        .encrypt(
            KIND_EVENT,
            &encode_event(&ChatEvent::Bye {
                reason: ByeReason::Expired,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(receive(&s, &bytes).unwrap_err().code, "not_ready");
    assert_eq!(s.snapshot().close_reason, None);
    s.close().await;
}

#[tokio::test]
async fn event_inbox_overflow_closes_and_erases_instead_of_dropping() {
    let (s, _) = keyed(708);
    ready(&s);
    for _ in 0..MAX_INBOX_EVENTS {
        queue_received(&s, ChatEvent::Receipt { id: "4".repeat(32) }).unwrap();
    }
    assert_eq!(
        queue_received(&s, ChatEvent::Receipt { id: "4".repeat(32) })
            .unwrap_err()
            .code,
        "inbox_full"
    );
    assert_eq!(s.snapshot().phase, Phase::Closed);
    assert!(lock(&s.data).event_inbox.is_empty());
    let (s, _) = keyed(709);
    ready(&s);
    let event = ChatEvent::Text {
        id: "5".repeat(32),
        text: "x".repeat(256 * 1024),
    };
    let mut overflow = false;
    for _ in 0..40 {
        if queue_received(&s, event.clone()).is_err() {
            overflow = true;
            break;
        }
    }
    assert!(overflow && !s.active());
    assert!(lock(&s.data).crypto.is_none());
    assert!(lock(&s.data).inbox.is_empty());
}

#[tokio::test]
async fn backpressure_waits_for_low_water_and_is_cancelled_by_stop() {
    let s = Session::new(710, "pressure".into(), None);
    let amount = Arc::new(AtomicUsize::new(BUFFER_HIGH));
    let task = {
        let s = s.clone();
        let amount = amount.clone();
        tokio::spawn(async move {
            wait_send_buffer(&s, 1024, || async { amount.load(Ordering::SeqCst) }).await
        })
    };
    tokio::time::sleep(Duration::from_millis(15)).await;
    assert!(!task.is_finished());
    amount.store(BUFFER_LOW + 1, Ordering::SeqCst);
    tokio::time::sleep(Duration::from_millis(15)).await;
    assert!(!task.is_finished());
    amount.store(BUFFER_LOW, Ordering::SeqCst);
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let task = {
        let s = s.clone();
        tokio::spawn(async move { wait_send_buffer(&s, 1024, || async { BUFFER_HIGH }).await })
    };
    s.close().await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .code,
        "session_closed"
    );
}

#[tokio::test]
async fn local_close_clears_immediately_even_while_send_mutex_is_held() {
    let (s, _) = keyed(711);
    ready(&s);
    queue_received(&s, ChatEvent::Receipt { id: "6".repeat(32) }).unwrap();
    let guard = s.sending.lock().await;
    let task = {
        let s = s.clone();
        tokio::spawn(async move { s.close_local(ByeReason::Exit).await })
    };
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(s.snapshot().phase, Phase::Closed);
    assert!(lock(&s.data).crypto.is_none());
    assert!(lock(&s.data).event_inbox.is_empty());
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    drop(guard);
}

#[tokio::test]
async fn incremental_hashing_is_generation_bound_and_cleared_on_close() {
    use super::attachment_hash::{abort, finish, update};
    let (s, _) = keyed(712);
    ready(&s);
    let id = "7".repeat(32);
    update(&s, 712, &id, b"a", true).unwrap();
    update(&s, 712, &id, b"bc", false).unwrap();
    assert_eq!(
        finish(&s, 712, &id).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(finish(&s, 712, &id).unwrap_err().code, "hasher_missing");
    for i in 0..4 {
        update(&s, 712, &format!("{i:032x}"), b"", true).unwrap();
    }
    assert_eq!(
        update(&s, 712, &id, b"", true).unwrap_err().code,
        "hasher_capacity"
    );
    assert_eq!(
        abort(&s, 711, &"0".repeat(32)).unwrap_err().code,
        "session_changed"
    );
    assert_eq!(lock(&s.data).hashers.len(), 4);
    abort(&s, 712, &"0".repeat(32)).unwrap();
    assert_eq!(
        update(&s, 712, &id, &vec![0; 256 * 1024 + 1], true)
            .unwrap_err()
            .code,
        "hash_chunk_size"
    );
    update(&s, 712, &id, b"", true).unwrap();
    let part = vec![0u8; 256 * 1024];
    for _ in 0..400 {
        update(&s, 712, &id, &part, false).unwrap();
    }
    assert_eq!(
        update(&s, 712, &id, &[1], false).unwrap_err().code,
        "file_size"
    );
    assert!(!lock(&s.data).hashers.contains_key(&id));
    s.close().await;
    assert!(lock(&s.data).hashers.is_empty());
}

#[tokio::test]
async fn stale_ui_disconnect_cannot_close_replacement_or_reserve_bye_nonce() {
    use crate::commands::util_api::{disconnect_session, DisconnectReason};
    let (replacement, _) = keyed(713);
    ready(&replacement);
    disconnect_session(&replacement, Some(712), DisconnectReason::Exit).await;
    assert!(replacement.active());
    assert_eq!(replacement.snapshot().close_reason, None);
    let probe = lock(&replacement.data)
        .crypto
        .as_mut()
        .unwrap()
        .encrypt(super::protocol::KIND_READY, &[])
        .unwrap();
    assert_eq!(u64::from_be_bytes(probe[2..10].try_into().unwrap()), 1);
    disconnect_session(&replacement, Some(713), DisconnectReason::Error).await;
    assert_eq!(
        replacement.snapshot().close_reason,
        Some(CloseReason::ConnectionLost)
    );
}

#[tokio::test]
async fn queued_plaintext_sends_cancel_without_waiting_for_farewell_mutex() {
    let (s, _) = keyed(714);
    ready(&s);
    let held = s.sending.lock().await;
    let event = {
        let s = s.clone();
        tokio::spawn(async move {
            send_chat_event_session(
                &s,
                714,
                ChatEvent::Text {
                    id: "8".repeat(32),
                    text: "queued private text".repeat(1000),
                },
            )
            .await
        })
    };
    let legacy = {
        let s = s.clone();
        tokio::spawn(async move {
            super::data_channel::send_text_session(&s, "queued legacy text").await
        })
    };
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(!event.is_finished() && !legacy.is_finished());
    let close = {
        let s = s.clone();
        tokio::spawn(async move { s.close_local(ByeReason::Exit).await })
    };
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(150), event)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .code,
        "session_closed"
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(150), legacy)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err()
            .code,
        "session_closed"
    );
    assert_eq!(s.snapshot().close_reason, Some(CloseReason::LocalLeft));
    assert!(!close.is_finished());
    drop(held);
    tokio::time::timeout(Duration::from_secs(1), close)
        .await
        .unwrap()
        .unwrap();
}
