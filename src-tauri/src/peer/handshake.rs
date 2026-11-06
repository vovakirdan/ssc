//! Committed ephemeral key exchange. Both commitments precede either local reveal.
use super::crypto::{CryptoCtx, KeyExchange, VERSION};
use crate::errors::AppError;
use ring::rand::{SecureRandom, SystemRandom};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub const KIND_COMMIT: u8 = 0;
pub const KIND_REVEAL: u8 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Initiator = 0,
    Responder = 1,
}
impl Role {
    pub fn from_initiator(initiator: bool) -> Self {
        if initiator {
            Self::Initiator
        } else {
            Self::Responder
        }
    }
    fn peer(self) -> u8 {
        1 - self as u8
    }
}

pub struct Step {
    pub outgoing: Vec<Vec<u8>>,
    pub crypto: Option<CryptoCtx>,
}

pub struct Handshake {
    role: Role,
    context: [u8; 32],
    exchange: Option<KeyExchange>,
    nonce: Zeroizing<[u8; 32]>,
    commitment: [u8; 32],
    committed: bool,
    revealed: bool,
    peer_commitment: Option<[u8; 32]>,
}

fn error(code: &str) -> AppError {
    AppError::new(
        code,
        "key_exchange",
        "Обмен ключами отклонён; подключитесь заново",
        false,
    )
}

fn commitment(context: &[u8; 32], role: u8, public: &[u8; 32], nonce: &[u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"ssc-chat-v3/key-commitment\0");
    hash.update([VERSION, role]);
    hash.update(context);
    hash.update(public);
    hash.update(nonce);
    hash.finalize().into()
}

impl Handshake {
    pub fn new(session_id: &str, role: Role) -> Result<Self, AppError> {
        let exchange = KeyExchange::new()?;
        let mut nonce = Zeroizing::new([0u8; 32]);
        SystemRandom::new()
            .fill(nonce.as_mut())
            .map_err(|_| error("key_generation"))?;
        let mut hash = Sha256::new();
        hash.update(b"ssc-chat-v3/session-context\0");
        hash.update((session_id.len() as u64).to_be_bytes());
        hash.update(session_id.as_bytes());
        let context = hash.finalize().into();
        let commitment = commitment(&context, role as u8, &exchange.public_key(), &nonce);
        Ok(Self {
            role,
            context,
            exchange: Some(exchange),
            nonce,
            commitment,
            committed: false,
            revealed: false,
            peer_commitment: None,
        })
    }

    /// The caller serializes sending these frames with receive(), in this order.
    pub fn start(&mut self) -> Result<Vec<Vec<u8>>, AppError> {
        if self.committed || self.exchange.is_none() {
            return Err(error("duplicate_commitment"));
        }
        self.committed = true;
        let mut frame = vec![VERSION, KIND_COMMIT, self.role as u8];
        frame.extend_from_slice(&self.commitment);
        let mut frames = vec![frame];
        if self.peer_commitment.is_some() {
            frames.push(self.reveal()?);
        }
        Ok(frames)
    }

    fn reveal(&mut self) -> Result<Vec<u8>, AppError> {
        if !self.committed || self.peer_commitment.is_none() || self.revealed {
            return Err(error("handshake_order"));
        }
        let exchange = self
            .exchange
            .as_ref()
            .ok_or_else(|| error("duplicate_reveal"))?;
        let mut frame = vec![VERSION, KIND_REVEAL, self.role as u8];
        frame.extend_from_slice(&exchange.public_key());
        frame.extend_from_slice(self.nonce.as_ref());
        self.revealed = true;
        Ok(frame)
    }

    pub fn receive(&mut self, frame: &[u8]) -> Result<Step, AppError> {
        if frame.len() < 3 || frame[0] != VERSION || frame[2] != self.role.peer() {
            return Err(error("handshake_context"));
        }
        if self.exchange.is_none() {
            return Err(error("duplicate_reveal"));
        }
        match frame[1] {
            KIND_COMMIT => {
                if frame.len() != 35 {
                    return Err(error("commitment_length"));
                }
                if self.peer_commitment.is_some() {
                    return Err(error("duplicate_commitment"));
                }
                let peer =
                    <[u8; 32]>::try_from(&frame[3..]).map_err(|_| error("commitment_length"))?;
                if peer == self.commitment {
                    return Err(error("reflected_commitment"));
                }
                self.peer_commitment = Some(peer);
                let outgoing = if self.committed {
                    vec![self.reveal()?]
                } else {
                    vec![]
                };
                Ok(Step {
                    outgoing,
                    crypto: None,
                })
            }
            KIND_REVEAL => {
                if frame.len() != 67 {
                    return Err(error("reveal_length"));
                }
                if !self.committed || !self.revealed {
                    return Err(error("handshake_order"));
                }
                let expected = self
                    .peer_commitment
                    .ok_or_else(|| error("handshake_order"))?;
                let public =
                    <[u8; 32]>::try_from(&frame[3..35]).map_err(|_| error("reveal_length"))?;
                let nonce =
                    <[u8; 32]>::try_from(&frame[35..]).map_err(|_| error("reveal_length"))?;
                if commitment(&self.context, self.role.peer(), &public, &nonce) != expected {
                    return Err(error("commitment_mismatch"));
                }
                let local = self
                    .exchange
                    .as_ref()
                    .ok_or_else(|| error("duplicate_reveal"))?
                    .public_key();
                let mut hash = Sha256::new();
                hash.update(b"ssc-chat-v3/transcript\0");
                hash.update([VERSION]);
                hash.update(self.context);
                // Explicit role order gives both endpoints exactly the same transcript.
                for (role, commit, key, nonce) in if self.role == Role::Initiator {
                    [
                        (0, self.commitment, local, *self.nonce),
                        (1, expected, public, nonce),
                    ]
                } else {
                    [
                        (0, expected, public, nonce),
                        (1, self.commitment, local, *self.nonce),
                    ]
                } {
                    hash.update([role]);
                    hash.update(commit);
                    hash.update(key);
                    hash.update(nonce);
                }
                let transcript = hash.finalize().into();
                let exchange = self
                    .exchange
                    .take()
                    .ok_or_else(|| error("duplicate_reveal"))?;
                let crypto = exchange.finish(public, &transcript, self.role == Role::Initiator)?;
                Ok(Step {
                    outgoing: vec![],
                    crypto: Some(crypto),
                })
            }
            _ => Err(error("handshake_type")),
        }
    }
}

pub fn is_handshake(frame: &[u8]) -> bool {
    frame.len() >= 2 && matches!(frame[1], KIND_COMMIT | KIND_REVEAL)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (Handshake, Handshake) {
        (
            Handshake::new("session", Role::Initiator).unwrap(),
            Handshake::new("session", Role::Responder).unwrap(),
        )
    }
    #[test]
    fn both_commitments_precede_reveals_and_keys_agree() {
        let (mut a, mut b) = pair();
        let ac = a.start().unwrap().remove(0);
        let bc = b.start().unwrap().remove(0);
        assert_eq!(ac.len(), 35);
        assert_eq!(bc.len(), 35);
        let ar = a.receive(&bc).unwrap().outgoing.remove(0);
        let br = b.receive(&ac).unwrap().outgoing.remove(0);
        let mut ak = a.receive(&br).unwrap().crypto.unwrap();
        let mut bk = b.receive(&ar).unwrap().crypto.unwrap();
        assert_eq!(ak.sas, bk.sas);
        assert_eq!(
            bk.decrypt(&ak.encrypt(2, b"private").unwrap()).unwrap().1,
            b"private"
        );
        assert!(a.receive(&br).is_err());
    }
    #[test]
    fn commitment_received_before_open_does_not_reveal_until_local_commit() {
        let (mut a, mut b) = pair();
        let ac = a.start().unwrap().remove(0);
        assert!(b.receive(&ac).unwrap().outgoing.is_empty());
        let frames = b.start().unwrap();
        assert_eq!(frames[0][1], KIND_COMMIT);
        assert_eq!(frames[1][1], KIND_REVEAL);
    }
    #[test]
    fn early_duplicate_reflected_wrong_role_and_old_version_frames_are_rejected() {
        let (mut a, mut b) = pair();
        let ac = a.start().unwrap().remove(0);
        let bc = b.start().unwrap().remove(0);
        assert!(a.receive(&[VERSION, KIND_REVEAL, 1]).is_err());
        assert!(a.receive(&ac).is_err());
        let mut reflection = ac.clone();
        reflection[2] = 1;
        assert!(a.receive(&reflection).is_err());
        let mut old = bc.clone();
        old[0] = 2;
        assert!(a.receive(&old).is_err());
        a.receive(&bc).unwrap();
        assert!(a.receive(&bc).is_err());
        assert!(a.start().is_err());
    }
    #[test]
    fn valid_reveal_is_rejected_before_the_peer_commitment() {
        let (mut a, mut b) = pair();
        let ac = a.start().unwrap().remove(0);
        b.start().unwrap();
        let br = b.receive(&ac).unwrap().outgoing.remove(0);
        assert_eq!(a.receive(&br).err().unwrap().code, "handshake_order");
    }
    #[test]
    fn low_order_key_is_rejected_even_with_a_matching_commitment() {
        let (mut a, _) = pair();
        a.start().unwrap();
        let nonce = [1; 32];
        let mut commit = vec![VERSION, KIND_COMMIT, 1];
        commit.extend_from_slice(&commitment(&a.context, 1, &[0; 32], &nonce));
        a.receive(&commit).unwrap();
        let mut reveal = vec![VERSION, KIND_REVEAL, 1];
        reveal.extend_from_slice(&[0; 32]);
        reveal.extend_from_slice(&nonce);
        assert_eq!(a.receive(&reveal).err().unwrap().code, "invalid_key");
    }
    #[test]
    fn changed_key_nonce_and_cross_session_reveals_are_rejected() {
        for changed in [3, 35] {
            let (mut a, mut b) = pair();
            let ac = a.start().unwrap().remove(0);
            let bc = b.start().unwrap().remove(0);
            a.receive(&bc).unwrap();
            let mut br = b.receive(&ac).unwrap().outgoing.remove(0);
            br[changed] ^= 1;
            assert_eq!(a.receive(&br).err().unwrap().code, "commitment_mismatch");
        }
        let mut a = Handshake::new("first", Role::Initiator).unwrap();
        let mut b = Handshake::new("second", Role::Responder).unwrap();
        let ac = a.start().unwrap().remove(0);
        let bc = b.start().unwrap().remove(0);
        a.receive(&bc).unwrap();
        let br = b.receive(&ac).unwrap().outgoing.remove(0);
        assert_eq!(a.receive(&br).err().unwrap().code, "commitment_mismatch");
    }
}
