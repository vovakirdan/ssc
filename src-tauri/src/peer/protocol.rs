use crate::errors::AppError;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use rand::random;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

pub const KIND_READY: u8 = 1;
pub const KIND_CHUNK: u8 = 2;
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;
pub const CHUNK_BYTES: usize = 16 * 1024;
const MAX_CHUNKS: usize = MAX_MESSAGE_BYTES / CHUNK_BYTES;
const MAX_CHUNK_JSON_BYTES: usize = CHUNK_BYTES * 2;

fn error(code: &str, message: &str) -> AppError {
    AppError::new(code, "message", message, true)
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Chunk {
    id: String,
    index: usize,
    total: usize,
    data: String,
}

pub fn chunks(text: &str) -> Result<Vec<Vec<u8>>, AppError> {
    if text.is_empty() || text.len() > MAX_MESSAGE_BYTES {
        return Err(error(
            "message_size",
            "Допустимый размер сообщения: от 1 байта до 256 KiB",
        ));
    }
    let id = hex::encode(random::<[u8; 16]>());
    let total = text.len().div_ceil(CHUNK_BYTES);
    text.as_bytes()
        .chunks(CHUNK_BYTES)
        .enumerate()
        .map(|(index, part)| {
            serde_json::to_vec(&Chunk {
                id: id.clone(),
                index,
                total,
                data: STANDARD.encode(part),
            })
            .map_err(|_| error("encode_message", "Не удалось подготовить сообщение"))
        })
        .collect()
}

struct PartialMessage {
    id: String,
    next: usize,
    total: usize,
    bytes: Vec<u8>,
}
impl Drop for PartialMessage {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

#[derive(Default)]
pub struct Reassembler {
    pending: Option<PartialMessage>,
}
impl Reassembler {
    pub fn push(&mut self, payload: &[u8]) -> Result<Option<String>, AppError> {
        let result = self.push_inner(payload);
        if result.is_err() {
            self.pending = None;
        }
        result
    }
    fn push_inner(&mut self, payload: &[u8]) -> Result<Option<String>, AppError> {
        if payload.len() > MAX_CHUNK_JSON_BYTES {
            return Err(error("invalid_chunk", "Часть сообщения слишком большая"));
        }
        let chunk: Chunk = serde_json::from_slice(payload)
            .map_err(|_| error("invalid_chunk", "Повреждена часть сообщения"))?;
        if chunk.id.len() != 32
            || !chunk.id.bytes().all(|b| b.is_ascii_hexdigit())
            || chunk.total == 0
            || chunk.total > MAX_CHUNKS
            || chunk.index >= chunk.total
            || chunk.data.len() > CHUNK_BYTES.div_ceil(3) * 4
        {
            return Err(error(
                "invalid_chunk",
                "Недопустимые параметры части сообщения",
            ));
        }
        let data = STANDARD
            .decode(&chunk.data)
            .map_err(|_| error("invalid_chunk", "Повреждено содержимое части сообщения"))?;
        if data.is_empty()
            || data.len() > CHUNK_BYTES
            || (chunk.index + 1 < chunk.total && data.len() != CHUNK_BYTES)
        {
            return Err(error(
                "invalid_chunk",
                "Недопустимый размер части сообщения",
            ));
        }
        // A failed partial send can be followed by a new message; never display the abandoned prefix.
        if chunk.index == 0 {
            self.pending = Some(PartialMessage {
                id: chunk.id.clone(),
                next: 0,
                total: chunk.total,
                bytes: Vec::new(),
            });
        }
        let pending = self
            .pending
            .as_mut()
            .ok_or_else(|| error("incomplete_message", "Не получено начало сообщения"))?;
        if pending.id != chunk.id
            || pending.next != chunk.index
            || pending.total != chunk.total
            || pending.bytes.len() + data.len() > MAX_MESSAGE_BYTES
        {
            return Err(error(
                "invalid_chunk_order",
                "Нарушен порядок частей сообщения",
            ));
        }
        pending.bytes.extend(data);
        pending.next += 1;
        if pending.next != pending.total {
            return Ok(None);
        }
        let mut complete = self
            .pending
            .take()
            .ok_or_else(|| error("incomplete_message", "Не удалось собрать сообщение"))?;
        String::from_utf8(std::mem::take(&mut complete.bytes))
            .map(Some)
            .map_err(|_| error("invalid_utf8", "Некорректное кодирование сообщения"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_roundtrip_across_byte_boundaries() {
        let original = "🔒Привет\n".repeat(6000);
        let encoded = chunks(&original).unwrap();
        let mut receiver = Reassembler::default();
        for part in &encoded[..encoded.len() - 1] {
            assert_eq!(receiver.push(part).unwrap(), None);
        }
        assert_eq!(
            receiver.push(encoded.last().unwrap()).unwrap(),
            Some(original)
        );
    }
    #[test]
    fn exact_size_limit_and_empty_input() {
        assert!(chunks("").is_err());
        assert_eq!(
            chunks(&"a".repeat(MAX_MESSAGE_BYTES)).unwrap().len(),
            MAX_CHUNKS
        );
        assert!(chunks(&"a".repeat(MAX_MESSAGE_BYTES + 1)).is_err());
    }
    #[test]
    fn incomplete_message_is_replaced_without_showing_prefix() {
        let parts = chunks(&"a".repeat(CHUNK_BYTES + 1)).unwrap();
        let mut receiver = Reassembler::default();
        assert_eq!(receiver.push(&parts[0]).unwrap(), None);
        assert_eq!(
            receiver.push(&chunks("new").unwrap()[0]).unwrap(),
            Some("new".into())
        );
        assert!(receiver.push(&parts[1]).is_err());
    }
    #[test]
    fn missing_duplicate_and_out_of_order_chunks_are_rejected() {
        let parts = chunks(&"a".repeat(CHUNK_BYTES * 3)).unwrap();
        let mut receiver = Reassembler::default();
        assert!(receiver.push(&parts[1]).is_err());
        receiver.push(&parts[0]).unwrap();
        assert!(receiver.push(&parts[2]).is_err());
        assert!(receiver.pending.is_none());
    }
    #[test]
    fn forged_metadata_and_oversized_payload_are_bounded() {
        let mut part: serde_json::Value = serde_json::from_slice(&chunks("x").unwrap()[0]).unwrap();
        part["total"] = serde_json::json!(usize::MAX);
        assert!(Reassembler::default()
            .push(&serde_json::to_vec(&part).unwrap())
            .is_err());
        assert!(Reassembler::default()
            .push(&vec![0; MAX_CHUNK_JSON_BYTES + 1])
            .is_err());
    }
}
