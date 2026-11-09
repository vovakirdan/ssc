//! Shared desktop/browser protocol v3. Entropy is supplied by the caller's CSPRNG.
#![forbid(unsafe_code)]

pub mod crypto;
pub mod handshake;
pub mod invitation;
pub mod message;
mod session;
pub use session::{ReceiveEvent, Session};
#[cfg(target_arch = "wasm32")]
mod wasm;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError {
    pub code: String,
    pub stage: String,
    pub message: String,
    pub retryable: bool,
}
impl ProtocolError {
    pub fn new(code: &str, stage: &str, message: &str, retryable: bool) -> Self {
        Self {
            code: code.into(),
            stage: stage.into(),
            message: message.into(),
            retryable,
        }
    }
}
impl std::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for ProtocolError {}
