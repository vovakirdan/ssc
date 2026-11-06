use crate::{
    errors::AppError,
    peer::{
        data_channel,
        state::{self, lock, Phase, Snapshot, Verification},
    },
};
use tauri::command;
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
    })
}
#[command]
pub async fn disconnect() {
    if let Ok(s) = state::current() {
        s.close().await;
    }
}

#[command]
pub fn drain_messages(generation: u64) -> Vec<state::ReceivedMessage> {
    state::current()
        .ok()
        .filter(|s| s.generation == generation)
        .map(|s| std::mem::take(&mut lock(&s.data).inbox))
        .unwrap_or_default()
}
