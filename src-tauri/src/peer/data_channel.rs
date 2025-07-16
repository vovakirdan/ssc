use crate::commands::util_api::get_fingerprint;
use crate::logger::log;
use crate::logger::{emit_connected, emit_disconnected, emit_message};
use crate::peer::crypto::{build_ctx, u64_to_nonce};
use crate::peer::state::{
    APP, COLLECTING_CANDIDATES, CRYPTO, DATA_CH, DISCONNECT_TASK, LOCAL_CANDIDATES, MY_PRIV,
    MY_PUB, PENDING_REMOTE_CANDIDATES, TAG_LEN, WAS_CONNECTED,
};
use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Nonce},
    ChaCha20Poly1305, Key,
};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use hkdf::Hkdf;
use once_cell::sync::Lazy;
use rand::Rng;
use ring::{agreement, rand as ring_rand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::time::timeout;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::peer_connection::policy::bundle_policy::RTCBundlePolicy;
use webrtc::peer_connection::policy::rtcp_mux_policy::RTCRtcpMuxPolicy;
use webrtc::{
    api::APIBuilder,
    data_channel::{data_channel_init::RTCDataChannelInit, RTCDataChannel},
    ice_transport::{ice_gatherer_state::RTCIceGathererState, ice_server::RTCIceServer},
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription, RTCPeerConnection,
    },
};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// общий обработчик data-channel
pub fn attach_dc(dc: &Arc<RTCDataChannel>) {
    log("attach_dc called - clearing old state");

    // отменяем отложенный disconnect, если он был
    if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
        log("Aborting pending disconnect task in attach_dc");
        handle.abort();
    }

    // Очищаем старое состояние перед созданием нового соединения
    log("Clearing CRYPTO context in attach_dc");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;
    *WAS_CONNECTED.lock().unwrap() = false;

    // очищаем отложенные кандидаты
    PENDING_REMOTE_CANDIDATES.lock().unwrap().clear();

    // очищаем локальные кандидаты
    LOCAL_CANDIDATES.lock().unwrap().clear();
    *COLLECTING_CANDIDATES.lock().unwrap() = false;

    {
        *DATA_CH.lock().unwrap() = Some(dc.clone());
    }

    // Генерируем ключи сразу при создании data channel
    let rng = ring_rand::SystemRandom::new();
    let my_priv = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng).unwrap();
    let my_pub = my_priv.compute_public_key().unwrap();
    let my_pub_bytes = <[u8; 32]>::try_from(my_pub.as_ref()).unwrap();
    *MY_PRIV.lock().unwrap() = Some(my_priv);
    *MY_PUB.lock().unwrap() = Some(my_pub_bytes);
    log(&format!(
        "Generated pub key: {}",
        hex::encode(my_pub.as_ref())
    ));

    // Отправляем наш pub-key когда data channel открыт
    dc.on_open(Box::new({
        let dc = dc.clone();
        move || {
            log("Data channel opened, sending pub key...");
            tauri::async_runtime::spawn({
                let dc = dc.clone();
                async move {
                    let _result = dc.send(&Bytes::from(my_pub.as_ref().to_vec())).await;
                    log(&format!("Sent pub key: {}", hex::encode(my_pub.as_ref())));
                }
            });
            Box::pin(async {})
        }
    }));

    dc.on_message(Box::new(|msg| {
        log(&format!("Received message, length: {}", msg.data.len()));

        // ----- если это 32-байтовый pub-key -----
        if msg.data.len() == 32 {
            let peer_pub = <[u8; 32]>::try_from(&msg.data[..32]).unwrap();
            log(&format!("Received pub key: {}", hex::encode(&peer_pub)));

            // Проверяем, не создали ли мы уже криптографический контекст
            if CRYPTO.lock().unwrap().is_some() {
                log("Crypto context already exists, skipping...");
                return Box::pin(async {});
            }

            // Строим криптографический контекст
            let ctx = build_ctx(&peer_pub);
            log(&format!("SAS generated: {}", ctx.sas));
            *CRYPTO.lock().unwrap() = Some(ctx);

            // Всегда отправляем событие подключения после установки криптографического контекста
            log("Crypto context established, sending connected event");

            // Проверим, что fingerprint доступен сразу после создания контекста
            let _test_fp = get_fingerprint();
            log(&format!(
                "Fingerprint immediately after context creation: {:?}",
                _test_fp
            ));

            // Проверим APP handle перед отправкой события
            let _app_exists = APP.lock().unwrap().is_some();
            log(&format!(
                "APP handle exists before emit_connected: {}",
                _app_exists
            ));

            // Отправляем событие подключения
            log("Sending ssc-connected event immediately");
            emit_connected();

            return Box::pin(async {});
        }

        // ----- иначе зашифрованное сообщение -----
        let mut lock = CRYPTO.lock().unwrap();
        if let Some(ref mut ctx) = *lock {
            if msg.data.len() < TAG_LEN {
                log(&format!(
                    "Message too short: {} < {}",
                    msg.data.len(),
                    TAG_LEN
                ));
                return Box::pin(async {});
            }

            let nonce = u64_to_nonce(ctx.recv_n);
            let ciphertext = &msg.data[..];

            match ctx.opening.decrypt(&nonce, ciphertext) {
                Ok(plaintext) => {
                    // Простая защита от replay: проверяем что sequence number больше последнего принятого
                    if ctx.recv_n > ctx.last_accepted_recv {
                        // Обновляем последний принятый sequence number
                        ctx.last_accepted_recv = ctx.recv_n;
                        ctx.recv_n += 1;

                        let plain = String::from_utf8_lossy(&plaintext).to_string();
                        log(&format!("Decrypted message: {}", plain));
                        emit_message(&plain);
                    } else {
                        log(&format!(
                            "Replay attack detected: received seq {} <= last accepted seq {}",
                            ctx.recv_n, ctx.last_accepted_recv
                        ));
                    }
                }
                Err(_) => {
                    log(&format!(
                        "Failed to decrypt message with seq {}",
                        ctx.recv_n
                    ));
                }
            }
        } else {
            log("No crypto context available for message decryption");
        }
        Box::pin(async {})
    }));

    dc.on_close(Box::new(|| {
        log("Data channel closed - emitting disconnected");
        emit_disconnected();
        Box::pin(async {})
    }));
}
