mod commands;
mod config;
mod logger;
mod peer;
mod utils;

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
            commands::legacy_api::generate_offer,
            commands::legacy_api::accept_offer_and_create_answer,
            commands::legacy_api::set_answer,
            // New API with candidates
            commands::candidate_api::generate_offer_with_candidates,
            commands::candidate_api::accept_offer_with_candidates,
            commands::candidate_api::set_answer_with_candidates,
            peer::ice::add_ice_candidate,
            // Utility functions
            commands::util_api::send_text,
            commands::util_api::get_fingerprint,
            commands::util_api::is_connected,
            commands::util_api::disconnect,
            peer::ice::check_ice_server_availability,
            peer::connection::set_ice_servers,
            peer::connection::get_ice_servers,
            greet
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
