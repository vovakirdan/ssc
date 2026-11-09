# Secret Chat TURN stream patch

This is the crates.io `webrtc-ice` **0.13.0** package with an additional TURN
TCP/TLS transport. This is a local patch, not an upstream release.

- Original crate SHA-256: `eb51bde0d790f109a15bfe4d04f1b56fb51d567da231643cb3f21bb74d678997`.
- Upstream: https://github.com/webrtc-rs/webrtc, commit
  `0ed74adf74f499f393a3481e17bb603546f465ca`, subdirectory `ice`.
- Original licenses and `Cargo.toml.orig` remain as provenance. The active manifest
  is `Cargo.toml`; the standalone lockfile is regenerated. No build artifacts belong here.

## Reason and production changes

The original relay gatherer accepts only TURN over UDP. Its TCP and TLS branches
are TODO comments; accepting a URL in application settings did **not** establish
support. Native live tests reproduced missing relay candidates for both transports.

- `src/turn_stream.rs` implements the existing `util::Conn` packet interface over
  a connected TCP or TLS stream. STUN frames use their 20-byte header and body
  length; ChannelData uses its 4-byte header and 4-byte stream padding.
- Incoming packet size is bounded by the existing TURN client's 65535-byte buffer.
  Invalid type/length/cookie, truncation and oversized frames return errors. A
  persistent decoder retains partial bytes across cancelled reads. Writes are
  serialized; cancellation during a partial write poisons the connection rather
  than allowing a later message to append to an incomplete frame.
- A closed connection wakes blocked reads/writes. Connection and TLS negotiation
  have a five-second deadline. TLS uses rustls's ring provider, WebPKI roots and
  normal certificate/host verification. No custom permissive verifier exists.
- `src/agent/agent_gather.rs` selects this connection for TCP URLs, requests TLS
  for `turns`, and pins the selected peer address for TURN transactions. Relayed
  peer traffic remains UDP, as with the previous implementation. Existing UDP
  transport is unchanged.
- `src/lib.rs` adds the private module. Dependencies add `tokio-rustls`0.26
  (ring/tls12, no default AWS provider) and `webpki-roots`1.0. Dev-only `rcgen`0.13.2
  produces an untrusted certificate for the negative TLS test.

The existing gatherer's IPv4 relay policy and virtual-network limitations remain.
The new stream connector uses the operating-system TCP stack. This patch is not
ICE-TCP or TCP relay allocations; it supports TCP/TLS between client and TURN server.

## Verification

```sh
CARGO_TARGET_DIR=/tmp/ssc-ice-transport-target cargo test --manifest-path src-tauri/vendor/webrtc-ice/Cargo.toml --locked --lib
```

The suite contains97 upstream tests and8 additional framing/cancellation/TLS
checks. Both application and standalone lockfiles must stay vulnerability-audited.
A successful unit test is not live transport acceptance: run the native managed
smoke against the deployed server separately for UDP, TCP and TLS, asserting relay
routes and two-sided SAS. The initial pre-patch TCP/TLS failures are not passes.

The upstream ping-pong example only uses HTTP/1. Its Hyper dev features were narrowed
from full to client/server/http1/tcp to remove the unused vulnerable h2 0.3 graph.
The standalone lockfile was refreshed, including patched bytes.

## Opt-in TURN control summaries

`turn_diagnostics::TraceConn` delegates the existing TURN control connection
unchanged, including payload bytes, addresses and underlying connection type.
With diagnostics disabled (the default), packet inspection is skipped. The
application's existing debug switch enables the bounded in-memory summary;
disabling or clearing diagnostics removes it. No Rust/WebRTC raw logger is enabled.

The summary retains only fixed transport/direction/method/class labels, selected
numeric STUN error codes, packet counts and relative first/last times. It never
reads transaction IDs into diagnostic state and never retains addresses,
usernames, realms, nonces, credentials, SDP or payload bytes. At most 128 groups
are stored. The summary is process-wide since enable/clear; callers should clear
it before a diagnostic attempt if they need a fresh interval.

Tests check default-off behavior, metadata redaction and actual loopback UDP
payload/address forwarding. The standalone ICE manifest patches the same local
TURN source as the desktop application; its transaction timeout patch is
explained in `../turn/SSC_PATCH.md`.

Opaque datagrams are counted as fixed `dtls_record`, `other_datagram` or
`malformed_stun` labels; no payload or record sequence is retained. Incoming
rows additionally expose `from_server: true|false|null`. The wrapper learns the
configured TURN control endpoint from the existing outbound send destination
and compares the incoming source internally. No peer address enters the
summary. Outgoing rows use null; null also means the expected endpoint was not
yet known. This comparison does not filter, connect or redirect the socket.
