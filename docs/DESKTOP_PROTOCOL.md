# Desktop protocol v4

The desktop application has one current session. Each session owns its peer connection,
DataChannel, ephemeral key exchange, encryption context, partial incoming message and unread
inbox. Replacement retires the previous session. Callbacks retain only weak references;
closed sessions reject late callbacks. Rust owns the ten-second recovery period; an epoch
invalidates old recovery tasks. The UI consumes generation-tagged state and errors.

## Handshake and messages

Manual offer/answer exchange remains available. An invitation is `v4:` followed by Base64
of GZIP-compressed compact JSON (SDP and ICE candidates). Older invitations are rejected.
The encoded input limit is 512 KiB; decompressed JSON is limited to 256 KiB, with explicit
overflow detection. Answers must match the current connection ID. Invalid SDP and keys
produce safe errors rather than panics. ICE gathering waits for completion, up to ten seconds.

The ordered, reliable WebRTC DataChannel carries binary frames:

- Commitment: `[version=4, kind=0, role] || SHA256 commitment (32 bytes)`.
- Reveal: `[version=4, kind=3, role] || X25519 public key (32) || random nonce (32)`.
- Encrypted: `[version=4, kind, sequence:u64 big-endian] || ciphertext || Poly1305 tag`.
- Role is 0 for the offer creator and 1 for the answerer. Encrypted kind 1 is READY;
  kind 2 is a text chunk; kind 4 is a bounded authenticated typed chat event.

Each session generates fresh X25519 keys and a fresh 32-byte nonce. Session context is
SHA256(`ssc-chat-v4/session-context\0 || u64_be(connectionID.length) || connectionID`).
The commitment is SHA256(`ssc-chat-v4/key-commitment\0 || [4,role] || context || public || nonce`).
Each endpoint sends its commitment before revealing, and reveals only after receiving the
other commitment. Duplicate, premature, reflected, wrong-role and mismatched reveals abort.
There is no v2/v3 fallback. A 20-second deadline begins when the peer transport connects or the
DataChannel opens; it stops once keys are established and does not limit human comparison.

Transcript is SHA256(`ssc-chat-v4/transcript\0 || [4] || context ||
[0] || creatorCommit || creatorPublic || creatorNonce ||
[1] || guestCommit || guestPublic || guestNonce`). HKDF-SHA256 uses that transcript as salt
and the X25519 shared secret as input. Info `ssc-chat-v4/directional-keys` derives 64 bytes:
creator sends with the first 32 bytes, guest with the second. Separate info `ssc-chat-v4/sas`
derives 32 bytes, whose first six become the lowercase 12-character hex SAS.
All-zero/noncontributory and reflected DH keys are rejected. Both users must compare the
complete code through a trusted means; this is not an independent identity service.
Commit/reveal prevents choosing a key after seeing the honest reveal to cheaply grind two
matching codes. It does not prevent online guesses, denial of service, or users skipping comparison.

ChaCha20-Poly1305 authenticates the complete ten-byte header as AAD. Its nonce is four zero
bytes followed by the sequence number. Directions have separate keys and counters starting
at one. A send reserves its sequence before encryption and never rolls it back on send
failure. Receiving authenticates the explicit sequence before accepting it; authenticated
forward gaps are allowed, repeated/older numbers are rejected. Exhaustion closes the session.

READY is sent only after explicit local SAS confirmation. Both local and peer confirmation,
plus an available transport, are required to send chat messages. The UI never confirms SAS
in response to a transport event. `ssc-sas` and `get_fingerprint` carry
`{generation,sas}`; confirmation sends that exact pair. Rust rejects stale pairs before
changing readiness. Rejection also carries generation and cannot close a replacement session.

A UTF-8 message is at most 256 KiB. Its bytes are split into chunks of at most 16 KiB.
Encrypted chunk plaintext is JSON `{id,index,total,data}`, where `data` is Base64 of the
chunk bytes. At most 16 ordered chunks form one message. A new index-zero chunk replaces an
abandoned partial message; incomplete prefixes are never displayed. One async send lock
serializes messages, controls and READY frames. File chunks yield between sends and use a256KiB receipt window; plaintext is not replayed into replacement sessions.
“Передано каналу” means accepted by the local transport. “Получено собеседником” requires an authenticated receipt from the peer application, not proof that a person read it.

A bounded unread inbox (128 complete messages / 8 MiB) bridges React mount/subscription
windows. `drain_messages({generation})` atomically removes messages for that generation only.
The v4 typed event inbox is separately bounded to512 events/8MiB; draining either API consumes its corresponding mirror. Closing clears both inboxes, hashers, partial data and keys; the UI clears its history.

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
CARGO_TARGET_DIR=/tmp/ssc-dtls-security-target cargo test --manifest-path src-tauri/vendor/webrtc-dtls/Cargo.toml --locked --lib
cargo audit --file src-tauri/Cargo.lock
npm audit
cargo build --manifest-path src-tauri/Cargo.toml --locked
```

The Rust suite includes actual localhost DTLS/SCTP peers and the production invitation,
key-exchange, readiness and message paths. React tests mock Tauri and do not establish a
real connection. Desktop checks run on pull requests; local results are not hosted CI proof.

For native WebView smoke testing, build a release executable and start two `tauri-driver` instances on
ports 4444/4445, with native WebKitWebDriver ports 4446/4447. Use separate Xvfb displays in a
headless environment. Then run `node scripts/native-smoke.mjs`. `SSC_BINARY` can override
the default release binary path. The smoke test starts two application windows, exchanges
manual invitations through real IPC, compares SAS, confirms through UI controls, sends a
message through the UI, verifies report redaction and checks history cleanup. It uses no
external ICE servers and therefore does not validate TURN or arbitrary NAT traversal.

Before release, additionally verify forced TURN routing on a real server, reconnect after
a network interruption, and platform-specific Windows/macOS builds. These require separate
evidence from the localhost tests.

Short-link pairing and its local native check are described in [connection screens](CONNECTION_SCREENS.md#короткая-ссылка). Both desktop participants must use v4; old codes and older clients are rejected.

## v4 encrypted attachments and controls

Typed JSON controls (strict unknown/duplicate fields) carry text IDs, attachment offers/consent/chunks/progress/completion/cancellation, receipts, fixed reactions, capabilities and authenticated exit/expiry. Names<=1024UTF8 bytes are basenames; MIME<=128ASCII, file<=100MiB, canonical IDs32hex/SHA64hex/Base64. Controls fit64KiB including AEAD framing, file chunks decode to<=16KiB. Both READY flags are mandatory. Metadata, original file bytes and voice are encrypted in the same verified DataChannel; the HTTP helper stores no file payload.

The shared volatile application model reserves<=200MiB logical attachment storage, one transfer each direction/four pending offers, and bounded512/8MiB application events. It incrementally hashes with shared Rust SHA256 and verifies completion before exposing the original. Supported media can preview; unknown files/HTML/SVG never execute inline. Explicit Save preserves original bytes in a user-owned downloaded copy. No persistent chat cache is written by the app; browser/OS backing storage is outside physical-erasure guarantees.

Authenticated Bye retires keys and clears application history immediately. File/hash callbacks after cancel, retirement or generation replacement cannot publish data or close a replacement. Abrupt browser/process termination is best effort; lack of Bye must not be labelled deliberate exit. Native normal close gives the encrypted final frame a bounded250ms transport window before room deletion.

Interface exit controls are in the header, confirmed with default focus on Stay. The remaining-time header reads the native monotonic deadline or the already-pinned browser deadline; animation never renews it. Auto/Dark/Light persist only appearance preferences for the new chat UI. Existing native access identity/settings remain separate.
