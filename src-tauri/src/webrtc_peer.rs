use base64::{engine::general_purpose, Engine as _};
use once_cell::sync::Lazy;
use rand::Rng;
use serde::{Deserialize, Serialize};
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
pub static APP: Lazy<Mutex<Option<AppHandle>>> = Lazy::new(|| Mutex::new(None));

#[derive(Serialize, Deserialize)]
pub struct SdpPayload {
    pub sdp: RTCSessionDescription,
    pub id: String,
    pub ts: i64,
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
fn enc(p: &SdpPayload) -> String {
    general_purpose::STANDARD.encode(serde_json::to_vec(p).unwrap())
}
fn dec(s: &str) -> SdpPayload {
    serde_json::from_slice(&general_purpose::STANDARD.decode(s).unwrap()).unwrap()
}

fn emit_connected() {
    if let Some(app) = APP.lock().unwrap().clone() {
        let _ = app.emit("ssc-connected", ());
    }
}

fn emit_disconnected() {
    if let Some(app) = APP.lock().unwrap().clone() {
        let _ = app.emit("ssc-disconnected", ());
    }
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
        if st == RTCPeerConnectionState::Connected {
            emit_connected(); // call helper
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
    {
        let mut lock = DATA_CH.lock().unwrap();
        *lock = Some(dc.clone());
    }
    dc.on_message(Box::new(|msg| {
        let txt = String::from_utf8_lossy(&msg.data).to_string();
        emit_message(&txt);
        println!("[DATA] Received: {}", txt);
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

/// текст по каналу
pub async fn send_text(text: String) -> bool {
    let dc = { DATA_CH.lock().unwrap().as_ref().cloned() };
    if let Some(dc) = dc {
        dc.send_text(text).await.is_ok()
    } else {
        false
    }
}
