use crate::ProtocolError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
pub const MAX_INVITATION_BYTES: usize = 512 * 1024;
pub const MAX_DECOMPRESSED_BYTES: usize = 256 * 1024;
fn invitation_error(code: &str, message: &str) -> ProtocolError {
    ProtocolError::new(code, "invitation", message, true)
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SdpType {
    Offer,
    Answer,
    #[serde(other)]
    Unspecified,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionDescription {
    #[serde(rename = "type")]
    pub sdp_type: SdpType,
    pub sdp: String,
}
impl SessionDescription {
    pub fn offer(sdp: String) -> Result<Self, ProtocolError> {
        Ok(Self {
            sdp_type: SdpType::Offer,
            sdp,
        })
    }
    pub fn answer(sdp: String) -> Result<Self, ProtocolError> {
        Ok(Self {
            sdp_type: SdpType::Answer,
            sdp,
        })
    }
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SdpPayload {
    pub sdp: SessionDescription,
    pub id: String,
    pub ts: i64,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_mline_index: Option<u16>,
    pub connection_id: String,
}
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConnectionBundle {
    pub sdp_payload: SdpPayload,
    pub ice_candidates: Vec<IceCandidate>,
}
/// Компактная SDP (только переменные значения)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactSdp {
    /// Тип описания: 0 = offer, 1 = answer
    pub t: u8,
    /// ICE ufrag
    pub uf: String,
    /// ICE pwd
    pub up: String,
    /// DTLS fingerprint без двоеточий (hex в uppercase)
    pub fp: String,
    /// DTLS setup role: 0 = actpass, 1 = active, 2 = passive
    pub sr: u8,
    /// MID для m=application
    pub mi: String,
    /// SCTP порт (обычно 5000)
    pub sp: u16,
}

/// Компактная обёртка SDP с метаданными (ID/TS)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactSdpPayload {
    pub s: CompactSdp,
    pub id: String,
    pub ts: i64,
}

/// Компактный ICE кандидат с короткими ключами
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactIceCandidate {
    /// Полная строка candidate, но ключ короче
    pub c: String,
    /// sdp_mid
    pub md: Option<String>,
    /// sdp_mline_index
    pub ml: Option<u16>,
    /// ID соединения
    pub i: String,
}

/// Компактный пакет соединения (SDP + кандидаты)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CompactConnectionBundle {
    pub s: CompactSdpPayload,
    pub cs: Vec<CompactIceCandidate>,
}

pub fn enc_bundle(bundle: &ConnectionBundle) -> Result<String, ProtocolError> {
    let compact = bundle_to_compact(bundle)?;
    // Reject locally produced bundles that a receiving peer would reject.
    compact_bundle_to_full(&compact)?;
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
    let encoded = format!("v4:{}", STANDARD.encode(bytes));
    if encoded.len() > MAX_INVITATION_BYTES {
        return Err(invitation_error(
            "invitation_too_large",
            "Приглашение слишком большое",
        ));
    }
    Ok(encoded)
}
pub fn dec_bundle(encoded: &str) -> Result<ConnectionBundle, ProtocolError> {
    if encoded.len() > MAX_INVITATION_BYTES {
        return Err(invitation_error(
            "invitation_too_large",
            "Приглашение слишком большое",
        ));
    }
    let data = encoded.trim().strip_prefix("v4:").ok_or_else(|| {
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
fn sdp_to_compact(sdp: &SessionDescription) -> Result<CompactSdp, ProtocolError> {
    let get = |prefix| {
        find_attr_value(&sdp.sdp, prefix).ok_or_else(|| {
            invitation_error(
                "invalid_sdp",
                "В приглашении отсутствуют параметры соединения",
            )
        })
    };
    let t = match sdp.sdp_type {
        SdpType::Offer => 0,
        SdpType::Answer => 1,
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
fn validate_compact(c: &CompactSdp) -> Result<(), ProtocolError> {
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
fn compact_to_sdp(c: &CompactSdp) -> Result<SessionDescription, ProtocolError> {
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
        SessionDescription::offer(text)
    } else {
        SessionDescription::answer(text)
    };
    result.map_err(|_| invitation_error("invalid_sdp", "Не удалось разобрать параметры соединения"))
}
fn bundle_to_compact(b: &ConnectionBundle) -> Result<CompactConnectionBundle, ProtocolError> {
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
fn compact_bundle_to_full(c: &CompactConnectionBundle) -> Result<ConnectionBundle, ProtocolError> {
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
        for input in ["v4:!", "v4:eA==", "v4:", "v2:eA=="] {
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
            dec_bundle(&format!("v4:{}", STANDARD.encode(gz.finish().unwrap())))
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
