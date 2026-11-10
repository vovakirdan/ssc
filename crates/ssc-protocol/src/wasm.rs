use crate::{decode_event, invitation, FileHasher as CoreFileHasher, ProtocolError, Session};
use js_sys::{Array, Object, Reflect, Uint8Array};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;
use zeroize::Zeroizing;

fn js_error(error: ProtocolError) -> JsValue {
    js_sys::Error::new(&error.code).into()
}
fn input<const N: usize>(bytes: &[u8]) -> Result<[u8; N], JsValue> {
    bytes
        .try_into()
        .map_err(|_| js_sys::Error::new("invalid_entropy_length").into())
}
fn frames(value: Vec<Vec<u8>>) -> Array {
    value
        .into_iter()
        .map(|v| Uint8Array::from(v.as_slice()))
        .collect()
}
fn set(object: &Object, key: &str, value: &JsValue) -> Result<(), JsValue> {
    Reflect::set(object, &key.into(), value).map(|_| ())
}

#[wasm_bindgen]
pub struct ProtocolSession {
    inner: Session,
}
#[wasm_bindgen]
impl ProtocolSession {
    #[wasm_bindgen(constructor)]
    pub fn new(
        session_id: &str,
        initiator: bool,
        seed: Vec<u8>,
        nonce: Vec<u8>,
    ) -> Result<ProtocolSession, JsValue> {
        // Copies received from the JS boundary are wiped once the owned key state exists.
        let seed = Zeroizing::new(seed);
        let nonce = Zeroizing::new(nonce);
        let seed_array = Zeroizing::new(input::<32>(&seed)?);
        let nonce_array = Zeroizing::new(input::<32>(&nonce)?);
        Ok(Self {
            inner: Session::new(session_id, initiator, *seed_array, *nonce_array)
                .map_err(js_error)?,
        })
    }
    pub fn start(&mut self) -> Result<Array, JsValue> {
        self.inner.start().map(frames).map_err(js_error)
    }
    pub fn receive(&mut self, frame: &[u8]) -> Result<JsValue, JsValue> {
        let event = self.inner.receive(frame).map_err(js_error)?;
        let object = Object::new();
        set(&object, "outgoing", &frames(event.outgoing))?;
        set(
            &object,
            "sas",
            &event.sas.map(JsValue::from).unwrap_or(JsValue::NULL),
        )?;
        set(&object, "peer_ready", &event.peer_ready.into())?;
        set(
            &object,
            "message",
            &event.message.map(JsValue::from).unwrap_or(JsValue::NULL),
        )?;
        let typed = event
            .event
            .as_ref()
            .map(|event| {
                let json = serde_json::to_string(event)
                    .map_err(|_| js_sys::Error::new("invalid_event"))?;
                js_sys::JSON::parse(&json)
            })
            .transpose()?
            .unwrap_or(JsValue::NULL);
        set(&object, "event", &typed)?;
        Ok(object.into())
    }
    pub fn sas(&self) -> Option<String> {
        self.inner.sas().map(str::to_owned)
    }
    pub fn peer_ready(&self) -> bool {
        self.inner.peer_ready()
    }
    pub fn local_ready(&self) -> bool {
        self.inner.local_ready()
    }
    pub fn confirm(&mut self) -> Result<Uint8Array, JsValue> {
        self.inner
            .confirm()
            .map(|v| Uint8Array::from(v.as_slice()))
            .map_err(js_error)
    }
    pub fn send_text(&mut self, text: &str, message_id: &[u8]) -> Result<Array, JsValue> {
        self.inner
            .send_text(text, input::<16>(message_id)?)
            .map(frames)
            .map_err(js_error)
    }
    pub fn send_event(&mut self, json: &str) -> Result<Uint8Array, JsValue> {
        let event = decode_event(json.as_bytes()).map_err(js_error)?;
        self.inner
            .send_event(event)
            .map(|frame| Uint8Array::from(frame.as_slice()))
            .map_err(js_error)
    }
    pub fn close(&mut self) {
        self.inner.close();
    }
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserCandidate {
    candidate: String,
    sdp_mid: Option<String>,
    sdp_m_line_index: Option<u16>,
}
#[wasm_bindgen]
pub fn encode_invitation(
    session_id: &str,
    sdp_type: &str,
    sdp: &str,
    candidates_json: &str,
) -> Result<String, JsValue> {
    if sdp.len() > invitation::MAX_DECOMPRESSED_BYTES
        || candidates_json.len() > invitation::MAX_DECOMPRESSED_BYTES
    {
        return Err(js_sys::Error::new("invitation_too_large").into());
    }
    let kind = match sdp_type {
        "offer" => invitation::SdpType::Offer,
        "answer" => invitation::SdpType::Answer,
        _ => return Err(js_sys::Error::new("invalid_sdp_type").into()),
    };
    let candidates: Vec<BrowserCandidate> = serde_json::from_str(candidates_json)
        .map_err(|_| js_sys::Error::new("invalid_candidates"))?;
    let bundle = invitation::ConnectionBundle {
        sdp_payload: invitation::SdpPayload {
            sdp: invitation::SessionDescription {
                sdp_type: kind,
                sdp: sdp.to_owned(),
            },
            id: session_id.to_owned(),
            ts: (js_sys::Date::now() / 1000.0) as i64,
        },
        ice_candidates: candidates
            .into_iter()
            .map(|c| invitation::IceCandidate {
                candidate: c.candidate,
                sdp_mid: c.sdp_mid,
                sdp_mline_index: c.sdp_m_line_index,
                connection_id: session_id.to_owned(),
            })
            .collect(),
    };
    // Encode uses the same validation as receiving, including candidate/session bounds.
    invitation::enc_bundle(&bundle).map_err(js_error)
}
#[wasm_bindgen]
pub fn decode_invitation(encoded: &str) -> Result<JsValue, JsValue> {
    let bundle = invitation::dec_bundle(encoded).map_err(js_error)?;
    let object = Object::new();
    set(&object, "session_id", &bundle.sdp_payload.id.into())?;
    set(
        &object,
        "sdp_type",
        &match bundle.sdp_payload.sdp.sdp_type {
            invitation::SdpType::Offer => "offer",
            _ => "answer",
        }
        .into(),
    )?;
    set(&object, "sdp", &bundle.sdp_payload.sdp.sdp.into())?;
    let candidates = Array::new();
    for c in bundle.ice_candidates {
        let v = Object::new();
        set(&v, "candidate", &c.candidate.into())?;
        set(
            &v,
            "sdpMid",
            &c.sdp_mid.map(JsValue::from).unwrap_or(JsValue::NULL),
        )?;
        set(
            &v,
            "sdpMLineIndex",
            &c.sdp_mline_index
                .map(JsValue::from)
                .unwrap_or(JsValue::NULL),
        )?;
        candidates.push(&v);
    }
    set(&object, "ice_candidates", &candidates)?;
    Ok(object.into())
}

#[wasm_bindgen]
pub struct FileHasher {
    inner: CoreFileHasher,
}
#[wasm_bindgen]
impl FileHasher {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: CoreFileHasher::new(),
        }
    }
    pub fn update(&mut self, data: &[u8]) -> Result<(), JsValue> {
        self.inner.update(data).map_err(js_error)
    }
    pub fn finish(&mut self) -> Result<String, JsValue> {
        self.inner.finish().map_err(js_error)
    }
    pub fn abort(&mut self) {
        self.inner.abort();
    }
}
impl Default for FileHasher {
    fn default() -> Self {
        Self::new()
    }
}
