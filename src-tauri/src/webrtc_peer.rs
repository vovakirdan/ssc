use once_cell::sync::Lazy;
use std::sync::{Arc, Mutex};
use webrtc::{
    api::APIBuilder,
    data_channel::{data_channel_init::RTCDataChannelInit},
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

/// Глобальный singleton — текущий peer
pub static PEER: Lazy<Mutex<Option<Arc<RTCPeerConnection>>>> =
    Lazy::new(|| Mutex::new(None));

/// Обёртка для (OFFER|ANSWER) в QR
#[derive(Serialize, Deserialize)]
pub struct SdpPayload {
    pub sdp: RTCSessionDescription,
    pub id: String,          // случайный ID сессии
    pub ts:  i64,            // unix-time
}

//—─────────────────────────────────────────────────────────────

fn rtc_config() -> RTCConfiguration {
    RTCConfiguration {
        ice_servers: vec![
            // публичные STUN (работают без логина)
            RTCIceServer {
                urls: vec![
                    "stun:stun.l.google.com:19302".into(),
                    "stun:stun1.l.google.com:19302".into(),
                ],
                ..Default::default()
            },
            // пример TURN
            // RTCIceServer {
            //     urls: vec!["turn:relay.example.com:3478".into()],
            //     username: "user".into(),
            //     credential: "pass".into(),
            //     ..Default::default()
            // },
        ],
        ..Default::default()
    }
}

async fn new_peer() -> Arc<RTCPeerConnection> {
    let api = APIBuilder::new().build();
    let pc = Arc::new(api.new_peer_connection(rtc_config()).await.unwrap());

    // Data-channel; сообщения пока просто выводим
    let dc = pc
        .create_data_channel("ssc-data", Some(RTCDataChannelInit::default()))
        .await
        .unwrap();

    dc.on_message(Box::new(move |msg| {
        println!("[DATA] {:?}", String::from_utf8_lossy(&msg.data));
        Box::pin(async {})
    }));

    pc.on_peer_connection_state_change(Box::new(|state| {
        println!("★ Peer state: {state:?}");
        Box::pin(async {})
    }));

    pc
}

//—─────────────────────────────────────────────────────────────
// helper: ждём окончания ICE-gathering, затем берём полное SDP
async fn wait_gathering_complete(pc: &RTCPeerConnection) {
    let mut gather_complete = pc.gathering_complete_promise().await;
    gather_complete.recv().await;
}

//—─────────────────────────────────────────────────────────────
// PUBLIC API (вызываем через Tauri invoke)
//—─────────────────────────────────────────────────────────────

/// Шаг 1 (A): генерируем OFFER, кодируем base64 → QR
pub async fn generate_offer() -> String {
    let pc = new_peer().await;
    {
        let mut lock = PEER.lock().unwrap();
        *lock = Some(pc.clone());
    }

    let offer = pc.create_offer(None).await.unwrap();
    pc.set_local_description(offer.clone()).await.unwrap();
    wait_gathering_complete(&pc).await;

    let payload = SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id:  random_id(),
        ts:  chrono::Utc::now().timestamp(),
    };
    encode_payload(&payload)
}

/// Шаг 2 (B): приняли OFFER (encoded), создаём ANSWER
pub async fn accept_offer_and_create_answer(encoded_offer: String) -> String {
    let offer: SdpPayload = decode_payload(&encoded_offer);
    let pc = new_peer().await;
    {
        let mut lock = PEER.lock().unwrap();
        *lock = Some(pc.clone());
    }

    pc.set_remote_description(offer.sdp).await.unwrap();
    let answer = pc.create_answer(None).await.unwrap();
    pc.set_local_description(answer.clone()).await.unwrap();
    wait_gathering_complete(&pc).await;

    let payload = SdpPayload {
        sdp: pc.local_description().await.unwrap(),
        id:  offer.id,                        // тот же id сессии
        ts:  chrono::Utc::now().timestamp(),
    };
    encode_payload(&payload)
}

/// Шаг 3 (A): получаем ANSWER, завершаем соединение
pub async fn set_answer(encoded_answer: String) -> bool {
    let answer: SdpPayload = decode_payload(&encoded_answer);
    let pc = {
        let lock = PEER.lock().unwrap();
        lock.as_ref().cloned()
    };
    
    if let Some(pc) = pc {
        pc.set_remote_description(answer.sdp).await.is_ok()
    } else {
        false
    }
}

//—──────────────── helpers ────────────────
fn random_id() -> String {
    let mut rng = rand::rng();
    let bytes: [u8; 8] = rng.random();
    hex::encode(bytes)
}
fn encode_payload(p: &SdpPayload) -> String {
    let json = serde_json::to_string(p).unwrap();
    general_purpose::STANDARD.encode(json)
}
fn decode_payload(s: &str) -> SdpPayload {
    let json = String::from_utf8(general_purpose::STANDARD.decode(s).unwrap()).unwrap();
    serde_json::from_str(&json).unwrap()
}
