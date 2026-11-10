//! Fixed help sources only; never pass a renderer-provided URL to the OS.
use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelpSource {
    #[serde(rename = "x25519")]
    X25519,
    Hkdf,
    Aead,
    Webrtc,
    Sas,
    Implementation,
}

pub fn url(source: HelpSource) -> &'static str {
    match source {
        HelpSource::X25519 => "https://www.rfc-editor.org/rfc/rfc7748",
        HelpSource::Hkdf => "https://www.rfc-editor.org/rfc/rfc5869",
        HelpSource::Aead => "https://www.rfc-editor.org/rfc/rfc8439",
        HelpSource::Webrtc => "https://www.rfc-editor.org/rfc/rfc8831",
        HelpSource::Sas => "https://www.rfc-editor.org/rfc/rfc6189",
        HelpSource::Implementation => "https://github.com/vovakirdan/ssc",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_known_source_ids_resolve_to_fixed_https_destinations() {
        for id in ["x25519", "hkdf", "aead", "webrtc", "sas", "implementation"] {
            let source = serde_json::from_value(serde_json::json!(id)).unwrap();
            let url = super::url(source);
            assert!(
                url.starts_with("https://www.rfc-editor.org/rfc/rfc")
                    || url == "https://github.com/vovakirdan/ssc"
            );
        }
        for value in [
            "https://other.invalid",
            "file:///tmp/test",
            "shell:run",
            "unknown",
        ] {
            assert!(serde_json::from_value::<super::HelpSource>(serde_json::json!(value)).is_err());
        }
    }
}
