use crate::{
    errors::AppError,
    peer::{
        data_channel,
        state::{self, lock, Phase, Snapshot, Verification},
    },
};
use serde::Deserialize;
use ssc_protocol::{ByeReason, ChatEvent};
use tauri::command;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisconnectReason {
    Exit,
    Expired,
    Error,
}
#[command]
pub async fn send_text(text: String) -> Result<bool, AppError> {
    let s = state::current()?;
    let result = data_channel::send_text_session(&s, &text).await;
    if let Err(error) = &result {
        s.error(error);
        if error.code == "nonce_exhausted" {
            s.close().await;
        }
    }
    result
}
#[command]
pub async fn send_chat_event(generation: u64, event: ChatEvent) -> Result<(), AppError> {
    let s = state::current()?;
    let result = data_channel::send_chat_event_session(&s, generation, event).await;
    if let Err(error) = &result {
        s.error(error);
        if matches!(
            error.code.as_str(),
            "nonce_exhausted" | "send_failed" | "backpressure_timeout"
        ) {
            s.close().await;
        }
    }
    result
}

#[command]
pub fn drain_chat_events(generation: u64) -> state::ChatEvents {
    state::current()
        .map(|s| s.drain_chat_events(generation))
        .unwrap_or(state::ChatEvents {
            generation,
            events: Vec::new(),
        })
}

#[command]
pub fn get_capture_protection() -> crate::peer::capture::CaptureProtection {
    crate::peer::capture::status()
}

#[command]
pub fn request_voice_capture(generation: u64) -> Result<(), AppError> {
    let s = state::current()?;
    crate::peer::microphone::arm(&s, generation)
}

#[command]
pub fn open_help_source(
    app: tauri::AppHandle,
    source: crate::peer::help::HelpSource,
) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(crate::peer::help::url(source), None::<&str>)
        .map_err(|_| {
            AppError::new(
                "help_open_failed",
                "help",
                "Не удалось открыть источник в браузере",
                true,
            )
        })
}

#[command]
pub async fn hash_attachment_chunk(
    generation: u64,
    id: String,
    data: Vec<u8>,
    reset: Option<bool>,
) -> Result<(), AppError> {
    let s = state::current()?;
    crate::peer::attachment_hash::update(&s, generation, &id, &data, reset.unwrap_or(false))
}

#[command]
pub fn finish_attachment_hash(generation: u64, id: String) -> Result<String, AppError> {
    let s = state::current()?;
    crate::peer::attachment_hash::finish(&s, generation, &id)
}

#[command]
pub fn abort_attachment_hash(generation: u64, id: String) -> Result<(), AppError> {
    let s = state::current()?;
    crate::peer::attachment_hash::abort(&s, generation, &id)
}
#[command]
pub async fn confirm_sas(generation: u64, sas: String) -> Result<bool, AppError> {
    let s = state::current()?;
    let result = data_channel::confirm(&s, generation, &sas).await;
    if let Err(error) = &result {
        if error.code != "verification_stale" {
            s.error(error);
            s.close().await;
        }
    }
    result
}
#[command]
pub async fn reject_sas(generation: u64) {
    if let Ok(s) = state::current() {
        if s.generation == generation {
            s.close().await;
        }
    }
}
#[command]
pub fn get_fingerprint() -> Option<Verification> {
    let s = state::current().ok()?;
    if !s.active() {
        return None;
    }
    let result = lock(&s.data).crypto.as_ref().map(|c| Verification {
        generation: s.generation,
        sas: c.sas.clone(),
    });
    result
}
#[command]
pub fn is_connected() -> bool {
    state::current().is_ok_and(|s| s.snapshot().phase == Phase::Connected)
}
#[command]
pub fn get_sas_status() -> bool {
    state::current().is_ok_and(|s| lock(&s.data).local_ready)
}
#[command]
pub fn get_session_state() -> Snapshot {
    state::current().map(|s| s.snapshot()).unwrap_or(Snapshot {
        generation: 0,
        phase: Phase::Idle,
        route: None,
        close_reason: None,
    })
}
#[command]
pub async fn disconnect(reason: Option<DisconnectReason>, generation: Option<u64>) {
    if let Ok(s) = state::current() {
        disconnect_session(&s, generation, reason.unwrap_or(DisconnectReason::Exit)).await;
    }
}

pub(crate) async fn disconnect_session(
    s: &std::sync::Arc<state::Session>,
    generation: Option<u64>,
    reason: DisconnectReason,
) {
    if generation.is_some_and(|generation| generation != s.generation) {
        return;
    }
    match reason {
        DisconnectReason::Exit => s.close_local(ByeReason::Exit).await,
        DisconnectReason::Expired => s.close_local(ByeReason::Expired).await,
        DisconnectReason::Error => s.close().await,
    }
}

#[command]
pub fn drain_messages(generation: u64) -> Vec<state::ReceivedMessage> {
    state::current()
        .ok()
        .filter(|s| s.generation == generation)
        .map(|s| s.drain_messages(generation))
        .unwrap_or_default()
}
