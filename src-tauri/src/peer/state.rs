use crate::peer::crypto::CryptoCtx;
use crate::peer::types::{IceCandidate, ServerConfig};
use once_cell::sync::Lazy;
use ring::agreement;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use webrtc::{data_channel::RTCDataChannel, peer_connection::RTCPeerConnection};

/// ========== GLOBAL STATE ==========

/// WebRTC Peer Connection
pub static PEER: Lazy<Mutex<Option<Arc<RTCPeerConnection>>>> = Lazy::new(|| Mutex::new(None));

/// Data Channel для обмена сообщениями
pub static DATA_CH: Lazy<Mutex<Option<Arc<RTCDataChannel>>>> = Lazy::new(|| Mutex::new(None));

/// Криптографический контекст для шифрования
pub static CRYPTO: Lazy<Mutex<Option<CryptoCtx>>> = Lazy::new(|| Mutex::new(None));

/// Приватный ключ для обмена
pub static MY_PRIV: Lazy<Mutex<Option<agreement::EphemeralPrivateKey>>> =
    Lazy::new(|| Mutex::new(None));

/// Публичный ключ для обмена
pub static MY_PUB: Lazy<Mutex<Option<[u8; 32]>>> = Lazy::new(|| Mutex::new(None));

/// Handle для отправки событий в Tauri
pub static APP: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

/// Флаг установленного соединения
pub static WAS_CONNECTED: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Отложенная задача для graceful disconnect
pub static DISCONNECT_TASK: Lazy<Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> =
    Lazy::new(|| Mutex::new(None));

/// Кандидаты, полученные до установки remote description
pub static PENDING_REMOTE_CANDIDATES: Lazy<Mutex<Vec<IceCandidate>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

/// Локальные кандидаты для текущего соединения
pub static LOCAL_CANDIDATES: Lazy<Mutex<Vec<IceCandidate>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Флаг активного сбора кандидатов
pub static COLLECTING_CANDIDATES: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));

/// Глобальное хранилище для пользовательских ICE серверов
pub static USER_ICE_SERVERS: Lazy<Mutex<Option<Vec<ServerConfig>>>> =
    Lazy::new(|| Mutex::new(None));

/// ========== CONSTANTS ==========

/// Длина тега аутентификации для ChaCha20-Poly1305
pub const TAG_LEN: usize = 16;

/// Период ожидания перед принудительным отключением
pub const GRACE_PERIOD: Duration = Duration::from_secs(10);
