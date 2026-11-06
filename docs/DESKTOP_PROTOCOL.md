# Desktop protocol v2

The desktop application has one current session. Each session owns its peer connection,
DataChannel, ephemeral key exchange, encryption context, partial incoming message and unread
inbox. Replacement retires the previous session. Callbacks retain only weak references;
closed sessions reject late callbacks. Rust owns the ten-second recovery period; an epoch
invalidates old recovery tasks. The UI consumes generation-tagged state and errors.

## Handshake and messages

Manual offer/answer exchange remains available. An invitation is `v2:` followed by Base64
of GZIP-compressed compact JSON (SDP and ICE candidates). Older invitations are rejected.
The encoded input limit is 512 KiB; decompressed JSON is limited to 256 KiB, with explicit
overflow detection. Answers must match the current connection ID. Invalid SDP and keys
produce safe errors rather than panics. ICE gathering waits for completion, up to ten seconds.

The ordered, reliable WebRTC DataChannel carries binary frames:

- Key exchange: `[version=2, kind=0] || X25519 public key (32 bytes)`.
- Encrypted: `[version=2, kind, sequence:u64 big-endian] || ciphertext || Poly1305 tag`.
- Encrypted kind 1 is READY, with empty plaintext. Kind 2 is a message chunk.

Each session generates fresh X25519 keys. HKDF-SHA256 derives 64 bytes using empty salt and
`ssc-chat-v2 || min(publicA,publicB) || max(publicA,publicB)` as info. The lexicographically
smaller public key sends with the first 32-byte key and receives with the second; the other
side reverses them. SAS is the first six bytes of SHA256(first key), lowercase hex.
Users must compare this code through a trusted means; the application does not independently
identify the person at the other end.

ChaCha20-Poly1305 authenticates the complete ten-byte header as AAD. Its nonce is four zero
bytes followed by the sequence number. Directions have separate keys and counters starting
at one. A send reserves its sequence before encryption and never rolls it back on send
failure. Receiving authenticates the explicit sequence before accepting it; authenticated
forward gaps are allowed, repeated/older numbers are rejected. Exhaustion closes the session.

READY is sent only after explicit local SAS confirmation. Both local and peer confirmation,
plus an available transport, are required to send chat messages. The UI never confirms SAS
in response to a transport event.

A UTF-8 message is at most 256 KiB. Its bytes are split into chunks of at most 16 KiB.
Encrypted chunk plaintext is JSON `{id,index,total,data}`, where `data` is Base64 of the
chunk bytes. At most 16 ordered chunks form one message. A new index-zero chunk replaces an
abandoned partial message; incomplete prefixes are never displayed. One async send lock
serializes complete messages and READY frames. No application-level automatic retries occur.
“Передано каналу” means accepted by the local transport, not acknowledged by the recipient.

A bounded unread inbox (128 complete messages / 8 MiB) bridges React mount/subscription
windows. `drain_messages({generation})` atomically removes messages for that generation only.
Closing the session clears the inbox, partial data and keys; the UI clears its history.

## App interfaces

- Session snapshot: `{generation,phase,route}`. Phases: idle, invitation, ice, key_exchange,
  verification, waiting_peer, connected, recovering, closed. Route: direct, relay or null.
- `get_session_state` and event `ssc-session` share this shape.
- Errors: `{code,stage,message,retryable}`. No external error strings are embedded.
  Command rejection and `ssc-session-error {generation,error}` expose safe errors.
- `ssc-chat-message` wakes inbox draining; messages contain string IDs and generation IDs.
- `get_diagnostics`, `set_diagnostics({enabled})`, `clear_diagnostics` work in release too.

User ICE settings remain configurable. Saved settings are applied before starting a new
connection. An invalid saved configuration blocks starting, with Settings still accessible.
No project TURN is mandatory. Server availability probes are not proof of a complete route.

## Validation

Use Node >=22.12, Rust stable, and the Tauri GTK/WebKit development dependencies on Linux.

```sh
npm ci
npm run lint
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked --lib
cargo build --manifest-path src-tauri/Cargo.toml --locked
```

The Rust suite includes actual localhost DTLS/SCTP peers and the production invitation,
key-exchange, readiness and message paths. React tests mock Tauri and do not establish a
real connection. Desktop checks run on pull requests; local results are not hosted CI proof.

For native WebView smoke testing, start Vite on port 1420 and two `tauri-driver` instances on
ports 4444/4445, with native WebKitWebDriver ports 4446/4447. Use separate Xvfb displays in a
headless environment. Then run `node scripts/native-smoke.mjs`. `SSC_BINARY` can override
the default debug binary path. The smoke test starts two application windows, exchanges
manual invitations through real IPC, compares SAS, confirms through UI controls, sends a
message through the UI, verifies report redaction and checks history cleanup. It uses no
external ICE servers and therefore does not validate TURN or arbitrary NAT traversal.

Before release, additionally verify forced TURN routing on a real server, reconnect after
a network interruption, and platform-specific Windows/macOS builds. These require separate
evidence from the localhost tests.
