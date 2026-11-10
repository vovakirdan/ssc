//! Native message-ID entropy adapter around the shared bounded message codec.
use crate::errors::AppError;
pub use ssc_protocol::message::{Reassembler, KIND_CHUNK, KIND_READY};
pub use ssc_protocol::{decode_event, encode_event, ChatEvent, KIND_EVENT};

pub fn chunks(text: &str) -> Result<Vec<Vec<u8>>, AppError> {
    ssc_protocol::message::chunks_with_id(text, rand::random::<[u8; 16]>()).map_err(Into::into)
}
