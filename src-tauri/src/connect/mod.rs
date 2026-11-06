pub mod http;

use crate::{
    commands::candidate_api::{decode_checked, local_bundle, remote},
    errors::AppError,
    peer::{
        connection::new_peer,
        state::{self, lock, Session},
    },
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use http::{HttpError, Receipt, Service};
use ring::rand::{SecureRandom, SystemRandom};
use serde::Serialize;
use std::{future::Future, sync::Arc, time::Duration};
use tauri::{AppHandle, Emitter};
use tokio::time::Instant;
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use zeroize::Zeroizing;

#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Creator,
    Guest,
}
#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkPhase {
    Creating,
    WaitingPeer,
    Joining,
    SendingAnswer,
    WaitingConnection,
    Complete,
    Error,
    Cancelled,
}
#[derive(Clone, Serialize)]
pub struct LinkSnapshot {
    pub generation: u64,
    pub role: Role,
    pub phase: LinkPhase,
    pub url: Option<String>,
    pub expires_at: Option<String>,
    pub error: Option<AppError>,
}
enum Input {
    Creator {
        ttl: u64,
        key: Zeroizing<String>,
        offer: Option<Zeroizing<String>>,
        receipt: Option<Receipt>,
    },
    Guest {
        token: Zeroizing<String>,
        answer: Option<Zeroizing<String>>,
    },
    Finished,
}
pub struct Operation {
    snapshot: LinkSnapshot,
    service: Service,
    input: Input,
    running: bool,
    deadline: Option<Instant>,
}
fn error(code: &str, message: &str, retryable: bool) -> AppError {
    AppError::new(code, "link", message, retryable)
}
fn fatal(mut error: AppError) -> AppError {
    error.retryable = false;
    error
}
fn closed() -> AppError {
    error("link_cancelled", "Подключение отменено", false)
}
fn expired() -> AppError {
    error(
        "invitation_expired",
        "Срок приглашения истёк. Создайте новое приглашение.",
        false,
    )
}
fn with_op<T>(s: &Session, f: impl FnOnce(&mut Operation) -> T) -> Result<T, AppError> {
    let mut slot = lock(&s.link);
    s.ensure_active()?;
    slot.as_mut().map(f).ok_or_else(closed)
}
fn snapshot(s: &Session) -> Option<LinkSnapshot> {
    lock(&s.link).as_ref().map(|op| op.snapshot.clone())
}
fn publish(s: &Session) {
    if let Some(value) = snapshot(s) {
        s.emit("ssc-link", value);
    }
}
fn update(s: &Session, phase: LinkPhase) {
    if with_op(s, |op| {
        if op.snapshot.phase != LinkPhase::Complete {
            op.snapshot.phase = phase;
            op.snapshot.error = None;
        }
    })
    .is_ok()
    {
        crate::diagnostics::record(
            s.generation,
            "helper",
            match phase {
                LinkPhase::Creating => "creating",
                LinkPhase::WaitingPeer => "waiting_peer",
                LinkPhase::Joining => "joining",
                LinkPhase::SendingAnswer => "sending_answer",
                _ => "waiting_connection",
            },
            "state",
            None,
        );
        publish(s);
    }
}
fn established(s: &Session) -> bool {
    lock(&s.data).crypto.is_some()
}
fn set_expiry(s: &Session, expires: DateTime<Utc>) -> Result<(), AppError> {
    let remaining = (expires - Utc::now())
        .to_std()
        .map_err(|_| expired())?
        .min(Duration::from_secs(3600));
    with_op(s, |op| {
        op.deadline = Some(op.deadline.map_or(Instant::now() + remaining, |old| {
            old.min(Instant::now() + remaining)
        }));
        op.snapshot.expires_at = Some(expires.to_rfc3339());
    })
}
fn deadline(s: &Session) -> Result<Instant, AppError> {
    with_op(s, |op| {
        op.deadline
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(3600))
    })
}
async fn bounded<T>(s: &Session, future: impl Future<Output = T>) -> Result<T, AppError> {
    let limit = deadline(s)?;
    tokio::select! {
        biased;
        _=s.stopped()=>Err(closed()),
        _=tokio::time::sleep_until(limit)=>Err(expired()),
        result=future=>{s.ensure_active()?;Ok(result)}
    }
}
async fn sleep(s: &Session, wait: Duration) -> Result<(), AppError> {
    let wait = wait.min(deadline(s)?.saturating_duration_since(Instant::now()));
    bounded(s, tokio::time::sleep(wait)).await
}
async fn request<T, F, Fut>(s: &Session, mut action: F) -> Result<T, AppError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, HttpError>>,
{
    let delays = [1, 2, 5];
    for attempt in 0..=delays.len() {
        match bounded(s, action()).await? {
            Ok(value) => return Ok(value),
            Err(failure) => {
                crate::diagnostics::record(
                    s.generation,
                    "helper",
                    "request",
                    "error",
                    Some(&failure.error.code),
                );
                if !failure.error.retryable || attempt == delays.len() {
                    return Err(failure.error);
                }
                let wait = failure
                    .retry_after
                    .unwrap_or_else(|| Duration::from_secs(delays[attempt]));
                sleep(s, wait).await?;
            }
        }
    }
    Err(error(
        "helper_unavailable",
        "Помощник подключения недоступен",
        true,
    ))
}
fn start(s: Arc<Session>) {
    tauri::async_runtime::spawn(async move {
        let result = run(&s).await;
        if let Err(failure) = result {
            if s.active() && established(&s) {
                update(&s, LinkPhase::WaitingConnection);
            } else if s.active() {
                let changed = with_op(&s, |op| {
                    if op.snapshot.phase == LinkPhase::Complete {
                        return false;
                    }
                    op.running = false;
                    op.snapshot.phase = LinkPhase::Error;
                    op.snapshot.error = Some(failure.clone());
                    true
                })
                .unwrap_or(false);
                if !changed {
                    return;
                }
                crate::diagnostics::record(
                    s.generation,
                    "helper",
                    "operation",
                    "error",
                    Some(&failure.code),
                );
                publish(&s);
            }
        }
        let _ = with_op(&s, |op| op.running = false);
    });
}
async fn run(s: &Arc<Session>) -> Result<(), AppError> {
    let role = with_op(s, |op| op.snapshot.role)?;
    if established(s) {
        update(s, LinkPhase::WaitingConnection);
        return Ok(());
    }
    match role {
        Role::Creator => create_and_wait(s).await,
        Role::Guest => join(s).await,
    }
}
async fn create_and_wait(s: &Arc<Session>) -> Result<(), AppError> {
    let (service, needs_offer) = with_op(s, |op| {
        (
            op.service.clone(),
            matches!(&op.input, Input::Creator { offer: None, .. }),
        )
    })?;
    if needs_offer {
        new_peer(s, true).await.map_err(fatal)?;
        s.ensure_active()?;
        let offer = local_bundle(s, true).await.map_err(fatal)?;
        with_op(s, |op| {
            if let Input::Creator { offer: stored, .. } = &mut op.input {
                *stored = Some(Zeroizing::new(offer));
            }
        })?;
    }
    let existing = with_op(s, |op| match &op.input {
        Input::Creator { receipt, .. } => receipt.clone(),
        _ => None,
    })?;
    let receipt = if let Some(receipt) = existing {
        receipt
    } else {
        let (ttl, key, offer) = with_op(s, |op| match &op.input {
            Input::Creator {
                ttl,
                key,
                offer: Some(offer),
                ..
            } => Ok((*ttl, key.clone(), offer.clone())),
            _ => Err(closed()),
        })??;
        with_op(s, |op| {
            if op.deadline.is_none() {
                op.deadline = Some(Instant::now() + Duration::from_secs(ttl));
            }
        })?;
        let receipt = request(s, || service.create(&offer, ttl, &key)).await?;
        set_expiry(s, receipt.expires_at)?;
        with_op(s, |op| {
            if let Input::Creator {
                receipt: stored, ..
            } = &mut op.input
            {
                *stored = Some(receipt.clone());
                op.snapshot.url = Some(receipt.invitation_url.clone());
            }
        })?;
        receipt
    };
    update(s, LinkPhase::WaitingPeer);
    loop {
        if established(s) {
            update(s, LinkPhase::WaitingConnection);
            return Ok(());
        }
        let answer = request(s, || {
            service.poll_answer(&receipt.token, &receipt.creator_token)
        })
        .await?;
        if let Some(answer) = answer {
            let bundle = decode_checked(&answer, RTCSdpType::Answer).map_err(fatal)?;
            if bundle.sdp_payload.id != s.connection_id() {
                return Err(error(
                    "session_mismatch",
                    "Ответ относится к другому подключению",
                    false,
                ));
            }
            bounded(s, remote(s, bundle)).await?.map_err(fatal)?;
            update(s, LinkPhase::WaitingConnection);
            return Ok(());
        }
        sleep(s, Duration::from_secs(1)).await?;
    }
}
async fn join(s: &Arc<Session>) -> Result<(), AppError> {
    let (service, token, mut answer) = with_op(s, |op| match &op.input {
        Input::Guest { token, answer } => Ok((op.service.clone(), token.clone(), answer.clone())),
        _ => Err(closed()),
    })??;
    if answer.is_none() {
        let offer = request(s, || service.fetch_offer(&token)).await?;
        set_expiry(s, offer.expires_at)?;
        let bundle = decode_checked(&offer.offer, RTCSdpType::Offer).map_err(fatal)?;
        s.set_connection_id(bundle.sdp_payload.id.clone())?;
        new_peer(s, false).await.map_err(fatal)?;
        s.ensure_active()?;
        remote(s, bundle).await.map_err(fatal)?;
        let prepared = local_bundle(s, false).await.map_err(fatal)?;
        answer = Some(Zeroizing::new(prepared));
        with_op(s, |op| {
            if let Input::Guest { answer: stored, .. } = &mut op.input {
                *stored = answer.clone();
            }
        })?;
    }
    update(s, LinkPhase::SendingAnswer);
    if let Some(answer) = answer {
        request(s, || service.send_answer(&token, &answer)).await?;
    }
    update(s, LinkPhase::WaitingConnection);
    Ok(())
}
fn cleanup(generation: u64, service: Service, receipt: Receipt) {
    tauri::async_runtime::spawn(async move {
        match service.cancel(&receipt.token, &receipt.creator_token).await {
            Ok(()) => crate::diagnostics::record(generation, "helper", "cleanup", "done", None),
            Err(error) => crate::diagnostics::record(
                generation,
                "helper",
                "cleanup",
                "error",
                Some(&error.error.code),
            ),
        }
    });
}
pub fn connected(s: &Session) {
    let cleanup_data = {
        let mut slot = lock(&s.link);
        if let Some(op) = slot.as_mut() {
            if op.snapshot.phase == LinkPhase::Complete {
                return;
            }
            op.snapshot.phase = LinkPhase::Complete;
            op.snapshot.error = None;
            op.snapshot.url = None;
            let old = std::mem::replace(&mut op.input, Input::Finished);
            match old {
                Input::Creator {
                    receipt: Some(receipt),
                    ..
                } => Some((op.service.clone(), receipt)),
                _ => None,
            }
        } else {
            return;
        }
    };
    publish(s);
    if let Some((service, receipt)) = cleanup_data {
        cleanup(s.generation, service, receipt);
    }
}
pub fn retire(s: &Session) {
    if let Some(mut op) = lock(&s.link).take() {
        op.snapshot.phase = LinkPhase::Cancelled;
        op.snapshot.url = None;
        op.snapshot.error = None;
        if let Some(app) = &s.app {
            let _ = app.emit("ssc-link", &op.snapshot);
        }
        if let Input::Creator {
            receipt: Some(receipt),
            ..
        } = op.input
        {
            cleanup(s.generation, op.service, receipt);
        }
    }
}
fn initial(s: &Session, role: Role, service: Service, input: Input) -> Result<(), AppError> {
    let mut slot = lock(&s.link);
    s.ensure_active()?;
    *slot = Some(Operation {
        snapshot: LinkSnapshot {
            generation: s.generation,
            role,
            phase: if role == Role::Creator {
                LinkPhase::Creating
            } else {
                LinkPhase::Joining
            },
            url: None,
            expires_at: None,
            error: None,
        },
        service,
        input,
        running: true,
        deadline: if role == Role::Guest {
            Some(Instant::now() + Duration::from_secs(3600))
        } else {
            None
        },
    });
    drop(slot);
    publish(s);
    Ok(())
}
#[tauri::command]
pub async fn create_link_invitation(
    app: AppHandle,
    ttl_seconds: u64,
) -> Result<LinkSnapshot, AppError> {
    if !(60..=3600).contains(&ttl_seconds) {
        return Err(error(
            "invalid_ttl",
            "Время приглашения должно быть от 1 до 60 минут",
            false,
        ));
    }
    let service = Service::from_env()?;
    let mut key = Zeroizing::new([0u8; 32]);
    SystemRandom::new()
        .fill(key.as_mut())
        .map_err(|_| error("random_failed", "Не удалось создать приглашение", true))?;
    let key = Zeroizing::new(URL_SAFE_NO_PAD.encode(key.as_ref()));
    let s = state::begin(crate::utils::random_id(), app).await;
    initial(
        &s,
        Role::Creator,
        service,
        Input::Creator {
            ttl: ttl_seconds,
            key,
            offer: None,
            receipt: None,
        },
    )?;
    let result = snapshot(&s).ok_or_else(closed)?;
    start(s);
    Ok(result)
}
#[tauri::command]
pub async fn join_link_invitation(app: AppHandle, url: String) -> Result<LinkSnapshot, AppError> {
    let service = Service::from_env()?;
    let token = Zeroizing::new(service.parse_link(&url)?);
    let s = state::begin(crate::utils::random_id(), app).await;
    initial(
        &s,
        Role::Guest,
        service,
        Input::Guest {
            token,
            answer: None,
        },
    )?;
    let result = snapshot(&s).ok_or_else(closed)?;
    start(s);
    Ok(result)
}
#[tauri::command]
pub fn get_link_state() -> Option<LinkSnapshot> {
    state::current().ok().and_then(|s| snapshot(&s))
}
#[tauri::command]
pub async fn retry_link_invitation(generation: u64) -> Result<LinkSnapshot, AppError> {
    let s = state::current()?;
    if s.generation != generation {
        return Err(closed());
    }
    with_op(&s, |op| {
        if op.running
            || op.snapshot.phase != LinkPhase::Error
            || !op.snapshot.error.as_ref().is_some_and(|e| e.retryable)
        {
            return Err(closed());
        }
        op.running = true;
        op.snapshot.error = None;
        op.snapshot.phase = if op.snapshot.role == Role::Creator {
            LinkPhase::Creating
        } else {
            LinkPhase::Joining
        };
        Ok(())
    })??;
    let result = snapshot(&s).ok_or_else(closed)?;
    publish(&s);
    start(s);
    Ok(result)
}
#[tauri::command]
pub async fn cancel_link_invitation(generation: u64) {
    if let Ok(s) = state::current() {
        if s.generation == generation {
            s.close().await;
        }
    }
}
#[cfg(test)]
mod tests;
