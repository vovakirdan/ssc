use base64::{engine::general_purpose, Engine as _};
use bytes::Bytes;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use hkdf::Hkdf;
use once_cell::sync::Lazy;
use rand::Rng;
use ring::{agreement, rand as ring_rand};
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
use zeroize::{Zeroize, ZeroizeOnDrop};
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng, Nonce},
    ChaCha20Poly1305, Key,
};

/// ========== GLOBALS ==========
static PEER: Lazy<Mutex<Option<Arc<RTCPeerConnection>>>> = Lazy::new(|| Mutex::new(None));
static DATA_CH: Lazy<Mutex<Option<Arc<RTCDataChannel>>>> = Lazy::new(|| Mutex::new(None));
static CRYPTO: Lazy<Mutex<Option<CryptoCtx>>> = Lazy::new(|| Mutex::new(None));
static MY_PRIV: Lazy<Mutex<Option<agreement::EphemeralPrivateKey>>> =
    Lazy::new(|| Mutex::new(None));
static MY_PUB: Lazy<Mutex<Option<[u8; 32]>>> = Lazy::new(|| Mutex::new(None));
pub static APP: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

const TAG_LEN: usize = 16;

#[derive(Serialize, Deserialize)]
pub struct SdpPayload {
    pub sdp: RTCSessionDescription,
    pub id: String,
    pub ts: i64,
}

/// Безопасная обёртка для ключа с автоматической очисткой памяти
#[derive(Zeroize, ZeroizeOnDrop)]
struct ZeroizedKey {
    key: [u8; 32],
}

impl ZeroizedKey {
    fn new(key: [u8; 32]) -> Self {
        Self { key }
    }
    
    fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

struct CryptoCtx {
    sealing: ChaCha20Poly1305,
    opening: ChaCha20Poly1305,
    send_n: u64,
    recv_n: u64,
    last_accepted_recv: u64, // Защита от replay - последний принятый recv sequence number
    sas: String,
    // Храним ключи в безопасной обёртке для возможности очистки
    _send_key: ZeroizedKey,
    _recv_key: ZeroizedKey,
}

impl Drop for CryptoCtx {
    fn drop(&mut self) {
        // Зануляем все чувствительные данные
        self.send_n.zeroize();
        self.recv_n.zeroize();
        self.last_accepted_recv.zeroize();
        self.sas.zeroize();
        // ZeroizedKey автоматически очистится благодаря ZeroizeOnDrop
    }
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
    let crypto_guard = CRYPTO.lock().unwrap();
    let result = crypto_guard.as_ref().map(|c| {
        // println!("Found crypto context with SAS: {}", c.sas);
        c.sas.clone()
    });
    // println!("get_fingerprint called, crypto exists: {}, result: {:?}", crypto_guard.is_some(), result); // Отладочная информация
    result
}

fn u64_to_nonce(v: u64) -> Nonce<ChaCha20Poly1305> {
    let mut b = [0u8; 12];
    b[4..].copy_from_slice(&v.to_be_bytes());
    *Nonce::<ChaCha20Poly1305>::from_slice(&b)
}

fn build_ctx(peer_pub: &[u8; 32]) -> CryptoCtx {
    // ----- свой ключ -----
    let my_priv = MY_PRIV
        .lock()
        .unwrap()
        .take()
        .expect("private key must exist during key-exchange");

    // ----- общий секрет -----
    let peer_pub_key = agreement::UnparsedPublicKey::new(&agreement::X25519, peer_pub);
    let mut shared =
        agreement::agree_ephemeral(my_priv, &peer_pub_key, |secret| secret.to_vec()).unwrap();

    // ----- разделение ключей по направлениям -----
    // Получаем 64 байта из HKDF для двух ключей
    let hk = Hkdf::<Sha256>::new(None, &shared);
    let mut okm = [0u8; 64];
    hk.expand(b"ssc-chat", &mut okm).unwrap();
    
    // Очищаем shared сразу после использования
    shared.zeroize();
    
    let (k1, k2) = okm.split_at(32);

    // Получаем собственный публичный ключ из глобальной переменной
    let my_pub = MY_PUB
        .lock()
        .unwrap()
        .expect("my_pub must be set before building crypto context");

    // Детерминированно выбираем ключи на основе публичных ключей
    let (send_key_slice, recv_key_slice) = if my_pub < *peer_pub {
        (k1, k2)
    } else {
        (k2, k1)
    };
    
    // Копируем ключи в массивы
    let mut send_key = [0u8; 32];
    let mut recv_key = [0u8; 32];
    send_key.copy_from_slice(send_key_slice);
    recv_key.copy_from_slice(recv_key_slice);

    // ----- SAS на основе первого ключа -----
    let fp_raw = Sha256::digest(k1);
    let sas = hex::encode(&fp_raw[..6]); // PARANOID mode: 48 bits (6 bytes = 12 hex chars)
    
    // Очищаем okm после использования
    okm.zeroize();

    let sealing = ChaCha20Poly1305::new(Key::from(send_key));
    let opening = ChaCha20Poly1305::new(Key::from(recv_key));
    
    // Создаём безопасные обёртки для ключей
    let send_key_wrapped = ZeroizedKey::new(send_key);
    let recv_key_wrapped = ZeroizedKey::new(recv_key);
    
    // Очищаем временные копии ключей
    send_key.zeroize();
    recv_key.zeroize();

    CryptoCtx {
        sealing,
        opening,
        send_n: 1,
        recv_n: 1,
        last_accepted_recv: 0, // Начинаем с 0, первое сообщение будет иметь sequence = 1
        sas: sas,
        _send_key: send_key_wrapped,
        _recv_key: recv_key_wrapped,
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
    // println!("emit_state called with event: {}", evt); // Отладочная информация
    if let Some(app) = APP.lock().unwrap().clone() {
        // println!("APP handle exists, emitting event: {}", evt); // Отладочная информация
        let _result = app.emit(evt, ());
        // println!("Emit result: {:?}", result); // Отладочная информация
    } else {
        // println!("APP handle is None, cannot emit event: {}", evt); // Отладочная информация
    }
}

fn emit_connected() {
    emit_state("ssc-connected");
}

fn emit_disconnected() {
    // println!("emit_disconnected called - clearing all state"); // Отладочная информация
    // println!("Clearing CRYPTO context in emit_disconnected");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;
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
            RTCPeerConnectionState::Connected => {
                // Не отправляем событие подключения здесь, ждем установки криптографического контекста
            }
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
    // println!("attach_dc called - clearing old state"); // Отладочная информация
    
    // Очищаем старое состояние перед созданием нового соединения
    // println!("Clearing CRYPTO context in attach_dc");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;
    
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
    // println!("Generated pub key: {}", hex::encode(my_pub.as_ref())); // Отладочная информация

    // Отправляем наш pub-key когда data channel открыт
    dc.on_open(Box::new({
        let dc = dc.clone();
        move || {
            // println!("Data channel opened, sending pub key..."); // Отладочная информация
            tauri::async_runtime::spawn({
                let dc = dc.clone();
                async move {
                    let _result = dc.send(&Bytes::from(my_pub.as_ref().to_vec())).await;
                    // println!("Sent pub key: {}", hex::encode(my_pub.as_ref())); // Отладочная информация
                }
            });
            Box::pin(async {})
        }
    }));

    dc.on_message(Box::new(|msg| {
        // println!("Received message, length: {}", msg.data.len()); // Отладочная информация

        // ----- если это 32-байтовый pub-key -----
        if msg.data.len() == 32 {
            let peer_pub = <[u8; 32]>::try_from(&msg.data[..32]).unwrap();
            // println!("Received pub key: {}", hex::encode(&peer_pub)); // Отладочная информация

            // Проверяем, не создали ли мы уже криптографический контекст
            if CRYPTO.lock().unwrap().is_some() {
                // println!("Crypto context already exists, skipping..."); // Отладочная информация
                return Box::pin(async {});
            }

            // Строим криптографический контекст
            let ctx = build_ctx(&peer_pub);
            // println!("SAS generated: {}", ctx.sas); // Отладочная информация
            *CRYPTO.lock().unwrap() = Some(ctx);

            // Всегда отправляем событие подключения после установки криптографического контекста
            // println!("Crypto context established, sending connected event"); // Отладочная информация
            
            // Проверим, что fingerprint доступен сразу после создания контекста
            let _test_fp = get_fingerprint();
            // println!("Fingerprint immediately after context creation: {:?}", test_fp);
            
            // Проверим APP handle перед отправкой события
            let _app_exists = APP.lock().unwrap().is_some();
            // println!("APP handle exists before emit_connected: {}", app_exists);
            
            // Отправляем событие подключения
            // println!("Sending ssc-connected event immediately");
            emit_connected();
            
            return Box::pin(async {});
        }

        // ----- иначе зашифрованное сообщение -----
        let mut lock = CRYPTO.lock().unwrap();
        if let Some(ref mut ctx) = *lock {
            if msg.data.len() < TAG_LEN {
                // println!("Message too short: {} < {}", msg.data.len(), TAG_LEN);
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
                        // println!("Decrypted message: {}", plain);
                        emit_message(&plain);
                    } else {
                        // println!(
                        //     "Replay attack detected: received seq {} <= last accepted seq {}",
                        //     ctx.recv_n, ctx.last_accepted_recv
                        // );
                    }
                }
                Err(_) => {
                    // println!("Failed to decrypt message with seq {}", ctx.recv_n);
                }
            }
        } else {
            // println!("No crypto context available for message decryption");
        }
        Box::pin(async {})
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

/// проверка готовности соединения
pub fn is_connected() -> bool {
    CRYPTO.lock().unwrap().is_some()
}

/// текст по каналу
pub async fn send_text(text: String) -> bool {
    // println!("send_text called with: {}", text);
    let dc = { DATA_CH.lock().unwrap().as_ref().cloned() };
    if let Some(dc) = dc {
        // Получаем данные из мьютекса и освобождаем его
        let result = {
            let mut crypto_guard = CRYPTO.lock().unwrap();
            if let Some(ref mut ctx) = *crypto_guard {
                let seq_num = ctx.send_n;
                let nonce = u64_to_nonce(seq_num);
                ctx.send_n += 1;

                let plaintext = text.into_bytes();
                match ctx.sealing.encrypt(&nonce, plaintext.as_ref()) {
                    Ok(ciphertext) => {
                        // println!("Encrypted message with seq {}, length: {}", seq_num, ciphertext.len());
                        Some(ciphertext)
                    }
                    Err(_) => {
                        // println!("Encryption failed");
                        None
                    }
                }
            } else {
                // println!("No crypto context available for sending");
                None
            }
        }; // мьютекс освобождается здесь

        if let Some(ciphertext) = result {
            let send_result = dc.send(&Bytes::from(ciphertext)).await.is_ok();
            // println!("Send result: {}", send_result);
            return send_result;
        }
    }
    // println!("No data channel available for sending");
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

    // очищаем криптографический контекст и ключи
    // println!("Clearing CRYPTO context in disconnect");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;

    // отправляем событие отключения
    emit_disconnected();
}
