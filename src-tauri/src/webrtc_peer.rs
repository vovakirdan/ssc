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
use tokio::time::sleep;
use webrtc::{
    api::APIBuilder,
    data_channel::{data_channel_init::RTCDataChannelInit, RTCDataChannel},
    ice_transport::{ice_server::RTCIceServer, ice_gatherer_state::RTCIceGathererState},
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription, RTCPeerConnection,
    },
};
use zeroize::{Zeroize, ZeroizeOnDrop};
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::peer_connection::policy::bundle_policy::RTCBundlePolicy;
use webrtc::peer_connection::policy::rtcp_mux_policy::RTCRtcpMuxPolicy;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use tokio::time::timeout;
use tokio::sync::mpsc;

/// ========== GLOBAL STATE ==========

/// WebRTC Peer Connection
static PEER: Lazy<Mutex<Option<Arc<RTCPeerConnection>>>> = Lazy::new(|| Mutex::new(None));

/// Data Channel для обмена сообщениями
static DATA_CH: Lazy<Mutex<Option<Arc<RTCDataChannel>>>> = Lazy::new(|| Mutex::new(None));

/// Криптографический контекст для шифрования
static CRYPTO: Lazy<Mutex<Option<CryptoCtx>>> = Lazy::new(|| Mutex::new(None));

/// Приватный ключ для обмена
static MY_PRIV: Lazy<Mutex<Option<agreement::EphemeralPrivateKey>>> =
    Lazy::new(|| Mutex::new(None));

/// Публичный ключ для обмена
static MY_PUB: Lazy<Mutex<Option<[u8; 32]>>> = Lazy::new(|| Mutex::new(None));

/// Handle для отправки событий в Tauri
pub static APP: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

/// Флаг установленного соединения
static WAS_CONNECTED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Отложенная задача для graceful disconnect
static DISCONNECT_TASK: Lazy<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> =
    Lazy::new(|| Mutex::new(None));

/// Кандидаты, полученные до установки remote description
static PENDING_REMOTE_CANDIDATES: Lazy<Mutex<Vec<IceCandidate>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Локальные кандидаты для текущего соединения
static LOCAL_CANDIDATES: Lazy<Mutex<Vec<IceCandidate>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Флаг активного сбора кандидатов
static COLLECTING_CANDIDATES: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Глобальное хранилище для пользовательских ICE серверов
static USER_ICE_SERVERS: Lazy<Mutex<Option<Vec<ServerConfig>>>> = Lazy::new(|| Mutex::new(None));

/// ========== CONSTANTS ==========

/// Длина тега аутентификации для ChaCha20-Poly1305
const TAG_LEN: usize = 16;

/// Период ожидания перед принудительным отключением
const GRACE_PERIOD: Duration = Duration::from_secs(10);

// ========== PUBLIC TYPES ==========

/// Полезная нагрузка SDP с метаданными
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SdpPayload {
    pub sdp: RTCSessionDescription,
    pub id: String,
    pub ts: i64,
}

/// ICE кандидат для WebRTC соединения
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub connection_id: String, // ID соединения для сопоставления
}

/// Полный пакет соединения с SDP и кандидатами
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConnectionBundle {
    pub sdp_payload: SdpPayload,
    pub ice_candidates: Vec<IceCandidate>,
}

/// Конфигурация ICE сервера
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    pub id: String,
    pub r#type: String, // 'stun' or 'turn'
    pub url: String,
    pub username: Option<String>,
    pub credential: Option<String>,
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

/// ========== UTILITY FUNCTIONS ==========

/// Логирование с временными метками
fn log(msg: &str) {
    // Проверяем конфигурацию логирования
    if crate::config::LOGGING_ENABLED {
        #[cfg(debug_assertions)]
        {
            // В режиме разработки дополнительно проверяем dev::ENABLE_LOGGING
            if !crate::config::dev::ENABLE_LOGGING {
                return;
            }
        }
        
        let now = chrono::Local::now();
        println!("RUST: [{}] {}", now.format("%Y-%m-%d %H:%M:%S%.3f"), msg);
    }
}

/// Печать ICE-candidate при появлении (Trickle-ICE)
async fn dump_candidate(label: &str, cand: &RTCIceCandidate) {
    if let Ok(init) = cand.to_json() {
        log(&format!(
            "Trickle {label}: candidate={} sdp_mid={:?} sdp_mline_index={:?} username_fragment={:?}",
            init.candidate, init.sdp_mid, init.sdp_mline_index, init.username_fragment
        ));
    }
}

/// Быстрый снимок getStats → выбранная пара
async fn dump_selected_pair(pc: &RTCPeerConnection, moment: &str) {
    let stats = pc.get_stats().await;
    for (_, v) in stats.reports {
        match v {
            webrtc::stats::StatsReportType::CandidatePair(pair) => {
                if pair.nominated {
                    log(&format!(
                        "STATS {moment}: {}:{}  type: {:?}  bytes={}/{} state={:?}",
                        pair.local_candidate_id, pair.remote_candidate_id,
                        pair.stats_type,
                        pair.bytes_sent, pair.bytes_received, pair.state
                    ));
                }
            }
            _ => {}
        }
    }
}

// Функция для добавления схемы протокола к URL ICE сервера, если она отсутствует
fn add_ice_url_scheme(config: &ServerConfig) -> String {
    // Если url уже начинается с "turn:" или "stun:", возвращаем как есть
    if config.url.starts_with("turn:") || config.url.starts_with("stun:") {
        config.url.clone()
    } else {
        // В зависимости от типа сервера добавляем нужную схему
        let scheme = if config.r#type == "turn" { "turn:" } else { "stun:" };
        format!("{}{}", scheme, config.url)
    }
}

pub async fn check_ice_server_availability(config: ServerConfig) -> bool {
    log(&format!("check_ice_server_availability called with config: {:?}", config));

    let url = add_ice_url_scheme(&config);
    
    log(&format!("Processed URL: '{}' -> '{}'", config.url, url));
    
    // Создаем ICE сервер из конфигурации
    let ice_server = RTCIceServer {
        urls: vec![url],
        username: config.username.clone().unwrap_or_default(),
        credential: config.credential.clone().unwrap_or_default(),
    };
    
    log(&format!("Created ICE server: urls={:?}, username='{}', credential='{}'", 
                ice_server.urls, ice_server.username, ice_server.credential));

    // Создаем конфигурацию для peer connection
    let rtc_config = RTCConfiguration {
        ice_servers: vec![ice_server],
        ..Default::default()
    };
    
    log(&format!("Created RTC config with {} ICE servers", rtc_config.ice_servers.len()));

    // Создаем API и peer connection
    let api = APIBuilder::new().build();
    log("APIBuilder created successfully");
    
    match api.new_peer_connection(rtc_config).await {
        Ok(peer_connection) => {
            log("Peer connection created successfully, starting ICE gathering check");
            // Проверяем доступность через gathering состояние
            check_via_ice_gathering(peer_connection.into(), &config.r#type).await
        }
        Err(e) => {
            log(&format!("Failed to create peer connection: {:?}", e));
            false
        }
    }
}

async fn check_via_ice_gathering(
    peer_connection: Arc<RTCPeerConnection>,
    server_type: &str,
) -> bool {
    log(&format!("check_via_ice_gathering called with server_type: {}", server_type));
    
    let (tx, mut rx) = mpsc::channel(10);
    let tx_clone = tx.clone();
    
    // Подписываемся на изменения состояния gathering
    peer_connection.on_ice_gathering_state_change(Box::new(move |state| {
        let tx = tx_clone.clone();
        log(&format!("ICE gathering state changed to: {:?}", state));
        tokio::spawn(async move {
            let _ = tx.send(state).await;
        });
        Box::pin(async {})
    }));

    // Подписываемся на ICE кандидатов
    let (candidate_tx, mut candidate_rx) = mpsc::channel(10);
    let server_type_clone = server_type.to_string();
    
    peer_connection.on_ice_candidate(Box::new(move |candidate| {
        let tx = candidate_tx.clone();
        let server_type = server_type_clone.clone();
        
        Box::pin(async move {
            if let Some(c) = candidate {
                log(&format!("Received ICE candidate: {:?}", c));
                // Проверяем тип кандидата
                let candidate_type = c.to_json().map(|json| {
                    log(&format!("Candidate JSON: candidate='{}'", json.candidate));
                    // Для STUN серверов ищем srflx кандидатов
                    // Для TURN серверов ищем relay кандидатов
                    if server_type == "stun" && json.candidate.contains("srflx") {
                        log("Found srflx candidate for STUN server");
                        true
                    } else if server_type == "turn" && json.candidate.contains("relay") {
                        log("Found relay candidate for TURN server");
                        true
                    } else {
                        log(&format!("Candidate type mismatch: expected {} but got candidate: {}", server_type, json.candidate));
                        false
                    }
                }).unwrap_or_else(|e| {
                    log(&format!("Failed to get candidate JSON: {:?}", e));
                    false
                });
                
                if candidate_type {
                    log("Sending success signal for candidate match");
                    let _ = tx.send(true).await;
                }
            } else {
                log("Received null candidate (gathering complete)");
            }
        })
    }));

    // Создаем data channel для инициации ICE gathering
    log("Creating data channel to initiate ICE gathering");
    match peer_connection.create_data_channel("test", None).await {
        Ok(_) => {
            log("Data channel created successfully");
        },
        Err(e) => {
            log(&format!("Failed to create data channel: {:?}", e));
            return false;
        }
    }

    // Создаем offer для запуска ICE gathering
    log("Creating offer to start ICE gathering");
    match peer_connection.create_offer(None).await {
        Ok(offer) => {
            log("Offer created successfully, setting local description");
            if let Err(e) = peer_connection.set_local_description(offer).await {
                log(&format!("Failed to set local description: {:?}", e));
                return false;
            }
            log("Local description set successfully");
        }
        Err(e) => {
            log(&format!("Failed to create offer: {:?}", e));
            return false;
        }
    }

    // Ждем результат с таймаутом
    let check_timeout = Duration::from_secs(10);
    log(&format!("Starting timeout wait of {} seconds", check_timeout.as_secs()));
    
    tokio::select! {
        // Ждем подходящего кандидата
        result = timeout(check_timeout, candidate_rx.recv()) => {
            match result {
                Ok(Some(true)) => {
                    log("Received success signal from candidate match");
                    let _ = peer_connection.close().await;
                    true
                },
                Ok(Some(false)) => {
                    log("Received false signal from candidate match");
                    let _ = peer_connection.close().await;
                    false
                },
                Ok(None) => {
                    log("Candidate channel closed without success signal");
                    let _ = peer_connection.close().await;
                    false
                },
                Err(_) => {
                    log("Timeout waiting for candidate match");
                    let _ = peer_connection.close().await;
                    false
                }
            }
        }
        // Или ждем failed состояния
        _ = async {
            while let Some(state) = rx.recv().await {
                log(&format!("Received gathering state: {:?}", state));
                if state == RTCIceGathererState::Complete {
                    log("ICE gathering completed");
                    break;
                }
            }
        } => {
            log("Gathering state monitoring completed");
            let _ = peer_connection.close().await;
            false
        }
    }
}

/// Получение конфигурации серверов из фронтенда
pub fn get_user_ice_servers(servers: Vec<ServerConfig>) -> Vec<RTCIceServer> {
    servers.into_iter().map(|config| {
        let url = add_ice_url_scheme(&config);
        
        RTCIceServer {
            urls: vec![url],
            username: config.username.unwrap_or_default(),
            credential: config.credential.unwrap_or_default(),
        }
    }).collect()
}

/// Создает конфигурацию для peer connection
fn rtc_config(custom_servers: Option<Vec<ServerConfig>>) -> RTCConfiguration {
    let ice_servers = if let Some(servers) = custom_servers {
        // Используем пользовательские серверы
        get_user_ice_servers(servers)
    } else {
        // Используем дефолтные серверы
        vec![
            RTCIceServer {
                urls: vec![
                    "stun:stun.l.google.com:19302".into(),
                    "stun:stun1.l.google.com:19302".into(),
                ],
                ..Default::default()
            },
        ]
    };

    RTCConfiguration {
        ice_servers,
        // Добавляем более агрессивные настройки ICE
        ice_candidate_pool_size: 10,
        bundle_policy: RTCBundlePolicy::MaxBundle,
        rtcp_mux_policy: RTCRtcpMuxPolicy::Require,
        // ice_transport_policy: webrtc::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy::Relay,
        ..Default::default()
    }
}

/// Устанавливает пользовательские ICE серверы, возвращает true при успехе, false при ошибке
pub fn set_ice_servers(servers: Vec<ServerConfig>) -> bool {
    log(&format!("Setting {} custom ICE servers", servers.len()));
    
    // Валидация серверов
    for server in &servers {
        if server.url.is_empty() {
            log("Server URL cannot be empty");
            return false;
        }
        
        if server.r#type == "turn" && (server.username.is_none() || server.credential.is_none()) {
            log("TURN servers require username and credential");
            return false;
        }
    }
    
    *USER_ICE_SERVERS.lock().unwrap() = Some(servers);
    log("Custom ICE servers set successfully");
    true
}

/// Получает пользовательские ICE серверы, возвращает дефолтные серверы если не установлены
pub fn get_ice_servers() -> Vec<ServerConfig> {
    USER_ICE_SERVERS.lock().unwrap().clone().unwrap_or_else(|| {
        // Возвращаем дефолтные серверы в формате ServerConfig
        vec![
            ServerConfig {
                id: "default-stun".into(),
                r#type: "stun".into(),
                url: "stun:stun.l.google.com:19302".into(),
                username: None,
                credential: None,
            },
            ServerConfig {
                id: "default-turn".into(),
                r#type: "turn".into(),
                url: "stun:stun1.l.google.com:19302".into(),
                username: None,
                credential: None,
            }
        ]
    })
}

fn random_id() -> String {
    hex::encode(rand::rng().random::<[u8; 8]>())
}

pub fn get_fingerprint() -> Option<String> {
    let crypto_guard = CRYPTO.lock().unwrap();
    let result = crypto_guard.as_ref().map(|c| {
        log(&format!("Found crypto context with SAS: {}", c.sas));
        c.sas.clone()
    });
    log(&format!(
        "get_fingerprint called, crypto exists: {}, result: {:?}",
        crypto_guard.is_some(),
        result
    ));
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

fn dec_bundle(s: &str) -> ConnectionBundle {
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

fn emit_state(evt: &str) {
    log(&format!("emit_state called with event: {}", evt));
    if let Some(app) = APP.lock().unwrap().clone() {
        log(&format!("APP handle exists, emitting event: {}", evt));
        match app.emit(evt, ()) {
            Ok(_) => log(&format!("Successfully emitted event: {}", evt)),
            Err(e) => log(&format!("Failed to emit event {}: {:?}", evt, e)),
        }
    } else {
        log(&format!("APP handle is None, cannot emit event: {}", evt));
    }
}

fn emit_connected() {
    log("emit_connected called - setting WAS_CONNECTED flag");
    *WAS_CONNECTED.lock().unwrap() = true;
    emit_state("ssc-connected");
}

fn emit_disconnected() {
    log("emit_disconnected called - clearing all state");
    log("Clearing CRYPTO context in emit_disconnected");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;
    *WAS_CONNECTED.lock().unwrap() = false;
    
    // очищаем отложенные кандидаты
    PENDING_REMOTE_CANDIDATES.lock().unwrap().clear();
    
    // очищаем локальные кандидаты
    LOCAL_CANDIDATES.lock().unwrap().clear();
    *COLLECTING_CANDIDATES.lock().unwrap() = false;
    
    emit_state("ssc-disconnected");
}

fn emit_message(msg: &str) {
    if let Some(app) = APP.lock().unwrap().clone() {
        let _ = app.emit("ssc-message", msg);
    }
}

fn emit_connection_problem() {
    log("emit_connection_problem called - connection issues detected");
    emit_state("ssc-connection-problem");
}

fn emit_connection_recovering() {
    log("emit_connection_recovering called - connection is recovering");
    emit_state("ssc-connection-recovering");
}

fn emit_connection_recovered() {
    log("emit_connection_recovered called - connection recovered");
    emit_state("ssc-connection-recovered");
}

fn emit_connection_failed() {
    log("emit_connection_failed called - connection recovery failed");
    emit_state("ssc-connection-failed");
}

// Добавляем новую функцию для ожидания кандидатов с таймаутом
async fn wait_for_candidates(timeout_secs: u64) -> Vec<IceCandidate> {
    let start = std::time::Instant::now();
    
    loop {
        // Ждем минимум 2 секунды для сбора кандидатов
        if start.elapsed().as_secs() < 2 {
            sleep(Duration::from_millis(100)).await;
            continue;
        }
        
        // После 2 секунд проверяем состояние
        let collecting = *COLLECTING_CANDIDATES.lock().unwrap();
        let candidates_count = LOCAL_CANDIDATES.lock().unwrap().len();
        
        log(&format!("Candidate collection status: collecting={}, count={}, elapsed={}s", 
                    collecting, candidates_count, start.elapsed().as_secs()));
        
        // Если сбор закончен ИЛИ есть хотя бы relay кандидаты - возвращаем
        if !collecting || candidates_count > 0 {
            break;
        }
        
        // Проверяем таймаут
        if start.elapsed().as_secs() >= timeout_secs {
            log(&format!("Candidate collection timeout after {} seconds", timeout_secs));
            break;
        }
        
        sleep(Duration::from_millis(100)).await;
    }
    
    LOCAL_CANDIDATES.lock().unwrap().clone()
}

fn analyze_candidates(candidates: &[IceCandidate]) {
    let mut host_count = 0;
    let mut srflx_count = 0;
    let mut relay_count = 0;
    
    for candidate in candidates {
        if candidate.candidate.contains("typ host") {
            host_count += 1;
        } else if candidate.candidate.contains("typ srflx") {
            srflx_count += 1;
        } else if candidate.candidate.contains("typ relay") {
            relay_count += 1;
        }
    }
    
    log(&format!(
        "Candidate analysis: {} host, {} srflx, {} relay",
        host_count, srflx_count, relay_count
    ));
    
    if relay_count == 0 {
        log("WARNING: No TURN relay candidates found! Connection through NAT may fail.");
    }
}

/// Применяет ICE кандидат от удаленной стороны
pub async fn add_ice_candidate(candidate: IceCandidate) -> bool {
    log(&format!("add_ice_candidate called: {:?}", candidate));
    
    let pc = { PEER.lock().unwrap().as_ref().cloned() };
    
    if let Some(pc) = pc {
        // Если remote description уже установлен, применяем кандидат сразу
        if pc.remote_description().await.is_some() {
            let ice_candidate = RTCIceCandidateInit {
                candidate: candidate.candidate,
                sdp_mid: candidate.sdp_mid,
                sdp_mline_index: candidate.sdp_mline_index,
                username_fragment: None,
            };
            
            match pc.add_ice_candidate(ice_candidate).await {
                Ok(_) => {
                    log("Successfully added ICE candidate");
                    true
                }
                Err(e) => {
                    log(&format!("Failed to add ICE candidate: {:?}", e));
                    false
                }
            }
        } else {
            // Если remote description еще не установлен, сохраняем кандидат
            log("Remote description not set yet, queuing candidate");
            PENDING_REMOTE_CANDIDATES.lock().unwrap().push(candidate);
            true
        }
    } else {
        log("No peer connection available, queuing candidate");
        PENDING_REMOTE_CANDIDATES.lock().unwrap().push(candidate);
        false
    }
}

/// Применяет все отложенные кандидаты после установки remote description
async fn apply_pending_candidates(pc: &RTCPeerConnection) {
    let candidates = {
        let mut pending = PENDING_REMOTE_CANDIDATES.lock().unwrap();
        pending.drain(..).collect::<Vec<_>>()
    };
    
    for candidate in candidates {
        log(&format!("Applying pending candidate: {:?}", candidate));
        let ice_candidate = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: None,
        };
        
        if let Err(e) = pc.add_ice_candidate(ice_candidate).await {
            log(&format!("Failed to apply pending candidate: {:?}", e));
        }
    }
}

/// создаём Peer; если `initiator`, то сами делаем data-channel
async fn new_peer(initiator: bool, connection_id: String) -> Arc<RTCPeerConnection> {
    let api = APIBuilder::new().build();
    
    // Получаем пользовательские серверы если они установлены
    let custom_servers = USER_ICE_SERVERS.lock().unwrap().clone();
    let config = rtc_config(custom_servers);
    
    let pc = Arc::new(api.new_peer_connection(config).await.unwrap());

    // Начинаем сбор кандидатов
    *COLLECTING_CANDIDATES.lock().unwrap() = true;
    LOCAL_CANDIDATES.lock().unwrap().clear();

    // Обработчик для сбора локальных кандидатов
    pc.on_ice_candidate(Box::new(move |cand: Option<RTCIceCandidate>| {
        let conn_id = connection_id.clone();
        if let Some(c) = cand {
            tauri::async_runtime::spawn({
                let c = c.clone();
                async move { 
                    dump_candidate("LOCAL", &c).await;
                    
                    // Сохраняем кандидат локально
                    if let Ok(init) = c.to_json() {
                        let ice_candidate = IceCandidate {
                            candidate: init.candidate,
                            sdp_mid: init.sdp_mid,
                            sdp_mline_index: init.sdp_mline_index,
                            connection_id: conn_id,
                        };
                        
                        // Всегда сохраняем кандидат, независимо от флага collecting
                        LOCAL_CANDIDATES.lock().unwrap().push(ice_candidate);
                        log(&format!("Added ICE candidate, total count: {}", LOCAL_CANDIDATES.lock().unwrap().len()));
                    }
                }
            });
        } else {
            // cand == None означает конец сбора
            log("ICE candidate gathering completed (null candidate received)");
            *COLLECTING_CANDIDATES.lock().unwrap() = false;
        }
        Box::pin(async {})
    }));

    // Добавляем обработчик ICE gathering state для отладки
    pc.on_ice_gathering_state_change(Box::new(move |state| {
        log(&format!("ICE gathering state changed to: {:?}", state));
        Box::pin(async {})
    }));

    // делаем копию для обработчика состояний
    let pc_state = pc.clone();

    pc.on_peer_connection_state_change(Box::new(move |st: RTCPeerConnectionState| {
        log(&format!("Peer connection state changed to: {:?}", st));

        match st {
            RTCPeerConnectionState::Connected => {
                log("Peer connection connected - canceling any pending disconnect task");
                // отменяем отложенный disconnect, если он был
                if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
                    log("Aborting pending disconnect task");
                    handle.abort();
                }

                // повторно дёргаем UI, если контекст уже готов
                let crypto_exists = CRYPTO.lock().unwrap().is_some();
                if crypto_exists {
                    log("Crypto context exists - re-emitting connected event");
                    emit_connection_recovered();
                    emit_connected();
                } else {
                    log("Peer connection connected - waiting for crypto context");
                }
            }

            RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed => {
                log(&format!("Peer connection {:?} - starting grace period", st));

                // уже ожидаем? – ничего не делаем
                if DISCONNECT_TASK.lock().unwrap().is_some() {
                    log("Disconnect task already pending, ignoring");
                    return Box::pin(async {});
                }
                
                let pc_stats = pc_state.clone();
                tauri::async_runtime::spawn(async move {
                    dump_selected_pair(&pc_stats, "BEFORE-FAIL").await;
                });

                // Уведомляем о проблемах с подключением
                emit_connection_problem();

                // ставим отложенную проверку
                let handle = tauri::async_runtime::spawn({
                    let pc = pc_state.clone(); // используем копию, а не исходный pc
                    async move {
                        log(&format!(
                            "Grace period started, waiting {} s",
                            GRACE_PERIOD.as_secs()
                        ));
                        emit_connection_recovering();
                        sleep(GRACE_PERIOD).await;

                        let state_now = pc.connection_state();
                        let crypto_exists = CRYPTO.lock().unwrap().is_some();
                        log(&format!(
                            "Grace over ➜ state={:?}, crypto_exists={}",
                            state_now, crypto_exists
                        ));

                        // если соединение так и не восстановилось — отправляем событие неудачного восстановления
                        if state_now != RTCPeerConnectionState::Connected {
                            emit_connection_failed();
                        } else {
                            log("Connection recovered during grace period");
                        }
                    }
                });
                *DISCONNECT_TASK.lock().unwrap() = Some(handle);
            }

            RTCPeerConnectionState::Closed => {
                log("Peer connection closed - emitting disconnected immediately");
                // отменяем отложенный disconnect, если он был
                if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
                    handle.abort();
                }
                emit_disconnected();
            }

            _ => {
                log(&format!("Peer connection state: {:?} - ignoring", st));
            }
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

/// ========== LEGACY API ==========

/// A-сторона: создаём OFFER → base64 (устаревший API)
pub async fn generate_offer() -> String {
    log("generate_offer called - creating new peer connection");
    let connection_id = random_id();
    let pc = new_peer(true, connection_id.clone()).await;
    {
        *PEER.lock().unwrap() = Some(pc.clone());
    }

    log("Creating offer...");
    let offer = pc.create_offer(None).await.unwrap();
    log("Setting local description (offer)...");
    pc.set_local_description(offer).await.unwrap();
    
    // НЕ ждем ICE gathering - отправляем offer сразу
    log("Returning offer immediately (trickle ICE)");
    
    enc(&SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id: connection_id,
        ts: chrono::Utc::now().timestamp(),
    })
}

/// B-сторона: получает OFFER, делает ANSWER → base64
pub async fn accept_offer_and_create_answer(encoded: String) -> String {
    log("accept_offer_and_create_answer called - starting offer processing");
    let offer: SdpPayload = dec(&encoded);
    let pc = new_peer(false, offer.id.clone()).await;
    {
        *PEER.lock().unwrap() = Some(pc.clone());
    }

    log("Setting remote description (offer)...");
    pc.set_remote_description(offer.sdp).await.unwrap();
    
    // Применяем отложенные кандидаты
    apply_pending_candidates(&pc).await;
    
    log("Creating answer...");
    let answer = pc.create_answer(None).await.unwrap();
    log("Setting local description (answer)...");
    pc.set_local_description(answer).await.unwrap();
    
    // НЕ ждем ICE gathering
    log("Returning answer immediately (trickle ICE)");

    enc(&SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id: offer.id,
        ts: chrono::Utc::now().timestamp(),
    })
}

/// A-сторона: получает ANSWER и завершает handshake
pub async fn set_answer(encoded: String) -> bool {
    log("set_answer called - starting handshake completion");
    let answer: SdpPayload = dec(&encoded);
    let pc = { PEER.lock().unwrap().as_ref().cloned() };
    if let Some(pc) = pc {
        log("Setting remote description...");
        let result = pc.set_remote_description(answer.sdp).await;
        match result {
            Ok(_) => {
                log("Remote description set successfully");
                // Применяем отложенные кандидаты
                apply_pending_candidates(&pc).await;
                true
            }
            Err(e) => {
                log(&format!("Failed to set remote description: {:?}", e));
                false
            }
        }
    } else {
        log("No peer connection available for set_answer");
        false
    }
}

/// проверка готовности соединения
pub fn is_connected() -> bool {
    CRYPTO.lock().unwrap().is_some()
}

/// текст по каналу
pub async fn send_text(text: String) -> bool {
    log(&format!("send_text called with: {}", text));
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
                        log(&format!(
                            "Encrypted message with seq {}, length: {}",
                            seq_num,
                            ciphertext.len()
                        ));
                        Some(ciphertext)
                    }
                    Err(_) => {
                        log("Encryption failed");
                        None
                    }
                }
            } else {
                log("No crypto context available for sending");
                None
            }
        }; // мьютекс освобождается здесь

        if let Some(ciphertext) = result {
            let send_result = dc.send(&Bytes::from(ciphertext)).await.is_ok();
            log(&format!("Send result: {}", send_result));
            return send_result;
        }
    }
    log("No data channel available for sending");
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

    // отменяем отложенный disconnect, если он был
    if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
        log("Aborting pending disconnect task in manual disconnect");
        handle.abort();
    }

    // очищаем криптографический контекст и ключи
    log("Clearing CRYPTO context in disconnect");
    *CRYPTO.lock().unwrap() = None;
    *MY_PRIV.lock().unwrap() = None;
    *MY_PUB.lock().unwrap() = None;
    *WAS_CONNECTED.lock().unwrap() = false;

    // очищаем отложенные кандидаты
    PENDING_REMOTE_CANDIDATES.lock().unwrap().clear();
    
    // очищаем локальные кандидаты
    LOCAL_CANDIDATES.lock().unwrap().clear();
    *COLLECTING_CANDIDATES.lock().unwrap() = false;

    // отправляем событие отключения
    emit_disconnected();
}

/// ========== NEW API WITH CANDIDATES ==========

/// Генерация offer с полным набором ICE кандидатов
pub async fn generate_offer_with_candidates() -> String {
    log("generate_offer_with_candidates called");
    
    // Очищаем старые кандидаты
    LOCAL_CANDIDATES.lock().unwrap().clear();
    *COLLECTING_CANDIDATES.lock().unwrap() = true;
    
    let connection_id = random_id();
    let pc = new_peer(true, connection_id.clone()).await;
    {
        *PEER.lock().unwrap() = Some(pc.clone());
    }

    log("Creating offer...");
    let offer = pc.create_offer(None).await.unwrap();
    pc.set_local_description(offer).await.unwrap();
    
    // Ждем сбора кандидатов с таймаутом
    log("Waiting for ICE candidates...");
    let candidates = wait_for_candidates(10).await; // 10 секунд максимум
    
    log(&format!("Collected {} ICE candidates", candidates.len()));
    analyze_candidates(&candidates);
    
    let bundle = ConnectionBundle {
        sdp_payload: SdpPayload {
            sdp: pc.local_description().await.unwrap(),
            id: connection_id,
            ts: chrono::Utc::now().timestamp(),
        },
        ice_candidates: candidates,
    };
    
    // Кодируем всё вместе
    let json = serde_json::to_vec(&bundle).unwrap();
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&json).unwrap();
    let compressed = gz.finish().unwrap();
    general_purpose::STANDARD.encode(compressed)
}

/// Принятие offer с полным набором ICE кандидатов
pub async fn accept_offer_with_candidates(encoded: String) -> String {
    log("accept_offer_with_candidates called");
    
    // Декодируем bundle
    let bundle = dec_bundle(&encoded);
    
    // Очищаем старые кандидаты
    LOCAL_CANDIDATES.lock().unwrap().clear();
    *COLLECTING_CANDIDATES.lock().unwrap() = true;
    
    let pc = new_peer(false, bundle.sdp_payload.id.clone()).await;
    {
        *PEER.lock().unwrap() = Some(pc.clone());
    }

    // Устанавливаем remote description
    pc.set_remote_description(bundle.sdp_payload.sdp).await.unwrap();
    
    // Применяем все кандидаты из offer
    for candidate in bundle.ice_candidates {
        log(&format!("Applying remote candidate: {:?}", candidate));
        let ice_candidate = RTCIceCandidateInit {
            candidate: candidate.candidate,
            sdp_mid: candidate.sdp_mid,
            sdp_mline_index: candidate.sdp_mline_index,
            username_fragment: None,
        };
        
        if let Err(e) = pc.add_ice_candidate(ice_candidate).await {
            log(&format!("Failed to add candidate: {:?}", e));
        }
    }
    
    // Создаем answer
    let answer = pc.create_answer(None).await.unwrap();
    pc.set_local_description(answer).await.unwrap();
    
    // Ждем сбора кандидатов
    log("Waiting for ICE candidates...");
    let candidates = wait_for_candidates(10).await;
    
    log(&format!("Collected {} ICE candidates for answer", candidates.len()));
    analyze_candidates(&candidates);
    
    let bundle = ConnectionBundle {
        sdp_payload: SdpPayload {
            sdp: pc.local_description().await.unwrap(),
            id: bundle.sdp_payload.id,
            ts: chrono::Utc::now().timestamp(),
        },
        ice_candidates: candidates,
    };
    
    // Кодируем всё вместе
    let json = serde_json::to_vec(&bundle).unwrap();
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&json).unwrap();
    let compressed = gz.finish().unwrap();
    general_purpose::STANDARD.encode(compressed)
}

/// Установка answer с полным набором ICE кандидатов
pub async fn set_answer_with_candidates(encoded: String) -> bool {
    log("set_answer_with_candidates called");
    
    // Декодируем bundle
    let bundle = dec_bundle(&encoded);
    
    let pc = { PEER.lock().unwrap().as_ref().cloned() };
    if let Some(pc) = pc {
        // Устанавливаем remote description
        match pc.set_remote_description(bundle.sdp_payload.sdp).await {
            Ok(_) => {
                log("Remote description set successfully");
                
                // Применяем все кандидаты из answer
                for candidate in bundle.ice_candidates {
                    log(&format!("Applying remote candidate: {:?}", candidate));
                    let ice_candidate = RTCIceCandidateInit {
                        candidate: candidate.candidate,
                        sdp_mid: candidate.sdp_mid,
                        sdp_mline_index: candidate.sdp_mline_index,
                        username_fragment: None,
                    };
                    
                    if let Err(e) = pc.add_ice_candidate(ice_candidate).await {
                        log(&format!("Failed to add candidate: {:?}", e));
                    }
                }
                
                true
            }
            Err(e) => {
                log(&format!("Failed to set remote description: {:?}", e));
                false
            }
        }
    } else {
        log("No peer connection available");
        false
    }
}
