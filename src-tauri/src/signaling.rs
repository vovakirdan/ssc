use crate::webrtc_peer::{self, APP};
use tauri::{command, AppHandle}; // import APP to store

#[command] // A-side
pub async fn generate_offer(app: AppHandle) -> String {
    *APP.lock().unwrap() = Some(app); // remember handle
    webrtc_peer::generate_offer().await
}

#[command] // B-side create answer
pub async fn accept_offer_and_create_answer(app: AppHandle, encoded: String) -> String {
    *APP.lock().unwrap() = Some(app);
    webrtc_peer::accept_offer_and_create_answer(encoded).await
}

#[command] // A-side apply answer
pub async fn set_answer(app: AppHandle, encoded: String) -> bool {
    *APP.lock().unwrap() = Some(app);
    webrtc_peer::set_answer(encoded).await
}

#[command] // text send, no change
pub async fn send_text(msg: String) -> bool {
    webrtc_peer::send_text(msg).await
}

#[command]
pub fn get_fingerprint() -> Option<String> {
    webrtc_peer::get_fingerprint()
}

#[command]
pub fn is_connected() -> bool {
    webrtc_peer::is_connected()
}

#[command]
pub async fn disconnect() {
    webrtc_peer::disconnect().await
}
