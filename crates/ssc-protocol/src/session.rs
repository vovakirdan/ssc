use crate::{
    crypto::CryptoCtx,
    handshake::{self, Handshake, Role},
    message::{self, Reassembler, KIND_CHUNK, KIND_READY},
    ProtocolError,
};
use zeroize::Zeroizing;

#[derive(Default)]
pub struct ReceiveEvent {
    pub outgoing: Vec<Vec<u8>>,
    pub sas: Option<String>,
    pub peer_ready: bool,
    pub message: Option<String>,
}

/// One ordered reliable binary DataChannel. The transport must serialize all calls
/// and send returned frames in order; a failed send retires the session.
pub struct Session {
    handshake: Option<Handshake>,
    crypto: Option<CryptoCtx>,
    reassembler: Reassembler,
    local_ready: bool,
    peer_ready: bool,
    closed: bool,
}
fn error(code: &str) -> ProtocolError {
    ProtocolError::new(
        code,
        "session",
        "Сессия недоступна; подключитесь заново",
        false,
    )
}
impl Session {
    pub fn new(
        session_id: &str,
        initiator: bool,
        seed: [u8; 32],
        nonce: [u8; 32],
    ) -> Result<Self, ProtocolError> {
        if session_id.is_empty() || session_id.len() > 128 {
            return Err(error("invalid_session"));
        }
        Ok(Self {
            handshake: Some(Handshake::from_entropy(
                session_id,
                Role::from_initiator(initiator),
                seed,
                nonce,
            )),
            crypto: None,
            reassembler: Reassembler::default(),
            local_ready: false,
            peer_ready: false,
            closed: false,
        })
    }
    pub fn start(&mut self) -> Result<Vec<Vec<u8>>, ProtocolError> {
        if self.closed {
            return Err(error("session_closed"));
        }
        let result = self
            .handshake
            .as_mut()
            .ok_or_else(|| error("handshake_complete"))?
            .start();
        if result.is_err() {
            self.close();
        }
        result
    }
    pub fn receive(&mut self, frame: &[u8]) -> Result<ReceiveEvent, ProtocolError> {
        if self.closed {
            return Err(error("session_closed"));
        }
        let result = self.receive_inner(frame);
        if result.is_err() {
            self.close();
        }
        result
    }
    fn receive_inner(&mut self, frame: &[u8]) -> Result<ReceiveEvent, ProtocolError> {
        let mut event = ReceiveEvent::default();
        if handshake::is_handshake(frame) {
            let step = self
                .handshake
                .as_mut()
                .ok_or_else(|| error("duplicate_reveal"))?
                .receive(frame)?;
            event.outgoing = step.outgoing;
            if let Some(keys) = step.crypto {
                self.crypto = Some(keys);
                self.handshake = None;
            }
        } else {
            let (kind, payload) = self
                .crypto
                .as_mut()
                .ok_or_else(|| error("handshake_required"))?
                .decrypt(frame)?;
            let payload = Zeroizing::new(payload);
            match kind {
                KIND_READY if payload.is_empty() => self.peer_ready = true,
                KIND_READY => return Err(error("invalid_ready")),
                KIND_CHUNK if self.local_ready && self.peer_ready => {
                    event.message = self.reassembler.push(&payload)?
                }
                KIND_CHUNK => return Err(error("verification_required")),
                _ => return Err(error("invalid_frame")),
            }
        }
        event.sas = self.sas().map(str::to_owned);
        event.peer_ready = self.peer_ready;
        Ok(event)
    }
    pub fn sas(&self) -> Option<&str> {
        self.crypto.as_ref().map(|c| c.sas.as_str())
    }
    pub fn peer_ready(&self) -> bool {
        self.peer_ready
    }
    pub fn local_ready(&self) -> bool {
        self.local_ready
    }
    pub fn confirm(&mut self) -> Result<Vec<u8>, ProtocolError> {
        if self.closed {
            return Err(error("session_closed"));
        }
        if self.local_ready {
            return Err(error("already_confirmed"));
        }
        let frame = self
            .crypto
            .as_mut()
            .ok_or_else(|| error("handshake_required"))?
            .encrypt(KIND_READY, &[]);
        match frame {
            Ok(frame) => {
                self.local_ready = true;
                Ok(frame)
            }
            Err(error) => {
                self.close();
                Err(error)
            }
        }
    }
    pub fn send_text(
        &mut self,
        text: &str,
        message_id: [u8; 16],
    ) -> Result<Vec<Vec<u8>>, ProtocolError> {
        if self.closed {
            return Err(error("session_closed"));
        }
        if !self.local_ready || !self.peer_ready {
            return Err(error("verification_required"));
        }
        let chunks = message::chunks_with_id(text, message_id)?;
        let crypto = self
            .crypto
            .as_mut()
            .ok_or_else(|| error("handshake_required"))?;
        let result: Result<Vec<Vec<u8>>, ProtocolError> = chunks
            .into_iter()
            .map(|chunk| crypto.encrypt(KIND_CHUNK, &Zeroizing::new(chunk)))
            .collect();
        if result.is_err() {
            self.close();
        }
        result
    }
    pub fn close(&mut self) {
        self.handshake = None;
        self.crypto = None;
        self.reassembler = Reassembler::default();
        self.local_ready = false;
        self.peer_ready = false;
        self.closed = true;
    }
}
