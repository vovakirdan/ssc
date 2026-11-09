mod access;
mod commands;
mod connect;
mod diagnostics;
mod errors;
mod peer;
mod rooms;
mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            access::configure(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            access::initialize_access,
            access::activate_access,
            access::get_access_status,
            access::reset_access,
            peer::connection::set_network_mode,
            commands::candidate_api::generate_offer_with_candidates,
            commands::candidate_api::accept_offer_with_candidates,
            commands::candidate_api::set_answer_with_candidates,
            connect::create_link_invitation,
            connect::join_link_invitation,
            connect::retry_link_invitation,
            connect::cancel_link_invitation,
            connect::get_link_state,
            rooms::create_temporary_room,
            rooms::get_temporary_room_state,
            commands::util_api::send_text,
            commands::util_api::get_fingerprint,
            commands::util_api::is_connected,
            commands::util_api::disconnect,
            commands::util_api::confirm_sas,
            commands::util_api::reject_sas,
            commands::util_api::get_sas_status,
            commands::util_api::get_session_state,
            commands::util_api::drain_messages,
            peer::ice::check_ice_server_availability,
            peer::connection::set_ice_servers,
            peer::connection::get_ice_servers,
            diagnostics::get_diagnostics,
            diagnostics::set_diagnostics,
            diagnostics::clear_diagnostics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
