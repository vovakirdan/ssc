//! Native OS entropy and WebRTC adapters for the shared protocol implementation.
use crate::errors::AppError;
use crate::peer::types::{ConnectionBundle, IceCandidate, SdpPayload};
#[cfg(test)]
use ring::rand::{SecureRandom, SystemRandom};
pub use ssc_protocol::crypto::CryptoCtx;
#[cfg(test)]
pub use ssc_protocol::crypto::VERSION;
use ssc_protocol::{invitation as codec, ProtocolError};
use webrtc::peer_connection::sdp::{
    sdp_type::RTCSdpType, session_description::RTCSessionDescription,
};
#[cfg(test)]
use zeroize::Zeroizing;

impl From<ProtocolError> for AppError {
    fn from(error: ProtocolError) -> Self {
        Self::new(&error.code, &error.stage, &error.message, error.retryable)
    }
}

#[cfg(test)]
pub struct KeyExchange(ssc_protocol::crypto::KeyExchange);
#[cfg(test)]
impl KeyExchange {
    pub fn new() -> Result<Self, AppError> {
        let mut seed = Zeroizing::new([0; 32]);
        SystemRandom::new().fill(seed.as_mut()).map_err(|_| {
            AppError::new(
                "key_generation",
                "crypto",
                "Не удалось создать ключ сессии",
                false,
            )
        })?;
        Ok(Self(ssc_protocol::crypto::KeyExchange::from_seed(*seed)))
    }
    pub fn public_key(&self) -> [u8; 32] {
        self.0.public_key()
    }
    pub fn finish(
        self,
        peer: [u8; 32],
        transcript: &[u8; 32],
        initiator: bool,
    ) -> Result<CryptoCtx, AppError> {
        self.0
            .finish(peer, transcript, initiator)
            .map_err(Into::into)
    }
}

pub fn enc_bundle(bundle: &ConnectionBundle) -> Result<String, AppError> {
    let sdp_type = match bundle.sdp_payload.sdp.sdp_type {
        RTCSdpType::Offer => codec::SdpType::Offer,
        RTCSdpType::Answer => codec::SdpType::Answer,
        _ => codec::SdpType::Unspecified,
    };
    codec::enc_bundle(&codec::ConnectionBundle {
        sdp_payload: codec::SdpPayload {
            sdp: codec::SessionDescription {
                sdp_type,
                sdp: bundle.sdp_payload.sdp.sdp.clone(),
            },
            id: bundle.sdp_payload.id.clone(),
            ts: bundle.sdp_payload.ts,
        },
        ice_candidates: bundle
            .ice_candidates
            .iter()
            .map(|c| codec::IceCandidate {
                candidate: c.candidate.clone(),
                sdp_mid: c.sdp_mid.clone(),
                sdp_mline_index: c.sdp_mline_index,
                connection_id: c.connection_id.clone(),
            })
            .collect(),
    })
    .map_err(Into::into)
}

pub fn dec_bundle(encoded: &str) -> Result<ConnectionBundle, AppError> {
    let bundle = codec::dec_bundle(encoded)?;
    let sdp = match bundle.sdp_payload.sdp.sdp_type {
        codec::SdpType::Offer => RTCSessionDescription::offer(bundle.sdp_payload.sdp.sdp),
        _ => RTCSessionDescription::answer(bundle.sdp_payload.sdp.sdp),
    }
    .map_err(|_| {
        AppError::new(
            "invalid_sdp",
            "invitation",
            "Не удалось разобрать параметры соединения",
            true,
        )
    })?;
    Ok(ConnectionBundle {
        sdp_payload: SdpPayload {
            sdp,
            id: bundle.sdp_payload.id,
            ts: bundle.sdp_payload.ts,
        },
        ice_candidates: bundle
            .ice_candidates
            .into_iter()
            .map(|c| IceCandidate {
                candidate: c.candidate,
                sdp_mid: c.sdp_mid,
                sdp_mline_index: c.sdp_mline_index,
                connection_id: c.connection_id,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_adapter_reencodes_legacy_invitation_byte_for_byte() {
        let original="v3:H4sIAAAAAAAE/6WOzQ5AMBCE32XPDou49MZriAOl1E+Llot4d0P6BrLJZna/ZGYuciTC8iQ4okORoLqRLUGvQXeqH/Q4zYux67Y7f5yg6qV58W/g4/YveNEI4/eGb8bMd0S6xU9aYzrptTUxqEfhOEkBJVRZ3Q9J1GFGwwAAAA==";
        let decoded = dec_bundle(original).unwrap();
        assert_eq!(decoded.sdp_payload.id, "connection1");
        assert_eq!(enc_bundle(&decoded).unwrap(), original);
    }
}
