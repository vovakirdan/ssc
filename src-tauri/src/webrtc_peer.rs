use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use hkdf::Hkdf;
use once_cell::sync::Lazy;
use rand::Rng;
use ring::{aead, agreement, rand as ring_rand};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use webrtc::{
    api::APIBuilder,
    data_channel::{data_channel_init::RTCDataChannelInit, RTCDataChannel},
    ice_transport::ice_server::RTCIceServer,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription, RTCPeerConnection,
    },
};

/// ========== GLOBALS ==========
static PEER: Lazy<Mutex<Option<Arc<RTCPeerConnection>>>> = Lazy::new(|| Mutex::new(None));
static DATA_CH: Lazy<Mutex<Option<Arc<RTCDataChannel>>>> = Lazy::new(|| Mutex::new(None));
static CRYPTO: Lazy<Mutex<Option<CryptoCtx>>> = Lazy::new(|| Mutex::new(None));
static MY_PRIV: Lazy<Mutex<Option<agreement::EphemeralPrivateKey>>> = Lazy::new(|| Mutex::new(None));
pub static APP: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

const TAG_LEN: usize = 16;

#[derive(Serialize, Deserialize)]
pub struct SdpPayload {
    pub sdp: RTCSessionDescription,
    pub id: String,
    pub ts: i64,
}

struct CryptoCtx {
    sealing: aead::LessSafeKey,
    opening: aead::LessSafeKey,
    send_n: u64,
    recv_n: u64,
    sas : String,
}

/// ========== HELPERS ==========
fn rtc_config() -> RTCConfiguration {
    RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec![
                "stun:stun.l.google.com:19302".into(),
                "stun:stun1.l.google.com:19302".into(),
            ],
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn random_id() -> String {
    hex::encode(rand::rng().random::<[u8; 8]>())
}

pub fn get_fingerprint() -> Option<String> {
    let result = CRYPTO.lock().unwrap().as_ref().map(|c| c.sas.clone());
    // println!("get_fingerprint called, result: {:?}", result); // Отладочная информация
    result
}

fn u64_to_nonce(v: u64) -> aead::Nonce {
    let mut b = [0u8; 12];
    b[4..].copy_from_slice(&v.to_be_bytes());
    aead::Nonce::assume_unique_for_key(b)
}

fn build_ctx(peer_pub: &[u8; 32]) -> CryptoCtx {
    // ----- свой ключ -----
    let my_priv = MY_PRIV.lock().unwrap().take().expect("private key must exist during key-exchange");

    // ----- общий секрет -----
    let peer_pub_key = agreement::UnparsedPublicKey::new(&agreement::X25519, peer_pub);
    let shared = agreement::agree_ephemeral(
        my_priv,
        &peer_pub_key,
        |secret| secret.to_vec(),
    )
    .unwrap();

    // ----- ключ шифрования -----
    let hk  = Hkdf::<Sha256>::new(None, &shared);
    let mut key = [0u8; 32];
    hk.expand(b"ssc-chat", &mut key).unwrap();

    // ----- SAS -----
    let fp_raw = Sha256::digest(&key);
    let sas = hex::encode(&fp_raw[..6]); // PARANOID mode: 48 bits (6 bytes = 12 hex chars)

    let sealing = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key).unwrap());
    let opening = aead::LessSafeKey::new(aead::UnboundKey::new(&aead::CHACHA20_POLY1305, &key).unwrap());

    CryptoCtx { 
        sealing, 
        opening, 
        send_n: 0, 
        recv_n: 0, 
        sas : sas,
    }
}

fn enc(p: &SdpPayload) -> String {
    // 1. JSON -> bytes
    let json = serde_json::to_vec(p).unwrap();

    // 2. GZIP compress
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&json).unwrap();
    let compressed = gz.finish().unwrap();

    // 3. base64
    general_purpose::STANDARD.encode(compressed)
}

fn dec(s: &str) -> SdpPayload {
    // 1. base64 -> bytes
    let compressed = general_purpose::STANDARD.decode(s).unwrap();

    // 2. gunzip
    let mut gz = GzDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    gz.read_to_end(&mut json).unwrap();

    // 3. JSON -> struct
    serde_json::from_slice(&json).unwrap()
}

fn emit_state(evt: &str) {
    if let Some(app) = APP.lock().unwrap().clone() {
        let _ = app.emit(evt, ());
    }
}

fn emit_connected() {
    emit_state("ssc-connected");
}

fn emit_disconnected() {
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    emit_state("ssc-disconnected");
}

fn emit_message(msg: &str) {
    if let Some(app) = APP.lock().unwrap().clone() {
        let _ = app.emit("ssc-message", msg);
    }
}

async fn wait_ice(pc: &RTCPeerConnection) {
    let mut done = pc.gathering_complete_promise().await;
    done.recv().await;
}

/// создаём Peer; если `initiator`, то сами делаем data-channel
async fn new_peer(initiator: bool) -> Arc<RTCPeerConnection> {
    let api = APIBuilder::new().build();
    let pc = Arc::new(api.new_peer_connection(rtc_config()).await.unwrap());

    pc.on_peer_connection_state_change(Box::new(|st: RTCPeerConnectionState| {
        match st {
            RTCPeerConnectionState::Connected => emit_connected(),
            RTCPeerConnectionState::Disconnected
            | RTCPeerConnectionState::Failed
            | RTCPeerConnectionState::Closed => emit_disconnected(),
            _ => {}
        }
        Box::pin(async {})
    }));

    if initiator {
        let dc = pc
            .create_data_channel("ssc-data", Some(RTCDataChannelInit::default()))
            .await
            .unwrap();
        attach_dc(&dc);
    } else {
        pc.on_data_channel(Box::new(|dc: Arc<RTCDataChannel>| {
            attach_dc(&dc);
            Box::pin(async {})
        }));
    }
    pc
}

/// общий обработчик data-channel
fn attach_dc(dc: &Arc<RTCDataChannel>) {
    // println!("attach_dc called"); // Отладочная информация
    {
        *DATA_CH.lock().unwrap() = Some(dc.clone());
    }

    // Генерируем ключи сразу при создании data channel
    let rng = ring_rand::SystemRandom::new();
    let my_priv = agreement::EphemeralPrivateKey::generate(&agreement::X25519, &rng).unwrap();
    let my_pub = my_priv.compute_public_key().unwrap();
    *MY_PRIV.lock().unwrap() = Some(my_priv);
    // println!("Generated pub key: {}", hex::encode(my_pub.as_ref())); // Отладочная информация

    // Отправляем наш pub-key когда data channel открыт
    dc.on_open(Box::new({
        let dc = dc.clone();
        move || {
            // println!("Data channel opened, sending pub key..."); // Отладочная информация
            tauri::async_runtime::spawn({
                let dc = dc.clone();
                async move {
                    let result = dc.send(&Bytes::from(my_pub.as_ref().to_vec())).await;
                    // println!("Send result: {:?}", result); // Отладочная информация
                    
                    // После отправки pub-key проверяем, есть ли у нас уже криптографический контекст
                    if CRYPTO.lock().unwrap().is_some() {
                        // println!("We already have crypto context, sending connected event"); // Отладочная информация
                        emit_connected();
                    }
                }
            });
            Box::pin(async {})
        }
    }));

    dc.on_message(Box::new(|msg| {
        // println!("Received message, length: {}", msg.data.len()); // Отладочная информация
        
        // ----- если это 32-байтовый pub-key -----
        if msg.data.len() == 32 {
            let peer_pub = <[u8;32]>::try_from(&msg.data[..32]).unwrap();
            // println!("Received pub key: {}", hex::encode(&peer_pub)); // Отладочная информация
            
            // Строим криптографический контекст
            let ctx = build_ctx(&peer_pub);
            // println!("SAS generated: {}", ctx.sas); // Отладочная информация
            *CRYPTO.lock().unwrap() = Some(ctx);
            
            // Отправляем событие подключения только если у нас есть приватный ключ (мы уже отправили свой pub-key)
            if MY_PRIV.lock().unwrap().is_none() {
                // println!("Sending connected event"); // Отладочная информация
                emit_connected();
            } else {
                // println!("Waiting for our pub-key to be sent before emitting connected"); // Отладочная информация
            }
            return Box::pin(async{});
        }
    
        // ----- иначе зашифрованное сообщение -----
        let mut lock = CRYPTO.lock().unwrap();
        if let Some(ref mut ctx) = *lock {
            if msg.data.len() < TAG_LEN { return Box::pin(async{}) }
    
            let nonce = u64_to_nonce(ctx.recv_n); ctx.recv_n += 1;
            let mut buf = msg.data.to_vec();
    
            if ctx.opening.open_in_place(nonce, aead::Aad::empty(), &mut buf).is_ok() {
                let plain = String::from_utf8_lossy(&buf[..buf.len()-TAG_LEN]).to_string();
                emit_message(&plain);
            }
        }
        Box::pin(async{})
    }));    

    dc.on_close(Box::new(|| {
        emit_disconnected();
        Box::pin(async {})
    }));
}

/// ========== PUBLIC API ==========

/// A-сторона: создаём OFFER → base64
pub async fn generate_offer() -> String {
    let pc = new_peer(true).await;
    {
        *PEER.lock().unwrap() = Some(pc.clone());
    }

    let offer = pc.create_offer(None).await.unwrap();
    pc.set_local_description(offer).await.unwrap();
    wait_ice(&pc).await; // ← обратно вернули ICE-кандидаты

    enc(&SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id: random_id(),
        ts: chrono::Utc::now().timestamp(),
    })
}

/// B-сторона: получает OFFER, делает ANSWER → base64
pub async fn accept_offer_and_create_answer(encoded: String) -> String {
    let offer: SdpPayload = dec(&encoded);
    let pc = new_peer(false).await;
    {
        *PEER.lock().unwrap() = Some(pc.clone());
    }

    pc.set_remote_description(offer.sdp).await.unwrap();
    let answer = pc.create_answer(None).await.unwrap();
    pc.set_local_description(answer).await.unwrap();
    wait_ice(&pc).await;

    enc(&SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id: offer.id,
        ts: chrono::Utc::now().timestamp(),
    })
}

/// A-сторона: получает ANSWER и завершает handshake
pub async fn set_answer(encoded: String) -> bool {
    let answer: SdpPayload = dec(&encoded);
    let pc = { PEER.lock().unwrap().as_ref().cloned() };
    if let Some(pc) = pc {
        pc.set_remote_description(answer.sdp).await.is_ok()
    } else {
        false
    }
}

/// текст по каналу
pub async fn send_text(text: String) -> bool {
    let dc = { DATA_CH.lock().unwrap().as_ref().cloned() };
    if let Some(dc) = dc {
        // Получаем данные из мьютекса и освобождаем его
        let buf = {
            let mut crypto_guard = CRYPTO.lock().unwrap();
            if let Some(ref mut ctx) = *crypto_guard {
                let nonce = u64_to_nonce(ctx.send_n); 
                ctx.send_n += 1;

                let mut buf = text.into_bytes();
                ctx.sealing.seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut buf).unwrap();
                
                buf
            } else {
                return false;
            }
        }; // мьютекс освобождается здесь

        return dc.send(&Bytes::from(buf)).await.is_ok();
    }
    false
}

/// ручное разъединение
pub async fn disconnect() {
    // извлекаем data channel и освобождаем мьютекс
    let dc = DATA_CH.lock().unwrap().take();
    if let Some(dc) = dc {
        let _ = dc.close().await;
    }
    
    // извлекаем peer connection и освобождаем мьютекс
    let pc = PEER.lock().unwrap().take();
    if let Some(pc) = pc {
        let _ = pc.close().await;
    }
    
    // очищаем криптографический контекст и приватный ключ
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    
    // отправляем событие отключения
    emit_disconnected();
}
