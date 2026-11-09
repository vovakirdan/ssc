use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(1);
fn directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "ssc-access-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    storage::prepare(&path).unwrap();
    path
}
fn new_identity(origin: &str) -> Identity {
    Identity {
        version: 1,
        helper_origin: origin.into(),
        seed: B64.encode([7u8; 32]),
        activated: false,
    }
}

#[test]
fn identity_survives_reload_and_requires_original_origin() {
    let path = directory();
    let first = new_identity("https://connect.vovakirdan.ru");
    save(&path, &first).unwrap();
    let loaded = identity(&path, &first.helper_origin).unwrap().unwrap();
    assert_eq!(loaded.device_id().unwrap(), first.device_id().unwrap());
    assert!(!loaded.activated);
    assert_eq!(
        identity(&path, "https://another.example")
            .err()
            .unwrap()
            .code,
        "access_origin"
    );
    std::fs::remove_dir_all(path).unwrap();
}
#[cfg(unix)]
#[test]
fn storage_rejects_symlinks_public_files_and_oversized_data() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let path = directory();
    let file = path.join("file");
    storage::write(&file, b"private", false).unwrap();
    assert_eq!(
        std::fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let link = path.join("link");
    symlink(&file, &link).unwrap();
    assert!(storage::read(&link, false).is_err());
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(storage::read(&file, false).is_err());
    storage::write(&file, &vec![0; 16 * 1024 + 1], false).unwrap();
    assert!(storage::read(&file, false).is_err());
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn credentials_are_bounded_and_must_not_be_expired_or_non_turn() {
    let make = |url: &str, ttl: i64| CredentialResponse {
        expires_at: Utc::now() + chrono::Duration::seconds(ttl),
        ice_servers: vec![IceServer {
            urls: vec![url.into()],
            username: "temporary".into(),
            credential: "password".into(),
        }],
    };
    assert!(validate_credentials(make("https://evil.example", 600)).is_err());
    assert!(validate_credentials(make("turn:turn.vovakirdan.ru:3478?transport=udp", -1)).is_err());
    assert!(
        validate_credentials(make("turn:turn.vovakirdan.ru:3478?transport=udp", 1200)).is_err()
    );
    let valid =
        validate_credentials(make("turn:turn.vovakirdan.ru:3478?transport=udp", 600)).unwrap();
    assert_eq!(valid[0].r#type, "turn");
}
#[test]
fn activation_proof_binds_origin_token_and_public_key() {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let identity = new_identity("https://connect.vovakirdan.ru");
    let key = identity.key().unwrap();
    let public = B64.encode(key.public_key().as_ref());
    let message = format!(
        "ssc-access-v1\nactivate\n{}\n{}\n{public}\n",
        identity.helper_origin,
        B64.encode([3u8; 32])
    );
    let signature = key.sign(message.as_bytes());
    let verifier = UnparsedPublicKey::new(&ED25519, key.public_key().as_ref());
    verifier
        .verify(message.as_bytes(), signature.as_ref())
        .unwrap();
    assert!(verifier
        .verify(
            message
                .replace("connect.vovakirdan.ru", "other.example")
                .as_bytes(),
            signature.as_ref()
        )
        .is_err());
}

async fn mock_http<F>(count: usize, mut handler: F) -> (String, tokio::task::JoinHandle<()>)
where
    F: FnMut(&str, serde_json::Value) -> (u16, serde_json::Value) + Send + 'static,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        for _ in 0..count {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 4096];
            let (header_end, length) = loop {
                let n =
                    tokio::time::timeout(std::time::Duration::from_secs(5), stream.read(&mut buf))
                        .await
                        .unwrap()
                        .unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
                if let Some(at) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                    let head = std::str::from_utf8(&bytes[..at]).unwrap();
                    let length = head
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|v| v.parse::<usize>().ok())
                        })
                        .unwrap();
                    break (at + 4, length);
                }
            };
            while bytes.len() < header_end + length {
                let n = stream.read(&mut buf).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let head = std::str::from_utf8(&bytes[..header_end]).unwrap();
            let path = head.split_whitespace().nth(1).unwrap();
            let body = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
            let (status, value) = handler(path, body);
            let body = serde_json::to_vec(&value).unwrap();
            let header=format!("HTTP/1.1 {status} test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&body).await.unwrap();
        }
    });
    (origin, task)
}
fn http_client() -> Client {
    Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
}
#[tokio::test]
async fn activation_retries_the_durably_stored_key_and_removes_only_consumed_profile() {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let path = directory();
    let mut first_public = None;
    let origin_state = std::sync::Arc::new(Mutex::new(String::new()));
    let server_origin = origin_state.clone();
    let mut attempt = 0;
    let (origin, server) = mock_http(2, move |route, body| {
        assert_eq!(route, "/v1/access/activate");
        let public = body["public_key"].as_str().unwrap();
        let key = B64.decode(public).unwrap();
        let token = body["token"].as_str().unwrap();
        let signature = B64.decode(body["signature"].as_str().unwrap()).unwrap();
        let origin = server_origin.lock().unwrap();
        let message = format!("ssc-access-v1\nactivate\n{origin}\n{token}\n{public}\n");
        UnparsedPublicKey::new(&ED25519, &key)
            .verify(message.as_bytes(), &signature)
            .unwrap();
        attempt += 1;
        if let Some(ref first) = first_public {
            assert_eq!(public, first);
        } else {
            first_public = Some(public.to_string());
        }
        if attempt == 1 {
            (503, serde_json::json!({"error":"busy"}))
        } else {
            (
                200,
                serde_json::json!({"state":"active","device_id":B64.encode(Sha256::digest(key))}),
            )
        }
    })
    .await;
    *origin_state.lock().unwrap() = origin.clone();
    let bootstrap = serde_json::json!({"version":1,"helper_origin":origin,"bootstrap_token":B64.encode([9u8;32]),"expires_at":Utc::now()+chrono::Duration::days(1)});
    storage::write(
        &path.join("bootstrap.json"),
        &serde_json::to_vec(&bootstrap).unwrap(),
        false,
    )
    .unwrap();
    let client = http_client();
    assert_eq!(
        activate(&path, &origin, &client).await.err().unwrap().code,
        "access_unavailable"
    );
    let pending = identity(&path, &origin).unwrap().unwrap();
    assert!(!pending.activated);
    assert!(path.join("bootstrap.json").exists());
    let expected = pending.device_id().unwrap();
    let active = activate(&path, &origin, &client).await.unwrap();
    assert_eq!(active.device_id().unwrap(), expected);
    assert!(identity(&path, &origin).unwrap().unwrap().activated);
    assert!(path.join("access.active").is_file());
    assert!(!path.join("bootstrap.json").exists());
    // Restart: completed activation needs no bootstrap and no extra network request.
    assert!(activate(&path, &origin, &client).await.unwrap().activated);
    server.await.unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn credentials_sign_server_challenge_and_keep_secrets_out_of_status() {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let path = directory();
    let mut seed = new_identity("");
    seed.activated = true;
    let pubkey = seed.key().unwrap().public_key().as_ref().to_vec();
    let id = seed.device_id().unwrap();
    let expected_id = id.clone();
    let nonce = B64.encode([8u8; 72]);
    let expected_nonce = nonce.clone();
    let origin_state = std::sync::Arc::new(Mutex::new(String::new()));
    let server_origin = origin_state.clone();
    let (origin,server)=mock_http(2,move|route,body| {
        assert_eq!(body["device_id"],expected_id);
        if route=="/v1/access/challenge" { return(200,serde_json::json!({"challenge":nonce,"expires_at":Utc::now()+chrono::Duration::seconds(60)})); }
        assert_eq!(route,"/v1/turn/credentials");assert_eq!(body["challenge"],expected_nonce);
        let message=format!("ssc-access-v1\nturn\n{}\n{expected_id}\n{expected_nonce}\n",server_origin.lock().unwrap());let sig=B64.decode(body["signature"].as_str().unwrap()).unwrap();UnparsedPublicKey::new(&ED25519,&pubkey).verify(message.as_bytes(),&sig).unwrap();
        (200,serde_json::json!({"ice_servers":[{"urls":["turn:turn.vovakirdan.ru:3478?transport=udp"],"username":"short-lived","credential":"sensitive-password"}],"expires_at":Utc::now()+chrono::Duration::seconds(600)}))
    }).await;
    *origin_state.lock().unwrap() = origin.clone();
    seed.helper_origin = origin.clone();
    save(&path, &seed).unwrap();
    let servers = credentials(&path, &origin, &http_client()).await.unwrap();
    assert_eq!(servers[0].credential.as_deref(), Some("sensitive-password"));
    let wire = serde_json::to_string(&AccessStatus {
        state: "active",
        access_reference: Some(id.clone()),
        error: None,
    })
    .unwrap();
    assert!(wire.contains(&id));
    assert!(!wire.contains("sensitive"));
    assert!(!wire.contains(&seed.seed));
    server.await.unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn wrong_origin_expired_and_malformed_profiles_never_generate_or_send_a_key() {
    let path = directory();
    let origin = "http://127.0.0.1:1";
    for (profile_origin, expiry, code) in [
        (
            "https://evil.example",
            Utc::now() + chrono::Duration::days(1),
            "access_origin",
        ),
        (
            origin,
            Utc::now() - chrono::Duration::seconds(1),
            "access_activation_expired",
        ),
    ] {
        storage::write(&path.join("bootstrap.json"),&serde_json::to_vec(&serde_json::json!({"version":1,"helper_origin":profile_origin,"bootstrap_token":B64.encode([4u8;32]),"expires_at":expiry})).unwrap(),false).unwrap();
        assert_eq!(
            activate(&path, origin, &http_client())
                .await
                .err()
                .unwrap()
                .code,
            code
        );
        assert!(!path.join("access.key").exists());
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn concurrent_windows_share_an_os_lock_and_crash_drop_releases_it() {
    let path = directory();
    let held = storage::lock(&path).unwrap();
    assert_eq!(storage::lock(&path).err().unwrap().code, "access_busy");
    drop(held);
    let recovered = storage::lock(&path).unwrap();
    drop(recovered);
    std::fs::remove_dir_all(path).unwrap();
}
#[cfg(windows)]
#[test]
fn windows_identity_is_dpapi_encrypted_and_corruption_is_not_plaintext_fallback() {
    let path = directory();
    let first = new_identity("https://connect.vovakirdan.ru");
    save(&path, &first).unwrap();
    let raw = std::fs::read(path.join("access.key")).unwrap();
    assert!(!raw
        .windows(first.seed.len())
        .any(|window| window == first.seed.as_bytes()));
    assert_eq!(
        identity(&path, &first.helper_origin)
            .unwrap()
            .unwrap()
            .device_id()
            .unwrap(),
        first.device_id().unwrap()
    );
    std::fs::write(path.join("access.key"), b"not a DPAPI blob").unwrap();
    assert!(identity(&path, &first.helper_origin).is_err());
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn stateless_challenge_requires_full_nonce_expiry_mac_and_canonical_encoding() {
    let full = B64.encode([4u8; 72]);
    assert!(valid_challenge(&full));
    assert!(!valid_challenge(&B64.encode([4u8; 32])));
    assert!(!valid_challenge(&format!("{full}=")));
    assert!(!valid_challenge(&"!".repeat(96)));
}

#[test]
fn managed_turn_accepts_only_exact_configured_project_endpoints() {
    let valid = project_turn_urls("turn.vovakirdan.ru").unwrap();
    let make = |urls: Vec<String>| CredentialResponse {
        expires_at: Utc::now() + chrono::Duration::seconds(600),
        ice_servers: vec![IceServer {
            urls,
            username: "temporary".into(),
            credential: "password".into(),
        }],
    };
    let configs = validate_credentials(make(valid.to_vec())).unwrap();
    assert_eq!(configs.len(), 3);
    assert_eq!(
        configs.iter().map(|c| c.url.clone()).collect::<Vec<_>>(),
        valid
    );
    for url in [
        "turn:127.0.0.1:3478?transport=udp",
        "turn:10.0.0.1:3478?transport=udp",
        "turn:169.254.169.254:3478?transport=udp",
        "turn:evil.example:3478?transport=udp",
        "turn:turn.vovakirdan.ru.evil.example:3478?transport=udp",
        "turn:turn.vovakirdan.ru:22?transport=udp",
        "turn:turn.vovakirdan.ru:3478?transport=tcp&extra=1",
        "turn:turn.vovakirdan.ru:3478?transport=udp#fragment",
        "turn:user@turn.vovakirdan.ru:3478?transport=udp",
        "turn:turn.vovakirdan.ru:3478",
        "turns:turn.vovakirdan.ru:5349?transport=udp",
        "turns:turn.vovakirdan.ru:443?transport=tcp",
    ] {
        assert!(
            validate_credentials(make(vec![url.into()])).is_err(),
            "accepted unexpected relay endpoint"
        );
    }
}
#[test]
fn turn_host_override_is_a_hostname_not_a_uri_or_request_fragment() {
    assert_eq!(
        project_turn_urls("relay.example.net").unwrap()[2],
        "turns:relay.example.net:5349?transport=tcp"
    );
    for host in [
        "",
        "https://relay.example",
        "relay.example:3478",
        "user@relay.example",
        "relay.example?x=1",
        "relay.example/",
        "relay.example\n",
        ".example",
        "bad..example",
        "-bad.example",
        "bad-.example",
    ] {
        assert!(project_turn_urls(host).is_err());
    }
}

#[test]
fn status_exposes_only_an_active_operator_reference_without_key_material() {
    let mut active = new_identity("https://connect.vovakirdan.ru");
    active.activated = true;
    let expected = active.device_id().unwrap();
    let seed = active.seed.clone();
    let public = B64.encode(active.key().unwrap().public_key().as_ref());
    let wire = serde_json::to_value(access_status(Ok(Some(active)))).unwrap();
    assert_eq!(wire["state"], "active");
    assert_eq!(wire["access_reference"], expected);
    let serialized = wire.to_string();
    assert!(!serialized.contains(&seed));
    assert!(!serialized.contains(&public));
    for result in [
        Ok(None),
        Ok(Some(new_identity("https://connect.vovakirdan.ru"))),
        Err(error("access_denied", "Доступ отклонён", true)),
    ] {
        let wire = serde_json::to_value(access_status(result)).unwrap();
        assert!(wire.get("access_reference").is_none());
    }
}
#[test]
fn local_reset_deletes_only_access_material_preserves_other_data_and_can_repeat() {
    let path = directory();
    let mut old = new_identity("https://connect.vovakirdan.ru");
    old.activated = true;
    save(&path, &old).unwrap();
    storage::write(&path.join("access.active"), b"1\n", false).unwrap();
    storage::write(&path.join("bootstrap.json"), b"old activation", false).unwrap();
    storage::write(
        &path.join("saved-settings.json"),
        b"custom turn settings",
        false,
    )
    .unwrap();
    std::fs::create_dir(path.join("webview")).unwrap();
    std::fs::write(path.join("webview").join("history"), b"unrelated data").unwrap();
    reset_files(&path).unwrap();
    for name in ["access.key", "access.active", "bootstrap.json"] {
        assert!(!path.join(name).exists());
    }
    assert!(identity(&path, &old.helper_origin).unwrap().is_none());
    assert_eq!(
        std::fs::read(path.join("saved-settings.json")).unwrap(),
        b"custom turn settings"
    );
    assert_eq!(
        std::fs::read(path.join("webview/history")).unwrap(),
        b"unrelated data"
    );
    assert!(path.join(".access.lock").is_file());
    reset_files(&path).unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
#[cfg(unix)]
#[test]
fn local_reset_rejects_profile_redirection_and_never_follows_access_symlinks() {
    use std::os::unix::fs::symlink;
    let root = directory();
    let actual = root.join("actual");
    storage::prepare(&actual).unwrap();
    storage::write(&actual.join("outside"), b"must remain", false).unwrap();
    symlink(actual.join("outside"), actual.join("access.key")).unwrap();
    let redirected = root.join("redirected");
    symlink(&actual, &redirected).unwrap();
    assert!(reset_files(&redirected).is_err());
    assert!(actual.join("access.key").is_symlink());
    reset_files(&actual).unwrap();
    assert!(!actual.join("access.key").is_symlink());
    assert_eq!(
        std::fs::read(actual.join("outside")).unwrap(),
        b"must remain"
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn manually_entered_code_retries_after_lost_reply_and_restart_without_storing_token() {
    use ring::signature::{UnparsedPublicKey, ED25519};
    let path = directory();
    let mut first_public = None;
    let mut attempt = 0;
    let origin_state = std::sync::Arc::new(Mutex::new(String::new()));
    let server_origin = origin_state.clone();
    let token = B64.encode([19u8; 32]);
    let expected_token = token.clone();
    let (origin,server)=mock_http(2,move|route,body| {
        assert_eq!(route,"/v1/access/activate");assert_eq!(body["token"],expected_token);
        let public=body["public_key"].as_str().unwrap();let pubbytes=B64.decode(public).unwrap();let signature=B64.decode(body["signature"].as_str().unwrap()).unwrap();
        let message=format!("ssc-access-v1\nactivate\n{}\n{}\n{public}\n",server_origin.lock().unwrap(),expected_token);
        UnparsedPublicKey::new(&ED25519,&pubbytes).verify(message.as_bytes(),&signature).unwrap();
        if let Some(ref previous)=first_public {assert_eq!(public,previous);} else {first_public=Some(public.to_owned());}
        attempt+=1;
        if attempt==1 {(503,serde_json::json!({"error":"reply_lost"}))}
        else {(200,serde_json::json!({"state":"active","device_id":B64.encode(Sha256::digest(pubbytes))}))}
    }).await;
    *origin_state.lock().unwrap() = origin.clone();
    let client = http_client();
    assert_eq!(
        activate_manual(&path, &origin, &client, &format!(" \n{token}\t "))
            .await
            .err()
            .unwrap()
            .code,
        "access_unavailable"
    );
    assert!(!path.join("bootstrap.json").exists());
    assert!(!path.join("access.active").exists());
    let pending = identity(&path, &origin).unwrap().unwrap();
    assert!(!pending.activated);
    let expected = pending.device_id().unwrap();
    // Reload from disk and re-enter the same code, just as after restarting the app.
    let active = activate_manual(&path, &origin, &client, &token)
        .await
        .unwrap();
    assert_eq!(active.device_id().unwrap(), expected);
    assert!(identity(&path, &origin).unwrap().unwrap().activated);
    assert!(path.join("access.active").exists());
    assert!(!path.join("bootstrap.json").exists());
    assert_eq!(
        activate_manual(&path, &origin, &client, &token)
            .await
            .err()
            .unwrap()
            .code,
        "access_already_active"
    );
    assert_eq!(
        identity(&path, &origin)
            .unwrap()
            .unwrap()
            .device_id()
            .unwrap(),
        expected
    );
    server.await.unwrap();
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn malformed_manual_codes_never_create_keys_or_contact_service() {
    let path = directory();
    let origin = "http://127.0.0.1:1";
    for code in [
        "",
        "short",
        &"A".repeat(42),
        &format!("{}=", B64.encode([1u8; 32])),
        &"a".repeat(43),
    ] {
        assert_eq!(
            activate_manual(&path, origin, &http_client(), code)
                .await
                .err()
                .unwrap()
                .code,
            "access_code_invalid"
        );
        assert!(!path.join("access.key").exists());
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn server_denied_expired_used_or_conflicting_manual_codes_leave_pending_identity_retryable() {
    let path = directory();
    let token = B64.encode([24u8; 32]);
    for (status, reason) in [(403, "expired"), (403, "used"), (409, "conflict")] {
        let (origin, server) =
            mock_http(1, move |_, _| (status, serde_json::json!({"error":reason}))).await;
        // Use separate configured origins/profiles to avoid mutating any previous identity.
        let profile = path.join(status.to_string() + reason);
        storage::prepare(&profile).unwrap();
        assert_eq!(
            activate_manual(&profile, &origin, &http_client(), &token)
                .await
                .err()
                .unwrap()
                .code,
            "access_denied"
        );
        assert!(!identity(&profile, &origin).unwrap().unwrap().activated);
        assert!(!profile.join("access.active").exists());
        assert!(!profile.join("bootstrap.json").exists());
        server.await.unwrap();
    }
    std::fs::remove_dir_all(path).unwrap();
}
#[tokio::test]
async fn manual_activation_obeys_cross_process_lock_without_replacing_identity() {
    let path = directory();
    let held = storage::lock(&path).unwrap();
    assert_eq!(
        activate_manual(
            &path,
            "http://127.0.0.1:1",
            &http_client(),
            &B64.encode([20u8; 32])
        )
        .await
        .err()
        .unwrap()
        .code,
        "access_busy"
    );
    assert!(!path.join("access.key").exists());
    drop(held);
    std::fs::remove_dir_all(path).unwrap();
}
