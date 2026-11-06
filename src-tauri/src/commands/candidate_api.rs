use crate::{
    errors::AppError,
    peer::{
        connection::new_peer,
        crypto::{dec_bundle, enc_bundle},
        state::{self, lock, Session},
        types::{ConnectionBundle, SdpPayload},
    },
};
use std::{sync::Arc, time::Duration};
use tauri::{command, AppHandle};
use webrtc::{
    ice_transport::ice_candidate::RTCIceCandidateInit, peer_connection::sdp::sdp_type::RTCSdpType,
};
fn err(code: &str) -> AppError {
    AppError::new(
        code,
        "invitation",
        "Не удалось обработать данные соединения",
        true,
    )
}
fn validate(bundle: &ConnectionBundle, expected: RTCSdpType) -> Result<(), AppError> {
    if bundle.sdp_payload.sdp.sdp_type != expected {
        return Err(err("sdp_type"));
    }
    if bundle.sdp_payload.id.len() != 32
        || !bundle.sdp_payload.id.bytes().all(|c| c.is_ascii_hexdigit())
        || bundle.ice_candidates.len() > 256
        || bundle
            .ice_candidates
            .iter()
            .any(|c| c.connection_id != bundle.sdp_payload.id)
    {
        return Err(err("session_mismatch"));
    }
    Ok(())
}
pub(crate) async fn local_bundle(s: &Arc<Session>, offer: bool) -> Result<String, AppError> {
    let pc = lock(&s.data).pc.clone().ok_or_else(|| err("no_peer"))?;
    let mut complete = pc.gathering_complete_promise().await;
    let description = if offer {
        pc.create_offer(None).await
    } else {
        pc.create_answer(None).await
    }
    .map_err(|_| err("sdp_create"))?;
    pc.set_local_description(description)
        .await
        .map_err(|_| err("sdp_local"))?;
    tokio::time::timeout(Duration::from_secs(10), complete.recv())
        .await
        .map_err(|_| err("ice_timeout"))?;
    s.ensure_active()?;
    let sdp = pc
        .local_description()
        .await
        .ok_or_else(|| err("sdp_missing"))?;
    let candidates = lock(&s.data).candidates.clone();
    enc_bundle(&ConnectionBundle {
        sdp_payload: SdpPayload {
            sdp,
            id: s.connection_id(),
            ts: chrono::Utc::now().timestamp(),
        },
        ice_candidates: candidates,
    })
}
pub(crate) async fn remote(s: &Arc<Session>, bundle: ConnectionBundle) -> Result<(), AppError> {
    let pc = lock(&s.data).pc.clone().ok_or_else(|| err("no_peer"))?;
    s.ensure_active()?;
    pc.set_remote_description(bundle.sdp_payload.sdp)
        .await
        .map_err(|_| err("sdp_remote"))?;
    s.ensure_active()?;
    for c in bundle.ice_candidates {
        s.ensure_active()?;
        pc.add_ice_candidate(RTCIceCandidateInit {
            candidate: c.candidate,
            sdp_mid: c.sdp_mid,
            sdp_mline_index: c.sdp_mline_index,
            username_fragment: None,
        })
        .await
        .map_err(|_| err("ice_candidate"))?;
    }
    Ok(())
}
async fn finish<T>(s: &Arc<Session>, result: Result<T, AppError>) -> Result<T, AppError> {
    if let Err(error) = &result {
        s.error(error);
        s.close().await;
    }
    result
}
#[command]
pub async fn generate_offer_with_candidates(app: AppHandle) -> Result<String, AppError> {
    let s = state::begin(crate::utils::random_id(), app).await;
    let result = async {
        new_peer(&s, true).await?;
        local_bundle(&s, true).await
    }
    .await;
    finish(&s, result).await
}
#[command]
pub async fn accept_offer_with_candidates(
    app: AppHandle,
    encoded: String,
) -> Result<String, AppError> {
    let bundle = decode_checked(&encoded, RTCSdpType::Offer)?;
    let s = state::begin(bundle.sdp_payload.id.clone(), app).await;
    let result = async {
        new_peer(&s, false).await?;
        remote(&s, bundle).await?;
        local_bundle(&s, false).await
    }
    .await;
    finish(&s, result).await
}
#[command]
pub async fn set_answer_with_candidates(encoded: String) -> Result<bool, AppError> {
    let bundle = decode_checked(&encoded, RTCSdpType::Answer)?;
    let s = state::current()?;
    s.ensure_active()?;
    if bundle.sdp_payload.id != s.connection_id() {
        return Err(err("session_mismatch"));
    }
    let result = remote(&s, bundle).await.map(|_| true);
    finish(&s, result).await
}

pub(crate) fn decode_checked(
    encoded: &str,
    kind: RTCSdpType,
) -> Result<ConnectionBundle, AppError> {
    let result = dec_bundle(encoded.trim()).and_then(|b| {
        validate(&b, kind)?;
        Ok(b)
    });
    if let Err(e) = &result {
        crate::diagnostics::record(0, "invitation", &e.stage, "error", Some(&e.code));
    }
    result
}
