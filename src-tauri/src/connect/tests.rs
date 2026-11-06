use super::*;
fn creator(s: &Session) {
    initial(
        s,
        Role::Creator,
        Service::new("http://127.0.0.1:8080").unwrap(),
        Input::Creator {
            ttl: 300,
            key: Zeroizing::new("k".repeat(43)),
            offer: None,
            receipt: None,
        },
    )
    .unwrap();
}
#[tokio::test]
async fn closing_cancels_an_inflight_operation_and_removes_helper_state() {
    let s = Session::new(700, "a".repeat(32), None);
    creator(&s);
    let task = bounded(&s, std::future::pending::<()>());
    let closer = async {
        tokio::task::yield_now().await;
        s.close().await;
    };
    let (result, _) = tokio::join!(task, closer);
    assert_eq!(result.unwrap_err().code, "link_cancelled");
    assert!(snapshot(&s).is_none());
}
#[tokio::test]
async fn expired_operation_cannot_continue_or_extend_its_deadline() {
    let s = Session::new(701, "b".repeat(32), None);
    creator(&s);
    with_op(&s, |op| {
        op.deadline = Some(Instant::now() - Duration::from_secs(1))
    })
    .unwrap();
    let result = bounded(&s, std::future::pending::<()>()).await;
    assert_eq!(result.unwrap_err().code, "invitation_expired");
}
#[tokio::test]
async fn stale_operation_cannot_repopulate_a_retired_session() {
    let old = Session::new(702, "a".repeat(32), None);
    creator(&old);
    old.close().await;
    let replacement = Session::new(703, "b".repeat(32), None);
    creator(&replacement);
    update(&old, LinkPhase::WaitingPeer);
    assert!(snapshot(&old).is_none());
    assert_eq!(snapshot(&replacement).unwrap().generation, 703);
}
#[tokio::test]
async fn completed_state_cannot_be_overwritten_by_a_late_network_error() {
    let s = Session::new(704, "a".repeat(32), None);
    creator(&s);
    connected(&s);
    update(&s, LinkPhase::WaitingConnection);
    assert!(snapshot(&s).unwrap().phase == LinkPhase::Complete);
    assert!(matches!(
        lock(&s.link).as_ref().unwrap().input,
        Input::Finished
    ));
}

#[tokio::test]
async fn lost_creation_response_reuses_the_prepared_offer_and_key() {
    use super::http::tests::{json_response, mock, receipt, token};
    let fixture = mock(|origin| {
        vec![
            String::new(),
            json_response(201, receipt(origin)),
            json_response(404, serde_json::json!({"error":"gone"})),
        ]
    })
    .await;
    let s = Session::new(705, "a".repeat(32), None);
    initial(
        &s,
        Role::Creator,
        fixture.service.clone(),
        Input::Creator {
            ttl: 300,
            key: Zeroizing::new(token(3)),
            offer: Some(Zeroizing::new("prepared-offer".into())),
            receipt: None,
        },
    )
    .unwrap();
    let error = create_and_wait(&s).await.unwrap_err();
    assert_eq!(error.code, "link_unavailable");
    fixture.done.await.unwrap();
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(requests[0], requests[1], "retry changed the HTTP request");
}
#[tokio::test]
async fn answer_retry_uses_cached_answer_without_fetching_or_creating_a_new_peer() {
    use super::http::tests::{json_response, mock, token};
    let fixture=mock(|_|vec!["HTTP/1.1 503 unavailable\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into(),json_response(204,serde_json::Value::Null)]).await;
    let s = Session::new(706, "a".repeat(32), None);
    initial(
        &s,
        Role::Guest,
        fixture.service.clone(),
        Input::Guest {
            token: Zeroizing::new(token(1)),
            answer: Some(Zeroizing::new("prepared-answer".into())),
        },
    )
    .unwrap();
    join(&s).await.unwrap();
    fixture.done.await.unwrap();
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], requests[1]);
    assert!(lock(&s.data).pc.is_none());
}
#[tokio::test]
async fn operation_cannot_be_installed_after_its_session_was_closed() {
    let s = Session::new(707, "a".repeat(32), None);
    s.close().await;
    let result = initial(
        &s,
        Role::Guest,
        Service::new("http://127.0.0.1:8080").unwrap(),
        Input::Guest {
            token: Zeroizing::new("t".repeat(43)),
            answer: None,
        },
    );
    assert!(result.is_err());
    assert!(snapshot(&s).is_none());
}

#[tokio::test]
async fn malicious_retry_after_is_clamped_to_operation_deadline() {
    use super::http::tests::{mock, token};
    let fixture=mock(|_|vec!["HTTP/1.1 429 busy\r\nRetry-After: 18446744073709551615\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".into()]).await;
    let s = Session::new(708, "a".repeat(32), None);
    initial(
        &s,
        Role::Creator,
        fixture.service.clone(),
        Input::Creator {
            ttl: 300,
            key: Zeroizing::new(token(3)),
            offer: Some(Zeroizing::new("prepared".into())),
            receipt: None,
        },
    )
    .unwrap();
    with_op(&s, |op| {
        op.deadline = Some(Instant::now() + Duration::from_millis(100))
    })
    .unwrap();
    let key = token(3);
    let result = request(&s, || fixture.service.create("prepared", 300, &key)).await;
    assert_eq!(result.err().unwrap().code, "invitation_expired");
    fixture.done.await.unwrap();
    assert_eq!(fixture.requests.lock().unwrap().len(), 1);
}
