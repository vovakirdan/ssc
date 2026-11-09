use std::net::IpAddr;

use tokio::net::UdpSocket;
use tokio::time::Duration;
use util::vnet::net::*;

use super::*;
use crate::auth::*;
use crate::relay::relay_static::*;
use crate::server::config::*;
use crate::server::*;

async fn create_listening_test_client(rto_in_ms: u16) -> Result<Client> {
    let conn = UdpSocket::bind("0.0.0.0:0").await?;

    let c = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: "TEST SOFTWARE".to_owned(),
        rto_in_ms,
        conn: Arc::new(conn),
        vnet: None,
    })
    .await?;

    c.listen().await?;

    Ok(c)
}

async fn create_listening_test_client_with_stun_serv() -> Result<Client> {
    let conn = UdpSocket::bind("0.0.0.0:0").await?;

    let c = Client::new(ClientConfig {
        stun_serv_addr: "stun1.l.google.com:19302".to_owned(),
        turn_serv_addr: String::new(),
        username: String::new(),
        password: String::new(),
        realm: String::new(),
        software: "TEST SOFTWARE".to_owned(),
        rto_in_ms: 0,
        conn: Arc::new(conn),
        vnet: None,
    })
    .await?;

    c.listen().await?;

    Ok(c)
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request() -> Result<()> {
    //env_logger::init();

    let c = create_listening_test_client_with_stun_serv().await?;

    let resp = c.send_binding_request().await?;
    log::debug!("mapped-addr: {}", resp);
    {
        let ci = c.client_internal.lock().await;
        let tm = ci.tr_map.lock().await;
        assert_eq!(0, tm.size(), "should be no transaction left");
    }

    c.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request_to_parallel() -> Result<()> {
    env_logger::init();

    let c1 = create_listening_test_client(0).await?;
    let c2 = c1.clone();

    let (stared_tx, mut started_rx) = mpsc::channel::<()>(1);
    let (finished_tx, mut finished_rx) = mpsc::channel::<()>(1);

    let to = lookup_host(true, "stun1.l.google.com:19302").await?;

    tokio::spawn(async move {
        drop(stared_tx);
        if let Ok(resp) = c2.send_binding_request_to(&to.to_string()).await {
            log::debug!("mapped-addr: {}", resp);
        }
        drop(finished_tx);
    });

    let _ = started_rx.recv().await;

    let resp = c1.send_binding_request_to(&to.to_string()).await?;
    log::debug!("mapped-addr: {}", resp);

    let _ = finished_rx.recv().await;

    c1.close().await?;

    Ok(())
}

#[tokio::test]
async fn test_client_with_stun_send_binding_request_to_timeout() -> Result<()> {
    //env_logger::init();

    let c = create_listening_test_client(10).await?;

    let to = lookup_host(true, "127.0.0.1:9").await?;

    let result = c.send_binding_request_to(&to.to_string()).await;
    assert!(result.is_err(), "expected error, but got ok");

    c.close().await?;

    Ok(())
}

struct TestAuthHandler;
impl AuthHandler for TestAuthHandler {
    fn auth_handle(&self, username: &str, realm: &str, _src_addr: SocketAddr) -> Result<Vec<u8>> {
        Ok(generate_auth_key(username, realm, "pass"))
    }
}

// Create an allocation, and then delete all nonces
// The subsequent Write on the allocation will cause a CreatePermission
// which will be forced to handle a stale nonce response
#[tokio::test]
async fn test_client_nonce_expiration() -> Result<()> {
    // env_logger::init();

    // here, it should use static port, like "0.0.0.0:3478",
    // but, due to different test environment, let's fake it by using "0.0.0.0:0"
    // to auto assign a "static" port
    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);
    let server_port = conn.local_addr()?.port();

    let server = Server::new(ServerConfig {
        conn_configs: vec![ConnConfig {
            conn,
            relay_addr_generator: Box::new(RelayAddressGeneratorStatic {
                relay_address: IpAddr::from_str("127.0.0.1")?,
                address: "0.0.0.0".to_owned(),
                net: Arc::new(Net::new(None)),
            }),
        }],
        realm: "webrtc.rs".to_owned(),
        auth_handler: Arc::new(TestAuthHandler {}),
        channel_bind_timeout: Duration::from_secs(0),
        alloc_close_notify: None,
    })
    .await?;

    let conn = Arc::new(UdpSocket::bind("0.0.0.0:0").await?);

    let client = Client::new(ClientConfig {
        stun_serv_addr: format!("127.0.0.1:{server_port}"),
        turn_serv_addr: format!("127.0.0.1:{server_port}"),
        username: "foo".to_owned(),
        password: "pass".to_owned(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 0,
        conn,
        vnet: None,
    })
    .await?;

    client.listen().await?;

    let allocation = client.allocate().await?;

    {
        let mut nonces = server.nonces.lock().await;
        nonces.clear();
    }

    allocation
        .send_to(&[0x00], SocketAddr::from_str("127.0.0.1:8080")?)
        .await?;

    // Shutdown
    client.close().await?;
    server.close().await?;

    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn ssc_data_before_permission_response_does_not_deadlock_reader() -> Result<()> {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use tokio::sync::Notify;
    let server_socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let server_address = server_socket.local_addr()?;
    let seen_data = Arc::new(Notify::new());
    let permission_answered = Arc::new(AtomicBool::new(false));
    let permission_count = Arc::new(AtomicUsize::new(0));
    let outbound_finished = AtomicBool::new(false);
    let incoming_peer: SocketAddr = "198.51.100.10:40000".parse().unwrap();
    let outgoing_peer: SocketAddr = "203.0.113.20:40001".parse().unwrap();
    let server_task = {
        let socket = server_socket.clone();
        let seen = seen_data.clone();
        let answered = permission_answered.clone();
        let count = permission_count.clone();
        tokio::spawn(async move {
            let mut buffer = [0; 4096];
            loop {
                let (n, from) = socket.recv_from(&mut buffer).await.unwrap();
                if !stun::message::is_message(&buffer[..n]) {
                    continue;
                }
                let mut msg = Message::new();
                msg.raw = buffer[..n].to_vec();
                msg.decode().unwrap();
                if msg.typ.class != CLASS_REQUEST {
                    continue;
                }
                if msg.typ.method == METHOD_CREATE_PERMISSION
                    && count.fetch_add(1, Ordering::SeqCst) == 0
                {
                    let mut indication = Message::new();
                    indication
                        .build(&[
                            Box::new(TransactionId::new()),
                            Box::new(MessageType::new(METHOD_DATA, CLASS_INDICATION)),
                            Box::new(PeerAddress {
                                ip: incoming_peer.ip(),
                                port: incoming_peer.port(),
                            }),
                            Box::new(Data(b"inbound ICE before permission response".to_vec())),
                            Box::new(FINGERPRINT),
                        ])
                        .unwrap();
                    socket.send_to(&indication.raw, from).await.unwrap();
                    // The reader must deliver DATA while RelayConn.send_to still holds its
                    // allocation mutex, otherwise this deliberately forced ordering deadlocks.
                    seen.notified().await;
                }
                let mut response = Message::new();
                response
                    .build(&[
                        Box::new(msg.clone()),
                        Box::new(MessageType::new(msg.typ.method, CLASS_SUCCESS_RESPONSE)),
                        Box::new(FINGERPRINT),
                    ])
                    .unwrap();
                answered.store(true, Ordering::SeqCst);
                socket.send_to(&response.raw, from).await.unwrap();
            }
        })
    };
    let client = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: server_address.to_string(),
        username: "fake-user".into(),
        password: "fake-password".into(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 10,
        conn: Arc::new(UdpSocket::bind("127.0.0.1:0").await?),
        vnet: None,
    })
    .await?;
    let (tx, rx) = mpsc::channel(1024);
    let config = {
        let ci = client.client_internal.lock().await;
        *ci.read_ch_tx.lock().await = Some(tx);
        RelayConnConfig {
            relayed_addr: "127.0.0.1:50000".parse().unwrap(),
            integrity: MessageIntegrity::new_short_term_integrity("fake-integrity".into()),
            nonce: Nonce::new(ATTR_NONCE, "fake-nonce".into()),
            lifetime: Duration::ZERO,
            binding_mgr: ci.binding_mgr.clone(),
            read_ch_rx: Arc::new(Mutex::new(rx)),
        }
    };
    let relay = Arc::new(RelayConn::new(client.client_internal.clone(), config).await);
    client.listen().await?;
    let outbound = async {
        relay.send_to(b"outbound ICE", outgoing_peer).await.unwrap();
        outbound_finished.store(true, Ordering::SeqCst);
    };
    let inbound = async {
        let mut data = [0; 1024];
        let (n, peer) = relay.recv_from(&mut data).await.unwrap();
        assert_eq!(&data[..n], b"inbound ICE before permission response");
        assert_eq!(peer, incoming_peer);
        assert!(!outbound_finished.load(Ordering::SeqCst),"DATA arrived only after the outbound transaction finished; timeout may mask a reader deadlock");
        assert!(
            !permission_answered.load(Ordering::SeqCst),
            "test did not force DATA before permission response"
        );
        seen_data.notify_one();
        relay
            .send_to(b"inbound ICE response", incoming_peer)
            .await
            .unwrap();
    };
    tokio::time::timeout(Duration::from_secs(5), async {
        tokio::join!(outbound, inbound);
    })
    .await
    .expect("DATA/permission interleaving deadlocked");
    assert!(
        permission_count.load(Ordering::SeqCst) >= 2,
        "both different IPs must create permissions"
    );
    client.close().await?;
    server_task.abort();
    Ok(())
}

#[tokio::test]
async fn ssc_permission_timeout_must_not_be_reported_as_a_successful_send() -> Result<()> {
    // Bound-but-silent TURN socket: no kernel ICMP error and no permission responses.
    let silent_turn = UdpSocket::bind("127.0.0.1:0").await?;
    let client = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: silent_turn.local_addr()?.to_string(),
        username: "fake-user".into(),
        password: "fake-password".into(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 10,
        conn: Arc::new(UdpSocket::bind("127.0.0.1:0").await?),
        vnet: None,
    })
    .await?;
    let (tx, rx) = mpsc::channel(1024);
    let config = {
        let ci = client.client_internal.lock().await;
        *ci.read_ch_tx.lock().await = Some(tx);
        RelayConnConfig {
            relayed_addr: "127.0.0.1:50000".parse().unwrap(),
            integrity: MessageIntegrity::new_short_term_integrity("fake-integrity".into()),
            nonce: Nonce::new(ATTR_NONCE, "fake-nonce".into()),
            lifetime: Duration::ZERO,
            binding_mgr: ci.binding_mgr.clone(),
            read_ch_rx: Arc::new(Mutex::new(rx)),
        }
    };
    let relay = RelayConn::new(client.client_internal.clone(), config).await;
    client.listen().await?;
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        relay.send_to(b"outbound ICE", "203.0.113.20:40001".parse().unwrap()),
    )
    .await
    .expect("transaction did not time out");
    assert!(
        result.is_err(),
        "CreatePermission timed out without any response, but send_to returned success"
    );
    client.close().await?;
    Ok(())
}

async fn ssc_foreign_packet_then_permission_response(request_packet: bool) -> Result<()> {
    let socket = Arc::new(UdpSocket::bind("127.0.0.1:0").await?);
    let server_address = socket.local_addr()?;
    let foreign = UdpSocket::bind("127.0.0.1:0").await?;
    let worker = tokio::spawn(async move {
        let mut bytes = [0; 4096];
        let (n, from) = socket.recv_from(&mut bytes).await.unwrap();
        let mut request = Message::new();
        request.raw = bytes[..n].to_vec();
        request.decode().unwrap();
        let mut stray = Message::new();
        stray
            .build(&[
                Box::new(TransactionId::new()),
                Box::new(BINDING_REQUEST),
                Box::new(FINGERPRINT),
            ])
            .unwrap();
        let datagram = if request_packet {
            stray.raw
        } else {
            vec![22, 0xfe, 0xfd, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        };
        foreign.send_to(&datagram, from).await.unwrap();
        let mut reply = Message::new();
        reply
            .build(&[
                Box::new(request),
                Box::new(MessageType::new(
                    METHOD_CREATE_PERMISSION,
                    CLASS_SUCCESS_RESPONSE,
                )),
                Box::new(FINGERPRINT),
            ])
            .unwrap();
        socket.send_to(&reply.raw, from).await.unwrap();
        // Keep the TURN socket alive so a failed reader cannot be masked by ICMP.
        std::future::pending::<()>().await;
    });
    let client = Client::new(ClientConfig {
        stun_serv_addr: String::new(),
        turn_serv_addr: server_address.to_string(),
        username: "fake-user".into(),
        password: "fake-password".into(),
        realm: String::new(),
        software: String::new(),
        rto_in_ms: 10,
        conn: Arc::new(UdpSocket::bind("127.0.0.1:0").await?),
        vnet: None,
    })
    .await?;
    client.listen().await?;
    let mut request = Message::new();
    request.build(&[
        Box::new(TransactionId::new()),
        Box::new(MessageType::new(METHOD_CREATE_PERMISSION, CLASS_REQUEST)),
        Box::new(FINGERPRINT),
    ])?;
    let result = {
        let mut ci = client.client_internal.lock().await;
        ci.perform_transaction(&request, &server_address.to_string(), false)
            .await
    };
    worker.abort();
    client.close().await?;
    let transaction = result?;
    assert_eq!(
        transaction.msg.typ,
        MessageType::new(METHOD_CREATE_PERMISSION, CLASS_SUCCESS_RESPONSE),
        "foreign datagram stopped the TURN reader; valid permission response was lost"
    );
    Ok(())
}
#[tokio::test]
async fn ssc_foreign_binding_request_must_not_kill_turn_reader() -> Result<()> {
    ssc_foreign_packet_then_permission_response(true).await
}
#[tokio::test]
async fn ssc_foreign_dtls_does_not_kill_turn_reader() -> Result<()> {
    ssc_foreign_packet_then_permission_response(false).await
}
