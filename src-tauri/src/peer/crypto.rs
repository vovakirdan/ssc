use crate::errors::AppError;
use crate::peer::types::{
    CompactConnectionBundle, CompactIceCandidate, CompactSdp, CompactSdpPayload, ConnectionBundle,
    IceCandidate, SdpPayload,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use hkdf::Hkdf;
use ring::{agreement, rand::SystemRandom};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use webrtc::peer_connection::sdp::{
    sdp_type::RTCSdpType, session_description::RTCSessionDescription,
};
use zeroize::{Zeroize, Zeroizing};

pub const VERSION: u8 = 2;
pub const MAX_INVITATION_BYTES: usize = 512 * 1024;
pub const MAX_DECOMPRESSED_BYTES: usize = 256 * 1024;
const HEADER_LEN: usize = 10;
const MAX_FRAME_BYTES: usize = 70 * 1024;

fn error(code: &str, message: &str) -> AppError {
    AppError::new(code, "crypto", message, false)
}
fn invitation_error(code: &str, message: &str) -> AppError {
    AppError::new(code, "invitation", message, true)
}

pub struct KeyExchange {
    private: agreement::EphemeralPrivateKey,
    public: [u8; 32],
}
impl KeyExchange {
    pub fn new() -> Result<Self, AppError> {
        let private =
            agreement::EphemeralPrivateKey::generate(&agreement::X25519, &SystemRandom::new())
                .map_err(|_| error("key_generation", "Не удалось создать ключ сессии"))?;
        let key = private
            .compute_public_key()
            .map_err(|_| error("key_generation", "Не удалось получить публичный ключ"))?;
        let mut public = [0; 32];
        public.copy_from_slice(key.as_ref());
        Ok(Self { private, public })
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.public
    }
    pub fn finish(self, peer: [u8; 32]) -> Result<CryptoCtx, AppError> {
        if peer == self.public {
            return Err(error(
                "invalid_key",
                "Публичный ключ собеседника недопустим",
            ));
        }
        let key = agreement::UnparsedPublicKey::new(&agreement::X25519, peer);
        let shared = agreement::agree_ephemeral(self.private, &key, |secret| {
            Zeroizing::new(secret.to_vec())
        })
        .map_err(|_| error("invalid_key", "Не удалось согласовать ключ сессии"))?;
        let hk = Hkdf::<Sha256>::new(None, &shared);
        let mut keys = Zeroizing::new([0u8; 64]);
        // Bind the derived keys and SAS to both ephemeral public keys, in a shared order.
        let (lower, upper) = if self.public < peer {
            (self.public, peer)
        } else {
            (peer, self.public)
        };
        let mut info = b"ssc-chat-v2".to_vec();
        info.extend_from_slice(&lower);
        info.extend_from_slice(&upper);
        hk.expand(&info, keys.as_mut())
            .map_err(|_| error("key_derivation", "Не удалось получить ключи шифрования"))?;
        let (k1, k2) = keys.split_at(32);
        let (send, recv) = if self.public < peer {
            (k1, k2)
        } else {
            (k2, k1)
        };
        let sealing = ChaCha20Poly1305::new_from_slice(send)
            .map_err(|_| error("key_derivation", "Некорректный ключ шифрования"))?;
        let opening = ChaCha20Poly1305::new_from_slice(recv)
            .map_err(|_| error("key_derivation", "Некорректный ключ шифрования"))?;
        let sas = hex::encode(&Sha256::digest(k1)[..6]);
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
    pub fn encrypt(&mut self, kind: u8, payload: &[u8]) -> Result<Vec<u8>, AppError> {
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
    pub fn decrypt(&mut self, frame: &[u8]) -> Result<(u8, Vec<u8>), AppError> {
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

pub fn enc_bundle(bundle: &ConnectionBundle) -> Result<String, AppError> {
    let compact = bundle_to_compact(bundle)?;
    let json = serde_json::to_vec(&compact)
        .map_err(|_| invitation_error("invalid_json", "Не удалось создать приглашение"))?;
    if json.len() > MAX_DECOMPRESSED_BYTES {
        return Err(invitation_error(
            "invitation_too_large",
            "Приглашение слишком большое",
        ));
    }
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&json)
        .map_err(|_| invitation_error("compression_failed", "Не удалось упаковать приглашение"))?;
    let bytes = gz
        .finish()
        .map_err(|_| invitation_error("compression_failed", "Не удалось упаковать приглашение"))?;
    let encoded = format!("v2:{}", STANDARD.encode(bytes));
    if encoded.len() > MAX_INVITATION_BYTES {
        return Err(invitation_error(
            "invitation_too_large",
            "Приглашение слишком большое",
        ));
    }
    Ok(encoded)
}
pub fn dec_bundle(encoded: &str) -> Result<ConnectionBundle, AppError> {
    if encoded.len() > MAX_INVITATION_BYTES {
        return Err(invitation_error(
            "invitation_too_large",
            "Приглашение слишком большое",
        ));
    }
    let data = encoded.trim().strip_prefix("v2:").ok_or_else(|| {
        invitation_error(
            "unsupported_version",
            "Версия приглашения не поддерживается; обновите приложение у обоих участников",
        )
    })?;
    let compressed = STANDARD
        .decode(data)
        .map_err(|_| invitation_error("invalid_base64", "Повреждено кодирование приглашения"))?;
    let mut bytes = Vec::new();
    GzDecoder::new(compressed.as_slice())
        .take(MAX_DECOMPRESSED_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invitation_error("invalid_gzip", "Повреждено сжатое приглашение"))?;
    if bytes.len() > MAX_DECOMPRESSED_BYTES {
        return Err(invitation_error(
            "invitation_too_large",
            "Распакованное приглашение слишком большое",
        ));
    }
    let compact: CompactConnectionBundle = serde_json::from_slice(&bytes)
        .map_err(|_| invitation_error("invalid_json", "Некорректный формат приглашения"))?;
    compact_bundle_to_full(&compact)
}

fn find_attr_value(sdp: &str, prefix: &str) -> Option<String> {
    sdp.lines()
        .find_map(|line| line.strip_prefix(prefix).map(|v| v.trim().to_owned()))
}
fn sdp_to_compact(sdp: &RTCSessionDescription) -> Result<CompactSdp, AppError> {
    let get = |prefix| {
        find_attr_value(&sdp.sdp, prefix).ok_or_else(|| {
            invitation_error(
                "invalid_sdp",
                "В приглашении отсутствуют параметры соединения",
            )
        })
    };
    let t = match sdp.sdp_type {
        RTCSdpType::Offer => 0,
        RTCSdpType::Answer => 1,
        _ => {
            return Err(invitation_error(
                "invalid_sdp_type",
                "Ожидается offer или answer",
            ))
        }
    };
    let fp_line = get("a=fingerprint:")?;
    let mut fingerprint = fp_line.split_whitespace();
    if fingerprint.next() != Some("sha-256") {
        return Err(invitation_error(
            "invalid_sdp",
            "Неподдерживаемый отпечаток соединения",
        ));
    }
    let fp = fingerprint
        .next()
        .unwrap_or_default()
        .replace(':', "")
        .to_uppercase();
    let sr = match get("a=setup:")?.as_str() {
        "actpass" => 0,
        "active" => 1,
        "passive" => 2,
        _ => {
            return Err(invitation_error(
                "invalid_sdp",
                "Некорректная роль соединения",
            ))
        }
    };
    let compact = CompactSdp {
        t,
        uf: get("a=ice-ufrag:")?,
        up: get("a=ice-pwd:")?,
        fp,
        sr,
        mi: get("a=mid:")?,
        sp: get("a=sctp-port:")?
            .parse()
            .map_err(|_| invitation_error("invalid_sdp", "Некорректный порт канала"))?,
    };
    validate_compact(&compact)?;
    Ok(compact)
}
fn validate_compact(c: &CompactSdp) -> Result<(), AppError> {
    if c.t > 1
        || c.sr > 2
        || c.sp == 0
        || c.fp.len() != 64
        || !c.fp.bytes().all(|b| b.is_ascii_hexdigit())
        || [&c.uf, &c.up, &c.mi].iter().any(|v| {
            v.is_empty()
                || v.len() > 256
                || v.bytes()
                    .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        })
    {
        return Err(invitation_error(
            "invalid_sdp",
            "Некорректные параметры соединения",
        ));
    }
    Ok(())
}
fn compact_to_sdp(c: &CompactSdp) -> Result<RTCSessionDescription, AppError> {
    validate_compact(c)?;
    let fp =
        c.fp.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| format!("{}{}", pair[0] as char, pair[1] as char))
            .collect::<Vec<_>>()
            .join(":");
    let setup = match c.sr {
        0 => "actpass",
        1 => "active",
        _ => "passive",
    };
    let text = format!(concat!("v=0\r\no=- 0 0 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n", "a=group:BUNDLE {mid}\r\na=msid-semantic: WMS\r\nm=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\nc=IN IP4 0.0.0.0\r\n", "a=ice-ufrag:{uf}\r\na=ice-pwd:{up}\r\na=ice-options:trickle\r\na=fingerprint:sha-256 {fp}\r\na=setup:{setup}\r\na=mid:{mid}\r\na=sctp-port:{port}\r\na=max-message-size:262144\r\n"), mid=c.mi, uf=c.uf, up=c.up, port=c.sp, fp=fp, setup=setup);
    let result = if c.t == 0 {
        RTCSessionDescription::offer(text)
    } else {
        RTCSessionDescription::answer(text)
    };
    result.map_err(|_| invitation_error("invalid_sdp", "Не удалось разобрать параметры соединения"))
}
fn bundle_to_compact(b: &ConnectionBundle) -> Result<CompactConnectionBundle, AppError> {
    Ok(CompactConnectionBundle {
        s: CompactSdpPayload {
            s: sdp_to_compact(&b.sdp_payload.sdp)?,
            id: b.sdp_payload.id.clone(),
            ts: b.sdp_payload.ts,
        },
        cs: b
            .ice_candidates
            .iter()
            .map(|c| CompactIceCandidate {
                c: c.candidate.clone(),
                md: c.sdp_mid.clone(),
                ml: c.sdp_mline_index,
                i: c.connection_id.clone(),
            })
            .collect(),
    })
}
fn compact_bundle_to_full(c: &CompactConnectionBundle) -> Result<ConnectionBundle, AppError> {
    if c.s.id.is_empty()
        || c.s.id.len() > 128
        || c.cs.len() > 256
        || c.cs
            .iter()
            .any(|ice| ice.i != c.s.id || ice.c.len() > 4096 || ice.c.contains(['\r', '\n']))
    {
        return Err(invitation_error(
            "invalid_invitation",
            "Параметры приглашения не согласованы",
        ));
    }
    Ok(ConnectionBundle {
        sdp_payload: SdpPayload {
            sdp: compact_to_sdp(&c.s.s)?,
            id: c.s.id.clone(),
            ts: c.s.ts,
        },
        ice_candidates: c
            .cs
            .iter()
            .map(|v| IceCandidate {
                candidate: v.c.clone(),
                sdp_mid: v.md.clone(),
                sdp_mline_index: v.ml,
                connection_id: v.i.clone(),
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn pair() -> (CryptoCtx, CryptoCtx) {
        let a = KeyExchange::new().unwrap();
        let b = KeyExchange::new().unwrap();
        let ap = a.public_key();
        let bp = b.public_key();
        (a.finish(bp).unwrap(), b.finish(ap).unwrap())
    }
    fn bundle() -> ConnectionBundle {
        let c = CompactSdp {
            t: 0,
            uf: "abcd".into(),
            up: "abcdefghijklmnopqrstuv".into(),
            fp: "AB".repeat(32),
            sr: 0,
            mi: "0".into(),
            sp: 5000,
        };
        ConnectionBundle {
            sdp_payload: SdpPayload {
                sdp: compact_to_sdp(&c).unwrap(),
                id: "connection1".into(),
                ts: 123,
            },
            ice_candidates: vec![],
        }
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
                .finish([0; 32])
                .err()
                .unwrap()
                .code,
            "invalid_key"
        );
        let key = KeyExchange::new().unwrap();
        let public = key.public_key();
        assert!(key.finish(public).is_err());
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
    fn versioned_invitation_roundtrips_and_rejects_old_version() {
        let source = bundle();
        let encoded = enc_bundle(&source).unwrap();
        let decoded = dec_bundle(&encoded).unwrap();
        assert_eq!(decoded.sdp_payload.id, source.sdp_payload.id);
        assert_eq!(decoded.sdp_payload.sdp.sdp, source.sdp_payload.sdp.sdp);
        assert_eq!(
            dec_bundle("cb:anything").unwrap_err().code,
            "unsupported_version"
        );
    }
    #[test]
    fn malformed_invitation_and_zip_bomb_return_safe_errors() {
        for input in ["v2:!", "v2:eA==", "v2:"] {
            assert!(dec_bundle(input).is_err());
        }
        assert_eq!(
            dec_bundle(&"x".repeat(MAX_INVITATION_BYTES + 1))
                .unwrap_err()
                .code,
            "invitation_too_large"
        );
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        gz.write_all(&vec![b' '; MAX_DECOMPRESSED_BYTES + 1])
            .unwrap();
        assert_eq!(
            dec_bundle(&format!("v2:{}", STANDARD.encode(gz.finish().unwrap())))
                .unwrap_err()
                .code,
            "invitation_too_large"
        );
    }
    #[test]
    fn compact_sdp_cannot_inject_lines_or_invalid_fingerprint() {
        let mut c = bundle_to_compact(&bundle()).unwrap();
        c.s.s.uf = "x\r\na=evil".into();
        assert!(compact_bundle_to_full(&c).is_err());
        c.s.s.uf = "abcd".into();
        c.s.s.fp = "é".repeat(32);
        assert!(compact_bundle_to_full(&c).is_err());
    }
}
