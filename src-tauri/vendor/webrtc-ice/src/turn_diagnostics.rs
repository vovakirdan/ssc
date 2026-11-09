//! Opt-in, bounded TURN control summaries. Raw packets and socket addresses are never retained.
use async_trait::async_trait;
use serde::Serialize;
use std::{
    any::Any,
    collections::BTreeMap,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex,
    },
    time::Instant,
};
use util::Conn;

const MAX_GROUPS: usize = 128;
static ENABLED: AtomicBool = AtomicBool::new(false);
static BUFFER: LazyLock<Mutex<Buffer>> = LazyLock::new(|| Mutex::new(Buffer::default()));
#[derive(Default)]
struct Buffer {
    started: Option<Instant>,
    groups: BTreeMap<Key, WireStat>,
    truncated: bool,
}
type Key = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Option<u16>,
    Option<bool>,
);
#[derive(Clone, Serialize)]
pub struct WireStat {
    pub transport: &'static str,
    pub direction: &'static str,
    pub method: &'static str,
    pub class: &'static str,
    pub error_code: Option<u16>,
    pub from_server: Option<bool>,
    pub packets: u64,
    pub first_ms: u64,
    pub last_ms: u64,
}
#[derive(Serialize)]
pub struct WireSnapshot {
    pub groups: Vec<WireStat>,
    pub truncated: bool,
}
fn lock() -> std::sync::MutexGuard<'static, Buffer> {
    BUFFER.lock().unwrap_or_else(|error| error.into_inner())
}
pub fn set_enabled(enabled: bool) {
    if enabled && ENABLED.load(Ordering::Acquire) {
        return;
    }
    ENABLED.store(false, Ordering::Release);
    let mut buffer = lock();
    *buffer = Buffer {
        started: enabled.then(Instant::now),
        ..Default::default()
    };
    ENABLED.store(enabled, Ordering::Release);
}
pub fn clear() {
    let mut buffer = lock();
    *buffer = Buffer {
        started: ENABLED.load(Ordering::Acquire).then(Instant::now),
        ..Default::default()
    };
}
pub fn snapshot() -> WireSnapshot {
    let buffer = lock();
    if !ENABLED.load(Ordering::Acquire) {
        return WireSnapshot {
            groups: vec![],
            truncated: false,
        };
    }
    WireSnapshot {
        groups: buffer.groups.values().cloned().collect(),
        truncated: buffer.truncated,
    }
}
#[derive(Clone, Copy)]
pub(crate) enum Transport {
    Udp,
    Tcp,
    Tls,
}
impl Transport {
    fn name(self) -> &'static str {
        match self {
            Self::Udp => "udp",
            Self::Tcp => "tcp",
            Self::Tls => "tls",
        }
    }
}
fn method(kind: u16) -> &'static str {
    match kind {
        1 => "binding",
        3 => "allocate",
        4 => "refresh",
        6 => "send",
        7 => "data",
        8 => "create_permission",
        9 => "channel_bind",
        _ => "other",
    }
}
fn classify(packet: &[u8]) -> Option<(&'static str, &'static str, Option<u16>)> {
    if packet.len() >= 3 && (20..=25).contains(&packet[0]) && packet[1] == 0xfe {
        return Some(("dtls_record", "data", None));
    }
    if packet.len() >= 4 && packet[0] >> 6 == 1 {
        return Some(("channel_data", "data", None));
    }
    if packet.len() < 8 || packet[0] >> 6 != 0 || packet[4..8] != [0x21, 0x12, 0xa4, 0x42] {
        return Some(("other_datagram", "data", None));
    }
    if packet.len() < 20 {
        return Some(("malformed_stun", "invalid", None));
    }
    let end = 20 + u16::from_be_bytes([packet[2], packet[3]]) as usize;
    if end > packet.len() || !(end - 20).is_multiple_of(4) {
        return Some(("malformed_stun", "invalid", None));
    }
    let kind = u16::from_be_bytes([packet[0], packet[1]]);
    let method = method((kind & 0x000f) | ((kind & 0x00e0) >> 1) | ((kind & 0x3e00) >> 2));
    let class = match ((kind >> 4) & 1) | ((kind >> 7) & 2) {
        0 => "request",
        1 => "indication",
        2 => "success",
        _ => "error",
    };
    let mut error_code = None;
    let mut offset = 20;
    while offset < end {
        if offset + 4 > end {
            return Some(("malformed_stun", "invalid", None));
        }
        let attr = u16::from_be_bytes([packet[offset], packet[offset + 1]]);
        let length = u16::from_be_bytes([packet[offset + 2], packet[offset + 3]]) as usize;
        if offset + 4 + length > end {
            return Some(("malformed_stun", "invalid", None));
        }
        if class == "error" && attr == 9 && length >= 4 {
            let code = u16::from(packet[offset + 6] & 7) * 100 + u16::from(packet[offset + 7]);
            if [
                300, 400, 401, 403, 420, 437, 438, 440, 441, 442, 443, 446, 447, 486, 500, 508,
            ]
            .contains(&code)
            {
                error_code = Some(code);
            }
        }
        offset += 4 + length.div_ceil(4) * 4;
        if offset > end {
            return Some(("malformed_stun", "invalid", None));
        }
    }
    Some((method, class, error_code))
}
fn record(transport: Transport, direction: &'static str, packet: &[u8], from_server: Option<bool>) {
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    let Some((method, class, error_code)) = classify(packet) else {
        return;
    };
    let mut buffer = lock();
    if !ENABLED.load(Ordering::Acquire) {
        return;
    }
    add(
        &mut buffer,
        transport,
        direction,
        method,
        class,
        error_code,
        from_server,
    );
}
fn add(
    buffer: &mut Buffer,
    transport: Transport,
    direction: &'static str,
    method: &'static str,
    class: &'static str,
    error_code: Option<u16>,
    from_server: Option<bool>,
) {
    let Some(start) = buffer.started else {
        return;
    };
    let ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
    let key = (
        transport.name(),
        direction,
        method,
        class,
        error_code,
        from_server,
    );
    if buffer.groups.len() >= MAX_GROUPS && !buffer.groups.contains_key(&key) {
        buffer.truncated = true;
        return;
    }
    let entry = buffer.groups.entry(key).or_insert(WireStat {
        transport: key.0,
        direction,
        method,
        class,
        error_code,
        from_server,
        packets: 0,
        first_ms: ms,
        last_ms: ms,
    });
    entry.packets = entry.packets.saturating_add(1);
    entry.last_ms = ms;
}
pub(crate) struct TraceConn {
    inner: Arc<dyn Conn + Send + Sync>,
    transport: Transport,
    // Configured TURN endpoint only, learned from the existing outbound I/O;
    // no peer addresses enter the diagnostic map or serialized snapshot.
    expected_server: Mutex<Option<SocketAddr>>,
}
impl TraceConn {
    fn expect_server(&self, address: SocketAddr) {
        let mut expected = self
            .expected_server
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if expected.is_none() {
            *expected = Some(address);
        }
    }
    fn from_server(&self, address: SocketAddr) -> Option<bool> {
        self.expected_server
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .map(|expected| expected == address)
    }
    pub(crate) fn new(inner: Arc<dyn Conn + Send + Sync>, transport: Transport) -> Self {
        Self {
            inner,
            transport,
            expected_server: Mutex::new(None),
        }
    }
}
#[async_trait]
impl Conn for TraceConn {
    async fn connect(&self, addr: SocketAddr) -> util::Result<()> {
        self.inner.connect(addr).await
    }
    async fn recv(&self, buffer: &mut [u8]) -> util::Result<usize> {
        let n = self.inner.recv(buffer).await?;
        record(
            self.transport,
            "inbound",
            &buffer[..n],
            self.inner
                .remote_addr()
                .and_then(|address| self.from_server(address)),
        );
        Ok(n)
    }
    async fn recv_from(&self, buffer: &mut [u8]) -> util::Result<(usize, SocketAddr)> {
        let (n, from) = self.inner.recv_from(buffer).await?;
        record(
            self.transport,
            "inbound",
            &buffer[..n],
            self.from_server(from),
        );
        Ok((n, from))
    }
    async fn send(&self, buffer: &[u8]) -> util::Result<usize> {
        let n = self.inner.send(buffer).await?;
        record(self.transport, "outbound", buffer, None);
        Ok(n)
    }
    async fn send_to(&self, buffer: &[u8], to: SocketAddr) -> util::Result<usize> {
        self.expect_server(to);
        let n = self.inner.send_to(buffer, to).await?;
        record(self.transport, "outbound", buffer, None);
        Ok(n)
    }
    fn local_addr(&self) -> util::Result<SocketAddr> {
        self.inner.local_addr()
    }
    fn remote_addr(&self) -> Option<SocketAddr> {
        self.inner.remote_addr()
    }
    async fn close(&self) -> util::Result<()> {
        self.inner.close().await
    }
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self.inner.as_any()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn error_packet() -> Vec<u8> {
        let mut bytes = vec![0; 28];
        bytes[..2].copy_from_slice(&0x0118u16.to_be_bytes());
        bytes[2..4].copy_from_slice(&8u16.to_be_bytes());
        bytes[4..8].copy_from_slice(&[0x21, 0x12, 0xa4, 0x42]);
        bytes[8..20].copy_from_slice(b"PRIVATE_TXID");
        bytes[20..24].copy_from_slice(&[0, 9, 0, 4]);
        bytes[26..28].copy_from_slice(&[4, 3]);
        bytes
    }
    #[test]
    fn diagnostics_default_off_whitelists_metadata_and_never_stores_packet_content() {
        assert!(snapshot().groups.is_empty());
        let mut packet = error_packet();
        let reason = b"SECRET_PASSWORD nonce realm username PRIVATE_ADDRESS SDP MESSAGE";
        packet.extend_from_slice(reason);
        let attribute_len = 4 + reason.len();
        while !packet.len().is_multiple_of(4) {
            packet.push(0);
        }
        let body_len = (packet.len() - 20) as u16;
        packet[2..4].copy_from_slice(&body_len.to_be_bytes());
        packet[22..24].copy_from_slice(&(attribute_len as u16).to_be_bytes());
        record(Transport::Udp, "inbound", &packet, Some(true));
        assert!(snapshot().groups.is_empty());
        let mut buffer = Buffer {
            started: Some(Instant::now()),
            ..Default::default()
        };
        let (method, class, code) = classify(&packet).unwrap();
        add(
            &mut buffer,
            Transport::Udp,
            "inbound",
            method,
            class,
            code,
            Some(true),
        );
        add(
            &mut buffer,
            Transport::Udp,
            "inbound",
            method,
            class,
            code,
            Some(true),
        );
        let result = WireSnapshot {
            groups: buffer.groups.into_values().collect(),
            truncated: buffer.truncated,
        };
        assert_eq!(result.groups.len(), 1);
        let entry = &result.groups[0];
        assert_eq!(entry.method, "create_permission");
        assert_eq!(entry.class, "error");
        assert_eq!(entry.error_code, Some(403));
        assert_eq!(entry.packets, 2);
        let json = serde_json::to_string(&result).unwrap();
        for forbidden in [
            "PRIVATE", "SECRET", "nonce", "realm", "username", "SDP", "MESSAGE",
        ] {
            assert!(!json.contains(forbidden));
        }
        clear();
        assert!(snapshot().groups.is_empty());
        set_enabled(false);
        record(Transport::Udp, "inbound", &packet, Some(true));
        assert!(snapshot().groups.is_empty());
    }
    #[tokio::test]
    async fn wrapper_forwards_udp_bytes_addresses_and_original_connection_type_unchanged() {
        let raw = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap());
        let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let original = raw.local_addr().unwrap();
        let wrapper = TraceConn::new(raw, Transport::Udp);
        assert_eq!(wrapper.local_addr().unwrap(), original);
        assert!(wrapper.as_any().is::<tokio::net::UdpSocket>());
        let packet = error_packet();
        wrapper
            .send_to(&packet, peer.local_addr().unwrap())
            .await
            .unwrap();
        let mut buffer = [0; 128];
        let (n, from) = peer.recv_from(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], packet.as_slice());
        assert_eq!(from, original);
        peer.send_to(&packet, from).await.unwrap();
        let (n, from) = wrapper.recv_from(&mut buffer).await.unwrap();
        assert_eq!(&buffer[..n], packet.as_slice());
        assert_eq!(from, peer.local_addr().unwrap());
        assert_eq!(wrapper.from_server(from), Some(true));
        let foreign = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        assert_eq!(
            wrapper.from_server(foreign.local_addr().unwrap()),
            Some(false)
        );
    }
    #[test]
    fn opaque_and_malformed_datagrams_are_counted_with_fixed_labels_only() {
        assert_eq!(
            classify(&[22, 0xfe, 0xfd, 0, 0]),
            Some(("dtls_record", "data", None))
        );
        assert_eq!(
            classify(&[0xff, 1, 2]),
            Some(("other_datagram", "data", None))
        );
        let mut malformed = error_packet();
        malformed[2..4].copy_from_slice(&64u16.to_be_bytes());
        assert_eq!(
            classify(&malformed),
            Some(("malformed_stun", "invalid", None))
        );
        for length in 0..malformed.len() {
            let _ = classify(&malformed[..length]);
        }
        let mut buffer = Buffer {
            started: Some(Instant::now()),
            ..Default::default()
        };
        for code in 0..200 {
            add(
                &mut buffer,
                Transport::Udp,
                "inbound",
                "other",
                "error",
                Some(code),
                Some(false),
            );
        }
        assert_eq!(buffer.groups.len(), MAX_GROUPS);
        assert!(buffer.truncated);
    }
}
