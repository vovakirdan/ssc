use super::*;
use crate::connect::http::tests::{json_response, mock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

static GLOBAL: Lazy<tokio::sync::Mutex<()>> = Lazy::new(|| tokio::sync::Mutex::new(()));
fn tok(n: u8) -> String {
    B64.encode([n; 32])
}
fn operation(origin: &str, generation: u64) -> Arc<Operation> {
    Arc::new(Operation {
        generation,
        origin: origin.into(),
        client: Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
            .unwrap(),
        connected: AtomicBool::new(false),
        data: Mutex::new(Data {
            id: tok(1),
            cap: Zeroizing::new(String::new()),
            timing: Deadlines::new(Utc::now(), Instant::now()),
            state_rank: 0,
            retired: false,
            snapshot: LinkSnapshot {
                generation,
                role: Role::Creator,
                phase: LinkPhase::Creating,
                url: None,
                expires_at: None,
                error: None,
            },
        }),
    })
}
fn metadata(state: &str, expires: Option<DateTime<Utc>>) -> Value {
    json!({"id":tok(1),"state":state,"join_expires_at":Utc::now()+chrono::Duration::minutes(5),"expires_at":expires})
}
fn meta(now: DateTime<Utc>, minutes: i64) -> Meta {
    Meta {
        id: tok(1),
        state: "waiting".into(),
        join_expires_at: now + chrono::Duration::minutes(minutes),
        expires_at: None,
    }
}
#[test]
fn room_links_require_exact_origin_canonical_path_and_secret() {
    let id = tok(1);
    let secret = tok(2);
    let link = format!("https://connect.example/r/{id}#k={secret}");
    assert!(parse_link("https://connect.example", &link).is_ok());
    for bad in [
        link.replace("connect.example", "evil.example"),
        link.replace("https:", "http:"),
        link.replace("connect.example", "user@connect.example"),
        link.replace("connect.example", "connect.example:444"),
        format!("{link}&extra=x"),
        link.replace("/r/", "/i/"),
        link.replace("#k=", "?k="),
        link.replace("#k=", "?extra=x#k="),
        link.replace("/r/", "/r/../r/"),
        link.replace("/r/", "//r/"),
        link.replace("/r/", "/r/%41"),
        format!("{link}="),
        format!("https://connect.example/r/{id}/#k={secret}"),
        format!("https://connect.example/r/{id}#k={}", "-".repeat(43)),
    ] {
        assert!(
            parse_link("https://connect.example", &bad).is_err(),
            "accepted noncanonical link"
        );
    }
    assert!(!is_link(&format!("https://connect.example/i/{id}")));
}
#[test]
fn waiting_deadline_never_extends_on_metadata_or_wall_clock_rollback() {
    let now = Utc::now();
    let mono = Instant::now();
    let mut clock = Deadlines::new(now, mono);
    clock.update(&meta(now, 2), now, mono).unwrap();
    let pinned = clock.mono;
    clock
        .update(
            &meta(now, 50),
            now + chrono::Duration::seconds(10),
            mono + Duration::from_secs(10),
        )
        .unwrap();
    assert_eq!(clock.mono, pinned);
    assert_eq!(clock.wall, now + chrono::Duration::minutes(2));
    clock
        .update(
            &meta(now, 50),
            now - chrono::Duration::hours(1),
            mono + Duration::from_secs(20),
        )
        .unwrap();
    assert_eq!(clock.mono, pinned);
    assert!(clock.expired(now - chrono::Duration::hours(1), pinned));
}
#[test]
fn corrected_wall_clock_does_not_end_or_shorten_a_pinned_room() {
    let now = Utc::now();
    let mono = Instant::now();
    let mut clock = Deadlines::new(now, mono);
    clock.update(&meta(now, 2), now, mono).unwrap();
    let pinned = clock.mono;
    let corrected = now + chrono::Duration::hours(3);
    assert!(!clock.expired(corrected, mono + Duration::from_secs(10)));
    clock
        .update(&meta(now, 2), corrected, mono + Duration::from_secs(10))
        .unwrap();
    assert_eq!(clock.mono, pinned);
}
#[test]
fn connected_deadline_pins_one_hour_and_the_absolute_maximum() {
    let now = Utc::now();
    let mono = Instant::now();
    let mut clock = Deadlines::new(now, mono);
    let mut connected = meta(now, 5);
    connected.state = "connected".into();
    connected.expires_at = Some(now + chrono::Duration::hours(12));
    clock
        .update(
            &connected,
            now + chrono::Duration::minutes(4),
            mono + Duration::from_secs(240),
        )
        .unwrap();
    assert_eq!(clock.mono, mono + Duration::from_secs(64 * 60));
    let pinned = clock.mono;
    let pinned_wall = clock.wall;
    clock
        .update(
            &connected,
            now + chrono::Duration::minutes(50),
            mono + Duration::from_secs(50 * 60),
        )
        .unwrap();
    assert_eq!(clock.mono, pinned);
    assert_eq!(clock.wall, pinned_wall);
    clock
        .update(
            &connected,
            now - chrono::Duration::hours(1),
            mono + Duration::from_secs(55 * 60),
        )
        .unwrap();
    assert_eq!(clock.mono, pinned);
    assert!(clock.mono <= clock.max_mono);
}
#[test]
fn late_connected_metadata_cannot_resurrect_an_expired_join() {
    let now = Utc::now();
    let mono = Instant::now();
    let mut clock = Deadlines::new(now, mono);
    let mut connected = meta(now, 5);
    connected.state = "connected".into();
    connected.expires_at = Some(now + chrono::Duration::hours(1));
    assert_eq!(
        clock
            .update(
                &connected,
                now + chrono::Duration::minutes(6),
                mono + Duration::from_secs(360)
            )
            .unwrap_err()
            .code,
        "room_expired"
    );
}
#[test]
fn room_metadata_rejects_wrong_ids_regression_missing_or_inconsistent_expiry() {
    let room = operation("https://connect.example", 90000);
    for mut value in [
        metadata("unknown", None),
        metadata("connected", None),
        metadata("waiting", Some(Utc::now() + chrono::Duration::hours(1))),
    ] {
        assert_eq!(room.metadata(&value).unwrap_err().code, "room_response");
        value["id"] = json!(tok(2));
        assert!(room.metadata(&value).is_err());
    }
    let mut missing = metadata("waiting", None);
    missing.as_object_mut().unwrap().remove("expires_at");
    assert!(room.metadata(&missing).is_err());
    let mut wrong = metadata("waiting", None);
    wrong["id"] = json!(tok(2));
    assert!(room.metadata(&wrong).is_err());
    room.metadata(&metadata("answered", None)).unwrap();
    assert!(room.metadata(&metadata("joined", None)).is_err());
    room.metadata(&metadata(
        "connected",
        Some(Utc::now() + chrono::Duration::hours(1)),
    ))
    .unwrap();
    assert!(room.metadata(&metadata("answered", None)).is_err());
}
#[tokio::test]
async fn http_retries_preserve_the_idempotency_key_claim_and_authorization() {
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![
            String::new(),
            json_response(503, json!({})),
            json_response(201, json!({"ok":true})),
        ]
    })
    .await;
    let room = operation(&origin, 90001);
    room.receipt(&tok(2), None).unwrap();
    let s = Session::new(room.generation, "a".repeat(32), None);
    let value = room
        .request(
            &s,
            Method::POST,
            &room.path("/join"),
            Some(json!({"invite_secret":tok(3),"claim_key":tok(4)})),
            Some(&tok(5)),
        )
        .await
        .unwrap();
    assert_eq!(value, json!({"ok":true}));
    fixture.done.await.unwrap();
    let requests = lock(&fixture.requests);
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0], requests[1]);
    assert_eq!(requests[1], requests[2]);
    let request = String::from_utf8_lossy(&requests[0]);
    assert!(request
        .to_lowercase()
        .contains(&format!("authorization: bearer {}", tok(2)).to_lowercase()));
    assert!(request
        .lines()
        .next()
        .unwrap()
        .starts_with(&format!("POST /v1/rooms/{}/join ", tok(1))));
    assert!(!request.lines().next().unwrap().contains(&tok(2)));
}
#[tokio::test]
async fn http_status_response_bounds_and_redirects_are_fail_closed_without_retries() {
    for (reply,code) in [
        (json_response(403,json!({"external":tok(7)})),"room_unavailable"),
        ("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n".into(),"room_response"),
        ("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),"room_response"),
        ("HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:9/private\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),"room_unavailable"),
    ]{
        let mut origin=String::new();let fixture=mock(|url|{origin=url.into();vec![reply]}).await;let room=operation(&origin,90002);let s=Session::new(room.generation,"b".repeat(32),None);
        let error=room.request(&s,Method::GET,&room.path(""),None,None).await.unwrap_err();assert_eq!(error.code,code);
        assert!(!serde_json::to_string(&error).unwrap().contains(&tok(7)));fixture.done.await.unwrap();assert_eq!(lock(&fixture.requests).len(),1);
    }
}
async fn hanging_body() -> (
    String,
    tokio::sync::oneshot::Receiver<()>,
    tokio::task::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        let _ = stream.read(&mut bytes).await.unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 100\r\n\r\n",
            )
            .await
            .unwrap();
        let _ = tx.send(());
        std::future::pending::<()>().await;
    });
    (origin, rx, task)
}
#[tokio::test]
async fn session_close_interrupts_an_already_started_response_body() {
    let (origin, ready, server) = hanging_body().await;
    let room = operation(&origin, 90003);
    let s = Session::new(room.generation, "c".repeat(32), None);
    let path = room.path("");
    let requested = room.request(&s, Method::GET, &path, None, None);
    let close = async {
        ready.await.unwrap();
        s.close().await;
    };
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(requested, close)
    })
    .await
    .unwrap();
    assert_eq!(result.unwrap_err().code, "room_closed");
    server.abort();
}
#[tokio::test]
async fn independent_deadline_closes_a_session_while_http_body_is_hung() {
    let _global = GLOBAL.lock().await;
    let (origin, ready, server) = hanging_body().await;
    let room = operation(&origin, 90004);
    let s = Session::new(room.generation, "d".repeat(32), None);
    lock(&room.data).timing.mono = Instant::now() + Duration::from_millis(150);
    *lock(&ROOM) = Some(room.clone());
    let path = room.path("");
    let requested = room.request(&s, Method::GET, &path, None, None);
    let watch = async {
        ready.await.unwrap();
        expiry_watch(s.clone(), room.clone()).await;
    };
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(requested, watch)
    })
    .await
    .unwrap();
    assert!(matches!(
        result.unwrap_err().code.as_str(),
        "room_expired" | "room_closed"
    ));
    assert!(!s.active());
    let data = lock(&room.data);
    assert!(data.retired);
    assert!(data.cap.is_empty());
    assert_eq!(data.snapshot.error.as_ref().unwrap().code, "room_expired");
    drop(data);
    *lock(&ROOM) = None;
    server.abort();
}
#[tokio::test]
async fn retiring_removes_capabilities_sends_authenticated_delete_and_rejects_late_results() {
    let _global = GLOBAL.lock().await;
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![json_response(204, Value::Null)]
    })
    .await;
    let room = operation(&origin, 90005);
    room.receipt(&tok(2), Some("private invitation")).unwrap();
    let s = Session::new(room.generation, "e".repeat(32), None);
    *lock(&ROOM) = Some(room.clone());
    s.close().await;
    fixture.done.await.unwrap();
    assert!(lock(&room.data).cap.is_empty());
    assert!(room.snapshot().url.is_none());
    assert_eq!(
        room.receipt(&tok(3), Some("late")).unwrap_err().code,
        "room_closed"
    );
    assert_eq!(
        room.metadata(&metadata("waiting", None)).unwrap_err().code,
        "room_closed"
    );
    let requests = lock(&fixture.requests);
    assert_eq!(requests.len(), 1);
    let request = String::from_utf8_lossy(&requests[0]);
    assert!(request.starts_with(&format!("DELETE /v1/rooms/{} ", tok(1))));
    assert!(request
        .to_lowercase()
        .contains(&format!("authorization: bearer {}", tok(2)).to_lowercase()));
    *lock(&ROOM) = None;
}
#[tokio::test]
async fn connected_signal_requires_the_matching_fully_verified_native_session() {
    let _global = GLOBAL.lock().await;
    let room = operation("https://connect.example", 90006);
    let s = Session::new(room.generation, "f".repeat(32), None);
    *lock(&ROOM) = Some(room.clone());
    connected(&s);
    assert!(!room.connected.load(Ordering::Acquire));
    let a = ssc_protocol::crypto::KeyExchange::from_seed([1; 32]);
    let b = ssc_protocol::crypto::KeyExchange::from_seed([2; 32]);
    {
        let mut data = lock(&s.data);
        data.crypto = Some(a.finish(b.public_key(), &[7; 32], true).unwrap());
        data.local_ready = true;
        data.peer_ready = true;
    }
    connected(&s);
    assert!(!room.connected.load(Ordering::Acquire));
    lock(&s.data).transport_connected = true;
    let other = Session::new(room.generation + 1, "0".repeat(32), None);
    connected(&other);
    assert!(!room.connected.load(Ordering::Acquire));
    connected(&s);
    assert!(room.connected.load(Ordering::Acquire));
    room.stage(&s, LinkPhase::Complete);
    assert!(room.snapshot().url.is_none());
    *lock(&ROOM) = None;
}
#[tokio::test]
async fn room_state_contains_only_timing_and_generation_and_old_helpers_remain_independent() {
    let _global = GLOBAL.lock().await;
    let room = operation("https://connect.example", 90007);
    room.receipt(&tok(2), Some("secret invitation")).unwrap();
    let s = Session::new(room.generation, "1".repeat(32), None);
    let previous = lock(&MANAGER).current.replace(s.clone());
    *lock(&ROOM) = Some(room.clone());
    let value = serde_json::to_value(get_temporary_room_state().unwrap()).unwrap();
    assert_eq!(value.as_object().unwrap().len(), 3);
    assert_eq!(value["generation"], s.generation);
    assert!(value["remaining_seconds"]
        .as_u64()
        .is_some_and(|v| v <= 300));
    let serialized = value.to_string();
    assert!(!serialized.contains(&tok(1)));
    assert!(!serialized.contains(&tok(2)));
    assert!(!serialized.contains("secret invitation"));
    assert!(lock(&s.link).is_none());
    lock(&room.data).cap = Zeroizing::new(String::new());
    retire(&s);
    assert!(get_temporary_room_state().is_none());
    *lock(&ROOM) = None;
    lock(&MANAGER).current = previous;
}
#[test]
fn temporary_answers_must_match_the_existing_connection_id() {
    let s = Session::new(90008, "a".repeat(32), None);
    let legacy="v4:H4sIAAAAAAAE/6WOzQ5AMBCE32XPDou49MZriAOl1E+Llot4d0P6BrLJZna/ZGYuciTC8iQ4okORoLqRLUGvQXeqH/Q4zYux67Y7f5yg6qV58W/g4/YveNEI4/eGb8bMd0S6xU9aYzrptTUxqEfhOEkBJVRZ3Q9J1GFGwwAAAA==";
    let mut bundle = ssc_protocol::invitation::dec_bundle(legacy).unwrap();
    bundle.sdp_payload.sdp.sdp_type = ssc_protocol::invitation::SdpType::Answer;
    bundle.sdp_payload.id = "b".repeat(32);
    let encoded = ssc_protocol::invitation::enc_bundle(&bundle).unwrap();
    assert_eq!(
        checked_answer(&s, &encoded).unwrap_err().code,
        "session_mismatch"
    );
    bundle.sdp_payload.id = s.connection_id();
    let encoded = ssc_protocol::invitation::enc_bundle(&bundle).unwrap();
    assert!(checked_answer(&s, &encoded).is_ok());
}

#[tokio::test]
async fn retry_budget_is_exactly_three_requests_even_for_rate_limits() {
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![
            json_response(503, json!({})),
            json_response(429, json!({})),
            json_response(503, json!({})),
        ]
    })
    .await;
    let room = operation(&origin, 90009);
    let s = Session::new(room.generation, "2".repeat(32), None);
    assert_eq!(
        room.request(&s, Method::GET, &room.path(""), None, None)
            .await
            .unwrap_err()
            .code,
        "room_unavailable"
    );
    fixture.done.await.unwrap();
    assert_eq!(lock(&fixture.requests).len(), 3);
}

fn transient_html(status: u16) -> String {
    let body = "<html>temporary proxy response</html>";
    format!("HTTP/1.1 {status} response\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len())
}
#[tokio::test]
async fn poll_retries_non_json_transient_statuses_beyond_the_mutation_retry_budget() {
    let mut origin = String::new();
    let expires = Utc::now() + chrono::Duration::hours(1);
    let response = metadata("connected", Some(expires));
    let fixture = mock(|url| {
        origin = url.into();
        vec![
            transient_html(429),
            transient_html(502),
            transient_html(503),
            transient_html(504),
            json_response(200, response.clone()),
        ]
    })
    .await;
    let room = operation(&origin, 90100);
    room.metadata(&response).unwrap();
    room.receipt(&tok(2), None).unwrap();
    let deadline = lock(&room.data).timing.mono;
    let s = Session::new(room.generation, "3".repeat(32), None);
    lock(&s.data)
        .inbox
        .push(crate::peer::state::ReceivedMessage {
            generation: s.generation,
            id: "1".into(),
            text: "existing conversation".into(),
        });
    let started = Instant::now();
    let value = tokio::time::timeout(Duration::from_secs(20), room.poll(&s, &room.path("")))
        .await
        .unwrap()
        .unwrap();
    assert!(
        started.elapsed() >= POLL_INTERVAL * 4,
        "polling retried faster than the established cadence"
    );
    room.metadata(&value).unwrap();
    assert_eq!(lock(&room.data).timing.mono, deadline);
    assert!(s.active());
    assert_eq!(lock(&s.data).inbox.len(), 1);
    fixture.done.await.unwrap();
    let requests = lock(&fixture.requests);
    assert_eq!(requests.len(), 5);
    for request in requests.iter() {
        let text = String::from_utf8_lossy(request);
        assert!(text.starts_with(&format!("GET /v1/rooms/{} ", tok(1))));
        assert!(text
            .to_lowercase()
            .contains(&format!("authorization: bearer {}", tok(2)).to_lowercase()));
    }
}
#[tokio::test]
async fn poll_never_retries_fatal_statuses_or_invalid_success_responses() {
    for (reply,code) in [
        (transient_html(400),"room_unavailable"),(transient_html(401),"room_unavailable"),(transient_html(404),"room_unavailable"),
        (transient_html(200),"room_response"),
        ("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1\r\nConnection: close\r\n\r\n{".into(),"room_response"),
    ] {
        let mut origin=String::new();let fixture=mock(|url|{origin=url.into();vec![reply,json_response(200,metadata("waiting",None))]}).await;
        let room=operation(&origin,90101);let s=Session::new(room.generation,"4".repeat(32),None);
        let error=room.poll(&s,&room.path("/answer")).await.unwrap_err();assert_eq!(error.code,code);assert_eq!(lock(&fixture.requests).len(),1);fixture.done.abort();
    }
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![
            json_response(200, json!({"id":tok(1),"state":"waiting"})),
            json_response(200, metadata("waiting", None)),
        ]
    })
    .await;
    let room = operation(&origin, 90102);
    let s = Session::new(room.generation, "5".repeat(32), None);
    let value = room.poll(&s, &room.path("")).await.unwrap();
    assert_eq!(room.metadata(&value).unwrap_err().code, "room_response");
    assert_eq!(lock(&fixture.requests).len(), 1);
    fixture.done.abort();
}
#[tokio::test]
async fn poll_backoff_is_cancelled_without_another_http_request() {
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![transient_html(429)]
    })
    .await;
    let room = operation(&origin, 90103);
    let s = Session::new(room.generation, "6".repeat(32), None);
    let path = room.path("");
    let requested = room.poll(&s, &path);
    let cancel = async {
        fixture.done.await.unwrap();
        s.close().await;
    };
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(requested, cancel)
    })
    .await
    .unwrap();
    assert!(matches!(
        result.unwrap_err().code.as_str(),
        "room_closed" | "session_closed"
    ));
    assert_eq!(lock(&fixture.requests).len(), 1);
    assert!(!s.active());
}
#[tokio::test]
async fn independent_monotonic_expiry_closes_a_session_during_transient_poll_backoff() {
    let _global = GLOBAL.lock().await;
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![transient_html(503)]
    })
    .await;
    let room = operation(&origin, 90104);
    let s = Session::new(room.generation, "7".repeat(32), None);
    *lock(&ROOM) = Some(room.clone());
    let path = room.path("");
    let requested = room.poll(&s, &path);
    let expire = async {
        fixture.done.await.unwrap();
        lock(&room.data).timing.mono = Instant::now() + Duration::from_millis(100);
        expiry_watch(s.clone(), room.clone()).await;
    };
    let (result, _) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(requested, expire)
    })
    .await
    .unwrap();
    assert!(matches!(
        result.unwrap_err().code.as_str(),
        "room_expired" | "room_closed" | "session_closed"
    ));
    assert!(!s.active());
    assert_eq!(lock(&fixture.requests).len(), 1);
    assert!(lock(&room.data).retired);
    *lock(&ROOM) = None;
}

#[tokio::test]
async fn local_retirement_is_immediate_and_cleanup_retries_transient_delete_responses() {
    let _global = GLOBAL.lock().await;
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![
            transient_html(429),
            transient_html(503),
            json_response(204, Value::Null),
        ]
    })
    .await;
    let room = operation(&origin, 90105);
    room.receipt(&tok(2), Some("private invitation")).unwrap();
    let s = Session::new(room.generation, "8".repeat(32), None);
    *lock(&ROOM) = Some(room.clone());
    s.close().await;
    assert!(!s.active());
    assert!(lock(&room.data).retired);
    assert!(lock(&room.data).cap.is_empty());
    assert!(room.snapshot().url.is_none());
    tokio::time::timeout(Duration::from_secs(4), fixture.done)
        .await
        .unwrap()
        .unwrap();
    let requests = lock(&fixture.requests);
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0], requests[1]);
    assert_eq!(requests[1], requests[2]);
    let request = String::from_utf8_lossy(&requests[0]);
    assert!(request.starts_with(&format!("DELETE /v1/rooms/{} ", tok(1))));
    assert!(request
        .to_lowercase()
        .contains(&format!("authorization: bearer {}", tok(2)).to_lowercase()));
    *lock(&ROOM) = None;
}
#[tokio::test]
async fn cleanup_retries_network_loss_but_never_fatal_statuses() {
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![String::new(), json_response(204, Value::Null)]
    })
    .await;
    let room = operation(&origin, 90106);
    room.cleanup(&tok(1), &tok(2)).await;
    fixture.done.await.unwrap();
    assert_eq!(lock(&fixture.requests).len(), 2);
    for status in [400, 401, 404, 500] {
        let mut origin = String::new();
        let fixture = mock(|url| {
            origin = url.into();
            vec![transient_html(status), json_response(204, Value::Null)]
        })
        .await;
        let room = operation(&origin, 90107);
        room.cleanup(&tok(1), &tok(2)).await;
        assert_eq!(lock(&fixture.requests).len(), 1);
        fixture.done.abort();
    }
}
#[tokio::test]
async fn cleanup_stops_after_three_transient_responses() {
    let mut origin = String::new();
    let fixture = mock(|url| {
        origin = url.into();
        vec![
            transient_html(502),
            transient_html(504),
            transient_html(429),
            json_response(204, Value::Null),
        ]
    })
    .await;
    let room = operation(&origin, 90108);
    room.cleanup(&tok(1), &tok(2)).await;
    assert_eq!(lock(&fixture.requests).len(), 3);
    fixture.done.abort();
}
#[tokio::test]
async fn cleanup_has_one_five_second_budget_even_with_a_hung_server() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut data = [0; 4096];
        let _ = stream.read(&mut data).await.unwrap();
        std::future::pending::<()>().await;
        drop(stream);
    });
    let room = operation(&origin, 90109);
    let started = Instant::now();
    tokio::time::timeout(Duration::from_secs(6), room.cleanup(&tok(1), &tok(2)))
        .await
        .expect("cleanup exceeded its total budget");
    assert!(started.elapsed() >= CLEANUP_TIMEOUT);
    server.abort();
}
