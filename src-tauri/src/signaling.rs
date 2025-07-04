use tauri::command;
use crate::webrtc_peer;

#[command]
pub async fn generate_offer() -> String {
    webrtc_peer::generate_offer().await
}

#[command]
pub async fn accept_offer_and_create_answer(encoded: String) -> String {
    webrtc_peer::accept_offer_and_create_answer(encoded).await
}

#[command]
pub async fn set_answer(encoded: String) -> bool {
    webrtc_peer::set_answer(encoded).await
}

#[command]
pub async fn send_text(msg: String) -> bool {
    webrtc_peer::send_text(msg).await
}