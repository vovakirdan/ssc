//! Incremental file hashing without retaining file chunks.
use crate::{events::MAX_ATTACHMENT_BYTES, ProtocolError};
use sha2::{Digest, Sha256};

pub struct FileHasher {
    state: Option<Sha256>,
    bytes: u64,
}
fn finished() -> ProtocolError {
    ProtocolError::new(
        "hasher_finished",
        "attachment",
        "Проверка файла уже завершена",
        false,
    )
}
impl Default for FileHasher {
    fn default() -> Self {
        Self::new()
    }
}
impl FileHasher {
    pub fn new() -> Self {
        Self {
            state: Some(Sha256::new()),
            bytes: 0,
        }
    }
    pub fn update(&mut self, data: &[u8]) -> Result<(), ProtocolError> {
        if self.state.is_none() {
            return Err(finished());
        }
        let Some(total) = self
            .bytes
            .checked_add(data.len() as u64)
            .filter(|total| *total <= MAX_ATTACHMENT_BYTES)
        else {
            self.abort();
            return Err(ProtocolError::new(
                "file_size",
                "attachment",
                "Файл превышает 100 MiB",
                false,
            ));
        };
        if let Some(state) = &mut self.state {
            state.update(data);
        }
        self.bytes = total;
        Ok(())
    }
    pub fn finish(&mut self) -> Result<String, ProtocolError> {
        let state = self.state.take().ok_or_else(finished)?;
        self.bytes = 0;
        Ok(hex::encode(state.finalize()))
    }
    pub fn abort(&mut self) {
        if let Some(mut state) = self.state.take() {
            state.reset();
        }
        self.bytes = 0;
    }
}
impl Drop for FileHasher {
    fn drop(&mut self) {
        self.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incremental_hashes_match_empty_16k_and_100m_independent_vectors() {
        let block = vec![0xa5; 256 * 1024];
        for (size, expected) in [
            (
                0,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                16 * 1024,
                "add4fa3e6dbfb8723b21cfe7debadbed5d5f2ca1ab88c666470daed16afbe21a",
            ),
            (
                MAX_ATTACHMENT_BYTES,
                "c15f74b731fd491b1964c8ccd322d9d2a0ce9cafc95d13dd8b778332e18339bf",
            ),
        ] {
            let mut hasher = FileHasher::new();
            let mut remaining = size;
            while remaining > 0 {
                let size = remaining.min(block.len() as u64) as usize;
                hasher.update(&block[..size]).unwrap();
                remaining -= size as u64;
            }
            assert_eq!(hasher.finish().unwrap(), expected);
            assert!(hasher.finish().is_err());
            assert!(hasher.update(&[]).is_err());
        }
    }
    #[test]
    fn size_limit_and_abort_are_terminal_without_wrapping() {
        let mut hasher = FileHasher::new();
        hasher.bytes = MAX_ATTACHMENT_BYTES;
        assert_eq!(hasher.update(&[0]).unwrap_err().code, "file_size");
        assert!(hasher.finish().is_err());
        assert!(hasher.update(&[]).is_err());
        let mut hasher = FileHasher::new();
        hasher.update(b"private file tail").unwrap();
        hasher.abort();
        assert!(hasher.finish().is_err());
        assert!(hasher.update(b"late").is_err());
    }
}
