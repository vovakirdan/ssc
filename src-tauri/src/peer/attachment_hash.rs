//! Bounded incremental hashing; this adapter never keeps file chunks or paths.
use super::state::{lock, Session, SessionData};
use crate::errors::AppError;
use ssc_protocol::FileHasher;

const MAX_HASHERS: usize = 4;
const MAX_HASH_CHUNK: usize = 256 * 1024;

fn ready<'a>(
    s: &'a Session,
    generation: u64,
    id: &str,
) -> Result<std::sync::MutexGuard<'a, SessionData>, AppError> {
    s.ensure_generation_ready(generation)?;
    if id.len() != 32
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(AppError::new(
            "invalid_hash_id",
            "attachment",
            "Недопустимый запрос проверки файла",
            false,
        ));
    }
    let d = lock(&s.data);
    if d.closed || !d.local_ready || !d.peer_ready || d.crypto.is_none() {
        return Err(AppError::new(
            "session_closed",
            "session",
            "Подключение уже изменилось",
            true,
        ));
    }
    Ok(d)
}

pub fn update(
    s: &Session,
    generation: u64,
    id: &str,
    data: &[u8],
    reset: bool,
) -> Result<(), AppError> {
    let mut d = ready(s, generation, id)?;
    if data.len() > MAX_HASH_CHUNK {
        return Err(AppError::new(
            "hash_chunk_size",
            "attachment",
            "Часть файла слишком большая",
            false,
        ));
    }
    if reset {
        if !d.hashers.contains_key(id) && d.hashers.len() >= MAX_HASHERS {
            return Err(AppError::new(
                "hasher_capacity",
                "attachment",
                "Одновременно проверяется слишком много файлов",
                true,
            ));
        }
        d.hashers.insert(id.to_owned(), FileHasher::new());
    }
    let hasher = d.hashers.get_mut(id).ok_or_else(|| {
        AppError::new(
            "hasher_missing",
            "attachment",
            "Проверка файла не начата",
            true,
        )
    })?;
    let result = hasher.update(data).map_err(Into::into);
    if result.is_err() {
        d.hashers.remove(id);
    }
    result
}

pub fn finish(s: &Session, generation: u64, id: &str) -> Result<String, AppError> {
    let mut d = ready(s, generation, id)?;
    let mut hasher = d.hashers.remove(id).ok_or_else(|| {
        AppError::new(
            "hasher_missing",
            "attachment",
            "Проверка файла не начата",
            true,
        )
    })?;
    hasher.finish().map_err(Into::into)
}

pub fn abort(s: &Session, generation: u64, id: &str) -> Result<(), AppError> {
    ready(s, generation, id)?.hashers.remove(id);
    Ok(())
}
