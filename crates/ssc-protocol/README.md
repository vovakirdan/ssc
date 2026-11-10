# Shared protocol v4

This crate implements the desktop and browser protocol from
[`DESKTOP_PROTOCOL.md`](../../docs/DESKTOP_PROTOCOL.md): committed X25519 exchange,
HKDF-SHA256, directional ChaCha20-Poly1305 frames, SAS, READY verification,
bounded UTF-8 message chunks and GZIP/Base64 invitations. It has no Tauri,
WebRTC, ring, network or random-number-generator dependency. Native adapters
supply operating-system entropy. Browsers supply fresh `crypto.getRandomValues`
bytes; deterministic values in tests are public fixtures only.

Build the browser module from source with Rust stable and the wasm32 target:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.130 --locked
bash crates/ssc-protocol/build-wasm.sh
```

The optional first argument is the generated module output directory. Generated
`pkg/` and `target/` files are ignored. CI must run the same build; no compiled
WASM blob is committed.

```js
import init, { ProtocolSession } from './ssc_protocol.js';
await init();
const seed = crypto.getRandomValues(new Uint8Array(32));
const nonce = crypto.getRandomValues(new Uint8Array(32));
const session = new ProtocolSession(connectionId, isOfferCreator, seed, nonce);
seed.fill(0);
nonce.fill(0);
```

`start()` returns commitment frames. Feed every binary DataChannel frame to
`receive(frame)`, which returns `{outgoing, sas, peer_ready, message, event}`. Send
`outgoing` frames in order. `sas` and `message` are null until available.
The `sas()` accessor returns undefined until the exchange finishes. Call
`confirm()` only on an explicit user action after comparing the entire SAS;
it returns an encrypted READY frame. Both users' confirmation is required for
`send_text(text, messageId16)`, whose ID also comes from `crypto.getRandomValues`.
An invalid incoming protocol frame retires the owned keys. `close()` retires
state and rejects further use; call the generated `free()` when retiring the
wrapper. No event automatically confirms a SAS.

Serialize all calls, binary Blob conversions, returned sends and complete
messages on one ordered reliable DataChannel. Reject incoming frames larger
than 64 KiB before crossing the WASM boundary. Any transport send failure must
retire the session: counters are never rolled back or retried with old frames.
The browser adapter owns transport recovery/deadlines and invitation helpers;
the protocol engine owns no timers, persistence, browser history or logging.

`encode_invitation(sessionId, 'offer'|'answer', sdp, candidatesJson)` takes
browser candidates with `candidate`, `sdpMid`, `sdpMLineIndex` keys.
`decode_invitation(value)` returns `{session_id, sdp_type, sdp, ice_candidates}`;
the latter candidates use browser keys. These use the same compact v4 codec as
desktop. Exceptions contain a fixed safe error code, never external input.

Validation:

```sh
cargo test --manifest-path crates/ssc-protocol/Cargo.toml --locked
cargo clippy --manifest-path crates/ssc-protocol/Cargo.toml --locked --all-targets -- -D warnings
bash crates/ssc-protocol/build-wasm.sh
node crates/ssc-protocol/tests/wasm.mjs
```

The tests retain primitive/handshake/chunk regressions and fixed v4
SAS/AEAD/event/GZIP vectors checked against an independent Node/OpenSSL oracle.
V3 invitations, commitments and encrypted frames are rejected. Actual WASM execution is checked
in Node; browser and live desktop transport acceptance are separate checks.
Clearing owned Rust buffers is best effort: no guarantee is made about browser,
JavaScript or compiler-generated copies or forensic recovery.

`send_event(JSON.stringify(event))` returns one encrypted kind-4 frame after
both users confirm SAS. `receive` returns the typed object in `event`;
reassembled text also retains `message` for the old adapter API. Large text
(up to 256 KiB) uses `send_text(text,id16)`, preserving the sender's 32-hex ID.
One control frame, including AEAD header/tag, is at most 64 KiB.

Chat events use the strict `type` tag in snake_case. They include text,
attachment offer/accept/reject/chunk/progress/complete/cancel, receipt, reaction,
capabilities and Bye. See `events.rs` for the exact exported typed contract.
Unknown fields/types are invalid; IDs and SHA256 are canonical lowercase hex.
Files are at most 100 MiB, original basenames at most 1024 UTF-8 bytes (path and
control characters are rejected), MIME at most 128 ASCII bytes, and chunks at
most 16 KiB decoded standard padded Base64. Reactions use six fixed emojis or
explicit null. Audio capabilities contain up to three unique approved MIME
values. Attachment consent, transfer order, cumulative RAM limits and final
integrity checks belong to the shared adapter model. The core never accumulates
file bodies. AttachmentCancel leaves the chat open; Bye returns its final frame
and retires session keys.

`FileHasher` incrementally computes SHA256 with the existing Rust implementation,
without retaining whole file chunks. Its total input is capped at 100 MiB.
`new/update/finish/abort` are available in native Rust and WASM; finish consumes
the state, and finish/abort/size errors prohibit further updates. The browser
wrapper must also call the generated `free()`. Sender adapters may hash
256 KiB stream chunks before offering a file and stream it again for transport;
receiver chunks are 16 KiB. Hash tests cover 0, 16 KiB and 100 MiB without allocating
a whole-file buffer. The digest state may include one partial SHA block;
releasing it does not promise forensic erasure of compiler/JavaScript copies.
