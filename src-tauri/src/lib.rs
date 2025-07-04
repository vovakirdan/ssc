mod signaling;
mod webrtc_peer;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            signaling::generate_offer,
            signaling::accept_offer_and_create_answer,
            signaling::set_answer,
            signaling::send_text,
            greet
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
