use tauri::command;
use rand::Rng;
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;
use crate::session::SESSION;

#[command]
pub async fn generate_offer() -> String {
    let mut rng = rand::rng();
    let random_bytes: [u8; 16] = rng.random();
    let now = Utc::now().timestamp();

    let payload = format!(
        "{{\"offer\":\"{}\",\"timestamp\":{}}}",
        general_purpose::STANDARD.encode(random_bytes),
        now
    );

    // Сохраняем состояние offer в глобальной сессии (примитивно)
    {
        let mut sess = SESSION.lock().unwrap();
        sess.local_offer = Some(payload.clone());
        sess.peer_answer = None;
    }

    general_purpose::STANDARD.encode(payload)
}

// Принять answer от другого клиента (после сканирования QR)
#[command]
pub async fn accept_answer(encoded: String) -> bool {
    // Валидация, разбор и сохранение peer_answer
    let Ok(decoded) = general_purpose::STANDARD.decode(&encoded) else { return false; };
    let answer = String::from_utf8_lossy(&decoded).to_string();

    // Можно парсить json если нужно
    let mut sess = SESSION.lock().unwrap();
    sess.peer_answer = Some(answer);
    true
}
