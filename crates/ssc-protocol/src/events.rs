//! Strict, authenticated chat controls. Attachment storage/consent belongs to the adapter.
use crate::{message::MAX_MESSAGE_BYTES, ProtocolError};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Deserializer, Serialize};

pub const KIND_EVENT: u8 = 4;
pub const MAX_ATTACHMENT_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_ATTACHMENT_NAME_BYTES: usize = 1024;
pub const ATTACHMENT_CHUNK_BYTES: usize = 16 * 1024;
/// Includes the ten-byte authenticated header and sixteen-byte AEAD tag.
pub const MAX_EVENT_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_EVENT_JSON_BYTES: usize = MAX_EVENT_FRAME_BYTES - 26;
pub const AUDIO_MIMES: [&str; 3] = ["audio/webm;codecs=opus", "audio/mp4", "audio/wav"];

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttachmentKind {
    Photo,
    Video,
    Audio,
    File,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ByeReason {
    Exit,
    Expired,
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ChatEvent {
    Text {
        id: String,
        text: String,
    },
    AttachmentOffer {
        id: String,
        name: String,
        mime: String,
        size: u64,
        sha256: String,
        kind: AttachmentKind,
    },
    AttachmentAccept {
        id: String,
    },
    AttachmentReject {
        id: String,
    },
    AttachmentChunk {
        id: String,
        offset: u64,
        data: String,
    },
    AttachmentProgress {
        id: String,
        received: u64,
    },
    AttachmentComplete {
        id: String,
    },
    AttachmentCancel {
        id: String,
    },
    Receipt {
        id: String,
    },
    Reaction {
        target: String,
        #[serde(deserialize_with = "required_nullable_string")]
        emoji: Option<String>,
    },
    Bye {
        reason: ByeReason,
    },
    Capabilities {
        audio: Vec<String>,
    },
}
// Debug output must not accidentally expose messages, filenames, IDs or chunks.
impl std::fmt::Debug for ChatEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Text { .. } => "ChatEvent::Text(..)",
            Self::AttachmentOffer { .. } => "ChatEvent::AttachmentOffer(..)",
            Self::AttachmentAccept { .. } => "ChatEvent::AttachmentAccept(..)",
            Self::AttachmentReject { .. } => "ChatEvent::AttachmentReject(..)",
            Self::AttachmentChunk { .. } => "ChatEvent::AttachmentChunk(..)",
            Self::AttachmentProgress { .. } => "ChatEvent::AttachmentProgress(..)",
            Self::AttachmentComplete { .. } => "ChatEvent::AttachmentComplete(..)",
            Self::AttachmentCancel { .. } => "ChatEvent::AttachmentCancel(..)",
            Self::Receipt { .. } => "ChatEvent::Receipt(..)",
            Self::Reaction { .. } => "ChatEvent::Reaction(..)",
            Self::Bye { .. } => "ChatEvent::Bye(..)",
            Self::Capabilities { .. } => "ChatEvent::Capabilities(..)",
        })
    }
}
fn required_nullable_string<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}
fn invalid() -> ProtocolError {
    ProtocolError::new(
        "invalid_event",
        "message",
        "Недопустимое событие чата",
        false,
    )
}
pub(crate) fn valid_hex(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
pub(crate) fn valid_id(value: &str) -> bool {
    valid_hex(value, 32)
}
impl ChatEvent {
    /// Checks semantic limits after deserialization and before encryption.
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let id = match self {
            Self::Text { id, .. }
            | Self::AttachmentOffer { id, .. }
            | Self::AttachmentAccept { id }
            | Self::AttachmentReject { id }
            | Self::AttachmentChunk { id, .. }
            | Self::AttachmentProgress { id, .. }
            | Self::AttachmentComplete { id }
            | Self::AttachmentCancel { id }
            | Self::Receipt { id } => Some(id),
            Self::Reaction { target, .. } => Some(target),
            _ => None,
        };
        if id.is_some_and(|id| !valid_id(id)) {
            return Err(invalid());
        }
        match self {
            Self::Text { text, .. } if text.is_empty() || text.len() > MAX_MESSAGE_BYTES => {
                return Err(invalid())
            }
            Self::AttachmentOffer {
                name,
                mime,
                size,
                sha256,
                ..
            } => {
                if name.is_empty()
                    || name.len() > MAX_ATTACHMENT_NAME_BYTES
                    || matches!(name.as_str(), "." | "..")
                    || name.contains(['/', '\\'])
                    || name.chars().any(char::is_control)
                    || mime.is_empty()
                    || mime.len() > 128
                    || !mime.is_ascii()
                    || mime.bytes().any(|b| b.is_ascii_control())
                    || *size > MAX_ATTACHMENT_BYTES
                    || !valid_hex(sha256, 64)
                {
                    return Err(invalid());
                }
            }
            Self::AttachmentChunk { offset, data, .. } => {
                if data.is_empty() || data.len() > ATTACHMENT_CHUNK_BYTES.div_ceil(3) * 4 {
                    return Err(invalid());
                }
                let decoded =
                    zeroize::Zeroizing::new(STANDARD.decode(data).map_err(|_| invalid())?);
                if decoded.is_empty()
                    || decoded.len() > ATTACHMENT_CHUNK_BYTES
                    || STANDARD.encode(&decoded) != *data
                    || offset
                        .checked_add(decoded.len() as u64)
                        .is_none_or(|end| end > MAX_ATTACHMENT_BYTES)
                {
                    return Err(invalid());
                }
            }
            Self::AttachmentProgress { received, .. } if *received > MAX_ATTACHMENT_BYTES => {
                return Err(invalid())
            }
            Self::Reaction {
                emoji: Some(emoji), ..
            } if !["👍", "❤️", "😂", "😮", "😢", "🔥"].contains(&emoji.as_str()) => {
                return Err(invalid())
            }
            Self::Capabilities { audio }
                if audio.len() > 3
                    || audio
                        .iter()
                        .any(|mime| !AUDIO_MIMES.contains(&mime.as_str()))
                    || audio
                        .iter()
                        .enumerate()
                        .any(|(index, mime)| audio[..index].contains(mime)) =>
            {
                return Err(invalid())
            }
            _ => {}
        }
        Ok(())
    }
}
/// Encodes one bounded kind-4 payload. Large text uses the existing chunk codec.
pub fn encode_event(event: &ChatEvent) -> Result<Vec<u8>, ProtocolError> {
    event.validate()?;
    let bytes = serde_json::to_vec(event).map_err(|_| invalid())?;
    if bytes.len() > MAX_EVENT_JSON_BYTES {
        return Err(ProtocolError::new(
            "event_size",
            "message",
            "Событие чата слишком большое",
            false,
        ));
    }
    Ok(bytes)
}
/// Rejects unknown fields/types and malformed or out-of-bounds event payloads.
pub fn decode_event(payload: &[u8]) -> Result<ChatEvent, ProtocolError> {
    if payload.len() > MAX_EVENT_JSON_BYTES {
        return Err(ProtocolError::new(
            "event_size",
            "message",
            "Событие чата слишком большое",
            false,
        ));
    }
    let event: ChatEvent = serde_json::from_slice(payload).map_err(|_| invalid())?;
    event.validate()?;
    Ok(event)
}
