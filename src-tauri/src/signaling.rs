use crate::webrtc_peer::{self, APP};
use tauri::{command, AppHandle}; // import APP to store

// ========== OLD API (LEGACY) ==========

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

// ========== NEW API WITH CANDIDATES ==========

#[command]
pub async fn generate_offer_with_candidates(app: AppHandle) -> String {
    *APP.lock().unwrap() = Some(app);
    webrtc_peer::generate_offer_with_candidates().await
}

#[command]
pub async fn accept_offer_with_candidates(app: AppHandle, encoded: String) -> String {
    *APP.lock().unwrap() = Some(app);
    webrtc_peer::accept_offer_with_candidates(encoded).await
}

#[command]
pub async fn set_answer_with_candidates(app: AppHandle, encoded: String) -> bool {
    *APP.lock().unwrap() = Some(app);
    webrtc_peer::set_answer_with_candidates(encoded).await
}

#[command]
pub async fn add_ice_candidate(app: AppHandle, candidate: webrtc_peer::IceCandidate) -> bool {
    *APP.lock().unwrap() = Some(app);
    webrtc_peer::add_ice_candidate(candidate).await
}

// ========== UTILITY FUNCTIONS ==========

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

#[command]
pub async fn check_ice_server_availability(config: webrtc_peer::ServerConfig) -> bool {
    webrtc_peer::check_ice_server_availability(config).await
}

#[command]
pub async fn set_ice_servers(servers: Vec<webrtc_peer::ServerConfig>) -> bool {
    webrtc_peer::set_ice_servers(servers)
}

#[command]
pub async fn get_ice_servers() -> Vec<webrtc_peer::ServerConfig> {
    webrtc_peer::get_ice_servers()
}
