use crate::peer::state::{MY_PRIV, MY_PUB};
use crate::peer::types::{
    CompactConnectionBundle, CompactIceCandidate, CompactSdp, CompactSdpPayload, ConnectionBundle,
    IceCandidate, SdpPayload,
};
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::{KeyInit, Nonce},
    ChaCha20Poly1305, Key,
};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use hkdf::Hkdf;
use ring::agreement;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use zeroize::{Zeroize, ZeroizeOnDrop};
use webrtc::peer_connection::sdp::sdp_type::RTCSdpType;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;

/// Безопасная обёртка для ключа с автоматической очисткой памяти
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct ZeroizedKey {
    pub key: [u8; 32],
}

impl ZeroizedKey {
    fn new(key: [u8; 32]) -> Self {
        Self { key }
    }
}

/// Контекст шифрования
pub struct CryptoCtx {
    pub sealing: ChaCha20Poly1305,
    pub opening: ChaCha20Poly1305,
    pub send_n: u64,
    pub recv_n: u64,
    pub last_accepted_recv: u64, // Защита от replay - последний принятый recv sequence number
    pub sas: String,
    // Храним ключи в безопасной обёртке для возможности очистки
    pub _send_key: ZeroizedKey,
    pub _recv_key: ZeroizedKey,
}

impl Drop for CryptoCtx {
    fn drop(&mut self) {
        // Зануляем все чувствительные данные
        self.send_n.zeroize();
        self.recv_n.zeroize();
        self.last_accepted_recv.zeroize();
        self.sas.zeroize();
        // ZeroizedKey автоматически очистится благодаря ZeroizeOnDrop
    }
}

/// Создаём контекст шифрования
pub fn build_ctx(peer_pub: &[u8; 32]) -> CryptoCtx {
    // ----- свой ключ -----
    let my_priv = MY_PRIV
        .lock()
        .unwrap()
        .take()
        .expect("private key must exist during key-exchange");

    // ----- общий секрет -----
    let peer_pub_key = agreement::UnparsedPublicKey::new(&agreement::X25519, peer_pub);
    let mut shared =
        agreement::agree_ephemeral(my_priv, &peer_pub_key, |secret| secret.to_vec()).unwrap();

    // ----- разделение ключей по направлениям -----
    // Получаем 64 байта из HKDF для двух ключей
    let hk = Hkdf::<Sha256>::new(None, &shared);
    let mut okm = [0u8; 64];
    hk.expand(b"ssc-chat", &mut okm).unwrap();

    // Очищаем shared сразу после использования
    shared.zeroize();

    let (k1, k2) = okm.split_at(32);

    // Получаем собственный публичный ключ из глобальной переменной
    let my_pub = MY_PUB
        .lock()
        .unwrap()
        .expect("my_pub must be set before building crypto context");

    // Детерминированно выбираем ключи на основе публичных ключей
    let (send_key_slice, recv_key_slice) = if my_pub < *peer_pub {
        (k1, k2)
    } else {
        (k2, k1)
    };

    // Копируем ключи в массивы
    let mut send_key = [0u8; 32];
    let mut recv_key = [0u8; 32];
    send_key.copy_from_slice(send_key_slice);
    recv_key.copy_from_slice(recv_key_slice);

    // ----- SAS на основе первого ключа -----
    let fp_raw = Sha256::digest(k1);
    let sas = hex::encode(&fp_raw[..6]); // PARANOID mode: 48 bits (6 bytes = 12 hex chars)

    // Очищаем okm после использования
    okm.zeroize();

    let sealing = ChaCha20Poly1305::new(&Key::from(send_key));
    let opening = ChaCha20Poly1305::new(&Key::from(recv_key));

    // Создаём безопасные обёртки для ключей
    let send_key_wrapped = ZeroizedKey::new(send_key);
    let recv_key_wrapped = ZeroizedKey::new(recv_key);

    // Очищаем временные копии ключей
    send_key.zeroize();
    recv_key.zeroize();

    CryptoCtx {
        sealing,
        opening,
        send_n: 1,
        recv_n: 1,
        last_accepted_recv: 0, // Начинаем с 0, первое сообщение будет иметь sequence = 1
        sas: sas,
        _send_key: send_key_wrapped,
        _recv_key: recv_key_wrapped,
    }
}

/// Преобразование u64 в nonce
pub fn u64_to_nonce(v: u64) -> Nonce<ChaCha20Poly1305> {
    let mut b = [0u8; 12];
    b[4..].copy_from_slice(&v.to_be_bytes());
    *Nonce::<ChaCha20Poly1305>::from_slice(&b)
}

pub fn enc(p: &SdpPayload) -> String {
    // Пытаемся использовать компактное кодирование; если не удалось — используем легаси
    if let Some(compact) = sdp_to_compact_payload(p) {
        // Префиксируем, чтобы декодер мог определить формат
        let encoded = encode_json_gzip_base64(&compact);
        return format!("c:{}", encoded);
    }

    // Легаси путь: JSON -> gzip -> base64 без префикса
    let encoded = encode_json_gzip_base64(p);
    encoded
}

pub fn dec(s: &str) -> SdpPayload {
    if let Some(rest) = s.strip_prefix("c:") {
        // Компактная форма
        let compact: CompactSdpPayload = decode_base64_gzip_json(rest);
        return compact_to_sdp_payload(&compact);
    }

    // Легаси без префикса
    decode_base64_gzip_json::<SdpPayload>(s)
}

pub fn dec_bundle(s: &str) -> ConnectionBundle {
    if let Some(rest) = s.strip_prefix("cb:") {
        // Компактный бандл
        let compact: CompactConnectionBundle = decode_base64_gzip_json(rest);
        return compact_bundle_to_full(&compact);
    }

    // Легаси без префикса
    decode_base64_gzip_json::<ConnectionBundle>(s)
}

/// Кодирует ConnectionBundle в компактном виде с префиксом
pub fn enc_bundle(bundle: &ConnectionBundle) -> String {
    let compact = bundle_to_compact(bundle);
    let encoded = encode_json_gzip_base64(&compact);
    format!("cb:{}", encoded)
}

// ===== Низкоуровневые утилиты кодирования =====

fn encode_json_gzip_base64<T: serde::Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).unwrap();
    let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
    gz.write_all(&json).unwrap();
    let compressed = gz.finish().unwrap();
    general_purpose::STANDARD.encode(compressed)
}

fn decode_base64_gzip_json<T: for<'de> serde::Deserialize<'de>>(s: &str) -> T {
    let compressed = general_purpose::STANDARD.decode(s).unwrap();
    let gz = GzDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    const MAX_DECOMPRESSED_SIZE: u64 = 256 * 1024; // 256 KiB
    let mut limited_reader = gz.take(MAX_DECOMPRESSED_SIZE);
    limited_reader.read_to_end(&mut json).unwrap();
    serde_json::from_slice(&json).unwrap()
}

// ===== Преобразование SDP <-> CompactSdp =====

fn sdp_to_compact_payload(p: &SdpPayload) -> Option<CompactSdpPayload> {
    sdp_to_compact(&p.sdp).map(|s| CompactSdpPayload { s, id: p.id.clone(), ts: p.ts })
}

fn sdp_to_compact(sdp: &RTCSessionDescription) -> Option<CompactSdp> {
    let sdp_text = sdp.sdp.clone();

    // Извлекаем переменные значения
    let uf = find_attr_value(&sdp_text, "a=ice-ufrag:")?;
    let up = find_attr_value(&sdp_text, "a=ice-pwd:")?;

    let fp_line = find_attr_value(&sdp_text, "a=fingerprint:")?;
    // Формат обычно: sha-256 AA:BB:... Превращаем в ABCD... без двоеточий
    let fp = fp_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .replace(':', "")
        .to_uppercase();

    let setup = find_attr_value(&sdp_text, "a=setup:")?;
    let sr = match setup.as_str() {
        "actpass" => 0u8,
        "active" => 1u8,
        "passive" => 2u8,
        _ => 0u8,
    };

    // MID и SCTP порт
    let mi = find_attr_value(&sdp_text, "a=mid:").unwrap_or_else(|| "0".to_string());
    let sp = if let Some(v) = find_attr_value(&sdp_text, "a=sctp-port:") {
        v.parse::<u16>().ok()
    } else if let Some(v) = find_attr_value(&sdp_text, "a=sctpmap:") {
        // формат: 5000 webrtc-datachannel 1024
        v.split_whitespace().next().and_then(|n| n.parse::<u16>().ok())
    } else {
        None
    };
    let sp = sp.unwrap_or(5000);

    let t = match sdp.sdp_type {
        RTCSdpType::Offer => 0u8,
        RTCSdpType::Answer => 1u8,
        _ => 0u8,
    };

    Some(CompactSdp { t, uf, up, fp, sr, mi, sp })
}

fn compact_to_sdp_payload(c: &CompactSdpPayload) -> SdpPayload {
    let sdp = compact_to_sdp(&c.s);
    SdpPayload { sdp, id: c.id.clone(), ts: c.ts }
}

fn compact_to_sdp(c: &CompactSdp) -> RTCSessionDescription {
    // Восстанавливаем fingerprint в формат с двоеточиями
    let fp_colon = c
        .fp
        .as_bytes()
        .chunks(2)
        .map(|ch| std::str::from_utf8(ch).unwrap())
        .collect::<Vec<_>>()
        .join(":");

    let setup = match c.sr {
        0 => "actpass",
        1 => "active",
        2 => "passive",
        _ => "actpass",
    };

    // Консервативный шаблон SDP для data-channel (SCTP)
    let sdp_text = format!(
        concat!(
            "v=0\r\n",
            "o=- 0 0 IN IP4 127.0.0.1\r\n",
            "s=-\r\n",
            "t=0 0\r\n",
            "a=group:BUNDLE {mid}\r\n",
            "a=msid-semantic: WMS\r\n",
            "m=application 9 UDP/DTLS/SCTP webrtc-datachannel\r\n",
            "c=IN IP4 0.0.0.0\r\n",
            "a=ice-ufrag:{uf}\r\n",
            "a=ice-pwd:{up}\r\n",
            "a=ice-options:trickle\r\n",
            "a=fingerprint:sha-256 {fp}\r\n",
            "a=setup:{setup}\r\n",
            "a=mid:{mid}\r\n",
            "a=sctp-port:{port}\r\n",
            "a=max-message-size:262144\r\n"
        ),
        uf = c.uf,
        up = c.up,
        fp = fp_colon,
        setup = setup,
        mid = c.mi,
        port = c.sp,
    );

    let sdp_type = match c.t { 0 => "offer", 1 => "answer", _ => "offer" };

    // Создаем RTCSessionDescription через serde, так как поля приватные
    let sdp_json = format!(
        "{{\"type\":{typ},\"sdp\":{sdp}}}",
        typ = serde_json::to_string(sdp_type).unwrap(),
        sdp = serde_json::to_string(&sdp_text).unwrap()
    );
    serde_json::from_str::<RTCSessionDescription>(&sdp_json).unwrap()
}

fn find_attr_value(sdp: &str, prefix: &str) -> Option<String> {
    sdp.lines().find_map(|l| l.strip_prefix(prefix).map(|v| v.trim().to_string()))
}

// ===== Bundle преобразования =====

fn bundle_to_compact(b: &ConnectionBundle) -> CompactConnectionBundle {
    let s = sdp_to_compact_payload(&b.sdp_payload)
        .unwrap_or_else(|| CompactSdpPayload {
            // Если не удалось, пытаемся минимум заполнить обязательные поля
            s: CompactSdp {
                t: match b.sdp_payload.sdp.sdp_type { RTCSdpType::Offer => 0, RTCSdpType::Answer => 1, _ => 0 },
                uf: find_attr_value(&b.sdp_payload.sdp.sdp, "a=ice-ufrag:").unwrap_or_default(),
                up: find_attr_value(&b.sdp_payload.sdp.sdp, "a=ice-pwd:").unwrap_or_default(),
                fp: find_attr_value(&b.sdp_payload.sdp.sdp, "a=fingerprint:")
                    .map(|v| v.split_whitespace().nth(1).unwrap_or("").replace(':', "").to_uppercase())
                    .unwrap_or_default(),
                sr: 0,
                mi: find_attr_value(&b.sdp_payload.sdp.sdp, "a=mid:").unwrap_or_else(|| "0".into()),
                sp: find_attr_value(&b.sdp_payload.sdp.sdp, "a=sctp-port:")
                    .and_then(|v| v.parse::<u16>().ok())
                    .unwrap_or(5000),
            },
            id: b.sdp_payload.id.clone(),
            ts: b.sdp_payload.ts,
        });

    let cs = b
        .ice_candidates
        .iter()
        .map(|c| CompactIceCandidate { c: c.candidate.clone(), md: c.sdp_mid.clone(), ml: c.sdp_mline_index, i: c.connection_id.clone() })
        .collect();

    CompactConnectionBundle { s, cs }
}

fn compact_bundle_to_full(c: &CompactConnectionBundle) -> ConnectionBundle {
    let sdp_payload = compact_to_sdp_payload(&c.s);
    let ice_candidates = c
        .cs
        .iter()
        .map(|c| IceCandidate { candidate: c.c.clone(), sdp_mid: c.md.clone(), sdp_mline_index: c.ml, connection_id: c.i.clone() })
        .collect();
    ConnectionBundle { sdp_payload, ice_candidates }
}
