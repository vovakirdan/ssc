use crate::logger::log;
use crate::peer::connection::new_peer;
use crate::peer::crypto::{dec, enc};
use crate::peer::ice::apply_pending_candidates;
use crate::peer::state::PEER;
use crate::peer::types::SdpPayload;
use crate::utils::random_id;
use tauri::command;

/// A-сторона: создаём OFFER → base64 (устаревший API)
#[command]
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
#[command]
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
#[command]
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
