use crate::peer::state::{
    APP, COLLECTING_CANDIDATES, CRYPTO, LOCAL_CANDIDATES, MY_PRIV, MY_PUB,
    PENDING_REMOTE_CANDIDATES, WAS_CONNECTED,
};
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

/// Логирование с временными метками
pub fn log(msg: &str) {
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
pub async fn dump_candidate(label: &str, cand: &RTCIceCandidate) {
    if let Ok(init) = cand.to_json() {
        log(&format!(
            "Trickle {label}: candidate={} sdp_mid={:?} sdp_mline_index={:?} username_fragment={:?}",
            init.candidate, init.sdp_mid, init.sdp_mline_index, init.username_fragment
        ));
    }
}

/// Быстрый снимок getStats → выбранная пара
pub async fn dump_selected_pair(pc: &RTCPeerConnection, moment: &str) {
    let stats = pc.get_stats().await;
    for (_, v) in stats.reports {
        match v {
            webrtc::stats::StatsReportType::CandidatePair(pair) => {
                if pair.nominated {
                    log(&format!(
                        "STATS {moment}: {}:{}  type: {:?}  bytes={}/{} state={:?}",
                        pair.local_candidate_id,
                        pair.remote_candidate_id,
                        pair.stats_type,
                        pair.bytes_sent,
                        pair.bytes_received,
                        pair.state
                    ));
                }
            }
            _ => {}
        }
    }
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

pub fn emit_connected() {
    log("emit_connected called - setting WAS_CONNECTED flag");
    *WAS_CONNECTED.lock().unwrap() = true;
    emit_state("ssc-connected");
}

pub fn emit_disconnected() {
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

pub fn emit_message(msg: &str) {
    if let Some(app) = APP.lock().unwrap().clone() {
        let _ = app.emit("ssc-message", msg);
    }
}

pub fn emit_connection_problem() {
    log("emit_connection_problem called - connection issues detected");
    emit_state("ssc-connection-problem");
}

pub fn emit_connection_recovering() {
    log("emit_connection_recovering called - connection is recovering");
    emit_state("ssc-connection-recovering");
}

pub fn emit_connection_recovered() {
    log("emit_connection_recovered called - connection recovered");
    emit_state("ssc-connection-recovered");
}

pub fn emit_connection_failed() {
    log("emit_connection_failed called - connection recovery failed");
    emit_state("ssc-connection-failed");
}
