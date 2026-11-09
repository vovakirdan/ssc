use crate::ProtocolError;
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use x25519_dalek::{PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};
pub const VERSION: u8 = 3;
const HEADER_LEN: usize = 10;
const MAX_FRAME_BYTES: usize = 70 * 1024;
fn error(code: &str, message: &str) -> ProtocolError {
    ProtocolError::new(code, "crypto", message, false)
}
pub struct KeyExchange {
    private: StaticSecret,
    public: [u8; 32],
}
impl KeyExchange {
    /// The caller must supply 32 fresh bytes from the operating system/browser CSPRNG.
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let seed = Zeroizing::new(seed);
        let private = StaticSecret::from(*seed);
        let public = PublicKey::from(&private).to_bytes();
        Self { private, public }
    }
    #[cfg(test)]
    pub(crate) fn new() -> Result<Self, ProtocolError> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let mut seed = [42; 32];
        seed[..8].copy_from_slice(&NEXT.fetch_add(1, Ordering::Relaxed).to_be_bytes());
        Ok(Self::from_seed(seed))
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }
    pub fn finish(
        self,
        peer: [u8; 32],
        transcript: &[u8; 32],
        initiator: bool,
    ) -> Result<CryptoCtx, ProtocolError> {
        if peer == self.public {
            return Err(error(
                "invalid_key",
                "Публичный ключ собеседника недопустим",
            ));
        }
        let shared = self.private.diffie_hellman(&PublicKey::from(peer));
        if !shared.was_contributory() {
            return Err(error("invalid_key", "Не удалось согласовать ключ сессии"));
        }
        let hk = Hkdf::<Sha256>::new(Some(transcript), shared.as_bytes());
        let mut keys = Zeroizing::new([0u8; 64]);
        hk.expand(b"ssc-chat-v3/directional-keys", keys.as_mut())
            .map_err(|_| error("key_derivation", "Не удалось получить ключи шифрования"))?;
        let (k1, k2) = keys.split_at(32);
        let (send, recv) = if initiator { (k1, k2) } else { (k2, k1) };
        let sealing = ChaCha20Poly1305::new_from_slice(send)
            .map_err(|_| error("key_derivation", "Некорректный ключ шифрования"))?;
        let opening = ChaCha20Poly1305::new_from_slice(recv)
            .map_err(|_| error("key_derivation", "Некорректный ключ шифрования"))?;
        let mut sas_key = Zeroizing::new([0u8; 32]);
        hk.expand(b"ssc-chat-v3/sas", sas_key.as_mut())
            .map_err(|_| error("key_derivation", "Не удалось получить код проверки"))?;
        let sas = hex::encode(&sas_key[..6]);
        Ok(CryptoCtx {
            sealing,
            opening,
            next_send: 1,
            last_recv: 0,
            sas,
        })
    }
}

pub struct CryptoCtx {
    sealing: ChaCha20Poly1305,
    opening: ChaCha20Poly1305,
    next_send: u64,
    last_recv: u64,
    pub sas: String,
}
impl Drop for CryptoCtx {
    fn drop(&mut self) {
        self.sas.zeroize();
        self.next_send.zeroize();
        self.last_recv.zeroize();
    }
}
impl CryptoCtx {
    pub fn encrypt(&mut self, kind: u8, payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        if !(1..=2).contains(&kind) || payload.len() > MAX_FRAME_BYTES - HEADER_LEN - 16 {
            return Err(error("invalid_frame", "Недопустимый кадр сообщения"));
        }
        let seq = self.next_send;
        self.next_send = seq.checked_add(1).ok_or_else(|| {
            error(
                "nonce_exhausted",
                "Сессия исчерпала номера сообщений; подключитесь заново",
            )
        })?;
        let mut header = vec![VERSION, kind];
        header.extend_from_slice(&seq.to_be_bytes());
        let nonce_bytes = nonce(seq);
        let ciphertext = self
            .sealing
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: payload,
                    aad: &header,
                },
            )
            .map_err(|_| error("encrypt_failed", "Не удалось зашифровать сообщение"))?;
        header.extend(ciphertext);
        Ok(header)
    }
    pub fn decrypt(&mut self, frame: &[u8]) -> Result<(u8, Vec<u8>), ProtocolError> {
        if frame.len() < HEADER_LEN + 16
            || frame.len() > MAX_FRAME_BYTES
            || frame[0] != VERSION
            || !(1..=2).contains(&frame[1])
        {
            return Err(error(
                "invalid_frame",
                "Неподдерживаемый или повреждённый кадр",
            ));
        }
        let mut seq_bytes = [0; 8];
        seq_bytes.copy_from_slice(&frame[2..HEADER_LEN]);
        let seq = u64::from_be_bytes(seq_bytes);
        if seq <= self.last_recv {
            return Err(error("replay", "Повторный или устаревший кадр отклонён"));
        }
        let nonce_bytes = nonce(seq);
        let plaintext = self
            .opening
            .decrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: &frame[HEADER_LEN..],
                    aad: &frame[..HEADER_LEN],
                },
            )
            .map_err(|_| {
                error(
                    "authentication_failed",
                    "Проверка подлинности сообщения не пройдена",
                )
            })?;
        self.last_recv = seq;
        Ok((frame[1], plaintext))
    }
}
fn nonce(seq: u64) -> [u8; 12] {
    let mut nonce = [0; 12];
    nonce[4..].copy_from_slice(&seq.to_be_bytes());
    nonce
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (CryptoCtx, CryptoCtx) {
        let a = KeyExchange::new().unwrap();
        let b = KeyExchange::new().unwrap();
        let ap = a.public_key();
        let bp = b.public_key();
        (
            a.finish(bp, &[7; 32], true).unwrap(),
            b.finish(ap, &[7; 32], false).unwrap(),
        )
    }
    #[test]
    fn peers_agree_on_sas_and_opposite_direction_keys() {
        let (mut a, mut b) = pair();
        assert_eq!(a.sas, b.sas);
        assert_eq!(a.sas.len(), 12);
        let frame = a.encrypt(2, b"hello").unwrap();
        assert_eq!(b.decrypt(&frame).unwrap(), (2, b"hello".to_vec()));
        assert_eq!(
            a.decrypt(&b.encrypt(1, b"ready").unwrap()).unwrap(),
            (1, b"ready".to_vec())
        );
    }
    #[test]
    fn invalid_public_key_returns_error_without_panicking() {
        assert_eq!(
            KeyExchange::new()
                .unwrap()
                .finish([0; 32], &[7; 32], true)
                .err()
                .unwrap()
                .code,
            "invalid_key"
        );
        let key = KeyExchange::new().unwrap();
        let public = key.public_key();
        assert!(key.finish(public, &[7; 32], true).is_err());
    }
    #[test]
    fn ciphertext_kind_and_sequence_are_authenticated_without_consuming_receive_state() {
        let (mut a, mut b) = pair();
        let frame = a.encrypt(2, b"message").unwrap();
        for index in [1, 9, 10, frame.len() - 1] {
            let mut bad = frame.clone();
            bad[index] ^= if index == 1 { 3 } else { 1 };
            assert!(b.decrypt(&bad).is_err());
        }
        assert_eq!(b.decrypt(&frame).unwrap().1, b"message");
        assert_eq!(b.decrypt(&frame).unwrap_err().code, "replay");
    }
    #[test]
    fn lost_send_does_not_desynchronize_following_frame() {
        let (mut a, mut b) = pair();
        let _failed_send = a.encrypt(2, b"lost").unwrap();
        assert_eq!(
            b.decrypt(&a.encrypt(2, b"next").unwrap()).unwrap().1,
            b"next"
        );
    }
    #[test]
    fn sequence_overflow_is_terminal_without_wrapping() {
        let (mut a, _) = pair();
        a.next_send = u64::MAX;
        assert_eq!(a.encrypt(2, b"x").unwrap_err().code, "nonce_exhausted");
        assert_eq!(a.next_send, u64::MAX);
    }
    #[test]
    fn transcript_changes_derived_keys_and_sas() {
        let private = StaticSecret::from([9; 32]);
        let public = PublicKey::from(&private).to_bytes();
        let first = KeyExchange { private, public };
        let second = KeyExchange {
            private: StaticSecret::from([9; 32]),
            public,
        };
        let peer = KeyExchange::new().unwrap().public_key();
        let mut x = first.finish(peer, &[1; 32], true).unwrap();
        let mut y = second.finish(peer, &[2; 32], true).unwrap();
        assert_ne!(x.sas, y.sas);
        assert_ne!(
            x.encrypt(2, b"same").unwrap(),
            y.encrypt(2, b"same").unwrap()
        );
    }
}
