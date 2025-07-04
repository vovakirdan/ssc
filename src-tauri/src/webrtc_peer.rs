use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use webrtc::{
    api::APIBuilder,
    data_channel::{data_channel_init::RTCDataChannelInit, RTCDataChannel},
    ice_transport::ice_server::RTCIceServer,
    peer_connection::{
        configuration::RTCConfiguration,
        sdp::session_description::RTCSessionDescription,
        RTCPeerConnection,
    },
};
use base64::{engine::general_purpose, Engine as _};
use rand::Rng;
use serde::{Deserialize, Serialize};

/// ========  GLOBALS  =========
pub static PEER: Lazy<Mutex<Option<Arc<RTCPeerConnection>>>> =
    Lazy::new(|| Mutex::new(None));
static DATA_CH: Lazy<Mutex<Option<Arc<RTCDataChannel>>>> =
    Lazy::new(|| Mutex::new(None));

#[derive(Serialize, Deserialize)]
pub struct SdpPayload {
    pub sdp: RTCSessionDescription,
    pub id: String,
    pub ts: i64,
}

/// ========  HELPERS  =========
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
    let mut rng = rand::rng();
    hex::encode(rng.random::<[u8; 8]>())
}

fn encode_payload(p: &SdpPayload) -> String {
    general_purpose::STANDARD.encode(serde_json::to_string(p).unwrap())
}

fn decode_payload(s: &str) -> SdpPayload {
    serde_json::from_slice(&general_purpose::STANDARD.decode(s).unwrap()).unwrap()
}

/// ========  PUBLIC API =========

/// Шаг 1 (A): генерируем OFFER, кодируем base64 → QR
pub async fn generate_offer() -> String {
    let api = APIBuilder::new().build();
    let pc = Arc::new(api.new_peer_connection(rtc_config()).await.unwrap());

    // Создаем data channel
    let dc = pc
        .create_data_channel("ssc-data", Some(RTCDataChannelInit::default()))
        .await
        .unwrap();

    {
        let mut dlock = DATA_CH.lock().unwrap();
        *dlock = Some(dc.clone());
    }

    {
        let mut lock = PEER.lock().unwrap();
        *lock = Some(pc.clone());
    }
    let offer = pc.create_offer(None).await.unwrap();
    pc.set_local_description(offer).await.unwrap();
    // НЕ ждем gathering complete для QR - это сделает его слишком большим

    let payload = SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id: random_id(),
        ts: chrono::Utc::now().timestamp(),
    };
    let result = encode_payload(&payload);
    println!("[RUST] Generated offer, encoded length: {}", result.len());
    result
}

/// Шаг 2 (B): приняли OFFER (encoded), создаём ANSWER
pub async fn accept_offer_and_create_answer(encoded: String) -> String {
    println!("[RUST] accept_offer_and_create_answer called, encoded length: {}", encoded.len());
    let offer: SdpPayload = decode_payload(&encoded);
    println!("[RUST] Decoded offer, session id: {}", offer.id);
    
    let api = APIBuilder::new().build();
    let pc = Arc::new(api.new_peer_connection(rtc_config()).await.unwrap());

    // Создаем data channel
    let dc = pc
        .create_data_channel("ssc-data", Some(RTCDataChannelInit::default()))
        .await
        .unwrap();

    {
        let mut dlock = DATA_CH.lock().unwrap();
        *dlock = Some(dc.clone());
    }

    {
        let mut lock = PEER.lock().unwrap();
        *lock = Some(pc.clone());
    }
    pc.set_remote_description(offer.sdp).await.unwrap();
    let answer = pc.create_answer(None).await.unwrap();
    pc.set_local_description(answer).await.unwrap();
    // НЕ ждем gathering complete - это может вызвать проблемы

    let payload = SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id: offer.id,
        ts: chrono::Utc::now().timestamp(),
    };
    let result = encode_payload(&payload);
    println!("[RUST] Created answer, encoded length: {}", result.len());
    result
}

/// Шаг 3 (A): получаем ANSWER, завершаем соединение
pub async fn set_answer(encoded: String) -> bool {
    println!("[RUST] set_answer called, encoded length: {}", encoded.len());
    let answer: SdpPayload = decode_payload(&encoded);
    println!("[RUST] Decoded answer, session id: {}", answer.id);
    
    let pc = { PEER.lock().unwrap().as_ref().cloned() };
    if let Some(pc) = pc {
        let result = pc.set_remote_description(answer.sdp).await.is_ok();
        println!("[RUST] set_remote_description result: {}", result);
        result
    } else {
        println!("[RUST] No peer connection found");
        false
    }
}

/// ========  TEXT SEND  =========
pub async fn send_text(text: String) -> bool {
    let ch = { DATA_CH.lock().unwrap().as_ref().cloned() };
    if let Some(dc) = ch {
        dc.send_text(text).await.is_ok()
    } else {
        false
    }
}
