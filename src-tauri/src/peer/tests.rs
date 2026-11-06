use super::{
    connection::rtc_config,
    crypto::KeyExchange,
    state::{lock, Phase, Session},
};
use std::{sync::Arc, time::Duration};
use webrtc::{api::APIBuilder, data_channel::RTCDataChannel};
#[test]
fn verification_and_recovery_are_independent() {
    let s = Session::new(1, "test".into(), None);
    let a = KeyExchange::new().unwrap();
    let b = KeyExchange::new().unwrap();
    {
        let mut d = lock(&s.data);
        d.crypto = Some(a.finish(b.public_key()).unwrap());
        d.transport_connected = true;
    }
    assert_eq!(s.snapshot().phase, Phase::Verification);
    lock(&s.data).local_ready = true;
    assert_eq!(s.snapshot().phase, Phase::WaitingPeer);
    lock(&s.data).peer_ready = true;
    assert_eq!(s.snapshot().phase, Phase::Connected);
    lock(&s.data).recovering = true;
    assert_eq!(s.snapshot().phase, Phase::Recovering);
    lock(&s.data).recovering = false;
    assert_eq!(s.snapshot().phase, Phase::Connected);
}
#[tokio::test]
async fn closing_is_idempotent_and_clears_secrets() {
    let s = Session::new(1, "test".into(), None);
    lock(&s.data).exchange = Some(KeyExchange::new().unwrap());
    s.close().await;
    s.close().await;
    assert_eq!(s.snapshot().phase, Phase::Closed);
    assert!(lock(&s.data).exchange.is_none());
}
#[tokio::test]
async fn two_local_webrtc_peers_exchange_encrypted_chunks() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let api = APIBuilder::new().build();
        let a = Arc::new(
            api.new_peer_connection(rtc_config(Some(vec![])))
                .await
                .unwrap(),
        );
        let b = Arc::new(
            api.new_peer_connection(rtc_config(Some(vec![])))
                .await
                .unwrap(),
        );
        let ka = KeyExchange::new().unwrap();
        let kb = KeyExchange::new().unwrap();
        let pub_a = ka.public_key();
        let pub_b = kb.public_key();
        let mut sender = ka.finish(pub_b).unwrap();
        let receiver = Arc::new(std::sync::Mutex::new(kb.finish(pub_a).unwrap()));
        let assembly = Arc::new(std::sync::Mutex::new(
            super::protocol::Reassembler::default(),
        ));
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);
        b.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let receiver = receiver.clone();
            let assembly = assembly.clone();
            let tx = tx.clone();
            Box::pin(async move {
                dc.on_message(Box::new(move |msg| {
                    let receiver = receiver.clone();
                    let assembly = assembly.clone();
                    let tx = tx.clone();
                    Box::pin(async move {
                        let text = {
                            let (kind, plain) = lock(&receiver).decrypt(&msg.data).unwrap();
                            assert_eq!(kind, super::protocol::KIND_CHUNK);
                            lock(&assembly).push(&plain).unwrap()
                        };
                        if let Some(text) = text {
                            tx.send(text).await.unwrap();
                        }
                    })
                }));
            })
        }));
        let dc = a.create_data_channel("test", None).await.unwrap();
        let (opened, ready) = tokio::sync::oneshot::channel();
        let opened = std::sync::Mutex::new(Some(opened));
        dc.on_open(Box::new(move || {
            if let Some(tx) = lock(&opened).take() {
                let _ = tx.send(());
            }
            Box::pin(async {})
        }));
        let mut ga = a.gathering_complete_promise().await;
        a.set_local_description(a.create_offer(None).await.unwrap())
            .await
            .unwrap();
        ga.recv().await;
        b.set_remote_description(a.local_description().await.unwrap())
            .await
            .unwrap();
        let mut gb = b.gathering_complete_promise().await;
        b.set_local_description(b.create_answer(None).await.unwrap())
            .await
            .unwrap();
        gb.recv().await;
        a.set_remote_description(b.local_description().await.unwrap())
            .await
            .unwrap();
        ready.await.unwrap();
        let text = "Привет 🦀 ".repeat(3000);
        for part in super::protocol::chunks(&text).unwrap() {
            let frame = sender.encrypt(super::protocol::KIND_CHUNK, &part).unwrap();
            dc.send(&bytes::Bytes::from(frame)).await.unwrap();
        }
        assert_eq!(rx.recv().await.unwrap(), text);
        a.close().await.unwrap();
        b.close().await.unwrap();
    })
    .await
    .expect("localhost WebRTC timed out");
}

#[tokio::test]
async fn runtime_handshake_waits_for_both_confirmations_and_serializes_messages() {
    use super::{
        connection::new_peer,
        data_channel::{confirm, send_text_session},
    };
    tokio::time::timeout(Duration::from_secs(20), async {
        lock(&super::state::MANAGER).servers = Some(vec![]);
        let a = Session::new(101, "a".repeat(32), None);
        let b = Session::new(102, "a".repeat(32), None);
        new_peer(&a, true).await.unwrap();
        new_peer(&b, false).await.unwrap();
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
        assert_eq!(
            lock(&a.data).crypto.as_ref().unwrap().sas,
            lock(&b.data).crypto.as_ref().unwrap().sas
        );
        assert!(send_text_session(&a, "too early").await.is_err());
        confirm(&a).await.unwrap();
        while !lock(&b.data).peer_ready {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(a.snapshot().phase, Phase::WaitingPeer);
        assert!(send_text_session(&a, "still too early").await.is_err());
        confirm(&b).await.unwrap();
        while a.snapshot().phase != Phase::Connected || b.snapshot().phase != Phase::Connected {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let large = "🦀hello".repeat(12000);
        let (r1, r2) = tokio::join!(
            send_text_session(&a, &large),
            send_text_session(&a, "parallel")
        );
        r1.unwrap();
        r2.unwrap();
        send_text_session(&b, "reverse direction").await.unwrap();
        while lock(&b.data).inbox.len() < 2 || lock(&a.data).inbox.is_empty() {
            assert!(
                a.active() && b.active(),
                "session closed during message transfer"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(a.active());
        assert!(b.active());
        assert!(
            lock(&b.data).inbox[0].text == large,
            "large message differs"
        );
        assert_eq!(lock(&b.data).inbox[1].text, "parallel");
        assert_eq!(lock(&a.data).inbox[0].text, "reverse direction");
        // A forged authenticated header/tag must retire the real receiving session.
        let dc = lock(&a.data).dc.clone().unwrap();
        let mut forged = vec![2, 1];
        forged.extend_from_slice(&u64::MAX.to_be_bytes());
        forged.extend_from_slice(&[0u8; 16]);
        dc.send(&bytes::Bytes::from(forged)).await.unwrap();
        while b.active() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(lock(&b.data).crypto.is_none());
        a.close().await;
        b.close().await;
        assert_eq!(a.snapshot().phase, Phase::Closed);
        assert_eq!(b.snapshot().phase, Phase::Closed);
    })
    .await
    .expect("runtime handshake timed out");
}
