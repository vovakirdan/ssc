use super::{
    data_channel::attach_dc,
    handshake::Role,
    state::{lock, Session, MANAGER},
    types::{IceCandidate, ServerConfig},
};
use crate::{errors::AppError, utils::add_ice_url_scheme};
use std::{sync::Arc, time::Duration};
use webrtc::{
    api::APIBuilder,
    data_channel::{data_channel_init::RTCDataChannelInit, RTCDataChannel},
    ice_transport::ice_server::RTCIceServer,
    peer_connection::{
        configuration::RTCConfiguration, peer_connection_state::RTCPeerConnectionState,
        RTCPeerConnection,
    },
};

pub fn rtc_config(servers: Option<Vec<ServerConfig>>) -> RTCConfiguration {
    RTCConfiguration {
        ice_transport_policy: if std::env::var("SSC_FORCE_RELAY").as_deref() == Ok("1") {
            webrtc::peer_connection::policy::ice_transport_policy::RTCIceTransportPolicy::Relay
        } else {
            Default::default()
        },
        ice_servers: servers
            .unwrap_or_else(default_servers)
            .into_iter()
            .map(|s| RTCIceServer {
                urls: vec![add_ice_url_scheme(&s)],
                username: s.username.unwrap_or_default(),
                credential: s.credential.unwrap_or_default(),
            })
            .collect(),
        ..Default::default()
    }
}
// An unactivated generic install can use P2P. A known access failure must remain visible.
pub(super) fn project_or_stun(
    result: Result<Vec<ServerConfig>, AppError>,
) -> Result<Vec<ServerConfig>, AppError> {
    match result {
        Ok(servers) => Ok(servers),
        Err(error) if error.code == "access_activation_required" => Ok(default_servers()),
        Err(error) => Err(error),
    }
}
pub async fn new_peer(
    session: &Arc<Session>,
    initiator: bool,
) -> Result<Arc<RTCPeerConnection>, AppError> {
    let servers = lock(&MANAGER).servers.clone();
    let servers = match servers {
        Some(servers) => servers,
        None => project_or_stun(crate::access::managed_servers().await)?,
    };
    session.ensure_active()?;
    let config = rtc_config(Some(servers));
    let pc = Arc::new(
        APIBuilder::new()
            .build()
            .new_peer_connection(config)
            .await
            .map_err(|_| {
                AppError::new("peer_create", "ice", "Не удалось создать соединение", true)
            })?,
    );
    if let Err(error) = session.ensure_active() {
        let _ = pc.close().await;
        return Err(error);
    }
    let installed = {
        let mut d = lock(&session.data);
        if d.closed {
            false
        } else {
            d.pc = Some(pc.clone());
            d.role = Some(Role::from_initiator(initiator));
            true
        }
    };
    if !installed {
        let _ = pc.close().await;
        return Err(AppError::new(
            "session_closed",
            "session",
            "Сессия завершена",
            true,
        ));
    }
    session.publish();
    let weak = Arc::downgrade(session);
    pc.on_ice_candidate(Box::new(move |candidate| {
        let weak = weak.clone();
        Box::pin(async move {
            if let Some(s) = weak.upgrade() {
                if !s.active() {
                    return;
                }
                if let Some(candidate) = candidate {
                    if let Ok(c) = candidate.to_json() {
                        let mut d = lock(&s.data);
                        if d.closed {
                            return;
                        }
                        let connection_id = d.connection_id.clone();
                        d.candidates.push(IceCandidate {
                            candidate: c.candidate,
                            sdp_mid: c.sdp_mid,
                            sdp_mline_index: c.sdp_mline_index,
                            connection_id,
                        });
                    }
                }
            }
        })
    }));
    let weak = Arc::downgrade(session);
    pc.on_peer_connection_state_change(Box::new(move |state| {
        let weak = weak.clone();
        Box::pin(async move {
            if let Some(s) = weak.upgrade() {
                if !s.active() {
                    return;
                }
                match state {
                    RTCPeerConnectionState::Connected => {
                        if !s.recovered() {
                            return;
                        }
                        s.start_handshake_timeout();
                        s.publish();
                        update_route(&s).await;
                    }
                    RTCPeerConnectionState::Disconnected | RTCPeerConnectionState::Failed => {
                        let Some(epoch) = s.start_recovery() else {
                            return;
                        };
                        s.publish();
                        let weak = Arc::downgrade(&s);
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            if let Some(s) = weak.upgrade() {
                                s.expire_recovery(epoch).await;
                            }
                        });
                    }
                    RTCPeerConnectionState::Closed => {
                        s.close_from_callback();
                    }
                    _ => {}
                }
            }
        })
    }));
    if initiator {
        let dc = pc
            .create_data_channel("ssc-v3", Some(RTCDataChannelInit::default()))
            .await
            .map_err(|_| {
                AppError::new(
                    "channel_create",
                    "data_channel",
                    "Не удалось открыть канал",
                    true,
                )
            })?;
        attach_dc(session, &dc)?;
    } else {
        let weak = Arc::downgrade(session);
        pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
            let weak = weak.clone();
            Box::pin(async move {
                if let Some(s) = weak.upgrade() {
                    if s.active() {
                        if let Err(error) = attach_dc(&s, &dc) {
                            s.error(&error);
                            s.close_from_callback();
                        }
                    } else {
                        let _ = dc.close().await;
                    }
                }
            })
        }));
    }
    Ok(pc)
}
async fn update_route(s: &Arc<Session>) {
    let pc = lock(&s.data).pc.clone();
    if let Some(pc) = pc {
        use webrtc::stats::StatsReportType;
        let stats = pc.get_stats().await;
        let pair = stats.reports.values().find_map(|report| match report {
            StatsReportType::CandidatePair(pair) if pair.nominated => Some(pair),
            _ => None,
        });
        if let Some(pair) = pair {
            let types: Vec<_> = [&pair.local_candidate_id, &pair.remote_candidate_id]
                .into_iter()
                .filter_map(|id| {
                    stats.reports.get(id).and_then(|report| match report {
                        StatsReportType::LocalCandidate(c)
                        | StatsReportType::RemoteCandidate(c) => Some(c.candidate_type.to_string()),
                        _ => None,
                    })
                })
                .collect();
            if types.len() == 2 && s.active() {
                lock(&s.data).route = Some(
                    if types.iter().any(|t| t == "relay") {
                        "relay"
                    } else {
                        "direct"
                    }
                    .into(),
                );
                s.publish();
            }
        }
    }
}
fn default_servers() -> Vec<ServerConfig> {
    vec![ServerConfig {
        id: "default-stun".into(),
        r#type: "stun".into(),
        url: "stun:stun.l.google.com:19302".into(),
        username: None,
        credential: None,
    }]
}
#[tauri::command]
pub fn set_ice_servers(servers: Vec<ServerConfig>) -> Result<bool, AppError> {
    for s in &servers {
        if !matches!(s.r#type.as_str(), "stun" | "turn")
            || s.url.trim().is_empty()
            || (s.r#type == "turn"
                && (s.username.as_ref().is_none_or(|v| v.is_empty())
                    || s.credential.as_ref().is_none_or(|v| v.is_empty())))
        {
            return Err(AppError::new(
                "ice_config",
                "ice",
                "Проверьте адрес и учётные данные сервера",
                true,
            ));
        }
    }
    lock(&MANAGER).servers = Some(servers);
    Ok(true)
}
#[tauri::command]
pub fn get_ice_servers() -> Vec<ServerConfig> {
    lock(&MANAGER)
        .servers
        .clone()
        .unwrap_or_else(default_servers)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkMode {
    Project,
    Custom,
    Disabled,
}

#[tauri::command]
pub fn set_network_mode(
    mode: NetworkMode,
    servers: Option<Vec<ServerConfig>>,
) -> Result<bool, AppError> {
    match mode {
        NetworkMode::Project => {
            lock(&MANAGER).servers = None;
            Ok(true)
        }
        NetworkMode::Custom => {
            set_ice_servers(servers.ok_or_else(|| {
                AppError::new("ice_config", "ice", "Не заданы свои серверы", true)
            })?)
        }
        NetworkMode::Disabled => {
            lock(&MANAGER).servers = Some(vec![]);
            Ok(true)
        }
    }
}
