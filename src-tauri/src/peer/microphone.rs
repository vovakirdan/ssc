//! Microphone access starts only after the explicit record action in a verified chat.
use super::state::{lock, Session};
use crate::errors::AppError;
use std::time::{Duration, Instant};
use tauri::webview::{PermissionKind, PermissionResponse};

const REQUEST_WINDOW: Duration = Duration::from_secs(5);

pub fn arm(s: &Session, generation: u64) -> Result<(), AppError> {
    s.ensure_generation_ready(generation)?;
    let mut d = lock(&s.data);
    if d.closed || !d.local_ready || !d.peer_ready || d.crypto.is_none() {
        return Err(AppError::new(
            "session_closed",
            "session",
            "Подключение уже изменилось",
            true,
        ));
    }
    d.microphone_until = Some(Instant::now() + REQUEST_WINDOW);
    Ok(())
}

#[cfg(any(target_os = "linux", test))]
fn consume(s: &Session, now: Instant) -> bool {
    let mut d = lock(&s.data);
    let until = d.microphone_until.take();
    !d.closed
        && d.local_ready
        && d.peer_ready
        && d.crypto.is_some()
        && until.is_some_and(|until| now < until)
}

pub fn permission(webview: tauri::Webview, kind: PermissionKind) -> PermissionResponse {
    // Linux WebKit's unhandled UserMedia requests are denied, rather than prompted.
    // WebView2/WKWebView retain their normal platform permission prompts.
    #[cfg(target_os = "linux")]
    if kind == PermissionKind::Microphone {
        return if webview.label() == "main"
            && super::state::current().is_ok_and(|s| consume(&s, Instant::now()))
        {
            PermissionResponse::Allow
        } else {
            PermissionResponse::Deny
        };
    }
    let _ = (webview, kind);
    PermissionResponse::Default
}

pub fn configure(app: &tauri::App) -> tauri::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use tauri::Manager;
        use webkit2gtk::{SettingsExt, WebViewExt};
        if let Some(window) = app.get_webview_window("main") {
            window.with_webview(|webview| {
                if let Some(settings) = webview.inner().settings() {
                    // Older supported WebKit versions defaulted to disabled.
                    // This enables the API, never grants device permission.
                    settings.set_enable_media_stream(true);
                }
            })?;
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer::crypto::KeyExchange;

    fn verified() -> std::sync::Arc<Session> {
        let s = Session::new(801, "e".repeat(32), None);
        let a = KeyExchange::new().unwrap();
        let b = KeyExchange::new().unwrap();
        let mut d = lock(&s.data);
        d.crypto = Some(a.finish(b.public_key(), &[9; 32], true).unwrap());
        d.local_ready = true;
        d.peer_ready = true;
        drop(d);
        s
    }

    #[test]
    fn microphone_is_unarmed_then_one_shot_and_expires() {
        let s = verified();
        assert!(!consume(&s, Instant::now()));
        arm(&s, 801).unwrap();
        assert!(consume(&s, Instant::now()));
        assert!(!consume(&s, Instant::now()));
        arm(&s, 801).unwrap();
        assert!(!consume(&s, Instant::now() + REQUEST_WINDOW));
        assert!(lock(&s.data).microphone_until.is_none());
    }

    #[tokio::test]
    async fn microphone_requires_current_verified_session_and_retires() {
        let s = verified();
        assert!(arm(&s, 800).is_err());
        assert!(lock(&s.data).microphone_until.is_none());
        lock(&s.data).peer_ready = false;
        assert!(arm(&s, 801).is_err());
        lock(&s.data).peer_ready = true;
        arm(&s, 801).unwrap();
        s.close().await;
        assert!(lock(&s.data).microphone_until.is_none());
        assert!(!consume(&s, Instant::now()));
        assert!(arm(&s, 801).is_err());
    }
}
