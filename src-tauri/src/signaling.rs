use tauri::command;
use rand::Rng;
use base64::{engine::general_purpose, Engine as _};
use chrono::Utc;

#[command]
pub async fn generate_offer() -> String {
    // На данном этапе вместо настоящего offer — просто случайная строка + timestamp
    // В будущем тут будет генерация WebRTC offer
    let mut rng = rand::rng();
    let random_bytes: [u8; 16] = rng.random();
    let now = Utc::now().timestamp();

    let payload = format!(
        "{{\"offer\":\"{}\",\"timestamp\":{}}}",
        general_purpose::STANDARD.encode(random_bytes),
        now
    );

    general_purpose::STANDARD.encode(payload)
}
