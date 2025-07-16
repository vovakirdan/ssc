use crate::logger::log;
use crate::peer::state::{
    APP, COLLECTING_CANDIDATES, LOCAL_CANDIDATES, PEER, PENDING_REMOTE_CANDIDATES,
};
use crate::peer::types::{IceCandidate, ServerConfig};
use crate::utils::add_ice_url_scheme;
use std::sync::Arc;
use std::time::Duration;
use tauri::command;
use tauri::AppHandle;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio::time::timeout;
use webrtc::ice_transport::ice_candidate::RTCIceCandidateInit;
use webrtc::{
    api::APIBuilder,
    ice_transport::{ice_gatherer_state::RTCIceGathererState, ice_server::RTCIceServer},
    peer_connection::{configuration::RTCConfiguration, RTCPeerConnection},
};

/// Применяет ICE кандидат от удаленной стороны
#[command]
pub async fn add_ice_candidate(app: AppHandle, candidate: IceCandidate) -> bool {
    *APP.lock().unwrap() = Some(app);
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
pub async fn apply_pending_candidates(pc: &RTCPeerConnection) {
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

#[command]
pub async fn check_ice_server_availability(config: ServerConfig) -> bool {
    log(&format!(
        "check_ice_server_availability called with config: {:?}",
        config
    ));

    let url = add_ice_url_scheme(&config);

    log(&format!("Processed URL: '{}' -> '{}'", config.url, url));

    // Создаем ICE сервер из конфигурации
    let ice_server = RTCIceServer {
        urls: vec![url],
        username: config.username.clone().unwrap_or_default(),
        credential: config.credential.clone().unwrap_or_default(),
    };

    log(&format!(
        "Created ICE server: urls={:?}, username='{}', credential='{}'",
        ice_server.urls, ice_server.username, ice_server.credential
    ));

    // Создаем конфигурацию для peer connection
    let rtc_config = RTCConfiguration {
        ice_servers: vec![ice_server],
        ..Default::default()
    };

    log(&format!(
        "Created RTC config with {} ICE servers",
        rtc_config.ice_servers.len()
    ));

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
    log(&format!(
        "check_via_ice_gathering called with server_type: {}",
        server_type
    ));

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
                let candidate_type = c
                    .to_json()
                    .map(|json| {
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
                            log(&format!(
                                "Candidate type mismatch: expected {} but got candidate: {}",
                                server_type, json.candidate
                            ));
                            false
                        }
                    })
                    .unwrap_or_else(|e| {
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
        }
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
    log(&format!(
        "Starting timeout wait of {} seconds",
        check_timeout.as_secs()
    ));

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

// Добавляем новую функцию для ожидания кандидатов с таймаутом
pub async fn wait_for_candidates(timeout_secs: u64) -> Vec<IceCandidate> {
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

        log(&format!(
            "Candidate collection status: collecting={}, count={}, elapsed={}s",
            collecting,
            candidates_count,
            start.elapsed().as_secs()
        ));

        // Если сбор закончен ИЛИ есть хотя бы relay кандидаты - возвращаем
        if !collecting || candidates_count > 0 {
            break;
        }

        // Проверяем таймаут
        if start.elapsed().as_secs() >= timeout_secs {
            log(&format!(
                "Candidate collection timeout after {} seconds",
                timeout_secs
            ));
            break;
        }

        sleep(Duration::from_millis(100)).await;
    }

    LOCAL_CANDIDATES.lock().unwrap().clone()
}

pub fn analyze_candidates(candidates: &[IceCandidate]) {
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
