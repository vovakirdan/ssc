use crate::peer::state::{MY_PRIV, MY_PUB};
use crate::peer::types::{ConnectionBundle, SdpPayload};
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

/// Безопасная обёртка для ключа с автоматической очисткой памяти
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ZeroizedKey {
    pub key: [u8; 32],
}

impl ZeroizedKey {
    fn new(key: [u8; 32]) -> Self {
        Self { key }
    }
}

/// Контекст шифрования
pub struct CryptoCtx {
    pub sealing: ChaCha20Poly1305,
    pub opening: ChaCha20Poly1305,
    pub send_n: u64,
    pub recv_n: u64,
    pub last_accepted_recv: u64, // Защита от replay - последний принятый recv sequence number
    pub sas: String,
    // Храним ключи в безопасной обёртке для возможности очистки
    pub _send_key: ZeroizedKey,
    pub _recv_key: ZeroizedKey,
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

/// Создаём контекст шифрования
pub fn build_ctx(peer_pub: &[u8; 32]) -> CryptoCtx {
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

    let sealing = ChaCha20Poly1305::new(&Key::from(send_key));
    let opening = ChaCha20Poly1305::new(&Key::from(recv_key));

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

/// Преобразование u64 в nonce
pub fn u64_to_nonce(v: u64) -> Nonce<ChaCha20Poly1305> {
    let mut b = [0u8; 12];
    b[4..].copy_from_slice(&v.to_be_bytes());
    *Nonce::<ChaCha20Poly1305>::from_slice(&b)
}

pub fn enc(p: &SdpPayload) -> String {
    // 1. JSON -> bytes
    let json = serde_json::to_vec(p).unwrap();

    // 2. GZIP compress
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&json).unwrap();
    let compressed = gz.finish().unwrap();

    // 3. base64
    general_purpose::STANDARD.encode(compressed)
}

pub fn dec(s: &str) -> SdpPayload {
    // 1. base64 -> bytes
    let compressed = general_purpose::STANDARD.decode(s).unwrap();

    // 2. gunzip с ограничением размера для защиты от zip-bomb
    let gz = GzDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    // Ограничиваем размер распаковываемых данных до 256 KiB
    const MAX_DECOMPRESSED_SIZE: u64 = 256 * 1024; // 256 KiB
    let mut limited_reader = gz.take(MAX_DECOMPRESSED_SIZE);
    limited_reader.read_to_end(&mut json).unwrap();

    // 3. JSON -> struct
    serde_json::from_slice(&json).unwrap()
}

pub fn dec_bundle(s: &str) -> ConnectionBundle {
    // 1. base64 -> bytes
    let compressed = general_purpose::STANDARD.decode(s).unwrap();

    // 2. gunzip с ограничением размера для защиты от zip-bomb
    let gz = GzDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    // Ограничиваем размер распаковываемых данных до 256 KiB
    const MAX_DECOMPRESSED_SIZE: u64 = 256 * 1024; // 256 KiB
    let mut limited_reader = gz.take(MAX_DECOMPRESSED_SIZE);
    limited_reader.read_to_end(&mut json).unwrap();

    // 3. JSON -> struct
    serde_json::from_slice(&json).unwrap()
}
