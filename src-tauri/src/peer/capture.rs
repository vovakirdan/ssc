//! Report only OS protection that the native API actually accepted.
use once_cell::sync::Lazy;
use serde::Serialize;
use std::sync::Mutex;

#[derive(Clone, Serialize)]
pub struct CaptureProtection {
    pub platform: &'static str,
    pub enabled: bool,
    pub limitation: Option<&'static str>,
}

fn platform_status() -> CaptureProtection {
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "other"
    };
    CaptureProtection {
        platform,
        enabled: false,
        limitation: Some(
            "Системная защита захвата окна не подтверждена. Используйте шторку приватности.",
        ),
    }
}

static STATUS: Lazy<Mutex<CaptureProtection>> = Lazy::new(|| Mutex::new(platform_status()));

pub fn configure(app: &tauri::App) {
    #[cfg(target_os = "windows")]
    {
        use tauri::Manager;
        let enabled = app
            .get_webview_window("main")
            .is_some_and(|window| window.set_content_protected(true).is_ok());
        let mut status = super::state::lock(&STATUS);
        status.enabled = enabled;
        if enabled {
            status.limitation =
                Some("Защита системного захвата запрошена у Windows. Она не гарантирует защиту от всех способов записи и внешней камеры.");
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = app;
}

pub fn status() -> CaptureProtection {
    super::state::lock(&STATUS).clone()
}

#[cfg(test)]
mod tests {
    #[test]
    fn default_status_never_claims_unverified_capture_protection() {
        let value = super::platform_status();
        assert!(!value.enabled);
        assert!(value.limitation.is_some());
        assert!(matches!(
            value.platform,
            "windows" | "macos" | "linux" | "other"
        ));
    }
}
