use crate::errors::AppError;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use reqwest::{header, Client, RequestBuilder, Response, StatusCode, Url};
use serde::{de::DeserializeOwned, Deserialize};
use std::{net::IpAddr, time::Duration};
use zeroize::Zeroize;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 512 * 1024;

#[derive(Clone)]
pub struct Service {
    origin: Url,
    client: Client,
}

// Capabilities deliberately have no Debug or Serialize implementation.
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub token: String,
    pub creator_token: String,
    pub invitation_url: String,
    pub expires_at: DateTime<Utc>,
}
impl Drop for Receipt {
    fn drop(&mut self) {
        self.token.zeroize();
        self.creator_token.zeroize();
        self.invitation_url.zeroize();
    }
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Offer {
    pub offer: String,
    pub expires_at: DateTime<Utc>,
}
impl Drop for Offer {
    fn drop(&mut self) {
        self.offer.zeroize();
    }
}

#[derive(Clone)]
pub struct HttpError {
    pub error: AppError,
    pub retry_after: Option<Duration>,
}
impl std::fmt::Debug for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpError")
            .field("error", &self.error)
            .field("retry_after", &self.retry_after)
            .finish()
    }
}
impl From<AppError> for HttpError {
    fn from(error: AppError) -> Self {
        Self {
            error,
            retry_after: None,
        }
    }
}
fn invalid_link() -> AppError {
    AppError::new(
        "invalid_link",
        "link",
        "Вставьте полную ссылку приглашения от настроенного помощника",
        false,
    )
}
fn malformed() -> HttpError {
    AppError::new(
        "helper_response",
        "helper",
        "Помощник вернул некорректные данные",
        false,
    )
    .into()
}
fn transport() -> HttpError {
    AppError::new(
        "helper_unavailable",
        "helper",
        "Помощник подключения недоступен. Можно повторить попытку",
        true,
    )
    .into()
}
fn valid_token(value: &str) -> bool {
    value.len() == 43
        && URL_SAFE_NO_PAD
            .decode(value)
            .is_ok_and(|bytes| bytes.len() == 32 && URL_SAFE_NO_PAD.encode(bytes) == value)
}
fn raw_userinfo(value: &str) -> bool {
    value.split_once("://").is_none_or(|(_, authority)| {
        authority
            .split('/')
            .next()
            .is_some_and(|part| part.contains('@'))
    })
}
fn valid_payload(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_PAYLOAD_BYTES
}
fn valid_expiry(expiry: DateTime<Utc>) -> bool {
    let delta = expiry.signed_duration_since(Utc::now()).num_seconds();
    expiry > Utc::now() && (0..=3630).contains(&delta)
}

impl Service {
    pub fn from_env() -> Result<Self, AppError> {
        let origin = match std::env::var("SSC_CONNECT_URL") {
            Ok(origin) => origin,
            Err(std::env::VarError::NotPresent) => "http://127.0.0.1:8080".into(),
            Err(_) => {
                return Err(AppError::new(
                    "helper_configuration",
                    "helper",
                    "Некорректный адрес помощника подключения",
                    false,
                ));
            }
        };
        Self::new(&origin)
    }
    pub fn new(origin: &str) -> Result<Self, AppError> {
        let error = || {
            AppError::new(
                "helper_configuration",
                "helper",
                "Некорректный адрес помощника подключения",
                false,
            )
        };
        if origin.trim() != origin
            || raw_userinfo(origin)
            || origin.len() > 2048
            || origin.chars().any(|c| c.is_control() || c == '\\')
        {
            return Err(error());
        }
        let parsed = Url::parse(origin).map_err(|_| error())?;
        if origin.contains(['?', '#'])
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || !matches!(parsed.path(), "" | "/")
        {
            return Err(error());
        }
        let host = parsed.host_str().ok_or_else(error)?;
        let loopback = host == "localhost"
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        if parsed.scheme() != "https" && !(parsed.scheme() == "http" && loopback) {
            return Err(error());
        }
        let client = Client::builder()
            .use_rustls_tls()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| error())?;
        Ok(Self {
            origin: parsed,
            client,
        })
    }
    pub fn parse_link(&self, raw: &str) -> Result<String, AppError> {
        let raw = raw.trim();
        if raw_userinfo(raw) || raw.len() > 2048 || raw.chars().any(|c| c.is_control() || c == '\\')
        {
            return Err(invalid_link());
        }
        let url = Url::parse(raw).map_err(|_| invalid_link())?;
        if raw.trim().contains(['?', '#'])
            || !url.username().is_empty()
            || url.password().is_some()
            || url.origin() != self.origin.origin()
        {
            return Err(invalid_link());
        }
        let authority = raw.split_once("://").ok_or_else(invalid_link)?.1;
        let raw_path = authority.find('/').map(|at| &authority[at..]).unwrap_or("");
        if raw_path != url.path() {
            return Err(invalid_link());
        }
        let token = url.path().strip_prefix("/i/").ok_or_else(invalid_link)?;
        if !valid_token(token) {
            return Err(invalid_link());
        }
        Ok(token.to_owned())
    }
    fn endpoint(&self, token: Option<&str>, answer: bool) -> Result<Url, HttpError> {
        if token.is_some_and(|value| !valid_token(value)) {
            return Err(invalid_link().into());
        }
        let mut url = self.origin.clone();
        let path = match token {
            Some(token) if answer => format!("/v1/invitations/{token}/answer"),
            Some(token) => format!("/v1/invitations/{token}"),
            None => "/v1/invitations".into(),
        };
        url.set_path(&path);
        Ok(url)
    }
    async fn response(&self, request: RequestBuilder) -> Result<Response, HttpError> {
        request.send().await.map_err(|_| transport())
    }
    pub async fn create(&self, offer: &str, ttl: u64, key: &str) -> Result<Receipt, HttpError> {
        if !valid_payload(offer) || !(60..=3600).contains(&ttl) || !valid_token(key) {
            return Err(malformed());
        }
        let mut header_value = header::HeaderValue::from_str(key).map_err(|_| malformed())?;
        header_value.set_sensitive(true);
        let response = self
            .response(
                self.client
                    .post(self.endpoint(None, false)?)
                    .header("Idempotency-Key", header_value)
                    .json(&serde_json::json!({"offer": offer,"ttl_seconds": ttl})),
            )
            .await?;
        check_status(&response, &[StatusCode::CREATED])?;
        let receipt: Receipt = read_json(response).await?;
        let link_token = self.parse_link(&receipt.invitation_url).ok();
        if !valid_token(&receipt.token)
            || !valid_token(&receipt.creator_token)
            || !valid_expiry(receipt.expires_at)
            || receipt.expires_at > Utc::now() + chrono::Duration::seconds(ttl as i64 + 30)
            || link_token.as_deref() != Some(receipt.token.as_str())
        {
            return Err(malformed());
        }
        Ok(receipt)
    }
    pub async fn fetch_offer(&self, token: &str) -> Result<Offer, HttpError> {
        let response = self
            .response(self.client.get(self.endpoint(Some(token), false)?))
            .await?;
        check_status(&response, &[StatusCode::OK])?;
        let offer: Offer = read_json(response).await?;
        if !valid_payload(&offer.offer) || !valid_expiry(offer.expires_at) {
            return Err(malformed());
        }
        Ok(offer)
    }
    pub async fn send_answer(&self, token: &str, answer: &str) -> Result<(), HttpError> {
        if !valid_payload(answer) {
            return Err(malformed());
        }
        let response = self
            .response(
                self.client
                    .post(self.endpoint(Some(token), true)?)
                    .json(&serde_json::json!({"answer":answer})),
            )
            .await?;
        check_status(&response, &[StatusCode::NO_CONTENT])
    }
    pub async fn poll_answer(
        &self,
        token: &str,
        creator: &str,
    ) -> Result<Option<String>, HttpError> {
        let response = self
            .response(
                self.client
                    .get(self.endpoint(Some(token), true)?)
                    .header(header::AUTHORIZATION, authorization(creator)?),
            )
            .await?;
        check_status(&response, &[StatusCode::OK, StatusCode::ACCEPTED])?;
        let pending = response.status() == StatusCode::ACCEPTED;
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Answer {
            state: String,
            answer: Option<String>,
            expires_at: DateTime<Utc>,
        }
        let reply: Answer = read_json(response).await?;
        if !valid_expiry(reply.expires_at) {
            return Err(malformed());
        }
        if pending && reply.state == "waiting" && reply.answer.is_none() {
            return Ok(None);
        }
        if !pending && reply.state == "answered" {
            if let Some(answer) = reply.answer.filter(|value| valid_payload(value)) {
                return Ok(Some(answer));
            }
        }
        Err(malformed())
    }
    pub async fn cancel(&self, token: &str, creator: &str) -> Result<(), HttpError> {
        let response = self
            .response(
                self.client
                    .delete(self.endpoint(Some(token), false)?)
                    .header(header::AUTHORIZATION, authorization(creator)?),
            )
            .await?;
        check_status(&response, &[StatusCode::NO_CONTENT, StatusCode::NOT_FOUND])
    }
}
fn authorization(token: &str) -> Result<header::HeaderValue, HttpError> {
    if !valid_token(token) {
        return Err(malformed());
    }
    let mut value =
        header::HeaderValue::from_str(&format!("Bearer {token}")).map_err(|_| malformed())?;
    value.set_sensitive(true);
    Ok(value)
}
fn check_status(response: &Response, expected: &[StatusCode]) -> Result<(), HttpError> {
    let status = response.status();
    if expected.contains(&status) {
        return Ok(());
    }
    let retryable = status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
    let (code, message) = match status {
        StatusCode::NOT_FOUND => (
            "link_unavailable",
            "Приглашение истекло, отменено или недоступно",
        ),
        StatusCode::CONFLICT => (
            "link_conflict",
            "Приглашение уже принято или данные операции изменились",
        ),
        StatusCode::TOO_MANY_REQUESTS => (
            "helper_rate_limited",
            "Помощник временно ограничил запросы. Подождите и повторите попытку",
        ),
        _ if status.is_redirection() => ("helper_redirect", "Перенаправление помощника отклонено"),
        _ if status.is_server_error() => (
            "helper_unavailable",
            "Помощник подключения временно недоступен",
        ),
        _ => ("helper_response", "Помощник отклонил запрос"),
    };
    Err(HttpError {
        error: AppError::new(code, "helper", message, retryable),
        retry_after: response
            .headers()
            .get(header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(retry_after),
    })
}
fn retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    DateTime::parse_from_rfc2822(value).ok().map(|date| {
        Duration::from_secs(date.signed_duration_since(Utc::now()).num_seconds().max(0) as u64)
    })
}
async fn read_json<T: DeserializeOwned>(mut response: Response) -> Result<T, HttpError> {
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if content_type.split(';').next().map(str::trim) != Some("application/json") {
        return Err(malformed());
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(oversized());
    }
    let mut bytes = zeroize::Zeroizing::new(Vec::new());
    while let Some(chunk) = response.chunk().await.map_err(|_| transport())? {
        if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) {
            return Err(oversized());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| malformed())
}
fn oversized() -> HttpError {
    AppError::new(
        "helper_response_size",
        "helper",
        "Ответ помощника превышает допустимый размер",
        false,
    )
    .into()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
    };

    pub(crate) fn token(byte: u8) -> String {
        URL_SAFE_NO_PAD.encode([byte; 32])
    }
    fn captured_header(request: &[u8], name: &str) -> Option<String> {
        String::from_utf8_lossy(request).lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name)
                .then(|| value.trim().to_owned())
        })
    }
    pub(crate) fn json_response(status: u16, value: serde_json::Value) -> String {
        let body = value.to_string();
        format!("HTTP/1.1 {status} response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
    }
    pub(crate) fn receipt(origin: &str) -> serde_json::Value {
        serde_json::json!({"token":token(1),"creator_token":token(2),"invitation_url":format!("{origin}/i/{}",token(1)),"expires_at":Utc::now()+chrono::Duration::minutes(5)})
    }
    pub(crate) struct Mock {
        pub(crate) service: Service,
        pub(crate) requests: Arc<Mutex<Vec<Vec<u8>>>>,
        pub(crate) done: JoinHandle<()>,
    }
    pub(crate) async fn mock(build: impl FnOnce(&str) -> Vec<String>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let replies = build(&origin);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let capture = requests.clone();
        let done = tokio::spawn(async move {
            for reply in replies {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut data = Vec::new();
                let mut buf = [0u8; 4096];
                let end = loop {
                    let count = stream.read(&mut buf).await.unwrap();
                    if count == 0 {
                        panic!("client closed request")
                    };
                    data.extend_from_slice(&buf[..count]);
                    if let Some(at) = data.windows(4).position(|v| v == b"\r\n\r\n") {
                        break at + 4;
                    }
                    assert!(data.len() < 16384);
                };
                let headers = String::from_utf8_lossy(&data[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|value| value.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while data.len() < end + length {
                    let count = stream.read(&mut buf).await.unwrap();
                    if count == 0 {
                        break;
                    }
                    data.extend_from_slice(&buf[..count]);
                }
                capture.lock().unwrap().push(data);
                let _ = stream.write_all(reply.as_bytes()).await;
                let _ = stream.shutdown().await;
            }
        });
        Mock {
            service: Service::new(&origin).unwrap(),
            requests,
            done,
        }
    }
    #[test]
    fn origin_and_link_validation_rejects_untrusted_targets_and_noncanonical_paths() {
        for valid in [
            "https://connect.example.test",
            "https://connect.example.test/",
            "http://localhost:8080",
            "http://127.0.0.1:8080",
            "http://[::1]:8080",
        ] {
            assert!(Service::new(valid).is_ok());
        }
        for invalid in [
            "http://connect.example.test",
            "ftp://connect.example.test",
            "https://user:secret@example.test",
            "https://@connect.example.test",
            "https:connect.example.test",
            "https://example.test/path",
            "https://example.test?",
            "https://example.test#",
            "https://:443",
            "https://example.test:65536",
            "http://127.0.0.1.example.test",
            "http://127.0.\n0.1:8080",
        ] {
            assert!(Service::new(invalid).is_err(), "{invalid}");
        }
        let service = Service::new("https://connect.example.test").unwrap();
        let good = format!("https://connect.example.test/i/{}", token(1));
        assert_eq!(
            service.parse_link(&format!("  {good}\n")).unwrap(),
            token(1)
        );
        for invalid in [
            format!("{good}?"),
            format!("{good}#"),
            format!("{good}/"),
            format!("{good}/extra"),
            good.replace("https:", "http:"),
            good.replace("connect.example.test", "evil.example.test"),
            good.replace("/i/", "/i/other/../"),
            good.replace("/i/", "/i/%2e/"),
            good.replace("/i/", "\\i/"),
            good.replace("/i/", "/i/%41"),
            good.replace("/i/", "/v1/"),
            good.replace("https://", "https://@"),
        ] {
            assert!(service.parse_link(&invalid).is_err(), "{invalid}");
        }
    }
    #[tokio::test]
    async fn creation_repeats_identical_body_and_sensitive_header() {
        let m = mock(|origin| {
            let r = json_response(201, receipt(origin));
            vec![r.clone(), r]
        })
        .await;
        let a = m.service.create("v2:offer", 300, &token(3)).await.unwrap();
        let b = m.service.create("v2:offer", 300, &token(3)).await.unwrap();
        assert_eq!(a.token, b.token);
        assert_eq!(a.creator_token, b.creator_token);
        assert_eq!(a.expires_at, b.expires_at);
        m.done.await.unwrap();
        let requests = m.requests.lock().unwrap();
        assert_eq!(requests[0], requests[1]);
        let wire = String::from_utf8_lossy(&requests[0]);
        assert!(wire.starts_with("POST /v1/invitations HTTP/1.1"));
        assert_eq!(
            captured_header(&requests[0], "Idempotency-Key"),
            Some(token(3))
        );
        assert!(wire.ends_with("{\"offer\":\"v2:offer\",\"ttl_seconds\":300}"));
    }
    #[tokio::test]
    async fn offer_answer_poll_and_cancel_use_the_expected_contract() {
        let expires = Utc::now() + chrono::Duration::minutes(5);
        let m=mock(|_|vec![json_response(200,serde_json::json!({"offer":"v2:offer","expires_at":expires})),"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into(),json_response(202,serde_json::json!({"state":"waiting","expires_at":expires})),json_response(200,serde_json::json!({"state":"answered","answer":"v2:answer","expires_at":expires})),"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()]).await;
        let offer = m.service.fetch_offer(&token(1)).await.unwrap();
        assert_eq!(offer.offer, "v2:offer");
        assert_eq!(offer.expires_at, expires);
        m.service.send_answer(&token(1), "v2:answer").await.unwrap();
        assert_eq!(
            m.service.poll_answer(&token(1), &token(2)).await.unwrap(),
            None
        );
        assert_eq!(
            m.service
                .poll_answer(&token(1), &token(2))
                .await
                .unwrap()
                .as_deref(),
            Some("v2:answer")
        );
        m.service.cancel(&token(1), &token(2)).await.unwrap();
        m.done.await.unwrap();
        let wires = m.requests.lock().unwrap();
        for request in &wires[2..] {
            assert_eq!(
                captured_header(request, "Authorization"),
                Some(format!("Bearer {}", token(2)))
            );
        }
        assert!(!String::from_utf8_lossy(&wires[0]).contains("Bearer"));
    }
    #[tokio::test]
    async fn status_errors_are_safe_and_retryable_only_when_appropriate() {
        for (status, code, retryable) in [
            (404, "link_unavailable", false),
            (409, "link_conflict", false),
            (429, "helper_rate_limited", true),
            (503, "helper_unavailable", true),
            (403, "helper_response", false),
        ] {
            let m=mock(|_|vec![format!("HTTP/1.1 {status} error\r\nRetry-After: 60\r\nContent-Length: 22\r\nConnection: close\r\n\r\nSECRET_UNTRUSTED_BODY")]).await;
            let error = m.service.fetch_offer(&token(1)).await.err().unwrap();
            assert_eq!(error.error.code, code);
            assert_eq!(error.error.retryable, retryable);
            assert_eq!(error.retry_after, Some(Duration::from_secs(60)));
            assert!(!format!("{error:?}").contains("SECRET_UNTRUSTED_BODY"));
            m.done.await.unwrap();
        }
        assert_eq!(retry_after("junk"), None);
        let date = (Utc::now() + chrono::Duration::seconds(90))
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        assert!(retry_after(&date).unwrap().as_secs() >= 88);
    }
    #[tokio::test]
    async fn redirects_are_not_followed() {
        let m=mock(|_|vec!["HTTP/1.1 302 Found\r\nLocation: http://evil.invalid/steal\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()]).await;
        let error = m
            .service
            .create("v2:offer", 300, &token(3))
            .await
            .err()
            .unwrap();
        assert_eq!(error.error.code, "helper_redirect");
        assert!(!error.error.retryable);
        m.done.await.unwrap();
        assert_eq!(m.requests.lock().unwrap().len(), 1);
    }
    #[tokio::test]
    async fn malicious_receipts_cannot_change_origin_token_or_expiry() {
        for field in ["invitation_url", "token", "creator_token", "expires_at"] {
            let m = mock(|origin| {
                let mut value = receipt(origin);
                value[field] = match field {
                    "invitation_url" => {
                        serde_json::json!(format!("https://evil.invalid/i/{}", token(1)))
                    }
                    "token" => serde_json::json!(token(7)),
                    "creator_token" => serde_json::json!("invalid"),
                    _ => serde_json::json!(Utc::now() - chrono::Duration::minutes(1)),
                };
                vec![json_response(201, value)]
            })
            .await;
            assert_eq!(
                m.service
                    .create("v2:offer", 300, &token(3))
                    .await
                    .err()
                    .unwrap()
                    .error
                    .code,
                "helper_response"
            );
            m.done.await.unwrap();
        }
    }
    #[tokio::test]
    async fn oversized_content_length_and_chunked_bodies_are_bounded() {
        let large = "a".repeat(MAX_RESPONSE_BYTES + 1);
        for response in [format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",MAX_RESPONSE_BYTES+1),format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n{:x}\r\n{large}\r\n0\r\n\r\n",large.len())] {
            let m=mock(|_|vec![response]).await;assert_eq!(m.service.fetch_offer(&token(1)).await.err().unwrap().error.code,"helper_response_size");m.done.await.unwrap();
        }
    }
    #[tokio::test]
    async fn malformed_and_oversized_payloads_and_pending_answers_are_rejected() {
        let expires = Utc::now() + chrono::Duration::minutes(5);
        let m=mock(|_|vec![json_response(200,serde_json::json!({"offer":"a".repeat(MAX_PAYLOAD_BYTES+1),"expires_at":expires})),json_response(202,serde_json::json!({"state":"answered","answer":"v2:bad","expires_at":expires})),json_response(200,serde_json::json!({"state":"waiting","expires_at":expires})),"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello".into()]).await;
        assert_eq!(
            m.service
                .fetch_offer(&token(1))
                .await
                .err()
                .unwrap()
                .error
                .code,
            "helper_response"
        );
        for _ in 0..2 {
            assert_eq!(
                m.service
                    .poll_answer(&token(1), &token(2))
                    .await
                    .err()
                    .unwrap()
                    .error
                    .code,
                "helper_response"
            );
        }
        assert_eq!(
            m.service
                .fetch_offer(&token(1))
                .await
                .err()
                .unwrap()
                .error
                .code,
            "helper_response"
        );
        m.done.await.unwrap();
    }
    #[tokio::test]
    async fn stalled_response_times_out_without_leaking_request_url() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(10)).await;
        });
        let service = Service::new(&origin).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(8), service.fetch_offer(&token(1)))
            .await
            .unwrap()
            .err()
            .unwrap();
        assert_eq!(error.error.code, "helper_unavailable");
        assert!(error.error.retryable);
        assert!(!format!("{error:?}").contains(&token(1)));
        server.abort();
    }
}
