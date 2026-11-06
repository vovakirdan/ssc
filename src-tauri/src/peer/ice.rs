use crate::{
    errors::AppError,
    peer::{connection::rtc_config, types::ServerConfig},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use webrtc::{api::APIBuilder, ice_transport::ice_candidate_type::RTCIceCandidateType};
#[tauri::command]
pub async fn check_ice_server_availability(config: ServerConfig) -> Result<bool, AppError> {
    if !matches!(config.r#type.as_str(), "stun" | "turn") {
        return Err(AppError::new(
            "ice_config",
            "ice",
            "Неизвестный тип сервера",
            true,
        ));
    }
    let relay = config.r#type == "turn";
    let pc = Arc::new(
        APIBuilder::new()
            .build()
            .new_peer_connection(rtc_config(Some(vec![config])))
            .await
            .map_err(|_| AppError::new("ice_config", "ice", "Не удалось проверить сервер", true))?,
    );
    let found = Arc::new(AtomicBool::new(false));
    let matched = found.clone();
    pc.on_ice_candidate(Box::new(move |c| {
        let matched = matched.clone();
        Box::pin(async move {
            if let Some(c) = c {
                if (relay && c.typ == RTCIceCandidateType::Relay)
                    || (!relay && c.typ == RTCIceCandidateType::Srflx)
                {
                    matched.store(true, Ordering::SeqCst);
                }
            }
        })
    }));
    let result = async {
        let mut complete = pc.gathering_complete_promise().await;
        pc.create_data_channel("probe", None)
            .await
            .map_err(|_| ())?;
        let offer = pc.create_offer(None).await.map_err(|_| ())?;
        pc.set_local_description(offer).await.map_err(|_| ())?;
        let _ = tokio::time::timeout(Duration::from_secs(10), complete.recv()).await;
        Ok::<bool, ()>(found.load(Ordering::SeqCst))
    }
    .await;
    let _ = pc.close().await;
    crate::diagnostics::record(
        0,
        "network",
        "ice_probe",
        if result == Ok(true) {
            "available"
        } else {
            "unavailable"
        },
        None,
    );
    result.map_err(|_| AppError::new("ice_probe", "ice", "Не удалось проверить сервер", true))
}
