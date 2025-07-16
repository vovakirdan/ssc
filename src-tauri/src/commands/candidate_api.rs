use crate::logger::log;
use crate::peer::connection::new_peer;
use crate::peer::crypto::dec_bundle;
use crate::peer::ice::{analyze_candidates, wait_for_candidates};
use crate::peer::state::{APP, COLLECTING_CANDIDATES, LOCAL_CANDIDATES, PEER};
use crate::peer::types::{ConnectionBundle, SdpPayload};
use crate::utils::random_id;
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::aead::KeyInit;
use flate2::{write::GzEncoder, Compression};
use sha2::Digest;
use std::io::Write;
use tauri::command;
use tauri::AppHandle;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;

/// Генерация offer с полным набором ICE кандидатов
#[command]
pub async fn generate_offer_with_candidates(app: AppHandle) -> String {
    *APP.lock().unwrap() = Some(app);
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
#[command]
pub async fn accept_offer_with_candidates(app: AppHandle, encoded: String) -> String {
    *APP.lock().unwrap() = Some(app);
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
    pc.set_remote_description(bundle.sdp_payload.sdp)
        .await
        .unwrap();

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

    log(&format!(
        "Collected {} ICE candidates for answer",
        candidates.len()
    ));
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
#[command]
pub async fn set_answer_with_candidates(app: AppHandle, encoded: String) -> bool {
    *APP.lock().unwrap() = Some(app);
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
