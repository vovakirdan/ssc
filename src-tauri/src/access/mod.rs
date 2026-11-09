//! Device access lives only in Rust; UI receives safe status, never credentials.
mod storage;
#[cfg(test)]
mod tests;
use crate::{errors::AppError, peer::types::ServerConfig};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD as B64, Engine as _};
use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use reqwest::{Client, StatusCode};
use ring::{
    rand::{SecureRandom, SystemRandom},
    signature::{Ed25519KeyPair, KeyPair},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, sync::Mutex};
use tauri::Manager;
use tokio::sync::Mutex as AsyncMutex;
use zeroize::{Zeroize, Zeroizing};

static PROFILE: Lazy<Mutex<Option<PathBuf>>> = Lazy::new(|| Mutex::new(None));
static ACCESS: Lazy<AsyncMutex<()>> = Lazy::new(|| AsyncMutex::new(()));

pub fn configure(app: &tauri::App) -> Result<(), AppError> {
    let path = match std::env::var_os("SSC_PROFILE_DIR") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        Some(_) => {
            return Err(error(
                "access_storage",
                "Путь профиля доступа не задан",
                false,
            ))
        }
        None => app.path().app_data_dir().map_err(|_| {
            error(
                "access_storage",
                "Не удалось определить каталог доступа",
                false,
            )
        })?,
    };
    storage::prepare(&path)?;
    *crate::peer::state::lock(&PROFILE) = Some(path);
    Ok(())
}
fn profile() -> Result<PathBuf, AppError> {
    crate::peer::state::lock(&PROFILE)
        .clone()
        .ok_or_else(|| error("access_storage", "Профиль доступа не подготовлен", false))
}
fn error(code: &str, message: &str, retryable: bool) -> AppError {
    AppError::new(code, "access", message, retryable)
}
fn malformed() -> AppError {
    error(
        "access_response",
        "Сервис доступа вернул некорректные данные",
        false,
    )
}

#[derive(Serialize)]
pub struct AccessStatus {
    state: &'static str,
    // An operator reference, not the public/private key; never include it in diagnostics.
    #[serde(skip_serializing_if = "Option::is_none")]
    access_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<AppError>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Identity {
    version: u8,
    helper_origin: String,
    seed: String,
    activated: bool,
}
impl Drop for Identity {
    fn drop(&mut self) {
        self.seed.zeroize();
    }
}
impl Identity {
    fn key(&self) -> Result<Ed25519KeyPair, AppError> {
        let seed = Zeroizing::new(B64.decode(&self.seed).map_err(|_| malformed())?);
        if seed.len() != 32 {
            return Err(malformed());
        }
        Ed25519KeyPair::from_seed_unchecked(&seed).map_err(|_| malformed())
    }
    fn device_id(&self) -> Result<String, AppError> {
        Ok(B64.encode(Sha256::digest(self.key()?.public_key().as_ref())))
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Bootstrap {
    version: u8,
    helper_origin: String,
    bootstrap_token: String,
    expires_at: DateTime<Utc>,
}
impl Drop for Bootstrap {
    fn drop(&mut self) {
        self.bootstrap_token.zeroize();
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Activated {
    device_id: String,
    state: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Challenge {
    challenge: String,
    expires_at: DateTime<Utc>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialResponse {
    ice_servers: Vec<IceServer>,
    expires_at: DateTime<Utc>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IceServer {
    urls: Vec<String>,
    username: String,
    credential: String,
}
impl Drop for IceServer {
    fn drop(&mut self) {
        self.username.zeroize();
        self.credential.zeroize();
    }
}

fn valid_token(value: &str) -> bool {
    value.len() == 43
        && B64
            .decode(value)
            .is_ok_and(|b| b.len() == 32 && B64.encode(b) == value)
}
fn valid_challenge(value: &str) -> bool {
    value.len() == 96
        && B64
            .decode(value)
            .is_ok_and(|bytes| bytes.len() == 72 && B64.encode(bytes) == value)
}
fn valid_expiry(value: DateTime<Utc>, max: i64) -> bool {
    let remaining = (value - Utc::now()).num_seconds();
    (1..=max).contains(&remaining)
}
fn identity(path: &std::path::Path, origin: &str) -> Result<Option<Identity>, AppError> {
    let Some(bytes) = storage::read(&path.join("access.key"), true)? else {
        return Ok(None);
    };
    let identity: Identity = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if identity.version != 1 || identity.helper_origin != origin {
        return Err(error(
            "access_origin",
            "Данные доступа относятся к другому сервису",
            false,
        ));
    }
    identity.key()?;
    Ok(Some(identity))
}
fn save(path: &std::path::Path, identity: &Identity) -> Result<(), AppError> {
    let bytes = Zeroizing::new(serde_json::to_vec(identity).map_err(|_| malformed())?);
    storage::write(&path.join("access.key"), &bytes, true)
}
fn finish_activation(path: &std::path::Path) -> Result<(), AppError> {
    storage::write(&path.join("access.active"), b"1\n", false)?;
    match std::fs::remove_file(path.join("bootstrap.json")) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(error(
            "access_storage",
            "Не удалось очистить использованный профиль активации",
            true,
        )),
    }
}
async fn activate(
    path: &std::path::Path,
    origin: &str,
    client: &Client,
) -> Result<Identity, AppError> {
    storage::prepare(path)?;
    let current = identity(path, origin)?;
    if current.as_ref().is_some_and(|identity| identity.activated) {
        finish_activation(path)?;
        return current.ok_or_else(malformed);
    }
    let bytes = storage::read(&path.join("bootstrap.json"), false)?.ok_or_else(|| {
        error(
            "access_activation_required",
            "Нужен персональный установщик с действующей активацией или свой TURN",
            true,
        )
    })?;
    let bootstrap: Bootstrap = serde_json::from_slice(&bytes).map_err(|_| malformed())?;
    if bootstrap.version != 1 || bootstrap.helper_origin != origin {
        return Err(error(
            "access_origin",
            "Профиль активации относится к другому сервису",
            false,
        ));
    }
    if !valid_token(&bootstrap.bootstrap_token)
        || !valid_expiry(bootstrap.expires_at, 7 * 24 * 60 * 60 + 60)
    {
        return Err(error(
            "access_activation_expired",
            "Профиль активации повреждён или просрочен; нужен новый установщик",
            true,
        ));
    }
    let mut identity = if let Some(current) = current {
        current
    } else {
        let mut seed = Zeroizing::new([0u8; 32]);
        SystemRandom::new()
            .fill(seed.as_mut())
            .map_err(|_| error("access_random", "Не удалось создать ключ доступа", true))?;
        let identity = Identity {
            version: 1,
            helper_origin: origin.into(),
            seed: B64.encode(seed.as_ref()),
            activated: false,
        };
        // Persist before the request: a lost reply must retry with exactly this key.
        save(path, &identity)?;
        identity
    };
    let key = identity.key()?;
    let public = B64.encode(key.public_key().as_ref());
    let message = format!(
        "ssc-access-v1\nactivate\n{origin}\n{}\n{public}\n",
        bootstrap.bootstrap_token
    );
    let signature = B64.encode(key.sign(message.as_bytes()).as_ref());
    let response: Activated = post(client, origin, "/v1/access/activate", &serde_json::json!({"token":bootstrap.bootstrap_token,"public_key":public,"signature":signature})).await?;
    if response.state != "active" || response.device_id != identity.device_id()? {
        return Err(malformed());
    }
    identity.activated = true;
    save(path, &identity)?;
    finish_activation(path)?;
    Ok(identity)
}
async fn post<T: DeserializeOwned>(
    client: &Client,
    origin: &str,
    path: &str,
    payload: &serde_json::Value,
) -> Result<T, AppError> {
    let failure = || {
        error(
            "access_unavailable",
            "Сервис доступа недоступен. Повторите попытку",
            true,
        )
    };
    let mut response = client
        .post(format!("{origin}{path}"))
        .json(payload)
        .send()
        .await
        .map_err(|_| failure())?;
    if !response.status().is_success() {
        return Err(match response.status() {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN | StatusCode::CONFLICT => error(
                "access_denied",
                "Доступ отклонён: активация использована, недействительна или отозвана",
                true,
            ),
            StatusCode::TOO_MANY_REQUESTS => error(
                "access_rate_limited",
                "Слишком много запросов доступа; повторите позже",
                true,
            ),
            status if status.is_server_error() => failure(),
            _ => malformed(),
        });
    }
    let media = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    if media != "application/json" || response.content_length().is_some_and(|n| n > 16 * 1024) {
        return Err(malformed());
    }
    let mut bytes = Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| failure())? {
        if bytes.len() + chunk.len() > 16 * 1024 {
            return Err(malformed());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| malformed())
}
async fn credentials(
    path: &std::path::Path,
    origin: &str,
    client: &Client,
) -> Result<Vec<ServerConfig>, AppError> {
    let identity = activate(path, origin, client).await?;
    let device_id = identity.device_id()?;
    let challenge: Challenge = post(
        client,
        origin,
        "/v1/access/challenge",
        &serde_json::json!({"device_id":device_id}),
    )
    .await?;
    if !valid_challenge(&challenge.challenge) || !valid_expiry(challenge.expires_at, 90) {
        return Err(malformed());
    }
    let message = format!(
        "ssc-access-v1\nturn\n{origin}\n{device_id}\n{}\n",
        challenge.challenge
    );
    let signature = B64.encode(identity.key()?.sign(message.as_bytes()).as_ref());
    let response: CredentialResponse = post(client, origin, "/v1/turn/credentials", &serde_json::json!({"device_id":device_id,"challenge":challenge.challenge,"signature":signature})).await?;
    validate_credentials(response)
}
fn project_turn_urls(host: &str) -> Result<[String; 3], AppError> {
    if host.is_empty()
        || host.len() > 253
        || !host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        return Err(error(
            "ice_config",
            "Некорректное имя настроенного TURN",
            false,
        ));
    }
    Ok([
        format!("turn:{host}:3478?transport=udp"),
        format!("turn:{host}:3478?transport=tcp"),
        format!("turns:{host}:5349?transport=tcp"),
    ])
}

fn validate_credentials(response: CredentialResponse) -> Result<Vec<ServerConfig>, AppError> {
    let host = match std::env::var("SSC_TURN_HOST") {
        Ok(host) => host,
        Err(std::env::VarError::NotPresent) => "turn.vovakirdan.ru".into(),
        Err(_) => {
            return Err(error(
                "ice_config",
                "Некорректное имя настроенного TURN",
                false,
            ))
        }
    };
    let allowed = project_turn_urls(&host)?;
    if !valid_expiry(response.expires_at, 630)
        || response.ice_servers.is_empty()
        || response.ice_servers.len() > 8
    {
        return Err(malformed());
    }
    let mut servers = Vec::new();
    for server in response.ice_servers {
        if server.username.is_empty()
            || server.username.len() > 256
            || server.credential.is_empty()
            || server.credential.len() > 256
            || server.urls.is_empty()
            || server.urls.len() > 8
        {
            return Err(malformed());
        }
        for url in &server.urls {
            // The service may supply credentials, but may not redirect relay traffic.
            if !allowed.contains(url) {
                return Err(malformed());
            }
            let transport = std::env::var("SSC_TURN_TRANSPORT").unwrap_or_default();
            let include = match transport.as_str() {
                "" => true,
                "udp" => url.starts_with("turn:") && url.ends_with("?transport=udp"),
                "tcp" => url.starts_with("turn:") && url.ends_with("?transport=tcp"),
                "tls" => url.starts_with("turns:"),
                _ => return Err(error("ice_config", "Неизвестный транспорт TURN", false)),
            };
            if include {
                servers.push(ServerConfig {
                    id: format!("project-{}", servers.len()),
                    r#type: "turn".into(),
                    url: url.clone(),
                    username: Some(server.username.clone()),
                    credential: Some(server.credential.clone()),
                });
            }
        }
    }
    if servers.is_empty() {
        return Err(malformed());
    }
    Ok(servers)
}
fn access_status(result: Result<Option<Identity>, AppError>) -> AccessStatus {
    let result = result.and_then(|identity| {
        identity
            .filter(|i| i.activated)
            .map(|i| i.device_id())
            .transpose()
    });
    match result {
        Ok(Some(reference)) => AccessStatus {
            state: "active",
            access_reference: Some(reference),
            error: None,
        },
        Ok(None) => AccessStatus {
            state: "inactive",
            access_reference: None,
            error: None,
        },
        Err(e) if e.code == "access_activation_required" => AccessStatus {
            state: "inactive",
            access_reference: None,
            error: None,
        },
        Err(e) => AccessStatus {
            state: "error",
            access_reference: None,
            error: Some(e),
        },
    }
}
#[tauri::command]
pub async fn initialize_access() -> AccessStatus {
    let _guard = ACCESS.lock().await;
    let result = async {
        let (origin, client) = crate::connect::http::access_config()?;
        let path = profile()?;
        let _profile_lock = storage::lock(&path)?;
        activate(&path, &origin, &client).await.map(Some)
    }
    .await;
    access_status(result)
}
#[tauri::command]
pub fn get_access_status() -> AccessStatus {
    access_status((|| {
        let (origin, _) = crate::connect::http::access_config()?;
        identity(&profile()?, &origin)
    })())
}
pub async fn managed_servers() -> Result<Vec<ServerConfig>, AppError> {
    let _guard = ACCESS.lock().await;
    let (origin, client) = crate::connect::http::access_config()?;
    let path = profile()?;
    let _profile_lock = storage::lock(&path)?;
    credentials(&path, &origin, &client).await
}
#[tauri::command]
pub async fn reset_access() -> Result<(), AppError> {
    let _guard = ACCESS.lock().await;
    reset_files(&profile()?)
}
fn reset_files(path: &std::path::Path) -> Result<(), AppError> {
    let _profile_lock = storage::lock(path)?;
    // Clear the installer marker before the key; persist completion below.
    for name in ["access.active", "access.key", "bootstrap.json"] {
        match std::fs::remove_file(path.join(name)) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => {
                return Err(error(
                    "access_storage",
                    "Не удалось сбросить локальные данные доступа",
                    true,
                ))
            }
        }
    }
    #[cfg(unix)]
    std::fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| {
            error(
                "access_storage",
                "Не удалось завершить сброс локальных данных доступа",
                true,
            )
        })?;
    Ok(())
}
