//! Native entropy adapter; wire state machine is shared with the WASM client.
use crate::errors::AppError;
use ring::rand::{SecureRandom, SystemRandom};
pub use ssc_protocol::handshake::{is_handshake, Role, Step};
use zeroize::Zeroizing;

pub struct Handshake(ssc_protocol::handshake::Handshake);
impl Handshake {
    pub fn new(session_id: &str, role: Role) -> Result<Self, AppError> {
        let rng = SystemRandom::new();
        let mut seed = Zeroizing::new([0; 32]);
        let mut nonce = Zeroizing::new([0; 32]);
        rng.fill(seed.as_mut())
            .and_then(|()| rng.fill(nonce.as_mut()))
            .map_err(|_| {
                AppError::new(
                    "key_generation",
                    "key_exchange",
                    "Обмен ключами отклонён; подключитесь заново",
                    false,
                )
            })?;
        Ok(Self(ssc_protocol::handshake::Handshake::from_entropy(
            session_id, role, *seed, *nonce,
        )))
    }
    pub fn start(&mut self) -> Result<Vec<Vec<u8>>, AppError> {
        self.0.start().map_err(Into::into)
    }
    pub fn receive(&mut self, frame: &[u8]) -> Result<Step, AppError> {
        self.0.receive(frame).map_err(Into::into)
    }
}
