use serde::{Deserialize, Serialize};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

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

// ===== Компактные представления для уменьшения размера оффера/ансвера =====

/// Компактная SDP (только переменные значения)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactSdp {
    /// Тип описания: 0 = offer, 1 = answer
    pub t: u8,
    /// ICE ufrag
    pub uf: String,
    /// ICE pwd
    pub up: String,
    /// DTLS fingerprint без двоеточий (hex в uppercase)
    pub fp: String,
    /// DTLS setup role: 0 = actpass, 1 = active, 2 = passive
    pub sr: u8,
    /// MID для m=application
    pub mi: String,
    /// SCTP порт (обычно 5000)
    pub sp: u16,
}

/// Компактная обёртка SDP с метаданными (ID/TS)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactSdpPayload {
    pub s: CompactSdp,
    pub id: String,
    pub ts: i64,
}

/// Компактный ICE кандидат с короткими ключами
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactIceCandidate {
    /// Полная строка candidate, но ключ короче
    pub c: String,
    /// sdp_mid
    pub md: Option<String>,
    /// sdp_mline_index
    pub ml: Option<u16>,
    /// ID соединения
    pub i: String,
}

/// Компактный пакет соединения (SDP + кандидаты)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactConnectionBundle {
    pub s: CompactSdpPayload,
    pub cs: Vec<CompactIceCandidate>,
}
