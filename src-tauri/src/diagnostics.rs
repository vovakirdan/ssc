use crate::peer::state::lock;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::{collections::VecDeque, sync::Mutex};
#[derive(Clone, Serialize)]
pub struct DiagnosticEvent {
    timestamp: String,
    session_id: u64,
    component: String,
    stage: String,
    outcome: String,
    code: Option<String>,
}
#[derive(Default)]
struct Buffer {
    enabled: bool,
    events: VecDeque<DiagnosticEvent>,
}
static BUFFER: Lazy<Mutex<Buffer>> = Lazy::new(|| Mutex::new(Buffer::default()));
#[derive(Serialize)]
pub struct Diagnostics {
    enabled: bool,
    events: Vec<DiagnosticEvent>,
}
// Only identifiers are accepted: no external error text, payloads, addresses, or keys.
fn identifier(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_lowercase() || *c == '_')
        .take(64)
        .collect()
}
pub fn record(session_id: u64, component: &str, stage: &str, outcome: &str, code: Option<&str>) {
    let mut b = lock(&BUFFER);
    if !b.enabled {
        return;
    }
    if b.events.len() == 2000 {
        b.events.pop_front();
    }
    b.events.push_back(DiagnosticEvent {
        timestamp: chrono::Utc::now().to_rfc3339(),
        session_id,
        component: identifier(component),
        stage: identifier(stage),
        outcome: identifier(outcome),
        code: code.map(identifier),
    });
}
#[tauri::command]
pub fn get_diagnostics() -> Diagnostics {
    let b = lock(&BUFFER);
    Diagnostics {
        enabled: b.enabled,
        events: b.events.iter().cloned().collect(),
    }
}
#[tauri::command]
pub fn set_diagnostics(enabled: bool) {
    let mut b = lock(&BUFFER);
    b.enabled = enabled;
    if !enabled {
        b.events.clear();
    }
}
#[tauri::command]
pub fn clear_diagnostics() {
    lock(&BUFFER).events.clear();
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_disabled_by_default() {
        assert!(!get_diagnostics().enabled);
        set_diagnostics(true);
        for _ in 0..2005 {
            record(1, "session", "ice", "ok", None);
        }
        assert_eq!(get_diagnostics().events.len(), 2000);
        let json = serde_json::to_string(&get_diagnostics()).unwrap();
        for secret in ["PUBKEY:", "PRIVATE_KEY", "PASSWORD", "SDP=", "MESSAGE_TEXT"] {
            assert!(!json.contains(secret));
        }
        set_diagnostics(false);
        record(1, "session", "ice", "ok", None);
        assert!(get_diagnostics().events.is_empty());
    }
}
