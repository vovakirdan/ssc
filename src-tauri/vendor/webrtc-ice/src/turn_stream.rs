//! TURN packet framing over TCP/TLS; the relayed peer transport remains UDP.
use async_trait::async_trait;
use std::{any::Any, io, net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf},
    net::TcpStream,
    sync::{watch, Mutex},
};
use tokio_rustls::{
    rustls::{self, pki_types::ServerName},
    TlsConnector,
};
use util::Conn;
const MAX_PACKET: usize = u16::MAX as usize;
trait Stream: AsyncRead + AsyncWrite + Unpin + Send + Sync {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + Sync> Stream for T {}
type Reader = PacketReader<ReadHalf<Box<dyn Stream>>>;
type Writer = WriteHalf<Box<dyn Stream>>;
pub(crate) struct TurnStream {
    reader: Mutex<Option<Reader>>,
    writer: Mutex<Option<Writer>>,
    local: SocketAddr,
    remote: SocketAddr,
    closed: watch::Sender<bool>,
}
fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid TURN stream frame")
}
fn closed() -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, "TURN stream closed")
}
impl TurnStream {
    pub(crate) async fn dial(host: &str, port: u16, tls: bool) -> io::Result<Self> {
        tokio::time::timeout(Duration::from_secs(5), async {
            let tcp = TcpStream::connect((host, port)).await?;
            tcp.set_nodelay(true)?;
            let local = tcp.local_addr()?;
            let remote = tcp.peer_addr()?;
            let stream: Box<dyn Stream> = if tls {
                let roots = rustls::RootCertStore::from_iter(
                    webpki_roots::TLS_SERVER_ROOTS.iter().cloned(),
                );
                let config = rustls::ClientConfig::builder_with_provider(Arc::new(
                    rustls::crypto::ring::default_provider(),
                ))
                .with_safe_default_protocol_versions()
                .map_err(|_| invalid())?
                .with_root_certificates(roots)
                .with_no_client_auth();
                let name = ServerName::try_from(host.to_owned()).map_err(|_| invalid())?;
                Box::new(
                    TlsConnector::from(Arc::new(config))
                        .connect(name, tcp)
                        .await
                        .map_err(|_| {
                            io::Error::new(
                                io::ErrorKind::PermissionDenied,
                                "TURN TLS validation failed",
                            )
                        })?,
                )
            } else {
                Box::new(tcp)
            };
            Ok(Self::new(stream, local, remote))
        })
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "TURN stream connection timed out"))?
    }
    fn new(stream: Box<dyn Stream>, local: SocketAddr, remote: SocketAddr) -> Self {
        let (reader, writer) = tokio::io::split(stream);
        Self {
            reader: Mutex::new(Some(PacketReader::new(reader))),
            writer: Mutex::new(Some(writer)),
            local,
            remote,
            closed: watch::channel(false).0,
        }
    }
    async fn cancelled(&self) {
        let mut closed = self.closed.subscribe();
        let _ = closed.wait_for(|done| *done).await;
    }
}
struct PacketReader<R> {
    stream: R,
    pending: Vec<u8>,
}
impl<R> PacketReader<R> {
    fn new(stream: R) -> Self {
        Self {
            stream,
            pending: Vec::new(),
        }
    }
}
// Keep consumed bytes in the connection so cancellation cannot lose a partial frame.
async fn read_packet<R: AsyncRead + Unpin>(
    reader: &mut PacketReader<R>,
    output: &mut [u8],
) -> io::Result<usize> {
    loop {
        let wire_length = if reader.pending.len() < 4 {
            4
        } else {
            let size = u16::from_be_bytes([reader.pending[2], reader.pending[3]]) as usize;
            let (header, padding) = match reader.pending[0] >> 6 {
                0 if size.is_multiple_of(4) => (20usize, 0),
                1 => (4usize, (4 - size % 4) % 4),
                _ => return Err(invalid()),
            };
            let length = header + size;
            if length > MAX_PACKET || length > output.len() {
                return Err(invalid());
            }
            if reader.pending.len() == length + padding {
                if header == 20 && reader.pending[4..8] != [0x21, 0x12, 0xa4, 0x42] {
                    return Err(invalid());
                }
                output[..length].copy_from_slice(&reader.pending[..length]);
                reader.pending.clear();
                return Ok(length);
            }
            length + padding
        };
        let mut scratch = [0u8; 2048];
        let needed = (wire_length - reader.pending.len()).min(scratch.len());
        let received = reader.stream.read(&mut scratch[..needed]).await?;
        if received == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Truncated TURN stream",
            ));
        }
        reader.pending.extend_from_slice(&scratch[..received]);
    }
}
struct IncompleteWrite {
    closed: watch::Sender<bool>,
    complete: bool,
}
impl Drop for IncompleteWrite {
    fn drop(&mut self) {
        if !self.complete {
            self.closed.send_replace(true);
        }
    }
}
fn framed_packet(packet: &[u8]) -> io::Result<Vec<u8>> {
    if packet.len() < 4 {
        return Err(invalid());
    }
    let size = u16::from_be_bytes([packet[2], packet[3]]) as usize;
    let (header, padding) = match packet[0] >> 6 {
        0 if size.is_multiple_of(4) => (20usize, 0),
        1 => (4usize, (4 - size % 4) % 4),
        _ => return Err(invalid()),
    };
    let length = header + size;
    if length > MAX_PACKET || (packet.len() != length && packet.len() != length + padding) {
        return Err(invalid());
    }
    if header == 20 && packet[4..8] != [0x21, 0x12, 0xa4, 0x42] {
        return Err(invalid());
    }
    let mut result = packet[..length].to_vec();
    result.resize(length + padding, 0);
    Ok(result)
}
#[async_trait]
impl Conn for TurnStream {
    async fn connect(&self, addr: SocketAddr) -> util::Result<()> {
        if addr != self.remote || *self.closed.borrow() {
            return Err(closed().into());
        }
        Ok(())
    }
    async fn recv(&self, output: &mut [u8]) -> util::Result<usize> {
        let mut reader = tokio::select! { biased; _ = self.cancelled() => return Err(closed().into()), reader = self.reader.lock() => reader };
        let reader = reader.as_mut().ok_or_else(closed)?;
        let result = tokio::select! { biased; _ = self.cancelled() => Err(closed()), result = read_packet(reader, output) => result };
        if result.is_err() {
            self.closed.send_replace(true);
        }
        Ok(result?)
    }
    async fn recv_from(&self, output: &mut [u8]) -> util::Result<(usize, SocketAddr)> {
        Ok((self.recv(output).await?, self.remote))
    }
    async fn send(&self, packet: &[u8]) -> util::Result<usize> {
        self.send_to(packet, self.remote).await
    }
    async fn send_to(&self, packet: &[u8], target: SocketAddr) -> util::Result<usize> {
        if target != self.remote {
            return Err(invalid().into());
        }
        let frame = framed_packet(packet)?;
        let mut writer = tokio::select! { biased; _ = self.cancelled() => return Err(closed().into()), writer = self.writer.lock() => writer };
        let writer = writer.as_mut().ok_or_else(closed)?;
        // A cancelled partial write must not permit a later frame on the same stream.
        let mut write_guard = IncompleteWrite {
            closed: self.closed.clone(),
            complete: false,
        };
        let result = tokio::select! { biased; _ = self.cancelled() => Err(closed()), result = writer.write_all(&frame) => result };
        if result.is_err() {
            self.closed.send_replace(true);
        }
        result?;
        write_guard.complete = true;
        Ok(packet.len())
    }
    fn local_addr(&self) -> util::Result<SocketAddr> {
        Ok(self.local)
    }
    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.remote)
    }
    async fn close(&self) -> util::Result<()> {
        self.closed.send_replace(true);
        if let Some(mut writer) = self.writer.lock().await.take() {
            let _ = tokio::time::timeout(Duration::from_secs(1), writer.shutdown()).await;
        }
        self.reader.lock().await.take();
        Ok(())
    }
    fn as_any(&self) -> &(dyn Any + Send + Sync) {
        self
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn stun() -> Vec<u8> {
        let mut p = vec![0, 1, 0, 0, 0x21, 0x12, 0xa4, 0x42];
        p.resize(20, 0);
        p
    }
    #[tokio::test]
    async fn fragmented_and_coalesced_frames_keep_boundaries_and_consume_padding() {
        let (mut tx, rx) = tokio::io::duplex(128);
        let mut wire = stun();
        wire.extend([0x40, 1, 0, 3, 1, 2, 3, 0]);
        wire.extend(stun());
        tokio::spawn(async move {
            for byte in wire {
                tx.write_all(&[byte]).await.unwrap();
            }
        });
        let mut output = [0u8; 128];
        let mut rx = PacketReader::new(rx);
        assert_eq!(read_packet(&mut rx, &mut output).await.unwrap(), 20);
        assert_eq!(read_packet(&mut rx, &mut output).await.unwrap(), 7);
        assert_eq!(&output[..7], &[0x40, 1, 0, 3, 1, 2, 3]);
        assert_eq!(read_packet(&mut rx, &mut output).await.unwrap(), 20);
    }
    #[tokio::test]
    async fn malformed_oversized_and_truncated_packets_fail() {
        for frame in [
            vec![0x80, 0, 0, 0],
            vec![0, 1, 0, 1],
            vec![0, 1, 0xff, 0xfc],
            vec![0x40, 1, 0, 3, 1],
            vec![0, 1, 0, 0, 0, 0, 0, 0],
        ] {
            let (mut tx, rx) = tokio::io::duplex(128);
            tx.write_all(&frame).await.unwrap();
            drop(tx);
            let mut rx = PacketReader::new(rx);
            assert!(read_packet(&mut rx, &mut [0u8; 128]).await.is_err());
        }
        assert!(framed_packet(&[0x40, 1, 0, 3, 1]).is_err());
        assert!(framed_packet(&[0, 1, 0, 0]).is_err());
    }
    #[tokio::test]
    async fn send_padding_and_concurrent_writes_do_not_interleave() {
        let (client, remote) = tokio::io::duplex(128);
        let conn = Arc::new(TurnStream::new(
            Box::new(client),
            "127.0.0.1:1".parse().unwrap(),
            "127.0.0.1:2".parse().unwrap(),
        ));
        let c = conn.clone();
        let a = tokio::spawn(async move { c.send(&[0x40, 1, 0, 3, 1, 2, 3]).await.unwrap() });
        let c = conn.clone();
        let b = tokio::spawn(async move { c.send(&stun()).await.unwrap() });
        let mut output = [0u8; 128];
        let mut remote = PacketReader::new(remote);
        let n = read_packet(&mut remote, &mut output).await.unwrap();
        let m = read_packet(&mut remote, &mut output).await.unwrap();
        assert_eq!(n + m, 27);
        assert_eq!(a.await.unwrap() + b.await.unwrap(), 27);
        assert_eq!(
            framed_packet(&[0x40, 1, 0, 3, 1, 2, 3, 0]).unwrap(),
            vec![0x40, 1, 0, 3, 1, 2, 3, 0]
        );
    }
    #[tokio::test]
    async fn close_unblocks_pending_read_and_prevents_reuse() {
        let (client, _remote) = tokio::io::duplex(128);
        let conn = Arc::new(TurnStream::new(
            Box::new(client),
            "127.0.0.1:1".parse().unwrap(),
            "127.0.0.1:2".parse().unwrap(),
        ));
        let c = conn.clone();
        let pending = tokio::spawn(async move { c.recv(&mut [0u8; 128]).await });
        conn.close().await.unwrap();
        assert!(pending.await.unwrap().is_err());
        assert!(conn.send(&stun()).await.is_err());
    }
    #[tokio::test]
    async fn cancelled_read_retains_partial_frame_for_retry() {
        let (client, mut remote) = tokio::io::duplex(128);
        let conn = TurnStream::new(
            Box::new(client),
            "127.0.0.1:1".parse().unwrap(),
            "127.0.0.1:2".parse().unwrap(),
        );
        let packet = stun();
        remote.write_all(&packet[..7]).await.unwrap();
        let mut output = [0u8; 128];
        assert!(
            tokio::time::timeout(Duration::from_millis(10), conn.recv(&mut output))
                .await
                .is_err()
        );
        remote.write_all(&packet[7..]).await.unwrap();
        assert_eq!(conn.recv(&mut output).await.unwrap(), packet.len());
        assert_eq!(&output[..packet.len()], packet.as_slice());
    }
    #[tokio::test]
    async fn cancelled_partial_write_permanently_closes_the_stream() {
        let (client, _remote) = tokio::io::duplex(1);
        let conn = TurnStream::new(
            Box::new(client),
            "127.0.0.1:1".parse().unwrap(),
            "127.0.0.1:2".parse().unwrap(),
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(10), conn.send(&stun()))
                .await
                .is_err()
        );
        assert!(conn.send(&stun()).await.is_err());
        assert!(conn.recv(&mut [0u8; 128]).await.is_err());
    }
    #[tokio::test]
    async fn tls_rejects_an_untrusted_or_non_tls_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (mut tcp, _) = listener.accept().await.unwrap();
            tcp.write_all(b"not TLS").await.unwrap();
        });
        assert!(TurnStream::dial("127.0.0.1", port, true).await.is_err());
    }

    #[tokio::test]
    async fn tls_rejects_a_self_signed_certificate() {
        let certificate = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let key =
            rustls::pki_types::PrivateKeyDer::Pkcs8(certificate.key_pair.serialize_der().into());
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate.cert.der().clone()], key)
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let _ = tokio_rustls::TlsAcceptor::from(Arc::new(config))
                .accept(tcp)
                .await;
        });
        assert!(TurnStream::dial("localhost", port, true).await.is_err());
    }
}
