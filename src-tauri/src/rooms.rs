//! RAM-only temporary-room capabilities; cryptographic v3 remains in the peer core.
use crate::{
    commands::candidate_api::{decode_checked, local_bundle, remote},
    connect::{LinkPhase, LinkSnapshot, Role},
    errors::AppError,
    peer::{
        connection::new_peer_with_servers,
        state::{self, lock, Session, MANAGER},
    },
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use reqwest::{Client, Method, Url};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use zeroize::Zeroizing;

static ROOM: Lazy<Mutex<Option<Arc<Operation>>>> = Lazy::new(|| Mutex::new(None));
struct Data {
    id: String,
    cap: Zeroizing<String>,
    snapshot: LinkSnapshot,
    timing: Deadlines,
    state_rank: u8,
    retired: bool,
}
struct Operation {
    generation: u64,
    origin: String,
    client: Client,
    data: Mutex<Data>,
    connected: AtomicBool,
}

const JOIN_TTL: Duration = Duration::from_secs(5 * 60);
const SESSION_TTL: Duration = Duration::from_secs(60 * 60);
const MAX_TTL: Duration = Duration::from_secs(65 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Wall time is for display; pinned monotonic limits cannot be extended by clock rollback.
struct Deadlines {
    anchor_wall: DateTime<Utc>,
    anchor_mono: Instant,
    wall: DateTime<Utc>,
    mono: Instant,
    join_wall: DateTime<Utc>,
    join_mono: Instant,
    max_wall: DateTime<Utc>,
    max_mono: Instant,
    session: Option<(DateTime<Utc>, Instant)>,
}
impl Deadlines {
    fn new(now: DateTime<Utc>, mono: Instant) -> Self {
        let wall = now + chrono::Duration::seconds(JOIN_TTL.as_secs() as i64);
        Self {
            anchor_wall: now,
            anchor_mono: mono,
            wall,
            mono: mono + JOIN_TTL,
            join_wall: wall,
            join_mono: mono + JOIN_TTL,
            max_wall: now + chrono::Duration::seconds(MAX_TTL.as_secs() as i64),
            max_mono: mono + MAX_TTL,
            session: None,
        }
    }
    fn expired(&self, _now: DateTime<Utc>, mono: Instant) -> bool {
        mono >= self.mono
    }
    fn update(&mut self, meta: &Meta, _now: DateTime<Utc>, mono: Instant) -> Result<(), AppError> {
        // Convert server UTC once against the start anchor; later OS clock corrections
        // cannot lengthen or prematurely end an already running conversation.
        let elapsed = chrono::Duration::from_std(mono.saturating_duration_since(self.anchor_mono))
            .map_err(|_| failure("room_expired"))?;
        let now = self.anchor_wall + elapsed;
        if self.expired(now, mono) {
            return Err(failure("room_expired"));
        }
        self.join_wall = self.join_wall.min(meta.join_expires_at);
        if let Ok(remaining) = meta.join_expires_at.signed_duration_since(now).to_std() {
            self.join_mono = self.join_mono.min(mono + remaining.min(JOIN_TTL));
        } else if meta.expires_at.is_none() {
            return Err(failure("room_expired"));
        }
        if let Some(expiry) = meta.expires_at {
            let remaining = expiry
                .signed_duration_since(now)
                .to_std()
                .map_err(|_| failure("room_expired"))?;
            let wall = expiry
                .min(now + chrono::Duration::seconds(SESSION_TTL.as_secs() as i64))
                .min(self.max_wall);
            let deadline = (mono + remaining.min(SESSION_TTL)).min(self.max_mono);
            self.session = Some(
                self.session
                    .map_or((wall, deadline), |(old_wall, old_mono)| {
                        (old_wall.min(wall), old_mono.min(deadline))
                    }),
            );
        }
        let (wall, deadline) = self.session.unwrap_or((self.join_wall, self.join_mono));
        if wall <= now || deadline <= mono {
            return Err(failure("room_expired"));
        }
        self.wall = wall;
        self.mono = deadline;
        Ok(())
    }
}
enum RequestError {
    Retry,
    Response,
    Denied,
}

enum Input {
    Creator,
    Guest {
        id: String,
        secret: Zeroizing<String>,
    },
}
#[derive(Deserialize)]
struct Meta {
    id: String,
    state: String,
    join_expires_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}
#[derive(Clone, Serialize)]
pub struct RoomState {
    generation: u64,
    expires_at: String,
    remaining_seconds: u64,
}
fn failure(code: &str) -> AppError {
    AppError::new(
        code,
        "room",
        if code == "room_expired" {
            "Время временного чата истекло. Создайте новое приглашение"
        } else {
            "Временный чат недоступен. Создайте новое приглашение"
        },
        false,
    )
}
fn token(value: &str) -> bool {
    value.len() == 43
        && B64
            .decode(value)
            .is_ok_and(|b| b.len() == 32 && B64.encode(b) == value)
}
fn random() -> Result<Zeroizing<String>, AppError> {
    let mut bytes = Zeroizing::new([0u8; 32]);
    SystemRandom::new()
        .fill(bytes.as_mut())
        .map_err(|_| failure("room_random"))?;
    Ok(Zeroizing::new(B64.encode(bytes.as_ref())))
}
fn parse_link(origin: &str, value: &str) -> Result<(String, Zeroizing<String>), AppError> {
    if value.len() > 4096 {
        return Err(failure("invalid_link"));
    }
    let url = Url::parse(value).map_err(|_| failure("invalid_link"))?;
    let id = url
        .path()
        .strip_prefix("/r/")
        .ok_or_else(|| failure("invalid_link"))?;
    let secret = url
        .fragment()
        .and_then(|s| s.strip_prefix("k="))
        .ok_or_else(|| failure("invalid_link"))?;
    if !token(id) || !token(secret) || value != format!("{origin}/r/{id}#k={secret}") {
        return Err(failure("invalid_link"));
    }
    Ok((id.into(), Zeroizing::new(secret.into())))
}
pub fn is_link(value: &str) -> bool {
    value.len() <= 4096 && Url::parse(value).is_ok_and(|u| u.path().starts_with("/r/"))
}
impl Operation {
    fn snapshot(&self) -> LinkSnapshot {
        lock(&self.data).snapshot.clone()
    }
    fn emit(&self, s: &Session) {
        s.emit("ssc-link", self.snapshot());
    }
    fn path(&self, suffix: &str) -> String {
        format!("/v1/rooms/{}{}", lock(&self.data).id, suffix)
    }
    async fn request(
        &self,
        s: &Session,
        method: Method,
        path: &str,
        body: Option<Value>,
        key: Option<&str>,
    ) -> Result<Value, AppError> {
        let cap = Zeroizing::new(lock(&self.data).cap.to_string());
        for attempt in 0..3 {
            s.ensure_active()?;
            let deadline = self.lease()?;
            let mut request = self
                .client
                .request(method.clone(), format!("{}{path}", self.origin));
            if !cap.is_empty() {
                request = request.bearer_auth(cap.as_str());
            }
            if let Some(key) = key {
                request = request.header("Idempotency-Key", key);
            }
            if let Some(body) = &body {
                request = request.json(body);
            }
            let exchange = async {
                let response = request.send().await.map_err(|_| RequestError::Retry)?;
                if response.status().is_server_error() || response.status().as_u16() == 429 {
                    return Err(RequestError::Retry);
                }
                if !response.status().is_success() {
                    return Err(RequestError::Denied);
                }
                crate::connect::http::read_json(response)
                    .await
                    .map_err(|error| {
                        if error.error.code == "helper_unavailable" {
                            RequestError::Retry
                        } else {
                            RequestError::Response
                        }
                    })
            };
            let result = tokio::select! {biased;
                _=s.stopped()=>return Err(failure("room_closed")),
                _=tokio::time::sleep_until(deadline.into())=>return Err(failure("room_expired")),
                result=tokio::time::timeout(REQUEST_TIMEOUT,exchange)=>result.unwrap_or(Err(RequestError::Retry)),
            };
            match result {
                Ok(value) => return Ok(value),
                Err(RequestError::Response) => return Err(failure("room_response")),
                Err(RequestError::Denied) => return Err(failure("room_unavailable")),
                Err(RequestError::Retry) if attempt == 2 => {
                    return Err(failure("room_unavailable"))
                }
                Err(RequestError::Retry) => {
                    tokio::select! {biased;
                        _=s.stopped()=>return Err(failure("room_closed")),
                        _=tokio::time::sleep_until(deadline.into())=>return Err(failure("room_expired")),
                        _=tokio::time::sleep(Duration::from_secs(1))=>{},
                    }
                }
            }
        }
        Err(failure("room_unavailable"))
    }
    fn lease(&self) -> Result<Instant, AppError> {
        let data = lock(&self.data);
        if data.retired {
            return Err(failure("room_closed"));
        }
        if data.timing.expired(Utc::now(), Instant::now()) {
            return Err(failure("room_expired"));
        }
        Ok(data.timing.mono)
    }
    fn metadata(&self, value: &Value) -> Result<(), AppError> {
        if value.get("expires_at").is_none() {
            return Err(failure("room_response"));
        }
        let meta: Meta =
            serde_json::from_value(value.clone()).map_err(|_| failure("room_response"))?;
        let mut data = lock(&self.data);
        if !token(&meta.id)
            || (!data.id.is_empty() && meta.id != data.id)
            || !["waiting", "joined", "answered", "connected"].contains(&meta.state.as_str())
        {
            return Err(failure("room_response"));
        }
        let rank = match meta.state.as_str() {
            "waiting" => 0,
            "joined" => 1,
            "answered" => 2,
            _ => 3,
        };
        if data.retired {
            return Err(failure("room_closed"));
        }
        if rank < data.state_rank || (rank == 3) != meta.expires_at.is_some() {
            return Err(failure("room_response"));
        }
        data.timing.update(&meta, Utc::now(), Instant::now())?;
        data.id = meta.id;
        data.state_rank = rank;
        data.snapshot.expires_at = Some(data.timing.wall.to_rfc3339());
        Ok(())
    }
    fn receipt(&self, cap: &str, url: Option<&str>) -> Result<(), AppError> {
        if !token(cap) {
            return Err(failure("room_response"));
        }
        let mut data = lock(&self.data);
        if data.retired {
            return Err(failure("room_closed"));
        }
        data.cap = Zeroizing::new(cap.into());
        data.snapshot.url = url.map(str::to_owned);
        Ok(())
    }
    fn fail(&self, s: &Session, error: &AppError) {
        {
            let mut data = lock(&self.data);
            if data.retired || data.snapshot.phase == LinkPhase::Error {
                return;
            }
            data.snapshot.phase = LinkPhase::Error;
            data.snapshot.error = Some(error.clone());
            data.snapshot.url = None;
        }
        self.emit(s);
        s.error(error);
    }
    fn stage(&self, s: &Session, phase: LinkPhase) {
        let mut data = lock(&self.data);
        if data.retired || matches!(data.snapshot.phase, LinkPhase::Error | LinkPhase::Cancelled) {
            return;
        }
        if phase == LinkPhase::Complete {
            data.snapshot.url = None;
        }
        data.snapshot.phase = phase;
        drop(data);
        self.emit(s);
    }
}

pub fn snapshot(s: &Session) -> Option<LinkSnapshot> {
    lock(&ROOM)
        .as_ref()
        .filter(|r| r.generation == s.generation)
        .map(|r| r.snapshot())
}
pub fn connected(s: &Session) {
    if s.snapshot().phase != crate::peer::state::Phase::Connected {
        return;
    }
    if let Some(room) = lock(&ROOM)
        .as_ref()
        .filter(|r| r.generation == s.generation)
    {
        room.connected.store(true, Ordering::Release);
    }
}
pub fn retire(s: &Session) {
    if let Some(room) = lock(&ROOM)
        .as_ref()
        .filter(|r| r.generation == s.generation)
        .cloned()
    {
        let mut data = lock(&room.data);
        data.retired = true;
        let id = std::mem::take(&mut data.id);
        let cap = std::mem::take(&mut data.cap);
        data.snapshot.url = None;
        if data.snapshot.phase != LinkPhase::Error {
            data.snapshot.phase = LinkPhase::Cancelled;
        }
        let snapshot = data.snapshot.clone();
        drop(data);
        if let Some(app) = &s.app {
            let _ = app.emit("ssc-link", snapshot);
        }
        if !id.is_empty() && !cap.is_empty() {
            tauri::async_runtime::spawn(async move {
                let _ = room
                    .client
                    .delete(format!("{}/v1/rooms/{id}", room.origin))
                    .bearer_auth(cap.as_str())
                    .send()
                    .await;
            });
        }
    }
}
fn checked_answer(
    s: &Session,
    encoded: &str,
) -> Result<crate::peer::types::ConnectionBundle, AppError> {
    let bundle = decode_checked(encoded, RTCSdpType::Answer)?;
    if bundle.sdp_payload.id != s.connection_id() {
        return Err(failure("session_mismatch"));
    }
    Ok(bundle)
}
async fn run(s: &Arc<Session>, room: &Arc<Operation>, input: Input) -> Result<(), AppError> {
    let (offer, creator) = match input {
        Input::Creator => {
            let key = random()?;
            let receipt = room
                .request(s, Method::POST, "/v1/rooms", Some(json!({})), Some(&key))
                .await?;
            room.metadata(&receipt)?;
            let cap = receipt["creator_token"]
                .as_str()
                .filter(|v| token(v))
                .ok_or_else(|| failure("room_response"))?;
            let link = receipt["invitation_url"]
                .as_str()
                .ok_or_else(|| failure("room_response"))?;
            let (id, _) = parse_link(&room.origin, link)?;
            if id != lock(&room.data).id {
                return Err(failure("room_response"));
            }
            room.receipt(cap, Some(link))?;
            (None, true)
        }
        Input::Guest { id, secret } => {
            lock(&room.data).id = id;
            let claim = random()?;
            let joined = room
                .request(
                    s,
                    Method::POST,
                    &room.path("/join"),
                    Some(json!({"invite_secret":secret.as_str(),"claim_key":claim.as_str()})),
                    None,
                )
                .await?;
            room.metadata(&joined)?;
            let cap = joined["guest_token"]
                .as_str()
                .filter(|v| token(v))
                .ok_or_else(|| failure("room_response"))?;
            room.receipt(cap, None)?;
            (
                Some(
                    joined["offer"]
                        .as_str()
                        .ok_or_else(|| failure("room_response"))?
                        .to_owned(),
                ),
                false,
            )
        }
    };
    let configured = lock(&MANAGER).servers.clone();
    let servers = if let Some(servers) = configured {
        servers
    } else {
        crate::access::room_servers(
            room.request(s, Method::POST, &room.path("/turn"), Some(json!({})), None)
                .await?,
        )?
    };
    if let Some(offer) = offer {
        let bundle = decode_checked(&offer, RTCSdpType::Offer)?;
        s.set_connection_id(bundle.sdp_payload.id.clone())?;
        new_peer_with_servers(s, false, servers).await?;
        remote(s, bundle).await?;
        let answer = local_bundle(s, false).await?;
        let value = room
            .request(
                s,
                Method::POST,
                &room.path("/answer"),
                Some(json!({"answer":answer})),
                None,
            )
            .await?;
        room.metadata(&value)?;
    } else {
        new_peer_with_servers(s, true, servers).await?;
        let offer = local_bundle(s, true).await?;
        let value = room
            .request(
                s,
                Method::POST,
                &room.path("/offer"),
                Some(json!({"offer":offer})),
                None,
            )
            .await?;
        room.metadata(&value)?;
        room.stage(s, LinkPhase::WaitingPeer);
    }
    let mut remote_set = !creator;
    let mut started = false;
    loop {
        s.ensure_active()?;
        room.lease()?;
        if !remote_set {
            let value = room
                .request(s, Method::GET, &room.path("/answer"), None, None)
                .await?;
            room.metadata(&value)?;
            if let Some(answer) = value["answer"].as_str() {
                remote(s, checked_answer(s, answer)?).await?;
                remote_set = true;
                room.stage(s, LinkPhase::WaitingConnection);
            }
        }
        if room.connected.load(Ordering::Acquire) && !started {
            let value = room
                .request(
                    s,
                    Method::POST,
                    &room.path("/connected"),
                    Some(json!({})),
                    None,
                )
                .await?;
            room.metadata(&value)?;
            started = true;
            room.stage(s, LinkPhase::Complete);
        } else {
            let value = room
                .request(s, Method::GET, &room.path(""), None, None)
                .await?;
            room.metadata(&value)?;
        }
        tokio::select! {_=s.stopped()=>return Ok(()),_=tokio::time::sleep(Duration::from_secs(2))=>{}}
    }
}
async fn expiry_watch(s: Arc<Session>, room: Arc<Operation>) {
    loop {
        if !s.active() {
            return;
        }
        if let Err(error) = room.lease() {
            room.fail(&s, &error);
            s.close().await;
            return;
        }
        tokio::select! {biased;
            _=s.stopped()=>return,
            _=tokio::time::sleep(Duration::from_millis(100))=>{},
        }
    }
}
async fn begin(app: AppHandle, input: Input) -> Result<LinkSnapshot, AppError> {
    let (origin, client) = crate::connect::http::access_config()?;
    let role = if matches!(input, Input::Creator) {
        Role::Creator
    } else {
        Role::Guest
    };
    let s = state::begin(crate::utils::random_id(), app).await;
    let room = Arc::new(Operation {
        generation: s.generation,
        origin,
        client,
        connected: AtomicBool::new(false),
        data: Mutex::new(Data {
            id: String::new(),
            cap: Zeroizing::new(String::new()),
            timing: Deadlines::new(Utc::now(), Instant::now()),
            state_rank: 0,
            retired: false,
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
        }),
    });
    {
        // Replacement closes under this same session lock before retiring its room.
        let session = lock(&s.data);
        if session.closed {
            return Err(failure("room_closed"));
        }
        *lock(&ROOM) = Some(room.clone());
    }
    let initial = room.snapshot();
    room.emit(&s);
    tauri::async_runtime::spawn(expiry_watch(s.clone(), room.clone()));
    tauri::async_runtime::spawn(async move {
        if let Err(error) = run(&s, &room, input).await {
            if s.active() {
                room.fail(&s, &error);
                s.close().await;
            }
        }
    });
    Ok(initial)
}
#[tauri::command]
pub async fn create_temporary_room(app: AppHandle) -> Result<LinkSnapshot, AppError> {
    begin(app, Input::Creator).await
}
pub async fn join(app: AppHandle, value: String) -> Result<LinkSnapshot, AppError> {
    let (origin, _) = crate::connect::http::access_config()?;
    let value = Zeroizing::new(value);
    let (id, secret) = parse_link(&origin, value.trim())?;
    begin(app, Input::Guest { id, secret }).await
}
#[tauri::command]
pub fn get_temporary_room_state() -> Option<RoomState> {
    let s = state::current().ok()?;
    let room = lock(&ROOM)
        .as_ref()
        .filter(|r| r.generation == s.generation)
        .cloned()?;
    if !s.active() {
        return None;
    }
    let d = lock(&room.data);
    (!d.retired).then(|| RoomState {
        generation: s.generation,
        expires_at: d.timing.wall.to_rfc3339(),
        remaining_seconds: d
            .timing
            .mono
            .saturating_duration_since(Instant::now())
            .as_millis()
            .div_ceil(1000) as u64,
    })
}

#[cfg(test)]
mod tests;
