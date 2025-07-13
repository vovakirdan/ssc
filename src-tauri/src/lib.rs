mod signaling;
mod webrtc_peer;
mod config;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // Legacy API
            signaling::generate_offer,
            signaling::accept_offer_and_create_answer,
            signaling::set_answer,
            
            // New API with candidates
            signaling::generate_offer_with_candidates,
            signaling::accept_offer_with_candidates,
            signaling::set_answer_with_candidates,
            signaling::add_ice_candidate,
            
            // Utility functions
            signaling::send_text,
            signaling::get_fingerprint,
            signaling::is_connected,
            signaling::disconnect,
            signaling::check_ice_server_availability,
            
            greet
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
