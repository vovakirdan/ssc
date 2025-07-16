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
