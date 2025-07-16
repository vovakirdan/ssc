use crate::logger::{
    dump_candidate, dump_selected_pair, emit_connected, emit_connection_failed,
    emit_connection_problem, emit_connection_recovered, emit_connection_recovering,
    emit_disconnected, log,
};
use crate::peer::data_channel::attach_dc;
use crate::peer::state::{
    COLLECTING_CANDIDATES, CRYPTO, DISCONNECT_TASK, GRACE_PERIOD, LOCAL_CANDIDATES,
    USER_ICE_SERVERS,
};
use crate::peer::types::IceCandidate;
use crate::peer::types::ServerConfig;
use crate::utils::add_ice_url_scheme;
use std::sync::Arc;
use tauri::command;
use tokio::time::sleep;
use webrtc::ice_transport::ice_candidate::RTCIceCandidate;
use webrtc::peer_connection::policy::bundle_policy::RTCBundlePolicy;
use webrtc::peer_connection::policy::rtcp_mux_policy::RTCRtcpMuxPolicy;
use webrtc::{
    api::APIBuilder,
    data_channel::{data_channel_init::RTCDataChannelInit, RTCDataChannel},
    ice_transport::ice_server::RTCIceServer,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState, RTCPeerConnection,
    },
};

/// создаём Peer; если `initiator`, то сами делаем data-channel
pub async fn new_peer(initiator: bool, connection_id: String) -> Arc<RTCPeerConnection> {
    let api = APIBuilder::new().build();

    // Получаем пользовательские серверы если они установлены
    let custom_servers = USER_ICE_SERVERS.lock().unwrap().clone();
    let config = rtc_config(custom_servers);

    let pc = Arc::new(api.new_peer_connection(config).await.unwrap());

    // Начинаем сбор кандидатов
    *COLLECTING_CANDIDATES.lock().unwrap() = true;
    LOCAL_CANDIDATES.lock().unwrap().clear();

    // Обработчик для сбора локальных кандидатов
    pc.on_ice_candidate(Box::new(move |cand: Option<RTCIceCandidate>| {
        let conn_id = connection_id.clone();
        if let Some(c) = cand {
            tauri::async_runtime::spawn({
                let c = c.clone();
                async move {
                    dump_candidate("LOCAL", &c).await;

                    // Сохраняем кандидат локально
                    if let Ok(init) = c.to_json() {
                        let ice_candidate = IceCandidate {
                            candidate: init.candidate,
                            sdp_mid: init.sdp_mid,
                            sdp_mline_index: init.sdp_mline_index,
                            connection_id: conn_id,
                        };

                        // Всегда сохраняем кандидат, независимо от флага collecting
                        LOCAL_CANDIDATES.lock().unwrap().push(ice_candidate);
                        log(&format!(
                            "Added ICE candidate, total count: {}",
                            LOCAL_CANDIDATES.lock().unwrap().len()
                        ));
                    }
                }
            });
        } else {
            // cand == None означает конец сбора
            log("ICE candidate gathering completed (null candidate received)");
            *COLLECTING_CANDIDATES.lock().unwrap() = false;
        }
        Box::pin(async {})
    }));

    // Добавляем обработчик ICE gathering state для отладки
    pc.on_ice_gathering_state_change(Box::new(move |state| {
        log(&format!("ICE gathering state changed to: {:?}", state));
        Box::pin(async {})
    }));

    // делаем копию для обработчика состояний
    let pc_state = pc.clone();

    pc.on_peer_connection_state_change(Box::new(move |st: RTCPeerConnectionState| {
        log(&format!("Peer connection state changed to: {:?}", st));

        match st {
            RTCPeerConnectionState::Connected => {
                log("Peer connection connected - canceling any pending disconnect task");
                // отменяем отложенный disconnect, если он был
                if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
                    log("Aborting pending disconnect task");
                    handle.abort();
                }

                // повторно дёргаем UI, если контекст уже готов
                let crypto_exists = CRYPTO.lock().unwrap().is_some();
                if crypto_exists {
                    log("Crypto context exists - re-emitting connected event");
                    emit_connection_recovered();
                    emit_connected();
                } else {
                    log("Peer connection connected - waiting for crypto context");
                }
            }

            RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed => {
                log(&format!("Peer connection {:?} - starting grace period", st));

                // уже ожидаем? – ничего не делаем
                if DISCONNECT_TASK.lock().unwrap().is_some() {
                    log("Disconnect task already pending, ignoring");
                    return Box::pin(async {});
                }

                let pc_stats = pc_state.clone();
                tauri::async_runtime::spawn(async move {
                    dump_selected_pair(&pc_stats, "BEFORE-FAIL").await;
                });

                // Уведомляем о проблемах с подключением
                emit_connection_problem();

                // ставим отложенную проверку
                let handle = tauri::async_runtime::spawn({
                    let pc = pc_state.clone(); // используем копию, а не исходный pc
                    async move {
                        log(&format!(
                            "Grace period started, waiting {} s",
                            GRACE_PERIOD.as_secs()
                        ));
                        emit_connection_recovering();
                        sleep(GRACE_PERIOD).await;

                        let state_now = pc.connection_state();
                        let crypto_exists = CRYPTO.lock().unwrap().is_some();
                        log(&format!(
                            "Grace over ➜ state={:?}, crypto_exists={}",
                            state_now, crypto_exists
                        ));

                        // если соединение так и не восстановилось — отправляем событие неудачного восстановления
                        if state_now != RTCPeerConnectionState::Connected {
                            emit_connection_failed();
                        } else {
                            log("Connection recovered during grace period");
                        }
                    }
                });
                *DISCONNECT_TASK.lock().unwrap() = Some(handle);
            }

            RTCPeerConnectionState::Closed => {
                log("Peer connection closed - emitting disconnected immediately");
                // отменяем отложенный disconnect, если он был
                if let Some(handle) = DISCONNECT_TASK.lock().unwrap().take() {
                    handle.abort();
                }
                emit_disconnected();
            }

            _ => {
                log(&format!("Peer connection state: {:?} - ignoring", st));
            }
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

/// Создает конфигурацию для peer connection
fn rtc_config(custom_servers: Option<Vec<ServerConfig>>) -> RTCConfiguration {
    let ice_servers = if let Some(servers) = custom_servers {
        // Используем пользовательские серверы
        get_user_ice_servers(servers)
    } else {
        // Используем дефолтные серверы
        vec![RTCIceServer {
            urls: vec![
                "stun:stun.l.google.com:19302".into(),
                "stun:stun1.l.google.com:19302".into(),
            ],
            ..Default::default()
        }]
    };

    RTCConfiguration {
        ice_servers,
        // Добавляем более агрессивные настройки ICE
        ice_candidate_pool_size: 10,
        bundle_policy: RTCBundlePolicy::MaxBundle,
        rtcp_mux_policy: RTCRtcpMuxPolicy::Require,
        // ice_transport_policy: webrtc::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy::Relay,
        ..Default::default()
    }
}

/// Получение конфигурации серверов из фронтенда
pub fn get_user_ice_servers(servers: Vec<ServerConfig>) -> Vec<RTCIceServer> {
    servers
        .into_iter()
        .map(|config| {
            let url = add_ice_url_scheme(&config);

            RTCIceServer {
                urls: vec![url],
                username: config.username.unwrap_or_default(),
                credential: config.credential.unwrap_or_default(),
            }
        })
        .collect()
}

/// Устанавливает пользовательские ICE серверы, возвращает true при успехе, false при ошибке
#[command]
pub fn set_ice_servers(servers: Vec<ServerConfig>) -> bool {
    log(&format!("Setting {} custom ICE servers", servers.len()));

    // Валидация серверов
    for server in &servers {
        if server.url.is_empty() {
            log("Server URL cannot be empty");
            return false;
        }

        if server.r#type == "turn" && (server.username.is_none() || server.credential.is_none()) {
            log("TURN servers require username and credential");
            return false;
        }
    }

    *USER_ICE_SERVERS.lock().unwrap() = Some(servers);
    log("Custom ICE servers set successfully");
    true
}

/// Получает пользовательские ICE серверы, возвращает дефолтные серверы если не установлены
#[command]
pub fn get_ice_servers() -> Vec<ServerConfig> {
    USER_ICE_SERVERS.lock().unwrap().clone().unwrap_or_else(|| {
        // Возвращаем дефолтные серверы в формате ServerConfig
        vec![
            ServerConfig {
                id: "default-stun".into(),
                r#type: "stun".into(),
                url: "stun:stun.l.google.com:19302".into(),
                username: None,
                credential: None,
            },
            ServerConfig {
                id: "default-turn".into(),
                r#type: "turn".into(),
                url: "stun:stun1.l.google.com:19302".into(),
                username: None,
                credential: None,
            },
        ]
    })
}
