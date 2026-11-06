use super::{
    crypto::{CryptoCtx, KeyExchange},
    protocol::Reassembler,
    types::{IceCandidate, ServerConfig},
};
use crate::errors::AppError;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, Emitter};
use webrtc::{data_channel::RTCDataChannel, peer_connection::RTCPeerConnection};

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone, Serialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Idle,
    Invitation,
    Ice,
    KeyExchange,
    Verification,
    WaitingPeer,
    Connected,
    Recovering,
    Closed,
}
#[derive(Clone, Serialize)]
pub struct ReceivedMessage {
    pub generation: u64,
    pub id: String,
    pub text: String,
}

#[derive(Clone, Serialize)]
pub struct Snapshot {
    pub generation: u64,
    pub phase: Phase,
    pub route: Option<String>,
}

type Resources = (Option<Arc<RTCDataChannel>>, Option<Arc<RTCPeerConnection>>);

pub struct SessionData {
    pub connection_id: String,
    pub pc: Option<Arc<RTCPeerConnection>>,
    pub dc: Option<Arc<RTCDataChannel>>,
    pub exchange: Option<KeyExchange>,
    pub crypto: Option<CryptoCtx>,
    pub local_ready: bool,
    pub peer_ready: bool,
    pub transport_connected: bool,
    pub closed: bool,
    pub recovering: bool,
    pub recovery_epoch: u64,
    pub candidates: Vec<IceCandidate>,
    pub reassembler: Reassembler,
    pub route: Option<String>,
    pub inbox: Vec<ReceivedMessage>,
    pub next_message_id: u64,
}
pub struct Session {
    pub generation: u64,
    pub data: Mutex<SessionData>,
    pub sending: tokio::sync::Mutex<()>,
    events: Mutex<()>,
    pub app: Option<AppHandle>,
    pub link: Mutex<Option<crate::connect::Operation>>,
    stopped: tokio::sync::watch::Sender<bool>,
}
impl Session {
    pub fn new(generation: u64, connection_id: String, app: Option<AppHandle>) -> Arc<Self> {
        Arc::new(Self {
            generation,
            app,
            link: Mutex::new(None),
            stopped: tokio::sync::watch::channel(false).0,
            sending: tokio::sync::Mutex::new(()),
            events: Mutex::new(()),
            data: Mutex::new(SessionData {
                connection_id,
                pc: None,
                dc: None,
                exchange: None,
                crypto: None,
                local_ready: false,
                peer_ready: false,
                transport_connected: false,
                closed: false,
                recovering: false,
                recovery_epoch: 0,
                candidates: Vec::new(),
                reassembler: Reassembler::default(),
                route: None,
                inbox: Vec::new(),
                next_message_id: 0,
            }),
        })
    }
    pub fn connection_id(&self) -> String {
        lock(&self.data).connection_id.clone()
    }
    pub fn set_connection_id(&self, id: String) -> Result<(), AppError> {
        let mut d = lock(&self.data);
        if d.closed || d.pc.is_some() {
            return Err(AppError::new(
                "session_changed",
                "session",
                "Подключение уже изменилось",
                false,
            ));
        }
        d.connection_id = id;
        Ok(())
    }
    pub async fn stopped(&self) {
        let mut stopped = self.stopped.subscribe();
        let _ = stopped.wait_for(|v| *v).await;
    }
    pub fn start_recovery(&self) -> Option<u64> {
        let mut d = lock(&self.data);
        if d.closed || d.recovering {
            return None;
        }
        d.transport_connected = false;
        d.recovering = true;
        d.recovery_epoch += 1;
        Some(d.recovery_epoch)
    }
    pub fn recovered(&self) -> bool {
        let mut d = lock(&self.data);
        if d.closed {
            return false;
        }
        d.transport_connected = true;
        d.recovering = false;
        d.recovery_epoch += 1;
        true
    }
    pub fn snapshot(&self) -> Snapshot {
        let d = lock(&self.data);
        let phase = if d.closed {
            Phase::Closed
        } else if d.recovering {
            Phase::Recovering
        } else if d.crypto.is_some() {
            if d.local_ready && d.peer_ready && d.transport_connected {
                Phase::Connected
            } else if d.local_ready {
                Phase::WaitingPeer
            } else {
                Phase::Verification
            }
        } else if d.dc.is_some() && d.transport_connected {
            Phase::KeyExchange
        } else if d.pc.is_some() {
            Phase::Ice
        } else {
            Phase::Invitation
        };
        Snapshot {
            generation: self.generation,
            phase,
            route: d.route.clone(),
        }
    }
    pub fn active(&self) -> bool {
        // Replacement closes the previous instance before any new async work starts.
        !lock(&self.data).closed
    }
    pub fn ensure_active(&self) -> Result<(), AppError> {
        if self.active() {
            Ok(())
        } else {
            Err(AppError::new(
                "session_closed",
                "session",
                "Сессия уже завершена",
                true,
            ))
        }
    }
    pub fn emit<T: Serialize + Clone>(&self, event: &str, payload: T) {
        let _events = lock(&self.events);
        if self.active() {
            if let Some(app) = &self.app {
                let _ = app.emit(event, payload);
            }
        }
    }
    pub fn publish(&self) {
        let _events = lock(&self.events);
        let snapshot = self.snapshot();
        crate::diagnostics::record(
            self.generation,
            "session",
            phase_name(&snapshot.phase),
            "state",
            None,
        );
        let connected = snapshot.phase == Phase::Connected;
        if let Some(app) = &self.app {
            let _ = app.emit("ssc-session", snapshot);
        }
        drop(_events);
        if connected {
            crate::connect::connected(self);
        }
    }
    pub fn error(&self, error: &AppError) {
        crate::diagnostics::record(
            self.generation,
            "session",
            &error.stage,
            "error",
            Some(&error.code),
        );
        self.emit(
            "ssc-session-error",
            serde_json::json!({"generation":self.generation,"error":error}),
        );
    }
    fn retire(&self, expected_epoch: Option<u64>) -> Option<Resources> {
        let mut d = lock(&self.data);
        if d.closed
            || expected_epoch.is_some_and(|epoch| !d.recovering || d.recovery_epoch != epoch)
        {
            return None;
        }
        d.closed = true;
        d.recovery_epoch += 1;
        d.crypto = None;
        d.exchange = None;
        d.reassembler = Reassembler::default();
        d.local_ready = false;
        d.peer_ready = false;
        d.candidates.clear();
        d.inbox.clear();
        let resources = (d.dc.take(), d.pc.take());
        drop(d);
        self.stopped.send_replace(true);
        crate::connect::retire(self);
        Some(resources)
    }
    pub async fn close(&self) {
        if let Some(resources) = self.retire(None) {
            self.publish();
            shutdown(resources).await;
        }
    }
    pub async fn expire_recovery(&self, epoch: u64) {
        if let Some(resources) = self.retire(Some(epoch)) {
            crate::diagnostics::record(
                self.generation,
                "network",
                "ice",
                "error",
                Some("connection_lost"),
            );
            self.publish();
            shutdown(resources).await;
        }
    }
    pub fn close_from_callback(&self) {
        // Transport callbacks must not wait for the transport's own receive/state tasks.
        if let Some(resources) = self.retire(None) {
            self.publish();
            tauri::async_runtime::spawn(shutdown(resources));
        }
    }
}
async fn shutdown((dc, pc): Resources) {
    if let Some(dc) = dc {
        let _ = dc.close().await;
    }
    if let Some(pc) = pc {
        let _ = pc.close().await;
    }
}
pub fn phase_name(p: &Phase) -> &'static str {
    match p {
        Phase::Idle => "idle",
        Phase::Invitation => "invitation",
        Phase::Ice => "ice",
        Phase::KeyExchange => "key_exchange",
        Phase::Verification => "verification",
        Phase::WaitingPeer => "waiting_peer",
        Phase::Connected => "connected",
        Phase::Recovering => "recovering",
        Phase::Closed => "closed",
    }
}

#[derive(Default)]
pub struct Manager {
    pub generation: u64,
    pub current: Option<Arc<Session>>,
    pub servers: Option<Vec<ServerConfig>>,
}
pub static MANAGER: Lazy<Mutex<Manager>> = Lazy::new(|| Mutex::new(Manager::default()));
pub fn current() -> Result<Arc<Session>, AppError> {
    lock(&MANAGER)
        .current
        .clone()
        .ok_or_else(|| AppError::new("no_session", "session", "Нет активного соединения", true))
}
pub async fn begin(id: String, app: AppHandle) -> Arc<Session> {
    let (old, session) = {
        let mut m = lock(&MANAGER);
        m.generation += 1;
        let session = Session::new(m.generation, id, Some(app));
        let old = m.current.replace(session.clone());
        (old, session)
    };
    if let Some(old) = old {
        old.close().await;
    }
    session.publish();
    session
}
