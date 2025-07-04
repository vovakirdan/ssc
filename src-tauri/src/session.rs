use std::sync::Mutex;

pub struct Session {
    pub local_offer: Option<String>,
    pub peer_answer: Option<String>,
    // В дальнейшем: peer_id, keys, rtc_peer и т.д.
}

// Примитивный singleton (Mutex). Для MVP хватит.
use once_cell::sync::Lazy;
pub static SESSION: Lazy<Mutex<Session>> = Lazy::new(|| {
    Mutex::new(Session {
        local_offer: None,
        peer_answer: None,
    })
});
